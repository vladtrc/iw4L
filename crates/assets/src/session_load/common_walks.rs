use super::*;

#[derive(Clone, Debug, Default)]
pub struct MatchMaterialSeed {
    pub catalog: MaterialCatalog,
}

pub fn load_match_material_seed(
    progress: &LoadProgress,
) -> Result<(MatchMaterialSeed, Vec<String>), String> {
    let root = games_root_from_env()?;
    let runtime = find_zone_file_version(&root, "common_mp", fastfile_iw4::ZONE_VERSION_PC)?;

    let (catalog, mut report, _, _, _) = bevy::tasks::futures_lite::future::block_on(
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

pub(super) fn walk_population_file(
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
    Vec<asset_game::CapturedStringTable>,
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
    const ZONES: [&str; 3] = ["code_post_gfx_mp", "localized_code_post_gfx_mp", "patch_mp"];
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

pub(super) fn resolve_foreign_material_donor(
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

pub(super) fn walk_foreign_material_common(
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

pub(super) fn merge_image_batch(
    global: &mut asset_material::MaterialDefinitions,
    label: &str,
    batch: asset_material::material_images::DecodedImageBatch,
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
pub(super) struct PendingImages {
    pub(super) job: load_jobs::Job,
    pub(super) task: bevy::tasks::Task<(
        &'static str,
        asset_material::material_images::DecodedImageBatch,
    )>,
}

impl PendingImages {
    pub(super) async fn join(
        self,
    ) -> (
        &'static str,
        asset_material::material_images::DecodedImageBatch,
    ) {
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
pub(super) struct HeldImagePlan {
    pub(super) label: &'static str,
    pub(super) plan: ImageDemandPlan,
    pub(super) job: load_jobs::Job,
}

/// Take a finished walk's plan, if it wants anything at all.
pub(super) fn hold_image_plan(
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
    pub(super) fn enqueue(self, progress: &LoadProgress) -> PendingImages {
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
    pub(super) fn prune_then_enqueue(
        mut self,
        catalog: &mut asset_material::MaterialDefinitions,
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

pub(super) enum ForeignCommonWork {
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
pub(super) struct Iw5WeaponBundle {
    pub(super) fx: FxCatalog,
    pub(super) weapons: WeaponBuild,
    pub(super) fpv: FpvMeshBuild,
    pub(super) world_guns: WorldWeaponBuild,
    pub(super) xanims: XAnimBuild,

    pub(super) materials: MaterialCatalog,
    pub(super) stats_tables: Vec<asset_game::CapturedStringTable>,
    pub(super) scene_models: asset_world::MapXModelSceneCatalog,
    pub(super) shared_surfaces: asset_model::SharedXModelSurfaces,
}

pub(super) fn resolve_iw5_weapon_donor(
    runtime_common: Option<&Path>,
    report: &mut Vec<String>,
) -> Option<PathBuf> {
    if runtime_common.is_some_and(|path| {
        asset_transport::t6_content::T6ContentMode::for_path(path)
            == asset_transport::t6_content::T6ContentMode::Zombies
    }) {
        return None;
    }
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

pub(super) fn walk_iw5_weapon_bundle(
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
            fx: census.fx,
            weapons: census.weapons,
            fpv: census.fpv,
            world_guns: census.world_weapons,
            xanims: census.xanims,
            materials: census.material_population,
            stats_tables: census.cac_tables,
            scene_models: census.scene_models,
            shared_surfaces: census.shared_surfaces,
        },
        pending_images,
        report,
    )
}

pub(super) fn walk_shared_iw5_common(
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
            fx: census.fx,
            weapons: census.weapons,
            fpv: census.fpv,
            world_guns: census.world_weapons,
            xanims: census.xanims,
            materials: MaterialCatalog::default(),
            stats_tables: census.cac_tables,
            scene_models: census.scene_models,
            shared_surfaces: census.shared_surfaces,
        },
        pending_images,
        report,
    )
}

pub(super) enum T5CommonPrep {
    Skip(Vec<String>),
    Ready {
        donor: PathBuf,
        opened: Result<std::sync::Arc<asset_transport::ZoneImage>, String>,
        leftover_startup: MaterialCatalog,
        leftover_stats: Vec<asset_game::CapturedStringTable>,
        report: Vec<String>,
    },
}

pub(super) fn t5_weapon_common_prep(
    runtime_common: Option<&Path>,
    progress: &LoadProgress,
) -> T5CommonPrep {
    let mut report = Vec::new();
    if runtime_common.is_some_and(|path| {
        asset_transport::t6_content::T6ContentMode::for_path(path)
            == asset_transport::t6_content::T6ContentMode::Zombies
    }) {
        return T5CommonPrep::Skip(report);
    }
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

pub(super) struct T5WeaponCommon {
    pub(super) weapons: WeaponBuild,
    pub(super) fpv: FpvMeshBuild,
    pub(super) world_guns: WorldWeaponBuild,
    pub(super) material_seed: MaterialCatalog,
    pub(super) xanims: XAnimBuild,
    pub(super) fx: FxCatalog,
    pub(super) impact_fx: Option<asset_game::OwnedFxImpactTable>,
    pub(super) projectiles: asset_model::ProjectileMeshBuild,
    pub(super) teamsets: std::collections::HashMap<String, asset_game::MapTeamSettings>,
    pub(super) scene_models: asset_world::MapXModelSceneCatalog,
    pub(super) images: Option<PendingImages>,
    pub(super) stats_tables: (
        Vec<asset_game::CapturedStringTable>,
        Vec<asset_game::CapturedStringTable>,
    ),
    pub(super) report: Vec<String>,
}

impl T5WeaponCommon {
    pub(super) fn empty(material_seed: MaterialCatalog, report: Vec<String>) -> Self {
        Self {
            weapons: asset_game::WeaponBuild::default(),
            fpv: FpvMeshBuild::default(),
            world_guns: WorldWeaponBuild::default(),
            material_seed,
            xanims: XAnimBuild::default(),
            fx: FxCatalog::default(),
            impact_fx: None,
            projectiles: asset_model::ProjectileMeshBuild::default(),
            teamsets: Default::default(),
            scene_models: Default::default(),
            images: None,
            stats_tables: (Vec::new(), Vec::new()),
            report,
        }
    }
}

pub(super) fn walk_t5_weapon_common(
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
        impact_fx: census.impact_fx,
        projectiles: census.projectile_meshes,
        teamsets: census.teamsets,
        scene_models: census.scene_models,
        images,
        stats_tables: (leftover_stats, census.cac_tables),
        report,
    }
}

fn t6_class_tables(
    root: &asset_transport::GamesRoot,
    report: &mut Vec<String>,
) -> Vec<asset_game::CapturedStringTable> {
    let zone = match find_zone_file_version(root, "patch_mp", fastfile_t6::ZONE_VERSION_PC) {
        Ok(zone) => zone,
        Err(error) => {
            report.push(format!("t6 class tables: {error}"));
            return Vec::new();
        }
    };
    let image = match asset_transport::open_t6_zone(&zone.path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 class tables: {error}"));
            return Vec::new();
        }
    };
    let schema = match fastfile_t6::schema::parse() {
        Ok(schema) => schema,
        Err(error) => {
            report.push(format!("t6 class tables: load plan {error:?}"));
            return Vec::new();
        }
    };
    let (load, walked) = fastfile_t6::load_zone(&schema, &image.bytes, |_, _| true);
    if let Err(error) = walked {
        report.push(format!("t6 class tables: patch_mp walk stopped: {error:?}"));
    }
    let tables: Vec<_> = load
        .assets
        .iter()
        .filter_map(|asset| asset_game::capture_t6_string_table(&load, asset))
        .filter(|table| {
            asset_game::is_stats_table_name(&table.name)
                || table.name.eq_ignore_ascii_case("mp/attachmentTable.csv")
                || table.name.eq_ignore_ascii_case("mp/mapstable.csv")
        })
        .collect();
    report.push(format!(
        "t6 class tables: {} from {}",
        tables.len(),
        zone.path.display()
    ));
    tables
}

pub(super) fn walk_t6_weapon_bundle(
    runtime_common: Option<&Path>,
    progress: &LoadProgress,
) -> (
    WeaponBuild,
    Option<Box<dyn crate::lane::CommonFamilyCompiler>>,
    Vec<asset_game::CapturedStringTable>,
    Vec<String>,
    Vec<(String, asset_material::material_images::ZoneUiImage)>,
) {
    let mut report = Vec::new();
    if runtime_common.is_some_and(|path| {
        asset_transport::t6_content::T6ContentMode::for_path(path)
            == asset_transport::t6_content::T6ContentMode::Zombies
    }) {
        return (WeaponBuild::default(), None, Vec::new(), report, Vec::new());
    }
    let root = match games_root_from_env() {
        Ok(root) => root,
        Err(error) => {
            report.push(format!("t6 weapons: {error}"));
            return (WeaponBuild::default(), None, Vec::new(), report, Vec::new());
        }
    };
    let donor = match find_common_mp_for_envelope(&root, fastfile_t6::ZONE_VERSION_PC) {
        Ok(donor) => donor,
        Err(error) => {
            report.push(format!("t6 weapons: {error}"));
            return (WeaponBuild::default(), None, Vec::new(), report, Vec::new());
        }
    };
    let tables = t6_class_tables(&root, &mut report);
    if runtime_common == Some(donor.path.as_path()) {
        report.push(format!(
            "t6 weapons: runtime common already owns {}; donor walk skipped",
            donor.path.display()
        ));
        return (WeaponBuild::default(), None, tables, report, Vec::new());
    }
    let stage = progress.begin_scoped(StageId::CommonAssets, "t6_weapons", None);
    let opened = open_zone_shared(&donor.path).map_err(|error| error.to_string());
    stage.finish_from(&opened);
    let image = match opened {
        Ok(image) => image,
        Err(error) => {
            report.push(format!(
                "t6 weapons: open {}: {error}",
                donor.path.display()
            ));
            return (WeaponBuild::default(), None, Vec::new(), report, Vec::new());
        }
    };
    let census = lane(image.game).load_common_mp(
        &donor.path,
        &image,
        progress,
        false,
        MaterialCatalog::default(),
    );
    report.extend(census.report);
    let mut weapons = census.weapons;
    weapons.apply_stats_tables(&tables);
    (
        weapons,
        census.preparation,
        tables,
        report,
        census.ui_images,
    )
}

pub(super) async fn walk_startup_material_zones(
    map_path: Option<&PathBuf>,
    progress: &LoadProgress,
) -> (
    MaterialCatalog,
    Vec<String>,
    Vec<asset_game::CapturedStringTable>,
    Vec<asset_world::CapturedLightDef>,
    crate::ScriptSources,
) {
    let Some(map_path) = map_path else {
        return (
            MaterialCatalog::default(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            crate::ScriptSources::default(),
        );
    };
    let startup_zones =
        if asset_transport::zone_game_for_path(map_path) == Some(asset_core::ZoneGame::T6) {
            asset_transport::t6_content::T6ContentMode::for_path(map_path).startup()
        } else {
            asset_transport::t6_content::T6ContentMode::Multiplayer.startup()
        };
    let games = games_root_from_env().ok();

    let opened = startup_zones
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
    let mut scripts = crate::ScriptSources::default();
    for (zone, task) in startup_zones.into_iter().zip(opened) {
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
                scripts.overlay(pop.scripts);
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
    (seed, report, stats, light_defs, scripts)
}

pub(super) fn load_localized_strings_beside(
    zone_ff: &std::path::Path,
    report: &mut Vec<String>,
    stage: &asset_transport::progress::StageHandle,
) -> LocalizeCatalog {
    let mut catalog = LocalizeCatalog::default();
    let root = match games_root_from_env() {
        Ok(root) => root,
        Err(error) => {
            report.push(format!("localize gap: IW4L_GAMES unset ({error})"));
            return catalog;
        }
    };
    let lanes: [(asset_core::AssetNamespace, u32, &[&str]); 4] = [
        (
            asset_core::AssetNamespace::Iw4,
            fastfile_iw4::ZONE_VERSION_PC,
            MP_LOCALIZED_ZONES,
        ),
        (
            asset_core::AssetNamespace::T5,
            fastfile_t5::ZONE_VERSION_PC,
            &["code_post_gfx_mp", "common_mp", "ui_mp"],
        ),
        (
            asset_core::AssetNamespace::Iw5,
            fastfile_iw5::ZONE_VERSION_PC,
            MP_LOCALIZED_ZONES,
        ),
        (
            asset_core::AssetNamespace::T6,
            fastfile_t6::ZONE_VERSION_PC,
            &[],
        ),
    ];
    let runtime_language = find_runtime_common_mp(&root, zone_ff)
        .ok()
        .and_then(|zone| zone.path.parent()?.file_name()?.to_str().map(str::to_owned));
    let mut plan = Vec::new();
    for (namespace, version, names) in lanes {
        let found: Vec<_> = if matches!(
            namespace,
            asset_core::AssetNamespace::T5 | asset_core::AssetNamespace::T6
        ) {
            let common = if namespace == asset_core::AssetNamespace::T6 {
                asset_transport::t6_content::T6ContentMode::for_path(zone_ff).common()
            } else {
                "common_mp"
            };
            match find_zone_file_version(&root, common, version).and_then(|zone| {
                let language = runtime_language.as_deref();
                if namespace == asset_core::AssetNamespace::T6 {
                    let anchor = if asset_transport::zone_game_for_path(zone_ff)
                        == Some(asset_core::ZoneGame::T6)
                    {
                        zone_ff
                    } else {
                        zone.path.as_path()
                    };
                    asset_transport::discover::find_t6_localized_zones(anchor, language)
                } else {
                    asset_transport::discover::find_t5_localized_zones(&zone.path, language)
                }
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
                    if namespace == asset_core::AssetNamespace::Iw4 {
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
pub(super) fn finish_zone_open<E>(
    stage: StageHandle,
    opened: &Result<std::sync::Arc<asset_transport::zone::ZoneImage>, E>,
) {
    if let Ok(image) = opened {
        stage.set_bytes(image.bytes.len() as u64);
    }
    stage.finish_from(opened);
}

#[derive(Default)]
pub(super) struct ZombieCommons {
    pub(super) xanims: XAnimBuild,
    pub(super) weapons: WeaponBuild,
    pub(super) fpv: FpvMeshBuild,
    pub(super) world_weapons: WorldWeaponBuild,
    pub(super) projectiles: asset_model::ProjectileMeshBuild,
    /// Singleplayer and zombie scripts, lowest priority first; the map and its
    /// patch go on top.
    pub(super) scripts: Option<crate::ScriptSources>,
    pub(super) map_patch_scripts: crate::ScriptSources,
    /// On-screen text of the singleplayer, zombie and map zones.
    pub(super) strings: LocalizeCatalog,
    /// Black Ops' own HUD menus and fonts.
    pub(super) hud_menus: Option<asset_game::MenuCatalog>,
    /// Each zone's models with the materials they index, for the actor
    /// bodies and heads scripts put on spawned zombies.
    pub(super) scene_models: Vec<(
        asset_world::MapXModelSceneCatalog,
        asset_material::MaterialCatalog,
    )>,
    /// The 2D images the zombie scripts and Black Ops' HUD name.
    pub(super) hud_images: Vec<(String, asset_material::material_images::ZoneUiImage)>,
    pub(super) report: Vec<String>,
}

/// T5 zombie maps run on the singleplayer script base and keep their actor
/// animations, weapons and shared models in `common_zombie` and its patch.
pub(super) fn walk_zombie_commons(zone_ff: &Path, progress: &LoadProgress) -> ZombieCommons {
    let mut commons = ZombieCommons::default();
    let is_zombie_map = zone_ff
        .file_stem()
        .is_some_and(|stem| stem.to_string_lossy().starts_with("zombie"));
    if !is_zombie_map
        || asset_transport::zone_game_for_path(zone_ff) != Some(asset_core::ZoneGame::T5)
    {
        return commons;
    }
    let stem = zone_ff
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let map_patch = format!("{stem}_patch");
    let mut scripts = crate::ScriptSources::default();
    let mut script_names = std::collections::BTreeSet::new();
    for zone in [
        "code_post_gfx",
        "common",
        "common_zombie",
        "common_zombie_patch",
        stem.as_str(),
        map_patch.as_str(),
    ] {
        let found = match asset_transport::find_zone_for_tree(zone_ff, zone) {
            Ok(found) => found,
            Err(error) => {
                commons
                    .report
                    .push(format!("zombie common {zone}: {error}"));
                continue;
            }
        };
        let stage = progress.begin_scoped(StageId::MapAssets, "zombie common", None);
        let opened = open_zone_shared(&found.path);
        stage.finish_from(&opened);
        let image = match opened {
            Ok(image) => image,
            Err(error) => {
                commons
                    .report
                    .push(format!("zombie common {zone}: open: {error}"));
                continue;
            }
        };
        let census = lane(image.game).load_common_mp(
            &found.path,
            &image,
            progress,
            false,
            MaterialCatalog::default(),
        );
        let xanims = census.xanims.len();
        commons.xanims.absorb(census.xanims);
        commons.weapons.absorb_overriding(census.weapons.clone());
        commons.fpv.absorb(census.fpv.clone());
        commons.world_weapons.absorb(census.world_weapons.clone());
        commons.projectiles.absorb(census.projectile_meshes.clone());
        commons.report.push(format!(
            "zombie common {zone}: xanims={xanims} weapons={} scene_models={} scripts={}",
            census.weapons.len(),
            census.scene_models.len(),
            census.scripts.len(),
        ));
        script_names.extend(census.scripts.asset_names());
        commons
            .scene_models
            .push((census.scene_models, census.material_population));
        if zone == map_patch {
            commons.map_patch_scripts = census.scripts;
        } else if zone != stem {
            scripts.overlay(census.scripts);
        }
    }
    commons.scripts = Some(scripts);
    let mut hud_menus = asset_game::MenuCatalog::default();
    let mut menu_zones = Vec::new();
    for zone in ["code_post_gfx", "patch"] {
        match asset_transport::find_zone_for_tree(zone_ff, zone) {
            Ok(found) => menu_zones.push((zone.to_owned(), found.path)),
            Err(error) => commons.report.push(format!("t5 menus {zone}: {error}")),
        }
    }
    match asset_transport::discover::find_t5_localized_zone(zone_ff, None, "code_post_gfx") {
        Ok(Some(found)) => menu_zones.push((found.zone_name.clone(), found.path)),
        Ok(None) => commons
            .report
            .push("t5 menus: no localized code_post_gfx (fonts)".into()),
        Err(error) => commons.report.push(format!("t5 menus fonts: {error}")),
    }
    for (zone, path) in &menu_zones {
        match asset_game::load_t5_menu_catalog(path) {
            Ok(part) => {
                commons.report.push(format!(
                    "t5 menus {zone}: {} menus, {} lists, {} fonts",
                    part.menus.len(),
                    part.lists.len(),
                    part.fonts.len()
                ));
                hud_menus.absorb(part);
            }
            Err(error) => commons.report.push(format!("t5 menus {zone}: {error}")),
        }
        if zone == "patch"
            && let Ok(image) = open_zone_shared(path)
        {
            let population = lane(image.game).load_material_population(
                path,
                &image,
                progress,
                MaterialCatalog::default(),
            );
            commons
                .scene_models
                .push((Default::default(), population.materials));
        }
    }
    let mut hud_material_names = std::collections::BTreeSet::new();
    for list in asset_game::T5_HUD_MENU_LISTS {
        for menu in hud_menus.list_menus.get(*list).into_iter().flatten() {
            let Some(menu) = hud_menus.get(menu) else {
                continue;
            };
            for material in std::iter::once(&menu.window_background)
                .chain(menu.items.iter().map(|item| &item.background))
            {
                if let Some(name) = material.strip_prefix("t5:material/") {
                    hud_material_names.insert(name.to_ascii_lowercase());
                }
            }
        }
    }
    for font in hud_menus.fonts.values() {
        for material in [&font.material, &font.glow_material] {
            if !material.is_empty() {
                hud_material_names.insert(material.to_ascii_lowercase());
            }
        }
    }
    commons.report.push(format!(
        "t5 hud menus: {} menus, {} fonts, {} materials named",
        hud_menus.menus.len(),
        hud_menus.fonts.len(),
        hud_material_names.len()
    ));
    commons.hud_menus = Some(hud_menus);
    for stem in ["code_post_gfx", "common", "common_zombie", stem.as_str()] {
        let found = asset_transport::discover::find_t5_localized_zone(zone_ff, None, stem);
        // Localized zones also hold materials the zombie bodies draw with.
        if let Ok(Some(zone)) = &found
            && let Ok(image) = open_zone_shared(&zone.path)
        {
            let census = lane(image.game).load_common_mp(
                &zone.path,
                &image,
                progress,
                false,
                MaterialCatalog::default(),
            );
            commons
                .scene_models
                .push((census.scene_models, census.material_population));
        }
        match found.map(|found| found.map(|zone| load_localize_catalog_in_lane(&zone.path))) {
            Ok(Some(Ok(part))) => {
                commons
                    .report
                    .push(format!("zombie localize {stem}: {} strings", part.len()));
                commons.strings.absorb(part);
            }
            Ok(Some(Err(error))) => commons
                .report
                .push(format!("zombie localize {stem}: {error}")),
            Ok(None) => commons
                .report
                .push(format!("zombie localize {stem}: no localized zone")),
            Err(error) => commons
                .report
                .push(format!("zombie localize {stem}: {error}")),
        }
    }
    let main = asset_transport::game_root_for_zone(zone_ff)
        .ok()
        .map(|root| root.join("main"));
    let hud = zombie_hud_images(
        &commons.scene_models,
        &script_names,
        &hud_material_names,
        main.as_deref(),
    );
    commons
        .report
        .push(format!("zombie hud images: {}", hud.len()));
    commons.hud_images = hud;
    commons
}

/// The 2D materials zombie scripts and Black Ops' HUD menus and fonts name
/// (round chalk, perk and power-up icons, the weapon info frame), decoded for
/// the HUD from the zone images they sample.
fn zombie_hud_images(
    zones: &[(asset_world::MapXModelSceneCatalog, MaterialCatalog)],
    names: &std::collections::BTreeSet<String>,
    hud_names: &std::collections::BTreeSet<String>,
    main: Option<&Path>,
) -> Vec<(String, asset_material::ZoneUiImage)> {
    let mut images = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (_, population) in zones {
        for material in &population.materials {
            let name = material.name.as_str().to_ascii_lowercase();
            // Scripts build some names (`"hud_chalk_" + round`), so HUD-named
            // materials count even when no literal names them.
            let hud_named = ["hud_", "specialty_", "zom_"]
                .iter()
                .any(|prefix| name.starts_with(prefix));
            if !material.name.is_real()
                || !(hud_named || names.contains(&name) || hud_names.contains(&name))
                || seen.contains(&name)
            {
                continue;
            }
            let Some(image) = material
                .textures
                .iter()
                .find_map(|texture| population.images.get(texture.image?))
            else {
                continue;
            };
            // Images without an in-zone payload stream from the IWDs.
            let decoded = if image.payload.is_empty() {
                let Some(main) = main else {
                    continue;
                };
                asset_material::decode_ui_image_from_main(main, image.name.as_str())
                    .and_then(|found| found.ok_or_else(|| "not in the IWDs".to_owned()))
            } else {
                asset_material::decode_zone_image_rgba(
                    u32::from(image.width),
                    u32::from(image.height),
                    image.format,
                    &image.payload,
                )
            };
            match decoded {
                Ok((width, height, rgba)) => {
                    seen.insert(name.clone());
                    images.push((
                        name,
                        asset_material::ZoneUiImage {
                            iwi: std::sync::Arc::new([]),
                            state: None,
                            rgba: Some((width, height, std::sync::Arc::new(rgba))),
                        },
                    ));
                }
                Err(error) => diag::info!(Zone, "zombie hud image {name}: {error}"),
            }
        }
    }
    images
}
