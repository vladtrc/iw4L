use crate::frame::FrameWorld;
use anim_iw4::bg_random;
use entity_iw4::{TR_GRAVITY, TR_STATIONARY, Trajectory, bg_evaluate_trajectory};
use math_iw4::angle_vectors;
use playerstate_iw4::{ENTITYNUM_NONE, PM_TYPE_DEAD, PlayerState};
use weapon_iw4::{
    bg_ammo_table_key, bg_clip_table_key, bg_get_ammo_not_in_clip, bg_get_clip_for_hand,
    bg_player_weapons_find_slot, bg_set_ammo_not_in_clip, bg_set_clip_for_hand,
};

use crate::bullet_collision::{MASK_PLAYER_SOLID, PLAYER_MAXS, PLAYER_MINS};
use crate::gentity::init_item_state;
use crate::world::{ClientId, Tick, give_weapon_to_ps_akimbo, gsc_give_weapon_is_akimbo};
use crate::{ClientLifecycle, ItemPickupRecord};

pub const G_MAX_DROPPED_WEAPONS: usize = 16;

pub const G_DROP_FORWARD_SPEED: f32 = 10.0;
pub const G_DROP_UP_SPEED_BASE: f32 = 10.0;

pub const G_DROP_UP_SPEED_RAND: f32 = 5.0;

pub const G_DROP_HORZ_SPEED_RAND: f32 = 100.0;

pub const WEAP_INVENTORY_PRIMARY: i32 = 0;

pub const ITEM_MINS: [f32; 3] = [0.0, 0.0, 0.0];
pub const ITEM_MAXS: [f32; 3] = [1.0, 1.0, 1.0];

pub const PLAYER_DROP_Z: f32 = (PLAYER_MAXS[2] - PLAYER_MINS[2]) * 0.5;

pub const SCAVENGER_BAG_SCRIPT: &str = "scavenger_bag_mp";

pub const PERK_SCAVENGER: u32 = 1 << 22;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DroppedItem {
    pub state: entity_iw4::EntityState,
    pub origin: [f32; 3],
    pub falling: bool,
    pub clip_r: i32,
    pub clip_l: i32,
    pub stock: i32,
    pub scavenger: bool,
}

pub fn g_random(seed: &mut u32) -> f32 {
    bg_random(seed) as f32 * (1.0 / 32768.0)
}

pub fn g_crandom(seed: &mut u32) -> f32 {
    let bits = bg_random(seed) as f32;
    let unit = bits * (1.0 / 32768.0);
    unit + unit - 1.0
}

pub fn drop_item_velocity(yaw_deg: f32, seed: &mut u32) -> [f32; 3] {
    let (forward, _, _) = angle_vectors([0.0, yaw_deg, 0.0]);
    let mut velocity = [
        forward[0] * G_DROP_FORWARD_SPEED,
        forward[1] * G_DROP_FORWARD_SPEED,
        forward[2] * G_DROP_FORWARD_SPEED,
    ];
    velocity[2] += g_crandom(seed) * G_DROP_UP_SPEED_RAND + G_DROP_UP_SPEED_BASE;
    velocity[0] += g_crandom(seed) * G_DROP_HORZ_SPEED_RAND;
    velocity[1] += g_crandom(seed) * G_DROP_HORZ_SPEED_RAND;
    velocity
}

fn may_drop_weapon(world: &FrameWorld, ps: &PlayerState, weapon: u32) -> bool {
    if weapon == 0 {
        return false;
    }
    if bg_player_weapons_find_slot(&ps.weapons, weapon as i32) < 0 {
        return false;
    }
    let name = world.weapon_script_name(weapon);
    if name.contains("ac130") {
        return false;
    }
    let Some(facts) = world.combat_facts_for(weapon) else {
        return false;
    };
    if facts.inventory_type != WEAP_INVENTORY_PRIMARY {
        return false;
    }
    if name.contains("riotshield") {
        return true;
    }
    let clip_key = bg_clip_table_key(facts.clip_index, weapon);
    let clip_r = bg_get_clip_for_hand(&ps.ammoclip, clip_key, 0);
    let clip_l = bg_get_clip_for_hand(&ps.ammoclip, clip_key, 1);
    if clip_r == 0 && clip_l == 0 {
        return false;
    }
    let ammo_key = bg_ammo_table_key(facts.ammo_index, weapon);
    let stock = bg_get_ammo_not_in_clip(&ps.ammo, ammo_key);
    clip_r != 0 || clip_l != 0 || stock != 0
}

