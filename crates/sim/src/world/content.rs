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
    pub(super) fn script_model_states(&self) -> Option<Arc<xmodel_runtime::AnimStateTable>> {
        self.data.script_model_states.clone()
    }

    pub(super) fn script_model_clips(
        &self,
    ) -> Arc<std::collections::BTreeMap<String, Arc<xmodel_runtime::AnimClip>>> {
        self.data.script_model_clips.clone()
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
}

/// Resolves any animation of the match by name; installed by the session from
/// its animation catalog.
#[derive(Clone)]
pub struct AnimClipLookup(Arc<dyn Fn(&str) -> Option<Arc<xmodel_runtime::AnimClip>> + Send + Sync>);

impl AnimClipLookup {
    pub fn new(
        lookup: impl Fn(&str) -> Option<Arc<xmodel_runtime::AnimClip>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(lookup))
    }

    pub fn get(&self, name: &str) -> Option<Arc<xmodel_runtime::AnimClip>> {
        (self.0)(name)
    }
}

impl Default for AnimClipLookup {
    fn default() -> Self {
        Self::new(|_| None)
    }
}

impl std::fmt::Debug for AnimClipLookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnimClipLookup")
    }
}

#[derive(Debug)]
struct ContentData {
    weapons: Arc<crate::SimWeaponContent>,
    script_sound_aliases: Option<std::collections::BTreeMap<String, Option<bool>>>,
    /// The game the match's map belongs to: whose sound aliases its scripts name.
    family: Option<asset_core::FamilyId>,
    clip_brushes: Vec<SimBrush>,
    clip_bsp: SimClipBsp,
    clip_mesh: SimClipMesh,
    clip_cmodels: SimClipCmodels,
    player_kits: [PlayerKitCollision; 2],
    player_anim_tree: Option<Arc<xmodel_runtime::XAnimTreeDefinition>>,
    player_anim_node_names: Vec<String>,
    player_axis_anim_tree: Option<Arc<xmodel_runtime::XAnimTreeDefinition>>,
    player_anim_properties: Vec<xmodel_runtime::PlayerAnimProperties>,
    player_body_branches: Option<xmodel_runtime::PlayerBodyBranches>,
    script_model_anims: std::collections::BTreeMap<String, crate::ScriptModelPlayAnim>,
    anim_clips: AnimClipLookup,
    actor_anim_trees: std::collections::BTreeMap<String, Arc<crate::script::ActorAnimTree>>,
    actor_paths: Option<Arc<crate::script::ActorPaths>>,
    script_model_clips: Arc<std::collections::BTreeMap<String, Arc<xmodel_runtime::AnimClip>>>,
    script_model_states: Option<Arc<xmodel_runtime::AnimStateTable>>,
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
    pub fn bootstrap() -> Self {
        Self::for_match(Arc::new(crate::SimWeaponContent::bootstrap()))
    }

    pub fn for_match(weapons: Arc<crate::SimWeaponContent>) -> Self {
        Self {
            data: ContentData {
                weapons,
                script_sound_aliases: Default::default(),
                family: None,
                clip_brushes: Default::default(),
                clip_bsp: Default::default(),
                clip_mesh: Default::default(),
                clip_cmodels: Default::default(),
                player_kits: Default::default(),
                player_anim_tree: Default::default(),
                player_anim_node_names: Default::default(),
                player_axis_anim_tree: Default::default(),
                player_anim_properties: Default::default(),
                player_body_branches: Default::default(),
                script_model_anims: Default::default(),
                anim_clips: Default::default(),
                actor_anim_trees: Default::default(),
                actor_paths: Default::default(),
                script_model_states: None,
                script_model_clips: Default::default(),
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

    pub fn set_family(&mut self, family: Option<asset_core::FamilyId>) {
        self.data.family = family;
    }

    pub fn finish(mut self) -> Arc<SimContent> {
        self.data.clip_mesh.rebuild_smodel_grid();
        Arc::new(SimContent { data: self.data })
    }

    pub fn set_script_model_clips(
        &mut self,
        clips: impl IntoIterator<Item = (String, Arc<xmodel_runtime::AnimClip>)>,
    ) {
        self.data.script_model_clips = Arc::new(clips.into_iter().collect());
    }

    pub fn set_script_model_states(&mut self, states: Option<Arc<xmodel_runtime::AnimStateTable>>) {
        self.data.script_model_states = states;
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

    pub fn set_actor_anim_trees(
        &mut self,
        trees: impl IntoIterator<Item = Arc<crate::script::ActorAnimTree>>,
    ) {
        self.data.actor_anim_trees = trees
            .into_iter()
            .map(|tree| (tree.name().to_owned(), tree))
            .collect();
    }

    pub fn set_actor_paths(&mut self, paths: crate::script::ActorPaths) {
        self.data.actor_paths = (!paths.is_empty()).then(|| Arc::new(paths));
    }

    pub fn set_anim_clips(&mut self, lookup: AnimClipLookup) {
        self.data.anim_clips = lookup;
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
}

impl SimContent {
    pub(super) fn weapons(&self) -> &Arc<crate::SimWeaponContent> {
        &self.data.weapons
    }
    pub(super) fn script_sound_aliases(
        &self,
    ) -> &Option<std::collections::BTreeMap<String, Option<bool>>> {
        &self.data.script_sound_aliases
    }
    pub(super) fn family(&self) -> Option<asset_core::FamilyId> {
        self.data.family
    }
    pub(super) fn player_kits(&self) -> &[PlayerKitCollision; 2] {
        &self.data.player_kits
    }
    pub(super) fn player_anim_tree(&self) -> &Option<Arc<xmodel_runtime::XAnimTreeDefinition>> {
        &self.data.player_anim_tree
    }
    pub(super) fn player_anim_node_names(&self) -> &Vec<String> {
        &self.data.player_anim_node_names
    }
    pub(super) fn player_axis_anim_tree(
        &self,
    ) -> &Option<Arc<xmodel_runtime::XAnimTreeDefinition>> {
        &self.data.player_axis_anim_tree
    }
    pub(super) fn player_anim_properties(&self) -> &Vec<xmodel_runtime::PlayerAnimProperties> {
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
    pub(super) fn actor_anim_tree(&self, name: &str) -> Option<Arc<crate::script::ActorAnimTree>> {
        self.data
            .actor_anim_trees
            .get(&name.to_ascii_lowercase())
            .cloned()
    }
    pub(super) fn actor_paths(&self) -> Option<Arc<crate::script::ActorPaths>> {
        self.data.actor_paths.clone()
    }
    pub(super) fn anim_clips(&self) -> &AnimClipLookup {
        &self.data.anim_clips
    }
    pub(super) fn xanims(&self) -> &Arc<crate::MantleXAnimBind> {
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
    pub(super) fn player_anim_script(&self) -> &Option<Arc<PlayerAnimScript>> {
        &self.data.player_anim_script
    }
}
