use super::args::{float, int, optional, string, vector};
use super::natives::player::player;
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{Arc, NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;
use playerstate_iw4::{PlayerState, buttons, weap_flags};

pub(crate) mod campaign;

mod pof {
    pub const THERMAL_VISION: u32 = 0x8;
    pub const THERMAL_VISION_OVERLAY_FOF: u32 = 0x10;
    pub const REMOTE_CAMERA_SOUNDS: u32 = 0x20;
    pub const ALT_SCENE_REAR_VIEW: u32 = 0x40;
    pub const EMP_JAMMED: u32 = 0x400;
    pub const AC130: u32 = playerstate_iw4::other_flags::AC130;
}

const POINT_LOCK: u8 = 0x40;
const LOCKING: u8 = 1;
const LOCKED: u8 = 2;
const TOP: u8 = 4;
const DIRECT: u8 = 8;
const TOO_CLOSE: u8 = 16;
const NO_CLEARANCE: u8 = 32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MiniMap {
    upper_left: [f32; 2],
    north: [f32; 2],
    size: [f32; 2],
}

fn with_player<T>(
    world: &mut World,
    receiver: &Value,
    edit: impl FnOnce(&mut PlayerState) -> Result<T, String>,
) -> Result<T, String> {
    let client = player(world, receiver)?;
    let mut frame = FrameWorld::from_world(world);
    let ps = frame
        .player_mut(ClientId(client))
        .ok_or("player has disconnected")?;
    edit(ps)
}

fn other_flag(world: &mut World, receiver: &Value, flag: u32, on: bool) -> Result<Value, String> {
    with_player(world, receiver, |ps| {
        if on {
            ps.other_flags |= flag;
        } else {
            ps.other_flags &= !flag;
        }
        Ok(Value::Undefined)
    })
}

fn truthy(args: &[Value], at: usize) -> Result<bool, String> {
    Ok(optional(args, at, int)?.unwrap_or(1) != 0)
}

fn edit_lock(
    world: &mut World,
    receiver: &Value,
    edit: impl FnOnce(&mut crate::WeaponLock, &PlayerState, u32),
) -> Result<u32, String> {
    let client = player(world, receiver)?;
    let mut frame = FrameWorld::from_world(world);
    let id = ClientId(client);
    let ps = *frame.player(id).ok_or("player has disconnected")?;
    let meta = frame.client_meta_mut(id);
    let life = meta.life_sequence.0;
    if meta.weapon_lock.weapon != ps.weapon || meta.weapon_lock.life != life {
        meta.weapon_lock = crate::WeaponLock {
            weapon: ps.weapon,
            life,
            ..Default::default()
        };
    }
    edit(&mut meta.weapon_lock, &ps, life);
    Ok(client)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "setspreadoverride", |world, receiver, args| {
        let spread = int(args, 0)?;
        if !(1..64).contains(&spread) {
            return Err(format!(
                "setspreadoverride: spread must be between 1 and 63, not {spread}"
            ));
        }
        with_player(world, receiver, |ps| {
            ps.spread_override = spread;
            ps.spread_override_state = 2;
            Ok(Value::Undefined)
        })
    });
    registry.register(Method, "resetspreadoverride", |world, receiver, _| {
        with_player(world, receiver, |ps| {
            ps.spread_override_state = 1;
            ps.aim_spread_scale = 255.0;
            Ok(Value::Undefined)
        })
    });
    registry.register(Method, "player_recoilscaleon", |world, receiver, args| {
        let scale = int(args, 0)?.clamp(0, 100);
        with_player(world, receiver, |ps| {
            ps.recoil_scale = scale;
            ps.weap_flags |= weap_flags::RECOIL_SCALE;
            Ok(Value::Undefined)
        })
    });
    registry.register(Method, "player_recoilscaleoff", |world, receiver, _| {
        with_player(world, receiver, |ps| {
            ps.weap_flags &= !weap_flags::RECOIL_SCALE;
            Ok(Value::Undefined)
        })
    });
    registry.register(Method, "viewkick", |world, receiver, args| {
        let force = int(args, 0)?;
        if force < 0 {
            return Err(format!("viewkick: damage {force} < 0"));
        }
        let origin = vector(args, 1)?;
        with_player(world, receiver, |ps| {
            let blood = ((ps.max_health * force.min(127) + 50) / 100).min(127);
            let from = [
                ps.origin[0] - origin[0],
                ps.origin[1] - origin[1],
                ps.origin[2] - origin[2],
            ];
            if from == [0.0; 3] {
                ps.damage_yaw = 255;
                ps.damage_pitch = 255;
            } else {
                let angles = math_iw4::vect_to_angles(from);
                ps.damage_pitch = (angles[0] / 360.0 * 256.0) as u32 & 0xff;
                ps.damage_yaw = (angles[1] / 360.0 * 256.0) as u32 & 0xff;
            }
            ps.damage_count = blood;
            ps.damage_event = ps.damage_event.wrapping_add(1);
            Ok(Value::Undefined)
        })
    });

    registry.register(Method, "thermalvisionon", |world, receiver, _| {
        other_flag(world, receiver, pof::THERMAL_VISION, true)
    });
    registry.register(Method, "thermalvisionoff", |world, receiver, _| {
        other_flag(world, receiver, pof::THERMAL_VISION, false)
    });
    registry.register(Method, "thermalvisionfofoverlayon", |world, receiver, _| {
        other_flag(world, receiver, pof::THERMAL_VISION_OVERLAY_FOF, true)
    });
    registry.register(
        Method,
        "thermalvisionfofoverlayoff",
        |world, receiver, _| other_flag(world, receiver, pof::THERMAL_VISION_OVERLAY_FOF, false),
    );
    registry.register(Method, "remotecamerasoundscapeon", |world, receiver, _| {
        other_flag(world, receiver, pof::REMOTE_CAMERA_SOUNDS, true)
    });
    registry.register(Method, "remotecamerasoundscapeoff", |world, receiver, _| {
        other_flag(world, receiver, pof::REMOTE_CAMERA_SOUNDS, false)
    });
    registry.register(Method, "setempjammed", |world, receiver, args| {
        let on = truthy(args, 0)?;
        other_flag(world, receiver, pof::EMP_JAMMED, on)
    });
    registry.register(
        Method,
        "setrearviewrenderenabled",
        |world, receiver, args| {
            let on = truthy(args, 0)?;
            other_flag(world, receiver, pof::ALT_SCENE_REAR_VIEW, on)
        },
    );
    registry.register(Method, "startac130", |world, receiver, _| {
        other_flag(world, receiver, pof::AC130, true)
    });
    registry.register(Method, "stopac130", |world, receiver, _| {
        other_flag(world, receiver, pof::AC130, false)
    });
    registry.register(Method, "stunplayer", |world, receiver, args| {
        let client = player(world, receiver)?;
        let seconds = float(args, 0)?;
        let duration = (seconds * 1000.0 + 0.5).floor();
        if !duration.is_finite() || duration < 0.0 || duration > i32::MAX as f32 {
            return Err(format!("invalid stun duration {seconds}"));
        }
        let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
        if let Some(ps) = FrameWorld::from_world(world).player_mut(ClientId(client)) {
            ps.stun_time = now.wrapping_add(duration as i32);
        }
        Ok(Value::Undefined)
    });

    registry.register(
        Method,
        "worldpointinreticle_circle",
        |world, receiver, args| {
            let point = glam::Vec3::from_array(vector(args, 0)?);
            let fov = float(args, 1)?;
            let radius = float(args, 2)?;
            if !fov.is_finite() || fov <= 0.0 || fov >= 180.0 || !radius.is_finite() || radius < 0.0
            {
                return Err("invalid reticle field of view or radius".into());
            }
            with_player(world, receiver, |ps| {
                let eye =
                    glam::Vec3::from_array(ps.origin) + glam::Vec3::Z * ps.view_height_current;
                let delta = point - eye;
                let (forward, right, up) = math_iw4::angle_vectors(ps.viewangles);
                let depth = delta.dot(glam::Vec3::from_array(forward));
                let scale = 320.0 / (fov.to_radians() * 0.5).tan();
                let x = delta.dot(glam::Vec3::from_array(right)) * scale;
                let y = delta.dot(glam::Vec3::from_array(up)) * scale;
                Ok(Value::Int(i32::from(
                    depth > 0.0 && x * x + y * y < radius * radius * depth * depth,
                )))
            })
        },
    );
    registry.register(Method, "weaponlockstart", |world, receiver, args| {
        let aim = super::guidance::target(world, super::args::arg(args, 0)?, [0.0; 3])?;
        let now = now_ms(world);
        edit_lock(world, receiver, |lock, ps, life| {
            *lock = crate::WeaponLock {
                weapon: ps.weapon,
                life,
                flags: (lock.flags & (TOO_CLOSE | NO_CLEARANCE))
                    | LOCKING
                    | if aim.is_point() { POINT_LOCK } else { 0 },
                aim: Some(aim),
                acquire_started_at: now,
                ..Default::default()
            };
        })?;
        sync_script_locks(world);
        Ok(Value::Undefined)
    });
    registry.register(Method, "weaponlockfinalize", |world, receiver, args| {
        let offset = optional(args, 1, vector)?.unwrap_or([0.0; 3]);
        let aim = super::guidance::target(world, super::args::arg(args, 0)?, offset)?;
        let top = optional(args, 2, int)?.unwrap_or(0) != 0;
        edit_lock(world, receiver, |lock, ps, life| {
            lock.weapon = ps.weapon;
            lock.life = life;
            lock.aim = Some(aim);
            lock.flags = (lock.flags & (TOO_CLOSE | NO_CLEARANCE))
                | LOCKING
                | LOCKED
                | if aim.is_point() { POINT_LOCK } else { 0 }
                | if top { TOP } else { DIRECT };
        })?;
        sync_script_locks(world);
        Ok(Value::Undefined)
    });
    registry.register(Method, "weaponlockfree", |world, receiver, _| {
        edit_lock(world, receiver, |lock, _, _| {
            lock.flags &= !(POINT_LOCK | LOCKING | LOCKED | TOP | DIRECT);
            lock.aim = None;
        })?;
        Ok(Value::Undefined)
    });
    fn lock_bit(
        world: &mut World,
        receiver: &Value,
        args: &[Value],
        bit: u8,
    ) -> Result<Value, String> {
        let on = int(args, 0)? != 0;
        edit_lock(world, receiver, |lock, _, _| {
            if on {
                lock.flags |= bit;
            } else {
                lock.flags &= !bit;
            }
        })?;
        Ok(Value::Undefined)
    }
    registry.register(
        Method,
        "weaponlocktargettooclose",
        |world, receiver, args| lock_bit(world, receiver, args, TOO_CLOSE),
    );
    registry.register(Method, "weaponlocknoclearance", |world, receiver, args| {
        lock_bit(world, receiver, args, NO_CLEARANCE)
    });

    registry.register(Function, "setminimap", |world, _, args| {
        string(args, 0)?;
        let upper_left = [float(args, 1)?, float(args, 2)?];
        let lower_right = [float(args, 3)?, float(args, 4)?];
        let north_yaw = world
            .resource::<RoundScript>()
            .engine
            .worldspawn
            .get("northyaw")
            .and_then(|yaw| yaw.trim().parse::<f32>().ok())
            .unwrap_or(0.0);
        let bounds =
            hud_iw4::compass_map_bounds_from_minimap_corners(upper_left, lower_right, north_yaw)
                .ok_or("setminimap: the corners enclose no area")?;
        let (upper_left, north, size) = (bounds.upper_left, bounds.north, bounds.world_size);
        world.resource_mut::<RoundScript>().engine.minimap = Some(MiniMap {
            upper_left,
            north,
            size,
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "beginlocationselection", |world, receiver, args| {
        let client = player(world, receiver)?;
        let material = string(args, 0)?;
        let choose_direction = optional(args, 1, int)?.unwrap_or(0) != 0;
        let radius = optional(args, 2, float)?.unwrap_or(0.0);
        let mut frame = FrameWorld::from_world(world);
        if frame.client_meta(ClientId(client)).is_some() {
            frame.client_meta_mut(ClientId(client)).location_selection =
                Some(crate::LocationSelection {
                    material,
                    choose_direction,
                    radius,
                });
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "endlocationselection", |world, receiver, _| {
        let client = player(world, receiver)?;
        let mut frame = FrameWorld::from_world(world);
        if frame.client_meta(ClientId(client)).is_some() {
            frame.client_meta_mut(ClientId(client)).location_selection = None;
        }
        Ok(Value::Undefined)
    });
}

fn now_ms(world: &World) -> i32 {
    crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick)
}

pub(crate) fn sync_script_locks(world: &mut World) {
    let mut frame = FrameWorld::from_world(world);
    for id in frame.client_ids_sorted() {
        let Some(ps) = frame.player(id).copied() else {
            continue;
        };
        let Some(meta) = frame.client_meta(id) else {
            continue;
        };
        let lock = meta.weapon_lock;
        let valid = lock.weapon == ps.weapon
            && lock.life == meta.life_sequence.0
            && meta.lifecycle == crate::ClientLifecycle::Alive;
        let point = lock
            .aim
            .filter(|_| valid)
            .and_then(|aim| aim.resolve(&frame));
        if let Some(point) = point {
            frame.client_meta_mut(id).weapon_lock.target = point;
        } else if lock.aim.is_some() {
            frame.client_meta_mut(id).weapon_lock = crate::WeaponLock::default();
        }
    }
}

pub(crate) fn select_location(
    world: &mut World,
    client: u32,
    cmd: &mut playerstate_iw4::UserCmd,
    old_buttons: u32,
) {
    let selecting = FrameWorld::from_world(world)
        .client_meta(ClientId(client))
        .is_some_and(|m| m.location_selection.is_some());
    if !selecting {
        return;
    }
    let pressed = cmd.buttons & !old_buttons;
    let authority = world
        .resource::<crate::step::StepRequest>()
        .reason
        .advances_authority_world();
    let receiver = world
        .resource::<RoundScript>()
        .players
        .get(&client)
        .map(|slot| Value::Object(slot.object));
    if let (true, Some(receiver)) = (authority, receiver) {
        if pressed & buttons::LOCATION_SELECT != 0 {
            let map = world.resource::<RoundScript>().engine.minimap;
            let loc = |byte: u8| (f32::from(byte as i8) + 128.0) / 255.0;
            let location = map.map_or([0.0; 3], |map| {
                let x = loc(cmd.selected_location[0]) * map.size[0];
                let y = loc(cmd.selected_location[1]) * map.size[1];
                [
                    x * map.north[1] + map.upper_left[0] - y * map.north[0],
                    map.upper_left[1] - x * map.north[0] - y * map.north[1],
                    0.0,
                ]
            });
            let north_yaw = map.map_or(0.0, |map| map.north[1].atan2(map.north[0]).to_degrees());
            let yaw = (f32::from(cmd.selected_location[2]) * (360.0 / 256.0) + north_yaw)
                .rem_euclid(360.0);
            crate::script::runtime::raise(
                world,
                receiver,
                "confirm_location",
                vec![Value::Vector(location), Value::Float(yaw)],
            );
        } else if pressed & buttons::LOCATION_CANCEL != 0 {
            crate::script::runtime::raise(world, receiver, "cancel_location", Vec::new());
        }
    }
    cmd.buttons &= buttons::CROUCH | buttons::PRONE;
}

pub(crate) fn command_buttons(command: &str) -> u32 {
    match command {
        "+attack" | "mouse1" => buttons::ATTACK,
        "+activate" | "+usereload" => buttons::USE | buttons::USE_RELOAD,
        "+melee" => buttons::MELEE_CHARGE,
        "+frag" => buttons::FRAG,
        "+smoke" => buttons::SMOKE,
        "+speed_throw" | "+toggleads_throw" | "mouse2" => buttons::ADS,
        "+gostand" | "space" => buttons::JUMP,
        _ => campaign::command_buttons(command),
    }
}

fn raise_commands(world: &mut World, client: u32, fired: impl Fn(&str) -> bool) {
    if !world
        .resource::<crate::step::StepRequest>()
        .reason
        .advances_authority_world()
    {
        return;
    }
    let Some((receiver, notifies)) =
        world
            .resource::<RoundScript>()
            .players
            .get(&client)
            .map(|slot| {
                let notifies: Vec<Arc<str>> = slot
                    .commands
                    .iter()
                    .filter(|(command, _)| fired(command))
                    .map(|(_, notify)| notify.clone())
                    .collect();
                (Value::Object(slot.object), notifies)
            })
    else {
        return;
    };
    for notify in notifies {
        crate::script::runtime::raise(world, receiver.clone(), &notify, Vec::new());
    }
}

pub(crate) fn player_commands(world: &mut World, client: u32, cmd_buttons: u32, old_buttons: u32) {
    let pressed = cmd_buttons & !old_buttons;
    let released = old_buttons & !cmd_buttons;
    if pressed | released == 0 {
        return;
    }
    raise_commands(world, client, |command| match command.strip_prefix('-') {
        Some(held) => released & command_buttons(&format!("+{held}")) != 0,
        None => pressed & command_buttons(command) != 0,
    });
}

pub(crate) fn action_slot_command(world: &mut World, client: u32, slot: u8) {
    let command = format!("+actionslot {}", u32::from(slot) + 1);
    raise_commands(world, client, |bound| bound == command);
}
