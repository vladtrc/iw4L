use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::runner::{End, Event, Outcome};
use crate::scenario::Phase;
use crate::scenarios::heavy_gameplay_lifecycle::SCENARIO;

pub fn write(run_dir: &Path, manifest: &Value) -> Result<(), String> {
    let path = run_dir.join("run.json");
    let tmp = run_dir.join(".run.json.tmp");
    let text = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
}

fn stdout_of(program: &str, args: &[&str], cwd: &Path) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

pub fn build_facts(root: &Path, bin: &Path) -> Value {
    let dirty = stdout_of(
        "git",
        &["status", "--porcelain", "--untracked-files=no"],
        root,
    )
    .map(|s| s.lines().count());
    let profile = bin
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned());
    let meta = std::fs::metadata(bin).ok();
    json!({
        "commit": stdout_of("git", &["rev-parse", "HEAD"], root),
        "dirty_tracked_files": dirty,
        "profile": profile,
        "binary": bin.display().to_string(),
        "binary_bytes": meta.as_ref().map(std::fs::Metadata::len),
        "binary_modified_unix": meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
    })
}

pub fn platform_facts() -> Value {
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "kernel": stdout_of("uname", &["-sr"], Path::new(".")),
        "session": std::env::var("XDG_SESSION_TYPE").ok(),
        "cpus": std::thread::available_parallelism().map(|n| n.get()).ok(),
    })
}

#[derive(Default)]
struct PhaseLog {
    forced_spawn: Option<String>,
    give: Vec<String>,
    warnings: Noise,
}

#[derive(Default, Clone)]
struct Noise {
    warn: u64,
    error: u64,
    notable: Vec<String>,
    heads: BTreeMap<String, u64>,
}

const NOTABLE_KEPT: usize = 20;
const HEADS_KEPT: usize = 10;
const HEAD_CHARS: usize = 72;

impl Noise {
    fn push(&mut self, level: &str, message: &str) {
        let failure = [
            "timed out",
            "script aborted",
            "panicked",
            "refused —",
            " refused tried=",
            "spawn: refused",
            "spawn: aborted",
        ]
        .iter()
        .any(|s| message.contains(s));
        match level {
            "error" => self.error += 1,
            "warn" => self.warn += 1,
            _ if failure => {}
            _ => return,
        }
        if (level == "error" || failure) && self.notable.len() < NOTABLE_KEPT {
            self.notable.push(message.to_owned());
        } else if level == "warn" {
            let head: String = message.chars().take(HEAD_CHARS).collect();
            *self.heads.entry(head).or_insert(0) += 1;
        }
    }

    fn to_json(&self) -> Value {
        let mut heads: Vec<(&String, &u64)> = self.heads.iter().collect();
        heads.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        json!({
            "warn": self.warn,
            "error": self.error,
            "notable": self.notable,
            "top_warn_heads": heads
                .iter()
                .take(HEADS_KEPT)
                .map(|(head, n)| json!({ "head": head, "count": n }))
                .collect::<Vec<_>>(),
            "distinct_warn_heads": self.heads.len(),
        })
    }
}

fn read_log(path: &Path) -> (BTreeMap<String, PhaseLog>, Noise) {
    let mut phases: BTreeMap<String, PhaseLog> = BTreeMap::new();
    let mut unphased = Noise::default();
    let Ok(text) = std::fs::read_to_string(path) else {
        return (phases, unphased);
    };
    let mut current: Option<String> = None;
    for line in text.lines() {
        if let Some(at) = line.find("benchmark-mark:") {
            if let Some(event) = crate::runner::parse(&line[at..], "benchmark-mark:", 0)
                && let Some(label) = event.get("label")
                && let Some(phase) = label.strip_suffix(".begin")
            {
                current = Some(phase.to_owned());
            }
            continue;
        }
        let bucket = current
            .as_ref()
            .map(|p| phases.entry(p.clone()).or_default());
        let level = line.split("  ").nth(1).unwrap_or("").trim();
        let message = line.split("  ").skip(2).collect::<Vec<_>>().join("  ");
        let message = if message.is_empty() {
            line.to_owned()
        } else {
            message
        };
        if message.starts_with("spawn: forced") {
            if let Some(b) = bucket {
                b.forced_spawn = Some(message);
            }
        } else if message.starts_with("give:") {
            if let Some(b) = bucket {
                b.give.push(message);
            }
        } else {
            match bucket {
                Some(b) => b.warnings.push(level, &message),
                None => unphased.push(level, &message),
            }
        }
    }
    (phases, unphased)
}

