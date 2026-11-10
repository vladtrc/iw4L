use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use asset_anim::XAnimCatalog;
use asset_game::{WEAPON_ANIM_SLOTS, WeaponAnimations, WeaponFpvFacts};
use asset_material::TS_COLOR_MAP;
use asset_model::FpvMeshCatalog;
use assets::{AssetEdge, FpvMeshIndex, PreparedFpvMeshes, PreparedWeapons, PreparedXAnims};
use bevy::prelude::*;
use render_material::{RuntimeMaterialCatalog, RuntimeSortedMaterialTable};
use render_scene::{SmodelPassMaterial, TessMaterials, WorldModelLightingAtlas};

use crate::anim::fpv_rig::{
    FpvMandatoryRefusal, FpvMaterialAdmission, FpvSurfaceVerdict, PreparedFpvComposition,
    PreparedFpvModel, PreparedFpvRig, compose_clip_tracks,
};

type FpvLayoutCache = HashMap<(usize, Option<[u32; 6]>), Result<Arc<PreparedFpvModel>, String>>;

type CamoMaterialSwaps = Vec<(u8, Result<Arc<HashMap<usize, SmodelPassMaterial>>, String>)>;

const PREPARE_FRAME_BUDGET: std::time::Duration = std::time::Duration::from_millis(6);

const NO_COLOUR_MAP: &str = "technique samples no colour map";

#[path = "fpv_table.rs"]
mod table;
pub use table::FpvOwnerInputs;
use table::FpvPreparationOwner;
pub use table::{BoundFpvTable, FpvRigSet, FpvWeaponSlot, FpvWeaponTable, FpvWeaponView};

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
    progress: Option<asset_transport::StageHandle>,
    work_total: u64,
    work_done: u64,

    models: Vec<usize>,
    next_model: usize,
    camouflage_materials: Vec<usize>,
    next_camouflage: usize,
    admission: FpvMaterialAdmission,
    image_cache: HashMap<u32, Handle<Image>>,

    layouts_queue: Vec<(usize, Option<[u32; 6]>)>,
    next_layout: usize,
    layouts: FpvLayoutCache,

    next_weapon: u32,
    alternate_queue: Vec<(u32, u32, Option<String>)>,
    compositions: HashMap<usize, Result<Arc<PreparedFpvComposition>, String>>,
    tracks: HashMap<(usize, usize), Option<Arc<[u16]>>>,
    rigs: HashMap<RigKey, Arc<PreparedFpvRig>>,
    facts: Vec<Option<WeaponFpvFacts>>,
    hud_iris: Vec<bool>,
    melee: Vec<u32>,
    guns: Vec<Option<FpvMeshIndex>>,
    slots: Vec<[FpvWeaponSlot; 2]>,
    alternate_slots: HashMap<(u32, u32), [FpvWeaponSlot; 2]>,
    script_slots: HashMap<(u32, u32, String), [FpvWeaponSlot; 2]>,
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
                .is_some_and(|table| Arc::ptr_eq(table.material_catalog(), materials))
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

fn assembly_key(assembly: &Arc<asset_game::FpvAssembly>) -> usize {
    Arc::as_ptr(assembly) as usize
}

fn composition_key(composition: &Arc<PreparedFpvComposition>) -> usize {
    Arc::as_ptr(composition) as usize
}

#[path = "fpv_material.rs"]
mod material;
use material::{admit_material, admit_surface};

