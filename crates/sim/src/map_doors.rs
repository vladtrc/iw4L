use crate::frame::FrameWorld;
use crate::{ClientId, ClientLifecycle, EventAudience, MatchPhase, ScriptModelId, Tick};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DoorSwitch {
    pub cmodel: u32,
    pub origin: [f32; 3],
    pub half: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DoorLeaf {
    pub cmodel: u32,
    pub model_angles: [f32; 3],
    pub model: u32,
    pub brush: u32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapDoors {
    pub switches: [DoorSwitch; 2],
    pub leaves: [DoorLeaf; 2],
    pub sound_origin: [f32; 3],
    pub startup_at: Option<u32>,
    pub started_at: Option<u32>,
    pub open: bool,
    pub completed: bool,
    pub alarm_count: u8,
    pub activations: u32,

    pub hints: Vec<ClientId>,

    pub held: Vec<ClientId>,
}

impl MapDoors {
    pub fn unavailable(&self, now: u32) -> bool {
        self.started_at
            .is_some_and(|at| gamemode_iw4::radiation_unavailable(now.saturating_sub(at)))
    }
}

fn can_use(world: &FrameWorld, ps: &playerstate_iw4::PlayerState, switch: DoorSwitch) -> bool {
    let eye = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let delta = std::array::from_fn::<_, 3, _>(|i| switch.origin[i] - eye[i]);
    let distance2: f32 = delta.iter().map(|x| x * x).sum();
    if distance2 > 128.0 * 128.0 {
        return false;
    }
    let pitch = ps.viewangles[0].to_radians();
    let yaw = ps.viewangles[1].to_radians();
    let forward = [
        pitch.cos() * yaw.cos(),
        pitch.cos() * yaw.sin(),
        -pitch.sin(),
    ];
    let dot: f32 = (0..3).map(|i| forward[i] * delta[i]).sum();
    let half = switch.half.into_iter().fold(0.0_f32, f32::max);
    if dot < 0.0
        || (distance2 > 1.0 && dot * dot / distance2 < 1.0 / (half * half / distance2 + 1.0))
    {
        return false;
    }
    world
        .trace_world(eye, switch.origin, [0.0; 3], [0.0; 3], 0x11)
        .fraction
        >= 1.0
}

fn sound(world: &mut FrameWorld, tick: Tick, origin: [f32; 3], alias: &str) {
    let event_parm = i32::from(world.sound_alias_index(alias));
    world.push_entity_event(
        tick,
        EventAudience::All,
        entity_iw4::EntityEventKind::SOUND_ALIAS,
        crate::EntityEventPayload {
            number: 2046,
            event_parm,
            origin,
            ..Default::default()
        },
    );
}

fn fraction(elapsed: u32, accel: f32) -> f32 {
    let t =
        (elapsed as f32 / 1000.0).clamp(0.0, gamemode_iw4::RADIATION_DOOR_TIME_MS as f32 / 1000.0);
    let duration = gamemode_iw4::RADIATION_DOOR_TIME_MS as f32 / 1000.0;
    if t < accel {
        t * t / (duration * accel)
    } else {
        1.0 - (duration - t) * (duration - t) / (duration * (duration - accel))
    }
}

pub(crate) fn advance(
    world: &mut FrameWorld,
    tick: Tick,
    cmds: &[(ClientId, playerstate_iw4::UserCmd)],
) {
    let Some(mut doors) = world.map_doors.take() else {
        return;
    };
    let now = tick.0.saturating_mul(50);
    doors.hints.clear();
    let playing = world.phase() == MatchPhase::Playing;
    if playing {
        for id in world.client_ids_sorted() {
            if !world
                .client_meta(id)
                .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                continue;
            }
            if let Some(ps) = world.player(id)
                && doors.switches.iter().any(|s| can_use(world, ps, *s))
            {
                doors.hints.push(id);
            }
        }
        if doors.startup_at.is_none() {
            doors.startup_at = Some(now + gamemode_iw4::RADIATION_STARTUP_DELAY_MS);
        }
    }
    let held: Vec<_> = cmds
        .iter()
        .filter(|(_, cmd)| {
            cmd.buttons & (playerstate_iw4::buttons::USE | playerstate_iw4::buttons::USE_RELOAD)
                != 0
        })
        .map(|(id, _)| *id)
        .collect();
    let user = held
        .iter()
        .copied()
        .find(|id| doors.hints.contains(id) && !doors.held.contains(id));
    let startup = doors.started_at.is_none() && doors.startup_at.is_some_and(|at| now >= at);
    if playing && (startup || (user.is_some() && !doors.unavailable(now))) {
        doors.started_at = Some(now);
        doors.completed = false;
        doors.alarm_count = 0;
        doors.activations += 1;
        if !startup {
            let origin = world
                .player(user.expect("user activation"))
                .expect("alive user")
                .origin;
            sound(world, tick, origin, "evt_hydraulic_switch");
        }
        sound(world, tick, doors.leaves[0].origin, "evt_hydraulic_start");
        world
            .script_gaps_mut()
            .raise(gamemode_iw4::ScriptGapCause::RadiationSwitchExploder);
    }
    doors.held = held;
    if let Some(at) = doors.started_at {
        let elapsed = now.saturating_sub(at);
        if gamemode_iw4::radiation_kill_edge_active(doors.open, elapsed, doors.completed)
            && elapsed.saturating_sub(crate::MATCH_TICK_MS)
                < gamemode_iw4::RADIATION_KILL_EDGE_DELAY_MS
        {
            world
                .script_gaps_mut()
                .raise(gamemode_iw4::ScriptGapCause::RadiationDoorKillEdge);
        }
        while gamemode_iw4::radiation_alarm_due(doors.alarm_count, elapsed) {
            for origin in [
                [-664.0, 110.0, 436.0],
                [-664.0, -72.0, 436.0],
                [-666.0, -602.0, 436.0],
                [-666.0, 660.0, 444.0],
            ] {
                sound(world, tick, origin, "amb_alarm_buzz");
            }
            doors.alarm_count += 1;
        }
        if !doors.completed {
            for (i, leaf) in doors.leaves.iter().enumerate() {
                let accel = gamemode_iw4::radiation_door_accel_s(i, !doors.open);
                let progress = fraction(elapsed, accel);
                let roll = gamemode_iw4::radiation_leaf_roll_deg(i)
                    * if doors.open { 1.0 - progress } else { progress };
                let mut angles = leaf.angles;
                angles[2] += roll;
                let parent = glam::Quat::from_rotation_x(roll.to_radians());
                let child = glam::Quat::from_euler(
                    glam::EulerRot::ZYX,
                    leaf.model_angles[1].to_radians(),
                    leaf.model_angles[0].to_radians(),
                    leaf.model_angles[2].to_radians(),
                );
                let (yaw, pitch, r) = (parent * child).to_euler(glam::EulerRot::ZYX);
                let model_angles = [pitch.to_degrees(), yaw.to_degrees(), r.to_degrees()];
                for ordinal in [leaf.model, leaf.brush] {
                    let number = world
                        .gentity_number(ScriptModelId::from_authored_source_ordinal(ordinal))
                        .expect("Radiation operation lost its installed door mover");
                    let mover = world
                        .script_mover_mut_by_number(number)
                        .expect("installed door mover");
                    mover.state.apos_tr_base = if ordinal == leaf.model {
                        model_angles
                    } else {
                        angles
                    };
                }

                for cap in world.entity_collision_capabilities_mut() {
                    for brush in &mut cap.linked_brushes {
                        if brush.cmodel_handle == leaf.cmodel {
                            brush.angles = angles;
                        }
                    }
                }
            }
            if elapsed >= gamemode_iw4::RADIATION_DOOR_TIME_MS {
                doors.open = !doors.open;
                doors.completed = true;
                sound(
                    world,
                    tick,
                    doors.sound_origin,
                    if doors.open {
                        "evt_hydraulic_open"
                    } else {
                        "evt_hydraulic_close"
                    },
                );
            }
        }
        sweep_clear(world, tick, &doors, elapsed);
    }
    world.map_doors = Some(doors);
}

fn sweep_clear(world: &mut FrameWorld, tick: Tick, doors: &MapDoors, elapsed: u32) {
    if gamemode_iw4::radiation_first_drop_pulse(elapsed, doors.completed) {
        world
            .script_gaps_mut()
            .raise(gamemode_iw4::ScriptGapCause::RadiationDoorDropToGround);
    }
    if !gamemode_iw4::radiation_destroy_due(elapsed, doors.completed) {
        return;
    }
    let bounds: Vec<_> = doors
        .leaves
        .iter()
        .filter_map(|leaf| {
            let cmodel = world.clip_cmodels().models.get(leaf.cmodel as usize)?;
            Some((leaf.origin, cmodel.mins, cmodel.maxs))
        })
        .collect();
    if bounds.is_empty() {
        return;
    }
    let mut rows = Vec::new();
    world.visit_projectiles(|projectile| {
        rows.push((projectile.entnum, projectile.origin, projectile.weapon));
    });
    for (entnum, origin, weapon) in rows {
        if !gamemode_iw4::is_weapon_equipment(world.weapon_script_name(weapon)) {
            continue;
        }
        if !bounds.iter().any(|(door, mins, maxs)| {
            gamemode_iw4::radiation_touching_door(origin, *door, *mins, *maxs)
        }) {
            continue;
        }
        if let Some(projectile) = world.projectile_mut_by_number(entnum) {
            projectile.detonate_at_ms = Some(crate::level_time_ms(tick));
        }
    }
}
