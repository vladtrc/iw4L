use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use assets::{
    AssetEdge, AssetNamespace, FpvHands, FpvMeshCatalog, FpvMeshIndex, FpvSideAssemblies,
    PreparedFpvMeshes, PreparedWeapons, PreparedXAnims, TS_COLOR_MAP, WEAPON_ANIM_SLOTS,
    WeaponAnimations, WeaponBodyFacts, WeaponRegistry, XAnimCatalog,
};
use bevy::prelude::*;
use render_material::{RuntimeMaterialCatalog, RuntimeSortedMaterialTable};
use render_scene::{SmodelPassMaterial, TessMaterials, WorldModelLightingAtlas};

use crate::anim::fpv_rig::{
    FpvMandatoryRefusal, FpvMaterialAdmission, FpvSurfaceVerdict, PreparedFpvComposition,
    PreparedFpvModel, PreparedFpvRig, compose_clip_tracks,
};
use crate::gaps::RenderGapCause;

const PREPARE_FRAME_BUDGET: std::time::Duration = std::time::Duration::from_millis(6);

const NO_COLOUR_MAP: &str = "technique samples no colour map";

#[derive(Default)]
pub struct FpvRigSet {
    bare: [Option<Arc<PreparedFpvRig>>; 2],
    rocket: [Option<Arc<PreparedFpvRig>>; 2],
}