impl FpvPreparationJob {
    fn new(
        owner: FpvPreparationOwner,
        fpv: &FpvMeshCatalog,
        progress: Option<asset_transport::StageHandle>,
    ) -> Self {
        let registry = Arc::clone(&owner.weapons);
        let weapon_n = registry.len() as u32;
        let mut models: Vec<usize> = Vec::new();
        let mut seen_models: HashSet<usize> = HashSet::new();
        let mut layouts_queue: Vec<(usize, Option<[u32; 6]>)> = Vec::new();
        let mut seen_layouts: HashSet<(usize, Option<[u32; 6]>)> = HashSet::new();
        let mut seen_assemblies: HashSet<usize> = HashSet::new();
        for (id, parent, viewmodel) in (1..=weapon_n)
            .map(|id| (id, 0, None))
            .chain(
                registry
                    .alternate_fpv_pairs()
                    .map(|(id, parent)| (id, parent, None)),
            )
            .chain(
                registry
                    .script_fpv_variants()
                    .map(|(id, parent, name)| (id, parent, Some(name))),
            )
        {
            for axis in [false, true] {
                let Some(sides) =
                    registry.fpv_assemblies_for_viewmodel(id, parent, axis, viewmodel)
                else {
                    continue;
                };
                for assembly in std::iter::once(&sides.bare)
                    .chain(&sides.rocket)
                    .chain(&sides.melee)
                    .chain(&sides.ads)
                    .chain(&sides.jammed)
                {
                    if !seen_assemblies.insert(assembly_key(assembly)) {
                        continue;
                    }
                    for part in assembly.parts() {
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
        for id in 1..=weapon_n {
            for appearance in registry.appearances_of(id) {
                if let Some((order, _)) = appearance.view_model(fpv)
                    && seen_models.insert(order)
                {
                    models.push(order);
                }
            }
        }
        let mut camouflage_materials = HashSet::new();
        for id in 1..=weapon_n {
            for appearance in registry.appearances_of(id) {
                let Some(camo) = appearance.material_camouflage() else {
                    continue;
                };
                for (_, key) in &camo.materials {
                    if let Some(material) = owner.materials.material_for_key(key) {
                        camouflage_materials.insert(usize::from(material.asset_id.0));
                    }
                }
            }
        }
        let mut camouflage_materials: Vec<_> = camouflage_materials.into_iter().collect();
        camouflage_materials.sort_unstable();
        let work_total = (models.len()
            + camouflage_materials.len()
            + layouts_queue.len()
            + registry.alternate_fpv_pairs().count()
            + registry.script_fpv_variants().count()) as u64
            + u64::from(weapon_n);
        if let Some(stage) = &progress {
            stage.set_total(work_total);
        }
        let admission =
            FpvMaterialAdmission::new(Arc::clone(&owner.meshes), Arc::clone(&owner.materials));
        Self {
            owner,
            stage: FpvPreparationStage::Admit,
            progress,
            work_total,
            work_done: 0,
            models,
            next_model: 0,
            camouflage_materials,
            next_camouflage: 0,
            admission,
            image_cache: HashMap::new(),
            layouts_queue,
            next_layout: 0,
            layouts: HashMap::new(),
            next_weapon: 0,
            alternate_queue: {
                let mut pairs: Vec<_> = registry
                    .alternate_fpv_pairs()
                    .map(|(id, parent)| (id, parent, None))
                    .chain(
                        registry
                            .script_fpv_variants()
                            .map(|(id, parent, name)| (id, parent, Some(name.to_owned()))),
                    )
                    .collect();
                pairs.sort_unstable();
                pairs.reverse();
                pairs
            },
            compositions: HashMap::new(),
            tracks: HashMap::new(),
            rigs: HashMap::new(),
            facts: Vec::with_capacity(weapon_n as usize + 1),
            hud_iris: Vec::with_capacity(weapon_n as usize + 1),
            melee: Vec::with_capacity(weapon_n as usize + 1),
            guns: Vec::with_capacity(weapon_n as usize + 1),
            slots: Vec::with_capacity(weapon_n as usize + 1),
            alternate_slots: HashMap::new(),
            script_slots: HashMap::new(),
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
            if let Some(&bound) = self.camouflage_materials.get(self.next_camouflage) {
                self.next_camouflage += 1;
                let name = tess.catalog().parts().materials[bound].name.as_str();
                admit_material(
                    tess.catalog(),
                    &tess.material_images,
                    bound,
                    name,
                    lighting,
                    &mut self.admission,
                    &mut self.image_cache,
                );
                self.tick();
            } else {
                self.stage = FpvPreparationStage::Layout;
            }
            return;
        };
        self.next_model += 1;
        if let Some(entry) = fpv.get_at(order) {
            let verdicts = entry
                .skel
                .surfaces_for_lod(0)
                .map(|surface| {
                    let verdict = admit_surface(
                        tess.catalog(),
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
        assembly: &Arc<asset_game::FpvAssembly>,
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
        viewmodel: Option<&str>,
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
                        let clip = self.owner.clips.clip_at(order)?;
                        compose_clip_tracks(
                            assembly,
                            order,
                            &clip,
                            registry.fpv_tracks_for_viewmodel(viewmodel)?,
                        )
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
        viewmodel: Option<&str>,
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
            self.clip_tracks(composition, right, viewmodel),
            self.clip_tracks(composition, left, viewmodel),
        ];
        let rig = Arc::new(PreparedFpvRig::build(
            Arc::clone(composition),
            dual,
            &self.admission,
            tracks,
            Arc::clone(&self.owner.meshes),
            [right, left].map(|orders| {
                orders
                    .iter()
                    .map(|order| order.and_then(|order| self.owner.clips.clip_at(order)))
                    .collect()
            }),
        ));
        self.rigs.insert(key, Arc::clone(&rig));
        rig
    }

    fn camo_swaps(&self, fpv: &FpvMeshCatalog, id: u32, gun: FpvMeshIndex) -> CamoMaterialSwaps {
        let Some(base) = fpv.get_at(gun.order()) else {
            return Vec::new();
        };
        let authored = |entry: &asset_model::FpvMeshEntry, surface: usize| {
            entry
                .material_edges
                .get(surface)
                .and_then(|edge| edge.bound_index())
        };
        self.owner
            .weapons
            .appearances_of(id)
            .into_iter()
            .map(|appearance| {
                let result = (|| {
                    if let asset_game::AppearanceModelStatus::DeclaredUnavailable {
                        source,
                        reason,
                    } = appearance.view_status()
                    {
                        return Err(format!("{source}: {reason:?}"));
                    }
                    let mut swaps = HashMap::new();
                    if let Some(camo) = appearance.material_camouflage() {
                        for (from, to) in &camo.materials {
                            let source = self
                                .owner
                                .materials
                                .material_for_key(from)
                                .ok_or_else(|| format!("source material missing: {from:?}"))?;
                            let target = self
                                .owner
                                .materials
                                .material_for_key(to)
                                .ok_or_else(|| format!("target material missing: {to:?}"))?;
                            let row = self
                                .admission
                                .by_authored
                                .get(&usize::from(target.asset_id.0))
                                .ok_or_else(|| format!("target material refused: {to:?}"))?;
                            let material =
                                self.admission.materials.get(*row as usize).ok_or_else(|| {
                                    format!("target material unavailable: {to:?}")
                                })?;
                            swaps.insert(usize::from(source.asset_id.0), material.clone());
                        }
                    } else {
                        let (order, camo) = appearance
                            .view_model(fpv)
                            .ok_or_else(|| "appearance owner mismatch".to_owned())?;
                        if order == gun.order() {
                            return Ok(Arc::new(swaps));
                        }
                        if camo.skel.surfaces_for_lod(0) != base.skel.surfaces_for_lod(0)
                            || camo.skel.bone_names != base.skel.bone_names
                            || camo.skel.surface_vertex_ranges != base.skel.surface_vertex_ranges
                            || camo.skel.surface_index_ranges != base.skel.surface_index_ranges
                            || camo.skel.positions != base.skel.positions
                            || !camo.skel.has_same_vertex_attributes(&base.skel)
                            || camo.skel.indices != base.skel.indices
                        {
                            return Err("appearance topology mismatch".to_owned());
                        }
                        for surface in base.skel.surfaces_for_lod(0) {
                            let from = authored(base, surface).ok_or_else(|| {
                                format!("base material unresolved: surface {surface}")
                            })?;
                            let to = authored(camo, surface).ok_or_else(|| {
                                format!("appearance material unresolved: surface {surface}")
                            })?;
                            if from == to {
                                continue;
                            }
                            let Some(FpvSurfaceVerdict::Admitted(row)) =
                                self.admission.verdict(order, surface)
                            else {
                                return Err(format!("appearance surface refused: {surface}"));
                            };
                            let material =
                                self.admission.materials.get(*row as usize).ok_or_else(|| {
                                    format!("appearance material unavailable: {surface}")
                                })?;
                            swaps.entry(from).or_insert_with(|| material.clone());
                        }
                    }
                    Ok(Arc::new(swaps))
                })();
                (appearance.slot(), result)
            })
            .collect()
    }

    fn prepare_next_weapon(&mut self, fpv: &FpvMeshCatalog, xanims: &XAnimCatalog) {
        let registry = Arc::clone(&self.owner.weapons);
        let (id, parent, viewmodel) = if self.next_weapon as usize > registry.len() {
            let Some(pair) = self.alternate_queue.pop() else {
                self.stage = FpvPreparationStage::Done;
                return;
            };
            pair
        } else {
            let id = self.next_weapon;
            self.next_weapon += 1;
            self.facts.push(
                registry
                    .bind_published_row(id)
                    .ok()
                    .and_then(|weapon| weapon.fpv_facts()),
            );
            self.hud_iris.push(registry.overlay_is_hud_iris(id));
            self.melee.push(registry.melee_weapon_of(id));
            (id, 0, None)
        };
        let gun = registry
            .gun_xmodel_edge_of(id)
            .and_then(|edge| edge.bound_index())
            .filter(|&order| fpv.get_at(order).is_some())
            .map(FpvMeshIndex::from_order);
        if parent == 0 && viewmodel.is_none() {
            self.guns.push(gun);
        }
        if id == 0 || (registry.uses_script_viewmodels() && viewmodel.is_none()) {
            self.slots
                .push([FpvWeaponSlot::Absent, FpvWeaponSlot::Absent]);
            return;
        }
        let Some(gun_index) = gun else {
            let cause = || {
                Cow::<'static, str>::Owned(format!(
                    "gunXModel[0] unresolved for weapon {weapon_id}",
                    weapon_id = id
                ))
            };
            let sides = [
                FpvWeaponSlot::Refused(cause()),
                FpvWeaponSlot::Refused(cause()),
            ];
            if let Some(name) = viewmodel {
                self.script_slots.insert((id, parent, name), sides);
            } else if parent == 0 {
                self.slots.push(sides);
            } else {
                self.alternate_slots.insert((id, parent), sides);
            }
            self.tick();
            return;
        };

        let right_edges = registry.sz_xanim_right_edges_of(id);
        let create_lr = registry
            .bind_published_row(id)
            .ok()
            .and_then(|weapon| weapon.fpv_facts())
            .is_some_and(|facts| facts.dual_animation());
        let right = if create_lr {
            WeaponAnimations::from_registry_edges(&registry, id, right_edges, xanims)
        } else {
            WeaponAnimations::from_registry(&registry, id, xanims)
        };
        let left_edges = registry.sz_xanim_left_edges_of(id);
        let left_idle_bound =
            left_edges.is_some_and(|edges| edges[asset_iw4::size::weap_anim::IDLE].is_bound());
        let left = (create_lr && left_idle_bound)
            .then(|| WeaponAnimations::from_registry_edges(&registry, id, left_edges, xanims));
        let idle_name = registry
            .sz_xanim_edges_of(id)
            .and_then(|row| row[asset_iw4::size::weap_anim::IDLE].bound_index())
            .and_then(|order| xanims.clip_at(order))
            .map(|clip| clip.name.clone());
        let Some(namespace) =
            registry.component_namespace_of(id, asset_game::WeaponComponent::ViewModel)
        else {
            return;
        };
        let gun_name = fpv
            .get_at(gun_index.order())
            .map(|entry| entry.skel.name.clone())
            .unwrap_or_else(String::new);
        let camos = self.camo_swaps(fpv, id, gun_index);
        let right_orders: Vec<Option<usize>> = right.clip_orders().to_vec();
        let left_orders: Vec<Option<usize>> = left
            .as_ref()
            .map(|left| left.clip_orders().to_vec())
            .unwrap_or_else(Vec::new);
        debug_assert_eq!(right_orders.len(), WEAPON_ANIM_SLOTS);

        let source_edges = if create_lr {
            right_edges
        } else {
            registry.sz_xanim_edges_of(id)
        };
        let sources_match = source_edges
            .into_iter()
            .flatten()
            .chain(
                left.is_some()
                    .then_some(left_edges)
                    .flatten()
                    .into_iter()
                    .flatten(),
            )
            .filter_map(|edge| edge.bound_index())
            .all(|order| {
                let clip = xanims.clip_at(order);
                registry
                    .fpv_tracks_for_viewmodel(viewmodel.as_deref())
                    .is_some_and(|tracks| {
                        tracks.matches_clip(fpv.identity(), order, clip.as_deref())
                    })
            });
        let sides: [FpvWeaponSlot; 2] = [false, true].map(|axis| {
            if !sources_match {
                return FpvWeaponSlot::Refused(Cow::<'static, str>::Owned(format!(
                    "FPV dependency unresolved for weapon {weapon_id}: {role} `{name}`",
                    weapon_id = id,
                    role = "FPV track publication",
                    name = String::from("clip or mesh source differs from the prepared mapping")
                )));
            }
            let (Some((hands, hands_index)), Some(assemblies)) = (
                registry.fpv_hands_for(id, axis, viewmodel.as_deref()),
                registry.fpv_assemblies_for_viewmodel(id, parent, axis, viewmodel.as_deref()),
            ) else {
                return FpvWeaponSlot::Refused(Cow::<'static, str>::Owned(format!(
                    "FPV dependency unresolved for weapon {weapon_id}: {role} `{name}`",
                    weapon_id = id,
                    role = "FPV skeleton",
                    name = registry.fpv_assembly_gap_of(id, axis)
                )));
            };
            let bare = self.composition(&assemblies.bare);
            let rocket = assemblies
                .rocket
                .as_ref()
                .map(|rocket| self.composition(rocket));
            let (bare, rocket) = match (bare, rocket.transpose()) {
                (Ok(bare), Ok(rocket)) => (bare, rocket),
                (Err(error), _) | (_, Err(error)) => {
                    return FpvWeaponSlot::Refused(Cow::<'static, str>::Owned(format!(
                        "FPV dependency unresolved for weapon {weapon_id}: {role} `{name}`",
                        weapon_id = id,
                        role = "FPV layout",
                        name = error
                    )));
                }
            };
            let refusal: Option<FpvMandatoryRefusal> = bare
                .refusal()
                .or_else(|| rocket.as_ref().and_then(|rocket| rocket.refusal()))
                .cloned();
            if let Some(refusal) = refusal {
                return FpvWeaponSlot::Refused(Cow::<'static, str>::Owned(format!(
                    "FPV dependency unresolved for weapon {weapon_id}: {role} `{name}`",
                    weapon_id = id,
                    role = "FPV material",
                    name = refusal
                )));
            }
            let melee = match assemblies
                .melee
                .as_ref()
                .map(|assembly| self.composition(assembly))
                .transpose()
            {
                Ok(melee) => melee,
                Err(error) => {
                    return FpvWeaponSlot::Refused(Cow::<'static, str>::Owned(format!(
                        "FPV dependency unresolved for weapon {weapon_id}: {role} `{name}`",
                        weapon_id = id,
                        role = "melee FPV layout",
                        name = error
                    )));
                }
            };
            if let Some(refusal) = melee.as_ref().and_then(|composition| composition.refusal()) {
                return FpvWeaponSlot::Refused(Cow::<'static, str>::Owned(format!(
                    "FPV dependency unresolved for weapon {weapon_id}: {role} `{name}`",
                    weapon_id = id,
                    role = "melee FPV material",
                    name = refusal
                )));
            }
            let ads = match assemblies
                .ads
                .as_ref()
                .map(|assembly| self.composition(assembly))
                .transpose()
            {
                Ok(ads) => ads,
                Err(error) => {
                    return FpvWeaponSlot::Refused(Cow::<'static, str>::Owned(format!(
                        "FPV dependency unresolved for weapon {weapon_id}: {role} `{name}`",
                        weapon_id = id,
                        role = "ADS FPV layout",
                        name = error
                    )));
                }
            };
            if let Some(refusal) = ads.as_ref().and_then(|composition| composition.refusal()) {
                return FpvWeaponSlot::Refused(Cow::<'static, str>::Owned(format!(
                    "FPV dependency unresolved for weapon {weapon_id}: {role} `{name}`",
                    weapon_id = id,
                    role = "ADS FPV material",
                    name = refusal
                )));
            }
            let jammed = match assemblies
                .jammed
                .as_ref()
                .map(|assembly| self.composition(assembly))
                .transpose()
            {
                Ok(jammed) => jammed.filter(|composition| composition.refusal().is_none()),
                Err(_) => None,
            };
            let mut rigs = FpvRigSet::default();
            if let Some(jammed) = &jammed {
                rigs.jammed[0] =
                    Some(self.rig(jammed, false, &right_orders, &[], viewmodel.as_deref()));
                if left.is_some() {
                    rigs.jammed[1] = Some(self.rig(
                        jammed,
                        true,
                        &right_orders,
                        &left_orders,
                        viewmodel.as_deref(),
                    ));
                }
            }
            rigs.bare[0] = Some(self.rig(&bare, false, &right_orders, &[], viewmodel.as_deref()));
            if left.is_some() {
                rigs.bare[1] = Some(self.rig(
                    &bare,
                    true,
                    &right_orders,
                    &left_orders,
                    viewmodel.as_deref(),
                ));
            }
            if let Some(rocket) = &rocket {
                rigs.rocket[0] =
                    Some(self.rig(rocket, false, &right_orders, &[], viewmodel.as_deref()));
                if left.is_some() {
                    rigs.rocket[1] = Some(self.rig(
                        rocket,
                        true,
                        &right_orders,
                        &left_orders,
                        viewmodel.as_deref(),
                    ));
                }
            }
            if let Some(melee) = &melee {
                rigs.melee[0] =
                    Some(self.rig(melee, false, &right_orders, &[], viewmodel.as_deref()));
                if left.is_some() {
                    rigs.melee[1] = Some(self.rig(
                        melee,
                        true,
                        &right_orders,
                        &left_orders,
                        viewmodel.as_deref(),
                    ));
                }
            }
            if let Some(ads) = &ads {
                rigs.ads[0] = Some(self.rig(ads, false, &right_orders, &[], viewmodel.as_deref()));
                if left.is_some() {
                    rigs.ads[1] = Some(self.rig(
                        ads,
                        true,
                        &right_orders,
                        &left_orders,
                        viewmodel.as_deref(),
                    ));
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
                camos: camos.clone(),
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
        if let Some(name) = viewmodel {
            self.script_slots.insert((id, parent, name), sides);
        } else if parent == 0 {
            self.slots.push(sides);
        } else {
            self.alternate_slots.insert((id, parent), sides);
        }
        self.tick();
    }

    fn finish(self) -> (FpvPreparationOwner, FpvWeaponTable) {
        for line in &self.refused {
            diag::info!(Fpv, "fpv refused before use: {line}");
        }
        if let Some(stage) = self.progress {
            stage.done();
        }
        let table = FpvWeaponTable {
            owner: self.owner.clone(),
            facts: self.facts,
            hud_iris: self.hud_iris,
            melee: self.melee,
            guns: self.guns,
            slots: self.slots,
            alternate_slots: self.alternate_slots,
            script_slots: self.script_slots,
        };
        (self.owner, table)
    }
}

