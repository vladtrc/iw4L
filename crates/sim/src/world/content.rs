use crate::player_anim_script::PlayerAnimScript;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct SimBrush {
    pub planes: Vec<[f32; 4]>,
    pub contents: u32,

    pub plane_surface_flags: Vec<u32>,

    pub glass_encoded: u16,
}

impl clipmap_iw4::BrushView for SimBrush {
    fn planes(&self) -> &[[f32; 4]] {
        &self.planes
    }
    fn contents(&self) -> u32 {
        self.contents
    }
    fn plane_surface_flags(&self) -> &[u32] {
        &self.plane_surface_flags
    }
    fn glass_encoded(&self) -> u16 {
        self.glass_encoded
    }
}

#[derive(Clone, Debug, Default)]
pub struct SimClipBsp {
    pub nodes: Vec<clipmap_iw4::ClipNode>,
    pub leaves: Vec<clipmap_iw4::ClipLeaf>,
    pub leafbrushes: Vec<u16>,
}

#[derive(Clone, Debug, Default)]
pub struct SimClipMesh {
    pub tables: std::sync::Arc<clipmap_iw4::ClipMeshTables>,

    pub static_models: Vec<SimStaticModel>,

    pub smodel_grid: crate::smodel_grid::SmodelGrid,
}

impl SimClipMesh {
    pub fn rebuild_smodel_grid(&mut self) {
        self.smodel_grid =
            crate::smodel_grid::SmodelGrid::build(self.static_models.iter().map(|sm| {
                crate::smodel_grid::SmodelBounds {
                    mid: sm.model.bounds_mid,
                    half: sm.model.bounds_half,
                }
            }));
    }
}

#[derive(Clone, Debug)]
pub struct SimStaticModel {
    pub index: u32,
    pub name: String,
    pub model: clipmap_iw4::ClipStaticModel,
}

