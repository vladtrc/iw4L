use std::sync::Arc;

use asset_game::WeaponRegistry;
use weapon_iw4::{HITLOC_COUNT, WeaponCombatFacts, WeaponHostRules};

pub struct PreparedSimWeapons {
    revision: u64,
    content: Arc<sim::SimWeaponContent>,
}

impl PreparedSimWeapons {
    pub(crate) fn content(&self) -> &Arc<sim::SimWeaponContent> {
        &self.content
    }

    pub(crate) fn owned_by(&self, weapons: &WeaponRegistry) -> bool {
        self.revision == weapons.revision()
    }
}

pub(crate) fn compile(
    weapons: &WeaponRegistry,
    world_models: &asset_model::WorldWeaponCatalog,
    global_location: Option<[f32; HITLOC_COUNT]>,
    aliases: Vec<(String, u32)>,
    pen_table: weapon_iw4::PenetrationDepthTable,
    pen_table_loaded: bool,
) -> Result<PreparedSimWeapons, &'static str> {
    let rules = WeaponHostRules::default();
    let aliases: Vec<_> = weapons.native_script_aliases().chain(aliases).collect();
    let mut refused = Vec::new();
    let groups = weapons.configuration_transition_groups();
    let rows = weapons.published_weapons().map(|weapon| {
        let id = weapon.wire_id();
        let execution = weapons
            .configuration_admission(id)
            .map_err(|reason| format!("{reason:?}"))
            .and_then(|()| compile_combat(weapons, weapon, rules, global_location, &mut refused));
        let setup = weapons.describe_configuration(id).and_then(|selection| {
            let family = selection.family.as_ref()?;
            Some(sim::WeaponSetup {
                realm: family.namespace,
                base: family.base.clone(),
                attachments: selection.attachments.clone(),
                stand_in: None,
            })
        });
        let shield_model = weapon
            .world_facts()
            .filter(|facts| facts.is_shield())
            .and_then(|_| weapons.world_model_entry(id, world_models))
            .and_then(|entry| entry.skel.retained_capability())
            .map(Arc::new);
        let script_sounds = weapons
            .sounds_of(id)
            .map(|sounds| sim::WeaponScriptSounds {
                fire: sounds.fire.clone(),
                fire_player: sounds.fire_player.clone(),
                pickup: sounds.pickup.clone(),
                pickup_player: sounds.pickup_player.clone(),
                proj_explosion: sounds.proj_explosion.clone(),
            })
            .unwrap_or(sim::WeaponScriptSounds::default());
        sim::SimWeaponRow {
            unlock_requirement: weapons.weapon_unlock_requirement(id).map_err(str::to_owned),
            wire_id: id,
            scales: weapon.movement_scales(),
            execution,
            transition_group: groups[id as usize],
            camouflage_slots: std::iter::once(0)
                .chain(
                    weapons
                        .camouflage_choices(id)
                        .into_iter()
                        .map(|(slot, _)| slot),
                )
                .collect(),
            penetration: weapons
                .penetration_facts_of(id)
                .unwrap_or(weapon_iw4::BulletPenFacts::default()),
            script_name: weapons.script_name_of(id),
            setup,
            world_model: (
                weapons.world_model_of(id).unwrap_or("").to_owned(),
                weapons.hide_tags_of(id).to_vec(),
            ),
            shield_model,
            projectile_model: weapons.projectile_model_of(id).unwrap_or("").to_owned(),
            melee_only: weapons.melee_only_of(id),
            script_sounds,
            equipment: weapons
                .equipment_facts_of(id)
                .unwrap_or(sim::EquipmentRuntimeFacts::default()),
        }
    });
    let ranks = weapons
        .rank_progression()
        .map_err(|_| "invalid rank progression")?;
    let content =
        sim::SimWeaponContent::compile(rows, aliases, pen_table, pen_table_loaded, ranks)?;
    if !refused.is_empty() {
        diag::info!(
            Sim,
            "combat facts refused for {} weapons: {}",
            refused.len(),
            refused.join(" ")
        );
    }
    Ok(PreparedSimWeapons {
        revision: weapons.revision(),
        content,
    })
}

fn compile_combat(
    weapons: &WeaponRegistry,
    weapon: asset_game::BoundWeapon<'_>,
    rules: WeaponHostRules,
    global_location: Option<[f32; HITLOC_COUNT]>,
    refused: &mut Vec<String>,
) -> Result<WeaponCombatFacts, String> {
    if weapon.wire_id() == 0 {
        return Err("unarmed".to_owned());
    }
    match weapon.combat_facts(rules, global_location) {
        Ok(facts) => Ok(facts),
        Err(reason) => {
            refused.push(format!("{}({reason:?})", weapons.name_of(weapon.wire_id())));
            Err(format!("{reason:?}"))
        }
    }
}

pub struct ClassWeaponAdmission<'a> {
    catalog: &'a asset_game::EditorWeaponCatalog,
}

impl<'a> ClassWeaponAdmission<'a> {
    pub fn prepare(catalog: &'a asset_game::EditorWeaponCatalog) -> Self {
        Self { catalog }
    }
    pub fn allows(&self, row: &crate::ClassRow) -> bool {
        crate::loadout::resolve_editor_class(row, self.catalog).is_ok_and(|loadout| {
            loadout
                .weapons
                .iter()
                .enumerate()
                .all(|(slot, &id)| self.catalog.class_slot_admission(id, slot))
        })
    }
}