fn refused_table(
    owner: &FpvPreparationOwner,
    fpv: &FpvMeshCatalog,
    cause: Cow<'static, str>,
) -> FpvWeaponTable {
    let registry = &owner.weapons;
    let weapon_n = registry.len() as u32;
    let mut table = FpvWeaponTable {
        owner: owner.clone(),
        facts: Vec::with_capacity(weapon_n as usize + 1),
        hud_iris: Vec::with_capacity(weapon_n as usize + 1),
        melee: Vec::with_capacity(weapon_n as usize + 1),
        guns: Vec::with_capacity(weapon_n as usize + 1),
        slots: Vec::with_capacity(weapon_n as usize + 1),
        alternate_slots: HashMap::new(),
        script_slots: HashMap::new(),
    };
    for id in 0..=weapon_n {
        table.facts.push(
            registry
                .bind_published_row(id)
                .ok()
                .and_then(|weapon| weapon.fpv_facts()),
        );
        table.hud_iris.push(registry.overlay_is_hud_iris(id));
        table.melee.push(registry.melee_weapon_of(id));
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
        tess.catalog().parts().sorted_materials,
        RuntimeSortedMaterialTable::Ready { .. }
    );
    let owner = FpvPreparationOwner {
        materials: Arc::clone(tess.catalog()),
        images: Arc::clone(&tess.material_images),
        weapons: Arc::clone(weapons.registry()),
        meshes: Arc::clone(&fpv_meshes.0),
        clips: Arc::clone(&xanims.0),
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
                Cow::Borrowed("FPV catalog missing"),
                "the material generation has no sorted table",
            ))
        } else if lighting.is_none() {
            Some((
                Cow::Borrowed("FPV hidden: ModelLightingCache atlas missing"),
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
            .map(|process| {
                process
                    .progress
                    .begin(asset_transport::StageId::FirstPerson, None)
            });
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