fn take_player_weapon(ps: &mut PlayerState, weapon: u32) {
    let want = weapon as i32;
    for slot in &mut ps.weapons {
        if *slot == want {
            *slot = 0;
        }
    }
    if ps.weapon == weapon {
        ps.weapon = 0;
    }
}

fn ammo_from_ps(world: &FrameWorld, ps: &PlayerState, weapon: u32) -> (i32, i32, i32) {
    let Some(facts) = world.combat_facts_for(weapon) else {
        return (0, 0, 0);
    };
    let clip_key = bg_clip_table_key(facts.clip_index, weapon);
    let ammo_key = bg_ammo_table_key(facts.ammo_index, weapon);
    (
        bg_get_clip_for_hand(&ps.ammoclip, clip_key, 0),
        bg_get_clip_for_hand(&ps.ammoclip, clip_key, 1),
        bg_get_ammo_not_in_clip(&ps.ammo, ammo_key),
    )
}

fn set_ammo_on_ps(
    world: &FrameWorld,
    ps: &mut PlayerState,
    weapon: u32,
    clip_r: i32,
    clip_l: i32,
    stock: i32,
) {
    let Some(facts) = world.combat_facts_for(weapon) else {
        return;
    };
    let clip_key = bg_clip_table_key(facts.clip_index, weapon);
    let ammo_key = bg_ammo_table_key(facts.ammo_index, weapon);
    if clip_key != 0 {
        let _ = bg_set_clip_for_hand(&mut ps.ammoclip, clip_key, 0, clip_r);
        let _ = bg_set_clip_for_hand(&mut ps.ammoclip, clip_key, 1, clip_l);
    }
    if ammo_key != 0 {
        let _ = bg_set_ammo_not_in_clip(&mut ps.ammo, ammo_key, stock);
    }
}

fn add_ammo_on_ps(
    world: &FrameWorld,
    ps: &mut PlayerState,
    weapon: u32,
    clip_r: i32,
    clip_l: i32,
    stock: i32,
) {
    let (have_r, have_l, have_stock) = ammo_from_ps(world, ps, weapon);

    let Some(facts) = world.combat_facts_for(weapon) else {
        return;
    };
    set_ammo_on_ps(
        world,
        ps,
        weapon,
        have_r,
        have_l,
        have_stock
            .saturating_add(stock)
            .saturating_add(clip_r)
            .saturating_add(clip_l)
            .min(facts.max_ammo),
    );
}

fn current_primary_weapon(world: &FrameWorld, ps: &PlayerState) -> u32 {
    let weapon = ps.weapon;
    if weapon == 0 {
        return 0;
    }
    if bg_player_weapons_find_slot(&ps.weapons, weapon as i32) < 0 {
        return 0;
    }
    match world.combat_facts_for(weapon) {
        Some(facts) if facts.inventory_type == WEAP_INVENTORY_PRIMARY => weapon,
        _ => 0,
    }
}

fn aabb_overlap(
    a_origin: [f32; 3],
    a_mins: [f32; 3],
    a_maxs: [f32; 3],
    b_origin: [f32; 3],
    b_mins: [f32; 3],
    b_maxs: [f32; 3],
) -> bool {
    for i in 0..3 {
        let a_min = a_origin[i] + a_mins[i];
        let a_max = a_origin[i] + a_maxs[i];
        let b_min = b_origin[i] + b_mins[i];
        let b_max = b_origin[i] + b_maxs[i];
        if a_min > b_max || a_max < b_min {
            return false;
        }
    }
    true
}

fn walker_can_touch(world: &FrameWorld, id: ClientId, ps: &PlayerState) -> bool {
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return false;
    }
    if ps.health < 1 {
        return false;
    }
    if ps.pm_type >= PM_TYPE_DEAD {
        return false;
    }
    true
}

fn has_scavenger_perk(ps: &PlayerState) -> bool {
    (ps.perks[0] & PERK_SCAVENGER) != 0
}

