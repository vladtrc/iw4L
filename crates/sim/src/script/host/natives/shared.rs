//! Builtins Modern Warfare 2's and Black Ops' scripts both call, served the
//! same way for either.

use super::super::args::{arg, float, string, vector};
use super::super::arrays::new_array;
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{NativeRegistry, Value};
use crate::script_player;
use crate::world::ClientId;
use bevy_ecs::prelude::World;

pub(crate) fn sight(world: &mut World, start: [f32; 3], end: [f32; 3]) -> bool {
    let t = super::super::presence::settled(world).trace_world(
        start,
        end,
        [0.0; 3],
        [0.0; 3],
        crate::bullet_collision::MASK_SHOT,
    );
    t.fraction >= 1.0 && t.startsolid == 0
}

pub(crate) fn weapon_facts(
    world: &mut World,
    name: &str,
) -> Result<weapon_iw4::WeaponCombatFacts, String> {
    if name == "none" || name.is_empty() {
        return Ok(Default::default());
    }
    let frame = FrameWorld::from_world(world);
    let index = frame
        .weapon_index_by_script_name(name)
        .ok_or_else(|| format!("unknown weapon '{name}'"))?;
    Ok(frame.weapon_combat_row(index).unwrap_or_default())
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Function, "clamp", |_, _, args| {
        match (arg(args, 0)?, arg(args, 1)?, arg(args, 2)?) {
            (Value::Int(v), Value::Int(lo), Value::Int(hi)) => {
                Ok(Value::Int((*v).max(*lo).min(*hi)))
            }
            _ => {
                let (v, lo, hi) = (float(args, 0)?, float(args, 1)?, float(args, 2)?);
                Ok(Value::Float(v.max(lo).min(hi)))
            }
        }
    });
    registry.register(Function, "log", |_, _, args| {
        Ok(Value::Float(float(args, 0)?.ln()))
    });
    registry.register(Function, "sighttracepassed", |world, _, args| {
        let (start, end) = (vector(args, 0)?, vector(args, 1)?);
        Ok(Value::Int(sight(world, start, end).into()))
    });
    registry.register(Function, "getassignedteam", |world, _, args| {
        super::player::player(world, arg(args, 0)?)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "isweaponcliponly", |world, _, args| {
        let facts = weapon_facts(world, &string(args, 0)?)?;
        Ok(Value::Int(
            (facts.clip_size > 0 && facts.max_ammo == 0).into(),
        ))
    });
    registry.register(Function, "weaponissemiauto", |world, _, args| {
        let facts = weapon_facts(world, &string(args, 0)?)?;
        Ok(Value::Int((facts.fire_type == 1).into()))
    });
    registry.register(Function, "weaponstartammo", |world, _, args| {
        Ok(Value::Int(
            weapon_facts(world, &string(args, 0)?)?.start_ammo,
        ))
    });
    registry.register(Method, "getcurrentoffhand", |world, receiver, _| {
        let id = ClientId(super::player::player(world, receiver)?);
        let frame = FrameWorld::from_world(world);
        let weapon = frame
            .player(id)
            .map_or(0, |ps| ps.offhand_primary.max(0) as u32);
        let name = if weapon == 0 {
            "none".to_owned()
        } else {
            script_player::weapon_name(&frame, weapon).to_owned()
        };
        Ok(Value::String(name.into()))
    });
    registry.register(Method, "getfractionmaxammo", |world, receiver, args| {
        let id = ClientId(super::player::player(world, receiver)?);
        let name = string(args, 0)?;
        let facts = weapon_facts(world, &name)?;
        let frame = FrameWorld::from_world(world);
        let weapon = script_player::weapon_named(&frame, &name)?;
        let stock = script_player::ammo_stock(&frame, id, weapon);
        Ok(Value::Float(stock as f32 / facts.max_ammo.max(1) as f32))
    });
    registry.register(Method, "getfractionstartammo", |world, receiver, args| {
        let id = ClientId(super::player::player(world, receiver)?);
        let name = string(args, 0)?;
        let facts = weapon_facts(world, &name)?;
        let frame = FrameWorld::from_world(world);
        let weapon = script_player::weapon_named(&frame, &name)?;
        let held = script_player::ammo_clip(&frame, id, weapon)
            + script_player::ammo_stock(&frame, id, weapon);
        let start = (facts.start_ammo + facts.clip_size).max(1);
        Ok(Value::Float(held as f32 / start as f32))
    });
    registry.register(Method, "getweaponslist", |world, receiver, _| {
        let id = ClientId(super::player::player(world, receiver)?);
        let held = script_player::weapons(
            &FrameWorld::from_world(world),
            id,
            script_player::WeaponList::All,
        );
        let held: Vec<u32> = held
            .into_iter()
            .map(|w| super::super::players::script_weapon(world, id.0, w))
            .collect();
        let frame = FrameWorld::from_world(world);
        let names: Vec<Value> = held
            .into_iter()
            .map(|w| Value::String(script_player::weapon_name(&frame, w).into()))
            .collect();
        new_array(world, names)
    });
}
