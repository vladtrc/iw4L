use crate::{EquipmentRuntimeFacts, ProjectileState};
use glam::{Quat, Vec3};

pub(crate) fn steer(
    projectile: &mut ProjectileState,
    from_ms: i32,
    now: i32,
    facts: EquipmentRuntimeFacts,
) {
    if projectile.stable_missile
        || !projectile.live
        || projectile.grounded
        || projectile.attached_to.is_some()
        || projectile.guide.target.is_some()
        || facts.weap_type != weapon_iw4::WEAPTYPE_PROJECTILE
        || facts.missile_guidance != 0
        || projectile.pos.tr_type == entity_iw4::TR_STATIONARY
        || !facts.missile_drift_interval_s.is_finite()
        || facts.missile_drift_interval_s <= 0.0
        || !facts.missile_drift_rate_deg_s.is_finite()
        || facts.missile_drift_rate_deg_s <= 0.0
        || facts.projectile_speed <= 0
    {
        return;
    }
    let delay_ms = 1000.0 * facts.missile_drift_start_distance.max(0) as f64
        / f64::from(facts.projectile_speed);
    let elapsed_ms = f64::from(now.saturating_sub(projectile.spawn_time_ms));
    let step_start = from_ms.max(projectile.spawn_time_ms);
    if elapsed_ms <= delay_ms || now <= step_start {
        return;
    }
    let velocity = Vec3::from_array(projectile.velocity);
    if !velocity.is_finite() || velocity.length_squared() == 0.0 {
        return;
    }
    let interval_ms = (f64::from(facts.missile_drift_interval_s) * 1000.0).max(1.0);
    let phase = ((elapsed_ms - delay_ms) / interval_ms).floor() as u64;
    let seed = (u64::from(projectile.id.0) << 32) | u64::from(projectile.spawn_time_ms as u32);
    let mut rng = crate::identities::MatchRng::new(seed ^ phase ^ u64::from(projectile.weapon));
    let rates = Vec3::from_array(std::array::from_fn(|_| {
        (rng.next_u32() as f32 / u32::MAX as f32 * 2.0 - 1.0)
            * facts.missile_drift_rate_deg_s.to_radians()
    }));
    let seconds =
        ((elapsed_ms - delay_ms).min(f64::from(now.saturating_sub(step_start))) * 0.001) as f32;
    let turned = Quat::from_scaled_axis(rates * seconds) * velocity;
    if !turned.is_finite() || turned.length_squared() == 0.0 {
        return;
    }
    projectile.velocity = turned.to_array();
    projectile.pos = entity_iw4::Trajectory {
        tr_time: step_start,
        tr_type: entity_iw4::TR_LINEAR,
        tr_duration: 0,
        tr_base: projectile.origin,
        tr_delta: projectile.velocity,
    };
    projectile.apos = entity_iw4::fire_missile_apos(turned.normalize().to_array());
}
