use bevy::prelude::*;
use playerstate_iw4::PlayerState;
use sim::{ClientSnapshotMeta, MatchPhase, Tick};

#[derive(Component)]
pub struct PredictedItem {
    pub item: sim::DroppedItem,
    pub present: bool,
    pub parented: bool,
}

pub struct PredictionScene<'a> {
    pub level: &'a super::level::ClientLevel,
    pub collision: &'a sim::CollisionScene,
    pub bodies: Vec<sim::bg::player_collision::PlayerBodyClip>,
    pub items: Vec<&'a mut PredictedItem>,
}

impl PredictionScene<'_> {
    pub fn advance_items(
        &mut self,
        ps: &mut PlayerState,
        meta: &mut ClientSnapshotMeta,
        tick: Tick,
        phase: MatchPhase,
    ) {
        if phase != MatchPhase::Playing {
            return;
        }
        let time = sim::level_time_ms(tick);
        for row in &mut self.items {
            if row.present && !row.parented {
                sim::advance_item(&mut row.item, time, |a, b, mins, maxs, mask| {
                    self.collision.trace_world(a, b, mins, maxs, mask)
                });
            }
        }
        if meta.lifecycle != sim::ClientLifecycle::Alive
            || ps.health < 1
            || ps.pm_type >= playerstate_iw4::PM_TYPE_DEAD
        {
            return;
        }
        self.items.sort_unstable_by_key(|row| row.item.state.number);
        for row in &mut self.items {
            let item = &mut row.item;
            if !row.present
                || !(0..3).all(|axis| {
                    ps.origin[axis] + sim::PLAYER_MINS[axis] <= item.origin[axis] + 1.0
                        && ps.origin[axis] + sim::PLAYER_MAXS[axis] >= item.origin[axis]
                })
            {
                continue;
            }
            if item.scavenger {
                if ps.perks[0] & playerstate_iw4::PERK_SCAVENGER == 0 {
                    continue;
                }
                row.present = false;
                break;
            }
            let weapon = item.state.index as u32;
            if !ps.weapons.contains(&item.state.index) {
                continue;
            }
            let Some(facts) = self.level.content.weapon_combat_facts(weapon) else {
                continue;
            };
            let clip_key = weapon_iw4::clip_table_key(facts.clip_index, weapon);
            let ammo_key = weapon_iw4::ammo_table_key(facts.ammo_index, weapon);
            let have_stock = weapon_iw4::get_ammo_not_in_clip(&ps.ammo, ammo_key);
            if have_stock >= facts.max_ammo || item.stock + item.clip_r + item.clip_l <= 0 {
                continue;
            }
            let mut room = facts.max_ammo.saturating_sub(have_stock).max(0);
            let mut take = |have: i32| {
                let from = have.max(0).min(room);
                room -= from;
                (from, have - from)
            };
            let (stock, left_stock) = take(item.stock);
            let (right, left_right) = take(item.clip_r);
            let (left, left_left) = take(item.clip_l);
            item.stock = left_stock;
            item.clip_r = left_right;
            item.clip_l = left_left;
            row.present = [left_stock, left_right, left_left]
                .into_iter()
                .any(|ammo| ammo > 0);
            let name = self.level.content.weapon_script_name(weapon);
            let akimbo = facts.dual_wield || name.contains("_akimbo");
            weapon_iw4::pickup_owned_ammo(ps, weapon, Some(&facts), akimbo, stock + right + left);
            meta.weapon_carry.sync_pickup_ammo(
                ps,
                weapon,
                weapon_iw4::get_clip_for_hand(&ps.ammoclip, clip_key, 0),
                weapon_iw4::get_ammo_not_in_clip(&ps.ammo, ammo_key),
            );
            break;
        }
    }
}

pub fn player_bodies(
    entities: &Query<(&crate::CEntity, &crate::CEntityRuntime)>,
    snapshot: Option<&sim::Snapshot>,
) -> Vec<sim::bg::player_collision::PlayerBodyClip> {
    let mut bodies: Vec<_> = entities
        .iter()
        .filter(|(_, runtime)| runtime.next_state.e_type == entity_iw4::ET_PLAYER)
        .filter(|(identity, _)| {
            identity
                .client()
                .and_then(|id| snapshot.and_then(|snapshot| snapshot.meta.for_client(id)))
                .is_some_and(|meta| meta.lifecycle == sim::ClientLifecycle::Alive)
        })
        .map(
            |(identity, runtime)| sim::bg::player_collision::PlayerBodyClip {
                entnum: identity.number(),
                origin: runtime.next_state.tr_base,
            },
        )
        .collect();
    bodies.sort_unstable_by_key(|body| body.entnum);
    bodies
}
