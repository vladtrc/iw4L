use bevy_ecs::prelude::World;

use crate::frame::FrameWorld;
use crate::script::runtime::raise;
use crate::script::{RoundScript, Value};
use crate::script_player::{Finish, Hit};

const BULLETS: [&str; 4] = [
    "MOD_PISTOL_BULLET",
    "MOD_RIFLE_BULLET",
    "MOD_EXPLOSIVE_BULLET",
    "MOD_HEAD_SHOT",
];

fn shielded(
    (before, after, max): (i32, i32, i32),
    (bullet, critical): (bool, bool),
    now: i64,
    until: &mut Option<i64>,
    window: i64,
) -> i32 {
    if before == max {
        *until = None;
    }
    if after == before {
        return after;
    }
    let mut after = after;
    if bullet && critical && until.is_none_or(|until| until <= now) {
        after = 0;
    }
    if after < 1 && before > 0 && bullet {
        let end = *until.get_or_insert(now + window);
        if now < end {
            after = 1;
        }
    }
    after
}

fn int_field(runtime: &mut RoundScript, object: u64, name: &str) -> i64 {
    match runtime.object_field(object, name) {
        Value::Int(n) => i64::from(n),
        Value::Float(n) => n as i64,
        _ => 0,
    }
}

pub(super) fn damage(
    world: &mut World,
    tick: crate::Tick,
    hit: &Hit,
    attacker: Value,
    weapon: Value,
) {
    let Value::Object(object) = super::player_object(world, hit.victim.0) else {
        return;
    };
    let now = super::now_ms(world);
    let Some((before, max, origin)) = FrameWorld::from_world(world)
        .player(hit.victim)
        .map(|ps| (ps.health, ps.max_health, ps.origin))
    else {
        return;
    };
    let mut after = before - hit.amount.max(0);
    let mut runtime = world.resource_mut::<RoundScript>();
    if runtime.campaign.invulnerable.contains(&object) {
        return;
    }
    if !runtime.campaign.health_shield_off.contains(&object) {
        let window = int_field(&mut runtime, object, "deathinvulnerabletime");
        let reach = int_field(&mut runtime, object, "criticalbulletdamagedist") as f32;
        let near = (0..3)
            .map(|k| (origin[k] - hit.point[k]).powi(2))
            .sum::<f32>();
        let critical = reach != 0.0 && near < reach * reach;
        let mut until = runtime.campaign.health_shield.get(&object).copied();
        let bullet = BULLETS.contains(&hit.means);
        after = shielded(
            (before, after, max),
            (bullet, critical),
            now,
            &mut until,
            window,
        );
        match until {
            Some(until) => runtime.campaign.health_shield.insert(object, until),
            None => runtime.campaign.health_shield.remove(&object),
        };
    }
    if runtime.campaign.death_shield.contains(&object) && before > 1 {
        after = after.max(1);
    }
    drop(runtime);
    let amount = (before - after).max(0);
    let dir = (hit.dir != [0.0; 3]).then_some(hit.dir);
    let mut frame = FrameWorld::from_world(world);
    let killed =
        crate::script_player::finish_damage(&mut frame, hit.victim, amount, dir) == Finish::Killed;
    if killed {
        crate::script_player::kill(&mut frame, tick, hit.victim, hit.attacker, hit.commit);
    }
    let means = Value::string(hit.means);
    raise(
        world,
        Value::Object(object),
        "damage",
        vec![
            Value::Int(hit.amount),
            attacker.clone(),
            Value::Vector(hit.dir),
            Value::Vector(hit.point),
            means.clone(),
            Value::string(""),
            Value::string(""),
            Value::string(""),
            Value::Int(hit.flags),
            weapon.clone(),
        ],
    );
    if killed {
        let args = vec![Value::Undefined, attacker, Value::Undefined, means, weapon];
        super::owe(world, hit.victim.0, super::KILLED, args);
        super::settle_deaths(world);
    }
}
