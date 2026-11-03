use alloc::vec::Vec;
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeaponCarry {
    pub ammo_clip: i32,

    pub ammo_stock: i32,

    pub ammo_by_weapon: Vec<(u32, i32, i32)>,

    pub taped_mag_spent: Vec<u32>,

    pub weapon_shot_count: u8,
    pub burst_latch: bool,
    pub burst_latch_secondary: bool,

    pub rechamber_pending: bool,
    pub rechamber_pending_secondary: bool,
    pub pending_brass: [Option<crate::PendingBrass>; 2],
}

impl WeaponCarry {
    pub fn ammo_for(&self, weapon: u32) -> (i32, i32) {
        self.ammo_by_weapon
            .iter()
            .find(|(w, _, _)| *w == weapon)
            .map(|(_, c, s)| (*c, *s))
            .unwrap_or((0, 0))
    }

    pub fn set_ammo(&mut self, weapon: u32, clip: i32, stock: i32) {
        if weapon == 0 {
            return;
        }
        if let Some(row) = self
            .ammo_by_weapon
            .iter_mut()
            .find(|(w, _, _)| *w == weapon)
        {
            row.1 = clip;
            row.2 = stock;
            return;
        }
        self.ammo_by_weapon.push((weapon, clip, stock));
    }

    pub fn quick_reload_ready(&self, weapon: u32) -> bool {
        !self.taped_mag_spent.contains(&weapon)
    }

    pub fn set_quick_reload_ready(&mut self, weapon: u32, ready: bool) {
        self.taped_mag_spent.retain(|&w| w != weapon);
        if !ready && weapon != 0 {
            self.taped_mag_spent.push(weapon);
        }
    }

    pub fn clear_ammo_inventory(&mut self) {
        self.ammo_by_weapon.clear();
        self.taped_mag_spent.clear();
        self.ammo_clip = 0;
        self.ammo_stock = 0;
    }

    pub fn mirror_held_ammo(&mut self, weapon: u32) {
        let (clip, stock) = self.ammo_for(weapon);
        self.ammo_clip = clip;
        self.ammo_stock = stock;
    }

    pub fn sync_pickup_ammo(
        &mut self,
        ps: &playerstate_iw4::PlayerState,
        weapon: u32,
        clip: i32,
        stock: i32,
    ) {
        self.ammo_by_weapon
            .retain(|(id, _, _)| ps.weapons.contains(&(*id as i32)));
        self.taped_mag_spent
            .retain(|id| ps.weapons.contains(&(*id as i32)));
        self.set_ammo(weapon, clip, stock);
        self.mirror_held_ammo(ps.weapon);
    }
}

pub fn pickup_owned_ammo(
    ps: &mut playerstate_iw4::PlayerState,
    weapon: u32,
    facts: Option<&crate::WeaponCombatFacts>,
    akimbo: bool,
    taken: i32,
) {
    crate::latch_weapon_dual_wield(&ps.weapons, &mut ps.weapon_data, weapon, akimbo);
    if ps.weapon == weapon {
        ps.last_weapon_hand = crate::num_hands_for_held(&ps.weapons, &ps.weapon_data, weapon);
    }
    let Some(facts) = facts else {
        return;
    };
    let clip_key = crate::clip_table_key(facts.clip_index, weapon);
    let ammo_key = crate::ammo_table_key(facts.ammo_index, weapon);
    let have_r = crate::get_clip_for_hand(&ps.ammoclip, clip_key, 0);
    let have_l = crate::get_clip_for_hand(&ps.ammoclip, clip_key, 1);
    let have_stock = crate::get_ammo_not_in_clip(&ps.ammo, ammo_key);
    if clip_key != 0 {
        let _ = crate::set_clip_for_hand(&mut ps.ammoclip, clip_key, 0, have_r);
        let _ = crate::set_clip_for_hand(&mut ps.ammoclip, clip_key, 1, have_l);
    }
    if ammo_key != 0 {
        let _ = crate::set_ammo_not_in_clip(
            &mut ps.ammo,
            ammo_key,
            have_stock.saturating_add(taken).min(facts.max_ammo),
        );
    }
}
