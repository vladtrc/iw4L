use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::perfetto_query::{Table, ensure_trace_health, resolve_processor, run_sql};

const SCENARIO: &str = "heavy_gameplay_lifecycle";
const SEED: u64 = 10_465_170_034_885_992_326;
const SPANS: &[&str] = &["wall", "render_thread", "Present", "FixedUpdate"];
const FRAME_LINE_PCT: f64 = 5.0;
const RSS_LINE_MIB: f64 = 256.0;
const BUILD_IDENTITY_VERSION: u64 = 1;

struct Config {
    hashes: Vec<String>,
    rounds: usize,
    warmup: usize,
    replay: Option<PathBuf>,
}

struct Arm {
    sha: String,
    short: String,
    subject: String,
    bin: PathBuf,
}

#[derive(Default, Clone)]
struct Stats {
    n: usize,
    mean: f64,
    p50: f64,
    p90: f64,
    p99: f64,
}

#[derive(Default, Clone)]
struct RunMetrics {
    passed: bool,
    approved_run: Option<PathBuf>,
    spans: BTreeMap<String, Stats>,
    hitches: usize,
    b_load_s: Option<f64>,
    rss_first_spawn_mib: Option<f64>,
    rss_last_scene_mib: Option<f64>,
    peak_rss_swap_mib: Option<f64>,
    kernel_hwm_mib: Option<f64>,
    error: Option<String>,
}

pub fn run(root: &Path, args: Vec<String>) -> bool {
    crate::hr("perf-compare — release perf series over commits");
    match parse(args).and_then(|config| run_inner(root, &config)) {
        Ok(()) => true,
        Err(error) => {
            println!("{error}");
            false
        }
    }
}

fn usage() -> String {
    "usage: cargo xtask perf-compare <hash> [<hash>...] [--rounds N] [--warmup N] [--replay run.json]\n\
     the first hash is the baseline; every hash is built once into iw4l-artifacts/perf-compare/bin/"
        .to_owned()
}

fn parse(args: Vec<String>) -> Result<Config, String> {
    let mut config = Config {
        hashes: Vec::new(),
        rounds: 3,
        warmup: 1,
        replay: None,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("{arg} needs a value\n{}", usage()))
        };
        match arg.as_str() {
            "--rounds" => {
                config.rounds = value()?
                    .parse()
                    .map_err(|_| format!("--rounds: not a count\n{}", usage()))?;
            }
            "--warmup" => {
                config.warmup = value()?
                    .parse()
                    .map_err(|_| format!("--warmup: not a count\n{}", usage()))?;
            }
            "--replay" => config.replay = Some(PathBuf::from(value()?)),
            flag if flag.starts_with("--") => {
                return Err(format!("unknown flag {flag}\n{}", usage()));
            }
            _ => config.hashes.push(arg),
        }
    }
    if config.hashes.is_empty() || config.rounds == 0 {
        return Err(usage());
    }
    Ok(config)
}

