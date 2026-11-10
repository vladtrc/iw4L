use crate::ProjectileId;
use crate::bullet_collision::{CONTENTS_SKY, ColliderId, MASK_BULLET_WORLD, TraceOutcome};
use crate::frame::FrameWorld;
use crate::match_state::ClientLifecycle;
use crate::world::{ClientId, Tick};
use bevy_ecs::prelude::{Resource, World};

const PITCH_RANGE: [f32; 2] = [1.0, 87.0];
const PITCH_RATE: f32 = 15.0;
const YAW_RATE: f32 = 20.0;
const SPEED_RANGE: [f32; 2] = [3_000.0, 6_000.0];
const SPEED_UP: f32 = 2_000.0;
const SPEED_DOWN: f32 = 500.0;

const SKY_CEILING_MIN_NORMAL_Z: f32 = 0.7;

// The mark is kept for the missile's whole flight. The pilot's control link is
// dropped when the pilot dies, and the missile must still pass the sky ceiling then.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct RemoteMissiles(Vec<ProjectileId>);

pub(crate) fn mark_remote(world: &mut World, id: ProjectileId) {
    let mut marked = world.get_resource_or_insert_with(RemoteMissiles::default);
    if !marked.0.contains(&id) {
        marked.0.push(id);
    }
}

pub(crate) fn is_remote(world: &mut FrameWorld, id: ProjectileId) -> bool {
    world
        .ecs()
        .get_resource::<RemoteMissiles>()
        .is_some_and(|marked| marked.0.contains(&id))
}

fn forget_gone_missiles(world: &mut FrameWorld) {
    let marked = match world.ecs().get_resource::<RemoteMissiles>() {
        Some(marked) if !marked.0.is_empty() => marked.0.clone(),
        _ => return,
    };
    let mut flying = Vec::with_capacity(marked.len());
    world.visit_projectiles(|p| {
        if marked.contains(&p.id) {
            flying.push(p.id);
        }
    });
    if flying.len() != marked.len() {
        world.ecs().resource_mut::<RemoteMissiles>().0 = flying;
    }
}

pub(crate) fn projectile_mask(remote: bool) -> u32 {
    if remote {
        MASK_BULLET_WORLD & !CONTENTS_SKY
    } else {
        MASK_BULLET_WORLD
    }
}

pub(crate) fn sky_wall_outcome(
    outcome: TraceOutcome,
    start: [f32; 3],
    sky: &trace_iw4::Trace,
) -> TraceOutcome {
    if sky.fraction >= 1.0
        || sky.startsolid != 0
        || sky.allsolid != 0
        || sky.normal[2] > SKY_CEILING_MIN_NORMAL_Z
    {
        return outcome;
    }
    let reach = match outcome {
        TraceOutcome::Hit { end, .. } | TraceOutcome::Miss { end } => distance(start, end),
        TraceOutcome::StartSolid { .. } | TraceOutcome::Invalid { .. } => return outcome,
    };
    if distance(start, sky.endpos) >= reach {
        return outcome;
    }
    TraceOutcome::Hit {
        fraction: sky.fraction,
        end: sky.endpos,
        normal: sky.normal,
        collider: ColliderId::World {
            surface_flags: sky.surface_flags,
            contents: sky.contents,
            glass_encoded: 0,
        },
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

pub(crate) fn steer(
    world: &mut FrameWorld,
    id: ClientId,
    cmd: &playerstate_iw4::UserCmd,
    msec: i32,
) {
    let Some(mut link) = world
        .client_meta(id)
        .and_then(|m| m.remote_missile)
        .filter(|link| link.unlink_at_ms.is_none())
    else {
        return;
    };
    let seconds = msec as f32 * 0.001;
    let delta = math_iw4::angles_to_axis([
        f32::from(cmd.remote_control[0] as i8) / 127.0 * seconds * PITCH_RATE,
        f32::from(cmd.remote_control[1] as i8) / 127.0 * seconds * YAW_RATE,
        0.0,
    ]);
    let transposed = core::array::from_fn(|row| core::array::from_fn(|col| delta[col][row]));
    let mut angles = math_iw4::axis_to_angles(math_iw4::matrix_multiply(
        transposed,
        math_iw4::angles_to_axis(link.angles),
    ));
    angles[0] = (angles[0] / 360.0 - (angles[0] / 360.0 + 0.5).floor()) * 360.0;
    angles[0] = angles[0].clamp(PITCH_RANGE[0], PITCH_RANGE[1]);
    link.angles = angles;
    let held = cmd.buttons & playerstate_iw4::buttons::REMOTE_CONTROL != 0;
    link.attack = held && cmd.buttons & playerstate_iw4::buttons::ATTACK != 0;
    link.armed |= held && !link.attack;
    world.client_meta_mut(id).remote_missile = Some(link);
}

pub(crate) fn advance(world: &mut FrameWorld, tick: Tick) {
    forget_gone_missiles(world);
    let now = crate::level_time_ms(tick);
    for id in world.client_ids_sorted() {
        let Some(mut link) = world.client_meta(id).and_then(|m| m.remote_missile) else {
            continue;
        };
        let alive = world
            .client_meta(id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive);
        if !alive {
            world.client_meta_mut(id).remote_missile = None;
            continue;
        }
        if link.unlink_at_ms.is_some() {
            continue;
        }
        let Some(projectile) = world
            .projectile_mut_by_number(link.entnum)
            .filter(|p| p.id == link.projectile && p.live)
        else {
            continue;
        };
        let (dir, _, _) = math_iw4::angle_vectors(link.angles);
        let seconds = crate::MATCH_TICK_MS as f32 * 0.001;
        let mut speed = projectile
            .velocity
            .iter()
            .zip(dir)
            .map(|(v, d)| v * d)
            .sum::<f32>()
            .max(0.0);
        if link.armed && !link.boosted && link.attack {
            speed = SPEED_RANGE[1];
            link.boosted = true;
        } else {
            let target = SPEED_RANGE[0];
            speed = if speed < target {
                (speed + SPEED_UP * seconds).min(target)
            } else {
                (speed - SPEED_DOWN * seconds).max(target)
            };
        }
        let velocity =
            entity_iw4::truncated_tr_delta([dir[0] * speed, dir[1] * speed, dir[2] * speed]);
        projectile.velocity = velocity;
        projectile.pos = entity_iw4::Trajectory {
            tr_time: now.saturating_sub(crate::MATCH_TICK_MS as i32),
            tr_type: entity_iw4::TR_LINEAR,
            tr_duration: 0,
            tr_delta: velocity,
            tr_base: projectile.origin,
        };
        projectile.apos = entity_iw4::fire_missile_apos(dir);
        world.client_meta_mut(id).remote_missile = Some(link);
    }
}
