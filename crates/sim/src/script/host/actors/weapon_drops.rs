use super::*;
use crate::frame::FrameWorld;
use crate::item::WeaponDropMotion;

// Script-visible overflow behavior: two drops can sample animated attachment
// motion simultaneously; further drops are released immediately.
const SAMPLED_DROPS_PER_ACTOR: usize = 2;
const INVENTORY_ALTERNATE: i32 = 3;

#[derive(Clone, Debug)]
pub(crate) struct Pending {
    owner: u64,
    number: i32,
    weapon: u32,
    tag: Option<&'static str>,
    motion: WeaponDropMotion,
    time: i64,
    sampled: bool,
}

fn sample(world: &mut World, owner: u64, tag: Option<&str>) -> Result<WeaponDropMotion, String> {
    let (origin, angles, presence) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let Value::Vector(origin) = runtime.object_field(owner, "origin") else {
            return Err("weapon source origin is unavailable".into());
        };
        let Value::Vector(angles) = runtime.object_field(owner, "angles") else {
            return Err("weapon source angles are unavailable".into());
        };
        (
            origin,
            angles,
            runtime.entities[&owner]
                .presence
                .ok_or("weapon source has no physical presence")?,
        )
    };
    let (center, fallback) = {
        let frame = FrameWorld::from_world(world);
        let body = frame
            .entity_collision_capabilities()
            .iter()
            .find(|row| row.owner.script_model() == Some(presence))
            .and_then(|row| row.body.as_ref())
            .ok_or("weapon source has no physical body")?;
        (
            std::array::from_fn(|i| origin[i] + body.bounds.midpoint[i]),
            [origin[0], origin[1], origin[2] + body.bounds.half_size[2]],
        )
    };
    let pose = tag.and_then(|tag| super::super::presence::tag_world(world, owner, tag));
    let (at, angles) = pose.map_or((fallback, angles), |(at, axis)| {
        (at, math_iw4::axis_to_angles(axis))
    });
    if !origin
        .into_iter()
        .chain(at)
        .chain(angles)
        .all(f32::is_finite)
    {
        return Err("weapon drop pose must be finite".into());
    }
    if !FrameWorld::from_world(world).collision_complete() {
        return Err("weapon drop collision is unavailable".into());
    }
    let at = if pose.is_some() {
        let hit = super::super::trace::capsule(
            world,
            movement_iw4::GroundTraceInput {
                start: center,
                end: at,
                mins: crate::item::ITEM_MINS,
                maxs: crate::item::ITEM_MAXS,
                tracemask: crate::item::ITEM_PLACEMENT_MASK,
            },
            &[owner],
        );
        std::array::from_fn(|i| center[i] + (at[i] - center[i]) * hit.fraction)
    } else {
        at
    };
    Ok(WeaponDropMotion {
        origin: at,
        angles,
        velocity: [0.; 3],
        angular_velocity: [0.; 3],
    })
}

fn release(world: &mut World, drop: &Pending, motion: WeaponDropMotion) -> Result<(), String> {
    let (ammo, alternate) = super::super::weapons::items::drop_ammo(world, drop.weapon)?;
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let number = crate::item::launch_script_weapon(
        &mut FrameWorld::from_world(world),
        tick,
        drop.number,
        ammo,
        alternate,
        motion,
    )?;
    let item = match super::super::natives::player::new_item_entity(world, number, "weapon_item") {
        Ok(item) => item,
        Err(message) => {
            FrameWorld::from_world(world).despawn_dropped_item(number);
            return Err(message);
        }
    };
    if world
        .resource::<RoundScript>()
        .can_receive_call(&drop.owner)
    {
        crate::script::runtime::raise(
            world,
            Value::Object(drop.owner),
            "weapon_dropped",
            vec![item],
        );
    }
    Ok(())
}