fn run_inner(root: &Path, config: &Config) -> Result<(), String> {
    let base = root.join("iw4l-artifacts/perf-compare");
    let mut arms = Vec::new();
    for hash in &config.hashes {
        let sha = git(
            root,
            &["rev-parse", "--verify", &format!("{hash}^{{commit}}")],
        )?;
        if arms.iter().any(|arm: &Arm| arm.sha == sha) {
            return Err(format!("{hash} names the same commit twice"));
        }
        arms.push(Arm {
            short: sha[..8].to_owned(),
            subject: git(root, &["log", "-1", "--format=%s", &sha])?,
            bin: base.join("bin").join(&sha).join("iw4l"),
            sha,
        });
    }
    for arm in &arms {
        build(root, arm)?;
    }
    let status = Command::new("cargo")
        .current_dir(root)
        .args(["build", "--quiet", "-p", "approved_tests"])
        .status()
        .map_err(|error| format!("build approved_tests: {error}"))?;
    if !status.success() {
        return Err("approved_tests runner does not build on this tree".into());
    }
    let runner = root.join("target/debug/approved_tests");
    let processor = resolve_processor(root)?;

    let series = base.join(utc_stamp());
    std::fs::create_dir_all(series.join("runs")).map_err(|e| format!("create series: {e}"))?;
    println!("series: {}", series.display());

    // Without --replay the first warmup run pins seed, map B and spawns for the
    // whole series, so at least one warmup always runs then.
    let warmup = config.warmup.max(usize::from(config.replay.is_none()));
    let mut replay = config.replay.clone();
    for index in 0..warmup {
        let dir = series
            .join("runs")
            .join(format!("w{index}-{}", arms[0].short));
        println!("warmup {}/{warmup} {}", index + 1, arms[0].short);
        let metrics = run_once(root, &runner, &arms[0], replay.as_deref(), &dir)?;
        if replay.is_none() {
            let run = metrics
                .approved_run
                .filter(|_| metrics.passed)
                .ok_or("warmup run did not pass, so it cannot pin the replay")?;
            replay = Some(run.join("run.json"));
        }
    }
    let replay = replay.ok_or("no replay")?;
    println!("replay: {}", replay.display());

    let mut results: Vec<(usize, usize, RunMetrics)> = Vec::new();
    let mut ordinal = 0;
    for round in 0..config.rounds {
        let order: Vec<usize> = if round % 2 == 0 {
            (0..arms.len()).collect()
        } else {
            (0..arms.len()).rev().collect()
        };
        for arm_index in order {
            ordinal += 1;
            let arm = &arms[arm_index];
            let dir = series
                .join("runs")
                .join(format!("{ordinal:02}-{}", arm.short));
            println!(
                "run {ordinal}/{} round {} {}",
                config.rounds * arms.len(),
                round + 1,
                arm.short
            );
            let mut metrics = run_once(root, &runner, arm, Some(&replay), &dir)?;
            if let Some(run) = metrics.approved_run.clone()
                && let Err(error) = analyze(&processor, &run, &arm.sha, &mut metrics)
            {
                metrics.passed = false;
                metrics.error = Some(error);
            }
            write_json(&dir.join("metrics.json"), &metrics_json(&metrics))?;
            print_run(&metrics);
            results.push((ordinal, arm_index, metrics));
        }
    }

    report(root, &base, &series, config, &arms, &replay, &results)
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

// One fixed tree path and one target dir: a second tree path on the same target
// dir would reuse fingerprints of the first tree's sources.
fn build(root: &Path, arm: &Arm) -> Result<(), String> {
    let dir = arm.bin.parent().expect("bin has a parent");
    if arm.bin.is_file() {
        let metadata = std::fs::read(dir.join("build.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if let Some(metadata) = metadata
            && metadata["sha"].as_str() == Some(arm.sha.as_str())
            && metadata["identity_version"].as_u64() == Some(BUILD_IDENTITY_VERSION)
            && metadata["binary_sha256"].as_str()
                == Some(crate::release::file_sha256(&arm.bin)?.as_str())
        {
            println!("bin {}: cached", arm.short);
            return Ok(());
        }
        println!("bin {}: rebuilding stale build identity", arm.short);
    }
    let tree = root.join("target/perf-compare/tree");
    let target = root.join("target/perf-compare/target");
    if tree.join(".git").exists() {
        git(
            &tree,
            &["checkout", "--quiet", "--detach", "--force", &arm.sha],
        )?;
    } else {
        git(root, &["worktree", "prune"])?;
        let tree_arg = tree.display().to_string();
        git(root, &["worktree", "add", "--detach", &tree_arg, &arm.sha])?;
    }
    // Older comparison commits do not watch the worktree's real Git HEAD.
    // Refresh their build script without changing any source bytes or dirty state.
    let build_script = tree.join("apps/bootstrap/build.rs");
    if build_script.is_file() {
        std::fs::File::open(&build_script)
            .and_then(|file| file.set_modified(std::time::SystemTime::now()))
            .map_err(|error| format!("refresh build identity: {error}"))?;
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let log_path = dir.join("build.log");
    println!("bin {}: building ({})", arm.short, log_path.display());
    let started = Instant::now();
    let mut built = false;
    for offline in [true, false] {
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("open {}: {e}", log_path.display()))?;
        let mut command = Command::new("cargo");
        command
            .current_dir(&tree)
            .env_remove("CARGO_TARGET_DIR")
            .args(["build", "--profile", "play", "-p", "launcher", "--locked"])
            .arg("--target-dir")
            .arg(&target)
            .stdout(log.try_clone().map_err(|e| format!("log: {e}"))?)
            .stderr(log);
        if offline {
            command.arg("--offline");
        }
        let status = command
            .status()
            .map_err(|error| format!("cargo build {}: {error}", arm.short))?;
        if status.success() {
            built = true;
            break;
        }
    }
    if !built {
        return Err(format!(
            "{} does not build; see {}",
            arm.short,
            log_path.display()
        ));
    }
    let staged = dir.join("iw4l.part");
    std::fs::copy(target.join("play/iw4l"), &staged)
        .map_err(|e| format!("copy {} binary: {e}", arm.short))?;
    std::fs::rename(&staged, &arm.bin).map_err(|e| format!("install {} binary: {e}", arm.short))?;
    write_json(
        &dir.join("build.json"),
        &json!({
            "sha": arm.sha,
            "subject": arm.subject,
            "profile": "play",
            "elapsed_s": started.elapsed().as_secs_f64(),
            "identity_version": BUILD_IDENTITY_VERSION,
            "binary_sha256": crate::release::file_sha256(&arm.bin)?,
        }),
    )?;
    println!(
        "bin {}: built in {:.0}s",
        arm.short,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn run_once(
    root: &Path,
    runner: &Path,
    arm: &Arm,
    replay: Option<&Path>,
    dir: &Path,
) -> Result<RunMetrics, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let mut command = Command::new(runner);
    command
        .current_dir(root)
        .arg(SCENARIO)
        .arg("--bin")
        .arg(&arm.bin)
        .env("IW4L_BENCH", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match replay {
        Some(replay) => command.arg("--replay").arg(replay),
        None => command.arg("--seed").arg(SEED.to_string()),
    };
    let mut child = command
        .spawn()
        .map_err(|error| format!("run {}: {error}", runner.display()))?;

    let pid = Arc::new(AtomicU32::new(0));
    let state = Arc::new(Mutex::new(Marks::default()));
    let log = Arc::new(Mutex::new(
        std::fs::File::create(dir.join("runner.log"))
            .map_err(|e| format!("create runner.log: {e}"))?,
    ));
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|stream| {
        let (pid, state, log) = (Arc::clone(&pid), Arc::clone(&state), Arc::clone(&log));
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = writeln!(log.lock().expect("log lock"), "{line}");
                state.lock().expect("marks lock").read(&line, &pid);
            }
        })
    })
    .collect();

    let mut samples = String::from("elapsed_ms\trss_kib\tswap_kib\thwm_kib\n");
    let (mut peak, mut hwm) = (0_u64, 0_u64);
    let started = Instant::now();
    let exit = loop {
        if let Some(status) = child.try_wait().map_err(|e| format!("wait runner: {e}"))? {
            break status;
        }
        let game = pid.load(Ordering::Acquire);
        if game != 0
            && let Some((rss, swap, high)) = proc_memory(game)
        {
            peak = peak.max(rss + swap);
            hwm = hwm.max(high);
            samples.push_str(&format!(
                "{}\t{rss}\t{swap}\t{high}\n",
                started.elapsed().as_millis()
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    for reader in readers {
        let _ = reader.join();
    }
    std::fs::write(dir.join("memory.tsv"), samples).map_err(|e| format!("memory.tsv: {e}"))?;

    let marks = state.lock().expect("marks lock").clone();
    let kib_to_mib = |kib: u64| (kib > 0).then(|| kib as f64 / 1024.0);
    Ok(RunMetrics {
        passed: exit.success(),
        approved_run: marks.run_dir,
        rss_first_spawn_mib: marks.rss.get("a.scene_1.spawned").copied(),
        rss_last_scene_mib: marks.rss.get("b.scene_3.end").copied(),
        peak_rss_swap_mib: kib_to_mib(peak),
        kernel_hwm_mib: kib_to_mib(hwm),
        ..RunMetrics::default()
    })
}

#[derive(Default, Clone)]
struct Marks {
    run_dir: Option<PathBuf>,
    rss: BTreeMap<String, f64>,
}

impl Marks {
    fn read(&mut self, line: &str, pid: &AtomicU32) {
        if let Some(path) = line.strip_prefix("run: ") {
            self.run_dir = Some(PathBuf::from(path.trim()));
        }
        let Some(mark) = line.split("benchmark-mark: ").nth(1) else {
            return;
        };
        let field = |key: &str| {
            mark.split_whitespace()
                .find_map(|pair| pair.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
        };
        if let Some(game) = field("pid").and_then(|v| v.parse().ok()) {
            pid.store(game, Ordering::Release);
        }
        if let (Some(label), Some(rss)) = (
            field("label"),
            field("rss_mib").and_then(|v| v.parse().ok()),
        ) {
            self.rss.insert(label.to_owned(), rss);
        }
    }
}

fn proc_memory(pid: u32) -> Option<(u64, u64, u64)> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let value = |key: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|kib| kib.parse::<u64>().ok())
    };
    Some((value("VmRSS:")?, value("VmSwap:")?, value("VmHWM:")?))
}

fn analyze(
    processor: &Path,
    run: &Path,
    expected_sha: &str,
    metrics: &mut RunMetrics,
) -> Result<(), String> {
    let runs = run.join("iw4l-artifacts/runs");
    let traces: Vec<PathBuf> = std::fs::read_dir(&runs)
        .map_err(|e| format!("read {}: {e}", runs.display()))?
        .flatten()
        .map(|entry| entry.path().join("trace.pftrace"))
        .filter(|path| path.is_file())
        .collect();
    let [trace] = traces.as_slice() else {
        return Err(format!(
            "{} holds {} traces, expected one",
            runs.display(),
            traces.len()
        ));
    };
    let manifest_path = trace.with_file_name("manifest.json");
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(&manifest_path).map_err(|error| format!("read build manifest: {error}"))?,
    )
    .map_err(|error| format!("parse build manifest: {error}"))?;
    if manifest["build"]["git_sha"].as_str() != Some(expected_sha)
        || manifest["build"]["git_dirty"].as_bool() != Some(false)
    {
        return Err(format!(
            "build manifest does not identify clean source {expected_sha}"
        ));
    }
    let root = crate::repo_root();
    ensure_trace_health(&root, processor, trace)?;
    let marks = run_sql(
        processor,
        trace,
        "SELECT s.ts AS ts, a.display_value AS label FROM slice s JOIN args a USING(arg_set_id) \
         WHERE s.name = 'benchmark_mark' AND a.key = 'debug.label' ORDER BY s.ts",
    )?;
    let marks: Vec<(i64, String)> = rows(&marks, &["ts", "label"])
        .filter_map(|cells| Some((cells[0].parse().ok()?, cells[1].to_owned())))
        .collect();
    let mark_ts = |label: &str| marks.iter().find(|(_, l)| l == label).map(|(ts, _)| *ts);
    if let (Some(begin), Some(end)) = (mark_ts("b.load.begin"), mark_ts("b.load.end")) {
        metrics.b_load_s = Some((end - begin) as f64 / 1e9);
    }
    let names = SPANS
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(",");
    let spans = run_sql(
        processor,
        trace,
        &format!("SELECT name, ts, dur FROM slice WHERE name IN ({names}) AND dur > 0 ORDER BY ts"),
    )?;
    let mut durations: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for cells in rows(&spans, &["name", "ts", "dur"]) {
        let (Ok(ts), Ok(dur)) = (cells[1].parse::<i64>(), cells[2].parse::<f64>()) else {
            continue;
        };
        let phase = marks.partition_point(|(mark, _)| *mark <= ts);
        if phase > 0 && marks[phase - 1].1.ends_with(".spawned") {
            durations
                .entry(cells[0].to_owned())
                .or_default()
                .push(dur / 1e6);
        }
    }
    metrics.hitches = durations
        .get("wall")
        .map_or(0, |wall| wall.iter().filter(|ms| **ms >= 33.3).count());
    metrics.spans = durations
        .into_iter()
        .map(|(name, values)| (name, stats(values)))
        .collect();
    if !metrics.spans.contains_key("wall") {
        return Err("no wall frames inside *.spawned windows".into());
    }
    Ok(())
}

fn rows<'a>(table: &'a Table, names: &[&str]) -> impl Iterator<Item = Vec<&'a str>> {
    let columns: Vec<Option<usize>> = names
        .iter()
        .map(|name| table.headers.iter().position(|header| header == name))
        .collect();
    table.rows.iter().filter_map(move |row| {
        columns
            .iter()
            .map(|column| column.and_then(|index| row.get(index)).map(String::as_str))
            .collect()
    })
}

