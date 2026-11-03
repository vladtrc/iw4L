use bevy::prelude::*;
use entity_iw4::Trajectory;
use sim::{ClientId, FireCause, ProjectileState};
use std::time::Duration;

use crate::client::presentation::entities::CEntity;

const PREDICTED_PROJECTILE_GRACE: Duration = Duration::from_millis(200);

/// Flight of a projectile entity: its authoritative row, or the owner's launch until one arrives.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ProjectileMotion {
    pub owner: ClientId,
    pub weapon: u32,
    pub pos: Trajectory,
    pub apos: Trajectory,
    pub launch_time: i32,
    pub spawn_time_ms: i32,
}

impl ProjectileMotion {
    pub fn from_state(projectile: &ProjectileState) -> Self {
        Self {
            owner: projectile.owner,
            weapon: projectile.weapon,
            pos: projectile.pos,
            apos: projectile.apos,
            launch_time: projectile.launch_time,
            spawn_time_ms: projectile.spawn_time_ms,
        }
    }

    pub fn origin_at(&self, at_time_ms: i32) -> [f32; 3] {
        entity_iw4::evaluate_trajectory(&self.pos, at_time_ms)
    }
}

/// Owner launch awaiting the authoritative projectile with the same cause.
#[derive(Component, Debug)]
pub struct ClientPredictedProjectile {
    pub cause: FireCause,
    timeout: Timer,
}

/// Keeps a projectile entity's motion equal to its authoritative row.
pub(crate) fn install_projectile_motion(
    commands: &mut Commands,
    entity: Entity,
    state: entity_iw4::EntityState,
    snapshot: Option<&sim::Snapshot>,
) {
    let row = (state.e_type == entity_iw4::ET_MISSILE)
        .then(|| {
            snapshot?
                .projectiles
                .iter()
                .find(|p| p.entnum == state.number)
        })
        .flatten();
    match row {
        Some(row) => {
            commands
                .entity(entity)
                .insert(ProjectileMotion::from_state(row));
        }
        None => {
            commands.entity(entity).remove::<ProjectileMotion>();
        }
    }
}

pub(crate) fn spawn_predicted_projectile(
    commands: &mut Commands,
    cause: FireCause,
    weapon: u32,
    tick: sim::Tick,
    launch: sim::LaunchMotion,
    roundtrip: Option<Duration>,
) {
    let timeout = roundtrip.unwrap_or(Duration::ZERO) + PREDICTED_PROJECTILE_GRACE;
    perf::instant!("predicted_projectile_spawn", "iw4l.sim", |ctx| {
        ctx.arg("client", i64::from(cause.client.0));
        ctx.arg("life", i64::from(cause.life.0));
        ctx.arg("command", i64::from(cause.command.0));
        ctx.arg("ordinal", i64::from(cause.ordinal));
        ctx.arg("hand", i64::from(cause.hand));
        ctx.arg("weapon", i64::from(weapon));
        ctx.arg("timeout_ms", timeout.as_millis() as i64);
    });
    commands.spawn((
        ProjectileMotion {
            owner: cause.client,
            weapon,
            pos: launch.pos,
            apos: launch.apos,
            launch_time: launch.launch_time,
            spawn_time_ms: sim::level_time_ms(tick),
        },
        ClientPredictedProjectile {
            cause,
            timeout: Timer::new(timeout, TimerMode::Once),
        },
        DespawnOnExit(frame::Timeline::Live),
    ));
}

/// The authority refused or never launched this shot.
pub(crate) fn expire_predicted_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    mut pending: Query<(Entity, &mut ClientPredictedProjectile), Without<CEntity>>,
) {
    for (entity, mut predicted) in &mut pending {
        if predicted.timeout.tick(time.delta()).is_finished() {
            perf::instant!("predicted_projectile_timed_out", "iw4l.sim", |ctx| {
                let cause = predicted.cause;
                ctx.arg("client", i64::from(cause.client.0));
                ctx.arg("life", i64::from(cause.life.0));
                ctx.arg("command", i64::from(cause.command.0));
                ctx.arg("ordinal", i64::from(cause.ordinal));
                ctx.arg("hand", i64::from(cause.hand));
            });
            commands.entity(entity).despawn();
        }
    }
}
