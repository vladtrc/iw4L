use crate::combat::{AcceptedShot, spread_direction_on_plane};
use crate::equipment::{GrenadeLaunchKind, ProjectileState, spawn_grenade_projectile};
use crate::frame::FrameWorld;
use crate::identities::MatchRng;
use entity_iw4::{
    TR_LINEAR, Trajectory, g_fire_grenade_no_draw_ms, g_fire_missile_apos, truncated_tr_delta,
};
use math_iw4::vec3_length;
use weapon_iw4::{FireWeaponKind, ROCKET_SPREAD_PLANE, fire_weapon_kind};

pub(crate) fn fire_accepted_shot(world: &mut FrameWorld, tick: crate::Tick, shot: &AcceptedShot) {
    let Some(combat) = world.combat_facts_for(shot.weapon) else {
        return;
    };
    match fire_weapon_kind(combat.weap_type, combat.weap_class) {
        Some(FireWeaponKind::Bullet) => {}
        Some(FireWeaponKind::GrenadeLauncher) => {
            spawn_grenade_projectile(
                world,
                shot.attacker,
                shot.weapon,
                tick,
                shot.origin,
                shot.angles,
                shot.owner_velocity,
                GrenadeLaunchKind::Launcher,
            );
        }
        Some(FireWeaponKind::Missile) => g_fire_missile(world, tick, shot),
        Some(FireWeaponKind::ThrownGrenade) => {
            spawn_grenade_projectile(
                world,
                shot.attacker,
                shot.weapon,
                tick,
                shot.origin,
                shot.angles,
                shot.owner_velocity,
                GrenadeLaunchKind::Thrown {
                    remaining_fuse_ms: None,
                },
            );
        }
        None => {}
    }
}

fn g_fire_missile(world: &mut FrameWorld, tick: crate::Tick, shot: &AcceptedShot) {
    let Some(facts) = world.missile_launch_facts(shot.weapon) else {
        panic!(
            "G_FireMissile needs iProjectileSpeed@+0x404 on the equipment row; RPG is not an offhand"
        );
    };
    let mut rng = MatchRng::new(shot.combat_seed as u64);
    let dir = spread_direction_on_plane(
        shot.angles,
        shot.spread_degrees,
        &mut rng,
        ROCKET_SPREAD_PLANE,
    );
    let speed = facts.projectile_speed as f32;
    let gun_vel = shot.owner_velocity;
    let id = world.allocate_projectile_id();
    let entnum = world
        .allocate_dynamic_entity(crate::gentity::EntityRunKind::Missile)
        .expect("G_Spawn exhausted dynamic entity slots for missile")
        .number();
    let time_ms = crate::level_time_ms(tick);
    let velocity = truncated_tr_delta([
        dir[0] * speed + gun_vel[0],
        dir[1] * speed + gun_vel[1],
        dir[2] * speed + gun_vel[2],
    ]);
    let raw_speed = vec3_length([
        dir[0] * speed + gun_vel[0],
        dir[1] * speed + gun_vel[1],
        dir[2] * speed + gun_vel[2],
    ]);
    let pos = Trajectory {
        tr_time: time_ms,
        tr_type: TR_LINEAR,
        tr_duration: 0,
        tr_delta: velocity,
        tr_base: shot.origin,
    };
    perf::projectile(shot.weapon);
    world.push_projectile(ProjectileState {
        id,
        owner: shot.attacker,
        owner_life: shot.attacker_life,
        weapon: shot.weapon,
        origin: shot.origin,
        velocity,
        pos,
        apos: g_fire_missile_apos(dir),
        entnum,
        launch_time: time_ms + g_fire_grenade_no_draw_ms(raw_speed),
        spawn_time_ms: time_ms,
        detonate_at_ms: None,
        cleanup_at_ms: time_ms.saturating_add(crate::equipment::ROCKET_CLEANUP_MS),
        travel_distance: 0.0,
        live: true,
        stuck_pane: None,
    });
}