fn stats(mut values: Vec<f64>) -> Stats {
    values.sort_by(f64::total_cmp);
    let at = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
    Stats {
        n: values.len(),
        mean: values.iter().sum::<f64>() / values.len() as f64,
        p50: at(0.5),
        p90: at(0.9),
        p99: at(0.99),
    }
}

fn metrics_json(metrics: &RunMetrics) -> Value {
    json!({
        "passed": metrics.passed,
        "approved_run": metrics.approved_run,
        "error": metrics.error,
        "gameplay_ms": metrics.spans.iter().map(|(name, s)| (name.clone(), json!({
            "n": s.n, "mean": s.mean, "p50": s.p50, "p90": s.p90, "p99": s.p99,
        }))).collect::<serde_json::Map<_, _>>(),
        "hitches_ge_33ms": metrics.hitches,
        "b_load_s": metrics.b_load_s,
        "rss_mib": {
            "a.scene_1.spawned": metrics.rss_first_spawn_mib,
            "b.scene_3.end": metrics.rss_last_scene_mib,
            "sampled_peak_rss_plus_swap": metrics.peak_rss_swap_mib,
            "kernel_hwm": metrics.kernel_hwm_mib,
        },
    })
}

fn print_run(metrics: &RunMetrics) {
    println!(
        "  {} wall mean | p50 | p90 | p99 | n = {} b.load={} peak={}{}",
        if metrics.passed { "passed" } else { "FAILED" },
        wall_cells(metrics),
        opt(metrics.b_load_s, 2),
        opt(metrics.peak_rss_swap_mib, 0),
        metrics
            .error
            .as_ref()
            .map_or(String::new(), |error| format!(" error: {error}")),
    );
}

