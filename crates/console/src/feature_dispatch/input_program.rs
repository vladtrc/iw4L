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
                ["prepare_actor", client, path] => {
                    if *role != frame::RuntimeRole::Listen {
                        return Err("actor preparation requires a listen host".into());
                    }
                    let client = sim::ClientId(client.parse::<u32>().map_err(|e| e.to_string())?);
                    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                    reject_unknown(&value, &["name", "rank", "prestige", "title", "icon", "score", "kills", "deaths", "streak"])?;
                    let p = sim::input_program::InputActorPreparation {
                        name: value["name"].as_str().ok_or("missing actor name")?.to_owned(),
                        rank: integer(&value,"rank")?.try_into().map_err(|_| "invalid rank")?,
                        prestige: integer(&value,"prestige")?.try_into().map_err(|_| "invalid prestige")?,
                        title: integer(&value,"title")?.try_into().map_err(|_| "invalid title")?,
                        icon: integer(&value,"icon")?.try_into().map_err(|_| "invalid icon")?,
                        score: integer(&value,"score")?.try_into().map_err(|_| "invalid score")?,
                        kills: integer(&value,"kills")?.try_into().map_err(|_| "invalid kills")?,
                        deaths: integer(&value,"deaths")?.try_into().map_err(|_| "invalid deaths")?,
                        streak: parse_streak(&value["streak"])? ,
                    };
                    world.prepare_input_actor(client, &p)?;
                    Ok(format!("input actor {} prepared", client.0))
                }
                ["prepare_team_scores", allies, axis] => {
                    if *role != frame::RuntimeRole::Listen {
                        return Err("score preparation requires a listen host".into());
                    }
                    world.prepare_input_team_scores(
                        allies.parse::<i32>().map_err(|e| e.to_string())?,
                        axis.parse::<i32>().map_err(|e| e.to_string())?,
                    )?;
                    Ok(format!("input team scores prepared: allies={allies} axis={axis}"))
                }
                ["prepare_health", client, health] => {
                    if *role != frame::RuntimeRole::Listen {
                        return Err("actor preparation requires a listen host".into());
                    }
                    let client = sim::ClientId(client.parse::<u32>().map_err(|e| e.to_string())?);
                    let health = health.parse::<i32>().map_err(|e| e.to_string())?;
                    world.prepare_input_actor_health(client, health)?;
                    Ok(format!("input actor {} prepared with {health} health", client.0))
                }
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
                _ => Err("usage: input_program prepare_actor <client> <json> | prepare_team_scores <allies> <axis> | prepare_health <client> <health> | load <path> | dump <path> | status".into()),
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
                        "source_bytes": r.source_command.command.canonical_bytes().as_slice(),
                        "server_time": r.player.command_time, "origin": r.player.origin,
                        "world": sim::world_graph::world_at(r.player.origin),
                        "velocity": r.player.velocity, "angles": r.player.viewangles,
                        "ground_entity": r.player.ground_entity_num, "weapon": r.player.weapon,
                        "weapon_state": r.player.weaponstate_primary,
                        "weapon_anim": r.player.weap_anim, "weapon_time": r.player.weapon_time,
                        "weapon_delay": r.player.weapon_delay, "pm_flags": r.player.pm_flags,
                        "health": r.player.health, "inventory": r.player.weapons,
                        "weapon_data": r.player.weapon_data.as_slice(),
                        "ads_frac": r.player.f_weapon_pos_frac,
                        "view_height": r.player.view_height_current, "off_hand_index": r.player.off_hand_index,
                        "cursor_hint":r.player.cursor_hint,"cursor_hint_entity":r.player.cursor_hint_ent_index
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
    let meta = &run.initial_meta;
    report["initial_match"] = json!({"score":meta.score,"kills":meta.kills,"deaths":meta.deaths,
        "rank":meta.rank,"prestige":meta.prestige,"title":meta.player_card_title,"icon":meta.player_card_icon,
        "name":String::from_utf8_lossy(&meta.name).trim_end_matches('\0')});
    let streak = world.input_actor_streak(run.program.client);
    report["final_streak"] =
        json!({"count":streak.count,"last_earned":streak.last_earned,"owned":streak.owned});
    let streak = &run.initial_streak;
    report["initial_streak"] =
        json!({"count":streak.count,"last_earned":streak.last_earned,"owned":streak.owned});
    report["match_states"] = json!(run.match_states.iter().map(|state|json!({"tick":state.tick.0,"ended":state.ended,
        "score_limit":state.score_limit,"scores":state.scores.iter().map(|(id,score,kills,deaths)|json!({"client":id.0,"score":score,"kills":kills,"deaths":deaths})).collect::<Vec<_>>() })).collect::<Vec<_>>());
    report["scoreboard_window_ms"] = json!(run.program.scoreboard_window_ms);
    report["scoreboard_samples"] = json!(
        run.scoreboard_samples
            .iter()
            .map(|(tick, origin)| json!({"tick":tick.0,"origin":origin}))
            .collect::<Vec<_>>()
    );
    report["final_killcams"] = json!(run.final_killcams.iter().map(|cam|
        json!({"start_tick":cam.start.0,"last_tick":cam.last.0,"closed_tick":cam.closed.map(|t|t.0),"client":cam.client.0,"target":cam.target,"length_ms":cam.length_ms})).collect::<Vec<_>>());
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
    report["projectile_impacts"] = json!(
        world
            .projectile_impact_log()
            .iter()
            .filter(
                |(tick, impact)| run.start_tick.is_some_and(|start| *tick >= start)
                    && impact.owner == run.program.client
            )
            .map(
                |(tick, impact)| json!({"tick":tick.0,"id":impact.id.0,"weapon":impact.weapon,
            "origin":impact.origin,"geometry":format!("{:?}",impact.geometry),
            "terminal":format!("{:?}",impact.terminal),"amount":impact.amount})
            )
            .collect::<Vec<_>>()
    );
    report["death_tick"] = json!(run.death_tick.map(|t| t.0));
    report["future_weapons"] = json!(run.program.future_weapons);
    report["allow_menu_responses"] = json!(run.program.allow_menu_responses);
    report["menu_responses"] = json!(
        run.menu_responses
            .iter()
            .map(|(tick, menu, response)| json!({"tick":tick.0,"menu":menu,"response":response}))
            .collect::<Vec<_>>()
    );
    report["weapon_facts"] = json!(run.initial_player.weapons.iter().copied()
        .filter(|&id|id>0).map(|id|id as u32)
        .chain(run.program.future_weapons.iter().map(|&id|u32::from(id)))
        .filter_map(|id|world.weapon_combat_facts(id).map(|f|json!({
            "id":id,"damage":f.damage,"min_damage":f.min_damage,"location_damage_mult":f.location_damage,
            "fire_ms":f.fire_time_ms,"fire_delay_ms":f.fire_delay_ms,
            "drop_ms":f.drop_time_ms,"raise_ms":f.raise_time_ms,"first_raise_ms":f.first_raise_time_ms,
            "reload_ms":f.reload_time_ms,"reload_add_ms":f.reload_add_time_ms,
            "rechamber_ms":f.rechamber_time_ms,"bolt_delay_ms":f.rechamber_bolt_delay_ms,
            "bolt_action":f.bolt_action,"clip_size":f.clip_size,
            "clip_only":f.clip_only,"start_ammo":f.start_ammo,"max_ammo":f.max_ammo
        }))).collect::<Vec<_>>());
    report["missile_facts"] = json!(
        run.initial_player
            .weapons
            .iter()
            .copied()
            .filter(|&id| id > 0)
            .filter_map(|id| world
                .content()
                .missile_launch_facts(id as u32)
                .map(|f| json!({
                    "id":id,"speed":f.projectile_speed,"guidance":f.missile_guidance
                })))
            .collect::<Vec<_>>()
    );
    report["events"] = json!(run.events.iter().map(|record| {
        let data = match record.event {
            sim::SimEvent::Died { victim, attacker, weapon, source, .. } =>
                json!({"kind":"died", "victim":victim.0, "attacker":attacker.map(|a|a.0), "weapon":weapon, "source":format!("{source:?}")}),
            sim::SimEvent::ScoreChanged { client, score, kills, deaths } =>
                json!({"kind":"score", "client":client.0, "score":score, "kills":kills, "deaths":deaths}),
            sim::SimEvent::MatchEnded { reason } => json!({"kind":"match_ended", "reason":format!("{reason:?}")}),
            _ => json!({"kind":"other", "event":format!("{:?}",record.event)}),
        };
        json!({"tick":record.tick.0, "sequence":record.sequence.0, "data":data})
    }).collect::<Vec<_>>());
    report["pickups"] = json!(
        run.pickups
            .iter()
            .map(|(tick, item)| json!({
                "tick":tick.0, "picker":item.picker, "weapon":item.weapon,
                "from_entnum":item.from_entnum, "swapped_entnum":item.swapped_entnum,
                "clip_r":item.clip_r, "clip_l":item.clip_l, "stock":item.stock,
            }))
            .collect::<Vec<_>>()
    );
    let snapshot = world.snapshot(sim::Tick(0));
    report["final_match"] = json!({"phase":format!("{:?}",snapshot.meta.phase),
        "mode":format!("{:?}",snapshot.meta.kind), "score_limit":snapshot.meta.score_limit,
        "scores":snapshot.meta.clients.iter().map(|(id,meta)| json!({"client":id.0,
            "kills":meta.kills,"deaths":meta.deaths,"score":meta.score})).collect::<Vec<_>>()});
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