fn parse_forced(line: &str) -> Value {
    if line.contains(" refused ") {
        return json!({ "refused": true, "line": line });
    }
    let origin = line
        .split("origin=[")
        .nth(1)
        .and_then(|s| s.split(']').next())
        .map(|s| {
            s.split(',')
                .filter_map(|v| v.trim().parse::<f64>().ok())
                .collect::<Vec<_>>()
        });
    let after = line.split("origin=[").nth(1).unwrap_or("");
    let field = |key: &str| {
        after
            .split_whitespace()
            .find_map(|t| t.strip_prefix(key))
            .map(str::to_owned)
    };
    json!({
        "refused": false,
        "origin": origin,
        "yaw": field("yaw=").and_then(|v| v.parse::<f64>().ok()),
        "source": field("source="),
        "tried": field("tried=").and_then(|v| v.parse::<u64>().ok()),
        "fallback": field("fallback="),
    })
}

fn mark<'a>(marks: &'a [Event], label: &str) -> Option<&'a Event> {
    marks.iter().find(|m| m.get("label") == Some(label))
}

fn origin(event: &Event) -> Option<[f64; 3]> {
    let raw = event.get("origin")?;
    let v: Vec<f64> = raw.split(',').filter_map(|s| s.parse().ok()).collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

fn facts(event: &Event) -> Value {
    json!({
        "ns": event.ns(),
        "controller_ms": event.at_ms,
        "tick": event.num("tick"),
        "clients": event.num("clients"),
        "alive": event.num("alive"),
        "match_kills": event.num("kills"),
        "match_deaths": event.num("deaths"),
        "local": event.get("local"),
        "local_life": event.num("local_life"),
        "local_deaths": event.num("local_deaths"),
        "origin": origin(event),
        "yaw": event.num("yaw"),
        "rss_mib": event.num("rss_mib"),
        "heap_mib": event.num("heap_mib"),
    })
}

struct Assertions(Vec<Value>);

impl Assertions {
    fn check(&mut self, id: &str, passed: bool, evidence: impl Into<Value>) {
        self.0
            .push(json!({ "id": id, "passed": passed, "evidence": evidence.into() }));
    }
    fn all_passed(&self) -> bool {
        self.0.iter().all(|a| a["passed"] == true)
    }
}

pub fn finish(run_dir: &Path, manifest: &mut Value, phases: &[Phase], outcome: &Outcome) -> bool {
    let child = run_dir.join("iw4l-artifacts");
    let (log, unphased) = read_log(&child.join("logs/latest.log"));
    let marks = &outcome.marks;
    let lifecycle = &outcome.lifecycle;

    let (end_json, exit_ms, exited_clean) = match &outcome.end {
        End::Exited { code, at_ms } => (
            json!({ "kind": "exited", "code": code, "controller_ms": at_ms }),
            Some(*at_ms),
            *code == Some(0),
        ),
        End::Timeout { waiting_for, at_ms } => (
            json!({ "kind": "killed_after_timeout", "waiting_for": waiting_for, "controller_ms": at_ms }),
            None,
            false,
        ),
        End::SpawnFailed(reason) => (
            json!({ "kind": "spawn_failed", "reason": reason }),
            None,
            false,
        ),
    };
    manifest["process"] = end_json;
    let failure_reason = match &outcome.end {
        End::Exited { code, .. } => {
            format!("process exited (code {code:?}) before this phase ended")
        }
        End::Timeout { waiting_for, .. } => format!("timeout waiting for {waiting_for}"),
        End::SpawnFailed(reason) => reason.clone(),
    };

    manifest["render"] = match &outcome.runtime {
        Some(r) => json!({
            "backend": r.get("backend"),
            "adapter": r.get("adapter"),
            "window": r.get("window"),
            "render_target": r.get("target"),
            "present_mode": r.get("present"),
        }),
        None => json!({ "backend": "unknown", "note": "the game printed no runtime line" }),
    };

    let mut phase_rows = Vec::new();
    for phase in phases {
        let begin = mark(marks, &format!("{}.begin", phase.name));
        let end = if phase.name == "quit" {
            None
        } else {
            mark(marks, &format!("{}.end", phase.name))
        };
        let (status, reason, completed_ms) = match (begin, end, phase.name.as_str()) {
            (Some(_), _, "quit") => match exit_ms {
                Some(ms) if exited_clean => ("completed", None, Some(ms)),
                Some(ms) => ("failed", Some(failure_reason.clone()), Some(ms)),
                None => ("failed", Some(failure_reason.clone()), None),
            },
            (Some(_), Some(e), _) => ("completed", None, Some(e.at_ms)),
            (Some(_), None, _) => ("failed", Some(failure_reason.clone()), None),
            (None, _, _) => ("not_reached", None, None),
        };
        let warnings = log
            .get(&phase.name)
            .map(|l| l.warnings.to_json())
            .unwrap_or_else(|| Noise::default().to_json());
        phase_rows.push(json!({
            "name": phase.name,
            "status": status,
            "failure_reason": reason,
            "started_controller_ms": begin.map(|b| b.at_ms),
            "completed_controller_ms": completed_ms,
            "duration_ms": begin.zip(completed_ms).map(|(b, e)| e.saturating_sub(b.at_ms)),
            "game_ns": begin.and_then(Event::ns).zip(end.and_then(Event::ns))
                .map(|(b, e)| json!({ "begin": b, "end": e, "duration": e.saturating_sub(b) })),
            "facts_at_end": end.map(facts),
            "log": warnings,
        }));
    }
    if let Some(first) = phase_rows.first_mut() {
        first["started_controller_ms"] = 0.into();
        if let Some(end) = first["completed_controller_ms"].as_u64() {
            first["duration_ms"] = end.into();
        }
    }
    manifest["phases"] = phase_rows.into();

    let disconnect = lifecycle.iter().find(|e| e.name == "disconnect_requested");
    let after = |name: &str| {
        let from = disconnect.and_then(Event::ns).unwrap_or(0);
        lifecycle
            .iter()
            .find(|e| e.name == name && e.ns().unwrap_or(0) >= from)
    };
    let boundary = |e: Option<&Event>| {
        e.map(|e| {
            let mut v = json!({ "ns": e.ns(), "controller_ms": e.at_ms });
            for (k, val) in &e.fields {
                if k != "ns" && k != "pid" {
                    v[k] = val.clone().into();
                }
            }
            v
        })
    };
    let revoked = after("local_session_revoked");
    let menu = after("menu_interactive");
    let teardown = after("old_runtime_teardown_complete");
    let settled = after("retirement_settled");
    let quit = lifecycle.iter().find(|e| e.name == "quit_requested");
    manifest["lifecycle"] = json!({
        "disconnect_requested": boundary(disconnect),
        "local_session_revoked": boundary(revoked),
        "menu_interactive": boundary(menu),
        "old_runtime_teardown_complete": boundary(teardown),
        "retirement_settled": boundary(settled),
        "retirement_outstanding": teardown.and_then(|t| t.get("in_flight")).map(str::to_owned),
        "quit_requested": boundary(quit),
        "process_exited": exit_ms.map(|ms| json!({ "controller_ms": ms })),
        "quit_to_exit_ms": quit.zip(exit_ms).map(|(q, e)| e.saturating_sub(q.at_ms)),
    });

    let mut asserts = Assertions(Vec::new());
    let scenes = manifest["scenes"]
        .as_array()
        .cloned()
        .unwrap_or_else(Vec::new);
    let mut observed_scenes = Vec::new();
    for mut scene in scenes {
        let phase = scene["phase"].as_str().unwrap_or("").to_owned();
        let index = phase
            .rsplit('_')
            .next()
            .and_then(|n| n.parse::<usize>().ok())
            .map_or(0, |n| n.saturating_sub(1));
        let ticks = SCENARIO.scenes.get(index).map_or(0, |s| s.ticks);
        let phase_log = log.get(&phase);
        let spawn = phase_log
            .and_then(|l| l.forced_spawn.as_deref())
            .map(parse_forced);
        let spawned = mark(marks, &format!("{phase}.spawned"));
        let done = mark(marks, &format!("{phase}.inputs_done"));
        let give = phase_log.map(|l| l.give.clone()).unwrap_or_else(Vec::new);
        let weapon_used = scene["weapon"]["give"].as_str().map(|asked| {
            if give.iter().any(|g| g.starts_with("give: queued")) {
                json!({ "used": asked, "fallback_taken": false, "log": give })
            } else {
                json!({ "used": scene["weapon"]["fallback"], "fallback_taken": true, "log": give })
            }
        });
        let tick_delta = spawned
            .and_then(|s| s.num("tick"))
            .zip(done.and_then(|d| d.num("tick")))
            .map(|(a, b)| b - a);
        let travel = spawned
            .and_then(origin)
            .zip(done.and_then(origin))
            .map(|(a, b)| {
                ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
            });
        let died = done.is_some_and(|d| d.get("local") != Some("Alive"))
            || spawned
                .and_then(|s| s.num("local_deaths"))
                .zip(done.and_then(|d| d.num("local_deaths")))
                .is_some_and(|(a, b)| b > a);
        let lives = spawned
            .and_then(|s| s.num("local_life"))
            .zip(done.and_then(|d| d.num("local_life")))
            .map(|(a, b)| b - a + 1.0);
        let kills_deaths = spawned.zip(done).map(|(s, d)| {
            json!({
                "kills": d.num("kills").unwrap_or(0.0) - s.num("kills").unwrap_or(0.0),
                "deaths": d.num("deaths").unwrap_or(0.0) - s.num("deaths").unwrap_or(0.0),
            })
        });

        let landed = spawn.as_ref().is_some_and(|s| s["refused"] == false);
        asserts.check(
            &format!("{phase}.force_spawn_landed"),
            landed,
            spawn.clone().unwrap_or(Value::Null),
        );
        asserts.check(
            &format!("{phase}.simulation_advanced"),
            tick_delta.is_some_and(|d| d >= f64::from(ticks) * 0.9),
            json!({ "ticks": tick_delta, "declared": ticks }),
        );
        asserts.check(
            &format!("{phase}.others_alive"),
            done.and_then(|d| d.num("alive")).is_some_and(|a| a >= 2.0),
            json!({ "alive_at_end": done.and_then(|d| d.num("alive")) }),
        );
        asserts.check(
            &format!("{phase}.input_moved_or_died"),
            travel.is_some_and(|t| t >= 32.0) || died,
            json!({ "travel_units": travel, "died": died }),
        );

        scene["observed"] = json!({
            "spawn": spawn,
            "weapon": weapon_used,
            "at_spawned": spawned.map(facts),
            "at_inputs_done": done.map(facts),
            "ticks": tick_delta,
            "travel_units": travel,
            "died": died,
            "lives": lives,
            "kills_deaths_in_match": kills_deaths,
        });
        observed_scenes.push(scene);
    }
    manifest["scenes"] = observed_scenes.into();

    let players = f64::from(SCENARIO.players);
    let clients_at = |label: &str| mark(marks, label).and_then(|m| m.num("clients"));
    let a_clients = clients_at("a.populate.end");
    let b_clients = clients_at("b.populate.end");
    manifest["bots"]["actual_players_a"] = a_clients.into();
    manifest["bots"]["actual_players_b"] = b_clients.into();

    asserts.check(
        "a.map_loaded",
        mark(marks, "a.cold_load.end").is_some(),
        "a.cold_load.end mark",
    );
    asserts.check(
        "a.players",
        a_clients.is_some_and(|c| c >= players),
        json!({ "clients": a_clients, "target": players }),
    );
    asserts.check(
        "disconnect.requested",
        disconnect.is_some(),
        "lifecycle disconnect_requested",
    );
    asserts.check(
        "disconnect.session_revoked_after_request",
        revoked.is_some(),
        boundary(revoked).unwrap_or(Value::Null),
    );
    asserts.check(
        "disconnect.menu_interactive_after_revoke",
        menu.zip(revoked).is_some_and(|(m, r)| m.ns() >= r.ns()),
        boundary(menu).unwrap_or(Value::Null),
    );
    asserts.check(
        "disconnect.old_runtime_teardown_complete",
        teardown.is_some(),
        boundary(teardown).unwrap_or(Value::Null),
    );
    asserts.check(
        "b.map_loaded",
        mark(marks, "b.load.end").is_some(),
        "b.load.end mark",
    );
    asserts.check(
        "b.players",
        b_clients.is_some_and(|c| c >= players),
        json!({ "clients": b_clients, "target": players }),
    );
    asserts.check("quit.requested", quit.is_some(), "lifecycle quit_requested");
    asserts.check(
        "quit.process_exited_by_itself",
        exited_clean && quit.is_some(),
        manifest["process"].clone(),
    );

    let mut captures = serde_json::Map::new();
    for capture in phases.iter().filter_map(|p| p.capture) {
        let png = child
            .join("screenshots/approved")
            .join(format!("{capture}.png"));
        let png_to = run_dir.join(format!("{capture}.png"));
        let dump = find_dump(&child.join("dumps"), capture);
        let dump_to = run_dir.join(format!("{capture}.dump.txt"));
        let png_ok = png.is_file() && std::fs::rename(&png, &png_to).is_ok();
        let dump_ok = dump.is_some_and(|d| std::fs::rename(d, &dump_to).is_ok());
        asserts.check(
            &format!("capture.{capture}"),
            png_ok,
            png_to.display().to_string(),
        );
        captures.insert(
            capture.to_owned(),
            json!({
                "screenshot": png_ok.then(|| png_to.display().to_string()),
                "dump": dump_ok.then(|| dump_to.display().to_string()),
            }),
        );
    }

    manifest["artifacts"] = json!({
        "captures": captures,
        "log": child.join("logs/latest.log").display().to_string(),
        "perf_run": perf_run(&child.join("runs")).map(|p| p.display().to_string()),
        "child_stdout": run_dir.join("child_stdout.txt").display().to_string(),
        "child_stderr": run_dir.join("child_stderr.txt").display().to_string(),
    });
    manifest["log_outside_phases"] = unphased.to_json();

    let passed = asserts.all_passed() && exited_clean;
    manifest["assertions"] = asserts.0.into();
    manifest["result"] = match (&outcome.end, passed) {
        (_, true) => "passed",
        (End::Timeout { .. }, _) => "failed/timeout",
        _ => "failed",
    }
    .into();
    passed
}

fn find_dump(dir: &Path, capture: &str) -> Option<PathBuf> {
    let suffix = format!("-{capture}.txt");
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(&suffix))
        })
}

fn perf_run(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.is_dir())
}