fn push_dropped_item(
    world: &mut FrameWorld,
    weapon: u32,
    origin: [f32; 3],
    pos: Trajectory,
    apos: Trajectory,
    owner: i32,
    clip_r: i32,
    clip_l: i32,
    stock: i32,
    falling: bool,
    scavenger: bool,
) -> i32 {
    if world.dropped_item_count() >= G_MAX_DROPPED_WEAPONS {
        let evicted_number = world
            .dropped_item_numbers_sorted()
            .into_iter()
            .next()
            .expect("cap eviction requires an occupied dropped item");
        let evicted = world
            .remove_dropped_item_by_number(evicted_number)
            .expect("cap eviction number vanished");
        world.free_dynamic_entity_number(evicted.state.number);
    }
    let entnum = world
        .allocate_dynamic_entity(crate::gentity::EntityRunKind::Item)
        .expect("G_Spawn exhausted dynamic entity slots for dropped item")
        .number();
    let state = init_item_state(entnum, weapon, pos, apos, owner);
    world.push_dropped_item(DroppedItem {
        state,
        origin,
        falling,
        clip_r,
        clip_l,
        stock,
        scavenger,
    });
    entnum
}

fn launch_dropped_from_ps(
    world: &mut FrameWorld,
    tick: Tick,
    ps: &PlayerState,
    owner: i32,
    weapon: u32,
    clip_r: i32,
    clip_l: i32,
    stock: i32,
    scavenger: bool,
) -> i32 {
    let mut seed = world.anim_event_seed();
    let velocity = drop_item_velocity(ps.viewangles[1], &mut seed);
    world.set_anim_event_seed(seed);

    let origin = [ps.origin[0], ps.origin[1], ps.origin[2] + PLAYER_DROP_Z];
    let time_ms = crate::corpse::level_time_ms(tick);
    let pos = Trajectory {
        tr_type: TR_GRAVITY,
        tr_time: time_ms,
        tr_duration: 0,
        tr_delta: velocity,
        tr_base: origin,
    };
    let apos = Trajectory {
        tr_type: TR_STATIONARY,
        tr_time: 0,
        tr_duration: 0,
        tr_delta: [0.0; 3],
        tr_base: [0.0, ps.viewangles[1], 0.0],
    };
    push_dropped_item(
        world, weapon, origin, pos, apos, owner, clip_r, clip_l, stock, true, scavenger,
    )
}

pub(crate) fn try_drop_scavenger_for_death(
    world: &mut FrameWorld,
    tick: Tick,
    victim: ClientId,
    attacker: Option<ClientId>,
) {
    let Some(attacker) = attacker else {
        return;
    };
    if attacker == victim {
        return;
    }
    let Some(weapon) = world.weapon_index_by_script_name(SCAVENGER_BAG_SCRIPT) else {
        return;
    };
    let Some(ps) = world.player(victim).copied() else {
        return;
    };
    let _ = launch_dropped_from_ps(world, tick, &ps, victim.0 as i32, weapon, 0, 0, 0, true);
}

pub(crate) fn try_drop_weapon_for_death(world: &mut FrameWorld, tick: Tick, victim: ClientId) {
    let Some(ps) = world.player(victim).copied() else {
        return;
    };
    let weapon = ps.weapon;
    if !may_drop_weapon(world, &ps, weapon) {
        return;
    }
    let (clip_r, clip_l, stock) = ammo_from_ps(world, &ps, weapon);
    if launch_dropped_from_ps(
        world,
        tick,
        &ps,
        victim.0 as i32,
        weapon,
        clip_r,
        clip_l,
        stock,
        false,
    ) == ENTITYNUM_NONE
    {
        return;
    }
    if let Some(ps) = world.player_mut(victim) {
        take_player_weapon(ps, weapon);
    }
}

pub(crate) fn think_item_move(world: &mut FrameWorld, time_ms: i32, number: i32) {
    if let Ok(entity) = world.entity_kernel().current_ref(number) {
        if world
            .entity_kernel()
            .resolve(entity)
            .is_ok_and(|view| view.relations.parent.is_some())
        {
            return;
        }
    }
    let Some(item) = world.dropped_item_by_number(number) else {
        return;
    };
    if !item.falling {
        return;
    }
    let traj = Trajectory {
        tr_time: item.state.tr_time,
        tr_type: item.state.tr_type,
        tr_duration: item.state.tr_duration,
        tr_delta: item.state.tr_delta,
        tr_base: item.state.tr_base,
    };
    let desired = bg_evaluate_trajectory(&traj, time_ms);
    let hit = world.trace_clip(
        item.origin,
        desired,
        ITEM_MINS,
        ITEM_MAXS,
        MASK_PLAYER_SOLID,
    );
    let mut fraction = hit.fraction;
    if hit.startsolid != 0 {
        fraction = 0.0;
    }
    let endpos = if fraction >= 1.0 { desired } else { hit.endpos };
    let Some(item) = world.dropped_item_mut_by_number(number) else {
        return;
    };
    item.origin = endpos;
    if fraction >= 1.0 {
        return;
    }
    if hit.allsolid != 0 || hit.normal[2] > 0.0 {
        item.falling = false;
        item.state.tr_type = TR_STATIONARY;
        item.state.tr_base = endpos;
        item.state.tr_delta = [0.0; 3];
        item.state.tr_time = 0;
        item.state.tr_duration = 0;
    }
}

