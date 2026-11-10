type AlternateFpvCache = HashMap<(u32, u32), [Option<Result<crate::FpvSideAssemblies, String>>; 2]>;

use crate::{
    NotetrackConvention, WeaponAttachments, WeaponBodyFacts, WeaponCamouflageChoice,
    WeaponCombatFx, WeaponKickFacts, WeaponReticleAssets, WeaponSoundAliases, WeaponSoundSlot,
    WeaponSwayFacts, overlay_name_is_hud_iris,
};
mod catalog_linking;
mod editor;
mod model_linking;
pub use editor::{EditorWeaponCatalog, EditorWeaponSelection};
mod configuration_preparation;
mod publication;
use catalog_linking::{material_hint_edge, stamp_combat_fx, stamp_mw2_tracer};
mod capture_merge;
use capture_merge::{
    idle_from_capture, merge_body_facts, merge_combat_fx, merge_combat_slots, merge_sound_aliases,
    merge_sz_xanims, movement_from_capture, read_hide_tags, read_name, read_script_string_map,
    read_sz_xanims, xanims_idle,
};
mod preparation;
pub use preparation::{
    ComponentPreparationPolicy, PreparedComponentReference, PreparedComponentTarget,
    WeaponComponent, WeaponPreparationRecipe,
};
mod appearance;
mod registry;
pub use appearance::{AppearanceModelStatus, SelectedWeaponAppearance};
mod flame_stream;
pub use flame_stream::{FlameStreamView, FlameTube, WeaponFlameStream};
mod binding;
mod capture_iw4;
pub use binding::{BoundWeapon, WeaponHandle};
mod combat;
pub use combat::WeaponCombatRefusal;
pub(crate) mod configuration;
mod equipment;
mod fpv;
mod presentation;
pub use fpv::WeaponFpvFacts;
pub use presentation::{
    ProjectileCameraPolicy, WeaponEventFacts, WeaponHudFacts, WeaponWorldFacts,
};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn mint_weapon_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

use asset_core::{
    AssetEdge, AssetEdgeCensus, AssetEdgeReason, FpvMeshSpace, FxSpace, MaterialSpace,
    ProjectileModelSpace, TracerSpace, WorldWeaponSpace, XAnimSpace, ZoneOwner,
};
use asset_iw4::size::{WEAPON_ANIM_COUNT, weap_anim};

use fastfile_iw4::{
    Ptr, ScriptStrings, WeaponIdleCapture, WeaponMovementOfsCapture, ZonePtr, ZoneStream,
};
use weapon_iw4::WEAPON_ANIM_SLOTS;
use weapon_iw4::{WeaponIdleInputs, WeaponMovementOfsInputs};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponCamoModels {
    pub view: Vec<(u8, String)>,
    pub world: Vec<(u8, String)>,
    pub choices: Vec<WeaponCamouflageChoice>,
    pub invalid_view: Vec<u8>,
    pub invalid_world: Vec<u8>,
}

impl WeaponCamoModels {
    pub fn is_empty(&self) -> bool {
        self.view.is_empty()
            && self.world.is_empty()
            && self.invalid_view.is_empty()
            && self.invalid_world.is_empty()
    }
}

#[derive(Clone, Debug)]
struct CatalogWeapon {
    pub namespace: crate::FamilyId,
    pub name: String,
    pub alternate_weapon: Option<String>,
    pub impact_payload: Option<String>,

    pub weap_def: Option<(u8, u32)>,

    pub display_name_key: Option<String>,

    pub reticle: WeaponReticleAssets,

    pub hud_material_edges: WeaponHudMaterialEdges,

    pub overlay_material: Option<String>,

    pub overlay_image: Option<String>,
    pub reticle_center_slot: Option<Ptr>,
    pub reticle_side_slot: Option<Ptr>,

    pub overlay_material_slot: Option<Ptr>,

    pub attachment_assets: Vec<Option<String>>,
    pub attached_models: [Vec<String>; 2],
    pub native_capture: crate::WeaponNativeCapture,

    pub hud_icon: Option<String>,
    pub hud_icon_slot: Option<Ptr>,
    pub pickup_icon: Option<String>,
    pub pickup_icon_slot: Option<Ptr>,
    pub pickup_icon_image: Option<String>,
    pub pickup_icon_ratio: i32,
    pub hud_icon_ratio: i32,

    pub hud_icon_image: Option<String>,

    pub dpad_icon: Option<String>,
    pub dpad_icon_image: Option<String>,
    pub dpad_icon_atlas: Option<[u8; 2]>,
    pub dpad_icon_ratio: i32,
    pub kill_icon: Option<String>,
    pub kill_icon_slot: Option<Ptr>,

    pub kill_icon_image: Option<String>,

    pub proj_trail: Option<String>,
    pub proj_trail_slot: Option<Ptr>,

    pub proj_beacon: Option<String>,
    pub proj_beacon_slot: Option<Ptr>,

    pub proj_ignition: Option<String>,
    pub proj_ignition_slot: Option<Ptr>,

    pub projectile_fx: WeaponProjectileFx,

    pub gun_xmodel: Option<String>,

    pub hand_xmodel: Option<String>,
    pub dual_wield_weapon: Option<String>,

    pub clip_model: Option<String>,

    pub world_model: Option<String>,

    pub camo_models: WeaponCamoModels,
    pub skin_parent: Option<String>,

    pub projectile_model: Option<String>,

    pub rocket_model: Option<String>,

    pub knife_xmodel: Option<String>,

    pub sz_xanims: [Option<String>; WEAPON_ANIM_SLOTS],

    pub sz_xanims_right: [Option<String>; WEAPON_ANIM_SLOTS],

    pub sz_xanims_left: [Option<String>; WEAPON_ANIM_SLOTS],

    pub hide_tags: Vec<String>,

