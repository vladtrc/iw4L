use crate::evidence::{self, RunReport, rows};
use crate::{
    Result,
    author::{Program, Tape, tape_digest},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
fn delta(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    a?.checked_sub(b?)
}
fn command_match(tape: &Tape, run: &RunReport) -> Result<(bool, usize)> {
    let receipts = rows(&run.receipts, "receipts")?;
    let base: i32 = run
        .base_command_time
        .ok_or("missing evidence: base_command_time")?
        .try_into()?;
    let matched = receipts
        .iter()
        .zip(&tape.samples)
        .enumerate()
        .all(|(i, (r, s))| {
            let bytes = r.source_bytes.as_ref().unwrap_or(&r.bytes);
            bytes.as_slice() == s.canonical(base).as_slice()
                && r.sequence == Some(i64::from(tape.first_sequence) + i as i64)
        });
    Ok((matched, receipts.len()))
}
pub fn integrity(p: &Program, d: &Value) -> Result<Value> {
    let d = evidence::parse_run(d)?;
    integrity_run(p, &d)
}
fn integrity_run(p: &Program, d: &RunReport) -> Result<Value> {
    let mut tapes = BTreeMap::from([(0, &p.tape)]);
    for a in &p.actors {
        if tapes.insert(a.client, &a.tape).is_some() {
            return Err("duplicate program client".into());
        }
    }
    let local: u32 = d.client.try_into()?;
    let mut runs = BTreeMap::from([(local, d)]);
    for r in rows(&d.actors, "actors")? {
        let client = r.client.try_into()?;
        if runs.insert(client, r).is_some() {
            return Err("duplicate receipt client".into());
        }
    }
    let mut checks = BTreeMap::new();
    checks.insert("actor_set".to_owned(), tapes.keys().eq(runs.keys()));
    let mut ensemble = Sha256::new();
    ensemble.update(b"iw4l-input-ensemble-v1\0");
    for (client, tape) in tapes {
        let digest = tape_digest(tape, client);
        ensemble.update(digest);
        let Some(run) = runs.get(&client) else {
            checks.insert(format!("actor_{client}_present"), false);
            continue;
        };
        let digest_hex = digest
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>();
        checks.insert(
            format!("actor_{client}_digest"),
            run.program_sha256 == digest_hex,
        );
        checks.insert(
            format!("actor_{client}_complete"),
            run.complete && run.failure.is_none(),
        );
        let (matched, count) = command_match(tape, run)?;
        let terminal = tape.stop_on_death && run.death_tick.is_some();
        checks.insert(
            format!("actor_{client}_commands"),
            matched
                && if terminal {
                    count <= tape.samples.len()
                } else {
                    count == tape.samples.len()
                },
        );
    }
    checks.insert(
        "ensemble_digest".into(),
        d.ensemble_sha256 == format!("{:x}", ensemble.finalize()),
    );
    Ok(json!({"passed":checks.values().all(|v|*v),"checks":checks}))
}
pub fn declared(p: &Program, manifest: &Value, d: &Value) -> Result<Value> {
    let d = evidence::parse_run(d)?;
    let manifest = evidence::parse_manifest(manifest)?;
    let integrity = integrity_run(p, &d)?;
    let source = serde_json::to_value(p)?;
    let mut checks = BTreeMap::new();
    checks.insert(
        "compiled_program_digest".to_owned(),
        manifest.program_json_sha256
            == format!("{:x}", Sha256::digest(serde_json::to_vec(&source)?)),
    );
    let names: BTreeMap<&str, i64> = manifest
        .actors
        .iter()
        .map(|a| (a.name.as_str(), a.client))
        .collect();
    let mut runs = BTreeMap::from([(d.client, &d)]);
    for a in rows(&d.actors, "actors")? {
        runs.insert(a.client, a);
    }
    let deaths: Vec<_> = rows(&d.events, "events")?
        .iter()
        .filter(|e| e.data.kind == "died")
        .collect();
    for (i, e) in manifest.expectations.iter().enumerate() {
        let kind = if e.kind.is_empty() {
            "invalid"
        } else {
            e.kind.as_str()
        };
        let outcome = (|| -> Result<bool> {
            let name = e.actor.as_deref().ok_or("missing actor expectation")?;
            let client = *names.get(name).ok_or("missing actor")?;
            let r = *runs.get(&client).ok_or("missing run")?;
            let receipts = rows(&r.receipts, "receipts")?;
            let weapon = |key: Option<&str>, default: &str| -> Result<i64> {
                let s = key.unwrap_or(default);
                manifest
                    .bindings
                    .weapons
                    .get(s)
                    .copied()
                    .ok_or_else(|| "missing weapon".into())
            };
            let beat = |key: Option<&str>| -> Result<i64> {
                let key = key.ok_or("missing beat")?;
                manifest
                    .beats_ms
                    .get(key)
                    .copied()
                    .ok_or_else(|| "missing beat time".into())
            };
            let source = || -> Result<i64> {
                names
                    .get(e.source.as_deref().ok_or("missing source")?)
                    .copied()
                    .ok_or_else(|| "missing source actor".into())
            };
            Ok(match kind {
                "shots" => {
                    let expected: Vec<_> = e
                        .weapons
                        .as_ref()
                        .ok_or("missing evidence array: weapons")?
                        .iter()
                        .map(|w| manifest.bindings.weapons.get(w).copied())
                        .collect();
                    let actual: Vec<_> =
                        rows(&r.shots, "shots")?.iter().map(|s| s.weapon).collect();
                    expected == actual && expected.iter().all(Option::is_some)
                }
                "pickup_from_death" => {
                    let w = weapon(e.weapon.as_deref(), "")?;
                    let s = source()?;
                    rows(&d.pickups, "pickups")?.iter().any(|p| {
                        p.picker == Some(client)
                            && p.weapon == Some(w)
                            && deaths.iter().any(|ev| {
                                ev.data.victim == Some(s)
                                    && ev.data.attacker == Some(client)
                                    && ev.tick.is_some_and(|t| p.tick.is_some_and(|pt| t <= pt))
                            })
                    })
                }
                "deferred_can_swap" => {
                    let w = weapon(e.weapon.as_deref(), "")?;
                    let earliest = beat(e.consume_in.as_deref())?;
                    let raised = receipts.iter().find(|row| {
                        row.weapon == Some(w) && row.weapon_anim.is_some_and(|a| a & 511 == 12)
                    });
                    let pending: Vec<_> = receipts
                        .iter()
                        .filter(|row| {
                            row.inventory.contains(&w)
                                && raised.is_none_or(|a| {
                                    row.sequence
                                        .is_some_and(|s| a.sequence.is_some_and(|asq| s < asq))
                                })
                        })
                        .collect();
                    raised.is_some_and(|a| {
                        delta(a.server_time, r.base_command_time).is_some_and(|t| t >= earliest)
                    }) && !pending.is_empty()
                        && pending.iter().all(|row| {
                            row.inventory
                                .iter()
                                .position(|v| *v == w)
                                .is_some_and(|i| row.weapon_data.get(i * 5) == Some(&0))
                        })
                }
                "gravity_portal" => {
                    let worlds = &manifest.bindings.worlds;
                    let a = e.source.as_deref().ok_or("missing source")?;
                    let b = e.destination.as_deref().ok_or("missing destination")?;
                    worlds.get(a).is_some_and(|v| !v.is_null())
                        && worlds.get(b).is_some_and(|v| !v.is_null())
                        && receipts.windows(2).any(|pair| {
                            pair[0].world.map(Value::from) == worlds.get(a).cloned()
                                && pair[1].world.map(Value::from) == worlds.get(b).cloned()
                        })
                }
                "native_final_kill" => {
                    let w = weapon(e.weapon.as_deref(), "")?;
                    let ended = rows(&d.events, "events")?
                        .iter()
                        .rev()
                        .find(|ev| ev.data.kind == "match_ended");
                    deaths.last().zip(ended).is_some_and(|(ev, end)| {
                        ev.data.attacker == Some(client)
                            && ev.data.weapon == Some(w)
                            && ev.tick.is_some_and(|t| end.tick.is_some_and(|et| t <= et))
                    })
                }
                "survives_hipfire" => {
                    let start = beat(e.beat.as_deref())?;
                    let end = start + e.window_ms.unwrap_or(3000);
                    let sniper = *runs
                        .get(
                            names
                                .get(e.source.as_deref().ok_or("missing source actor")?)
                                .ok_or("missing sniper")?,
                        )
                        .ok_or("missing sniper run")?;
                    let w = weapon(e.weapon.as_deref(), "")?;
                    let hipfires: Vec<_> = rows(&sniper.shots, "shots")?
                        .iter()
                        .filter(|s| s.weapon == Some(w) && s.ads_frac == Some(0.))
                        .filter_map(|s| s.id)
                        .collect();
                    rows(&sniper.shot_scenes, "shot_scenes")?.iter().any(|s| {
                        let before = s.targets.iter().find(|t| t.client == client);
                        let after = receipts
                            .iter()
                            .find(|row| row.tick.is_some_and(|t| s.tick.is_some_and(|st| t > st)));
                        before.zip(after).is_some_and(|(b, a)| {
                            s.shot.is_some_and(|id| hipfires.contains(&id))
                                && delta(a.server_time, r.base_command_time)
                                    .is_some_and(|t| (start..=end).contains(&t))
                                && a.health.is_some_and(|h| h > 0)
                                && a.health.is_some_and(|h| b.health.is_some_and(|bh| h < bh))
                                && a.tick.is_some_and(|t| s.tick.is_some_and(|st| t <= st + 1))
                        })
                    })
                }
                _ => false,
            })
        })()
        .unwrap_or(false);
        checks.insert(format!("expectation_{i}_{kind}"), outcome);
    }
    Ok(
        json!({"passed":integrity["passed"]==true&&checks.values().all(|v|*v),"checks":checks,"integrity":integrity,"scope":"Native command integrity and declared simulation outcomes. Visual review remains separate."}),
    )
}
