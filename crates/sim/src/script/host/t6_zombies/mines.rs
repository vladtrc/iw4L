use super::*;

fn detected(origin: [f32; 3], forward: [f32; 3], target: [f32; 3]) -> bool {
    let offset = Vec3::from_array(target) - Vec3::from_array(origin);
    if offset.x * offset.x + offset.y * offset.y > 96.0 * 96.0 || offset.z.abs() > 96.0 {
        return false;
    }
    let direction = offset + Vec3::Z * 32.0;
    let forward = Vec3::from_array(forward);
    direction.dot(forward) >= 20.0
        && direction.normalize_or_zero().dot(forward) > 70.0_f32.to_radians().cos()
}

pub(super) fn advance(world: &mut World, state: &Survival, tick: Tick) {
    let frame = FrameWorld::from_world(world);
    let mut mines = Vec::new();
    frame.visit_projectiles(|projectile| {
        if projectile.live && frame.weapon_script_name(projectile.weapon) == "claymore_zm" {
            mines.push(*projectile);
        }
    });
    drop(frame);
    mines.sort_by_key(|mine| mine.id.0);
    let mut counts = BTreeMap::<ClientId, usize>::new();
    for mine in mines {
        let count = counts.entry(mine.owner).or_default();
        *count += 1;
        if mine.detonate_at_ms.is_some() {
            continue;
        }
        if *count > 12 {
            let mut frame = FrameWorld::from_world(world);
            if let Some(projectile) = frame.projectile_mut_by_number(mine.entnum) {
                projectile.detonate_at_ms = Some(crate::level_time_ms(tick).saturating_add(100));
                projectile.detonation_armed = true;
            }
            diag::info!(
                Sim,
                "zombies mine limit exceeded client={} projectile={} delay_ms=100",
                mine.owner.0,
                mine.id.0
            );
            continue;
        }
        if mine.pos.tr_type != entity_iw4::TR_STATIONARY {
            continue;
        }
        let angles = entity_iw4::evaluate_trajectory(&mine.apos, crate::level_time_ms(tick));
        let forward = math_iw4::angle_vectors(angles).0;
        let target = state.actors.iter().find_map(|(&object, actor)| {
            (detected(mine.origin, forward, actor.origin)
                && super::super::natives::engine::damage_visible(world, object, mine.origin))
            .then_some(object)
        });
        if let Some(target) = target {
            let mut frame = FrameWorld::from_world(world);
            if let Some(projectile) = frame.projectile_mut_by_number(mine.entnum) {
                projectile.detonate_at_ms = Some(crate::level_time_ms(tick).saturating_add(400));
                projectile.detonation_armed = true;
            }
            drop(frame);
            let _ = super::super::natives::engine::play_sound_at(
                world,
                mine.origin,
                "wpn_claymore_alert",
            );
            diag::info!(
                Sim,
                "zombies mine triggered projectile={} target={target} delay_ms=400",
                mine.id.0
            );
        }
    }
}

pub(super) fn new_round(world: &mut World) {
    let mut frame = FrameWorld::from_world(world);
    let Some(weapon) = weapon_id(&frame, "claymore_zm") else {
        return;
    };
    for client in frame.client_ids_sorted() {
        if !frame
            .client_meta(client)
            .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
            || !frame
                .player(client)
                .is_some_and(|ps| ps.weapons.contains(&(weapon as i32)))
        {
            continue;
        }
        crate::script_player::set_ammo_clip(&mut frame, client, weapon, 2);
        if let Some(ps) = frame.player_mut(client) {
            ps.action_slot_type[3] = 1;
            ps.action_slot_param[3] = weapon as i32;
        }
        diag::info!(Sim, "zombies mine replenished client={} count=2", client.0);
    }
}
