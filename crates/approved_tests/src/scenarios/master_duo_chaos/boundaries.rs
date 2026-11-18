use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    time::{Duration, Instant},
};

use asset_core::atlas::{WorldId, WorldLayout};
use serde_json::{Value, json};

use super::{Player, Result, event, field, launch};

fn check(manifest: &mut Value, name: &str, passed: bool, evidence: Value) -> Result<()> {
    if !manifest["boundary_assertions"].is_array() {
        manifest["boundary_assertions"] = json!([]);
    }
    manifest["boundary_assertions"]
        .as_array_mut()
        .ok_or("missing assertions")?
        .push(json!({ "name": name, "passed": passed, "evidence": evidence }));
    if passed {
        Ok(())
    } else {
        Err(format!("boundary assertion failed: {name}"))
    }
}

fn dump(player: &mut Player, label: &str) -> Result<String> {
    player.send(&format!("dump {label}; mark {label}"))?;
    player.mark(label)?;
    let suffix = format!("-{label}.txt");
    let path = fs::read_dir(player.directory.join("iw4l-artifacts/dumps"))
        .map_err(|e| e.to_string())?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(&suffix))
        })
        .ok_or_else(|| format!("missing dump {label}"))?;
    fs::read_to_string(path).map_err(|e| e.to_string())
}

fn rows(text: &str, prefix: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|l| l.strip_prefix(prefix))
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

