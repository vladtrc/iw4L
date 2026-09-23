use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::tasks::{TaskPool, TaskPoolBuilder};

use crate::{
    BodyMeshCatalog, ClipCollision, FpvMeshBuild, FpvMeshCatalog, FxCatalog, IntermissionView,
    LocalizeCatalog, MP_LOCALIZED_ZONES, MaterialCatalog, PreparedGaps, PreparedMap,
    ProjectileMeshBuild, WeaponBuild, WeaponRegistry, WorldDraw, WorldWeaponBuild,
    WorldWeaponCatalog, XAnimBuild, XAnimCatalog, find_common_mp_for_envelope,
    find_common_mp_for_zone, find_runtime_common_mp, find_runtime_zone, find_zone_file_version,
    find_zone_for_tree, games_root_from_env,
    lane::{LoadedWorld, lane},
    lane_capability::PreparedCapability,
    load_localize_catalog_in_lane,
    material_images::ImageDemandPlan,
    open_zone_shared, peek_zone_version,
    progress::{LoadProgress, StageHandle, StageId},
};
use asset_transport::load_jobs::{self, JobKind};

pub use asset_world::WorldDrawPolicy;

static PROCESS_CPUS: std::sync::OnceLock<Vec<usize>> = std::sync::OnceLock::new();
static NEXT_COMMON_PROFILE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn publish_process_cpus(cpus: Vec<usize>) {
    let _ = PROCESS_CPUS.set(cpus);
}

pub fn load_workers() -> usize {
    PROCESS_CPUS
        .get()
        .map(Vec::len)
        .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
        .map_or(1, |n| n.saturating_sub(2).max(1))
}

pub fn load_pool() -> &'static TaskPool {
    static POOL: std::sync::OnceLock<TaskPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        TaskPoolBuilder::new()
            .num_threads(load_workers())
            .thread_name("iw4l load".to_owned())
            .on_thread_spawn(|| {
                if let Some(cpus) = PROCESS_CPUS.get() {
                    set_thread_cpus(cpus);
                }
            })
            .build()
    })
}

#[cfg(target_os = "linux")]
fn set_thread_cpus(cpus: &[usize]) {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        for cpu in cpus {
            if *cpu >= libc::CPU_SETSIZE as usize {
                return;
            }
            libc::CPU_SET(*cpu, &mut set);
        }
        libc::sched_setaffinity(0, size_of::<libc::cpu_set_t>(), &raw const set);
    }
}

