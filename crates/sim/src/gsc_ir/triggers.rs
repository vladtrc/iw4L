use super::runtime::raise;
use super::*;
use crate::bullet_collision::{PLAYER_MAXS, PLAYER_MINS};
use crate::frame::FrameWorld;
use bevy_ecs::prelude::World;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fires {
    Touch,
    Once,
    Use,
}

fn fires(classname: &str) -> Option<Fires> {
    match classname {
        "trigger_multiple" | "trigger_radius" | "trigger_disk" => Some(Fires::Touch),
        "trigger_once" => Some(Fires::Once),
        "trigger_use" | "trigger_use_touch" => Some(Fires::Use),
        _ => None,
    }
}

fn volume<'f>(runtime: &mut Runtime, frame: &'f FrameWorld, object: u64) -> Option<Volume<'f>> {
    let entity = runtime.entities.get(&object)?;
    let (cylinder, brush, trigger_model) = (entity.cylinder, entity.brush, entity.trigger_model);
    let origin = match runtime.object_field(object, "origin") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    if let Some((radius, height)) = cylinder {
        return Some(Volume::Cylinder {
            origin,
            radius,
            height,
        });
    }
    if let Some(n) = trigger_model {
        let hulls = frame.clip_cmodels().triggers.get(n as usize)?;
        return (!hulls.is_empty()).then_some(Volume::Hulls { origin, hulls });
    }
    let model = frame.clip_cmodels().models.get(brush? as usize)?;
    let first = model.first_brush as usize;
    let ids = frame
        .clip_bsp()
        .leafbrushes
        .get(first..first + usize::from(model.num_brushes))
        .unwrap_or(&[]);
    if !ids.is_empty() {
        return Some(Volume::Brushes {
            origin,
            ids,
            brushes: frame.clip_brushes(),
        });
    }
    Some(Volume::Box {
        mins: std::array::from_fn(|i| origin[i] + model.mins[i]),
        maxs: std::array::from_fn(|i| origin[i] + model.maxs[i]),
    })
}

#[derive(Clone, Copy, Debug)]
enum Volume<'f> {
    Cylinder {
        origin: [f32; 3],
        radius: f32,
        height: f32,
    },
    Box {
        mins: [f32; 3],
        maxs: [f32; 3],
    },
    Brushes {
        origin: [f32; 3],
        ids: &'f [u16],
        brushes: &'f [crate::SimBrush],
    },
    Hulls {
        origin: [f32; 3],
        // Hull planes are relative to this origin.
        hulls: &'f [crate::SimTriggerHull],
    },
}

impl Volume<'_> {
    fn touches(&self, mins: [f32; 3], maxs: [f32; 3]) -> bool {
        match *self {
            Volume::Brushes {
                origin,
                ids,
                brushes,
            } => {
                let mid: [f32; 3] = std::array::from_fn(|i| (mins[i] + maxs[i]) * 0.5 - origin[i]);
                let half: [f32; 3] = std::array::from_fn(|i| (maxs[i] - mins[i]) * 0.5);
                ids.iter()
                    .filter_map(|&id| brushes.get(usize::from(id)))
                    .any(|brush| {
                        brush.planes.iter().all(|p| {
                            let reach =
                                p[0].abs() * half[0] + p[1].abs() * half[1] + p[2].abs() * half[2];
                            p[0] * mid[0] + p[1] * mid[1] + p[2] * mid[2] - p[3] <= reach
                        })
                    })
            }
            Volume::Cylinder {
                origin,
                radius,
                height,
            } => {
                let mid: [f32; 3] = std::array::from_fn(|i| (mins[i] + maxs[i]) * 0.5);
                let half: [f32; 3] = std::array::from_fn(|i| (maxs[i] - mins[i]) * 0.5);
                let (dx, dy) = (origin[0] - mid[0], origin[1] - mid[1]);
                let reach = radius + half[0];
                (origin[2] + height * 0.5 - mid[2]).abs() < height * 0.5 + half[2]
                    && dx * dx + dy * dy < reach * reach
            }
            Volume::Box { mins: lo, maxs: hi } => {
                (0..3).all(|i| mins[i] <= hi[i] && maxs[i] >= lo[i])
            }
            Volume::Hulls { origin, hulls } => {
                let mid: [f32; 3] = std::array::from_fn(|i| (mins[i] + maxs[i]) * 0.5 - origin[i]);
                let half: [f32; 3] = std::array::from_fn(|i| (maxs[i] - mins[i]) * 0.5);
                hulls.iter().any(|hull| {
                    (0..3).all(|i| (mid[i] - hull.mid[i]).abs() <= half[i] + hull.half[i])
                        && hull.slabs.iter().all(|&(dir, at, width)| {
                            let reach: f32 = (0..3).map(|i| dir[i].abs() * half[i]).sum();
                            let along: f32 = (0..3).map(|i| dir[i] * mid[i]).sum();
                            (along - at).abs() <= width + reach
                        })
                })
            }
        }
    }
}

