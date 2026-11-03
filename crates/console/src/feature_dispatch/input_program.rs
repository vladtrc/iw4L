use super::ConsoleEcho;
use bevy::prelude::*;
use serde_json::{Value, json};
use sim::input_program::InputProgram;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct InputProgramInstaller<'w> {
    prediction: ResMut<'w, net::ClientPredictionState>,
    pending: ResMut<'w, net::PendingClientSends>,
}

impl InputProgramInstaller<'_> {
    fn install(
        &mut self,
        world: &mut sim::SimWorld,
        program: InputProgram,
        actors: Vec<InputProgram>,
    ) -> Result<(), String> {
        world.install_input_ensemble(program, actors)?;
        self.prediction.0.disarm();
        *self.pending = net::PendingClientSends::default();
        Ok(())
    }
}

pub(crate) fn route_input_program(
    mut events: MessageReader<crate::ConsoleCommand>,
    mut authority: Option<ResMut<net::AuthorityWorld>>,
    role: Res<frame::RuntimeRole>,
    local: Res<net::LocalPresentClient>,
    mut installer: InputProgramInstaller,
    weapons: Option<Res<assets::PreparedWeapons>>,
    mut echo: ConsoleEcho,
) {
    for cmd in events.read().filter(|cmd| cmd.name == "input_program") {
        let result = (|| -> Result<String, String> {
            let world = &mut authority.as_mut().ok_or("no authority")?.0;
            match cmd
                .args
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
            {
                ["load", path] => {
                    if *role != frame::RuntimeRole::Listen {
                        return Err("input programs currently require a listen host".into());
                    }
                    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                    let (program, actors) = parse_program(&text, local.0)?;
                    installer.install(world, program, actors)?;
                    Ok("input program loaded; exclusive ownership until map reload".into())
                }
                ["status"] => {
                    let run = world.input_program().ok_or("no input program")?;
                    Ok(format!(
                        "input program: issued={} consumed={} total={} complete={} failure={:?}",
                        run.issued,
                        run.receipts.len(),
                        run.program.samples.len(),
                        run.complete(),
                        run.failure
                    ))
                }
                ["dump", path] => {
                    let run = world.input_program().ok_or("no input program")?;
                    let report = actor_report(world, weapons.as_deref(), run);
                    std::fs::write(
                        path,
                        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(format!("input program receipts: {path}"))
                }
                _ => Err("usage: input_program load <path> | dump <path> | status".into()),
            }
        })();
        echo.write(result.unwrap_or_else(|e| format!("input_program: {e}")));
    }
}

fn actor_report(
    world: &sim::SimWorld,
    weapons: Option<&assets::PreparedWeapons>,
    run: &sim::input_program::InputProgramRun,
) -> Value {
    let initial_inventory: Vec<_> = run
                        .initial_player
                        .weapons
                        .iter()
                        .filter(|&&id| id > 0)
                        .map(|&id| json!({"id": id, "name": world.weapon_script_name(id as u32),
                            "fire_raise_ms": weapons.as_ref().map(|w| w.registry().timers_of(id as u32)),
                            "drop_quickdrop_quickraise_ms": weapons.as_ref().map(|w| w.registry().switch_timers_of(id as u32)),
                            "sprint_ms": weapons.as_ref().map(|w| w.registry().sprint_timers_of(id as u32)),
                            "reload_ms": weapons.as_ref().map(|w| w.registry().reload_timers_of(id as u32)) }))
                        .collect();
    let samples: Vec<_> = run.receipts.iter().map(|r| json!({
                        "tick": r.tick.0, "sequence": r.command.sequence.map(|seq| seq.0),
                        "bytes": r.command.command.canonical_bytes().as_slice(),
                        "server_time": r.player.command_time, "origin": r.player.origin,
                        "velocity": r.player.velocity, "angles": r.player.viewangles,
                        "ground_entity": r.player.ground_entity_num, "weapon": r.player.weapon,
                        "weapon_state": r.player.weaponstate_primary,
                        "weapon_anim": r.player.weap_anim, "weapon_time": r.player.weapon_time,
                        "weapon_delay": r.player.weapon_delay, "pm_flags": r.player.pm_flags,
                        "view_height": r.player.view_height_current, "off_hand_index": r.player.off_hand_index
                    })).collect();
    let shots: Vec<_> = run.shots.iter().map(|shot| json!({
                        "id": shot.shot_id.0, "weapon": shot.weapon, "origin": shot.origin,
                        "angles": shot.angles, "owner_velocity": shot.owner_velocity,
                        "cause": shot.fire_cause.map(|c| json!({"client":c.client.0,"life":c.life.0,"command":c.command.0,"ordinal":c.ordinal,"hand":c.hand})),
                        "combat_seed": shot.combat_seed, "spread_degrees": shot.spread_degrees,
                        "ads_frac": shot.ads_frac
                    })).collect();
    let launches: Vec<_> = run.launches.iter().map(|p| json!({
                        "weapon": p.weapon, "origin": p.origin, "velocity": p.velocity,
                        "base": p.pos.tr_base, "delta": p.pos.tr_delta, "spawn_time_ms": p.spawn_time_ms,
                        "cause": p.cause.map(|c| json!({"client":c.client.0,"life":c.life.0,"command":c.command.0,"ordinal":c.ordinal,"hand":c.hand}))
                    })).collect();
    let verdicts: Vec<_> = run
        .shot_verdicts
        .iter()
        .map(|v| {
            json!({
                "shot": v.shot_id.0, "terminal": format!("{:?}", v.terminal), "end": v.end,
                "startsolid": v.startsolid, "geometry": format!("{:?}", v.geometry)
            })
        })
        .collect();
    let report = json!({ "schema_version": 1, "complete": run.complete(),
                        "program_sha256": run.digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
                        "root_seed": world.root_seed(), "content_digest": format!("{:?}", world.content_digest()),
                        "initial_player_debug": format!("{:?}", run.initial_player), "initial_inventory": initial_inventory, "failure": run.failure,
                        "start_tick": run.start_tick.map(|t| t.0), "base_command_time": run.initial_player.command_time,
                        "initial_origin": run.initial_player.origin, "initial_angles": run.initial_player.viewangles,
                        "initial_delta_angles": run.initial_player.delta_angles, "life": run.life.0,
                        "shots": shots, "launches": launches, "shot_verdicts": verdicts,
                        "issued": run.issued, "total": run.program.samples.len(), "receipts": samples });
    let mut report = report;
    report["shot_scenes"] = json!(run.shot_scenes.iter().map(|sample| json!({
        "tick": sample.tick.0, "shot": sample.shot.0,
        "targets": sample.targets.iter().map(|target| json!({
            "client": target.client.0, "origin": target.player.origin,
            "velocity": target.player.velocity, "head": target.head,
            "health": target.player.health, "ground_entity": target.player.ground_entity_num,
            "pm_flags": target.player.pm_flags, "view_height": target.player.view_height_current,
        })).collect::<Vec<_>>()
    })).collect::<Vec<_>>());
    report["actor_name"] = json!(
        world
            .snapshot(sim::Tick(0))
            .meta
            .for_client(run.program.client)
            .map(|meta| {
                let end = meta
                    .name
                    .iter()
                    .position(|&byte| byte == 0)
                    .unwrap_or(meta.name.len());
                String::from_utf8_lossy(&meta.name[..end]).into_owned()
            })
    );
    report["actor_team"] = json!(world.client_state_team(run.program.client));
    report["client"] = json!(run.program.client.0);
    report["schema_version"] = json!(if run.actors.is_empty() { 1 } else { 2 });
    report["ensemble_sha256"] = json!(
        run.ensemble_digest()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    report["actors"] = json!(
        run.actors
            .iter()
            .map(|actor| actor_report(world, weapons, actor))
            .collect::<Vec<_>>()
    );
    report
}

fn integer(value: &Value, key: &str) -> Result<i64, String> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("missing integer {key}"))
}

