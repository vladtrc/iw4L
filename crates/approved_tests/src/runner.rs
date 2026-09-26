use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub struct Launch<'a> {
    pub bin: &'a Path,
    pub cwd: &'a Path,
    pub args: Vec<String>,
    pub env: Vec<(&'static str, String)>,
    pub phase_timeout: Duration,
    pub quit_timeout: Duration,
}

#[derive(Clone)]
pub struct Event {
    pub at_ms: u128,
    pub name: String,
    pub fields: BTreeMap<String, String>,
}

impl Event {
    pub fn ns(&self) -> Option<u128> {
        self.fields.get("ns")?.parse().ok()
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }
    pub fn num(&self, key: &str) -> Option<f64> {
        self.get(key)?.parse().ok()
    }
}

pub enum End {
    Exited { code: Option<i32>, at_ms: u128 },
    Timeout { waiting_for: String, at_ms: u128 },
    SpawnFailed(String),
}

pub struct Outcome {
    pub end: End,
    pub marks: Vec<Event>,
    pub lifecycle: Vec<Event>,
    pub script: Vec<Event>,
    pub runtime: Option<Event>,
}

pub fn run(launch: Launch<'_>) -> Outcome {
    let mut outcome = Outcome {
        end: End::SpawnFailed(String::new()),
        marks: Vec::new(),
        lifecycle: Vec::new(),
        script: Vec::new(),
        runtime: None,
    };
    let stdout_file = std::fs::File::create(launch.cwd.join("child_stdout.txt"));
    let stderr_file = std::fs::File::create(launch.cwd.join("child_stderr.txt"));
    let (mut stdout_file, stderr_file) = match (stdout_file, stderr_file) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            outcome.end = End::SpawnFailed(format!("child output files: {e}"));
            return outcome;
        }
    };
    let mut command = Command::new(launch.bin);
    command
        .args(&launch.args)
        .current_dir(launch.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr_file));
    for (key, value) in &launch.env {
        command.env(key, value);
    }
    let started = Instant::now();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            outcome.end = End::SpawnFailed(format!("spawn {}: {e}", launch.bin.display()));
            return outcome;
        }
    };
    let (tx, rx) = mpsc::channel::<(u128, String)>();
    let stdout = child.stdout.take().expect("stdout is piped");
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send((started.elapsed().as_millis(), line)).is_err() {
                break;
            }
        }
    });

    let mut last_progress = Instant::now();
    let mut quit_at: Option<Instant> = None;
    let mut waiting_for = String::from("first mark");
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok((at_ms, line)) => {
                let _ = writeln!(stdout_file, "{at_ms:>9} {line}");
                if let Some(event) = parse(&line, "benchmark-mark:", at_ms) {
                    last_progress = Instant::now();
                    if let Some(label) = event.get("label") {
                        waiting_for = format!("the mark after {label}");
                    }
                    println!("[{:>7.1}s] {}", at_ms as f64 / 1000.0, line);
                    outcome.marks.push(event);
                } else if let Some(event) = parse(&line, "lifecycle:", at_ms) {
                    last_progress = Instant::now();
                    if event.name == "quit_requested" && quit_at.is_none() {
                        quit_at = Some(Instant::now());
                        waiting_for = "process exit after quit".into();
                    }
                    println!("[{:>7.1}s] {}", at_ms as f64 / 1000.0, line);
                    outcome.lifecycle.push(event);
                } else if let Some(event) = parse(&line, "gsc:", at_ms) {
                    last_progress = Instant::now();
                    println!("[{:>7.1}s] {}", at_ms as f64 / 1000.0, line);
                    outcome.script.push(event);
                } else if let Some(event) = parse(&line, "runtime:", at_ms) {
                    println!("[{:>7.1}s] {}", at_ms as f64 / 1000.0, line);
                    outcome.runtime.get_or_insert(event);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        if let Some(end) = exited(&mut child, started) {
            while let Ok((at_ms, line)) = rx.try_recv() {
                let _ = writeln!(stdout_file, "{at_ms:>9} {line}");
                if let Some(event) = parse(&line, "benchmark-mark:", at_ms) {
                    outcome.marks.push(event);
                } else if let Some(event) = parse(&line, "lifecycle:", at_ms) {
                    outcome.lifecycle.push(event);
                } else if let Some(event) = parse(&line, "gsc:", at_ms) {
                    outcome.script.push(event);
                }
            }
            outcome.end = end;
            return outcome;
        }
        let over = match quit_at {
            Some(quit) => quit.elapsed() > launch.quit_timeout,
            None => last_progress.elapsed() > launch.phase_timeout,
        };
        if over {
            let _ = child.kill();
            let _ = child.wait();
            outcome.end = End::Timeout {
                waiting_for,
                at_ms: started.elapsed().as_millis(),
            };
            return outcome;
        }
    }
}

fn exited(child: &mut Child, started: Instant) -> Option<End> {
    match child.try_wait() {
        Ok(Some(status)) => Some(End::Exited {
            code: status.code(),
            at_ms: started.elapsed().as_millis(),
        }),
        Ok(None) => None,
        Err(_) => Some(End::Exited {
            code: None,
            at_ms: started.elapsed().as_millis(),
        }),
    }
}

pub fn parse(line: &str, prefix: &str, at_ms: u128) -> Option<Event> {
    let rest = line.strip_prefix(prefix)?.trim_start();
    let mut fields = BTreeMap::new();
    let mut name = String::new();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in rest.chars() {
        match c {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    for (i, token) in tokens.into_iter().enumerate() {
        match token.split_once('=') {
            Some((k, v)) => {
                fields.insert(k.to_owned(), v.to_owned());
            }
            None if i == 0 => name = token,
            None => {}
        }
    }
    Some(Event {
        at_ms,
        name,
        fields,
    })
}