#[cfg(not(target_os = "linux"))]
fn set_thread_cpus(_cpus: &[usize]) {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommonKey {
    runtime: Option<PathBuf>,
    foreign: Option<PathBuf>,
}

impl CommonKey {
    fn for_match(zone_ff: Option<&Path>, runtime: Option<&Path>, report: &mut Vec<String>) -> Self {
        Self {
            runtime: runtime.map(Path::to_path_buf),
            foreign: resolve_foreign_material_donor(zone_ff, runtime, report),
        }
    }

    fn shell(games: &crate::GamesRoot, report: &mut Vec<String>) -> Self {
        let runtime = match find_common_mp_for_envelope(games, fastfile_iw4::ZONE_VERSION_PC) {
            Ok(found) => Some(found.path),
            Err(error) => {
                report.push(format!("CAC iw4: {error}"));
                None
            }
        };
        Self {
            runtime,
            foreign: None,
        }
    }
}

impl std::fmt::Display for CommonKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.runtime {
            Some(path) => write!(f, "{}", path.display())?,
            None => f.write_str("<no runtime common_mp>")?,
        }
        if let Some(foreign) = &self.foreign {
            write!(f, " + {}", foreign.display())?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct CommonCounts {
    startup_count: usize,
    t5_mat_count: usize,
    t5_reuse_mat: usize,
    t5_xanim_n: usize,
    foreign_count: usize,
    seed_mat: usize,
    seed_img: usize,
    common_reuse_mat: usize,
    common_reuse_img: usize,
}

#[derive(Clone)]
struct CommonProducts {
    material_seed: MaterialCatalog,
    shared_surfaces: asset_model::SharedXModelSurfaces,
    scene_models: crate::MapXModelSceneCatalog,
    light_defs: Vec<crate::CapturedLightDef>,
    pen_table: weapon_iw4::PenetrationDepthTable,
    pen_table_loaded: bool,
    lochit_table: Option<[f32; weapon_iw4::HITLOC_COUNT]>,
    tracers: crate::TracerCatalog,
    xmodel_walk: crate::PreparedXModelWalkCensus,
    s1_common_bytes: usize,
    teamsets: std::collections::HashMap<String, crate::MapTeamSettings>,
    film_visions:
        std::collections::BTreeMap<String, Result<crate::FilmVision, crate::FilmVisionParseError>>,
    weapons: WeaponBuild,
    fpv_meshes: FpvMeshBuild,
    world_weapons: WorldWeaponBuild,
    projectile_meshes: ProjectileMeshBuild,
    xanims: XAnimBuild,
    player_anim_sources: crate::PlayerAnimSources,
    fx: FxCatalog,
    fx_models: crate::FxModelCatalog,
    impact_fx: Option<crate::OwnedFxImpactTable>,
    t5_xanims: XAnimBuild,
    t5_fx: FxCatalog,
    iw5_materials: MaterialCatalog,
    strings: LocalizeCatalog,
    counts: CommonCounts,
    report: Vec<String>,
    localize_report: Vec<String>,
}

struct KeptImages {
    label: &'static str,
    namespace: &'static str,
    batch: crate::material_images::DecodedImageBatch,
    job: load_jobs::Job,
}

pub struct CommonSet {
    id: u64,
    key: CommonKey,
    products: CommonProducts,
    donor_images: Flight<Vec<KeptImages>>,
    fpv_plan: Option<ImageDemandPlan>,
    retained: std::sync::Mutex<crate::material_images::PayloadRetention>,
    cac_tables: Vec<(crate::AssetNamespace, crate::CapturedStringTable)>,
    prepared_ms: f32,
    ready_at: std::time::Instant,
}

impl CommonSet {
    async fn donor_images(&self) -> &[KeptImages] {
        self.donor_images.wait().await
    }

    fn donor_batches(&self) -> usize {
        self.donor_images.get().map_or(0, Vec::len)
    }

    fn retain(&self, batch: &crate::material_images::DecodedImageBatch) -> u64 {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .keep(batch)
    }

    fn retained_bytes(&self) -> u64 {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .bytes()
    }

    fn retained_payloads(&self) -> usize {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .payloads()
    }
}

struct Flight<T> {
    done: std::sync::OnceLock<T>,
    waiting: std::sync::Mutex<Vec<std::task::Waker>>,
}

impl<T> Flight<T> {
    fn new() -> Self {
        Self {
            done: std::sync::OnceLock::new(),
            waiting: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn get(&self) -> Option<&T> {
        self.done.get()
    }

    fn land(&self, value: T) {
        let _ = self.done.set(value);
        let waiting = std::mem::take(
            &mut *self
                .waiting
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()),
        );
        for waker in waiting {
            waker.wake();
        }
    }

    fn wait(&self) -> impl std::future::Future<Output = &T> {
        std::future::poll_fn(move |cx| {
            if let Some(value) = self.done.get() {
                return std::task::Poll::Ready(value);
            }
            let mut waiting = self
                .waiting
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            // Checked again under the lock `land` takes before it wakes, or a
            // value landing in between would leave this waiter asleep.
            if let Some(value) = self.done.get() {
                return std::task::Poll::Ready(value);
            }
            waiting.push(cx.waker().clone());
            std::task::Poll::Pending
        })
    }
}

struct CommonFlight {
    key: CommonKey,
    set: Flight<Option<Arc<CommonSet>>>,
}

static COMMON: std::sync::Mutex<Option<Arc<CommonFlight>>> = std::sync::Mutex::new(None);

struct FlightGuard(Option<Arc<CommonFlight>>);

impl Drop for FlightGuard {
    fn drop(&mut self) {
        let Some(flight) = self.0.take() else {
            return;
        };
        let mut slot = COMMON.lock().unwrap_or_else(|poison| poison.into_inner());
        if slot
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &flight))
        {
            *slot = None;
        }
        drop(slot);
        flight.set.land(None);
    }
}

async fn ensure_common(key: CommonKey) -> (Arc<CommonSet>, &'static str) {
    let (flight, reach) = {
        let mut slot = COMMON.lock().unwrap_or_else(|poison| poison.into_inner());
        match slot.as_ref() {
            Some(flight) if flight.key == key => {
                let reach = if flight.set.get().is_some() {
                    "reused"
                } else {
                    "joined in flight"
                };
                (Arc::clone(flight), reach)
            }
            _ => {
                let flight = Arc::new(CommonFlight {
                    key: key.clone(),
                    set: Flight::new(),
                });
                if let Some(previous) = slot.replace(Arc::clone(&flight)) {
                    diag::info!(World, "common set: {} replaced by {key}", previous.key);
                }
                let running = Arc::clone(&flight);
                load_pool()
                    .spawn(async move {
                        let mut guard = FlightGuard(Some(running));
                        let set = prepare_common(key).await;
                        if let Some(flight) = guard.0.take() {
                            flight.set.land(Some(set));
                        }
                    })
                    .detach();
                (flight, "prepared")
            }
        }
    };
    diag::info!(World, "common set: {reach} for {}", flight.key);
    match flight.set.wait().await {
        Some(set) => (Arc::clone(set), reach),
        None => panic!("common set for {} was not prepared", flight.key),
    }
}

pub struct ShellCommon {
    pub weapons: WeaponRegistry,
    pub tables: Vec<(crate::AssetNamespace, crate::CapturedStringTable)>,
    pub report: Vec<String>,
}

pub async fn load_shell_common(games: crate::GamesRoot) -> ShellCommon {
    let mut report = Vec::new();
    let key = CommonKey::shell(&games, &mut report);
    let (common, reach) = ensure_common(key).await;
    let weapons = common.products.weapons.clone().publish();
    report.push(format!(
        "CAC: {reach} common set {}; weapons={} (iw4={} iw5={} t5={}) tables={}",
        common.key,
        weapons.len(),
        weapons.namespace_count(crate::AssetNamespace::Iw4),
        weapons.namespace_count(crate::AssetNamespace::Iw5),
        weapons.namespace_count(crate::AssetNamespace::T5),
        common.cac_tables.len(),
    ));
    for (namespace, table) in &common.cac_tables {
        report.push(format!(
            "CAC {}: {} rows={}",
            namespace.as_str(),
            table.name,
            table.rows
        ));
    }
    ShellCommon {
        weapons,
        tables: common.cac_tables.clone(),
        report,
    }
}

async fn prepare_common(key: CommonKey) -> Arc<CommonSet> {
    let started = std::time::Instant::now();
    let progress = LoadProgress::default();
    let pool = load_pool();
    let anchor = key.runtime.clone();

    let startup_walk = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        pool.spawn(async move { walk_startup_material_zones(anchor.as_ref(), &progress).await })
    };

    let mut donor_report = Vec::new();
    let material_donor = key.foreign.clone();
    let weapon_donor = resolve_iw5_weapon_donor(anchor.as_deref(), &mut donor_report);
    let iw5_stats_walk = weapon_donor.clone().map(|donor| {
        let progress = progress.clone();
        pool.spawn(async move { walk_iw5_stats_tables(&donor, &progress) })
    });
    let shared_donor = match (&material_donor, &weapon_donor) {
        (Some(material), Some(weapons)) if material == weapons => Some(material.clone()),
        _ => None,
    };
    let (data_common_walk, mut iw5_weapon_walk) = match shared_donor {
        Some(donor) => {
            let progress = progress.clone();
            let job = load_jobs::open(JobKind::ImageDecode).namespace("iw5");
            (
                ForeignCommonWork::Shared(
                    pool.spawn(async move { walk_shared_iw5_common(&donor, &progress, job) }),
                ),
                None,
            )
        }
        None => {
            let material = material_donor.map(|donor| {
                let progress = progress.clone();
                let job = load_jobs::open(JobKind::ImageDecode).namespace("iw5");
                pool.spawn(async move { walk_foreign_material_common(&donor, &progress, job) })
            });
            let weapons = weapon_donor.map(|donor| {
                let progress = progress.clone();
                let job = load_jobs::open(JobKind::ImageDecode).namespace("iw5");
                pool.spawn(async move { walk_iw5_weapon_bundle(&donor, &progress, job) })
            });
            (ForeignCommonWork::Split(material), weapons)
        }
    };

    let common_open = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        pool.spawn(async move {
            let Some(path) = anchor else {
                progress.record_skipped_scoped(StageId::CommonAssets, "common_mp");
                return None;
            };
            let stage = progress.begin_scoped(StageId::CommonAssets, "common_mp", None);
            let opened = open_zone_shared(&path);
            finish_zone_open(stage, &opened);
            Some((path, opened))
        })
    };

    let t5_common_prep = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        pool.spawn(async move { t5_weapon_common_prep(anchor.as_deref(), &progress) })
    };
    let localize_walk = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        pool.spawn(async move {
            let mut report = Vec::new();
            let strings = match &anchor {
                Some(path) => {
                    let stage = progress.begin(StageId::Localization, None);
                    let catalog = load_localized_strings_beside(path, &mut report, &stage);
                    stage.done();
                    catalog
                }
                None => {
                    progress.record_skipped(StageId::Localization);
                    report.push(
                        "localize: no runtime common_mp — every on-screen string is a gap".into(),
                    );
                    LocalizeCatalog::default()
                }
            };
            (strings, report)
        })
    };

    let (material_seed, mut common_report, iw4_stats, startup_light_defs) = startup_walk.await;
    let startup_count = material_seed.materials.len();

    let t5_weapon_walk = {
        let progress = progress.clone();
        let job = load_jobs::open(JobKind::ImageDecode).namespace("t5");
        pool.spawn(async move {
            walk_t5_weapon_common(t5_common_prep.await, &progress, material_seed, job)
        })
    };

    let T5WeaponCommon {
        weapons: t5_weapons,
        fpv: t5_fpv,
        world_guns: t5_world_guns,
        mut material_seed,
        xanims: t5_xanims,
        fx: t5_fx,
        projectiles: t5_projectiles,
        teamsets: t5_teamsets,
        images: t5_images,
        stats_tables: t5_stats,
        report: t5_report,
    } = t5_weapon_walk.await;
    let t5_ids = t5_weapons.len();
    let t5_fpv_n = t5_fpv.len();
    let t5_xanim_n = t5_xanims.len();
    let t5_reuse_mat = material_seed.link_reused_materials;
    let t5_mat_count = material_seed
        .materials
        .len()
        .saturating_sub(startup_count)
        .saturating_add(t5_reuse_mat);
    common_report.extend(t5_report);
    let (foreign_materials, mut foreign_report, shared_bundle, foreign_images) =
        match data_common_walk {
            ForeignCommonWork::Shared(task) => {
                let (materials, bundle, images, report) = task.await;
                (materials, report, Some(bundle), images)
            }
            ForeignCommonWork::Split(Some(task)) => {
                let (materials, images, report) = task.await;
                (materials, report, None, images)
            }
            ForeignCommonWork::Split(None) => (MaterialCatalog::default(), Vec::new(), None, None),
        };

    common_report.append(&mut donor_report);
    common_report.append(&mut foreign_report);

    let foreign_count = foreign_materials.materials.len();
    material_seed.absorb_asset_population(foreign_materials);

    let mut shared_surfaces = asset_model::SharedXModelSurfaces::default();
    let mut common_scene_models = crate::MapXModelSceneCatalog::default();
    let mut common_light_defs = startup_light_defs;
    let mut common_pen_table = weapon_iw4::PenetrationDepthTable::empty();
    let mut common_pen_loaded = false;
    let mut common_lochit_table = None;

    let mut common_tracers = crate::TracerCatalog::default();
    let mut xmodel_walk = crate::PreparedXModelWalkCensus::default();
    let mut s1_common_bytes = 0;
    let mut teamsets = t5_teamsets;
    let mut common_film_visions = std::collections::BTreeMap::new();
    let mut fpv_plan = None;
    let mut iw4_census_stats = Vec::new();

    let common_opened = common_open.await;

    let (
        mut weapons,
        mut fpv_meshes,
        mut world_weapons,
        mut projectile_meshes,
        mut xanims,
        mut player_anim_sources,
        common_fx,
        common_fx_models,
        common_impact,
        material_seed,
        mut common_walk_report,
    ) = match common_opened {
        Some((path, Ok(image))) => {
            let mut census =
                lane(image.game).load_common_mp(&path, &image, &progress, true, material_seed);
            fpv_plan = census.pending_images.take().filter(|plan| !plan.is_empty());
            shared_surfaces = census.shared_surfaces;
            common_scene_models = census.scene_models;
            common_light_defs.extend(census.light_defs);
            common_pen_table = census.pen_table;
            common_pen_loaded = census.pen_table_loaded;
            common_lochit_table = census.lochit_table;
            common_tracers = census.tracers;
            xmodel_walk = census.xmodel_walk;
            s1_common_bytes = census.s1_common_bytes;
            teamsets.extend(census.teamsets);
            common_film_visions = census.film_visions;
            iw4_census_stats = census.cac_tables;
            (
                census.weapons,
                census.fpv,
                census.world_weapons,
                census.projectile_meshes,
                census.xanims,
                census.player_anim_sources,
                census.fx,
                census.fx_models,
                census.impact_fx,
                census.material_population,
                census.report,
            )
        }
        Some((_, Err(error))) => (
            crate::WeaponBuild::default(),
            FpvMeshBuild::default(),
            WorldWeaponBuild::default(),
            ProjectileMeshBuild::default(),
            XAnimBuild::default(),
            crate::PlayerAnimSources::default(),
            crate::FxCatalog::default(),
            crate::FxModelCatalog::default(),
            None,
            material_seed,
            vec![format!("common_mp models: open zone: {error}")],
        ),
        None => (
            crate::WeaponBuild::default(),
            FpvMeshBuild::default(),
            WorldWeaponBuild::default(),
            ProjectileMeshBuild::default(),
            XAnimBuild::default(),
            crate::PlayerAnimSources::default(),
            crate::FxCatalog::default(),
            crate::FxModelCatalog::default(),
            None,
            material_seed,
            Vec::new(),
        ),
    };
    common_report.append(&mut common_walk_report);
    weapons.apply_stats_tables(&iw4_stats);

    let common_reuse_mat = material_seed.link_reused_materials;
    let common_reuse_img = material_seed.link_reused_images;
    let seed_mat = material_seed.materials.len();
    let seed_img = material_seed.images.len();

    player_anim_sources.compile();
    common_report.push(player_anim_sources.compile_report_line());
    common_report.push(player_anim_sources.parse_report_line());

    let (bundle, bundle_images, mut iw5_report) = match (iw5_weapon_walk.take(), shared_bundle) {
        (Some(task), _) => task.await,
        (None, Some(bundle)) => (bundle, None, Vec::new()),
        (None, None) => (Iw5WeaponBundle::default(), None, Vec::new()),
    };
    let Iw5WeaponBundle {
        weapons: mut iw5_weapons,
        fpv: iw5_fpv,
        world_guns: iw5_world_guns,
        xanims: iw5_xanims,
        materials: iw5_materials,
        stats_tables: iw5_census_stats,
    } = bundle;
    let iw5_stats = match iw5_stats_walk {
        Some(task) => {
            let (tables, report) = task.await;
            common_report.extend(report);
            tables
        }
        None => Vec::new(),
    };
    iw5_weapons.apply_stats_tables(&iw5_stats);
    let iw4_weapon_n = weapons.len();
    let fpv_common_n = fpv_meshes.len();
    let absorbed = iw5_weapons.len();
    weapons.absorb(iw5_weapons);
    let iw5_fpv_added = fpv_meshes.absorb(iw5_fpv);
    let world_gun_common_n = world_weapons.len();
    let iw5_world_added = world_weapons.absorb(iw5_world_guns);
    let xanim_before_iw5 = xanims.len();
    let iw5_xanim_added = xanims.absorb(iw5_xanims);
    let iw5_mat_n = iw5_materials.materials.len();
    common_report.append(&mut iw5_report);
    common_report.push(format!(
        "iw5 weapon bundle: donor={} absorbed={absorbed} catalog {iw4_weapon_n}→{} fpv {fpv_common_n}→{} (+{iw5_fpv_added} iw5 keys) world guns {world_gun_common_n}→{} (+{iw5_world_added} iw5 keys) xanims {xanim_before_iw5}→{} (+{iw5_xanim_added} iw5 keys) materials={iw5_mat_n} (absorbed after IW4 pool)",
        absorbed,
        weapons.len(),
        fpv_meshes.len(),
        world_weapons.len(),
        xanims.len(),
    ));
    match weapons.resolve_index("iw5_msr") {
        Ok(Some(id)) => {
            let facts = weapons.facts_of(id);
            common_report.push(format!(
                "iw5_msr: id={id} gun={} world={} fire={:?} clip={:?} class={:?} type={:?} ftype={:?} bolt={:?} raise={:?} start={:?} dmg={:?} overlay={:?} overlay_img={:?} ov_w={:?} ov_h={:?}",
                weapons.gun_xmodel_of(id).unwrap_or("<none>"),
                weapons.world_model_of(id).unwrap_or("<none>"),
                facts.map(|f| f.fire_time_ms),
                facts.map(|f| f.clip_size),
                facts.map(|f| f.weap_class),
                facts.map(|f| f.weap_type),
                facts.map(|f| f.fire_type),
                facts.map(|f| f.bolt_action),
                facts.map(|f| f.raise_time_ms),
                facts.map(|f| f.start_ammo),
                facts.map(|f| f.damage),
                weapons.overlay_material_of(id),
                weapons.overlay_image_of(id),
                facts.map(|f| f.ads_overlay_width),
                facts.map(|f| f.ads_overlay_height),
            ));
        }
        Ok(None) | Err(_) => common_report.push("iw5_msr: not in merged catalog".into()),
    }

    let mut t5_weapons = t5_weapons;
    let (t5_code_stats, t5_census_stats) = t5_stats;
    t5_weapons.apply_stats_tables(&t5_code_stats);
    let t5_absorbed = t5_weapons.len();
    weapons.absorb(t5_weapons);
    projectile_meshes.absorb(t5_projectiles);
    let t5_fpv_added = fpv_meshes.absorb(t5_fpv);
    let t5_world_added = world_weapons.absorb(t5_world_guns);
    common_report.push(format!(
        "t5 weapon absorb: donor={t5_ids} unique={t5_absorbed} fpv=+{t5_fpv_added}/{t5_fpv_n} world_guns=+{t5_world_added}; registry now {} (t5={})",
        weapons.len(),
        weapons.namespace_count(crate::AssetNamespace::T5)
    ));
    common_report.push(format!(
        "FPV generation: common={fpv_common_n} iw5_keys={iw5_fpv_added} t5={t5_fpv_n} t5_keys={t5_fpv_added} collide={} merged={}",
        fpv_meshes.collide_name_count(),
        fpv_meshes.len()
    ));

    let cac_tables: Vec<(crate::AssetNamespace, crate::CapturedStringTable)> = [
        (crate::AssetNamespace::Iw4, iw4_stats, iw4_census_stats),
        (crate::AssetNamespace::Iw5, iw5_stats, iw5_census_stats),
        (crate::AssetNamespace::T5, t5_code_stats, t5_census_stats),
    ]
    .into_iter()
    .flat_map(|(namespace, code, common)| {
        code.into_iter()
            .chain(common)
            .map(move |table| (namespace, table))
    })
    .collect();

    weapons.set_family_tables(cac_tables.clone());
    let iw5_prepared = weapons.prepare_iw5_configurations();
    weapons.resolve_fpv_mesh_edges(&fpv_meshes);
    common_report.push(format!(
        "IW5 configurations: prepared={} refused={} {:?}",
        iw5_prepared.prepared,
        iw5_prepared.refused.len(),
        iw5_prepared
            .refused
            .iter()
            .map(|selection| format!(
                "{} [{}]",
                selection
                    .family
                    .as_ref()
                    .map(|key| key.to_string())
                    .unwrap_or_default(),
                selection.attachments.join(" ")
            ))
            .collect::<Vec<_>>()
    ));
    let (strings, localize_report) = localize_walk.await;
    common_report.extend(progress.timing_report());
    let prepared_ms = started.elapsed().as_secs_f32() * 1000.0;
    diag::info!(
        World,
        "common set: {key} prepared in {prepared_ms:.0}ms; donor images still decoding"
    );

    let mut material_seed = material_seed;
    material_seed.mark_images_common_owned();
    let mut iw5_materials = iw5_materials;
    iw5_materials.mark_images_common_owned();
    let set = Arc::new(CommonSet {
        id: NEXT_COMMON_PROFILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        key,
        products: CommonProducts {
            material_seed,
            shared_surfaces,
            scene_models: common_scene_models,
            light_defs: common_light_defs,
            pen_table: common_pen_table,
            pen_table_loaded: common_pen_loaded,
            lochit_table: common_lochit_table,
            tracers: common_tracers,
            xmodel_walk,
            s1_common_bytes,
            teamsets,
            film_visions: common_film_visions,
            weapons,
            fpv_meshes,
            world_weapons,
            projectile_meshes,
            xanims,
            player_anim_sources,
            fx: common_fx,
            fx_models: common_fx_models,
            impact_fx: common_impact,
            t5_xanims,
            t5_fx,
            iw5_materials,
            strings,
            counts: CommonCounts {
                startup_count,
                t5_mat_count,
                t5_reuse_mat,
                t5_xanim_n,
                foreign_count,
                seed_mat,
                seed_img,
                common_reuse_mat,
                common_reuse_img,
            },
            report: common_report,
            localize_report,
        },
        donor_images: Flight::new(),
        fpv_plan,
        retained: std::sync::Mutex::new(Default::default()),
        cac_tables,
        prepared_ms,
        ready_at: std::time::Instant::now(),
    });

    let keeping = Arc::clone(&set);
    load_pool()
        .spawn(async move {
            let mut kept = Vec::new();
            for (namespace, pending) in [
                ("t5", t5_images),
                ("iw5", foreign_images),
                ("iw5", bundle_images),
            ] {
                let Some(pending) = pending else {
                    continue;
                };
                let job = pending.job;
                let (label, batch) = pending.join().await;
                let batch = batch.into_kept();
                keeping.retain(&batch);
                kept.push(KeptImages {
                    label,
                    namespace,
                    batch,
                    job,
                });
            }
            diag::info!(
                World,
                "common set: {} donor image batches kept, {} payloads ({:.1}MiB)",
                kept.len(),
                keeping.retained_payloads(),
                keeping.retained_bytes() as f64 / (1024.0 * 1024.0),
            );
            keeping.donor_images.land(kept);
        })
        .detach();
    set
}

