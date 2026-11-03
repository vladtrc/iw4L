use super::WeaponRegistry;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WeaponHandle {
    revision: u64,
    row: u32,
}

#[derive(Clone, Copy)]
pub struct BoundWeapon<'a> {
    registry: &'a WeaponRegistry,
    handle: WeaponHandle,
}

impl<'a> BoundWeapon<'a> {
    pub fn movement_scales(self) -> (f32, f32, f32) {
        let facts = &self.registry.rows[self.handle.row as usize].facts;
        (
            facts.move_speed_scale,
            facts.ads_move_speed_scale,
            facts.sprint_duration_scale,
        )
    }

    pub fn preparation(self) -> &'a super::WeaponPreparationRecipe {
        &self.registry.rows[self.handle.row as usize].preparation
    }

    pub fn handle(self) -> WeaponHandle {
        self.handle
    }

    pub fn wire_id(self) -> u32 {
        self.handle.row
    }

    pub fn fpv_facts(self) -> Option<super::WeaponFpvFacts> {
        self.registry.fpv_facts_of(self.handle.row)
    }

    pub fn hud_facts(self) -> Option<super::WeaponHudFacts> {
        self.registry.hud_facts_of(self.handle.row)
    }

    pub fn event_facts(self) -> Option<super::WeaponEventFacts> {
        self.registry.event_facts_of(self.handle.row)
    }

    pub fn world_facts(self) -> Option<super::WeaponWorldFacts> {
        self.registry.world_facts_of(self.handle.row)
    }

    pub fn combat_facts(
        self,
        rules: weapon_iw4::WeaponHostRules,
        global_location: Option<[f32; weapon_iw4::HITLOC_COUNT]>,
    ) -> Result<weapon_iw4::WeaponCombatFacts, super::WeaponCombatRefusal> {
        self.registry
            .combat_facts_of(self.handle.row, rules, global_location)
    }
}

impl Default for WeaponRegistry {
    fn default() -> Self {
        crate::WeaponCatalog::default().into_build().registry
    }
}

impl WeaponRegistry {
    pub fn bind(&self, handle: WeaponHandle) -> Result<BoundWeapon<'_>, &'static str> {
        if handle.revision != self.revision {
            return Err("weapon handle belongs to another registry");
        }
        if self.rows.get(handle.row as usize).is_none() {
            return Err("unknown weapon handle");
        }
        Ok(BoundWeapon {
            registry: self,
            handle,
        })
    }

    pub fn bind_published_row(&self, row: u32) -> Result<BoundWeapon<'_>, &'static str> {
        self.bind(WeaponHandle {
            revision: self.revision,
            row,
        })
    }

    pub fn published_weapons(&self) -> impl Iterator<Item = BoundWeapon<'_>> {
        (0..self.rows.len() as u32).map(|row| BoundWeapon {
            registry: self,
            handle: WeaponHandle {
                revision: self.revision,
                row,
            },
        })
    }
}