pub(crate) fn phase_touch_items(world: &mut FrameWorld, _tick: Tick) {
    let mut walkers = world.client_ids_sorted();
    walkers.sort_unstable_by_key(|id| id.0);
    for walker in walkers {
        try_touch_one(world, walker);
    }
}

fn try_touch_one(world: &mut FrameWorld, walker: ClientId) {
    let Some(ps) = world.player(walker).copied() else {
        return;
    };
    if !walker_can_touch(world, walker, &ps) {
        return;
    }
    let candidates = world.dropped_item_numbers_sorted();
    let mut hit = None;
    for number in candidates {
        let Some(item) = world.dropped_item_by_number(number) else {
            continue;
        };
        if !aabb_overlap(
            ps.origin,
            PLAYER_MINS,
            PLAYER_MAXS,
            item.origin,
            ITEM_MINS,
            ITEM_MAXS,
        ) {
            continue;
        }

        if !item.scavenger && !ps.weapons.contains(&item.state.index) {
            continue;
        }
        if !item.scavenger {
            let weapon = item.state.index as u32;
            let Some(facts) = world.combat_facts_for(weapon) else {
                continue;
            };
            let (_, _, stock) = ammo_from_ps(world, &ps, weapon);
            if stock >= facts.max_ammo || item.stock + item.clip_r + item.clip_l <= 0 {
                continue;
            }
        }
        if item.scavenger && !has_scavenger_perk(&ps) {
            continue;
        }
        hit = Some(number);
        break;
    }
    let Some(number) = hit else {
        return;
    };
    grab_number(world, walker, number);
}

fn grab_number(world: &mut FrameWorld, walker: ClientId, number: i32) {
    let Some(item) = world.dropped_item_by_number(number) else {
        return;
    };
    if item.scavenger {
        grab_scavenger(world, walker, number);
        return;
    }
    let weapon = u32::try_from(item.state.index).unwrap_or(0);
    if weapon == 0 {
        return;
    }
    let Some(ps) = world.player(walker).copied() else {
        return;
    };
    let picker_pm_type = ps.pm_type;
    let already_has = bg_player_weapons_find_slot(&ps.weapons, weapon as i32) >= 0;
    world.remove_dropped_item_by_number(number);
    world.free_dynamic_entity_number(item.state.number);
    let mut swapped_entnum = ENTITYNUM_NONE;
    let akimbo = gsc_give_weapon_is_akimbo(world.weapon_script_name(weapon));
    if already_has {
        let mut next = ps;
        weapon_iw4::bg_latch_weapon_dual_wield(
            &next.weapons,
            &mut next.weapon_data,
            weapon,
            akimbo,
        );
        if next.weapon == weapon {
            next.last_weapon_hand =
                weapon_iw4::pm_num_hands_for_held(&next.weapons, &next.weapon_data, weapon);
        }
        add_ammo_on_ps(
            world,
            &mut next,
            weapon,
            item.clip_r,
            item.clip_l,
            item.stock,
        );
        if let Some(slot) = world.player_mut(walker) {
            *slot = next;
        }
    } else {
        let current = current_primary_weapon(world, &ps);
        if current != 0 && primary_count(world, &ps) >= 2 {
            swapped_entnum = drop_current_primary_at(
                world,
                walker,
                current,
                item.origin,
                item.state.apos_tr_base,
            );
        }
        if let Some(mut next) = world.player(walker).copied() {
            give_weapon_to_ps_akimbo(&mut next, weapon, akimbo);

            if let Some(facts) = world.combat_facts_for(weapon) {
                let hand = weapon_iw4::spawn_weapon_hand(weapon, &facts);
                next.weaponstate_primary = hand.weaponstate;
                next.weapon_time = hand.weapon_time;
                next.weapon_delay = hand.weapon_delay;
                next.weap_anim = hand.weap_anim;
                next.weaponstate_secondary = hand.weaponstate;
                next.weapon_time_secondary = hand.weapon_time;
                next.weapon_delay_secondary = hand.weapon_delay;
                next.weap_anim_secondary = hand.weap_anim;
            }
            set_ammo_on_ps(
                world,
                &mut next,
                weapon,
                item.clip_r,
                item.clip_l,
                item.stock,
            );
            if let Some(slot) = world.player_mut(walker) {
                *slot = next;
            }
        }
    }
    let next = world.player(walker).copied().expect("picker exists");
    let (clip, _, stock) = ammo_from_ps(world, &next, weapon);
    {
        let meta = world.client_meta_mut(walker);
        meta.ammo_by_weapon
            .retain(|(id, _, _)| next.weapons.contains(&(*id as i32)));
        meta.set_ammo(weapon, clip, stock);
        meta.mirror_held_ammo(next.weapon);
    }
    if !already_has {
        let meta = world.client_meta_mut(walker);
        meta.weapon_shot_count = 0;
        meta.burst_latch = false;
        meta.rechamber_pending = false;
    }
    world.item_pickups_mut().push(ItemPickupRecord {
        picker: walker.0 as i32,
        weapon,
        from_entnum: item.state.number,
        clip_r: item.clip_r,
        clip_l: item.clip_l,
        stock: item.stock,
        swapped_entnum,
        picker_pm_type,
    });
    let tick = Tick((world.entity_kernel().level_time_ms() / 50) as u32);
    world.push_entity_event(
        tick,
        crate::EventAudience::All,
        entity_iw4::EntityEventKind::ITEM_PICKUP,
        crate::EntityEventPayload {
            number: walker.0 as i32,
            event_parm: weapon as i32,
            weapon,
            origin: ps.origin,
            ..Default::default()
        },
    );
    perf::pickup(picker_pm_type);
}