fn advance_one(world: &mut World, drop: &mut Pending, now: i64) -> Result<bool, String> {
    if !world
        .resource::<RoundScript>()
        .can_receive_call(&drop.owner)
    {
        // Deletion must not swallow an already requested drop. Use the last
        // actual sampled pose, rather than inventing a replacement source.
        release(world, drop, drop.motion)?;
        return Ok(true);
    }
    let mut motion = sample(world, drop.owner, drop.tag)?;
    if !drop.sampled {
        drop.motion = motion;
        drop.time = now;
        drop.sampled = true;
        return Ok(false);
    } else if now > drop.time {
        let seconds = (now - drop.time) as f64 / 1000.0;
        for i in 0..3 {
            motion.velocity[i] =
                ((f64::from(motion.origin[i]) - f64::from(drop.motion.origin[i])) / seconds) as f32;
            let turn = math_iw4::angle_subtract(motion.angles[i], drop.motion.angles[i]);
            motion.angular_velocity[i] = (f64::from(turn) / seconds) as f32;
        }
        if !motion
            .velocity
            .into_iter()
            .chain(motion.angular_velocity)
            .all(f32::is_finite)
        {
            return Err("weapon drop motion is not finite".into());
        }
        release(world, drop, motion)?;
    } else {
        return Ok(false);
    }
    Ok(true)
}

pub(crate) fn advance(world: &mut World, now: i64) -> Result<(), String> {
    let pending = std::mem::take(&mut world.resource_mut::<RoundScript>().weapon_drops);
    let mut pending = pending.into_iter();
    let mut waiting = Vec::new();
    while let Some(mut drop) = pending.next() {
        match advance_one(world, &mut drop, now) {
            Ok(true) => {}
            Ok(false) => waiting.push(drop),
            Err(message) => {
                waiting.push(drop);
                waiting.extend(pending);
                world
                    .resource_mut::<RoundScript>()
                    .weapon_drops
                    .extend(waiting);
                return Err(message);
            }
        }
    }
    world
        .resource_mut::<RoundScript>()
        .weapon_drops
        .extend(waiting);
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "dropweapon", |world, receiver, args| {
        let owner = actor_id(world, receiver)?;
        let name = super::super::args::string(args, 0)?;
        if name.eq_ignore_ascii_case("none") {
            return Ok(Value::Undefined);
        }
        let place = super::super::args::string(args, 1)?;
        let speed = super::super::args::float(args, 2)?;
        if !speed.is_finite() {
            return Err("weapon drop speed must be finite".into());
        }
        let weapon = FrameWorld::from_world(world)
            .weapon_index_by_script_name(&name)
            .ok_or("unknown weapon to drop")?;
        {
            let frame = FrameWorld::from_world(world);
            let facts = frame
                .weapon_combat_row(weapon)
                .ok_or("weapon facts unavailable")?;
            if facts.inventory_type == INVENTORY_ALTERNATE {
                return Err("cannot drop an alternate-mode weapon".into());
            }
            if frame.weapon_item_facts(weapon).is_none() {
                return Err("weapon item ammo facts are unavailable".into());
            }
        }
        let tag = match place.as_str() {
            "left" => Some("tag_weapon_left"),
            "right" => Some("tag_weapon_right"),
            "chest" => Some("tag_weapon_chest"),
            "back" => Some("tag_stowed_back"),
            _ => None,
        };
        let motion = sample(world, owner, tag)?;
        let now = super::super::players::now_ms(world);
        let drop = Pending {
            owner,
            number: world.resource::<RoundScript>().entities[&owner].number,
            weapon,
            tag,
            motion,
            time: now,
            sampled: false,
        };
        let count = world
            .resource::<RoundScript>()
            .weapon_drops
            .iter()
            .filter(|drop| drop.owner == owner)
            .count();
        if tag.is_some() && count < SAMPLED_DROPS_PER_ACTOR {
            world.resource_mut::<RoundScript>().weapon_drops.push(drop);
        } else {
            let mut motion = motion;
            let center = sample(world, owner, None)?.origin;
            let direction = glam::Vec3::from_array(motion.origin) - glam::Vec3::from_array(center);
            let outward = direction.try_normalize().unwrap_or_else(|| {
                glam::Vec3::from_array(math_iw4::angles_to_axis(motion.angles)[0])
            });
            motion.velocity = (outward * speed).to_array();
            release(world, &drop, motion)?;
        }
        Ok(Value::Undefined)
    });
}