fn walk_iw5_stats_tables(
    donor: &Path,
    progress: &LoadProgress,
) -> (Vec<crate::CapturedStringTable>, Vec<String>) {
    let found = match find_zone_for_tree(donor, "code_post_gfx_mp") {
        Ok(found) => found,
        Err(error) => {
            return (
                Vec::new(),
                vec![format!("CAC iw5 code_post_gfx_mp: {error}")],
            );
        }
    };
    let population = walk_population_file(&found.path, progress, MaterialCatalog::default());
    let line = format!(
        "CAC iw5: {} tables={}",
        found.path.display(),
        population.cac_tables.len()
    );
    (population.cac_tables, vec![line])
}

#[derive(Default, Clone)]
pub struct PreparedWorld {
    pub draw: Option<WorldDraw>,
    pub dynamic_light: Option<crate::ResolvedLightDef>,
    pub static_model_meshes: Vec<crate::ModelMesh>,

    pub static_model_instances: Vec<Option<crate::StaticModelPlacement>>,

    pub map_xmodel_scene_assets: crate::MapXModelSceneCatalog,

    pub script_model_instances: Vec<crate::ScriptModelSceneInstance>,

    pub script_brush_models: Vec<crate::ScriptBrushModelPlacement>,

    pub map_use_triggers: Vec<crate::MapUseTrigger>,

    pub flag_descriptors: Vec<crate::FlagDescriptor>,

    pub script_structs: Vec<crate::MapScriptStruct>,

    pub dyn_ents: crate::DynEntCatalog,

    pub smodel_lighting_samples: Vec<crate::SmodelLightingSample>,

    pub light_grid: Option<crate::OwnedLightGrid>,

    pub fx: crate::FxCatalog,

    pub fx_models: crate::FxModelCatalog,

    pub fx_glass: Option<crate::FxGlassReset>,

    pub impact_fx: Option<crate::OwnedFxImpactTable>,
    pub reflection_probe_images: Vec<Option<bevy::prelude::Image>>,
    pub intermission_view: Option<IntermissionView>,

    pub exp_fog: Option<crate::ExpFog>,

    pub film_vision: Option<crate::FilmVision>,
    pub film_visions:
        std::collections::BTreeMap<String, Result<crate::FilmVision, crate::FilmVisionParseError>>,

    pub createart_name: Option<String>,
    pub min: [f32; 3],
    pub max: [f32; 3],

    pub world_bounds: Option<[f32; 6]>,
    pub policy: WorldDrawPolicy,
}

#[derive(Default, Clone)]
pub struct PreparedMatch {
    pub world: PreparedWorld,

    pub fx: crate::FxDefinitions,
    pub materials: crate::MatchMaterials,
    pub clip: Option<ClipCollision>,
    pub weapons: WeaponRegistry,
    pub fpv_meshes: FpvMeshCatalog,
    pub bodies: BodyMeshCatalog,
    pub world_weapons: WorldWeaponCatalog,

    pub projectile_meshes: crate::ProjectileMeshCatalog,
    pub xanims: XAnimCatalog,
    pub destructible_death: Vec<crate::DestructibleDeathRow>,
    pub player_anim_sources: crate::PlayerAnimSources,

    pub tracers: crate::TracerDefinitions,

    pub strings: LocalizeCatalog,
    pub report: Vec<String>,

    pub prepared_map: PreparedMap,

    pub pen_table: weapon_iw4::PenetrationDepthTable,
    pub pen_table_loaded: bool,
    pub lochit_table: Option<[f32; weapon_iw4::HITLOC_COUNT]>,

    pub xmodel_walk: crate::PreparedXModelWalkCensus,

    pub sound: Option<Result<asset_audio::SoundCatalog, String>>,
}

