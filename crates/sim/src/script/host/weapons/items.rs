use super::super::args::int;
use super::super::entities::EntityKind;
use crate::frame::FrameWorld;
use crate::item::{DroppedItem, ItemWeaponAmmo};
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;
use playerstate_iw4::ENTITYNUM_NONE;

fn draw(seed: &mut u32, min: i32, max: i32) -> i32 {
    min + anim_iw4::random(seed) as i32 % (max - min + 1)
}

fn random_ammo(
    weapon: u32,
    facts: crate::WeaponItemFacts,
    flags: i32,
    seed: &mut u32,
) -> Result<ItemWeaponAmmo, String> {
    let f = facts.ammo;
    let min = f.total_min.min(f.total_max);
    let max = f.total_min.max(f.total_max);
    if i64::from(max) - i64::from(min) + 1 > i64::from(i32::MAX)
        || f.clip_percent_max
            .checked_sub(f.clip_percent_min)
            .is_none_or(|range| range == i32::MAX)
        || f.clip_size < 0
    {
        return Err("invalid weapon item ammo bounds".into());
    }
    let dual_wield = f.allow_dual_wield && flags & 4 != 0;
    let mut total = if max < 0 {
        0
    } else {
        draw(seed, min, max).max(0)
    };
    let mut clips = [0; 2];
    if total > 0 {
        for clip in &mut clips[..if dual_wield { 2 } else { 1 }] {
            let value = if f.clip_size == 1 {
                1
            } else {
                let percent = if f.clip_percent_min < f.clip_percent_max {
                    draw(seed, f.clip_percent_min, f.clip_percent_max)
                } else {
                    f.clip_percent_max
                };
                (f.clip_size as f32 * (percent as f32 / 100.) + 0.5) as i32
            };
            *clip = value.min(total);
            total -= *clip;
        }
    }
    Ok(ItemWeaponAmmo {
        weapon,
        clip_r: clips[0],
        clip_l: clips[1],
        stock: total,
        dual_wield,
    })
}

pub(crate) fn initialize_map(world: &mut World) -> Result<(), String> {
    if world.resource::<RoundScript>().profile != asset_core::RuntimeProfile::Campaign {
        return Ok(());
    }
    let objects: Vec<_> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| {
            entity.kind == EntityKind::Map && entity.classname.starts_with("weapon_")
        })
        .map(|(object, entity)| (*object, entity.classname.clone()))
        .collect();
    let mut prepared = Vec::new();
    for (object, classname) in objects {
        let (mut origin, mut angles, flags, clipmask) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let Value::Vector(origin) = runtime.object_field(object, "origin") else {
                return Err(format!("{classname}: weapon item has no origin"));
            };
            let angles = match runtime.object_field(object, "angles") {
                Value::Vector(v) => v,
                _ => [0.; 3],
            };
            let flags = match runtime.object_field(object, "spawnflags") {
                Value::Int(v) => v,
                _ => 0,
            };
            let clipmask = match runtime.object_field(object, "clipmask") {
                Value::Int(v) if v != 0 => v as u32,
                _ => crate::item::ITEM_PLACEMENT_MASK,
            };
            (origin, angles, flags, clipmask)
        };
        let mut frame = FrameWorld::from_world(world);
        let mut ground = ENTITYNUM_NONE;
        if flags & 1 == 0 {
            let mut start = origin;
            let end = [origin[0], origin[1], origin[2] - 4096.0];
            let trace = |start| frame.trace_clip(start, end, [0.0; 3], [1.0; 3], clipmask);
            let mut hit = trace(start);
            if hit.startsolid != 0 {
                start[2] -= 15.0;
                hit = trace(start);
            }
            if hit.startsolid != 0 {
                diag::warn!(
                    Sim,
                    "campaign map item {classname} starts in solid at {origin:?}"
                );
            }
            origin = std::array::from_fn(|i| start[i] + (end[i] - start[i]) * hit.fraction);
            ground = i32::from(trace_iw4::trace_get_entity_hit_id(hit.hit_type, hit.hit_id));
            if hit.fraction < 1.0 {
                let forward = glam::Vec3::from_array(math_iw4::angles_to_axis(angles)[0]);
                let up = glam::Vec3::from_array(hit.normal);
                let right = up.cross(forward);
                let forward = right.cross(up);
                angles =
                    math_iw4::axis_to_angles([forward.to_array(), right.to_array(), up.to_array()]);
                angles[2] += 90.0;
            }
        }
        let weapon = frame
            .weapon_index_by_script_name(&classname[7..])
            .filter(|weapon| *weapon != 0)
            .ok_or_else(|| format!("{classname}: weapon index is unavailable"))?;
        let facts = frame
            .weapon_item_facts(weapon)
            .ok_or_else(|| format!("{classname}: native weapon item facts are unavailable"))?;
        let mut seed = frame.anim_event_seed();
        let primary = random_ammo(weapon, facts, flags, &mut seed)?;
        let alternate = if facts.alternate == 0 {
            None
        } else {
            let alt = frame.weapon_item_facts(facts.alternate).ok_or_else(|| {
                format!("{classname}: alternate weapon item facts are unavailable")
            })?;
            if alt.ammo.shared_ammo {
                None
            } else {
                Some(random_ammo(facts.alternate, alt, flags, &mut seed)?)
            }
        };
        frame.set_anim_event_seed(seed);
        prepared.push((object, origin, angles, ground, primary, alternate));
    }
    let count = prepared.len();
    for (object, origin, angles, ground, primary, alternate) in prepared {
        let mut frame = FrameWorld::from_world(world);
        let number = frame
            .allocate_dynamic_entity(crate::gentity::EntityRunKind::Item)
            .map_err(|error| format!("map weapon item allocation: {error:?}"))?
            .number();
        let stationary = |base| entity_iw4::Trajectory {
            tr_type: entity_iw4::TR_STATIONARY,
            tr_time: 0,
            tr_duration: 0,
            tr_base: base,
            tr_delta: [0.; 3],
        };
        let mut state = crate::gentity::init_item_state(
            number,
            primary.weapon,
            stationary(origin),
            stationary(angles),
            ENTITYNUM_NONE,
        );
        state.ground_entity_num = ground;
        frame.push_dropped_item(DroppedItem {
            state,
            origin,
            falling: false,
            clip_r: primary.clip_r,
            clip_l: primary.clip_l,
            stock: primary.stock,
            scavenger: false,
            drop_seq: 0,
            authored: true,
            dual_wield: primary.dual_wield,
            alternate,
            hide_part_bits: crate::item::model::initial_mask(
                &frame,
                primary.weapon,
                state.weapon_model,
            ),
        });
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(object, "origin", Value::Vector(origin));
        runtime.set_object_field(object, "angles", Value::Vector(angles));
        let entity = runtime.entities.get_mut(&object).unwrap();
        entity.kind = EntityKind::Item(number);
        entity.number = number;
    }
    if count != 0 {
        diag::info!(Sim, "campaign map weapon items: {count} initialized");
    }
    Ok(())
}