#[derive(Clone, Debug, Default)]
pub struct SimClipCmodels {
    pub models: Vec<clipmap_iw4::ClipCmodel>,
    pub triggers: Vec<Vec<SimTriggerHull>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimTriggerHull {
    pub mid: [f32; 3],
    pub half: [f32; 3],
    pub slabs: Vec<([f32; 3], f32, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct PlayerKitCollision {
    pub body_key: String,
    pub body: Option<Arc<xmodel_runtime::RetainedModelCapability>>,
    pub head_key: String,
    pub head: Option<Arc<xmodel_runtime::RetainedModelCapability>>,
}

impl PlayerKitCollision {
    pub(super) fn reuse_key(&self) -> xmodel_runtime::DObjReuseKey {
        xmodel_runtime::DObjReuseKey {
            e_type: entity_iw4::ET_PLAYER,
            model: xmodel_runtime::model_token(&[self.body_key.as_str(), self.head_key.as_str()]),
        }
    }
}

#[derive(Debug)]
pub struct SimContent {
    data: ContentData,
}

impl SimContent {
    pub fn map(&self) -> &Arc<asset_core::MapContentDefinition> {
        &self.data.map
    }

    pub fn weapon_combat_row(&self, weapon: u32) -> Option<weapon_iw4::WeaponCombatFacts> {
        self.data
            .weapons
            .weapon_combat
            .get(weapon as usize)
            .copied()
    }

    pub fn weapon_combat_facts(&self, weapon: u32) -> Option<weapon_iw4::WeaponCombatFacts> {
        self.weapon_combat_row(weapon)
            .filter(|facts| facts.is_usable())
    }
    pub fn equipment_facts_for(&self, weapon: u32) -> Option<crate::EquipmentRuntimeFacts> {
        self.data
            .weapons
            .equipment_runtime
            .get(weapon as usize)
            .copied()
            .filter(|facts| facts.is_usable())
    }
    pub fn missile_launch_facts(&self, weapon: u32) -> Option<crate::EquipmentRuntimeFacts> {
        self.data
            .weapons
            .equipment_runtime
            .get(weapon as usize)
            .copied()
            .filter(|facts| facts.projectile_speed > 0)
    }
    pub fn weapon_script_name(&self, weapon: u32) -> &str {
        self.data
            .weapons
            .weapon_script_names
            .get(weapon as usize)
            .map_or("", String::as_str)
    }

    pub fn weapon_combat_len(&self) -> usize {
        self.data.weapons.weapon_combat.len()
    }

    pub fn has_world_clip(&self) -> bool {
        !self.clip_brushes().is_empty() || self.clip_mesh().tables.tri_count() >= 1
    }

    pub fn collision_complete(&self) -> bool {
        self.data.collision_complete
    }
    pub fn clip_brushes(&self) -> &[SimBrush] {
        &self.data.clip_brushes
    }
    pub fn clip_bsp(&self) -> &SimClipBsp {
        &self.data.clip_bsp
    }
    pub fn clip_mesh(&self) -> &SimClipMesh {
        &self.data.clip_mesh
    }
    pub fn clip_cmodels(&self) -> &SimClipCmodels {
        &self.data.clip_cmodels
    }
    pub fn atlas(&self) -> Option<&Arc<crate::world_graph::Atlas>> {
        self.data.atlas.as_ref()
    }
}

#[derive(Debug)]
struct ContentData {
    map: Arc<asset_core::MapContentDefinition>,
    weapons: Arc<crate::SimWeaponContent>,
    script_sound_aliases: Option<std::collections::BTreeMap<String, Option<bool>>>,
    script_fx: std::collections::BTreeSet<String>,
    collision_complete: bool,
    clip_brushes: Vec<SimBrush>,
    clip_bsp: SimClipBsp,
    clip_mesh: SimClipMesh,
    clip_cmodels: SimClipCmodels,
    atlas: Option<Arc<crate::world_graph::Atlas>>,
    player_kits: [PlayerKitCollision; 2],
    player_anim_tree: Option<Arc<xmodel_runtime::XAnimTreeDefinition>>,
    player_anim_node_names: Vec<String>,
    player_axis_anim_tree: Option<Arc<xmodel_runtime::XAnimTreeDefinition>>,
    player_anim_properties: Vec<xmodel_runtime::PlayerAnimProperties>,
    player_body_branches: Option<xmodel_runtime::PlayerBodyBranches>,
    script_model_anims: std::collections::BTreeMap<String, crate::ScriptModelPlayAnim>,
    xanims: Arc<crate::MantleXAnimBind>,
    vehicle_turrets: std::collections::BTreeMap<String, String>,
    vehicle_compass: std::collections::BTreeMap<String, ([String; 2], [i32; 2])>,
    vehicle_accel: std::collections::BTreeMap<String, f32>,
    team_voice_prefix_allies: Option<String>,
    team_voice_prefix_axis: Option<String>,
    map_custom: std::collections::BTreeMap<String, String>,
    shocks: std::collections::BTreeMap<String, hud_iw4::ShockParams>,
    player_anim_script: Option<Arc<PlayerAnimScript>>,
}

#[derive(Debug)]
pub struct SimContentBuilder {
    data: ContentData,
}

impl SimContentBuilder {
    pub fn set_map(&mut self, map: Arc<asset_core::MapContentDefinition>) -> Result<(), String> {
        if let asset_core::GlassContent::Invalid(error) = &map.glass {
            return Err(error.clone());
        }
        self.data.map = map;
        Ok(())
    }

    pub fn bootstrap() -> Self {
        Self::for_match(Arc::new(crate::SimWeaponContent::bootstrap()))
    }

    pub fn for_match(weapons: Arc<crate::SimWeaponContent>) -> Self {
        Self {
            data: ContentData {
                map: Default::default(),
                weapons,
                script_sound_aliases: Default::default(),
                script_fx: Default::default(),
                collision_complete: false,
                clip_brushes: Default::default(),
                clip_bsp: Default::default(),
                clip_mesh: Default::default(),
                clip_cmodels: Default::default(),
                atlas: None,
                player_kits: Default::default(),
                player_anim_tree: Default::default(),
                player_anim_node_names: Default::default(),
                player_axis_anim_tree: Default::default(),
                player_anim_properties: Default::default(),
                player_body_branches: Default::default(),
                script_model_anims: Default::default(),
                xanims: Default::default(),
                vehicle_turrets: Default::default(),
                vehicle_compass: Default::default(),
                vehicle_accel: Default::default(),
                team_voice_prefix_allies: Default::default(),
                team_voice_prefix_axis: Default::default(),
                map_custom: Default::default(),
                shocks: Default::default(),
                player_anim_script: Default::default(),
            },
        }
    }

    pub fn uses_weapons(&self, weapons: &Arc<crate::SimWeaponContent>) -> bool {
        Arc::ptr_eq(&self.data.weapons, weapons)
    }

    pub fn set_script_fx(&mut self, names: std::collections::BTreeSet<String>) {
        self.data.script_fx = names;
    }

    pub fn set_script_sound_aliases(
        &mut self,
        aliases: Option<std::collections::BTreeMap<String, Option<bool>>>,
    ) {
        self.data.script_sound_aliases = aliases.map(|names| {
            names
                .into_iter()
                .map(|(name, looping)| (name.to_ascii_lowercase(), looping))
                .collect()
        });
    }

    pub fn finish(mut self) -> Arc<SimContent> {
        self.data.clip_mesh.rebuild_smodel_grid();
        Arc::new(SimContent { data: self.data })
    }

    pub fn set_script_model_anims(
        &mut self,
        anims: impl IntoIterator<Item = (String, crate::ScriptModelPlayAnim)>,
    ) {
        self.data.script_model_anims = anims
            .into_iter()
            .map(|(name, facts)| (name.to_ascii_lowercase(), facts))
            .collect();
    }

    pub fn set_player_anim_script(&mut self, script: Option<Arc<PlayerAnimScript>>) {
        self.data.player_anim_script = script;
    }

    pub fn set_player_kit_collisions(
        &mut self,
        allies: PlayerKitCollision,
        axis: PlayerKitCollision,
    ) {
        self.data.player_kits = [allies, axis];
    }

    pub fn set_player_anim_tree(
        &mut self,
        definition: Option<Arc<xmodel_runtime::XAnimTreeDefinition>>,
        node_names: Vec<String>,
    ) {
        self.data.player_anim_tree = definition;
        self.data.player_body_branches = node_names
            .iter()
            .position(|name| name == "legs")
            .zip(node_names.iter().position(|name| name == "torso"))
            .map(|(legs, torso)| xmodel_runtime::PlayerBodyBranches {
                legs: xmodel_runtime::XAnimNodeId(legs as u16),
                torso: xmodel_runtime::XAnimNodeId(torso as u16),
            });
        self.data.player_anim_node_names = node_names;
    }

    pub fn set_player_axis_anim_tree(
        &mut self,
        definition: Option<Arc<xmodel_runtime::XAnimTreeDefinition>>,
    ) {
        self.data.player_axis_anim_tree = definition;
    }

    pub fn set_player_anim_properties(
        &mut self,
        properties: Vec<xmodel_runtime::PlayerAnimProperties>,
    ) {
        self.data.player_anim_properties = properties;
    }

    pub fn set_mantle_xanims(&mut self, bind: crate::MantleXAnimBind) {
        self.data.xanims = Arc::new(bind);
    }

    pub fn set_vehicle_compass(
        &mut self,
        rows: impl IntoIterator<Item = (String, ([String; 2], [i32; 2]))>,
    ) {
        self.data.vehicle_compass = rows.into_iter().collect();
    }

    pub fn set_vehicle_accel(&mut self, rows: impl IntoIterator<Item = (String, f32)>) {
        self.data.vehicle_accel = rows.into_iter().collect();
    }

    pub fn set_vehicle_turrets(&mut self, turrets: Vec<(String, String)>) {
        self.data.vehicle_turrets = turrets.into_iter().collect();
    }

    pub fn set_team_voice_prefixes(&mut self, allies: Option<String>, axis: Option<String>) {
        self.data.team_voice_prefix_allies = allies;
        self.data.team_voice_prefix_axis = axis;
    }

    pub fn set_map_custom(&mut self, entry: std::collections::BTreeMap<String, String>) {
        self.data.map_custom = entry;
    }

    pub fn set_shocks(&mut self, shocks: std::collections::BTreeMap<String, hud_iw4::ShockParams>) {
        self.data.shocks = shocks;
    }

    pub fn set_clip_brushes(&mut self, brushes: Vec<SimBrush>) {
        self.data.clip_brushes = brushes;
        self.data.clip_bsp = SimClipBsp::default();
        self.data.clip_mesh = SimClipMesh::default();
        self.data.clip_cmodels = SimClipCmodels::default();
    }

    pub fn set_collision_complete(&mut self, complete: bool) {
        self.data.collision_complete = complete;
    }

    pub fn set_clip_map(
        &mut self,
        brushes: Vec<SimBrush>,
        bsp: SimClipBsp,
        mesh: SimClipMesh,
        cmodels: SimClipCmodels,
    ) {
        self.data.clip_brushes = brushes;
        self.data.clip_bsp = bsp;
        self.data.clip_mesh = mesh;
        self.data.clip_cmodels = cmodels;
    }

    pub fn set_atlas(&mut self, atlas: Option<Arc<crate::world_graph::Atlas>>) {
        self.data.atlas = atlas;
    }
}

impl SimContent {
    pub(crate) fn weapons(&self) -> &Arc<crate::SimWeaponContent> {
        &self.data.weapons
    }
    pub fn pen_table_loaded(&self) -> bool {
        self.weapons().pen_table_loaded
    }

    pub(super) fn script_fx(&self) -> &std::collections::BTreeSet<String> {
        &self.data.script_fx
    }

    pub(super) fn script_sound_aliases(
        &self,
    ) -> &Option<std::collections::BTreeMap<String, Option<bool>>> {
        &self.data.script_sound_aliases
    }
    pub(super) fn player_kits(&self) -> &[PlayerKitCollision; 2] {
        &self.data.player_kits
    }
    pub(super) fn player_anim_tree(&self) -> &Option<Arc<xmodel_runtime::XAnimTreeDefinition>> {
        &self.data.player_anim_tree
    }
    pub(super) fn player_anim_node_names(&self) -> &[String] {
        &self.data.player_anim_node_names
    }
    pub(super) fn player_axis_anim_tree(
        &self,
    ) -> &Option<Arc<xmodel_runtime::XAnimTreeDefinition>> {
        &self.data.player_axis_anim_tree
    }
    pub(super) fn player_anim_properties(&self) -> &[xmodel_runtime::PlayerAnimProperties] {
        &self.data.player_anim_properties
    }
    pub(super) fn player_body_branches(&self) -> &Option<xmodel_runtime::PlayerBodyBranches> {
        &self.data.player_body_branches
    }
    pub(super) fn script_model_anims(
        &self,
    ) -> &std::collections::BTreeMap<String, crate::ScriptModelPlayAnim> {
        &self.data.script_model_anims
    }
    pub(crate) fn xanims(&self) -> &Arc<crate::MantleXAnimBind> {
        &self.data.xanims
    }
    pub(super) fn vehicle_turrets(&self) -> &std::collections::BTreeMap<String, String> {
        &self.data.vehicle_turrets
    }
    pub(super) fn vehicle_compass(
        &self,
    ) -> &std::collections::BTreeMap<String, ([String; 2], [i32; 2])> {
        &self.data.vehicle_compass
    }
    pub(super) fn vehicle_accel(&self) -> &std::collections::BTreeMap<String, f32> {
        &self.data.vehicle_accel
    }

    pub(super) fn map_custom(&self) -> &std::collections::BTreeMap<String, String> {
        &self.data.map_custom
    }
    pub(super) fn shocks(&self) -> &std::collections::BTreeMap<String, hud_iw4::ShockParams> {
        &self.data.shocks
    }
    pub(crate) fn player_anim_script(&self) -> &Option<Arc<PlayerAnimScript>> {
        &self.data.player_anim_script
    }
}

impl weapon_iw4::WeaponFacts for SimContent {
    fn weapon_combat_row(&self, weapon: u32) -> Option<weapon_iw4::WeaponCombatFacts> {
        SimContent::weapon_combat_row(self, weapon)
    }
    fn weapon_combat_facts(&self, weapon: u32) -> Option<weapon_iw4::WeaponCombatFacts> {
        SimContent::weapon_combat_facts(self, weapon)
    }
    fn equipment_facts_for(&self, weapon: u32) -> Option<crate::EquipmentRuntimeFacts> {
        SimContent::equipment_facts_for(self, weapon)
    }
    fn weapon_script_name(&self, weapon: u32) -> &str {
        SimContent::weapon_script_name(self, weapon)
    }
}
