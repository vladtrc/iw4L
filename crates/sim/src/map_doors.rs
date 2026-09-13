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
            .is_some_and(|at| now.saturating_sub(at) < 28_000)
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
    let t = (elapsed as f32 / 1000.0).clamp(0.0, 8.0);
    if t < accel {
        t * t / (8.0 * accel)
    } else {
        1.0 - (8.0 - t) * (8.0 - t) / (8.0 * (8.0 - accel))
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
            doors.startup_at = Some(now + 300);
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
    }
    doors.held = held;
    if let Some(at) = doors.started_at {
        let elapsed = now.saturating_sub(at);
        while doors.alarm_count < 5 && elapsed >= 500 + u32::from(doors.alarm_count) * 2000 {
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
                let accel = if (i == 0) != doors.open { 4.8 } else { 5.6 };
                let progress = fraction(elapsed, accel);
                let roll = (if i == 0 { 123.0 } else { -123.0 })
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
            if elapsed >= 8000 {
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
    }
    world.map_doors = Some(doors);
}