pub(crate) fn drop_ammo(
    world: &mut World,
    weapon: u32,
) -> Result<(ItemWeaponAmmo, Option<ItemWeaponAmmo>), String> {
    let mut frame = FrameWorld::from_world(world);
    let facts = frame
        .weapon_item_facts(weapon)
        .ok_or("weapon item ammo facts are unavailable")?;
    let mut seed = frame.anim_event_seed();
    let primary = random_ammo(weapon, facts, 0, &mut seed)?;
    let alternate = if facts.alternate == 0 {
        None
    } else {
        let alt = frame
            .weapon_item_facts(facts.alternate)
            .ok_or("alternate item ammo facts are unavailable")?;
        if alt.ammo.shared_ammo {
            None
        } else {
            Some(random_ammo(facts.alternate, alt, 0, &mut seed)?)
        }
    };
    frame.set_anim_event_seed(seed);
    Ok((primary, alternate))
}

pub(crate) fn set_ammo(world: &mut World, number: i32, args: &[Value]) -> Result<Value, String> {
    let clip = int(args, 0)?;
    let stock = int(args, 1)?;
    let left = if args.len() > 2 { int(args, 2)? } else { clip };
    let slot = if args.len() > 3 { int(args, 3)? } else { 0 };
    if clip < 0 || stock < 0 || left < 0 {
        return Err("weapon item ammo must be nonnegative".into());
    }
    if !(0..=1).contains(&slot) {
        return Err("weapon item ammo slot must be 0 or 1".into());
    }
    let mut frame = FrameWorld::from_world(world);
    let item = frame
        .dropped_item_by_number(number)
        .ok_or("weapon item has been removed")?;
    let weapon = if slot == 0 {
        item.state.index as u32
    } else {
        let Some(alt) = item.alternate else {
            return Ok(Value::Undefined);
        };
        alt.weapon
    };
    let limit = frame
        .weapon_item_facts(weapon)
        .ok_or("native item clip size unavailable")?
        .ammo
        .clip_size;
    let item = frame.dropped_item_mut_by_number(number).unwrap();
    if slot == 0 {
        item.clip_r = clip.min(limit);
        item.stock = stock;
        if item.dual_wield {
            item.clip_l = left.min(limit);
        }
    } else {
        let alt = item.alternate.as_mut().unwrap();
        alt.clip_r = clip.min(limit);
        alt.stock = stock;
        if alt.dual_wield {
            alt.clip_l = left.min(limit);
        }
    }
    Ok(Value::Undefined)
}
