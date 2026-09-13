use std::sync::Arc;

use anim_iw4::{
    ANIM_COND_AKIMBO, ANIM_COND_PLAYERANIMTYPE, ANIM_COND_PLAYERANIMTYPEPRIMARY,
    ANIM_COND_WEAPON_POSITION, ANIM_COND_WEAPONCLASS, ANIM_ET_DEATH, PLAYER_ANIM_INDEX_MASK,
    PlayerAnimValue, anim_weapon_position_from_pm_flags, bg_random,
};
use playerstate_iw4::{PlayerState, bg_get_viewmodel_weapon_index};
use weapon_iw4::WeaponCombatFacts;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimScriptCommand {
    pub body_part: u8,
    pub anim_index: u16,

    pub duration_ms: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimScriptCondition {
    pub index: u8,
    pub bitflags: bool,
    pub bits: u64,
    pub value: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimScriptItem {
    pub skip: bool,
    pub conditions: Vec<AnimScriptCondition>,
    pub commands: Vec<AnimScriptCommand>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimConditions {
    pub written: u32,
    pub value: [u32; 18],
}

impl AnimConditions {
    pub fn set_value(&mut self, cond: u8, v: u32) {
        if (cond as usize) >= 18 {
            return;
        }
        self.written |= 1 << cond;
        self.value[cond as usize] = v;
    }

    pub fn set_bit(&mut self, cond: u8, bit: u8) {
        if (cond as usize) >= 18 || bit >= 64 {
            return;
        }
        self.written |= 1 << cond;
        self.value[cond as usize] |= 1 << bit;
    }
}

pub fn anim_conditions_from_pmove(
    ps: &PlayerState,
    view_facts: Option<WeaponCombatFacts>,
    primary_facts: Option<WeaponCombatFacts>,
) -> AnimConditions {
    let mut conds = AnimConditions::default();
    if let Some(facts) = view_facts {
        conds.set_bit(ANIM_COND_PLAYERANIMTYPE, bit_index(facts.player_anim_type));
        conds.set_bit(ANIM_COND_WEAPONCLASS, bit_index(facts.weap_class));
    }
    if let Some(facts) = primary_facts {
        conds.set_bit(
            ANIM_COND_PLAYERANIMTYPEPRIMARY,
            bit_index(facts.player_anim_type),
        );
    }
    conds.set_value(ANIM_COND_AKIMBO, u32::from(ps.last_weapon_hand == 1));
    conds.set_value(
        ANIM_COND_WEAPON_POSITION,
        anim_weapon_position_from_pm_flags(ps.pm_flags),
    );
    conds
}

fn bit_index(v: i32) -> u8 {
    u8::try_from(v).unwrap_or(u8::MAX)
}

pub fn pmove_anim_weapon_ids(ps: &PlayerState) -> (u32, u32) {
    (bg_get_viewmodel_weapon_index(ps), ps.weapon)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerAnimScript {
    slots: Vec<(u8, u8, Vec<AnimScriptItem>)>,
    events: Vec<(u8, Vec<AnimScriptItem>)>,
}

impl PlayerAnimScript {
    pub fn from_tables(
        slots: Vec<(u8, u8, Vec<AnimScriptItem>)>,
        events: Vec<(u8, Vec<AnimScriptItem>)>,
    ) -> Arc<Self> {
        Arc::new(Self { slots, events })
    }

    pub fn event_item_count(&self) -> usize {
        self.events.iter().map(|(_, items)| items.len()).sum()
    }

    fn items(&self, state: u8, movetype: u8) -> &[AnimScriptItem] {
        self.slots
            .iter()
            .find(|(s, m, _)| *s == state && *m == movetype)
            .map(|(_, _, items)| items.as_slice())
            .unwrap_or(&[])
    }

    fn event_items(&self, event: u8) -> &[AnimScriptItem] {
        self.events
            .iter()
            .find(|(e, _)| *e == event)
            .map(|(_, items)| items.as_slice())
            .unwrap_or(&[])
    }

    pub fn matches_movetype(&self, movetype: u8, conditions: &AnimConditions) -> bool {
        self.items(0, movetype)
            .iter()
            .any(|item| !item.skip && !item.commands.is_empty() && item_matches(item, conditions))
    }

    pub fn apply(
        &self,
        ps: &mut PlayerState,
        movetype: u8,
        client_num: u32,
        conditions: &AnimConditions,
    ) -> bool {
        if ps.pm_type >= 8 {
            return false;
        }
        if self.apply_movetype(ps, movetype, client_num, conditions) {
            return true;
        }
        if movetype != 1 {
            return self.apply_movetype(ps, 1, client_num, conditions);
        }
        false
    }

    pub fn apply_event(
        &self,
        ps: &mut PlayerState,
        event: u8,
        conditions: &AnimConditions,
        seed: &mut u32,
    ) -> bool {
        if event != ANIM_ET_DEATH && ps.pm_type >= 8 {
            return false;
        }
        if self.event_items(event).is_empty() {
            return false;
        }
        for item in self.event_items(event) {
            if item.skip || item.commands.is_empty() {
                continue;
            }
            if !item_matches(item, conditions) {
                continue;
            }
            let n = item.commands.len();
            let pick = (bg_random(seed) as usize) % n;
            let cmd = item.commands[pick];
            execute_command(ps, cmd, true, false, true);
            return true;
        }
        false
    }

    fn apply_movetype(
        &self,
        ps: &mut PlayerState,
        movetype: u8,
        client_num: u32,
        conditions: &AnimConditions,
    ) -> bool {
        for item in self.items(0, movetype) {
            if item.skip || item.commands.is_empty() {
                continue;
            }
            if !item_matches(item, conditions) {
                continue;
            }
            let n = item.commands.len();
            let cmd = item.commands[client_num as usize % n];
            execute_command(ps, cmd, false, true, false);
            return true;
        }
        false
    }
}

fn item_matches(item: &AnimScriptItem, conditions: &AnimConditions) -> bool {
    for cond in &item.conditions {
        let idx = cond.index as usize;
        if idx >= 18 {
            return false;
        }
        if cond.bitflags {
            if (cond.bits & u64::from(conditions.value[idx])) == 0 {
                return false;
            }
        } else {
            if conditions.written & (1 << cond.index) == 0 {
                return false;
            }
            if conditions.value[idx] as i32 != cond.value {
                return false;
            }
        }
    }
    true
}

fn execute_command(
    ps: &mut PlayerState,
    cmd: AnimScriptCommand,
    set_timer: bool,
    is_continue: bool,
    force: bool,
) {
    let duration = cmd.duration_ms.saturating_add(50);
    if cmd.body_part == 1 || cmd.body_part == 3 {
        play_anim(
            ps,
            cmd.anim_index,
            true,
            duration,
            set_timer,
            is_continue,
            force,
        );
    }
    if cmd.body_part == 2 {
        play_anim(
            ps,
            cmd.anim_index,
            false,
            duration,
            set_timer,
            is_continue,
            force,
        );
    } else if cmd.body_part == 3 {
        play_anim(ps, 0, false, duration, set_timer, is_continue, force);
    }
}

fn play_anim(
    ps: &mut PlayerState,
    anim_num: u16,
    legs: bool,
    duration: i32,
    set_timer: bool,
    is_continue: bool,
    force: bool,
) {
    let (timer, raw) = if legs {
        (ps.legs_timer, ps.legs_anim as u16)
    } else {
        (ps.torso_timer, ps.torso_anim as u16)
    };
    if timer >= 50 && !force {
        return;
    }
    let same = (raw & PLAYER_ANIM_INDEX_MASK) == (anim_num & PLAYER_ANIM_INDEX_MASK);
    if is_continue && same {
        if set_timer {
            if legs {
                ps.legs_timer = duration;
            } else {
                ps.torso_timer = duration;
            }
        }
        return;
    }
    let written = PlayerAnimValue::restarting(anim_num, raw).raw() as i32;
    if legs {
        ps.legs_anim = written;
        if set_timer {
            ps.legs_timer = duration;
        }
    } else {
        ps.torso_anim = written;
        if set_timer {
            ps.torso_timer = duration;
        }
    }
}