fn grab_scavenger(world: &mut FrameWorld, walker: ClientId, number: i32) {
    let Some(item) = world.dropped_item_by_number(number) else {
        return;
    };
    let Some(ps) = world.player(walker).copied() else {
        return;
    };
    if !has_scavenger_perk(&ps) {
        return;
    }
    let picker_pm_type = ps.pm_type;
    let weapon = u32::try_from(item.state.index).unwrap_or(0);
    world.remove_dropped_item_by_number(number);
    world.free_dynamic_entity_number(item.state.number);
    let mut next = ps;
    apply_scavenger_stock(world, &mut next);
    if let Some(slot) = world.player_mut(walker) {
        *slot = next;
    }
    world.item_pickups_mut().push(ItemPickupRecord {
        picker: walker.0 as i32,
        weapon,
        from_entnum: item.state.number,
        clip_r: 0,
        clip_l: 0,
        stock: 0,
        swapped_entnum: ENTITYNUM_NONE,
        picker_pm_type,
    });
    perf::pickup(picker_pm_type);
}

fn apply_scavenger_stock(world: &FrameWorld, ps: &mut PlayerState) {
    let held: Vec<u32> = ps
        .weapons
        .iter()
        .copied()
        .filter(|&w| w > 0)
        .map(|w| w as u32)
        .collect();
    for weapon in held {
        let Some(facts) = world.combat_facts_for(weapon) else {
            continue;
        };
        if facts.inventory_type != WEAP_INVENTORY_PRIMARY {
            continue;
        }
        let (clip_r, clip_l, stock) = ammo_from_ps(world, ps, weapon);
        set_ammo_on_ps(
            world,
            ps,
            weapon,
            clip_r,
            clip_l,
            stock + facts.clip_size.max(0),
        );
    }
}

fn drop_current_primary_at(
    world: &mut FrameWorld,
    walker: ClientId,
    weapon: u32,
    origin: [f32; 3],
    angles: [f32; 3],
) -> i32 {
    let Some(ps) = world.player(walker).copied() else {
        return ENTITYNUM_NONE;
    };
    let (clip_r, clip_l, stock) = ammo_from_ps(world, &ps, weapon);
    let pos = Trajectory {
        tr_type: TR_STATIONARY,
        tr_time: 0,
        tr_duration: 0,
        tr_delta: [0.0; 3],
        tr_base: origin,
    };
    let apos = Trajectory {
        tr_type: TR_STATIONARY,
        tr_time: 0,
        tr_duration: 0,
        tr_delta: [0.0; 3],
        tr_base: angles,
    };
    let entnum = push_dropped_item(
        world,
        weapon,
        origin,
        pos,
        apos,
        walker.0 as i32,
        clip_r,
        clip_l,
        stock,
        false,
        false,
    );
    if entnum == ENTITYNUM_NONE {
        return ENTITYNUM_NONE;
    }
    if let Some(ps_mut) = world.player_mut(walker) {
        take_player_weapon(ps_mut, weapon);
    }
    entnum
}