    pub sounds: WeaponSoundAliases,

    pub combat_fx: WeaponCombatFx,

    pub flame: Option<Box<WeaponFlameStream>>,

    pub(crate) combat_slots: CombatFxSlots,
    pub facts: WeaponBodyFacts,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponHudMaterialEdges {
    pub overlay: AssetEdge<MaterialSpace>,
    pub hud_icon: AssetEdge<MaterialSpace>,
    pub pickup_icon: AssetEdge<MaterialSpace>,
    pub kill_icon: AssetEdge<MaterialSpace>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponProjectileFx {
    pub trail: AssetEdge<FxSpace>,
    pub beacon: AssetEdge<FxSpace>,
    pub ignition: AssetEdge<FxSpace>,
}

impl WeaponProjectileFx {
    pub fn edges(self) -> [AssetEdge<FxSpace>; 3] {
        [self.trail, self.beacon, self.ignition]
    }
}

pub const T5_NOTE_SOUND_PREFIX: &str = "sndnt#";

pub const T5_NOTE_RUMBLE_PREFIX: &str = "rmbnt#";

#[derive(Clone, Debug, Default)]
pub struct LinkedNotetrackAction {
    pub sound_alias: Option<String>,
    pub rumble_alias: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponDependencyGap {
    pub id: u32,
    pub kind: &'static str,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CombatFxSlots {
    closed: bool,
    view_flash: Option<Ptr>,
    world_flash: Option<Ptr>,
    view_shell_eject: Option<Ptr>,
    world_shell_eject: Option<Ptr>,
    view_last_shot_eject: Option<Ptr>,
    world_last_shot_eject: Option<Ptr>,
    explosion: Option<Ptr>,
    tracer: Option<Ptr>,
}

impl CombatFxSlots {
    fn last_shot_pair_authored(self) -> bool {
        self.view_last_shot_eject.is_some() && self.world_last_shot_eject.is_some()
    }
}

#[derive(Clone, Debug, Default)]
pub struct WeaponCatalog {
    entries: Vec<CatalogWeapon>,
    strings: ScriptStrings,
    attachments: WeaponAttachments,
    capture_ns: Option<crate::FamilyId>,
    vehicle_turrets: HashMap<String, String>,
    vehicle_compass: HashMap<String, ([String; 2], [i32; 2])>,
    vehicle_accel: HashMap<String, f32>,
}

fn fpv_model_edge(
    hint: Option<&str>,
    ns: Option<crate::FamilyId>,
    fpv: &crate::FpvMeshCatalog,
) -> AssetEdge<FpvMeshSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    match hint {
        None => AssetEdge::Absent,
        Some(name) => match ns.and_then(|ns| fpv.index_by_name(ns, name)) {
            Some(index) => AssetEdge::bind_order(index, fpv.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn world_model_edge(
    hint: Option<&str>,
    namespace: Option<crate::FamilyId>,
    catalog: &crate::WorldWeaponCatalog,
) -> AssetEdge<WorldWeaponSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    match hint {
        None => AssetEdge::Absent,
        Some(name) => match namespace.and_then(|ns| catalog.index_by_name(ns, name)) {
            Some(index) => AssetEdge::bind_order(index, catalog.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn sound_alias_in_bank<'a>(
    hint: Option<&str>,
    ns: crate::FamilyId,
    catalog: &'a crate::SoundCatalog,
) -> Option<(crate::FamilyId, &'a str)> {
    let name = hint.filter(|name| !name.is_empty())?;
    let order = catalog.index_in(ns, name)?;
    let bound = catalog.bind_published_alias(order, 0).ok()?;
    (bound.policy().namespace == ns).then_some((ns, catalog.name_at(order)?))
}

#[derive(Clone, Debug)]
struct WeaponRow {
    preparation: WeaponPreparationRecipe,
    name: String,
    native_script_name: String,
    alternate_weapon: Option<String>,
    impact_payload: Option<String>,
    alternate_index: u32,

    namespace: crate::FamilyId,

    facts: WeaponBodyFacts,
    semantics: Option<crate::WeaponSemanticPolicy>,
    combat: Option<combat::WeaponCombatProjection>,
    fpv: Option<WeaponFpvFacts>,
    hud: Option<WeaponHudFacts>,
    events: Option<WeaponEventFacts>,
    world: Option<WeaponWorldFacts>,
    equipment: Option<weapon_iw4::EquipmentRuntimeFacts>,
    penetration: Option<weapon_iw4::BulletPenFacts>,

    gun_xmodel: Option<String>,

    hand_xmodel: Option<String>,
    dual_wield_weapon: Option<String>,
    secondary_gun_xmodel: Option<String>,

    gun_xmodel_edge: AssetEdge<FpvMeshSpace>,

    hand_xmodel_edge: AssetEdge<FpvMeshSpace>,

    rocket_model_edge: AssetEdge<FpvMeshSpace>,

    attachment_view_model_edges: Vec<AssetEdge<FpvMeshSpace>>,

    fpv_soldiers: [Option<Result<crate::FpvHandsPresentation, String>>; 2],

    fpv_mount_plan: Option<Result<asset_model::FpvMountPlan, asset_model::FpvMountError>>,

    fpv_assemblies: [Option<Result<crate::FpvSideAssemblies, String>>; 2],

    clip_model: Option<String>,

    world_model: Option<String>,

    world_model_edge: AssetEdge<WorldWeaponSpace>,

    camo_models: WeaponCamoModels,
    skin_parent: Option<String>,
    material_camos: Arc<[crate::WeaponCamouflage]>,
    appearances: Arc<[appearance::PreparedWeaponAppearance]>,

    camo_view_edges: Vec<(u8, AssetEdge<FpvMeshSpace>)>,
    camo_world_edges: Vec<(u8, AssetEdge<WorldWeaponSpace>)>,

    attachment_world_model_edges: Vec<AssetEdge<WorldWeaponSpace>>,

    attachment_world_mounts: Vec<Option<String>>,

    projectile_model: Option<String>,

    projectile_model_edge: AssetEdge<ProjectileModelSpace>,

    rocket_model: Option<String>,

    knife_xmodel: Option<String>,

    sz_xanims: [Option<String>; WEAPON_ANIM_SLOTS],

    sz_xanim_edges: [AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS],

    sz_xanim_right_edges: [AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS],

    sz_xanim_left_edges: [AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS],

    notetrack_actions: HashMap<String, LinkedNotetrackAction>,

    sz_xanims_right: [Option<String>; WEAPON_ANIM_SLOTS],

    sz_xanims_left: [Option<String>; WEAPON_ANIM_SLOTS],

    hide_tags: Vec<String>,

    attachment_view_models: Vec<String>,
    attachment_world_models: Vec<String>,
    attachment_view_ads_models: Vec<(String, String)>,

    indexed_name: bool,
    prepared_attachments: Vec<String>,
    attachment_caption_keys: Vec<String>,

    attachment_assets: Vec<Option<String>>,
    native_capture: crate::WeaponNativeCapture,

    sounds: WeaponSoundAliases,

    combat_fx: WeaponCombatFx,

    flame: Option<Box<WeaponFlameStream>>,

    reticle: WeaponReticleAssets,

    hud_material_edges: WeaponHudMaterialEdges,

    overlay_material: Option<String>,
    overlay_image: Option<String>,
    overlay_material_from_slot: bool,

    hud_icon: Option<String>,
    hud_icon_from_slot: bool,
    pickup_icon: Option<String>,
    pickup_icon_image: Option<String>,
    pickup_icon_authored: bool,
    pickup_icon_ratio: i32,
    hud_icon_ratio: i32,

    hud_icon_image: Option<String>,

    dpad_icon_image: Option<String>,
    dpad_icon_atlas: Option<[u8; 2]>,
    dpad_icon_ratio: i32,
    kill_icon: Option<String>,
    kill_icon_from_slot: bool,
    kill_icon_image: Option<String>,

    projectile_fx: WeaponProjectileFx,

    proj_trail: Option<String>,
    proj_trail_from_slot: bool,
    proj_beacon: Option<String>,
    proj_beacon_from_slot: bool,
    proj_ignition: Option<String>,
    proj_ignition_from_slot: bool,

    display_name_key: Option<String>,
}

impl Default for WeaponRow {
    fn default() -> Self {
        Self {
            preparation: WeaponPreparationRecipe::native(crate::FamilyId::IW4),
            name: String::new(),
            native_script_name: String::new(),
            alternate_weapon: None,
            impact_payload: None,
            alternate_index: 0,
            namespace: crate::FamilyId::IW4,
            facts: WeaponBodyFacts::default(),
            semantics: None,
            combat: None,
            fpv: None,
            hud: None,
            events: None,
            world: None,
            equipment: None,
            penetration: None,
            gun_xmodel: None,
            hand_xmodel: None,
            dual_wield_weapon: None,
            secondary_gun_xmodel: None,
            gun_xmodel_edge: AssetEdge::Absent,
            hand_xmodel_edge: AssetEdge::Absent,
            rocket_model_edge: AssetEdge::Absent,
            attachment_view_model_edges: Vec::new(),
            fpv_soldiers: [None, None],
            fpv_mount_plan: None,
            fpv_assemblies: [None, None],
            clip_model: None,
            world_model: None,
            world_model_edge: AssetEdge::Absent,
            camo_models: WeaponCamoModels::default(),
            skin_parent: None,
            material_camos: Arc::default(),
            appearances: Arc::default(),
            camo_view_edges: Vec::new(),
            camo_world_edges: Vec::new(),
            attachment_world_model_edges: Vec::new(),
            attachment_world_mounts: Vec::new(),
            projectile_model: None,
            projectile_model_edge: AssetEdge::Absent,
            rocket_model: None,
            knife_xmodel: None,
            sz_xanims: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanim_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
            sz_xanim_right_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
            sz_xanim_left_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
            notetrack_actions: HashMap::new(),
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: [const { None }; WEAPON_ANIM_SLOTS],
            hide_tags: Vec::new(),
            attachment_view_models: Vec::new(),
            attachment_world_models: Vec::new(),
            attachment_view_ads_models: Vec::new(),
            indexed_name: true,
            prepared_attachments: Vec::new(),
            attachment_caption_keys: Vec::new(),
            attachment_assets: Vec::new(),
            native_capture: Default::default(),
            sounds: WeaponSoundAliases::default(),
            combat_fx: WeaponCombatFx::empty(crate::FamilyId::IW4),
            flame: None,
            reticle: WeaponReticleAssets::default(),
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: None,
            overlay_image: None,
            overlay_material_from_slot: false,
            hud_icon: None,
            hud_icon_from_slot: false,
            pickup_icon: None,
            pickup_icon_image: None,
            pickup_icon_authored: false,
            pickup_icon_ratio: 0,
            hud_icon_ratio: 0,
            hud_icon_image: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: None,
            kill_icon_from_slot: false,
            kill_icon_image: None,
            projectile_fx: WeaponProjectileFx::default(),
            proj_trail: None,
            proj_trail_from_slot: false,
            proj_beacon: None,
            proj_beacon_from_slot: false,
            proj_ignition: None,
            proj_ignition_from_slot: false,
            display_name_key: None,
        }
    }
}

#[derive(Clone, Debug)]
struct ScriptViewmodel {
    hands: Result<crate::FpvHandsPresentation, String>,
    assemblies: Vec<[Option<Result<crate::FpvSideAssemblies, String>>; 2]>,
    alternates: AlternateFpvCache,
    tracks: Arc<crate::FpvClipTracks>,
}

#[derive(Clone, Debug)]
pub struct WeaponRegistry {
    script_viewmodels: Option<std::collections::BTreeMap<String, ScriptViewmodel>>,
    rows: Vec<WeaponRow>,

    world_catalog_identity: u64,
    fpv_catalog_identity: u64,
    family_tables: Arc<[(crate::FamilyId, crate::CapturedStringTable)]>,
    shared_ranks: std::sync::OnceLock<Result<crate::CapturedStringTable, &'static str>>,
    shared_icons: std::sync::OnceLock<Result<crate::CapturedStringTable, &'static str>>,

    attachments: WeaponAttachments,

    configurations: HashMap<crate::WeaponSelection, u32>,

    by_name: HashMap<String, u32>,

    by_namespaced: HashMap<(crate::FamilyId, String), u32>,

    item_groups: HashMap<(crate::FamilyId, String), String>,

    families: crate::WeaponFamilies,
    completion_names: Vec<String>,

    alternate_fpv: AlternateFpvCache,
    fpv_clip_tracks: Arc<crate::FpvClipTracks>,

    vehicle_turrets: HashMap<String, String>,
    vehicle_compass: HashMap<String, ([String; 2], [i32; 2])>,
    vehicle_accel: HashMap<String, f32>,

    revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownWeaponName {
    pub name: String,
}

impl core::fmt::Display for UnknownWeaponName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown weapon name `{}`", self.name)
    }
}

impl std::error::Error for UnknownWeaponName {}

#[derive(Clone, Debug)]
pub struct T6Melee {
    pub knife: String,
    pub melee: String,
    pub charge: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct WeaponBuild {
    registry: WeaponRegistry,
    combat_slots: Vec<CombatFxSlots>,
    family_tables: Vec<(crate::FamilyId, crate::CapturedStringTable)>,
}

impl std::ops::Deref for WeaponBuild {
    type Target = WeaponRegistry;

    fn deref(&self) -> &Self::Target {
        &self.registry
    }
}

impl WeaponBuild {
    pub fn hand_xmodel_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.hand_xmodel.as_deref())
    }

    pub fn material_camouflages_of(&self, weapon: u32) -> &[crate::WeaponCamouflage] {
        self.rows
            .get(weapon as usize)
            .map_or(&[], |row| &row.material_camos)
    }

    pub fn set_family_tables(
        &mut self,
        tables: Vec<(crate::FamilyId, crate::CapturedStringTable)>,
    ) {
        self.family_tables = tables;
    }

    /// Map-authored definitions replace common definitions without changing their IDs.
    /// The incoming build contains captured rows, before configuration preparation.
    pub fn overlay_authored(&mut self, mut other: Self) {
        self.combat_slots
            .resize(self.registry.rows.len(), CombatFxSlots::default());
        assert!(
            other.registry.configurations.is_empty(),
            "map definitions must precede configuration preparation"
        );
        self.registry
            .vehicle_compass
            .extend(std::mem::take(&mut other.registry.vehicle_compass));
        self.registry
            .vehicle_accel
            .extend(std::mem::take(&mut other.registry.vehicle_accel));
        self.registry
            .vehicle_turrets
            .extend(std::mem::take(&mut other.registry.vehicle_turrets));
        self.registry.attachments.absorb(other.registry.attachments);
        self.family_tables.extend(other.family_tables);
        self.registry.item_groups.extend(other.registry.item_groups);
        for (row, slots) in other
            .registry
            .rows
            .into_iter()
            .zip(other.combat_slots)
            .skip(1)
        {
            if let Some(&id) = self
                .registry
                .by_namespaced
                .get(&(row.namespace, row.name.clone()))
            {
                self.registry.rows[id as usize] = row;
                self.combat_slots[id as usize] = slots;
            } else {
                self.registry.rows.push(row);
                self.combat_slots.push(slots);
            }
        }
        self.registry.rebuild_name_maps();
        self.registry.revision = mint_weapon_revision();
    }

    pub fn absorb(&mut self, mut other: Self) {
        self.registry
            .vehicle_compass
            .extend(std::mem::take(&mut other.registry.vehicle_compass));
        self.registry
            .vehicle_accel
            .extend(std::mem::take(&mut other.registry.vehicle_accel));
        self.registry
            .vehicle_turrets
            .extend(std::mem::take(&mut other.registry.vehicle_turrets));
        if other.registry.is_empty() {
            self.registry.attachments.absorb(other.registry.attachments);
            self.family_tables.extend(other.family_tables);
            return;
        }
        if self.registry.rows.is_empty() {
            other.registry.vehicle_turrets = std::mem::take(&mut self.registry.vehicle_turrets);
            other.registry.vehicle_compass = std::mem::take(&mut self.registry.vehicle_compass);
            other.registry.vehicle_accel = std::mem::take(&mut self.registry.vehicle_accel);
            *self = other;
            return;
        }
        self.registry
            .rows
            .extend(other.registry.rows.into_iter().skip(1));
        self.registry.attachments.absorb(other.registry.attachments);
        self.combat_slots
            .extend(other.combat_slots.into_iter().skip(1));
        self.family_tables.extend(other.family_tables);
        self.registry.item_groups.extend(other.registry.item_groups);
        self.registry.rebuild_name_maps();
        self.registry.revision = mint_weapon_revision();
    }

    /// Finish pointer lookup against the source zone before moving its definitions.
    pub fn resolve_zone_combat_fx(
        &mut self,
        fx: &crate::FxCatalog,
        tracers: &crate::TracerCatalog,
    ) {
        self.resolve_combat_fx(fx, tracers);
        for slots in &mut self.combat_slots {
            slots.closed = true;
        }
    }

    pub fn resolve_combat_fx(&mut self, fx: &crate::FxCatalog, tracers: &crate::TracerCatalog) {
        let n = self.registry.rows.len().min(self.combat_slots.len());
        for i in 1..n {
            let category = {
                let row = &self.registry.rows[i];
                self.registry
                    .item_groups
                    .get(&(row.namespace, row.name.clone()))
                    .and_then(|group| crate::cac_category_from_item_group(group))
            };
            let row = &mut self.registry.rows[i];
            let Some(namespace) = row.component_namespace(WeaponComponent::Effect) else {
                row.combat_fx.refuse_preparation();
                continue;
            };
            stamp_combat_fx(
                &mut row.combat_fx,
                self.combat_slots[i],
                namespace,
                fx,
                tracers,
            );
            if row.namespace != crate::FamilyId::IW4 {
                stamp_mw2_tracer(&mut row.combat_fx, &row.facts, category, tracers);
            }
            row.preparation.bind_combat_effects(
                &mut row.combat_fx,
                self.combat_slots[i],
                fx,
                tracers,
            );
        }
    }

    pub fn resolve_hud_material_edges(&mut self, materials: &crate::MaterialDefinitions) {
        for row in &mut self.registry.rows {
            let overlay = row.preparation.bind_material(
                row.overlay_material.as_deref(),
                row.overlay_material_from_slot,
                materials,
            );
            if row.overlay_image.is_none()
                && let Some(index) = overlay.bound()
                && let Some(material) = materials.materials.get(index.order())
            {
                row.overlay_image = materials.hud_image_name(material).map(str::to_owned);
            }
            row.reticle.center_edge = row.preparation.bind_material(
                row.reticle.center_material.as_deref(),
                row.reticle.center_authored,
                materials,
            );
            row.reticle.side_edge = row.preparation.bind_material(
                row.reticle.side_material.as_deref(),
                row.reticle.side_authored,
                materials,
            );
            for (edge, image) in [
                (row.reticle.center_edge, &mut row.reticle.center_image),
                (row.reticle.side_edge, &mut row.reticle.side_image),
            ] {
                if let Some(index) = edge.bound()
                    && let Some(material) = materials.materials.get(index.order())
                {
                    *image = materials.hud_image_name(material).map(str::to_owned);
                }
            }
            row.hud_material_edges = WeaponHudMaterialEdges {
                overlay,
                hud_icon: row.preparation.bind_material(
                    row.hud_icon.as_deref(),
                    row.hud_icon_from_slot,
                    materials,
                ),
                pickup_icon: row.preparation.bind_material(
                    row.pickup_icon.as_deref(),
                    row.pickup_icon_authored,
                    materials,
                ),
                kill_icon: row.preparation.bind_material(
                    row.kill_icon.as_deref(),
                    row.kill_icon_from_slot,
                    materials,
                ),
            };
        }
    }

    pub fn resolve_projectile_fx_edges(&mut self, fx: &crate::FxCatalog) {
        for row in &mut self.registry.rows {
            row.projectile_fx = WeaponProjectileFx {
                trail: row.preparation.bind_fx(
                    fx,
                    row.proj_trail_from_slot,
                    row.proj_trail.as_deref(),
                ),
                beacon: row.preparation.bind_fx(
                    fx,
                    row.proj_beacon_from_slot,
                    row.proj_beacon.as_deref(),
                ),
                ignition: row.preparation.bind_fx(
                    fx,
                    row.proj_ignition_from_slot,
                    row.proj_ignition.as_deref(),
                ),
            };
        }
    }

    pub fn resolve_sz_xanim_edges(&mut self, xanims: &crate::XAnimCatalog) {
        let companions: Vec<_> = self
            .registry
            .rows
            .iter()
            .map(|row| {
                if !row.facts.dual_wield || xanims_idle(&row.sz_xanims).is_none() {
                    return None;
                }
                let name = row.dual_wield_weapon.as_deref()?;
                let id = self
                    .registry
                    .by_namespaced
                    .get(&(row.namespace, normalize_weapon_name(name)))?;
                let companion = &self.registry.rows[*id as usize];
                Some((
                    companion.gun_xmodel.clone(),
                    companion.sz_xanims_left.clone(),
                    companion.preparation.clone(),
                ))
            })
            .collect();
        for (row, companion) in self.registry.rows.iter_mut().zip(companions) {
            if let Some((gun, anims, preparation)) = companion {
                row.preparation.absorb_references(&preparation);
                row.secondary_gun_xmodel = gun;
                row.sz_xanims_right = row.sz_xanims.clone();
                row.sz_xanims_left = anims;
            }
        }
        for row in &mut self.registry.rows {
            let mut edges = [AssetEdge::Absent; WEAPON_ANIM_SLOTS];
            for (edge, hint) in edges.iter_mut().zip(row.sz_xanims.iter()) {
                *edge = row.preparation.bind_anim(xanims, hint.as_deref());
            }
            row.sz_xanim_edges = edges;
            row.sz_xanim_right_edges = std::array::from_fn(|slot| {
                row.preparation
                    .bind_anim(xanims, row.sz_xanims_right[slot].as_deref())
            });
            row.sz_xanim_left_edges = std::array::from_fn(|slot| {
                row.preparation
                    .bind_anim(xanims, row.sz_xanims_left[slot].as_deref())
            });
        }
    }

    pub fn complete_alternate_raise_times(&mut self, xanims: &crate::XAnimCatalog) -> usize {
        let mut timed = 0;
        for row in &mut self.registry.rows {
            let Some(complete) =
                crate::weapon_policy(row.namespace).and_then(|p| p.runtime.alternate_raise_time)
            else {
                continue;
            };
            if row.alternate_weapon.is_none() {
                continue;
            }
            let Some(clip) = row.sz_xanim_edges[weap_anim::ALT_RAISE]
                .bound_index()
                .and_then(|index| xanims.clip_at(index))
            else {
                continue;
            };
            if let Some(time) = complete(row.facts.alternate_raise_time_ms, clip.duration()) {
                row.facts.alternate_raise_time_ms = time;
                timed += 1;
            }
        }
        timed
    }

    pub fn resolve_notetrack_actions(&mut self, xanims: &crate::XAnimCatalog) -> (usize, usize) {
        let mut linked = 0;
        let mut inline = 0;
        for row in &mut self.registry.rows {
            let mut actions: HashMap<String, LinkedNotetrackAction> = HashMap::new();
            match row.sounds.notetrack_convention {
                NotetrackConvention::SoundMap => {
                    for (note, alias) in &row.sounds.notetrack_sound_map {
                        if !alias.is_empty() {
                            let action = actions.entry(note.to_ascii_lowercase()).or_default();
                            if action.sound_alias.is_none() {
                                action.sound_alias = Some(alias.clone());
                            }
                        }
                    }
                    for (note, alias) in &row.sounds.notetrack_rumble_map {
                        if !alias.is_empty() {
                            let action = actions.entry(note.to_ascii_lowercase()).or_default();
                            if action.rumble_alias.is_none() {
                                action.rumble_alias = Some(alias.clone());
                            }
                        }
                    }
                }
                NotetrackConvention::InlinePrefix => {
                    let indices: std::collections::BTreeSet<_> = row
                        .sz_xanim_edges
                        .iter()
                        .chain(&row.sz_xanim_right_edges)
                        .chain(&row.sz_xanim_left_edges)
                        .filter_map(|edge| edge.bound_index())
                        .collect();
                    for index in indices {
                        let Some(clip) = xanims.clip_at(index) else {
                            continue;
                        };
                        for notify in &clip.notifies {
                            let sound =
                                crate::parse_prefixed_notetrack(&notify.name, T5_NOTE_SOUND_PREFIX);
                            let rumble = crate::parse_prefixed_notetrack(
                                &notify.name,
                                T5_NOTE_RUMBLE_PREFIX,
                            );
                            if sound.is_none() && rumble.is_none() {
                                continue;
                            }
                            let action =
                                actions.entry(notify.name.to_ascii_lowercase()).or_default();
                            if let Some(alias) = sound {
                                row.preparation
                                    .declare_notetrack_sound(xanims.name_at(index), alias);
                            }
                            action.sound_alias = sound.map(str::to_owned);
                            action.rumble_alias = rumble.map(str::to_owned);
                        }
                    }
                }
            }
            linked += actions.len();
            if row.sounds.notetrack_convention == NotetrackConvention::InlinePrefix {
                inline += actions.len();
            }
            row.notetrack_actions = actions;
        }
        (linked, inline)
    }

    pub fn apply_stats_item_groups(&mut self, table: &crate::CapturedStringTable) {
        for id in 1..=self.len() as u32 {
            let Some(ns) = self.identity_namespace_of(id) else {
                continue;
            };
            let name = self.name_of(id).to_owned();
            if name.is_empty() {
                continue;
            }
            if let Some(group) = crate::item_group_for_weapon(table, &name) {
                self.registry
                    .item_groups
                    .insert((ns, name), group.to_owned());
            }
        }
    }

    pub fn set_material_camouflages(
        &mut self,
        namespace: crate::FamilyId,
        choices: std::collections::BTreeMap<String, Vec<crate::WeaponCamouflage>>,
        materials: &asset_material::MaterialCatalog,
    ) -> usize {
        let choices: std::collections::BTreeMap<_, Arc<[crate::WeaponCamouflage]>> = choices
            .into_iter()
            .map(|(name, camos)| {
                let camos = camos
                    .into_iter()
                    .filter(|camo| {
                        !camo.materials.is_empty()
                            && camo
                                .materials
                                .iter()
                                .all(|(_, key)| materials.material_index_by_key(key).is_some())
                    })
                    .collect::<Vec<_>>();
                (normalize_weapon_name(&name), camos.into())
            })
            .collect();
        let mut count = 0;
        for row in &mut self.registry.rows {
            if row.namespace == namespace
                && let Some(camos) = choices.get(&row.name)
            {
                row.material_camos = Arc::clone(camos);
                count += usize::from(!camos.is_empty());
            }
        }
        count
    }

    pub fn prepare_model_camouflages(
        &mut self,
        namespace: crate::FamilyId,
        table: &crate::CapturedStringTable,
        materials: &asset_material::MaterialCatalog,
        fpv: &crate::FpvMeshCatalog,
    ) -> usize {
        let Some(compiler) = crate::weapon_policy(namespace)
            .and_then(|p| p.camouflage.model.map(|model| model.choices))
        else {
            return 0;
        };
        let choices = compiler(table, materials);
        let mut count = 0;
        for row in &mut self.registry.rows {
            if row.namespace != namespace {
                continue;
            }
            row.camo_models.choices = choices
                .iter()
                .filter(|choice| {
                    row.gun_xmodel_edge
                        .bound_index()
                        .and_then(|index| fpv.get_at(index))
                        .zip(
                            row.camo_view_edges
                                .iter()
                                .find(|(slot, _)| *slot == choice.slot)
                                .and_then(|(_, edge)| edge.bound_index())
                                .and_then(|index| fpv.get_at(index)),
                        )
                        .is_some_and(|(base, camo)| {
                            base.skel.surfaces_for_lod(0) == camo.skel.surfaces_for_lod(0)
                        })
                        && row
                            .camo_world_edges
                            .iter()
                            .any(|(slot, edge)| *slot == choice.slot && edge.is_bound())
                })
                .cloned()
                .collect();
            count += usize::from(!row.camo_models.choices.is_empty());
        }
        count
    }

    pub fn prepare_material_camouflages(
        &mut self,
        namespace: crate::FamilyId,
        table: &crate::CapturedStringTable,
        choices: &crate::CapturedStringTable,
        materials: &mut asset_material::MaterialCatalog,
        fpv: &asset_model::FpvMeshBuild,
        world: &asset_model::WorldWeaponBuild,
    ) -> usize {
        let mut prepared = HashMap::new();
        let Some(factory) = crate::weapon_policy(namespace)
            .and_then(|p| p.camouflage.materials.map(|materials| materials.compiler))
        else {
            return 0;
        };
        let mut compiler = factory();
        let mut count = 0;
        for row in &mut self.registry.rows {
            if row.namespace != namespace {
                continue;
            }
            let Some(parent) = row.skin_parent.as_deref() else {
                continue;
            };
            let key = (
                parent.to_owned(),
                row.gun_xmodel.clone(),
                row.world_model.clone(),
            );
            let camos = prepared.entry(key).or_insert_with(|| {
                let mut keys = Vec::new();
                if let Some(entry) = row
                    .gun_xmodel
                    .as_deref()
                    .and_then(|name| fpv.get(namespace, name))
                {
                    keys.extend(entry.material_keys.iter().flatten().cloned());
                }
                if let Some(entry) = row
                    .world_model
                    .as_deref()
                    .and_then(|name| world.get(namespace, name))
                {
                    keys.extend(entry.material_keys.iter().flatten().cloned());
                }
                keys.sort_by(|a, b| a.name.cmp(&b.name));
                keys.dedup();
                Arc::<[crate::WeaponCamouflage]>::from(
                    compiler.prepare(table, choices, parent, &keys, materials),
                )
            });
            row.material_camos = Arc::clone(camos);
            count += usize::from(!camos.is_empty());
        }
        count
    }

    pub fn apply_stats_tables<'a>(
        &mut self,
        tables: impl IntoIterator<Item = &'a crate::CapturedStringTable>,
    ) {
        for table in tables {
            if crate::is_stats_table_name(&table.name) {
                self.apply_stats_item_groups(table);
            }
        }
    }

    pub fn stamp_projectile_model_edges(
        &mut self,
        catalog: &crate::ProjectileMeshCatalog,
        zone: ZoneOwner,
    ) {
        for row in &mut self.registry.rows {
            row.projectile_model_edge =
                row.preparation
                    .bind_projectile(row.projectile_model.as_deref(), catalog, zone);
        }
    }
}

fn normalize_weapon_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_suffix("_mp").unwrap_or(&lower).to_owned()
}

pub fn gsc_weapon_script_name(catalog_bare: &str) -> String {
    if catalog_bare.is_empty() {
        return String::new();
    }
    if catalog_bare.ends_with("_mp") {
        catalog_bare.to_owned()
    } else {
        format!("{catalog_bare}_mp")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FpvAssemblyCensus {
    pub built: usize,
    pub linked: usize,
    pub refused: usize,
    pub clip_tables: usize,
}

impl WeaponCatalog {
    pub fn attachment_data_mut(&mut self) -> &mut WeaponAttachments {
        &mut self.attachments
    }
}
impl WeaponRegistry {
    pub fn attachment_data(&self) -> &WeaponAttachments {
        &self.attachments
    }
}

impl WeaponBuild {
    pub fn configuration_draft_of(&self, id: u32) -> Option<crate::WeaponConfigurationDraft> {
        let row = self.registry.rows.get(id as usize)?;
        Some(crate::WeaponConfigurationDraft {
            name: row.name.clone(),
            attachment_caption_keys: row.attachment_caption_keys.clone(),
            attachment_view_ads_models: row.attachment_view_ads_models.clone(),
            hide_tags: row.hide_tags.clone(),
            prepared_attachments: row.prepared_attachments.clone(),
            alternate_index: row.alternate_index,
            indexed_name: row.indexed_name,
            namespace: row.namespace,
            alternate_weapon: row.alternate_weapon.clone(),
            facts: row.facts,
            attachment_assets: row.attachment_assets.clone(),
            native_capture: row.native_capture.clone(),
            attachment_view_models: row.attachment_view_models.clone(),
            attachment_world_models: row.attachment_world_models.clone(),
            reticle: row.reticle.clone(),
            overlay_material: row.overlay_material.clone(),
            overlay_image: row.overlay_image.clone(),
            overlay_material_from_slot: row.overlay_material_from_slot,
            projectile_model: row.projectile_model.clone(),
            combat_fx: row.combat_fx.clone(),
            proj_trail: row.proj_trail.clone(),
            proj_trail_from_slot: row.proj_trail_from_slot,
            proj_ignition: row.proj_ignition.clone(),
            proj_ignition_from_slot: row.proj_ignition_from_slot,
            sounds: row.sounds.clone(),
            sz_xanims: row.sz_xanims.clone(),
        })
    }
    pub fn set_configuration_draft(
        &mut self,
        id: u32,
        draft: crate::WeaponConfigurationDraft,
    ) -> bool {
        let Some(row) = self.registry.rows.get_mut(id as usize) else {
            return false;
        };
        if row.namespace != draft.namespace {
            return false;
        }
        row.name = draft.name;
        row.attachment_caption_keys = draft.attachment_caption_keys;
        row.attachment_view_ads_models = draft.attachment_view_ads_models;
        row.hide_tags = draft.hide_tags;
        row.prepared_attachments = draft.prepared_attachments;
        row.alternate_index = draft.alternate_index;
        row.indexed_name = draft.indexed_name;
        row.namespace = draft.namespace;
        row.alternate_weapon = draft.alternate_weapon;
        row.facts = draft.facts;
        row.attachment_assets = draft.attachment_assets;
        row.native_capture = draft.native_capture;
        row.attachment_view_models = draft.attachment_view_models;
        row.attachment_world_models = draft.attachment_world_models;
        row.reticle = draft.reticle;
        row.overlay_material = draft.overlay_material;
        row.overlay_image = draft.overlay_image;
        row.overlay_material_from_slot = draft.overlay_material_from_slot;
        row.projectile_model = draft.projectile_model;
        row.combat_fx = draft.combat_fx;
        row.proj_trail = draft.proj_trail;
        row.proj_trail_from_slot = draft.proj_trail_from_slot;
        row.proj_ignition = draft.proj_ignition;
        row.proj_ignition_from_slot = draft.proj_ignition_from_slot;
        row.sounds = draft.sounds;
        row.sz_xanims = draft.sz_xanims;
        true
    }
}

impl WeaponCatalog {
    /// Publish a capture from the owner configured on this zone walk.
    pub fn insert_capture(&mut self, input: crate::WeaponCaptureInput) {
        assert_eq!(
            Some(input.namespace),
            self.capture_ns,
            "weapon capture owner differs from zone owner"
        );
        fn address(slot: asset_material::AssetPointerIdentity) -> Ptr {
            Ptr {
                block: slot.block,
                offset: slot.offset,
            }
        }
        self.entries.push(CatalogWeapon {
            namespace: input.namespace,
            name: input.name,
            alternate_weapon: input.alternate_weapon,
            impact_payload: input.impact_payload,
            weap_def: input.weap_def.map(|slot| (slot.block, slot.offset)),
            display_name_key: input.display_name_key,
            reticle: input.reticle,
            hud_material_edges: input.hud_material_edges,
            overlay_material: input.overlay_material,
            overlay_image: input.overlay_image,
            reticle_center_slot: input.reticle_center_slot.map(address),
            reticle_side_slot: input.reticle_side_slot.map(address),
            overlay_material_slot: input.overlay_material_slot.map(address),
            attachment_assets: input.attachment_assets,
            attached_models: input.attached_models,
            native_capture: input.native_capture,
            hud_icon: input.hud_icon,
            hud_icon_slot: input.hud_icon_slot.map(address),
            pickup_icon: input.pickup_icon,
            pickup_icon_slot: input.pickup_icon_slot.map(address),
            pickup_icon_image: input.pickup_icon_image,
            pickup_icon_ratio: input.pickup_icon_ratio,
            hud_icon_ratio: input.hud_icon_ratio,
            hud_icon_image: input.hud_icon_image,
            dpad_icon: input.dpad_icon,
            dpad_icon_image: input.dpad_icon_image,
            dpad_icon_atlas: input.dpad_icon_atlas,
            dpad_icon_ratio: input.dpad_icon_ratio,
            kill_icon: input.kill_icon,
            kill_icon_slot: input.kill_icon_slot.map(address),
            kill_icon_image: input.kill_icon_image,
            proj_trail: input.proj_trail,
            proj_trail_slot: input.proj_trail_slot.map(address),
            proj_beacon: input.proj_beacon,
            proj_beacon_slot: input.proj_beacon_slot.map(address),
            proj_ignition: input.proj_ignition,
            proj_ignition_slot: input.proj_ignition_slot.map(address),
            projectile_fx: input.projectile_fx,
            gun_xmodel: input.gun_xmodel,
            hand_xmodel: input.hand_xmodel,
            dual_wield_weapon: input.dual_wield_weapon,
            clip_model: input.clip_model,
            world_model: input.world_model,
            camo_models: input.camo_models,
            skin_parent: input.skin_parent,
            projectile_model: input.projectile_model,
            rocket_model: input.rocket_model,
            knife_xmodel: input.knife_xmodel,
            sz_xanims: input.sz_xanims,
            sz_xanims_right: input.sz_xanims_right,
            sz_xanims_left: input.sz_xanims_left,
            hide_tags: input.hide_tags,
            sounds: input.sounds,
            combat_fx: input.combat_fx,
            flame: input.flame,
            combat_slots: CombatFxSlots {
                closed: false,
                view_flash: input.combat_slots.view_flash.map(address),
                world_flash: input.combat_slots.world_flash.map(address),
                view_shell_eject: input.combat_slots.view_shell_eject.map(address),
                world_shell_eject: input.combat_slots.world_shell_eject.map(address),
                view_last_shot_eject: input.combat_slots.view_last_shot_eject.map(address),
                world_last_shot_eject: input.combat_slots.world_last_shot_eject.map(address),
                explosion: input.combat_slots.explosion.map(address),
                tracer: input.combat_slots.tracer.map(address),
            },
            facts: input.facts,
        });
    }
}

impl WeaponBuild {
    pub fn prepare_family_models(
        &mut self,
        fpv: &asset_model::FpvMeshCatalog,
        input: &crate::WeaponModelPreparation<'_>,
    ) -> Vec<String> {
        let mut refusals = Vec::new();
        for row in &mut self.registry.rows {
            if row.namespace != input.family {
                continue;
            }
            row.preparation = WeaponPreparationRecipe::for_capture(row.namespace, &row.name);
            row.hand_xmodel = input
                .hands
                .filter(|name| fpv.get(input.family, name).is_some())
                .map(str::to_owned);
            row.fpv_soldiers = [None, None];
            row.fpv_mount_plan = None;
            row.fpv_assemblies = [None, None];
            if row
                .gun_xmodel
                .as_deref()
                .is_some_and(|name| fpv.get(row.namespace, name).is_none())
            {
                let refusal = format!(
                    "weapon {} is missing native component {:?}",
                    row.name,
                    WeaponComponent::ViewModel
                );
                diag::warn_once!(Fpv, "{refusal}");
                row.preparation.refusal = Some(refusal.clone());
                refusals.push(refusal);
            }
            row.sounds.notetrack_convention = input.notetrack_convention;
        }
        self.registry.revision = mint_weapon_revision();
        refusals
    }
}