impl FpvRigSet {
    pub fn pick(&self, rocket: bool, dual: bool) -> Option<&Arc<PreparedFpvRig>> {
        let hand = usize::from(dual);
        match (rocket, &self.rocket[hand]) {
            (true, Some(rig)) => Some(rig),
            _ => self.bare[hand].as_ref(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FpvViewCensus {
    pub gun_colormap_skip_n: u32,
    pub gun_colormap_skip_names: Vec<String>,
    pub mat_hints: Vec<String>,
}

pub struct FpvWeaponView {
    pub gun_name: String,
    pub gun_index: FpvMeshIndex,
    pub hands: FpvHands,
    pub hands_index: FpvMeshIndex,
    pub namespace: AssetNamespace,
    pub assemblies: FpvSideAssemblies,
    pub right: WeaponAnimations,
    pub left: Option<WeaponAnimations>,
    pub idle_name: Option<String>,
    pub rigs: FpvRigSet,
    pub census: FpvViewCensus,
}

pub enum FpvWeaponSlot {
    Absent,
    Ready(Arc<FpvWeaponView>),
    Refused(RenderGapCause),
}

static ABSENT_SLOT: FpvWeaponSlot = FpvWeaponSlot::Absent;

#[derive(Clone, Copy, Debug, Default)]
pub struct FpvPreparationCensus {
    pub models: usize,
    pub layouts: usize,
    pub compositions: usize,
    pub rigs: usize,
    pub materials: usize,
    pub elapsed_ms: f64,
}

pub struct FpvWeaponTable {
    material_catalog: Arc<RuntimeMaterialCatalog>,
    catalog_id: u64,
    facts: Vec<Option<WeaponBodyFacts>>,
    hud_iris: Vec<bool>,
    guns: Vec<Option<FpvMeshIndex>>,
    slots: Vec<[FpvWeaponSlot; 2]>,
    alternate_slots: HashMap<(u32, u32), [FpvWeaponSlot; 2]>,
    census: FpvPreparationCensus,
}

impl FpvWeaponTable {
    pub fn census(&self) -> FpvPreparationCensus {
        self.census
    }

    pub fn material_catalog(&self) -> &Arc<RuntimeMaterialCatalog> {
        &self.material_catalog
    }

    pub fn catalog_id(&self) -> u64 {
        self.catalog_id
    }

    pub fn facts_of(&self, weapon: u32) -> Option<WeaponBodyFacts> {
        self.facts.get(weapon as usize).copied().flatten()
    }

    pub fn overlay_is_hud_iris(&self, weapon: u32) -> bool {
        self.hud_iris.get(weapon as usize).copied().unwrap_or(false)
    }

    pub fn gun_index(&self, weapon: u32) -> Option<FpvMeshIndex> {
        self.guns.get(weapon as usize).copied().flatten()
    }

    pub fn slot(&self, weapon: u32, parent: u32, axis: bool) -> &FpvWeaponSlot {
        if parent != 0 && self.facts_of(weapon).is_some_and(|f| f.inventory_type == 3) {
            return self
                .alternate_slots
                .get(&(weapon, parent))
                .map_or(&ABSENT_SLOT, |s| &s[usize::from(axis)]);
        }
        self.slots
            .get(weapon as usize)
            .map_or(&ABSENT_SLOT, |sides| &sides[usize::from(axis)])
    }
}

struct FpvPreparationOwner {
    materials: Arc<RuntimeMaterialCatalog>,
    images: Arc<Vec<Option<Handle<Image>>>>,
    weapons: Arc<WeaponRegistry>,
    catalog_id: u64,
    atlas: Option<bevy::asset::AssetId<Image>>,
}

impl FpvPreparationOwner {
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.materials, &other.materials)
            && Arc::ptr_eq(&self.images, &other.images)
            && Arc::ptr_eq(&self.weapons, &other.weapons)
            && self.catalog_id == other.catalog_id
            && self.atlas == other.atlas
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FpvPreparationStage {
    Admit,
    Layout,
    Weapons,
    Done,
}

type RigKey = (usize, bool, Vec<Option<usize>>, Vec<Option<usize>>);

struct FpvPreparationJob {
    owner: FpvPreparationOwner,
    stage: FpvPreparationStage,
    started: std::time::Instant,
    progress: Option<assets::StageHandle>,
    work_total: u64,
    work_done: u64,

    models: Vec<usize>,
    next_model: usize,
    admission: FpvMaterialAdmission,
    image_cache: HashMap<u32, Handle<Image>>,

    layouts_queue: Vec<(usize, Option<[u32; 6]>)>,
    next_layout: usize,
    layouts: HashMap<(usize, Option<[u32; 6]>), Result<Arc<PreparedFpvModel>, String>>,

    next_weapon: u32,
    alternate_queue: Vec<(u32, u32)>,
    compositions: HashMap<usize, Result<Arc<PreparedFpvComposition>, String>>,
    tracks: HashMap<(usize, usize), Option<Arc<[u16]>>>,
    rigs: HashMap<RigKey, Arc<PreparedFpvRig>>,
    facts: Vec<Option<WeaponBodyFacts>>,
    hud_iris: Vec<bool>,
    guns: Vec<Option<FpvMeshIndex>>,
    slots: Vec<[FpvWeaponSlot; 2]>,
    alternate_slots: HashMap<(u32, u32), [FpvWeaponSlot; 2]>,
    refused: Vec<String>,
}

#[derive(Resource, Default)]
pub struct PreparedFpv {
    job: Option<FpvPreparationJob>,
    table: Option<(FpvPreparationOwner, Arc<FpvWeaponTable>)>,
}

impl PreparedFpv {
    pub fn table(&self) -> Option<&Arc<FpvWeaponTable>> {
        self.table.as_ref().map(|(_, table)| table)
    }

    pub fn settled_for(&self, materials: &Arc<RuntimeMaterialCatalog>) -> bool {
        self.job.is_none()
            && self
                .table()
                .is_some_and(|table| Arc::ptr_eq(&table.material_catalog, materials))
    }

    pub fn clear(&mut self) {
        if let Some(job) = self.job.take()
            && let Some(stage) = job.progress
        {
            stage.cancel();
        }
        self.table = None;
    }
}

fn assembly_key(assembly: &Arc<assets::FpvAssembly>) -> usize {
    Arc::as_ptr(assembly) as usize
}

fn composition_key(composition: &Arc<PreparedFpvComposition>) -> usize {
    Arc::as_ptr(composition) as usize
}

#[allow(clippy::too_many_arguments)]
fn admit_surface(
    global: &RuntimeMaterialCatalog,
    images: &[Option<Handle<Image>>],
    entry: &assets::FpvMeshEntry,
    surface_index: usize,
    lighting: &WorldModelLightingAtlas,
    admission: &mut FpvMaterialAdmission,
    image_cache: &mut HashMap<u32, Handle<Image>>,
) -> FpvSurfaceVerdict {
    use lighting_iw4::{
        MODEL_LIGHTING_INV_ATLAS_WIDTH, MODEL_LIGHTING_VOLUME_W, model_lighting_inv_image_height,
        model_lighting_lookup_scale,
    };

    let edge = entry
        .material_edges
        .get(surface_index)
        .copied()
        .unwrap_or(AssetEdge::Absent);
    let leftover = entry
        .material_names
        .get(surface_index)
        .and_then(|name| name.as_deref())
        .unwrap_or("-");
    let refused = |material: &str, cause: &'static str| FpvSurfaceVerdict::Refused {
        material: material.to_owned(),
        cause,
    };
    let bound = match edge {
        AssetEdge::Absent => return FpvSurfaceVerdict::Inapplicable("no material"),
        AssetEdge::Unresolved(_) => return refused(leftover, edge.edge_kind()),
        AssetEdge::Bound(_) => match edge.bound_index() {
            Some(bound) => bound,
            None => return refused(leftover, "bound edge without an index"),
        },
    };
    let Some(present_name) = entry.material_present_name(surface_index) else {
        return refused(leftover, "bound material has no name");
    };
    let Some(mat_i) = entry
        .skel
        .surface_materials
        .get(surface_index)
        .copied()
        .flatten()
        .map(|index| index.get())
    else {
        return refused(present_name, "surface carries no authored material");
    };
    if let Some(&row) = admission.by_authored.get(&mat_i) {
        return FpvSurfaceVerdict::Admitted(row);
    }
    let Some(authored) = global.materials.get(bound) else {
        return refused(present_name, "material outside the session catalog");
    };
    let Some(ordinal) = global.ordinal_for_material_name(present_name) else {
        return refused(present_name, "material has no sorted ordinal");
    };
    let Some(color_image) = authored
        .textures
        .iter()
        .find_map(|(_, texture)| texture.filter(|binding| binding.semantic == TS_COLOR_MAP))
        .map(|binding| binding.image.0)
    else {
        return FpvSurfaceVerdict::Inapplicable(NO_COLOUR_MAP);
    };
    let mut image = |image_index: u32| -> Option<Handle<Image>> {
        if let Some(cached) = image_cache.get(&image_index) {
            return Some(cached.clone());
        }
        let handle = images.get(image_index as usize).cloned().flatten()?;
        image_cache.insert(image_index, handle.clone());
        Some(handle)
    };
    let Some(color) = image(color_image) else {
        return refused(present_name, "colour map not uploaded");
    };
    let specular = authored
        .textures
        .iter()
        .find_map(|(_, texture)| {
            texture.filter(|binding| binding.semantic == assets::TS_SPECULAR_MAP)
        })
        .and_then(|binding| image(binding.image.0));

    const ENV_MAP_PARMS: u32 = 1_033_475_292;
    let env_map_parms = authored
        .constants
        .iter()
        .find(|(hash, _)| *hash == ENV_MAP_PARMS)
        .map(|(_, words)| words.map(f32::from_bits))
        .unwrap_or([0.0; 4]);
    let Some(inv_h) = model_lighting_inv_image_height(lighting.dims.image_height) else {
        return refused(present_name, "model lighting atlas has no rows");
    };
    let scale = model_lighting_lookup_scale(inv_h);
    let cull_mode =
        authored
            .state_bits_table
            .first()
            .and_then(|bits| match assets::cull_face_from_state_bits(*bits) {
                assets::MaterialCullFace::Back => Some(bevy::render::render_resource::Face::Back),
                assets::MaterialCullFace::Front => Some(bevy::render::render_resource::Face::Front),
                assets::MaterialCullFace::None => None,
            });
    let material = SmodelPassMaterial {
        model_lighting_required: true,
        color: Some(color),
        specular,
        probe: None,
        atlas: Some(lighting.image.clone()),
        alpha_mode: AlphaMode::Opaque,
        draw_mode: None,
        cull_mode,
        env_map_parms,
        lighting_lookup_scale: [scale.u, scale.v, scale.w, scale.q],
        atlas_lookup: [
            MODEL_LIGHTING_INV_ATLAS_WIDTH as f32,
            inv_h,
            MODEL_LIGHTING_VOLUME_W,
            0.0,
        ],
        sort_key: authored.sort_key,
        material_sorted_index: Some(ordinal.get()),
    };
    let row = admission.materials.len() as u32;
    admission.materials.push(material);
    admission.by_authored.insert(mat_i, row);
    FpvSurfaceVerdict::Admitted(row)
}

impl FpvPreparationJob {
    fn new(
        owner: FpvPreparationOwner,
        fpv: &FpvMeshCatalog,
        progress: Option<assets::StageHandle>,
    ) -> Self {
        let registry = Arc::clone(&owner.weapons);
        let weapon_n = registry.len() as u32;
        let mut models: Vec<usize> = Vec::new();
        let mut seen_models: HashSet<usize> = HashSet::new();
        let mut layouts_queue: Vec<(usize, Option<[u32; 6]>)> = Vec::new();
        let mut seen_layouts: HashSet<(usize, Option<[u32; 6]>)> = HashSet::new();
        let mut seen_assemblies: HashSet<usize> = HashSet::new();
        for (id, parent) in (1..=weapon_n)
            .map(|id| (id, 0))
            .chain(registry.alternate_fpv_pairs())
        {
            for axis in [false, true] {
                let Some(sides) = registry.fpv_assemblies_for(id, parent, axis) else {
                    continue;
                };
                for assembly in std::iter::once(&sides.bare).chain(&sides.rocket) {
                    if !seen_assemblies.insert(assembly_key(assembly)) {
                        continue;
                    }
                    for part in &assembly.parts {
                        let order = part.model.order();
                        if fpv.get_at(order).is_none() {
                            continue;
                        }
                        if seen_models.insert(order) {
                            models.push(order);
                        }
                        if seen_layouts.insert((order, part.hide)) {
                            layouts_queue.push((order, part.hide));
                        }
                    }
                }
            }
        }
        let work_total =
            (models.len() + layouts_queue.len() + registry.alternate_fpv_pairs().count()) as u64
                + u64::from(weapon_n);
        if let Some(stage) = &progress {
            stage.set_total(work_total);
        }
        Self {
            owner,
            stage: FpvPreparationStage::Admit,
            started: std::time::Instant::now(),
            progress,
            work_total,
            work_done: 0,
            models,
            next_model: 0,
            admission: FpvMaterialAdmission::default(),
            image_cache: HashMap::new(),
            layouts_queue,
            next_layout: 0,
            layouts: HashMap::new(),
            next_weapon: 0,
            alternate_queue: {
                let mut pairs: Vec<_> = registry.alternate_fpv_pairs().collect();
                pairs.sort_unstable();
                pairs.reverse();
                pairs
            },
            compositions: HashMap::new(),
            tracks: HashMap::new(),
            rigs: HashMap::new(),
            facts: Vec::with_capacity(weapon_n as usize + 1),
            hud_iris: Vec::with_capacity(weapon_n as usize + 1),
            guns: Vec::with_capacity(weapon_n as usize + 1),
            slots: Vec::with_capacity(weapon_n as usize + 1),
            alternate_slots: HashMap::new(),
            refused: Vec::new(),
        }
    }

    fn tick(&mut self) {
        self.work_done = self.work_done.saturating_add(1);
        if let Some(stage) = &self.progress {
            stage.set_completed(self.work_done.min(self.work_total));
        }
    }

    fn advance(
        &mut self,
        deadline: std::time::Instant,
        fpv: &FpvMeshCatalog,
        xanims: &XAnimCatalog,
        tess: &TessMaterials,
        lighting: &WorldModelLightingAtlas,
    ) {
        while self.stage != FpvPreparationStage::Done {
            if std::time::Instant::now() >= deadline {
                return;
            }
            match self.stage {
                FpvPreparationStage::Admit => self.admit_next(fpv, tess, lighting),
                FpvPreparationStage::Layout => self.lay_out_next(fpv),
                FpvPreparationStage::Weapons => self.prepare_next_weapon(fpv, xanims),
                FpvPreparationStage::Done => {}
            }
        }
    }

    fn admit_next(
        &mut self,
        fpv: &FpvMeshCatalog,
        tess: &TessMaterials,
        lighting: &WorldModelLightingAtlas,
    ) {
        let Some(&order) = self.models.get(self.next_model) else {
            self.stage = FpvPreparationStage::Layout;
            return;
        };
        self.next_model += 1;
        if let Some(entry) = fpv.get_at(order) {
            let verdicts = entry
                .skel
                .surfaces_for_lod(0)
                .map(|surface| {
                    let verdict = admit_surface(
                        &tess.catalog,
                        tess.material_images.as_ref(),
                        entry,
                        surface,
                        lighting,
                        &mut self.admission,
                        &mut self.image_cache,
                    );
                    (surface, verdict)
                })
                .collect();
            self.admission.record(order, verdicts);
        }
        self.tick();
    }

    fn lay_out_next(&mut self, fpv: &FpvMeshCatalog) {
        let Some(&(order, hide)) = self.layouts_queue.get(self.next_layout) else {
            self.stage = FpvPreparationStage::Weapons;
            self.next_weapon = 0;
            return;
        };
        self.next_layout += 1;
        let prepared = PreparedFpvModel::build(fpv, order, hide.as_ref(), &self.admission)
            .map(Arc::new)
            .map_err(|error| error.to_string());
        self.layouts.insert((order, hide), prepared);
        self.tick();
    }

    fn composition(
        &mut self,
        assembly: &Arc<assets::FpvAssembly>,
    ) -> Result<Arc<PreparedFpvComposition>, String> {
        let key = assembly_key(assembly);
        if let Some(done) = self.compositions.get(&key) {
            return done.clone();
        }
        let layouts = &self.layouts;
        let composed = PreparedFpvComposition::compose(Arc::clone(assembly), |order, hide| {
            layouts
                .get(&(order, hide))
                .cloned()
                .unwrap_or_else(|| Err("model missing from the first-person catalog".to_owned()))
        })
        .map(Arc::new);
        self.compositions.insert(key, composed.clone());
        composed
    }

    fn clip_tracks(
        &mut self,
        composition: &PreparedFpvComposition,
        orders: &[Option<usize>],
    ) -> Vec<Option<Arc<[u16]>>> {
        let registry = Arc::clone(&self.owner.weapons);
        let assembly = composition.assembly();
        let key = assembly_key(assembly);
        orders
            .iter()
            .map(|order| {
                let order = (*order)?;
                self.tracks
                    .entry((key, order))
                    .or_insert_with(|| {
                        compose_clip_tracks(assembly, order, registry.fpv_clip_tracks())
                    })
                    .clone()
            })
            .collect()
    }

    fn rig(
        &mut self,
        composition: &Arc<PreparedFpvComposition>,
        dual: bool,
        right: &[Option<usize>],
        left: &[Option<usize>],
    ) -> Arc<PreparedFpvRig> {
        let key: RigKey = (
            composition_key(composition),
            dual,
            right.to_vec(),
            left.to_vec(),
        );
        if let Some(rig) = self.rigs.get(&key) {
            return Arc::clone(rig);
        }
        let tracks = [
            self.clip_tracks(composition, right),
            self.clip_tracks(composition, left),
        ];
        let rig = Arc::new(PreparedFpvRig::build(
            Arc::clone(composition),
            dual,
            &self.admission,
            tracks,
        ));
        self.rigs.insert(key, Arc::clone(&rig));
        rig
    }

    fn census_of(&self, fpv: &FpvMeshCatalog, gun: FpvMeshIndex) -> FpvViewCensus {
        let mut census = FpvViewCensus::default();
        let order = gun.order();
        let Some(entry) = fpv.get_at(order) else {
            return census;
        };
        for (surface, verdict) in self.admission.verdicts_of(order) {
            let name = entry
                .material_names
                .get(*surface)
                .and_then(|name| name.as_deref());
            match verdict {
                FpvSurfaceVerdict::Admitted(_) => {
                    if let Some(name) = name
                        && census.mat_hints.len() < 12
                        && !census.mat_hints.iter().any(|seen| seen == name)
                    {
                        census.mat_hints.push(name.to_owned());
                    }
                }
                FpvSurfaceVerdict::Inapplicable(NO_COLOUR_MAP) => {
                    census.gun_colormap_skip_n = census.gun_colormap_skip_n.saturating_add(1);
                    if let Some(name) = name
                        && census.gun_colormap_skip_names.len() < 8
                        && !census
                            .gun_colormap_skip_names
                            .iter()
                            .any(|seen| seen == name)
                    {
                        census.gun_colormap_skip_names.push(name.to_owned());
                    }
                }
                _ => {}
            }
        }
        census
    }

    fn prepare_next_weapon(&mut self, fpv: &FpvMeshCatalog, xanims: &XAnimCatalog) {
        let registry = Arc::clone(&self.owner.weapons);
        let (id, parent) = if self.next_weapon as usize > registry.len() {
            let Some(pair) = self.alternate_queue.pop() else {
                self.stage = FpvPreparationStage::Done;
                return;
            };
            pair
        } else {
            let id = self.next_weapon;
            self.next_weapon += 1;
            self.facts.push(registry.facts_of(id));
            self.hud_iris.push(registry.overlay_is_hud_iris(id));
            (id, 0)
        };
        let gun = registry
            .gun_xmodel_edge_of(id)
            .and_then(|edge| edge.bound_index())
            .filter(|&order| fpv.get_at(order).is_some())
            .map(FpvMeshIndex::from_order);
        if parent == 0 {
            self.guns.push(gun);
        }
        if id == 0 {
            self.slots
                .push([FpvWeaponSlot::Absent, FpvWeaponSlot::Absent]);
            return;
        }
        let Some(gun_index) = gun else {
            let cause = || RenderGapCause::FpvGunXModelUnresolved { weapon_id: id };
            let sides = [
                FpvWeaponSlot::Refused(cause()),
                FpvWeaponSlot::Refused(cause()),
            ];
            if parent == 0 {
                self.slots.push(sides);
            } else {
                self.alternate_slots.insert((id, parent), sides);
            }
            self.tick();
            return;
        };

        let right_edges = registry.sz_xanim_right_edges_of(id);
        let right_idle_bound =
            right_edges.is_some_and(|edges| edges[assets::weap_anim::IDLE].is_bound());
        let no_dual = registry
            .facts_of(id)
            .map(|f| u8::from(f.no_dual_wield))
            .unwrap_or(1);
        let create_lr = no_dual == 0 && right_idle_bound;
        let right = if create_lr {
            WeaponAnimations::from_registry_edges(&registry, id, right_edges, xanims)
        } else {
            WeaponAnimations::from_registry(&registry, id, xanims)
        };
        let left_edges = registry.sz_xanim_left_edges_of(id);
        let left_idle_bound =
            left_edges.is_some_and(|edges| edges[assets::weap_anim::IDLE].is_bound());
        let left = (create_lr && left_idle_bound)
            .then(|| WeaponAnimations::from_registry_edges(&registry, id, left_edges, xanims));
        let idle_name = registry
            .sz_xanim_edges_of(id)
            .and_then(|row| row[assets::weap_anim::IDLE].bound_index())
            .and_then(|order| xanims.clip_at(order))
            .map(|clip| clip.name.clone());
        let namespace = registry.namespace_of(id).unwrap_or(AssetNamespace::Iw4);
        let gun_name = fpv
            .get_at(gun_index.order())
            .map(|entry| entry.skel.name.clone())
            .unwrap_or_else(String::new);
        let census = self.census_of(fpv, gun_index);
        let right_orders: Vec<Option<usize>> = right.clip_orders().to_vec();
        let left_orders: Vec<Option<usize>> = left
            .as_ref()
            .map(|left| left.clip_orders().to_vec())
            .unwrap_or_else(Vec::new);
        debug_assert_eq!(right_orders.len(), WEAPON_ANIM_SLOTS);

        let sides: [FpvWeaponSlot; 2] = [false, true].map(|axis| {
            let (Some((hands, hands_index)), Some(assemblies)) = (
                registry.fpv_hands_of(id, axis),
                registry.fpv_assemblies_for(id, parent, axis),
            ) else {
                return FpvWeaponSlot::Refused(RenderGapCause::FpvDependencyUnresolved {
                    weapon_id: id,
                    role: "FPV skeleton",
                    name: registry.fpv_assembly_gap_of(id, axis),
                });
            };
            let bare = self.composition(&assemblies.bare);
            let rocket = assemblies
                .rocket
                .as_ref()
                .map(|rocket| self.composition(rocket));
            let (bare, rocket) = match (bare, rocket.transpose()) {
                (Ok(bare), Ok(rocket)) => (bare, rocket),
                (Err(error), _) | (_, Err(error)) => {
                    return FpvWeaponSlot::Refused(RenderGapCause::FpvDependencyUnresolved {
                        weapon_id: id,
                        role: "FPV layout",
                        name: error,
                    });
                }
            };
            let refusal: Option<FpvMandatoryRefusal> = bare
                .refusal()
                .or_else(|| rocket.as_ref().and_then(|rocket| rocket.refusal()))
                .cloned();
            if let Some(refusal) = refusal {
                return FpvWeaponSlot::Refused(RenderGapCause::FpvDependencyUnresolved {
                    weapon_id: id,
                    role: "FPV material",
                    name: refusal.to_string(),
                });
            }
            let mut rigs = FpvRigSet::default();
            rigs.bare[0] = Some(self.rig(&bare, false, &right_orders, &[]));
            if left.is_some() {
                rigs.bare[1] = Some(self.rig(&bare, true, &right_orders, &left_orders));
            }
            if let Some(rocket) = &rocket {
                rigs.rocket[0] = Some(self.rig(rocket, false, &right_orders, &[]));
                if left.is_some() {
                    rigs.rocket[1] = Some(self.rig(rocket, true, &right_orders, &left_orders));
                }
            }
            FpvWeaponSlot::Ready(Arc::new(FpvWeaponView {
                gun_name: gun_name.clone(),
                gun_index,
                hands: hands.clone(),
                hands_index,
                namespace,
                assemblies: assemblies.clone(),
                right: right.clone(),
                left: left.clone(),
                idle_name: idle_name.clone(),
                rigs,
                census: census.clone(),
            }))
        });
        for (axis, side) in sides.iter().enumerate() {
            if let FpvWeaponSlot::Refused(cause) = side
                && self.refused.len() < 16
            {
                self.refused.push(format!(
                    "{} ({}): {cause}",
                    registry.name_of(id),
                    if axis == 0 { "allies" } else { "axis" }
                ));
            }
        }
        if parent == 0 {
            self.slots.push(sides);
        } else {
            self.alternate_slots.insert((id, parent), sides);
        }
        self.tick();
    }

    fn finish(self) -> (FpvPreparationOwner, FpvWeaponTable) {
        let ready = self
            .slots
            .iter()
            .chain(self.alternate_slots.values())
            .flatten()
            .filter(|slot| matches!(slot, FpvWeaponSlot::Ready(_)))
            .count();
        let refused_n = self
            .slots
            .iter()
            .chain(self.alternate_slots.values())
            .flatten()
            .filter(|slot| matches!(slot, FpvWeaponSlot::Refused(_)))
            .count();
        let inapplicable = self
            .models
            .iter()
            .flat_map(|&order| self.admission.verdicts_of(order))
            .filter(|(_, verdict)| matches!(verdict, FpvSurfaceVerdict::Inapplicable(_)))
            .count();
        let census = FpvPreparationCensus {
            models: self.models.len(),
            layouts: self.layouts.len(),
            compositions: self.compositions.len(),
            rigs: self.rigs.len(),
            materials: self.admission.materials.len(),
            elapsed_ms: self.started.elapsed().as_secs_f64() * 1000.0,
        };
        diag::info!(
            Fpv,
            "fpv prepared before Ready: models={} layouts={} compositions={} rigs={} materials={} inapplicable_surfaces={inapplicable} sides ready={ready} refused={refused_n} elapsed={:.1}ms",
            census.models,
            census.layouts,
            census.compositions,
            census.rigs,
            census.materials,
            census.elapsed_ms,
        );
        for line in &self.refused {
            diag::info!(Fpv, "fpv refused before use: {line}");
        }
        if let Some(stage) = self.progress {
            stage.done();
        }
        let table = FpvWeaponTable {
            material_catalog: Arc::clone(&self.owner.materials),
            catalog_id: self.owner.catalog_id,
            facts: self.facts,
            hud_iris: self.hud_iris,
            guns: self.guns,
            slots: self.slots,
            alternate_slots: self.alternate_slots,
            census,
        };
        (self.owner, table)
    }
}

fn refused_table(
    owner: &FpvPreparationOwner,
    fpv: &FpvMeshCatalog,
    cause: RenderGapCause,
) -> FpvWeaponTable {
    let registry = &owner.weapons;
    let weapon_n = registry.len() as u32;
    let mut table = FpvWeaponTable {
        material_catalog: Arc::clone(&owner.materials),
        catalog_id: owner.catalog_id,
        facts: Vec::with_capacity(weapon_n as usize + 1),
        hud_iris: Vec::with_capacity(weapon_n as usize + 1),
        guns: Vec::with_capacity(weapon_n as usize + 1),
        slots: Vec::with_capacity(weapon_n as usize + 1),
        alternate_slots: HashMap::new(),
        census: FpvPreparationCensus::default(),
    };
    for id in 0..=weapon_n {
        table.facts.push(registry.facts_of(id));
        table.hud_iris.push(registry.overlay_is_hud_iris(id));
        table.guns.push(
            registry
                .gun_xmodel_edge_of(id)
                .and_then(|edge| edge.bound_index())
                .filter(|&order| fpv.get_at(order).is_some())
                .map(FpvMeshIndex::from_order),
        );
        table.slots.push(if id == 0 {
            [FpvWeaponSlot::Absent, FpvWeaponSlot::Absent]
        } else {
            [
                FpvWeaponSlot::Refused(cause.clone()),
                FpvWeaponSlot::Refused(cause.clone()),
            ]
        });
    }
    table
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct PrepareFpvInputs<'w> {
    weapons: Option<Res<'w, PreparedWeapons>>,
    fpv_meshes: Option<Res<'w, PreparedFpvMeshes>>,
    xanims: Option<Res<'w, PreparedXAnims>>,
    tess: Res<'w, TessMaterials>,
    lighting: Option<Res<'w, WorldModelLightingAtlas>>,
    load: Option<Res<'w, assets::MapLoadProcess>>,
}

pub fn prepare_fpv_compositions(inputs: PrepareFpvInputs, mut prepared: ResMut<PreparedFpv>) {
    let PrepareFpvInputs {
        weapons,
        fpv_meshes,
        xanims,
        tess,
        lighting,
        load,
    } = inputs;
    let (Some(weapons), Some(fpv_meshes), Some(xanims)) = (weapons, fpv_meshes, xanims) else {
        if prepared.table.is_some() || prepared.job.is_some() {
            prepared.clear();
        }
        return;
    };
    if tess.material_images.is_empty() {
        return;
    }
    let sorted_ready = matches!(
        tess.catalog.sorted_materials,
        RuntimeSortedMaterialTable::Ready { .. }
    );
    let owner = FpvPreparationOwner {
        materials: Arc::clone(&tess.catalog),
        images: Arc::clone(&tess.material_images),
        weapons: Arc::clone(&weapons.0),
        catalog_id: fpv_meshes.0.identity(),
        atlas: lighting.as_ref().map(|atlas| atlas.image.id()),
    };
    if prepared.job.is_none()
        && prepared
            .table
            .as_ref()
            .is_some_and(|(settled, _)| settled.same(&owner))
    {
        return;
    }
    let restart = prepared
        .job
        .as_ref()
        .is_none_or(|job| !job.owner.same(&owner));
    if restart {
        prepared.clear();
        let shared_refusal = if !sorted_ready {
            Some((
                RenderGapCause::FpvCatalogMissing,
                "the material generation has no sorted table",
            ))
        } else if lighting.is_none() {
            Some((
                RenderGapCause::FpvNoLightingAtlas,
                "no model lighting atlas in this world",
            ))
        } else {
            None
        };
        if let Some((cause, why)) = shared_refusal {
            diag::info!(
                Fpv,
                "fpv: {why} — every first-person weapon refused before Ready"
            );
            let table = Arc::new(refused_table(&owner, &fpv_meshes.0, cause));
            prepared.table = Some((owner, table));
            return;
        }
        let progress = load
            .as_deref()
            .filter(|process| !process.is_complete())
            .map(|process| process.progress.begin(assets::StageId::FirstPerson, None));
        prepared.job = Some(FpvPreparationJob::new(owner, &fpv_meshes.0, progress));
    }
    let Some(lighting) = lighting.as_deref() else {
        return;
    };
    let deadline = std::time::Instant::now() + PREPARE_FRAME_BUDGET;
    let done = {
        let Some(job) = prepared.job.as_mut() else {
            return;
        };
        job.advance(deadline, &fpv_meshes.0, &xanims.0, &tess, lighting);
        job.stage == FpvPreparationStage::Done
    };
    if done && let Some(job) = prepared.job.take() {
        let (owner, table) = job.finish();
        prepared.table = Some((owner, Arc::new(table)));
    }
}