const LOOK_AT_REACH: f32 = 128.0;

fn looks_into(frame: &FrameWorld, client: u32, volume: &Volume<'_>) -> bool {
    let Some(ps) = frame.player(crate::ClientId(client)) else {
        return false;
    };
    let eye = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let (forward, _, _) = math_iw4::angle_vectors(ps.viewangles);
    (0..=16).any(|step| {
        let reach = LOOK_AT_REACH * step as f32 / 16.0;
        let at = std::array::from_fn(|i| eye[i] + forward[i] * reach);
        volume.touches(at, at)
    })
}

fn toucher(runtime: &mut Runtime, frame: &FrameWorld, object: u64) -> ([f32; 3], [f32; 3]) {
    if let Some(client) = runtime.player_client(object)
        && let Some(ps) = frame.player(crate::ClientId(client))
    {
        return (
            std::array::from_fn(|i| ps.origin[i] + PLAYER_MINS[i]),
            std::array::from_fn(|i| ps.origin[i] + PLAYER_MAXS[i]),
        );
    }
    let origin = match runtime.object_field(object, "origin") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    (origin, origin)
}

pub(super) fn is_touching(world: &mut World, a: u64, b: u64) -> bool {
    let mut runtime = std::mem::take(&mut *world.resource_mut::<Runtime>());
    let touching = {
        let frame = FrameWorld::from_world(world);
        [(a, b), (b, a)].into_iter().find_map(|(trigger, other)| {
            let volume = volume(&mut runtime, &frame, trigger)?;
            let (mins, maxs) = toucher(&mut runtime, &frame, other);
            Some(volume.touches(mins, maxs))
        })
    };
    *world.resource_mut::<Runtime>() = runtime;
    touching.unwrap_or(false)
}

pub(crate) fn dispatch_triggers(world: &mut World) {
    let mut runtime = std::mem::take(&mut *world.resource_mut::<Runtime>());
    let mut raised = Vec::new();
    {
        let mut frame = FrameWorld::from_world(world);
        let mut players: Vec<(u32, u64, bool)> = Vec::new();
        for (client, slot) in &runtime.players {
            let id = crate::ClientId(*client);
            if !frame
                .client_meta(id)
                .is_some_and(|m| m.lifecycle == crate::ClientLifecycle::Alive)
            {
                continue;
            }
            let held = crate::script_player::buttons(&mut frame, id)
                & (playerstate_iw4::buttons::USE | playerstate_iw4::buttons::USE_RELOAD)
                != 0;
            players.push((*client, slot.object, held));
        }
        let pressed: Vec<(u32, u64)> = players
            .iter()
            .filter(|(client, _, held)| *held && !runtime.use_held.contains(client))
            .map(|(client, object, _)| (*client, *object))
            .collect();
        runtime.use_held = players
            .iter()
            .filter(|(_, _, held)| *held)
            .map(|(client, _, _)| *client)
            .collect();
        for (client, player) in &pressed {
            if let Some(usable) = runtime.use_selected.get(client)
                && runtime.entities.contains_key(usable)
            {
                raised.push((*usable, *player));
            }
        }
        let triggers: Vec<(u64, Fires)> = runtime
            .entities
            .iter()
            .filter(|(object, _)| !runtime.fired_once.contains(object))
            .filter_map(|(object, e)| Some((*object, fires(&e.classname)?)))
            .collect();
        for (trigger, kind) in triggers {
            let Some(volume) = volume(&mut runtime, &frame, trigger) else {
                continue;
            };
            let candidates: Vec<(u32, u64)> = match kind {
                Fires::Use => pressed.clone(),
                Fires::Touch | Fires::Once => players
                    .iter()
                    .map(|(client, object, _)| (*client, *object))
                    .collect(),
            };
            let look_at = kind == Fires::Use && runtime.require_look_at.contains(&trigger);
            for (client, player) in candidates {
                let (mins, maxs) = toucher(&mut runtime, &frame, player);
                if !volume.touches(mins, maxs) || look_at && !looks_into(&frame, client, &volume) {
                    continue;
                }
                raised.push((trigger, player));
                if kind == Fires::Once {
                    runtime.fired_once.insert(trigger);
                    break;
                }
            }
        }
    }
    *world.resource_mut::<Runtime>() = runtime;
    for (trigger, player) in raised {
        raise(
            world,
            Value::Object(trigger),
            "trigger",
            vec![Value::Object(player)],
        );
    }
}
