use crate::combat::AcceptedShot;
use crate::equipment::ProjectileState;
use crate::frame::FrameWorld;
use entity_iw4::{
    TR_LINEAR, Trajectory, g_fire_grenade_no_draw_ms, g_fire_missile_apos, truncated_tr_delta,
};
use math_iw4::vec3_length;
use weapon_iw4::{FireWeaponKind, fire_weapon_kind};

pub(crate) fn fire_accepted_shot(world: &mut FrameWorld, tick: crate::Tick, shot: &AcceptedShot) {
    let Some(combat) = world.combat_facts_for(shot.weapon) else {
        return;
    };
    if fire_weapon_kind(combat.weap_type, combat.weap_class) == Some(FireWeaponKind::Missile) {
        g_fire_missile(world, tick, shot);
    }
}

fn g_fire_missile(world: &mut FrameWorld, tick: crate::Tick, shot: &AcceptedShot) {
    let Some(facts) = world.missile_launch_facts(shot.weapon) else {
        panic!(
            "G_FireMissile needs iProjectileSpeed@+0x404 on the equipment row; RPG is not an offhand"
        );
    };
    let dir = crate::bullet::angles_to_forward(shot.angles);
    let speed = facts.projectile_speed as f32;
    let gun_vel = world
        .player(shot.attacker)
        .map(|ps| ps.velocity)
        .unwrap_or([0.0; 3]);
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
    let raw_speed = {
        let v = [
            dir[0] * speed + gun_vel[0],
            dir[1] * speed + gun_vel[1],
            dir[2] * speed + gun_vel[2],
        ];
        vec3_length(v)
    };
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
        gravity: 0.0,
        age_ticks: 0,
        fuse_ticks: facts.fuse_ticks(),
        pos,
        apos: g_fire_missile_apos(dir),
        entnum,
        launch_time: time_ms + g_fire_grenade_no_draw_ms(raw_speed),
    });
}