fn parse_program(
    text: &str,
    client: sim::ClientId,
) -> Result<(InputProgram, Vec<InputProgram>), String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    reject_unknown(
        &value,
        &[
            "schema_version",
            "quantum_ms",
            "first_sequence",
            "samples",
            "actors",
        ],
    )?;
    let version = integer(&value, "schema_version")?;
    let mut actors = Vec::new();
    match version {
        1 if value.get("actors").is_none() => {}
        2 => {
            let rows = value
                .get("actors")
                .and_then(Value::as_array)
                .ok_or("missing actors")?;
            for actor in rows {
                reject_unknown(
                    actor,
                    &["client", "quantum_ms", "first_sequence", "samples"],
                )?;
                let id = u32::try_from(integer(actor, "client")?).map_err(|e| e.to_string())?;
                actors.push(parse_actor(actor, sim::ClientId(id))?);
            }
        }
        _ => return Err("unsupported input program schema or actors field".into()),
    }
    Ok((parse_actor(&value, client)?, actors))
}

fn parse_actor(value: &Value, client: sim::ClientId) -> Result<InputProgram, String> {
    let quantum_ms = i32::try_from(integer(&value, "quantum_ms")?).map_err(|e| e.to_string())?;
    let first_sequence =
        u32::try_from(integer(&value, "first_sequence")?).map_err(|e| e.to_string())?;
    let rows = value
        .get("samples")
        .and_then(Value::as_array)
        .ok_or("missing samples")?;
    let mut samples = Vec::with_capacity(rows.len());
    for row in rows {
        reject_unknown(
            row,
            &[
                "time_ms",
                "buttons",
                "angles",
                "weapon",
                "weapon_mapped",
                "off_hand_index",
                "forwardmove",
                "rightmove",
            ],
        )?;
        let mut cmd = net::idle_usercmd(0);
        cmd.server_time = i32::try_from(integer(row, "time_ms")?).map_err(|e| e.to_string())?;
        cmd.buttons = u32::try_from(integer(row, "buttons")?).map_err(|e| e.to_string())?;
        cmd.weapon = u16::try_from(integer(row, "weapon")?).map_err(|e| e.to_string())?;
        cmd.weapon_mapped =
            u16::try_from(integer(row, "weapon_mapped")?).map_err(|e| e.to_string())?;
        cmd.off_hand_index =
            u16::try_from(integer(row, "off_hand_index")?).map_err(|e| e.to_string())?;
        cmd.forwardmove = i8::try_from(integer(row, "forwardmove")?).map_err(|e| e.to_string())?;
        cmd.rightmove = i8::try_from(integer(row, "rightmove")?).map_err(|e| e.to_string())?;
        let angles = row
            .get("angles")
            .and_then(Value::as_array)
            .filter(|a| a.len() == 3)
            .ok_or("angles must contain three integer command angles")?;
        for (out, angle) in cmd.angles.iter_mut().zip(angles) {
            *out = i32::try_from(angle.as_i64().ok_or("invalid command angle")?)
                .map_err(|e| e.to_string())?;
        }
        samples.push(cmd);
    }
    Ok(InputProgram {
        client,
        quantum_ms,
        first_sequence: sim::CommandSequence(first_sequence),
        samples,
    })
}

fn reject_unknown(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let object = value.as_object().ok_or("expected an object")?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unsupported input program field {key}"));
        }
    }
    Ok(())
}