fn parse_streak(
    value: &Value,
) -> Result<Option<sim::input_program::InputStreakPreparation>, String> {
    if value.is_null() {
        return Ok(None);
    }
    reject_unknown(value, &["count", "loadout", "last_earned", "owned"])?;
    let strings = |key: &str| -> Result<Vec<String>, String> {
        value[key]
            .as_array()
            .ok_or_else(|| format!("missing array {key}"))?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("invalid {key}"))
            })
            .collect()
    };
    Ok(Some(sim::input_program::InputStreakPreparation {
        count: integer(value, "count")?
            .try_into()
            .map_err(|_| "invalid streak count")?,
        loadout: strings("loadout")?
            .try_into()
            .map_err(|_| "three streaks required")?,
        last_earned: if value["last_earned"].is_null() {
            None
        } else {
            Some(
                value["last_earned"]
                    .as_str()
                    .ok_or("invalid last earned streak")?
                    .to_owned(),
            )
        },
        owned: if value.get("owned").is_some() {
            strings("owned")?
        } else {
            vec![]
        },
    }))
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
            "future_weapons",
            "stop_on_death",
            "allow_menu_responses",
            "scoreboard_window_ms",
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
                    &[
                        "client",
                        "quantum_ms",
                        "first_sequence",
                        "samples",
                        "future_weapons",
                        "stop_on_death",
                        "allow_menu_responses",
                        "scoreboard_window_ms",
                    ],
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
    let quantum_ms = i32::try_from(integer(value, "quantum_ms")?).map_err(|e| e.to_string())?;
    let first_sequence =
        u32::try_from(integer(value, "first_sequence")?).map_err(|e| e.to_string())?;
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
                "remote_control",
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
        if let Some(remote) = row.get("remote_control") {
            let values = remote
                .as_array()
                .filter(|a| a.len() == 2)
                .ok_or("remote_control must contain two bytes")?;
            for (out, value) in cmd.remote_control.iter_mut().zip(values) {
                *out = u8::try_from(value.as_u64().ok_or("invalid remote control byte")?)
                    .map_err(|e| e.to_string())?;
            }
        }
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
        scoreboard_window_ms: value
            .get("scoreboard_window_ms")
            .filter(|v| !v.is_null())
            .map(|v| {
                let a = v
                    .as_array()
                    .filter(|a| a.len() == 2)
                    .ok_or("scoreboard_window_ms needs two integers")?;
                Ok::<_, String>([
                    i32::try_from(a[0].as_i64().ok_or("invalid scoreboard start")?)
                        .map_err(|e| e.to_string())?,
                    i32::try_from(a[1].as_i64().ok_or("invalid scoreboard end")?)
                        .map_err(|e| e.to_string())?,
                ])
            })
            .transpose()?,
        client,
        quantum_ms,
        first_sequence: sim::CommandSequence(first_sequence),
        samples,
        future_weapons: value
            .get("future_weapons")
            .map(|rows| {
                rows.as_array()
                    .ok_or_else(|| "future_weapons must be an array".to_owned())?
                    .iter()
                    .map(|row| {
                        row.as_u64()
                            .and_then(|v| u16::try_from(v).ok())
                            .ok_or_else(|| "invalid future weapon".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_else(Vec::new),
        allow_menu_responses: value
            .get("allow_menu_responses")
            .map(|v| v.as_bool().ok_or("allow_menu_responses must be a boolean"))
            .transpose()?
            .unwrap_or(false),
        stop_on_death: value
            .get("stop_on_death")
            .map(|v| v.as_bool().ok_or("stop_on_death must be a boolean"))
            .transpose()?
            .unwrap_or(false),
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
