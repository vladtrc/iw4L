use super::*;

pub(in crate::script::host) fn toggle(
    world: &mut World,
    object: u64,
    client: ClientId,
    player: u64,
) {
    let runtime = world.resource::<RoundScript>();
    let turret = &runtime.engine.turrets[&object];
    let mounted = turret.active_user == Some(player);
    if !mounted
        && (turret.active_user.is_some()
            || !turret.operable
            || turret.carried
            || turret.mode.as_ref() != "manual")
    {
        return;
    }
    let number = runtime.entities[&object].number;
    let distance = turret.player_position_dist;
    let base = field_vector(world, object, "angles");
    let pivot = field_vector(world, object, "origin");
    let mut frame = FrameWorld::from_world(world);
    let Some(ps) = frame.player_mut(client) else {
        return;
    };
    if mounted {
        ps.e_flags &= !playerstate_iw4::eflags::TURRET_ACTIVE_DUCK;
        ps.viewlocked = 0;
        ps.viewlocked_ent_num = playerstate_iw4::ENTITYNUM_NONE;
    } else {
        ps.e_flags |= playerstate_iw4::eflags::TURRET_ACTIVE_DUCK;
        let forward = math_iw4::angles_to_axis([0., base[1], 0.])[0];
        for axis in 0..2 {
            ps.origin[axis] = pivot[axis] - distance * forward[axis];
            ps.delta_angles[axis] += math_iw4::angle_subtract(base[axis], ps.viewangles[axis]);
            ps.viewangles[axis] = base[axis];
        }
        ps.viewlocked = 1;
        ps.viewlocked_ent_num = number;
        ps.velocity = [0.; 3];
    }
    let turret = world
        .resource_mut::<RoundScript>()
        .into_inner()
        .engine
        .turrets
        .get_mut(&object)
        .unwrap();
    turret.active_user = (!mounted).then_some(player);
    turret.manual_firing = false;
    turret.firing = false;
    raise(
        world,
        Value::Object(object),
        if mounted {
            "turret_deactivate"
        } else {
            "trigger"
        },
        vec![Value::Object(player)],
    );
}

pub(super) fn advance(world: &mut World, object: u64, turret: &mut Turret, now: i32) -> bool {
    let Some(player) = turret.active_user else {
        return false;
    };
    let Some(client) = world
        .resource::<RoundScript>()
        .player_client(player)
        .map(ClientId)
    else {
        return false;
    };
    let frame = FrameWorld::from_world(world);
    if !frame
        .client_meta(client)
        .is_some_and(|m| m.lifecycle == crate::ClientLifecycle::Alive)
    {
        toggle(world, object, client, player);
        return true;
    }
    let Some(ps) = frame.player(client) else {
        return false;
    };
    let mut angles = ps.viewangles;
    let view = angles;
    let origin = ps.origin;
    let controls = frame.client_meta(client).unwrap().controls;
    let demand = world
        .resource::<RoundScript>()
        .players
        .get(&client.0)
        .is_some_and(|p| p.turret_attack_held)
        && turret.fire_enabled
        && !controls.frozen
        && !controls.weapons_disabled;
    let base = field_vector(world, object, "angles");
    for (axis, lo, hi) in [
        (0, turret.arcs[2], turret.arcs[3]),
        (1, turret.arcs[1], turret.arcs[0]),
    ] {
        angles[axis] =
            base[axis] + math_iw4::angle_subtract(angles[axis], base[axis]).clamp(-lo, hi);
    }
    let pivot = field_vector(world, object, "origin");
    let forward = math_iw4::angles_to_axis([0., angles[1], 0.])[0];
    let desired = [
        pivot[0] - turret.player_position_dist * forward[0],
        pivot[1] - turret.player_position_dist * forward[1],
        origin[2],
    ];
    let mut frame = FrameWorld::from_world(world);
    let position = if origin == desired {
        origin
    } else {
        let trace = frame.trace_world(
            origin,
            desired,
            crate::bullet_collision::PLAYER_MINS,
            crate::bullet_collision::PLAYER_MAXS,
            crate::bullet_collision::MASK_PLAYER_SOLID,
        );
        if trace.startsolid != 0 || trace.allsolid != 0 {
            origin
        } else {
            std::array::from_fn(|i| origin[i] + (desired[i] - origin[i]) * trace.fraction)
        }
    };
    if let Some(ps) = frame.player_mut(client) {
        ps.origin = position;
        for axis in 0..2 {
            ps.delta_angles[axis] += math_iw4::angle_subtract(angles[axis], view[axis]);
        }
        ps.viewangles[..2].copy_from_slice(&angles[..2]);
    }
    turret.aim = math_iw4::angles_to_axis(angles)[0];
    turret.aim_initialized = true;
    turret.manual_firing = demand;
    turret.spin.advance(i64::from(now), demand);
    turret.firing = demand && turret.spin.at_speed();
    let firing = turret.firing;
    world
        .resource_mut::<RoundScript>()
        .engine
        .turrets
        .insert(object, turret.clone());
    if firing && let Err(error) = shoot(world, object) {
        diag::warn!(Sim, "player turret shot failed: {error}");
    }
    true
}