fn wall_cells(metrics: &RunMetrics) -> String {
    match metrics.spans.get("wall") {
        Some(wall) => format!(
            "{:.3} | {:.3} | {:.3} | {:.3} | {}",
            wall.mean, wall.p50, wall.p90, wall.p99, wall.n
        ),
        None => "— | — | — | — | 0".to_owned(),
    }
}

fn opt(value: Option<f64>, digits: usize) -> String {
    value.map_or_else(|| "—".to_owned(), |v| format!("{v:.digits$}"))
}

/// What one hash measured across its runs: per metric, the mean of run values
/// and the range they span.
struct Summary {
    metrics: Vec<(&'static str, Vec<f64>)>,
    runs: usize,
    passed: usize,
}

fn summarize(results: &[&RunMetrics]) -> Summary {
    let span = |name: &str, pick: fn(&Stats) -> f64| -> Vec<f64> {
        results
            .iter()
            .filter_map(|m| m.spans.get(name).map(pick))
            .collect()
    };
    let field = |pick: fn(&RunMetrics) -> Option<f64>| -> Vec<f64> {
        results.iter().filter_map(|m| pick(m)).collect()
    };
    Summary {
        metrics: vec![
            ("wall mean ms", span("wall", |s| s.mean)),
            ("wall p50 ms", span("wall", |s| s.p50)),
            ("wall p90 ms", span("wall", |s| s.p90)),
            ("wall p99 ms", span("wall", |s| s.p99)),
            ("render_thread mean ms", span("render_thread", |s| s.mean)),
            ("Present mean ms", span("Present", |s| s.mean)),
            ("FixedUpdate mean ms", span("FixedUpdate", |s| s.mean)),
            ("hitches ≥33 ms", field(|m| Some(m.hitches as f64))),
            ("b.load s", field(|m| m.b_load_s)),
            ("RSS first spawn MiB", field(|m| m.rss_first_spawn_mib)),
            ("RSS end of map B MiB", field(|m| m.rss_last_scene_mib)),
            ("peak RSS+swap MiB", field(|m| m.peak_rss_swap_mib)),
        ],
        runs: results.len(),
        passed: results.iter().filter(|m| m.passed).count(),
    }
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn range(values: &[f64]) -> Option<(f64, f64)> {
    let low = values.iter().copied().reduce(f64::min)?;
    let high = values.iter().copied().reduce(f64::max)?;
    Some((low, high))
}

fn report(
    root: &Path,
    base: &Path,
    series: &Path,
    config: &Config,
    arms: &[Arm],
    replay: &Path,
    results: &[(usize, usize, RunMetrics)],
) -> Result<(), String> {
    let summaries: Vec<Summary> = (0..arms.len())
        .map(|index| {
            summarize(
                &results
                    .iter()
                    .filter(|(_, arm, _)| *arm == index)
                    .map(|(_, _, metrics)| metrics)
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let name = series
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or("series dir has no name")?;
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map_err(|e| format!("hostname: {e}"))?;
    let host = host.trim();
    let head = format!(
        "Scenario {SCENARIO}, replay `{}`, {} alternating round(s), {} warmup run(s), host {}. \
         Gameplay = frames inside the six `*.spawned` windows; menu and loading frames are excluded. \
         Only arms inside this series compare: rerun every hash in one series instead of reusing an older one.\n",
        replay.display(),
        config.rounds,
        config.warmup,
        host,
    );

    let hashes = series.join("hashes");
    std::fs::create_dir_all(&hashes).map_err(|e| format!("create hashes: {e}"))?;
    for (index, arm) in arms.iter().enumerate() {
        let mut text = format!(
            "# {} {}\n\n{head}\nBinary `{}`.\n\n| run | result | wall mean | p50 | p90 | p99 | n | render_thread mean | Present mean | hitches | b.load s | RSS spawn → end | peak RSS+swap |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
            arm.short,
            arm.subject,
            arm.bin.display()
        );
        for (ordinal, _, m) in results.iter().filter(|(_, a, _)| *a == index) {
            let mean_of = |name: &str| m.spans.get(name).map(|s| s.mean);
            text.push_str(&format!(
                "| {ordinal:02} | {} | {} | {} | {} | {} | {} | {} → {} | {} |\n",
                if m.passed { "passed" } else { "FAILED" },
                wall_cells(m),
                opt(mean_of("render_thread"), 3),
                opt(mean_of("Present"), 3),
                m.hitches,
                opt(m.b_load_s, 2),
                opt(m.rss_first_spawn_mib, 0),
                opt(m.rss_last_scene_mib, 0),
                opt(m.peak_rss_swap_mib, 0),
            ));
            if let Some(error) = &m.error {
                text.push_str(&format!("\nrun {ordinal:02}: {error}\n"));
            }
        }
        std::fs::write(hashes.join(format!("{}.md", arm.short)), text)
            .map_err(|e| format!("write hash report: {e}"))?;
    }

    let baseline = &summaries[0];
    let mut text = format!("# perf-compare {}\n\n{head}\n", name);
    text.push_str("| | hash | subject | runs passed |\n|---|---|---|---|\n");
    for (index, (arm, summary)) in arms.iter().zip(&summaries).enumerate() {
        text.push_str(&format!(
            "| {} | [{}](hashes/{}.md) | {} | {}/{} |\n",
            if index == 0 { "baseline" } else { "candidate" },
            arm.short,
            arm.short,
            arm.subject,
            summary.passed,
            summary.runs
        ));
    }
    text.push_str("\nEach cell: mean over runs [min–max]; candidates add Δ vs baseline. `separated` means the ranges do not overlap.\n\n| metric |");
    for arm in arms {
        text.push_str(&format!(" {} |", arm.short));
    }
    text.push_str("\n|---|");
    text.push_str(&"---|".repeat(arms.len()));
    text.push('\n');
    let mut flags = Vec::new();
    for (row, (name, base_values)) in baseline.metrics.iter().enumerate() {
        text.push_str(&format!("| {name} |"));
        for (index, summary) in summaries.iter().enumerate() {
            let values = &summary.metrics[row].1;
            let (Some(value), Some((low, high))) = (mean(values), range(values)) else {
                text.push_str(" — |");
                continue;
            };
            let digits = if name.ends_with("MiB") || name.starts_with("hitches") {
                0
            } else {
                3
            };
            let mut cell = format!(" {value:.digits$} [{low:.digits$}–{high:.digits$}]");
            if index > 0
                && let (Some(base_value), Some((base_low, base_high))) =
                    (mean(base_values), range(base_values))
            {
                let delta = value - base_value;
                let separated = values.len() > 1
                    && base_values.len() > 1
                    && (low > base_high || high < base_low);
                if name.ends_with("MiB") {
                    cell.push_str(&format!(" Δ {delta:+.0} MiB"));
                    if delta >= RSS_LINE_MIB {
                        flags.push(format!("{} {name} +{delta:.0} MiB", arms[index].short));
                    }
                } else if base_value != 0.0 {
                    let pct = delta * 100.0 / base_value;
                    cell.push_str(&format!(" Δ {pct:+.1}%"));
                    if name.starts_with("wall") && pct >= FRAME_LINE_PCT && separated {
                        flags.push(format!("{} {name} {pct:+.1}%", arms[index].short));
                    }
                }
                if separated {
                    cell.push_str(" separated");
                }
            }
            text.push_str(&cell);
            text.push_str(" |");
        }
        text.push('\n');
    }
    text.push_str(&format!(
        "\nVerdict: {}\n",
        if config.rounds < 2 {
            "none: one round has no spread, so no range can separate; rerun with ROUNDS≥3.".to_owned()
        } else if flags.is_empty() {
            format!(
                "no candidate crosses the worthwhile lines (wall ≥{FRAME_LINE_PCT}% with separated ranges, RSS ≥{RSS_LINE_MIB} MiB)."
            )
        } else {
            format!("over the line: {}.", flags.join("; "))
        }
    ));
    text.push_str("\nRuns: `runs/<NN>-<hash>/` hold runner.log, memory.tsv and metrics.json; each names its approved-tests run with the trace.\n");
    let report_path = series.join("report.md");
    std::fs::write(&report_path, &text).map_err(|e| format!("write report: {e}"))?;

    write_json(
        &series.join("series.json"),
        &json!({
            "scenario": SCENARIO,
            "replay": replay,
            "rounds": config.rounds,
            "warmup": config.warmup,
            "root_head": git(root, &["rev-parse", "HEAD"]).ok(),
            "arms": arms.iter().map(|arm| json!({
                "sha": arm.sha, "subject": arm.subject, "bin": arm.bin,
            })).collect::<Vec<_>>(),
            "runs": results.iter().map(|(ordinal, arm, metrics)| json!({
                "ordinal": ordinal, "sha": arms[*arm].sha, "metrics": metrics_json(metrics),
            })).collect::<Vec<_>>(),
            "flags": flags,
        }),
    )?;
    let index_line = format!(
        "- [{}]({}/report.md) {} — {}\n",
        name,
        name,
        arms.iter()
            .map(|arm| arm.short.as_str())
            .collect::<Vec<_>>()
            .join(" vs "),
        if flags.is_empty() {
            "within the lines".to_owned()
        } else {
            flags.join("; ")
        },
    );
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(base.join("index.md"))
        .and_then(|mut index| index.write_all(index_line.as_bytes()))
        .map_err(|e| format!("append index.md: {e}"))?;

    println!("\n{text}");
    println!("report: {}", report_path.display());
    Ok(())
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| format!("json: {e}"))?;
    std::fs::write(path, text + "\n").map_err(|e| format!("write {}: {e}", path.display()))
}

fn utc_stamp() -> String {
    let output = Command::new("date")
        .args(["-u", "+%Y%m%dT%H%M%SZ"])
        .output()
        .ok()
        .filter(|output| output.status.success());
    output.map_or_else(
        || "series".to_owned(),
        |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    )
}