fn primary_count(world: &FrameWorld, ps: &PlayerState) -> usize {
    ps.weapons
        .iter()
        .filter(|&&w| {
            w > 0
                && world
                    .combat_facts_for(w as u32)
                    .is_some_and(|f| f.inventory_type == WEAP_INVENTORY_PRIMARY)
        })
        .count()
}

fn selected_item(world: &FrameWorld, walker: ClientId, ps: &PlayerState) -> Option<DroppedItem> {
    if !walker_can_touch(world, walker, ps)
        || ps.pm_flags & (4 | 0x4000) != 0
        || (16..=20).contains(&ps.weaponstate_primary)
    {
        return None;
    }

    if world
        .map_doors
        .as_ref()
        .is_some_and(|d| d.hints.contains(&walker))
        || world.use_hold().is_some_and(|h| h.client == walker)
        || world
            .objectives
            .bombs
            .iter()
            .any(|b| !b.destroyed && b.view.users.contains(&walker))
    {
        return None;
    }
    let eye = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let (forward, _, _) = angle_vectors(ps.viewangles);
    let mut best: Option<(f32, DroppedItem)> = None;
    for number in world.dropped_item_numbers_sorted() {
        let Some(item) = world.dropped_item_by_number(number) else {
            continue;
        };
        if item.scavenger || item.state.index <= 0 || ps.weapons.contains(&item.state.index) {
            continue;
        }

        if primary_count(world, ps) >= 2 && current_primary_weapon(world, ps) == 0 {
            continue;
        }
        let delta: [f32; 3] = core::array::from_fn(|i| item.origin[i] + 0.5 - eye[i]);
        let distance = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
        if distance > 128.0 {
            continue;
        }
        let dot = if distance > 0.0 {
            (0..3).map(|i| forward[i] * delta[i] / distance).sum()
        } else {
            0.0
        };
        if world
            .trace_world(eye, item.origin, [0.0; 3], [0.0; 3], 0x11)
            .fraction
            < 1.0
        {
            continue;
        }
        let score = distance + (1.0 - (dot + 1.0) * 0.5) * 256.0;
        if best.as_ref().is_none_or(|(old, _)| score < *old) {
            best = Some((score, item));
        }
    }
    best.map(|(_, item)| item)
}

pub(crate) fn phase_use_items(
    world: &mut FrameWorld,
    tick: Tick,
    presses: &[crate::UsePress],
    cmds: &[(u32, u32)],
) {
    let now = crate::corpse::level_time_ms(tick);
    for id in world.client_ids_sorted() {
        let Some(ps) = world.player(id).copied() else {
            continue;
        };
        let selected = selected_item(world, id, &ps);
        let held = cmds.iter().any(|(client, bits)| {
            *client == id.0
                && bits & (playerstate_iw4::buttons::USE | playerstate_iw4::buttons::USE_RELOAD)
                    != 0
        });
        let pressed = presses.iter().any(|p| p.client == id.0 && p.edge);
        let selected_ref = selected.map(|item| {
            world
                .entity_kernel()
                .current_ref(item.state.number)
                .expect("occupied item")
        });
        let meta = world.client_meta_mut(id);
        if !held || selected.is_none() {
            meta.item_use_entity = None;
        }
        if pressed {
            meta.item_use_entity = selected_ref;
        }
        let pending = meta.item_use_entity;
        let ready = now - meta.item_use_spawn_ms >= 500;
        if held && ready && pending.is_some() && selected_ref == pending {
            grab_number(world, id, pending.expect("pending item").number());
            world.client_meta_mut(id).item_use_entity = None;
        }
        let selected = selected_item(
            world,
            id,
            &world.player(id).copied().expect("client exists"),
        );
        let dual = selected.is_some_and(|item| {
            gsc_give_weapon_is_akimbo(world.weapon_script_name(item.state.index as u32))
        });
        if let Some(ps) = world.player_mut(id) {
            ps.cursor_hint = selected.map_or(0, |item| item.state.index + 4);
            ps.cursor_hint_ent_index = selected.map_or(ENTITYNUM_NONE, |item| item.state.number);
            ps.cursor_hint_string = -1;
            ps.cursor_hint_dual_wield = i32::from(dual);
        }
    }
}
