use crate::EquipmentRuntimeFacts;
use std::collections::BTreeMap;
use std::sync::Arc;
use weapon_iw4::WeaponCombatFacts;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponSetup {
    pub realm: crate::script::FamilyId,
    pub base: String,
    pub attachments: Vec<String>,
    pub stand_in: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponScriptSounds {
    pub fire: Option<String>,
    pub fire_player: Option<String>,
    pub pickup: Option<String>,
    pub pickup_player: Option<String>,
    pub proj_explosion: Option<String>,
}

pub struct SimWeaponRow {
    pub unlock_requirement: Result<gamemode_iw4::progression::UnlockRequirement, String>,
    pub wire_id: u32,
    pub scales: (f32, f32, f32),
    pub execution: Result<WeaponCombatFacts, String>,
    pub transition_group: u32,
    pub camouflage_slots: Vec<u8>,
    pub penetration: weapon_iw4::BulletPenFacts,
    pub script_name: String,
    pub setup: Option<WeaponSetup>,
    pub world_model: (String, Vec<String>),
    pub shield_model: Option<Arc<xmodel_runtime::RetainedModelCapability>>,
    pub projectile_model: String,
    pub melee_only: bool,
    pub script_sounds: WeaponScriptSounds,
    pub equipment: crate::EquipmentRuntimeFacts,
}

#[derive(Debug)]
pub struct SimWeaponContent {
    pub(crate) rank_progression: gamemode_iw4::progression::RankProgression,
    pub(crate) unlock_requirements:
        Vec<Result<gamemode_iw4::progression::UnlockRequirement, String>>,
    pub(crate) weapon_def_scales: Vec<(f32, f32, f32)>,
    pub(crate) weapon_combat: Vec<WeaponCombatFacts>,
    pub(crate) weapon_runnable: Vec<bool>,
    execution_refusals: Vec<Option<String>>,
    pub(crate) weapon_transition_groups: Vec<u32>,
    pub(crate) weapon_camouflage_slots: Vec<Vec<u8>>,
    pub(crate) bullet_pen: Vec<weapon_iw4::BulletPenFacts>,
    pub(crate) pen_table: weapon_iw4::PenetrationDepthTable,
    pub(crate) pen_table_loaded: bool,
    pub(crate) weapon_script_names: Arc<[String]>,
    pub(crate) weapon_script_aliases: BTreeMap<String, u32>,
    pub(crate) weapon_setups: Arc<[Option<WeaponSetup>]>,
    pub(crate) weapon_world_models: Vec<(String, Vec<String>)>,
    pub(crate) shield_models: Vec<Option<Arc<xmodel_runtime::RetainedModelCapability>>>,
    pub(crate) weapon_projectile_models: Vec<String>,
    pub(crate) weapon_melee_only: Vec<bool>,
    pub(crate) weapon_script_sounds: Vec<WeaponScriptSounds>,
    pub(crate) equipment_runtime: Vec<EquipmentRuntimeFacts>,
}

impl SimWeaponContent {
    pub(crate) fn bootstrap() -> Self {
        Self {
            rank_progression: Default::default(),
            unlock_requirements: Default::default(),
            weapon_def_scales: Default::default(),
            weapon_combat: Default::default(),
            weapon_runnable: Default::default(),
            execution_refusals: Default::default(),
            weapon_transition_groups: Default::default(),
            weapon_camouflage_slots: Default::default(),
            bullet_pen: Default::default(),
            pen_table: Default::default(),
            pen_table_loaded: Default::default(),
            weapon_script_names: Default::default(),
            weapon_script_aliases: Default::default(),
            weapon_setups: Default::default(),
            weapon_world_models: Default::default(),
            shield_models: Default::default(),
            weapon_projectile_models: Default::default(),
            weapon_melee_only: Default::default(),
            weapon_script_sounds: Default::default(),
            equipment_runtime: Default::default(),
        }
    }

    pub fn compile(
        rows: impl IntoIterator<Item = SimWeaponRow>,
        aliases: impl IntoIterator<Item = (String, u32)>,
        pen_table: weapon_iw4::PenetrationDepthTable,
        pen_table_loaded: bool,
        rank_progression: gamemode_iw4::progression::RankProgression,
    ) -> Result<Arc<Self>, &'static str> {
        if rank_progression.rank(0).is_none() {
            return Err("invalid rank progression");
        }
        let mut result = Self {
            rank_progression,
            pen_table,
            pen_table_loaded,
            ..Self::bootstrap()
        };
        let mut names = Vec::new();
        let mut setups = Vec::new();
        for row in rows {
            if row.wire_id as usize != names.len() {
                return Err("non-dense weapon rows");
            }
            result.unlock_requirements.push(row.unlock_requirement);
            result.weapon_def_scales.push(row.scales);
            let (combat, refusal) = match row.execution {
                Ok(combat) if row.wire_id != 0 => (combat, None),
                Ok(_) => (WeaponCombatFacts::none(), Some("unarmed".to_owned())),
                Err(reason) => (WeaponCombatFacts::none(), Some(reason)),
            };
            result.weapon_runnable.push(refusal.is_none());
            result.weapon_combat.push(combat);
            result.execution_refusals.push(refusal);
            result.weapon_transition_groups.push(row.transition_group);
            result.weapon_camouflage_slots.push(row.camouflage_slots);
            result.bullet_pen.push(row.penetration);
            result.weapon_world_models.push(row.world_model);
            result.shield_models.push(row.shield_model);
            result.weapon_projectile_models.push(row.projectile_model);
            result.weapon_melee_only.push(row.melee_only);
            result.weapon_script_sounds.push(row.script_sounds);
            result.equipment_runtime.push(row.equipment);
            names.push(row.script_name);
            setups.push(row.setup);
        }
        if names.is_empty() {
            return Err("missing weapon sentinel");
        }
        for (name, id) in aliases {
            if id as usize >= names.len() {
                return Err("unknown weapon alias");
            }
            result.weapon_script_aliases.insert(name, id);
        }
        result.weapon_script_names = names.into();
        result.weapon_setups = setups.into();
        Ok(Arc::new(result))
    }

    pub fn is_runnable(&self, id: u32) -> bool {
        self.weapon_runnable
            .get(id as usize)
            .copied()
            .unwrap_or(false)
    }

    pub fn execution_refusal(&self, id: u32) -> Option<&str> {
        self.execution_refusals.get(id as usize)?.as_deref()
    }

    pub fn combat(&self) -> &[WeaponCombatFacts] {
        &self.weapon_combat
    }

    pub fn equipment(&self) -> &[EquipmentRuntimeFacts] {
        &self.equipment_runtime
    }
}
