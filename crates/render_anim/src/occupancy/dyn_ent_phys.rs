use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use crate::occupancy::dyn_ent::xmodel_phys_hull;
use assets::ClipCollision;
use net::CgFrameClock;
use render_scene::DynEntModelEntity;
use render_scene::WorldDynEntInstance;

const GRAVITY_Z: f32 = -800.0;

const MSEC_STEP: i32 = 17;

const MAX_ITERS: i32 = 2;

const CONTENTS_SOLID: u32 = 1;
const SLEEP_SPEED: f32 = 4.0;
const GROUND_NZ: f32 = 0.7;
const CONTACT_SLOP: f32 = 0.125;
const SLEEP_MS: i32 = 200;

#[derive(Resource, Clone, Default)]
pub struct DynEntPhysClip(pub Option<Arc<ClipCollision>>);

#[derive(Resource, Default)]
pub struct DynEntPhysWorld {
    last_time: Option<i32>,
    bodies: HashMap<Entity, DynEntPhysBody>,
}

impl DynEntPhysWorld {
    pub fn is_awake(&self, entity: Entity) -> bool {
        self.bodies.contains_key(&entity)
    }

    pub fn destroy_body(&mut self, entity: Entity) {
        self.bodies.remove(&entity);
    }
}

#[derive(Clone, Copy, Debug, Message)]
pub struct DynEntPhysImpulse {
    pub entity: Entity,

    pub impulse: Vec3,
}

#[derive(Clone, Copy, Debug)]
struct DynEntPhysBody {
    origin: Vec3,
    vel: Vec3,
    mins: [f32; 3],
    maxs: [f32; 3],
    bounce: f32,
    friction: f32,
    on_ground: bool,
    asleep_ms: i32,
}

pub(crate) fn register_dyn_ent_phys(app: &mut App) {
    app.init_resource::<DynEntPhysClip>()
        .init_resource::<DynEntPhysWorld>()
        .add_message::<DynEntPhysImpulse>();
}

pub fn step_phys_world0(
    mut world: ResMut<DynEntPhysWorld>,
    clip: Res<DynEntPhysClip>,
    clock: Option<Res<CgFrameClock>>,
    catalog: Option<Res<assets::MapXModelSceneCatalog>>,
    mut impulses: MessageReader<DynEntPhysImpulse>,
    mut instances: Query<
        (Entity, &mut WorldDynEntInstance, &mut Transform),
        With<DynEntModelEntity>,
    >,
) {
    let catalog = catalog.as_deref();
    for impulse in impulses.read() {
        let Ok((entity, inst, transform)) = instances.get(impulse.entity) else {
            continue;
        };
        if inst.dead {
            continue;
        }
        let Some(props) = assets::retail_dyn_ent_props(inst.ty) else {
            continue;
        };
        if !props.use_physics {
            continue;
        }
        let Some(preset) = inst.phys_preset.as_ref() else {
            continue;
        };
        if preset.mass <= 0.0 {
            continue;
        }
        let (mins, maxs) = xmodel_phys_hull(catalog, &inst.current_model);
        let add_vel = impulse.impulse / preset.mass;
        world
            .bodies
            .entry(entity)
            .and_modify(|body| {
                body.vel += add_vel;
                body.asleep_ms = 0;
            })
            .or_insert_with(|| DynEntPhysBody {
                origin: transform.translation,
                vel: add_vel,
                mins,
                maxs,
                bounce: preset.bounce,
                friction: preset.friction,
                on_ground: false,
                asleep_ms: 0,
            });
    }

    let now = clock
        .as_deref()
        .filter(|clock| clock.started())
        .map(CgFrameClock::time);
    let mut advanced_ms = 0;
    if let Some(now) = now {
        match world.last_time {
            None => world.last_time = Some(now),
            Some(last) if now <= last => {}
            Some(last) => {
                let clip = clip.0.as_deref();
                let mut remain = now.saturating_sub(last);
                let mut left = MAX_ITERS;
                let mut advanced = 0i32;
                while remain > 0 && left > 0 {
                    let step = if MSEC_STEP < remain / left.max(1) {
                        remain / left
                    } else {
                        MSEC_STEP
                    }
                    .clamp(1, remain);
                    let dt = step as f32 / 1000.0;
                    if let Some(clip) = clip {
                        for body in world.bodies.values_mut() {
                            integrate_body(body, clip, dt);
                        }
                    } else {
                        for body in world.bodies.values_mut() {
                            integrate_free(body, dt);
                        }
                    }
                    remain -= step;
                    left -= 1;
                    advanced += step;
                }
                world.last_time = Some(last + advanced);
                advanced_ms = advanced;
            }
        }
    }

    let mut sleep = Vec::new();
    for (entity, body) in world.bodies.iter_mut() {
        if body.on_ground && body.vel.length() < SLEEP_SPEED {
            body.asleep_ms = body.asleep_ms.saturating_add(advanced_ms);
            if body.asleep_ms >= SLEEP_MS {
                sleep.push(*entity);
            }
        } else {
            body.asleep_ms = 0;
        }
    }

    for (entity, mut inst, mut transform) in &mut instances {
        let Some(body) = world.bodies.get(&entity) else {
            continue;
        };
        transform.translation = body.origin;
        inst.lighting_origin = body.origin.to_array();
        inst.transform.translation = body.origin;
    }

    for entity in sleep {
        world.bodies.remove(&entity);
    }
}

fn integrate_free(body: &mut DynEntPhysBody, dt: f32) {
    body.vel.z += GRAVITY_Z * dt;
    body.origin += body.vel * dt;
    body.on_ground = false;
}

fn integrate_body(body: &mut DynEntPhysBody, clip: &ClipCollision, dt: f32) {
    body.vel.z += GRAVITY_Z * dt;
    let start = body.origin;
    let end = start + body.vel * dt;
    let hit = clip.sweep_box(
        start.to_array(),
        end.to_array(),
        body.mins,
        body.maxs,
        CONTENTS_SOLID,
    );
    if hit.allsolid || hit.startsolid {
        body.vel = Vec3::ZERO;
        body.on_ground = true;
        let lift = if body.mins[2] < 0.0 {
            -body.mins[2]
        } else {
            1.0
        };
        body.origin.z += lift;
        return;
    }
    if hit.fraction < 1.0 {
        let n = Vec3::from_array(hit.normal);
        body.origin = Vec3::from_array(hit.endpos) + n * CONTACT_SLOP;
        let vn = body.vel.dot(n);
        if vn < 0.0 {
            body.vel -= (1.0 + body.bounce.max(0.0)) * vn * n;
            let n_comp = body.vel.dot(n) * n;
            let tan = body.vel - n_comp;
            body.vel = n_comp + tan * (1.0 - body.friction.clamp(0.0, 1.0));
        }
        body.on_ground = n.z > GROUND_NZ;
    } else {
        body.origin = end;
        body.on_ground = false;
    }
}