pub enum MatchLoadOutcome {
    Ready(PreparedMatch),
    Canceled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ZoneStamp {
    path: PathBuf,
    len: u64,
    modified: Option<std::time::SystemTime>,
}

impl ZoneStamp {
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            path: path.to_path_buf(),
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

struct ResidentMap {
    zone: ZoneStamp,
    common: Arc<CommonSet>,
    prepared: PreparedMatch,
}

static RESIDENT_MAP: std::sync::Mutex<Option<ResidentMap>> = std::sync::Mutex::new(None);

static NEXT_PRODUCTS_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn landed_common(key: &CommonKey) -> Option<Arc<CommonSet>> {
    let slot = COMMON.lock().unwrap_or_else(|poison| poison.into_inner());
    let flight = slot.as_ref().filter(|flight| flight.key == *key)?;
    flight.set.get().cloned().flatten()
}

fn resident_copy(zone: &ZoneStamp, key: &CommonKey) -> Option<PreparedMatch> {
    let common = landed_common(key)?;
    let slot = RESIDENT_MAP
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let resident = slot.as_ref()?;
    (resident.zone == *zone && Arc::ptr_eq(&resident.common, &common))
        .then(|| resident.prepared.clone())
}

pub async fn load_prepared_match(
    zone_ff: Result<PathBuf, String>,
    common_mp: Result<PathBuf, String>,
    progress: LoadProgress,
) -> MatchLoadOutcome {
    let stamp = zone_ff.as_deref().ok().and_then(ZoneStamp::of);
    if let Some(stamp) = &stamp {
        let key = CommonKey::for_match(
            Some(&stamp.path),
            common_mp.as_deref().ok(),
            &mut Vec::new(),
        );
        let copying = std::time::Instant::now();
        if let Some(mut prepared) = resident_copy(stamp, &key) {
            progress.record_reused_scoped(StageId::CommonAssets, "shared common");
            progress.record_reused_scoped(StageId::MapAssets, "resident");
            progress.record_reused_scoped(StageId::Images, "map");
            let line = format!(
                "resident map: reused `{}` walk #{} — no zone opened, no decode; match copy {:.0}ms",
                stamp.path.display(),
                prepared.materials.products_id,
                copying.elapsed().as_secs_f32() * 1000.0
            );
            diag::info!(World, "{line}");
            prepared.report.push(line);
            prepared.report.extend(progress.timing_report());
            return MatchLoadOutcome::Ready(prepared);
        }
    }
    let dropped = RESIDENT_MAP
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .take();
    if let Some(dropped) = dropped {
        diag::info!(
            World,
            "resident map: `{}` walk #{} dropped before the next walk",
            dropped.zone.path.display(),
            dropped.prepared.materials.products_id
        );
    }
    let (outcome, common) = walk_prepared_match(zone_ff, common_mp, progress.clone()).await;
    let MatchLoadOutcome::Ready(mut prepared) = outcome else {
        return outcome;
    };
    if let (Some(zone), Some(common)) = (stamp, common) {
        let keeping = std::time::Instant::now();
        let resident = prepared.clone();
        prepared.report.push(format!(
            "resident map: kept `{}` walk #{} for a same-map load ({:.0}ms)",
            zone.path.display(),
            prepared.materials.products_id,
            keeping.elapsed().as_secs_f32() * 1000.0
        ));
        *RESIDENT_MAP
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(ResidentMap {
            zone,
            common,
            prepared: resident,
        });
    }
    prepared.report.extend(progress.timing_report());
    MatchLoadOutcome::Ready(prepared)
}

async fn walk_prepared_match(
    zone_ff: Result<PathBuf, String>,
    common_mp: Result<PathBuf, String>,
    progress: LoadProgress,
) -> (MatchLoadOutcome, Option<Arc<CommonSet>>) {
    let zone_name = zone_ff
        .as_ref()
        .ok()
        .and_then(|path| path.file_stem())
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();

    let pool = load_pool();
    let map_open = {
        let zone_ff = zone_ff.clone();
        let progress = progress.clone();
        pool.spawn(async move {
            let path = match zone_ff {
                Ok(path) => path,
                Err(error) => {
                    progress.record_skipped_scoped(StageId::MapAssets, "open");
                    return Err(format!("zone not found: {error}"));
                }
            };
            let stage = progress.begin_scoped(StageId::MapAssets, "open", None);
            progress.begin_zone_open();
            let opened = open_zone_shared(&path);
            finish_zone_open(stage, &opened);
            let image = opened.map_err(|error| format!("open zone: {error}"))?;
            progress.record_zone_image_bytes(image.bytes.len());
            Ok::<_, String>((path, image))
        })
    };

    let mut donor_report = Vec::new();
    let key = CommonKey::for_match(
        zone_ff.as_ref().ok().map(PathBuf::as_path),
        common_mp.as_ref().ok().map(PathBuf::as_path),
        &mut donor_report,
    );
    let waiting = progress.begin_scoped(StageId::CommonAssets, "shared common", None);
    let (common, reach) = ensure_common(key).await;
    waiting.done();
    if progress.is_canceled() {
        diag::info!(
            World,
            "match walk: canceled while the common set was prepared — load retargeted"
        );
        return (MatchLoadOutcome::Canceled, None);
    }

    let cloning = std::time::Instant::now();
    let CommonProducts {
        material_seed,
        shared_surfaces,
        scene_models: common_scene_models,
        light_defs: common_light_defs,
        pen_table: common_pen_table,
        pen_table_loaded: common_pen_loaded,
        lochit_table: common_lochit_table,
        tracers: mut common_tracers,
        xmodel_walk,
        s1_common_bytes,
        teamsets,
        film_visions: mut common_film_visions,
        mut weapons,
        mut fpv_meshes,
        mut world_weapons,
        mut projectile_meshes,
        mut xanims,
        mut player_anim_sources,
        fx: common_fx,
        fx_models: common_fx_models,
        impact_fx: common_impact,
        t5_xanims,
        t5_fx,
        iw5_materials,
        strings,
        counts:
            CommonCounts {
                startup_count,
                t5_mat_count,
                t5_reuse_mat,
                t5_xanim_n,
                foreign_count,
                seed_mat,
                seed_img,
                common_reuse_mat,
                common_reuse_img,
            },
        report: mut common_report,
        localize_report,
    } = common.products.clone();
    let clone_ms = cloning.elapsed().as_secs_f32() * 1000.0;
    let iw5_mat_n = iw5_materials.materials.len();
    common_report.append(&mut donor_report);
    common_report.push(format!(
        "common set: {reach} for {} (prepared in {:.0}ms, {:.1}s ago); match copy {clone_ms:.0}ms; {} donor image batches and {} kept payloads ({:.1}MiB) shared, not decoded again",
        common.key,
        common.prepared_ms,
        common.ready_at.elapsed().as_secs_f32(),
        common.donor_batches(),
        common.retained_payloads(),
        common.retained_bytes() as f64 / (1024.0 * 1024.0),
    ));
    let common_images = hold_image_plan(
        "common_mp FPV",
        common.fpv_plan.clone(),
        load_jobs::open(JobKind::ImageDecode).namespace("iw4"),
    );

    let opened_map = map_open.await;
    if progress.is_canceled() {
        drop(opened_map);
        diag::info!(
            World,
            "match walk: canceled before the map walk — load retargeted"
        );
        return (MatchLoadOutcome::Canceled, None);
    }
    let (loaded, map_namespace) = match opened_map {
        Ok((path, image)) => {
            let game = image.game;
            (
                lane(game).load_world(
                    &path,
                    &image,
                    &progress,
                    shared_surfaces,
                    material_seed,
                    &mut common_film_visions,
                ),
                Some(crate::AssetNamespace::from_zone_game(game)),
            )
        }
        Err(gap) => {
            drop(material_seed);
            (
                LoadedWorld::with_gap(
                    WorldDrawPolicy::default(),
                    PreparedCapability::PreparedWorld,
                    gap,
                    Some("assets::session_load::load_prepared_match/zone_open"),
                ),
                None,
            )
        }
    };

    let LoadedWorld {
        mut world,
        mut materials,
        collision: clip,
        spawns: dm_spawns,
        mut bodies,
        fpv_meshes: map_fpv,
        xanims: map_xanims,
        mut facts,
        arena_bytes: s1_map_bytes,
        sound,
        mut report,
        gaps,
    } = loaded;
    world
        .map_xmodel_scene_assets
        .absorb_captured(common_scene_models);
    report.append(&mut common_report);

    if facts.team_settings.allies.is_none() && facts.team_settings.axis.is_none() {
        if let Some(name) = facts.t5_teamset.as_ref() {
            if let Some(icons) = teamsets.get(name) {
                facts.team_settings = icons.clone();
            }
        }
    }
    if facts.t5_teamset.is_some() {
        facts.script_sound.attackers = facts
            .script_sound
            .attackers
            .or_else(|| facts.team_settings.attackers.clone());
        facts.script_sound.defenders = facts
            .script_sound
            .defenders
            .or_else(|| facts.team_settings.defenders.clone());
    }
    match (
        facts.t5_teamset.as_deref(),
        facts.team_settings.allies.as_ref(),
        facts.team_settings.axis.as_ref(),
    ) {
        (Some(ts), Some(a), Some(x)) => {
            report.push(format!("team icons: teamset={ts} allies={a} axis={x}"));
        }
        (Some(ts), ..) => {
            report.push(format!(
                "team icons gap: teamset={ts} but common_mp had no matching _teamset_*.gsc icons"
            ));
        }
        _ => {}
    }

    report.push(format!(
        "map teams: allies={:?} axis={:?} attackers={:?} defenders={:?}",
        facts.team_settings.allies_name,
        facts.team_settings.axis_name,
        facts.script_sound.attackers,
        facts.script_sound.defenders,
    ));

    report.push(format!(
        "s1 pool walked: common={s1_common_bytes} map={s1_map_bytes} total={} rss={}",
        s1_common_bytes.saturating_add(s1_map_bytes),
        crate::process_resident_bytes().unwrap_or(0),
    ));

    let common_xanim_count = xanims.len();
    let map_xanim_count = map_xanims.len();
    let t5_xanim_added = xanims.absorb(t5_xanims);
    xanims.absorb_local(map_xanims);
    weapons.resolve_sz_xanim_edges(&xanims);
    let weapon_clip_indices = weapons.bound_weapon_xanim_indices();
    let clip_prewarm_started = std::time::Instant::now();
    let failed_weapon_clips: Vec<_> = weapon_clip_indices
        .iter()
        .copied()
        .filter(|&index| xanims.clip_at(index).is_none())
        .collect();
    report.push(format!(
        "weapon XAnim CPU prewarm: linked={} decoded={} failed={} elapsed_ms={:.1}",
        weapon_clip_indices.len(),
        weapon_clip_indices.len() - failed_weapon_clips.len(),
        failed_weapon_clips.len(),
        clip_prewarm_started.elapsed().as_secs_f64() * 1000.0,
    ));
    for index in failed_weapon_clips.iter().take(16) {
        report.push(format!(
            "weapon XAnim decode gap: index={index} name={}",
            xanims.name_at(*index).unwrap_or("<unknown>")
        ));
    }
    let (note_actions, inline_note_actions) = weapons.resolve_notetrack_actions(&xanims);
    report.push(format!(
        "weapon notetrack actions linked: {note_actions} ({inline_note_actions} T5 inline)"
    ));
    let sz_xanims = weapons.sz_xanim_edge_census();
    report.push(format!(
        "XAnim generation: common={common_xanim_count} t5={t5_xanim_n} t5_keys={t5_xanim_added} collide={} map={map_xanim_count} merged={}",
        xanims.collide_name_count(),
        xanims.len()
    ));
    report.push(format!(
        "weapon szXAnims after absorb: bound={} unresolved={} absent={}",
        sz_xanims.bound, sz_xanims.unresolved, sz_xanims.absent,
    ));
    player_anim_sources.bind_leaves(&xanims);
    report.push(player_anim_sources.bind_report_line());
    let gap_lines: Vec<String> = gaps
        .iter()
        .map(|gap| match gap.addr {
            Some(addr) => format!("lane gap [{addr}]: {}", gap.reason),
            None => format!("lane gap: {}", gap.reason),
        })
        .collect();
    report.extend(gap_lines.iter().cloned());
    report.extend(bodies.report_lines());
    let map_fpv_n = map_fpv.len();
    let map_fpv_added = fpv_meshes.absorb(map_fpv);
    fpv_meshes.set_map_namespace(map_namespace);
    weapons.resolve_fpv_mesh_edges(&fpv_meshes);
    weapons.resolve_fpv_hands(&fpv_meshes, &bodies);
    let assembly_started = std::time::Instant::now();
    let assemblies = weapons.resolve_fpv_assemblies(&fpv_meshes, &xanims);
    report.push(format!(
        "FPV assemblies: built={} kit sides linked={} refused={} clip track tables={} elapsed_ms={:.1}",
        assemblies.built,
        assemblies.linked,
        assemblies.refused,
        assemblies.clip_tables,
        assembly_started.elapsed().as_secs_f64() * 1000.0,
    ));

    world_weapons.seal_identity();
    weapons.resolve_world_model_edges(&world_weapons);
    let world_model_edges = weapons.world_model_edge_census();
    report.push(format!(
        "world gun generation: catalog={}; worldModel edges bound={} unresolved={} absent={}",
        world_weapons.len(),
        world_model_edges.bound,
        world_model_edges.unresolved,
        world_model_edges.absent,
    ));
    let dependency_gaps = weapons.dependency_gaps();
    let selectable_gap_ids: std::collections::BTreeSet<_> = dependency_gaps
        .iter()
        .map(|gap| gap.id)
        .filter(|&id| weapons.describe_configuration(id).is_some())
        .collect();
    report.push(format!(
        "weapon dependency audit: {} gaps in {} definitions; selectable={}",
        dependency_gaps.len(),
        dependency_gaps
            .iter()
            .map(|gap| gap.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        selectable_gap_ids.len(),
    ));
    for gap in &dependency_gaps {
        report.push(format!(
            "weapon dependency gap: {} {} `{}`",
            weapons.name_of(gap.id),
            gap.kind,
            gap.name
        ));
    }
    report.push(format!(
        "FPV map fanout: map={map_fpv_n} added={map_fpv_added} merged={} map_ns={map_namespace:?}",
        fpv_meshes.len()
    ));

    let before_unrouted = materials.unrouted_material_count();
    let promoted = materials.promote_iw5_fallback_tables();
    let t5_alias_map = materials.absorb_t5_feature_token_donors();
    let stub_routed = materials.reroute_stub_materials();
    let after_unrouted = materials.unrouted_material_count();
    let takes_ml = materials
        .materials
        .iter()
        .filter(|m| materials.takes_model_lighting(m) == Some(true))
        .count();
    let t5_fb = materials
        .technique_set_facts()
        .iter()
        .filter(|facts| facts.t5_fallback_table.is_some())
        .count();
    report.push(format!(
        "material route: iw5_promoted={promoted} \
             t5_tech_alias={t5_alias_map} t5_fallback={t5_fb} stub_routed={stub_routed} \
             unrouted {before_unrouted}→{after_unrouted}; takes_model_lighting={takes_ml}"
    ));

    let map_reuse_mat = materials.link_reused_materials;
    let map_reuse_img = materials.link_reused_images;
    let pool_mat = materials.materials.len();
    let pool_img = materials.images.len();
    report.push(format!(
            "s2 pool: seed_mat={seed_mat} seed_img={seed_img} common_reuse_mat={common_reuse_mat} common_reuse_img={common_reuse_img} map_reuse_mat={map_reuse_mat} map_reuse_img={map_reuse_img} pool_mat={pool_mat} pool_img={pool_img}"
        ));
    let map_new = pool_mat.saturating_sub(seed_mat);

    let mut global = materials;
    let provisional_map_ids: Vec<usize> = (0..global.materials.len()).collect();

    let iw5_linked = global.absorb_asset_population_host_materials_win(iw5_materials);
    report.push(format!(
        "iw5 leftover materials: donor={iw5_mat_n} linked={} pool={}",
        iw5_linked.len(),
        global.materials.len(),
    ));
    let promoted = global.promote_iw5_fallback_tables();
    let t5_alias = global.absorb_t5_feature_token_donors();
    global.reroute_stub_materials();
    let mat_refs = global.material_ref_census();
    let img_refs = global.image_ref_census();
    let shader_refs = global.shader_ref_census();
    let decl_refs = global.vertex_decl_ref_census();
    report.push(format!(
            "asset_ref before finalize: materials n={} real={} reference={}; images n={} real={} reference={}; shaders n={} real={} reference={}; decls n={} real={} reference={}",
            mat_refs.n, mat_refs.real, mat_refs.reference,
            img_refs.n, img_refs.real, img_refs.reference,
            shader_refs.n, shader_refs.real, shader_refs.reference,
            decl_refs.n, decl_refs.real, decl_refs.reference,
        ));
    let (mut global, finalized_ids) = global.publish();
    let map_ids = provisional_map_ids
        .into_iter()
        .map(|id| finalized_ids.get(id).copied().flatten())
        .collect::<Vec<_>>();
    let t5_fb = global
        .technique_set_facts()
        .iter()
        .filter(|facts| facts.t5_fallback_table.is_some())
        .count();
    report.push(format!(
            "material generation: startup={startup_count} t5={t5_mat_count} t5_reuse={t5_reuse_mat} foreign={foreign_count} common_pool={seed_mat} map_new={map_new} pooled={pool_mat} global={} common_reuse={common_reuse_mat} map_reuse={map_reuse_mat} unresolved_aliases={} iw5_promoted={promoted} t5_tech_alias={t5_alias} t5_fallback={t5_fb}",
            global.materials.len(),
            finalized_ids.iter().filter(|id| id.is_none()).count(),
        ));
    let mat_refs = global.material_ref_census();
    let img_refs = global.image_ref_census();
    let shader_refs = global.shader_ref_census();
    let decl_refs = global.vertex_decl_ref_census();
    report.push(format!(
            "asset_ref after finalize: materials n={} real={} reference={}; images n={} real={} reference={}; shaders n={} real={} reference={}; decls n={} real={} reference={}",
            mat_refs.n, mat_refs.real, mat_refs.reference,
            img_refs.n, img_refs.real, img_refs.reference,
            shader_refs.n, shader_refs.real, shader_refs.reference,
            decl_refs.n, decl_refs.real, decl_refs.reference,
        ));

    let global_memory = global.image_memory();
    report.push(global_memory.report_row("image memory global generation"));
    report.push(format!(
        "canonical materials after absorb: n={} images={} decoded={}",
        global.materials.len(),
        global_memory.images,
        global_memory.decoded_images,
    ));
    let shader_census = global.shader_source_census();
    report.push(format!(
        "shader source corpus: programs={} unresolved_aliases={} byteless={}",
        shader_census.programs, shader_census.unresolved_aliases, shader_census.byteless,
    ));
    let decl_streams = global.vertex_decl_stream_census();
    report.push(format!(
        "vertex decls: n={} stream0={} ppcc0t0t0nn n={} streams={}",
        decl_streams.n,
        decl_streams.stream0,
        decl_streams.ppcc_n,
        decl_streams
            .ppcc_stream_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| "missing".into()),
    ));

    // The donors first, and only then the plan that claims the same names
    // they do. Its claims are resolved against the catalog they leave behind
    // rather than against the one it was built from, which is why it waited.
    for kept in common.donor_images().await {
        let job = load_jobs::open(JobKind::ImageDecode)
            .namespace(kept.namespace)
            .canonical(kept.label)
            .depends_on(kept.job)
            .plan_ready()
            .enqueued()
            .started()
            .finished()
            .joined();
        merge_image_batch(
            &mut global,
            kept.label,
            kept.batch.share(),
            job,
            &mut report,
        );
    }
    if let Some(held) = common_images
        && let Some(pending) = held.prune_then_enqueue(&mut global, &progress, &mut report)
    {
        let job = pending.job;
        let (label, batch) = pending.join().await;
        let kept = common.retain(&batch);
        report.push(format!(
            "{label} payloads kept for the next map: +{:.1}MiB (common set holds {} payloads, {:.1}MiB)",
            kept as f64 / (1024.0 * 1024.0),
            common.retained_payloads(),
            common.retained_bytes() as f64 / (1024.0 * 1024.0),
        ));
        merge_image_batch(&mut global, label, batch, job, &mut report);
    }
    if let Ok(path) = &zone_ff {
        let stage = progress.begin_scoped(StageId::Images, "merged", None);
        let decoded = crate::decode_material_color_maps(path, &mut global, &stage, load_pool());
        stage.finish_from(&decoded);
        match decoded {
            Ok(stats) => report.push(format!(
                "merged material images: {}/{} decoded, {} missing, {} unsupported",
                stats.decoded, stats.requested, stats.missing, stats.unsupported
            )),
            Err(error) => report.push(format!("merged material images gap: {error}")),
        }
    }
    if let Some(draw) = world.draw.as_mut() {
        crate::resolve_primary_light_attenuation(draw, &global, &common_light_defs);
        let dynamic_light_name =
            (map_namespace == Some(crate::AssetNamespace::Iw4)).then_some("light_dynamic");
        let dynamic_light = dynamic_light_name.and_then(|name| {
            crate::resolve_named_light_def(name, &draw.light_defs, &common_light_defs, &global)
        });
        let (ordinal, source) =
            crate::resolve_outdoor_image(draw.outdoor_image_name.as_deref(), &global);
        draw.outdoor_image = ordinal;
        if source == "$outdoor" && draw.outdoor_image_name.is_none() {
            draw.outdoor_image_name = Some("$outdoor".into());
        }
        report.push(format!(
            "outdoorImage: source={source} name={} global_ordinal={} lookup_m00={:.6e} lookup_m30={:.4}",
            draw.outdoor_image_name.as_deref().unwrap_or("-"),
            draw.outdoor_image
                .map(|i| i.to_string())
                .unwrap_or_else(|| "NONE".into()),
            f32::from_bits(draw.outdoor_lookup[0]),
            f32::from_bits(draw.outdoor_lookup[12]),
        ));
        let named = draw
            .primary_lights
            .iter()
            .filter(|light| light.def_name.as_ref().is_some_and(|name| !name.is_empty()))
            .count();
        let atten = draw
            .primary_lights
            .iter()
            .filter(|light| light.attenuation_image.is_some())
            .count();
        report.push(format!(
            "GfxLightDef resolve: common_defs={} map_defs={} named_lights={named} atten_image={atten} (light-def name → global catalog; missing stays None)",
            common_light_defs.len(),
            draw.light_defs.len(),
        ));
        report.push(format!(
            "GfxLightDef names: common={:?} map={:?} lights={:?} images_common={:?} images_map={:?}",
            common_light_defs
                .iter()
                .map(|def| def.name.as_str())
                .collect::<Vec<_>>(),
            draw.light_defs
                .iter()
                .map(|def| def.name.as_str())
                .collect::<Vec<_>>(),
            draw.primary_lights
                .iter()
                .filter_map(|light| light.def_name.as_deref())
                .collect::<Vec<_>>(),
            common_light_defs
                .iter()
                .map(|def| def.attenuation_image_name.as_deref())
                .collect::<Vec<_>>(),
            draw.light_defs
                .iter()
                .map(|def| def.attenuation_image_name.as_deref())
                .collect::<Vec<_>>(),
        ));
        if let Ok(path) = &zone_ff {
            let mut requested: Vec<(usize, u8)> = draw
                .primary_lights
                .iter()
                .filter_map(|light| Some((light.attenuation_image?, light.attenuation_sampler)))
                .collect();
            if let Some(dynamic) = dynamic_light
                && let Some(image) = dynamic.attenuation_image
            {
                requested.push((image, dynamic.attenuation_sampler));
            }
            let want: std::collections::BTreeSet<usize> =
                requested.iter().map(|(index, _)| *index).collect();
            let want = want.len();
            let stage = progress.begin_scoped(StageId::Images, "attenuation", None);
            let decoded = crate::decode_catalog_images_from_iwd(
                path,
                &mut global,
                requested,
                &stage,
                load_pool(),
            );
            stage.finish_from(&decoded);
            match decoded {
                Ok(n) => report.push(format!(
                    "IWD light attenuation: decoded {n} of {want} GfxLightDef images (Image_LoadFromIwi; empty payload is not a host ramp)"
                )),
                Err(error) => report.push(format!("IWD light attenuation: {error}")),
            }
        }

        crate::resolve_primary_light_attenuation(draw, &global, &common_light_defs);
        let resolved_dynamic = dynamic_light_name.and_then(|name| {
            crate::resolve_named_light_def(name, &draw.light_defs, &common_light_defs, &global)
        });
        let dynamic_decoded = resolved_dynamic
            .and_then(|light| light.attenuation_image)
            .is_some_and(|index| {
                global
                    .images
                    .get(index)
                    .is_some_and(|image| image.decoded.is_some())
            });
        report.push(format!(
            "FX light_dynamic: image={:?} decoded={} width={:?} sampler={}",
            resolved_dynamic.and_then(|light| light.attenuation_image),
            dynamic_decoded,
            resolved_dynamic.and_then(|light| light.falloff_image_width),
            resolved_dynamic.map_or(0, |light| light.attenuation_sampler),
        ));
        world.dynamic_light =
            resolved_dynamic.filter(|light| dynamic_decoded && light.falloff_image_width.is_some());
        if dynamic_light_name.is_some() && world.dynamic_light.is_none() {
            report.push(
                "FX light_dynamic GAP: light definition or decoded attenuation image missing; additional FX lights unavailable"
                    .into(),
            );
        }
    }
    let builtins = crate::decode_in_zone_builtin_images(&mut global);
    if builtins != 0 {
        report.push(format!(
            "in-zone builtin images: decoded {builtins} leftover $ 2D loadDefs after absorb"
        ));
    }
    if let Ok(path) = &zone_ff {
        let stage = progress.begin_scoped(StageId::Images, "tracers", None);
        let decoded = crate::material_images::decode_color_or_2d_for_names(
            path,
            &mut global,
            common_tracers.named_materials(),
            &stage,
            load_pool(),
        );
        stage.finish_from(&decoded);
        match decoded {
            Ok(n) => report.push(format!(
                "tracer beam images after absorb: {n} TS_COLOR_MAP/TS_2D decoded"
            )),
            Err(error) => report.push(format!("tracer beam images after absorb: {error}")),
        }
    }
    let unique: std::collections::BTreeSet<String> = common_tracers
        .named_materials()
        .map(str::to_owned)
        .collect();
    for name in &unique {
        let bind = crate::fx_material_bind_name(name);
        let twins: Vec<&str> = global
            .materials
            .iter()
            .filter(|m| m.name.as_str() == bind)
            .map(|m| m.name.as_str())
            .collect();
        report.push(format!("tracer material `{name}` global twins={twins:?}"));
    }
    report.push(format!(
        "tracer color maps after absorb: Bound into global ({} unique names; no clone sidecar)",
        unique.len()
    ));

    world.fx.absorb(common_fx);
    let common_fx_model_n = common_fx_models.len();
    let map_fx_model_n = world.fx_models.len();
    let common_fx_model_added = world.fx_models.absorb(common_fx_models);
    let leftover_t5_fx_n = t5_fx.len();
    let leftover_t5_fx_gaps = t5_fx.capture_gaps;
    world.fx.absorb_missing(t5_fx);
    report.push(format!(
        "leftover t5 fx absorb_missing: donor={leftover_t5_fx_n} gaps={leftover_t5_fx_gaps} host now {}",
        world.fx.len()
    ));
    let fx_model_walked_n = world.fx_models.len();
    world.fx_models.keep_referenced(&world.fx.model_hints());
    world.fx.resolve_model_edges(&world.fx_models);
    let fx_model_edges = world.fx.model_edge_census();
    report.push(format!(
        "FX model generation: map={map_fx_model_n} common={common_fx_model_n} added={common_fx_model_added} walked={fx_model_walked_n} retained={} edges bound={} unresolved={} absent={}",
        world.fx_models.len(),
        fx_model_edges.bound,
        fx_model_edges.unresolved,
        fx_model_edges.absent,
    ));
    weapons.resolve_combat_fx(&world.fx, &common_tracers);
    weapons.resolve_projectile_fx_edges(&world.fx);
    let projectile_fx = weapons.projectile_fx_edge_census();
    report.push(format!(
        "weapon projectile FX after absorb: bound={} unresolved={} absent={}",
        projectile_fx.bound, projectile_fx.unresolved, projectile_fx.absent,
    ));
    {
        let materials = &global;
        let fx_model_materials = world.fx_models.resolve_materials(materials);
        report.push(format!(
            "FX model materialHandles after absorb: bound={} unresolved={} absent={}",
            fx_model_materials.bound, fx_model_materials.unresolved, fx_model_materials.absent,
        ));
        weapons.resolve_hud_material_edges(materials);
        if let Some(glass) = world.fx_glass.as_mut() {
            glass.resolve_material_edges(materials);
            let census = glass.material_edge_census();
            report.push(format!(
                "glass Material* after absorb: bound={} unresolved={} absent={}",
                census.bound, census.unresolved, census.absent
            ));
        }
        let hud_materials = weapons.hud_material_edge_census();
        let technique_sets = materials.technique_set_edge_census();
        report.push(format!(
            "material pointer graph after finalize: weapon_hud bound={} unresolved={} absent={}; technique_set bound={} unresolved={} absent={}",
            hud_materials.bound,
            hud_materials.unresolved,
            hud_materials.absent,
            technique_sets.bound,
            technique_sets.unresolved,
            technique_sets.absent,
        ));
        let graph = crate::resolve_after_absorb(
            materials,
            &mut common_tracers,
            &mut world.fx,
            None,
            Some(&mut bodies),
            Some(&mut world_weapons),
            None,
            Some(&mut fpv_meshes),
            Some(&mut projectile_meshes),
        );
        report.push(format!(
            "asset graph after absorb: tracer_mat bound={} unresolved={} absent={}; fx_elem bound={} unresolved={} absent={}; fx_child bound={} unresolved={} absent={}; fx_runner bound={} unresolved={} absent={}; body materialHandles bound={} unresolved={} absent={}; world-gun materialHandles bound={} unresolved={} absent={}; FPV materialHandles bound={} unresolved={} absent={} (WeaponDef.tracerType stamped at common_mp walk)",
            graph.tracer_materials.bound,
            graph.tracer_materials.unresolved,
            graph.tracer_materials.absent,
            graph.fx_elem_materials.bound,
            graph.fx_elem_materials.unresolved,
            graph.fx_elem_materials.absent,
            graph.fx_nested_children.bound,
            graph.fx_nested_children.unresolved,
            graph.fx_nested_children.absent,
            graph.fx_runner_children.bound,
            graph.fx_runner_children.unresolved,
            graph.fx_runner_children.absent,
            graph.xmodel_body_materials.bound,
            graph.xmodel_body_materials.unresolved,
            graph.xmodel_body_materials.absent,
            graph.xmodel_gun_materials.bound,
            graph.xmodel_gun_materials.unresolved,
            graph.xmodel_gun_materials.absent,
            graph.xmodel_fpv_materials.bound,
            graph.xmodel_fpv_materials.unresolved,
            graph.xmodel_fpv_materials.absent,
        ));
        report.push(format!(
            "Material stamp: iw4={} t5={} iw5={} (name-link identity; colliding T5 names are the later-zone row)",
            materials.namespace_count(crate::AssetNamespace::Iw4),
            materials.namespace_count(crate::AssetNamespace::T5),
            materials.namespace_count(crate::AssetNamespace::Iw5),
        ));
        report.push(format!(
            "fx elem material edges after absorb: {} bound ({} unique), {} unresolved (temp={}, catalog_miss={}), {} absent of {} Material* visuals (FxElemDef+0xbc); {} decal mark arms ({} Bound slots, {} unresolved, {} temp, {} array-unpatched)",
            world.fx.material_visual_bound_count(),
            world.fx.material_visual_unique_bound_count(),
            world.fx.material_visual_unresolved_count(),
            world.fx.material_visual_unresolved_temp_count(),
            world.fx.material_visual_unresolved_miss_count(),
            world.fx.material_visual_absent_count(),
            world.fx.material_visual_count(),
            world.fx.material_visual_decal_count(),
            world.fx.decal_mark_bound_slot_count(),
            world.fx.decal_mark_unresolved_slot_count(),
            world.fx.decal_mark_temp_slot_count(),
            world.fx.material_visual_decal_unpatched_count()
        ));
        report.push(format!(
            "fx elem decal unique Bound after absorb: {} (mc={}, wc={}); decoded color/2D in catalog {} of {} (not sprite Plan)",
            world.fx.unique_decal_mark_count(),
            world.fx.unique_decal_mark_mc_count(),
            world.fx.unique_decal_mark_wc_count(),
            world.fx.unique_decal_mark_decoded_color_count(materials),
            world.fx.unique_decal_mark_count()
        ));
        let miss = world
            .fx
            .unique_decal_mark_decoded_miss_samples(materials, 8);
        if !miss.is_empty() {
            report.push(format!(
                "fx elem decal catalog nocolor sample: {}",
                miss.join("; ")
            ));
        }
        let samples = world.fx.unresolved_material_samples(8);
        if !samples.is_empty() {
            report.push(format!(
                "fx elem unresolved Material* sample: {}",
                samples.join("; ")
            ));
        }
        let mark_samples = world.fx.decal_mark_samples(8);
        if !mark_samples.is_empty() {
            report.push(format!(
                "fx elem decal mark sample: {}",
                mark_samples.join("; ")
            ));
        }
    }

    report.push(
        "fx color maps handoff: n=0 bytes=0 (Bound GPU bind at spawn; no CPU clone sidecar; stub_aliases=0)"
            .into(),
    );
    {
        let missing: Vec<String> = world
            .fx
            .unique_bound_hints()
            .into_iter()
            .chain(world.fx.unique_decal_mark_hints())
            .filter(|(_, hint)| !crate::fx_color_decoded_in_catalog(&global, hint))
            .map(|(_, hint)| hint.to_owned())
            .collect();
        if !missing.is_empty() {
            if let Ok(path) = &zone_ff {
                let stage = progress.begin_scoped(StageId::Images, "fx_elem", None);
                let decoded = crate::material_images::decode_color_or_2d_for_names(
                    path,
                    &mut global,
                    missing.iter(),
                    &stage,
                    load_pool(),
                );
                stage.finish_from(&decoded);
                match decoded {
                    Ok(n) => report.push(format!(
                        "fx elem 2d images after absorb: {n} TS_COLOR_MAP/TS_2D decoded"
                    )),
                    Err(error) => report.push(format!("fx elem 2d images after absorb: {error}")),
                }
            }
        }
        let nocolor: Vec<(usize, String)> = world
            .fx
            .unique_bound_hints()
            .into_iter()
            .filter(|(_, hint)| !crate::fx_color_decoded_in_catalog(&global, hint))
            .map(|(index, hint)| (index, hint.to_owned()))
            .collect();
        if !nocolor.is_empty() {
            let (distortion, other): (Vec<_>, Vec<_>) = nocolor
                .into_iter()
                .partition(|(_, hint)| hint.contains("distortion"));
            report.push(format!(
                "fx elem Bound without decoded color: {} of {} unique (distortion={}, other={}) other_sample: {}",
                distortion.len() + other.len(),
                world.fx.material_visual_unique_bound_count(),
                distortion.len(),
                other.len(),
                if other.is_empty() {
                    "-".to_string()
                } else {
                    other
                        .iter()
                        .map(|(index, hint)| format!("{index}:{hint}"))
                        .collect::<Vec<_>>()
                        .join("; ")
                }
            ));
            for (index, hint) in &other {
                let Some(mat) = global.materials.get(*index) else {
                    report.push(format!(
                        "fx elem Bound `{index}:{hint}` has no global material row"
                    ));
                    continue;
                };
                let sem: Vec<u8> = mat.textures.iter().map(|t| t.semantic).collect();
                let decoded = mat.textures.iter().any(|t| {
                    t.image
                        .and_then(|i| global.images.get(i))
                        .is_some_and(|img| img.decoded.is_some())
                });
                report.push(format!(
                    "fx elem Bound `{index}:{}` techset={} camera_region={} tex={} sem={sem:?} decoded={decoded}",
                    mat.name,
                    mat.technique_set,
                    mat.camera_region,
                    mat.textures.len(),
                ));
            }
        }
    }
    if world.impact_fx.is_none() {
        world.impact_fx = common_impact;
    } else if let Some(common_table) = common_impact {
        report.push(format!(
            "impactfx: map table kept; common_mp table `{}` discarded",
            common_table.name
        ));
    }
    if let Some(ref table) = world.impact_fx {
        report.push(format!(
            "impactfx handoff: `{}` rows={} (fx catalog now {} defs)",
            table.name,
            table.row_count(),
            world.fx.len()
        ));
    } else {
        report.push("impactfx handoff: missing — combat play_oriented will miss cells".into());
    }
    report.push(format!(
        "tracer catalog handoff: {} named ({} bound, {} unresolved) (CG_SpawnTracer)",
        common_tracers.len(),
        common_tracers.bound_count(),
        common_tracers.unresolved_count()
    ));

    let prepared_map = PreparedMap {
        zone: zone_name,
        namespace: map_namespace,
        spawns: dm_spawns,
        facts,
        gaps: PreparedGaps { lines: gap_lines },
    };
    if prepared_map.facts.minimap_corners.is_some() {
        report.push("compass: minimap_corner pair from MapEnts".into());
    } else {
        report.push("compass gap: minimap_corner missing — no world-to-map frame".into());
    }
    match prepared_map.facts.north_yaw {
        Some(yaw) => report.push(format!("compass: worldspawn northyaw {yaw}")),
        None => {
            report.push("compass gap: worldspawn has no northyaw — map is drawn north-up".into())
        }
    }

    report.extend(localize_report);
    let (directory_ms, opens, inflate_ms) = crate::iwd_read_cost();
    report.push(format!(
        "IWD read cost: central_dir={directory_ms:.0}ms over {opens} opens, inflate={inflate_ms:.0}ms (summed over worker threads, not wall)"
    ));
    let (mip_hit, mip_miss, mip_io_ms) = crate::mip_cache_cost();
    report.push(format!(
        "mip cache: hit={mip_hit} miss={mip_miss} io={mip_io_ms:.0}ms"
    ));
    let (payload_reads, header_reads) = crate::iwd_entry_reads();
    report.push(format!(
        "IWD entry reads: payload={payload_reads} header-only={header_reads} (a header answers whether an image is a cubemap; a payload read is the whole entry inflated)"
    ));
    if let Some(draw) = world.draw.as_ref() {
        let world_mats = draw.batches.iter().filter_map(|batch| {
            let local = batch.material?;
            map_ids.get(local).copied().flatten().or(Some(local))
        });

        let smodel_mats = world.static_model_meshes.iter().flat_map(|mesh| {
            mesh.lod_surfaces.iter().flatten().filter_map(|surface| {
                let local = surface.material?;
                map_ids.get(local).copied().flatten()
            })
        });
        let fpv_mats = fpv_meshes.bound_material_indices();
        let fx_mats = world
            .fx
            .unique_bound_hints()
            .into_iter()
            .map(|(index, _)| index)
            .chain(
                world
                    .fx
                    .unique_decal_mark_hints()
                    .into_iter()
                    .map(|(index, _)| index),
            )
            .chain(
                common_tracers
                    .defs()
                    .filter_map(|def| def.material.bound_index()),
            );
        let mut set = crate::material_images::census_image_working_set(
            &global,
            world_mats,
            smodel_mats,
            fpv_mats,
            fx_mats,
        );
        let (probe_n, probe_bytes) = crate::material_images::cpu_image_census(
            world.reflection_probe_images.iter().flatten(),
        );
        let (lightmap_n, lightmap_bytes) = match &draw.lightmap {
            Ok(pages) => {
                crate::material_images::cpu_image_census(pages.iter().flatten().flat_map(|page| {
                    [
                        page.primary_image.as_ref(),
                        page.secondary_image.as_ref(),
                        Some(&page.ambient_image),
                        Some(&page.directional_image),
                        Some(&page.sun_mask_image),
                    ]
                    .into_iter()
                    .flatten()
                }))
            }
            Err(_) => (0, 0),
        };
        set.probe_n = probe_n;
        set.probe_bytes = probe_bytes;
        set.lightmap_n = lightmap_n;
        set.lightmap_bytes = lightmap_bytes;
        crate::material_images::store_image_working_set(set);
        report.push(format!(
            "image working set: decoded={} ({:.1}MiB) world-batch={} ({:.1}MiB) smodel={} ({:.1}MiB) fpv={} ({:.1}MiB) probe={} ({:.1}MiB) lightmap={} ({:.1}MiB) fx={} ({:.1}MiB); retail R_SyncRenderAssets enumerates the live DB — this row measures subsets, it does not skip a load",
            set.decoded_n,
            set.decoded_bytes as f64 / (1024.0 * 1024.0),
            set.world_n,
            set.world_bytes as f64 / (1024.0 * 1024.0),
            set.smodel_n,
            set.smodel_bytes as f64 / (1024.0 * 1024.0),
            set.fpv_n,
            set.fpv_bytes as f64 / (1024.0 * 1024.0),
            set.probe_n,
            set.probe_bytes as f64 / (1024.0 * 1024.0),
            set.lightmap_n,
            set.lightmap_bytes as f64 / (1024.0 * 1024.0),
            set.fx_n,
            set.fx_bytes as f64 / (1024.0 * 1024.0),
        ));
    }
    let fx = std::mem::take(&mut world.fx).publish();
    let xanims = xanims.publish();
    let destructible_death =
        crate::stamp_match_destructible_death(&xanims, &world.map_xmodel_scene_assets);
    for row in &destructible_death {
        report.push(format!(
            "destructible death {}: clip={} husk={}",
            row.kind,
            row.clip.edge_kind(),
            row.husk.edge_kind()
        ));
    }
    let prepared = PreparedMatch {
        fx,
        world,
        materials: crate::MatchMaterials {
            population: global,
            map_ids,
            common_profile_id: common.id,
            products_id: NEXT_PRODUCTS_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        },
        clip,
        weapons: weapons.publish(),
        fpv_meshes: fpv_meshes.publish(),
        bodies: bodies.publish(),
        world_weapons: world_weapons.publish(),
        projectile_meshes: projectile_meshes.publish(),
        xanims,
        destructible_death,
        player_anim_sources,
        tracers: common_tracers.publish(),
        strings,
        report,
        prepared_map,
        pen_table: common_pen_table,
        pen_table_loaded: common_pen_loaded,
        lochit_table: common_lochit_table,
        xmodel_walk,
        sound,
    };
    (MatchLoadOutcome::Ready(prepared), Some(common))
}

#[derive(Clone, Debug, Default)]
pub struct MatchMaterialSeed {
    pub catalog: MaterialCatalog,
}

pub fn load_match_material_seed(
    progress: &LoadProgress,
) -> Result<(MatchMaterialSeed, Vec<String>), String> {
    let root = games_root_from_env()?;
    let runtime = find_zone_file_version(&root, "common_mp", fastfile_iw4::ZONE_VERSION_PC)?;

    let (catalog, mut report, _, _) = bevy::tasks::futures_lite::future::block_on(
        walk_startup_material_zones(Some(&runtime.path), progress),
    );
    let catalog =
        walk_t5_leftover_materials(Some(runtime.path.as_path()), progress, catalog, &mut report);
    let (catalog, common_report) = walk_material_file(&runtime.path, progress, catalog);
    report.extend(common_report);
    report.push(format!(
        "match material seed: materials={} techsets={} shaders={} decls={} decoded={}",
        catalog.materials.len(),
        catalog.technique_set_facts().len(),
        catalog.shaders.len(),
        catalog.vertex_decls.len(),
        catalog.image_memory().decoded_images,
    ));
    Ok((MatchMaterialSeed { catalog }, report))
}

pub fn apply_match_material_map(
    seed: &MatchMaterialSeed,
    zone_ff: &Path,
    progress: &LoadProgress,
) -> (MaterialCatalog, Vec<String>) {
    let mut report = Vec::new();
    let runtime = games_root_from_env()
        .ok()
        .and_then(|root| find_runtime_common_mp(&root, zone_ff).ok());
    let (foreign, foreign_report) = walk_foreign_material_population(
        Some(zone_ff),
        runtime.as_ref().map(|found| found.path.as_path()),
        progress,
    );
    report.extend(foreign_report);
    let mut catalog = seed.catalog.clone();
    catalog.absorb_asset_population(foreign);

    let common_techsets = catalog.technique_set_facts().to_vec();
    let (mut catalog, map_report) = walk_material_file(zone_ff, progress, catalog);
    report.extend(map_report);
    let absorbed = catalog.absorb_technique_set_tables(&common_techsets);
    let promoted = catalog.promote_iw5_fallback_tables();
    let t5_alias = catalog.absorb_t5_feature_token_donors();
    let stub_routed = catalog.reroute_stub_materials();
    report.push(format!(
        "material route (census): absorbed_techsets={absorbed} t5_tech_alias={t5_alias} iw5_promoted={promoted} stub_routed={stub_routed} unrouted={}",
        catalog.unrouted_material_count()
    ));
    (catalog, report)
}

pub fn load_match_material_catalog(
    zone_ff: &Path,
    progress: &LoadProgress,
) -> Result<(MaterialCatalog, Vec<String>), String> {
    let (seed, mut report) = load_match_material_seed(progress)?;
    let (catalog, map_report) = apply_match_material_map(&seed, zone_ff, progress);
    report.extend(map_report);
    Ok((catalog, report))
}

fn walk_material_file(
    path: &Path,
    progress: &LoadProgress,
    seed: MaterialCatalog,
) -> (MaterialCatalog, Vec<String>) {
    let population = walk_population_file(path, progress, seed);
    (population.materials, population.report)
}

fn walk_population_file(
    path: &Path,
    progress: &LoadProgress,
    seed: MaterialCatalog,
) -> crate::lane::MaterialPopulation {
    let zone_name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "zone".into());
    let stage = progress.begin_scoped(StageId::CommonAssets, zone_name.clone(), None);
    let opened = open_zone_shared(path);
    finish_zone_open(stage, &opened);
    match opened {
        Ok(image) => lane(image.game).load_material_population(path, &image, progress, seed),
        Err(error) => crate::lane::MaterialPopulation {
            materials: seed,
            report: vec![format!("match materials: open {}: {error}", path.display())],
            ..Default::default()
        },
    }
}

fn walk_t5_leftover_materials(
    runtime_common: Option<&Path>,
    progress: &LoadProgress,
    seed: MaterialCatalog,
    report: &mut Vec<String>,
) -> MaterialCatalog {
    let root = match games_root_from_env() {
        Ok(root) => root,
        Err(error) => {
            report.push(format!("t5 leftover common: {error}"));
            return seed;
        }
    };
    let donor = match find_common_mp_for_envelope(&root, fastfile_t5::ZONE_VERSION_PC) {
        Ok(donor) => donor,
        Err(error) => {
            report.push(format!("t5 leftover common: {error}"));
            return seed;
        }
    };
    if runtime_common.is_some_and(|path| path == donor.path.as_path()) {
        report.push(format!(
            "t5 leftover common: runtime path is already {} — skip",
            donor.path.display()
        ));
        return seed;
    }
    let (leftover_startup, _, leftover_startup_report) =
        leftover_walk_t5_startup_materials(progress);
    report.extend(leftover_startup_report);
    let leftover_startup_n = leftover_startup.materials.len();
    let mut seed = seed;
    seed.absorb_missing_reals(leftover_startup);
    report.push(format!(
        "t5 leftover startup gfx absorb_missing_reals: donor={leftover_startup_n} host now {}",
        seed.materials.len()
    ));
    let (catalog, walked) = walk_material_file(&donor.path, progress, seed);
    report.extend(walked);
    report.push(format!(
        "t5 leftover common: path={} materials={}",
        donor.path.display(),
        catalog.materials.len(),
    ));
    catalog
}

fn leftover_walk_t5_startup_materials(
    progress: &LoadProgress,
) -> (
    MaterialCatalog,
    Vec<crate::CapturedStringTable>,
    Vec<String>,
) {
    let mut report = Vec::new();
    let root = match games_root_from_env() {
        Ok(root) => root,
        Err(error) => {
            report.push(format!("t5 leftover startup gfx: {error}"));
            return (MaterialCatalog::default(), Vec::new(), report);
        }
    };
    const ZONES: [&str; 2] = ["code_post_gfx_mp", "localized_code_post_gfx_mp"];
    let mut seed = MaterialCatalog::default();
    let mut stats = Vec::new();
    for zone in ZONES {
        let found = match find_zone_file_version(&root, zone, fastfile_t5::ZONE_VERSION_PC) {
            Ok(found) => found,
            Err(error) => {
                report.push(format!("t5 leftover startup gfx gap: {zone}: {error}"));
                continue;
            }
        };
        let population = walk_population_file(&found.path, progress, seed);
        report.extend(population.report);
        if zone == "code_post_gfx_mp" {
            stats = population.cac_tables;
        }
        seed = population.materials;
    }
    report.push(format!(
        "t5 leftover startup gfx: materials={}",
        seed.materials.len()
    ));
    (seed, stats, report)
}

fn walk_foreign_material_population(
    zone_ff: Option<&Path>,
    runtime_common: Option<&Path>,
    progress: &LoadProgress,
) -> (MaterialCatalog, Vec<String>) {
    let mut report = Vec::new();
    let Some(zone_ff) = zone_ff else {
        return (MaterialCatalog::default(), report);
    };
    let donor = match find_common_mp_for_zone(zone_ff) {
        Ok(donor) => donor,
        Err(error) => {
            report.push(format!(
                "foreign data common: no same-tree common_mp ({error})"
            ));
            return (MaterialCatalog::default(), report);
        }
    };
    if runtime_common.is_some_and(|path| path == donor.path.as_path()) {
        return (MaterialCatalog::default(), report);
    }
    match peek_zone_version(&donor.path) {
        Some(fastfile_iw5::ZONE_VERSION_PC) => {}
        Some(version) => {
            report.push(format!(
                "foreign data common: skip envelope {version:#x} at {} (IW5 material data only)",
                donor.path.display()
            ));
            return (MaterialCatalog::default(), report);
        }
        None => {
            report.push(format!(
                "foreign data common: unreadable envelope at {}",
                donor.path.display()
            ));
            return (MaterialCatalog::default(), report);
        }
    }
    let (materials, walked) = walk_material_file(&donor.path, progress, MaterialCatalog::default());
    report.extend(walked);
    report.push(format!(
        "foreign data common: path={} materials={} techsets={} (weapons discarded)",
        donor.path.display(),
        materials.materials.len(),
        materials.technique_set_facts().len(),
    ));
    (materials, report)
}

fn resolve_foreign_material_donor(
    zone_ff: Option<&Path>,
    runtime_common: Option<&Path>,
    report: &mut Vec<String>,
) -> Option<PathBuf> {
    let zone_ff = zone_ff?;
    let donor = match find_common_mp_for_zone(zone_ff) {
        Ok(donor) => donor,
        Err(error) => {
            report.push(format!(
                "foreign data common: no same-tree common_mp ({error})"
            ));
            return None;
        }
    };
    if runtime_common.is_some_and(|path| path == donor.path.as_path()) {
        return None;
    }
    match peek_zone_version(&donor.path) {
        Some(fastfile_iw5::ZONE_VERSION_PC) => Some(donor.path),
        Some(version) => {
            report.push(format!(
                "foreign data common: skip envelope {version:#x} at {} (IW5 material data only)",
                donor.path.display()
            ));
            None
        }
        None => {
            report.push(format!(
                "foreign data common: unreadable envelope at {}",
                donor.path.display()
            ));
            None
        }
    }
}

fn walk_foreign_material_common(
    donor: &Path,
    progress: &LoadProgress,
    job: load_jobs::Job,
) -> (MaterialCatalog, Option<PendingImages>, Vec<String>) {
    let mut report = Vec::new();
    let Some(mut census) = capture_common_zone(
        donor,
        "walking same-tree common_mp as material data",
        "foreign data common",
        progress,
        &mut report,
    ) else {
        return (MaterialCatalog::default(), None, report);
    };
    let pending_images = hold_image_plan("IW5 common_mp", census.pending_images.take(), job)
        .map(|held| held.enqueue(progress));
    let materials = census.material_population;
    report.extend(census.report);
    report.push(format!(
        "foreign data common: path={} materials={} techsets={} (weapons discarded)",
        donor.display(),
        materials.materials.len(),
        materials.technique_set_facts().len(),
    ));
    (materials, pending_images, report)
}

fn capture_common_zone(
    donor: &Path,
    stage_label: &str,
    report_label: &str,
    progress: &LoadProgress,
    report: &mut Vec<String>,
) -> Option<crate::lane::CommonCensus> {
    if progress.is_canceled() {
        report.push(format!("{report_label}: canceled before the walk started"));
        return None;
    }
    let stage = progress.begin_scoped(StageId::CommonAssets, stage_label, None);
    let opened = open_zone_shared(donor);
    finish_zone_open(stage, &opened);
    match opened {
        Ok(image) => Some(lane(image.game).load_common_mp(
            donor,
            &image,
            progress,
            true,
            MaterialCatalog::default(),
        )),
        Err(error) => {
            report.push(format!("{report_label}: open {}: {error}", donor.display()));
            None
        }
    }
}

fn merge_image_batch(
    global: &mut crate::MaterialDefinitions,
    label: &str,
    batch: crate::material_images::DecodedImageBatch,
    job: load_jobs::Job,
    report: &mut Vec<String>,
) {
    let requested = batch.stats.requested;
    let missing = batch.stats.missing;
    let unsupported = batch.stats.unsupported;
    let first_gap = batch.stats.first_gap.clone();
    let census = batch.apply(global);
    job.bytes(None, Some(census.final_cpu_bytes))
        // What the plan prepared and what it served out of another plan's work,
        // kept apart: only the first is work this plan did.
        .prepared(census.newly_prepared_bytes, census.reused_bytes)
        .merged(
            census.final_cpu_bytes,
            census.discarded_decoded_bytes,
            census.discard_line(),
        );
    report.push(format!(
            "{label} claimed images: {}/{requested} into the merged pool ({} already decoded by an earlier source, {missing} missing, {unsupported} unsupported, {} claimed rows dropped by the merge)",
            census.filled_rows, census.already_decoded, census.discarded_variants,
        ));
    report.push(format!(
        "{label} image demand: claimed_rows={} canonical_variants={} prepared_variants={} duplicate_claims={} pruned_variants={} pruned_rows={} final_cpu_bytes={} newly_prepared_bytes={} reused_variants={} reused_bytes={} discarded_decoded_bytes={} discarded_same_payload={}",
        census.claimed_rows,
        census.canonical_variants,
        census.prepared_variants,
        census.duplicate_claims,
        census.pruned_variants,
        census.pruned_rows,
        census.final_cpu_bytes,
        census.newly_prepared_bytes,
        census.reused_variants,
        census.reused_bytes,
        census.discarded_decoded_bytes,
        census.discarded_same_payload,
    ));
    if let Some(line) = census.discard_line() {
        report.push(format!("{label} image discard: {line}"));
    }
    // Who answered for the claims this plan prepared and the merge threw away,
    // which is what says whether a claim could have been resolved earlier.
    if !census.disputed_winners.is_empty() {
        report.push(format!(
            "{label} image dispute: {}",
            census
                .disputed_winners
                .iter()
                .map(|winner| format!(
                    "{}={} won_by={} first={}",
                    winner.kind,
                    winner.claims,
                    winner
                        .plan
                        .map_or_else(|| "none".to_owned(), |plan| format!("plan{plan}")),
                    winner.first,
                ))
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    if let Some(gap) = first_gap {
        report.push(format!("{label} claimed image gap: {gap}"));
    }
}

/// An image plan already on the pool, and the job row that records when it got
/// there. The walk that discovers the demand hands both over itself.
struct PendingImages {
    job: load_jobs::Job,
    task: bevy::tasks::Task<(&'static str, crate::material_images::DecodedImageBatch)>,
}

impl PendingImages {
    async fn join(self) -> (&'static str, crate::material_images::DecodedImageBatch) {
        let done = self.task.await;
        self.job.joined();
        done
    }
}

/// A plan whose walk has finished, holding the job row that records when it did.
///
/// `job` is opened by the producer when it starts looking, so the row carries
/// discovered → ready → enqueued → started → finished → joined. A plan waiting
/// for the catalog spends that wait in `ready → enqueued`, not in a decode
/// timer.
struct HeldImagePlan {
    label: &'static str,
    plan: ImageDemandPlan,
    job: load_jobs::Job,
}

/// Take a finished walk's plan, if it wants anything at all.
fn hold_image_plan(
    label: &'static str,
    plan: Option<ImageDemandPlan>,
    job: load_jobs::Job,
) -> Option<HeldImagePlan> {
    let plan = plan.filter(|plan| !plan.is_empty())?;
    // The census names the winner of a disputed claim by plan id, so the id
    // and the label are printed together once, here, where both are known.
    diag::info!(
        Zone,
        "image plan: plan{} is {label} ({} canonical variants, {} claimed rows)",
        plan.id(),
        plan.canonical_variants(),
        plan.claimed_rows(),
    );
    let job = job
        .canonical(label)
        .items(plan.canonical_variants() as u64)
        .plan_ready();
    Some(HeldImagePlan { label, plan, job })
}

impl HeldImagePlan {
    /// Put the plan on the load pool now, with every claim it made.
    fn enqueue(self, progress: &LoadProgress) -> PendingImages {
        let Self { label, plan, job } = self;
        let job = job.enqueued();
        let progress = progress.clone();
        let task = load_pool().spawn(async move {
            job.started();
            let stage = progress.begin_scoped(StageId::Images, label, None);
            // The plan splits itself across this same pool. The worker this
            // task is on joins that scope rather than blocking on it, so a
            // plan does not cost a load worker to supervise it.
            let batch = plan.run(&stage, job, load_pool());
            stage.done();
            job.finished();
            (label, batch)
        });
        PendingImages { job, task }
    }

    /// Resolve the plan's claims against the merged catalog, then decode what
    /// is left of it.
    ///
    /// The catalog has to be one every rival plan has already applied into: a
    /// row still carrying this plan's id with nothing decoded in it is a row
    /// this plan will fill, and a name with no such row left is a decode whose
    /// result the merge would count and throw away. Asked earlier, the same
    /// question cuts names a plan still in flight is about to release.
    ///
    /// Returns `None` when nothing survives, which is a plan that never runs
    /// rather than an empty one that does.
    fn prune_then_enqueue(
        mut self,
        catalog: &mut crate::MaterialDefinitions,
        progress: &LoadProgress,
        report: &mut Vec<String>,
    ) -> Option<PendingImages> {
        let resolving = std::time::Instant::now();
        self.plan.prune_to(catalog);
        let resolved_ms = resolving.elapsed().as_secs_f32() * 1000.0;
        let (variants, rows) = self.plan.pruned();
        // The cost of asking is on the line beside what it saved: this runs
        // on the consumer, between the last donor's apply and the decode, so
        // it is serial time the walk pays whatever the decode then skips.
        report.push(format!(
            "{} image claims resolved before decode: {} of {} variants cut ({rows} claimed rows), {} left to decode, {resolved_ms:.1}ms to resolve",
            self.label,
            variants,
            self.plan.canonical_variants(),
            self.plan.len(),
        ));
        if self.plan.is_empty() {
            // Nothing left to decode means nothing will ever call `apply`, and
            // `apply` is what hands the claimed rows back. Do it here instead,
            // or the rows keep a mark saying a decode is owed on them and the
            // stages after this one skip every name the plan had claimed.
            let released = self.plan.release_claims(catalog);
            report.push(format!(
                "{} image plan dropped before decode: every claim was answered elsewhere, {released} claimed rows handed back",
                self.label,
            ));
            self.job.items(0).enqueued().started().finished().joined();
            return None;
        }
        Some(self.enqueue(progress))
    }
}

enum ForeignCommonWork {
    Shared(
        bevy::tasks::Task<(
            MaterialCatalog,
            Iw5WeaponBundle,
            Option<PendingImages>,
            Vec<String>,
        )>,
    ),
    Split(Option<bevy::tasks::Task<(MaterialCatalog, Option<PendingImages>, Vec<String>)>>),
}

#[derive(Default)]
struct Iw5WeaponBundle {
    weapons: WeaponBuild,
    fpv: FpvMeshBuild,
    world_guns: WorldWeaponBuild,
    xanims: XAnimBuild,

    materials: MaterialCatalog,
    stats_tables: Vec<crate::CapturedStringTable>,
}

fn resolve_iw5_weapon_donor(
    runtime_common: Option<&Path>,
    report: &mut Vec<String>,
) -> Option<PathBuf> {
    let Ok(root) = games_root_from_env() else {
        report.push("iw5 weapons: IW4L_GAMES unset".into());
        return None;
    };
    let donor = match find_zone_file_version(&root, "common_mp", fastfile_iw5::ZONE_VERSION_PC) {
        Ok(donor) => donor,
        Err(error) => {
            report.push(format!("iw5 weapons: {error}"));
            return None;
        }
    };
    if runtime_common.is_some_and(|path| path == donor.path.as_path()) {
        report.push(format!(
            "iw5 weapons: skip, runtime common is already IW5 ({})",
            donor.path.display()
        ));
        return None;
    }
    Some(donor.path)
}

fn walk_iw5_weapon_bundle(
    donor: &Path,
    progress: &LoadProgress,
    job: load_jobs::Job,
) -> (Iw5WeaponBundle, Option<PendingImages>, Vec<String>) {
    let mut report = Vec::new();
    let Some(mut census) = capture_common_zone(
        donor,
        "walking IW5 common_mp as weapon bundle",
        "iw5 weapons",
        progress,
        &mut report,
    ) else {
        return (Iw5WeaponBundle::default(), None, report);
    };
    // The producer enqueues the bundle's images itself. Left for the consumer,
    // they wait out the synchronous `common_mp` walk and the unpacking of this
    // tuple: seconds of ready decode work with nobody holding it.
    let pending_images = hold_image_plan("IW5 weapon bundle", census.pending_images.take(), job)
        .map(|held| held.enqueue(progress));
    report.extend(census.report);
    report.push(format!(
        "iw5 weapons: path={} ids={} gun_named={} fpv={} world_guns={} xanims={} materials={}",
        donor.display(),
        census.weapons.len(),
        census.weapons.gun_xmodel_count(),
        census.fpv.len(),
        census.world_weapons.len(),
        census.xanims.len(),
        census.material_population.materials.len(),
    ));
    (
        Iw5WeaponBundle {
            weapons: census.weapons,
            fpv: census.fpv,
            world_guns: census.world_weapons,
            xanims: census.xanims,
            materials: census.material_population,
            stats_tables: census.cac_tables,
        },
        pending_images,
        report,
    )
}

fn walk_shared_iw5_common(
    donor: &Path,
    progress: &LoadProgress,
    job: load_jobs::Job,
) -> (
    MaterialCatalog,
    Iw5WeaponBundle,
    Option<PendingImages>,
    Vec<String>,
) {
    let mut report = Vec::new();
    let Some(mut census) = capture_common_zone(
        donor,
        "walking IW5 common_mp as material data + weapon bundle",
        "iw5 common",
        progress,
        &mut report,
    ) else {
        return (
            MaterialCatalog::default(),
            Iw5WeaponBundle::default(),
            None,
            report,
        );
    };
    // One capture serves the material seed and the weapon bundle, so there is
    // one plan here, not two: the donor is walked once and its images are
    // claimed once.
    let pending_images = hold_image_plan("IW5 common_mp", census.pending_images.take(), job)
        .map(|held| held.enqueue(progress));
    report.extend(census.report);
    let materials = census.material_population;
    report.push(format!(
        "iw5 common (shared capture): path={} materials={} techsets={} ids={} gun_named={} fpv={} world_guns={} xanims={} — one read/inflate/walk serves both the material seed and the weapon bundle; leftover materials are the seeded rows, not a second donor",
        donor.display(),
        materials.materials.len(),
        materials.technique_set_facts().len(),
        census.weapons.len(),
        census.weapons.gun_xmodel_count(),
        census.fpv.len(),
        census.world_weapons.len(),
        census.xanims.len(),
    ));
    (
        materials,
        Iw5WeaponBundle {
            weapons: census.weapons,
            fpv: census.fpv,
            world_guns: census.world_weapons,
            xanims: census.xanims,
            materials: MaterialCatalog::default(),
            stats_tables: census.cac_tables,
        },
        pending_images,
        report,
    )
}

enum T5CommonPrep {
    Skip(Vec<String>),
    Ready {
        donor: PathBuf,
        opened: Result<std::sync::Arc<crate::ZoneImage>, String>,
        leftover_startup: MaterialCatalog,
        leftover_stats: Vec<crate::CapturedStringTable>,
        report: Vec<String>,
    },
}

fn t5_weapon_common_prep(runtime_common: Option<&Path>, progress: &LoadProgress) -> T5CommonPrep {
    let mut report = Vec::new();
    let root = match games_root_from_env() {
        Ok(root) => root,
        Err(error) => {
            report.push(format!("t5 weapon common: {error}"));
            return T5CommonPrep::Skip(report);
        }
    };
    let donor = match find_common_mp_for_envelope(&root, fastfile_t5::ZONE_VERSION_PC) {
        Ok(donor) => donor,
        Err(error) => {
            report.push(format!("t5 weapon common: {error}"));
            return T5CommonPrep::Skip(report);
        }
    };
    if runtime_common.is_some_and(|path| path == donor.path.as_path()) {
        report.push(format!(
            "t5 weapon common: runtime path is already {} — skip absorb",
            donor.path.display()
        ));
        return T5CommonPrep::Skip(report);
    }
    let (leftover_startup, leftover_stats, leftover_startup_report) =
        leftover_walk_t5_startup_materials(progress);
    report.extend(leftover_startup_report);
    let stage = progress.begin_scoped(StageId::CommonAssets, "t5_weapons", None);
    let opened = open_zone_shared(&donor.path).map_err(|error| error.to_string());
    stage.finish_from(&opened);
    T5CommonPrep::Ready {
        donor: donor.path,
        opened,
        leftover_startup,
        leftover_stats,
        report,
    }
}

struct T5WeaponCommon {
    weapons: WeaponBuild,
    fpv: FpvMeshBuild,
    world_guns: WorldWeaponBuild,
    material_seed: MaterialCatalog,
    xanims: XAnimBuild,
    fx: FxCatalog,
    projectiles: crate::ProjectileMeshBuild,
    teamsets: std::collections::HashMap<String, crate::MapTeamSettings>,
    images: Option<PendingImages>,
    stats_tables: (
        Vec<crate::CapturedStringTable>,
        Vec<crate::CapturedStringTable>,
    ),
    report: Vec<String>,
}

impl T5WeaponCommon {
    fn empty(material_seed: MaterialCatalog, report: Vec<String>) -> Self {
        Self {
            weapons: crate::WeaponBuild::default(),
            fpv: FpvMeshBuild::default(),
            world_guns: WorldWeaponBuild::default(),
            material_seed,
            xanims: XAnimBuild::default(),
            fx: FxCatalog::default(),
            projectiles: crate::ProjectileMeshBuild::default(),
            teamsets: Default::default(),
            images: None,
            stats_tables: (Vec::new(), Vec::new()),
            report,
        }
    }
}

fn walk_t5_weapon_common(
    prep: T5CommonPrep,
    progress: &LoadProgress,
    material_seed: MaterialCatalog,
    job: load_jobs::Job,
) -> T5WeaponCommon {
    let (donor, opened, leftover_startup, leftover_stats, mut report) = match prep {
        T5CommonPrep::Skip(report) => return T5WeaponCommon::empty(material_seed, report),
        T5CommonPrep::Ready {
            donor,
            opened,
            leftover_startup,
            leftover_stats,
            report,
        } => (donor, opened, leftover_startup, leftover_stats, report),
    };
    let leftover_startup_n = leftover_startup.materials.len();
    let mut material_seed = material_seed;
    material_seed.absorb_missing_reals(leftover_startup);
    report.push(format!(
        "t5 leftover startup gfx absorb_missing_reals: donor={leftover_startup_n} host now {}",
        material_seed.materials.len()
    ));
    let image = match opened {
        Ok(image) => image,
        Err(error) => {
            report.push(format!(
                "t5 weapon common: open {}: {error}",
                donor.display()
            ));
            let mut empty = T5WeaponCommon::empty(material_seed, report);
            empty.stats_tables.0 = leftover_stats;
            return empty;
        }
    };
    let mut census = lane(image.game).load_common_mp(&donor, &image, progress, true, material_seed);
    let images = hold_image_plan("T5 common_mp", census.pending_images.take(), job)
        .map(|held| held.enqueue(progress));
    report.extend(census.report);
    report.push(format!(
        "t5 weapon common: path={} weapons={} fpv={} world_guns={} materials={} xanims={} fx={}",
        donor.display(),
        census.weapons.len(),
        census.fpv.len(),
        census.world_weapons.len(),
        census.material_population.materials.len(),
        census.xanims.len(),
        census.fx.len(),
    ));
    T5WeaponCommon {
        weapons: census.weapons,
        fpv: census.fpv,
        world_guns: census.world_weapons,
        material_seed: census.material_population,
        xanims: census.xanims,
        fx: census.fx,
        projectiles: census.projectile_meshes,
        teamsets: census.teamsets,
        images,
        stats_tables: (leftover_stats, census.cac_tables),
        report,
    }
}

async fn walk_startup_material_zones(
    map_path: Option<&PathBuf>,
    progress: &LoadProgress,
) -> (
    MaterialCatalog,
    Vec<String>,
    Vec<crate::CapturedStringTable>,
    Vec<crate::CapturedLightDef>,
) {
    const STARTUP_ZONES: [&str; 3] = ["code_post_gfx_mp", "localized_code_post_gfx_mp", "patch_mp"];
    let Some(map_path) = map_path else {
        return (
            MaterialCatalog::default(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
    };
    let games = games_root_from_env().ok();

    let opened = STARTUP_ZONES
        .map(|zone| {
            let games = games.clone();
            let map_path = map_path.clone();
            let progress = progress.clone();
            load_pool().spawn(async move {
                let found = match &games {
                    Some(root) => find_runtime_zone(root, &map_path, zone),
                    None => find_zone_for_tree(&map_path, zone),
                };
                found.and_then(|found| {
                    let stage = progress.begin_scoped(StageId::CommonAssets, zone, None);
                    let image = open_zone_shared(&found.path)
                        .map(|image| (found.path, image))
                        .map_err(|error| error.to_string());
                    if let Ok((_, image)) = &image {
                        stage.set_bytes(image.bytes.len() as u64);
                    }
                    stage.finish_from(&image);
                    image
                })
            })
        })
        .into_iter();
    let mut seed = MaterialCatalog::default();
    let mut report = Vec::new();
    let mut reuse_mat = 0usize;
    let mut reuse_img = 0usize;
    let mut zones_ok = 0usize;
    let mut stats = Vec::new();
    let mut light_defs = Vec::new();
    for (zone, task) in STARTUP_ZONES.into_iter().zip(opened) {
        match task.await {
            Ok((path, image)) => {
                let envelope = peek_zone_version(&path)
                    .map(|v| format!("{v:#x}"))
                    .unwrap_or_else(|| "unreadable".into());
                let pop = lane(image.game).load_material_population(&path, &image, progress, seed);
                report.extend(pop.report);
                reuse_mat = reuse_mat.saturating_add(pop.materials.link_reused_materials);
                reuse_img = reuse_img.saturating_add(pop.materials.link_reused_images);
                report.push(format!(
                    "material generation startup: {zone} {} materials images={} decoded={} envelope={envelope} path={}",
                    pop.materials.materials.len(),
                    pop.materials.images.len(),
                    pop.materials.image_memory().decoded_images,
                    path.display()
                ));
                seed = pop.materials;
                light_defs.extend(pop.light_defs);
                if zone == "code_post_gfx_mp" {
                    stats = pop.cac_tables;
                }
                zones_ok += 1;
            }
            Err(gap) => report.push(format!("material generation startup gap: {zone}: {gap}")),
        }
    }
    let startup_decoded = seed.image_memory().decoded_images;
    report.push(format!(
        "startup material decode: zones={zones_ok} decoded_images={startup_decoded} (0 is IWD deferred to absorb merge; not three parallel common_mp FPV decodes)",
    ));
    report.push(format!(
        "startup material walk: zones={zones_ok} fpv=0 weapons=0 (materials-only sink; not load_common_mp)",
    ));
    report.push(format!(
        "s2 startup walk: reuse_mat={reuse_mat} reuse_img={reuse_img} pool_mat={} pool_img={}",
        seed.materials.len(),
        seed.images.len(),
    ));
    (seed, report, stats, light_defs)
}

fn load_localized_strings_beside(
    zone_ff: &std::path::Path,
    report: &mut Vec<String>,
    stage: &crate::progress::StageHandle,
) -> LocalizeCatalog {
    let mut catalog = LocalizeCatalog::default();
    let root = match games_root_from_env() {
        Ok(root) => root,
        Err(error) => {
            report.push(format!("localize gap: IW4L_GAMES unset ({error})"));
            return catalog;
        }
    };
    let lanes: [(crate::AssetNamespace, u32, &[&str]); 3] = [
        (
            crate::AssetNamespace::Iw4,
            fastfile_iw4::ZONE_VERSION_PC,
            MP_LOCALIZED_ZONES,
        ),
        (
            crate::AssetNamespace::T5,
            fastfile_t5::ZONE_VERSION_PC,
            &["code_post_gfx_mp", "common_mp", "ui_mp"],
        ),
        (
            crate::AssetNamespace::Iw5,
            fastfile_iw5::ZONE_VERSION_PC,
            MP_LOCALIZED_ZONES,
        ),
    ];
    let runtime_language = find_runtime_common_mp(&root, zone_ff)
        .ok()
        .and_then(|zone| zone.path.parent()?.file_name()?.to_str().map(str::to_owned));
    let mut plan = Vec::new();
    for (namespace, version, names) in lanes {
        let found: Vec<_> = if namespace == crate::AssetNamespace::T5 {
            match find_zone_file_version(&root, "common_mp", version).and_then(|zone| {
                asset_transport::discover::find_t5_localized_zones(
                    &zone.path,
                    runtime_language.as_deref(),
                )
            }) {
                Ok(zones) => zones,
                Err(error) => {
                    report.push(format!("localize gap: {error}"));
                    Vec::new()
                }
            }
        } else {
            names
                .iter()
                .filter_map(|name| {
                    if namespace == crate::AssetNamespace::Iw4 {
                        find_runtime_zone(&root, zone_ff, name)
                    } else {
                        find_zone_file_version(&root, name, version)
                    }
                    .ok()
                })
                .collect()
        };
        plan.extend(found.into_iter().map(|found| (namespace, found)));
    }
    // The plan only closes once every lane has been searched: a zone that is
    // not there is not part of the volume, and counting it would report work
    // nobody is going to do.
    stage.set_total(plan.len() as u64);
    for (namespace, found) in plan {
        let name = &found.zone_name;
        match load_localize_catalog_in_lane(&found.path) {
            Ok(part) if part.is_empty() => {}
            Ok(part) => {
                report.push(format!(
                    "localize: {} {name} {} strings",
                    namespace.as_str(),
                    part.len()
                ));
                catalog.absorb_in_namespace(namespace, part);
            }
            Err(error) => report.push(format!(
                "localize gap: {} {name} failed: {error}",
                namespace.as_str()
            )),
        }
        // A zone that failed to open is still a unit this pass has handled.
        stage.advance(1);
    }
    report.push(format!(
        "localize: {} strings for on-screen text",
        catalog.len()
    ));
    catalog
}

/// Close a zone-open stage with the weight of what it read.
///
/// A zone open has no units to count — it is one read — but it does have a
/// size, and that size is what the rest of the load is built out of. Recording
/// it here means every opener says it the same way.
fn finish_zone_open<E>(
    stage: StageHandle,
    opened: &Result<std::sync::Arc<asset_transport::zone::ZoneImage>, E>,
) {
    if let Ok(image) = opened {
        stage.set_bytes(image.bytes.len() as u64);
    }
    stage.finish_from(opened);
}