pub(super) fn browser_boundary(
    binary: &Path,
    cwd: &Path,
    run: &Path,
    games: &Path,
    descriptor: &Path,
    manifest: &mut Value,
) -> Result<()> {
    let mut browser = launch(binary, cwd, run, games, descriptor, "browser", None)?;
    browser.send("wait progression; openmenu find_lobbies")?;
    let expected: BTreeSet<_> = manifest["large_rooms"]["ids"]
        .as_array()
        .ok_or("missing fixture IDs")?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let families = manifest["large_rooms"]["families"].clone();
    for refresh in 0..2 {
        browser.send("ui_browser_refresh")?;
        let mut snapshot = Value::Null;
        for attempt in 0..30 {
            browser.send("wait 0.5s")?;
            snapshot = rows(
                &dump(&mut browser, &format!("browser_{refresh}_{attempt}"))?,
                "browser_snapshot ",
            )
            .into_iter()
            .next()
            .ok_or("browser diagnostic missing")?;
            if snapshot["loading"] == false
                && snapshot["adverts"].as_array().is_some_and(|a| a.len() == 2)
            {
                break;
            }
        }
        let listed: BTreeSet<_> = snapshot["adverts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a["id"].as_str())
            .map(str::to_owned)
            .collect();
        check(
            manifest,
            &format!("browser.{refresh}.complete"),
            snapshot["loading"] == false && snapshot["error"].is_null() && listed == expected,
            snapshot.clone(),
        )?;
        check(
            manifest,
            &format!("browser.{refresh}.inventories"),
            snapshot["adverts"].as_array().is_some_and(|a| {
                a.iter()
                    .all(|a| a["requires"] == families && a["available"] == families)
            }),
            snapshot["adverts"].clone(),
        )?;
        let ui_ids: BTreeSet<_> = (0..2)
            .filter_map(|row| {
                snapshot["ui"][format!("ui_browser_{row}_id")]
                    .as_str()
                    .map(str::to_owned)
            })
            .collect();
        let visible = (0..2).all(|row| snapshot["ui"][format!("ui_browser_{row}_visible")] == "1");
        check(
            manifest,
            &format!("browser.{refresh}.native_rows"),
            visible && ui_ids == expected,
            snapshot["ui"].clone(),
        )?;
        check(
            manifest,
            &format!("browser.{refresh}.bounded_cells"),
            (0..2).all(|row| {
                [("assets", 10), ("state", 16)].iter().all(|(cell, limit)| {
                    snapshot["ui"][format!("ui_browser_{row}_{cell}")]
                        .as_str()
                        .is_some_and(|text| text.chars().count() <= *limit)
                })
            }),
            snapshot["ui"].clone(),
        )?;
    }
    browser.send("screenshot approved/max_rooms; wait 1s")?;
    browser.finish()
}

fn stage_scene(run: &Path, role: &str, recipe: &Value, profile: Option<&Path>) -> Result<()> {
    let artifacts = run.join(role).join("iw4l-artifacts");
    fs::create_dir_all(artifacts.join("maps")).map_err(|e| e.to_string())?;
    fs::write(
        artifacts.join("maps/approved-scene.json"),
        serde_json::to_vec_pretty(recipe).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if let Some(profile) = profile {
        fs::create_dir_all(artifacts.join("profile")).map_err(|e| e.to_string())?;
        fs::copy(profile, artifacts.join("profile/classes.txt")).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn log(player: &Player) -> Result<String> {
    fs::read_to_string(player.directory.join("iw4l-artifacts/logs/latest.log"))
        .map_err(|e| e.to_string())
}

fn map_hash(player: &Player) -> Result<String> {
    log(player)?
        .split_whitespace()
        .filter_map(|part| part.strip_prefix("c_map="))
        .next_back()
        .map(str::to_owned)
        .ok_or("map hash missing".into())
}

fn shader_row(view: &Value, code: u16) -> Option<[f32; 4]> {
    let row = view["constants"][code.to_string()][0].as_array()?;
    if row.len() != 4 {
        return None;
    }
    let values = row
        .iter()
        .map(|v| {
            v.as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .map(f32::from_bits)
        })
        .collect::<Option<Vec<_>>>()?;
    Some([values[0], values[1], values[2], values[3]])
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.00001 * b.abs().max(1.0)
}

fn own_t5_environment(view: &Value, environment: &Value) -> bool {
    let sun = &environment["sun"];
    let xyz = |code, key: &str| {
        shader_row(view, code).is_some_and(|row| {
            (0..3).all(|i| {
                sun[key][i]
                    .as_f64()
                    .is_some_and(|v| close(row[i], v as f32))
            })
        })
    };
    xyz(0x232, "direction")
        && xyz(0x233, "diffuse")
        && xyz(0x234, "specular")
        && shader_row(view, 0x289)
            .zip(environment["exposure"].as_f64())
            .is_some_and(|(row, exposure)| close(row[0], exposure as f32 / 8.0))
        && shader_row(view, 0x25).is_some()
        && shader_row(view, 0x26).is_some()
}

fn environments(
    host: &mut Player,
    scene: &assets::SceneRecipe,
    manifest: &mut Value,
) -> Result<()> {
    let worlds: Vec<_> = scene
        .worlds
        .iter()
        .enumerate()
        .filter(|(_, key)| {
            key.split_once(':')
                .is_some_and(|(family, _)| family == "t5")
        })
        .map(|(i, _)| i)
        .collect();
    if worlds.len() < 2 {
        return Err("scene boundary needs two T5 worlds with distinct environments".into());
    }
    let mut evidence = Vec::new();
    for world in worlds {
        let pair = scene
            .portals
            .iter()
            .map(|p| p.pair())
            .find(|p| {
                (p.a.world == WorldId::ROOT && p.b.world == WorldId(world))
                    || (p.b.world == WorldId::ROOT && p.a.world == WorldId(world))
            })
            .ok_or("each T5 world needs a direct portal to the root")?;
        let (root, foreign) = if pair.a.world == WorldId::ROOT {
            (pair.a, pair.b)
        } else {
            (pair.b, pair.a)
        };
        let mut observations = Vec::new();
        for (kind, door, is_root) in [("portal", root, false), ("direct", foreign, true)] {
            let offset = WorldLayout.origin(door.world);
            let origin: [f32; 3] = std::array::from_fn(|i| {
                door.base[i] + offset[i] + door.normal[i] * 72.0 + if i == 2 { 10.5 } else { 0.0 }
            });
            let yaw = (-door.normal[1]).atan2(-door.normal[0]).to_degrees();
            let label = format!("material_{world}_{kind}");
            host.send(&format!(
                "move {} {} {} {yaw} 0; wait 2s; screenshot approved/{label}",
                origin[0], origin[1], origin[2]
            ))?;
            let text = dump(host, &label)?;
            let environment = rows(&text, "world_environment ")
                .into_iter()
                .find(|v| v["world"].as_u64() == Some(world as u64))
                .ok_or("world environment missing")?;
            let view = rows(&text, "view_material ")
                .into_iter()
                .find(|v| v["world"].as_u64() == Some(world as u64) && v["root"] == is_root)
                .ok_or_else(|| format!("missing {kind} view for world {world}"))?;
            check(
                manifest,
                &format!("environment.{world}.{kind}.own_sun_exposure"),
                own_t5_environment(&view, &environment),
                json!({ "world": environment, "view": view }),
            )?;
            observations.push(view);
        }
        let matching = [0x25, 0x26, 0x232, 0x233, 0x234, 0x289].iter().all(|code| {
            shader_row(&observations[0], *code)
                .zip(shader_row(&observations[1], *code))
                .is_some_and(|(a, b)| a == b)
        });
        check(
            manifest,
            &format!("environment.{world}.portal_matches_direct"),
            matching,
            json!(observations),
        )?;
        evidence.push(observations.remove(0));
    }
    for code in [0x25, 0x26, 0x233, 0x289] {
        check(
            manifest,
            &format!("environment.same_family.distinct_{code}"),
            shader_row(&evidence[0], code) != shader_row(&evidence[1], code),
            json!([
                shader_row(&evidence[0], code),
                shader_row(&evidence[1], code)
            ]),
        )?;
    }
    Ok(())
}

pub(super) fn scene_boundaries(
    binary: &Path,
    cwd: &Path,
    run: &Path,
    games: &Path,
    descriptor: &Path,
    source: &Path,
    manifest: &mut Value,
) -> Result<()> {
    let recipe: Value = serde_json::from_slice(&fs::read(source).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let scene = assets::SceneRecipe::parse(&recipe)?;
    let root_map = scene.worlds.first().ok_or("scene has no root world")?;
    manifest["scene_recipe"] = recipe.clone();
    stage_scene(run, "scene_host", &recipe, None)?;
    fs::copy(
        run.join("host/account.dat"),
        run.join("scene_host/account.dat"),
    )
    .map_err(|e| e.to_string())?;
    let mut host = launch(
        binary,
        cwd,
        run,
        games,
        descriptor,
        "scene_host",
        Some("approved-scene"),
    )?;
    host.send("wait world; spawn intervention; wait spawn; force_match_start; bot add 1; bot hold on; wait 20t; mark scene_ready")?;
    host.mark("scene_ready")?;
    let status = host.status()?;
    let room = field(&status, "room")
        .ok_or("scene room missing")?
        .to_owned();
    let hash = map_hash(&host)?;
    let profile = host.directory.join("iw4l-artifacts/profile/classes.txt");
    environments(&mut host, &scene, manifest)?;
    for role in ["scene_client_1", "scene_client_2"] {
        stage_scene(run, role, &recipe, Some(&profile))?;
        let mut client = launch(binary, cwd, run, games, descriptor, role, None)?;
        client.send(&format!("wait progression; unlock all; set ui_mapname {root_map}; set ui_gametype dm; ui_join_lobby_id {room}; wait world; spawn intervention; wait spawn; wait 2s; mark scene_joined"))?;
        client.mark("scene_joined")?;
        let output = client.stdout()?;
        let joined = event(&output, "scene_joined").ok_or("join mark missing")?;
        let client_hash = map_hash(&client)?;
        check(
            manifest,
            &format!("scene.{role}.admitted"),
            joined.get("local") == Some("Alive")
                && joined.num("clients").is_some_and(|n| n >= 3.0)
                && client_hash == hash,
            json!({ "hash": client_hash, "host_hash": hash, "mark": joined.fields }),
        )?;
        client.send("dump composed_join; screenshot approved/composed_join; disconnect; wait torn; mark torn")?;
        client.mark("torn")?;
        client.finish()?;
    }
    let flip = scene
        .portals
        .iter()
        .position(|p| p.ends.iter().all(|e| e.door().two_sided))
        .ok_or("flip refusal fixture needs a two-sided portal")?;
    let mut changed = recipe.clone();
    changed["portals"][flip]["flip"] = json!(!scene.portals[flip].flip);
    assets::SceneRecipe::parse(&changed)?;
    stage_scene(run, "scene_flip", &changed, Some(&profile))?;
    let mut rejected = launch(binary, cwd, run, games, descriptor, "scene_flip", None)?;
    rejected.send(&format!("wait progression; unlock all; set ui_mapname {root_map}; set ui_gametype dm; ui_join_lobby_id {room}"))?;
    let start = Instant::now();
    let refusal = loop {
        if let Ok(text) = log(&rejected)
            && let Some(line) = text
                .lines()
                .find(|l| l.contains("map content mismatch") || l.contains("MapContent"))
        {
            break line.to_owned();
        }
        if rejected
            .child
            .try_wait()
            .map_err(|e| e.to_string())?
            .is_some()
        {
            return Err("mismatch client exited before rejection evidence".into());
        }
        if start.elapsed() > Duration::from_secs(120) {
            return Err("portal flip mismatch was not rejected".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    check(
        manifest,
        "scene.portal_flip.rejected",
        !refusal.is_empty(),
        json!({ "portal": flip, "refusal": refusal, "hash": map_hash(&rejected)? }),
    )?;
    rejected.finish()?;
    host.send("wait 10t; mark scene_survived")?;
    host.mark("scene_survived")?;
    check(
        manifest,
        "scene.host_survived_mismatch",
        event(&host.stdout()?, "scene_survived").is_some_and(|e| e.get("local") == Some("Alive")),
        json!(host.status()?),
    )?;
    host.send("disconnect; wait torn; mark torn")?;
    host.mark("torn")?;
    host.finish()
}
