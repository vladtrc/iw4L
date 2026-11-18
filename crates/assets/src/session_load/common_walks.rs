use super::*;

#[derive(Clone, Debug, Default)]
pub struct MatchMaterialSeed {
    pub catalog: MaterialCatalog,
}

pub fn load_match_material_seed(
    progress: &LoadProgress,
) -> Result<(MatchMaterialSeed, Vec<String>), String> {
    let root = games_root()?;
    let runtime = find_zone_file_version(&root, "common_mp", fastfile_iw4::ZONE_VERSION_PC)?;

    let (catalog, mut report, _, _, _, _) =
        bevy::tasks::futures_lite::future::block_on(walk_startup_material_zones(
            Some(&runtime.path),
            progress,
            asset_core::RuntimeProfile::Multiplayer,
        ));
    let catalog = walk_supplemental_material_seed(
        Some(runtime.path.as_path()),
        progress,
        catalog,
        &mut report,
    );
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
    let runtime = games_root()
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
    let promoted = catalog.promote_fallback_tables();
    let feature_aliases = catalog.absorb_feature_variant_donors();
    let stub_routed = catalog.reroute_stub_materials();
    report.push(format!(
        "material route (census): absorbed_techsets={absorbed} feature_variant_aliases={feature_aliases} fallback_tables_promoted={promoted} stub_routed={stub_routed} unrouted={}",
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
    let (_, population) = walk_population_file(path, progress, seed);
    (population.materials, population.report)
}

pub(super) fn walk_population_file(
    path: &Path,
    progress: &LoadProgress,
    seed: MaterialCatalog,
) -> (Option<asset_core::FamilyId>, crate::MaterialPopulation) {
    let zone_name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "zone".into());
    let stage = progress.begin_scoped(StageId::CommonAssets, zone_name.clone(), None);
    let opened = open_zone_shared(path);
    finish_zone_open(stage, &opened);
    match opened {
        Ok(image) => (
            Some(image.game),
            match lane(image.game) {
                Some(selected) => selected.load_material_population(path, &image, progress, seed),
                None => crate::MaterialPopulation {
                    materials: seed,
                    report: vec![format!("zone lane not installed: {}", image.game.as_str())],
                    ..Default::default()
                },
            },
        ),
        Err(error) => (
            None,
            crate::MaterialPopulation {
                materials: seed,
                report: vec![format!("match materials: open {}: {error}", path.display())],
                ..Default::default()
            },
        ),
    }
}

fn walk_supplemental_material_seed(
    runtime_common: Option<&Path>,
    progress: &LoadProgress,
    mut seed: MaterialCatalog,
    report: &mut Vec<String>,
) -> MaterialCatalog {
    let root = match games_root() {
        Ok(root) => root,
        Err(error) => {
            report.push(format!("supplemental material seed: {error}"));
            return seed;
        }
    };
    for (family, plan) in crate::plan_common_material_seeds(&root, runtime_common, report) {
        report.extend(plan.report);
        if plan.common.is_none() && plan.startup.is_empty() {
            continue;
        }
        let (startup, _, startup_report) = walk_planned_startup_materials(&plan.startup, progress);
        report.extend(startup_report);
        let count = startup.materials.len();
        seed.absorb_missing_reals(startup);
        report.push(format!(
            "{} startup material seed: donor={count} host={}",
            family.as_str(),
            seed.materials.len()
        ));
        if let Some(common) = plan.common {
            let (catalog, walked) = walk_material_file(&common, progress, seed);
            report.extend(walked);
            seed = catalog;
        }
    }
    seed
}

fn walk_planned_startup_materials(
    sources: &[crate::CommonStartupSource],
    progress: &LoadProgress,
) -> (MaterialCatalog, Vec<CommonTables>, Vec<String>) {
    let mut seed = MaterialCatalog::default();
    let mut tables = Vec::new();
    let mut report = Vec::new();
    for source in sources {
        let (owner, population) = walk_population_file(&source.path, progress, seed);
        report.extend(population.report);
        if source.collect_tables {
            tables.push(CommonTables::capture(owner, population.cac_tables));
        }
        seed = population.materials;
    }
    (seed, tables, report)
}

fn walk_foreign_material_population(
    zone_ff: Option<&Path>,
    runtime_common: Option<&Path>,
    progress: &LoadProgress,
) -> (MaterialCatalog, Vec<String>) {
    let mut report = Vec::new();
    let Some(donor) = resolve_foreign_material_donor(
        zone_ff,
        runtime_common,
        &mut report,
        frame::AssetFamilies::all_installed(),
    ) else {
        return (MaterialCatalog::default(), report);
    };
    let (materials, walked) = walk_material_file(&donor, progress, MaterialCatalog::default());
    report.extend(walked);
    report.push(format!(
        "foreign data common: path={} materials={} techsets={} (weapons discarded)",
        donor.display(),
        materials.materials.len(),
        materials.technique_set_facts().len(),
    ));
    (materials, report)
}

pub(super) fn resolve_foreign_material_donor(
    zone_ff: Option<&Path>,
    runtime_common: Option<&Path>,
    report: &mut Vec<String>,
    enabled: frame::AssetFamilies,
) -> Option<PathBuf> {
    crate::plan_common_material_donor(
        zone_ff?,
        runtime_common,
        |family| enabled.enabled(family.as_str()),
        report,
    )
}

pub(super) fn capture_common_zone(
    donor: &Path,
    expected: Option<asset_core::FamilyId>,
    stage_label: &str,
    report_label: &str,
    progress: &LoadProgress,
    report: &mut Vec<String>,
) -> Option<(asset_core::FamilyId, crate::CommonCensus)> {
    if progress.is_canceled() {
        report.push(format!("{report_label}: canceled before the walk started"));
        return None;
    }
    let stage = progress.begin_scoped(StageId::CommonAssets, stage_label, None);
    let opened = open_zone_shared(donor);
    finish_zone_open(stage, &opened);
    match opened {
        Ok(image) => {
            if expected.is_some_and(|family| family != image.game) {
                report.push(format!(
                    "{report_label}: source owner mismatch at {}: captured {}",
                    donor.display(),
                    image.game.as_str()
                ));
                return None;
            }
            let Some(selected) = lane(image.game) else {
                report.push(format!(
                    "{report_label}: zone lane not installed: {}",
                    image.game.as_str()
                ));
                return None;
            };
            Some((
                image.game,
                selected.load_common(
                    donor,
                    &image,
                    asset_core::RuntimeProfile::Multiplayer,
                    progress,
                    true,
                    MaterialCatalog::default(),
                ),
            ))
        }
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

pub(super) struct SeededSourcePrep {
    family: asset_core::FamilyId,
    donor: Option<PathBuf>,
    opened: Option<Result<std::sync::Arc<asset_transport::ZoneImage>, String>>,
    startup: MaterialCatalog,
    tables: Vec<CommonTables>,
}

#[derive(Default)]
pub(super) struct SeededCommonPrep {
    sources: Vec<SeededSourcePrep>,
    report: Vec<String>,
}

pub(super) fn prepare_seeded_common(
    runtime: Option<&Path>,
    progress: &LoadProgress,
    enabled: frame::AssetFamilies,
) -> SeededCommonPrep {
    let mut prep = SeededCommonPrep {
        sources: Vec::new(),
        report: Vec::new(),
    };
    let root = match games_root() {
        Ok(root) => root,
        Err(error) => {
            prep.report.push(error);
            return prep;
        }
    };
    for (family, policy) in crate::common_source_families() {
        if !enabled.enabled(family.as_str()) {
            continue;
        }
        let Some(planner) = policy.material_seed else {
            continue;
        };
        let plan = match planner(&root, runtime) {
            Ok(plan) => plan,
            Err(error) => {
                prep.report
                    .push(format!("{} seeded common: {error}", family.as_str()));
                continue;
            }
        };
        prep.report.extend(plan.report);
        let donor = plan.common;
        if donor.is_none() && plan.startup.is_empty() {
            continue;
        }
        let (startup, tables, report) = walk_planned_startup_materials(&plan.startup, progress);
        prep.report.extend(report);
        let opened = donor.as_ref().map(|path| {
            let stage = progress.begin_scoped(StageId::CommonAssets, "seeded common", None);
            let opened = open_zone_shared(path).map_err(|error| error.to_string());
            stage.finish_from(&opened);
            opened
        });
        prep.sources.push(SeededSourcePrep {
            family,
            donor,
            opened,
            startup,
            tables,
        });
    }
    prep
}

pub(super) struct SeededCommonSource {
    pub(super) path: PathBuf,
    pub(super) family: asset_core::FamilyId,
    pub(super) census: crate::CommonCensus,
}

pub(super) struct SeededCommonProducts {
    pub(super) materials: MaterialCatalog,
    pub(super) sources: Vec<SeededCommonSource>,
    pub(super) tables: Vec<CommonTables>,
    pub(super) images: Vec<(&'static str, HeldImagePlan)>,
    pub(super) report: Vec<String>,
}

pub(super) fn walk_seeded_common(
    prep: SeededCommonPrep,
    progress: &LoadProgress,
    mut materials: MaterialCatalog,
) -> SeededCommonProducts {
    let mut sources = Vec::new();
    let mut tables = Vec::new();
    let mut images = Vec::new();
    let mut report = prep.report;
    for source in prep.sources {
        materials.absorb_missing_reals(source.startup);
        // Startup tables survive a subsequent failed common capture.
        let table_start = tables.len();
        tables.extend(source.tables);
        let Some(opened) = source.opened else {
            continue;
        };
        let donor = source.donor.expect("opened common source path");
        let image = match opened {
            Ok(image) => image,
            Err(error) => {
                report.push(format!("seeded common {}: {error}", donor.display()));
                continue;
            }
        };
        if image.game != source.family {
            report.push(format!(
                "seeded common owner mismatch: {} planned {}, captured {}",
                donor.display(),
                source.family.as_str(),
                image.game.as_str()
            ));
            continue;
        }
        let Some(selected) = lane(image.game) else {
            report.push(format!("zone lane not installed: {}", image.game.as_str()));
            continue;
        };
        let mut census = selected.load_common(
            &donor,
            &image,
            asset_core::RuntimeProfile::Multiplayer,
            progress,
            true,
            materials,
        );
        materials = std::mem::take(&mut census.material_population);
        for table in &tables[table_start..] {
            if table.owner() == Some(image.game) {
                census.weapons.apply_stats_tables(&table.tables);
            }
        }
        report.append(&mut census.report);
        if let Some(plan) = hold_image_plan(
            "seeded common",
            census.pending_images.take(),
            load_jobs::open(JobKind::ImageDecode).namespace(source.family.as_str()),
        ) {
            images.push((source.family.as_str(), plan));
        }
        tables.push(CommonTables::capture(
            Some(image.game),
            std::mem::take(&mut census.cac_tables),
        ));
        sources.push(SeededCommonSource {
            path: donor,
            family: image.game,
            census,
        });
    }
    SeededCommonProducts {
        materials,
        sources,
        tables,
        images,
        report,
    }
}

pub(super) struct SupplementalCommonSource {
    pub(super) family: asset_core::FamilyId,
    pub(super) path: Option<PathBuf>,
    pub(super) census: Option<crate::CommonCensus>,
    pub(super) startup: MaterialCatalog,
    pub(super) startup_light_defs: Vec<asset_world::CapturedLightDef>,
    pub(super) tables: Vec<CommonTables>,
}

#[derive(Default)]
pub(super) struct SupplementalCommonProducts {
    pub(super) sources: Vec<SupplementalCommonSource>,
    pub(super) images: Vec<(&'static str, HeldImagePlan)>,
    pub(super) report: Vec<String>,
}

pub(super) fn walk_supplemental_common(
    runtime: Option<&Path>,
    progress: &LoadProgress,
    enabled: frame::AssetFamilies,
) -> SupplementalCommonProducts {
    let mut products = SupplementalCommonProducts {
        sources: Vec::new(),
        images: Vec::new(),
        report: Vec::new(),
    };
    let root = match games_root() {
        Ok(root) => root,
        Err(error) => {
            products.report.push(error);
            return products;
        }
    };
    for (family, policy) in crate::common_source_families() {
        if !enabled.enabled(family.as_str()) || policy.material_seed.is_some() {
            continue;
        }
        let Some(planner) = policy.supplemental else {
            continue;
        };
        let plan = match planner(&root, runtime) {
            Ok(plan) => plan,
            Err(error) => {
                products
                    .report
                    .push(format!("{} common sources: {error}", family.as_str()));
                continue;
            }
        };
        products.report.extend(plan.report);
        let mut startup = MaterialCatalog::default();
        let mut startup_light_defs = Vec::new();
        let mut tables = Vec::new();
        for source in plan.startup {
            let (owner, population) = walk_population_file(&source.path, progress, startup);
            products.report.extend(population.report);
            startup = population.materials;
            startup_light_defs.extend(population.light_defs);
            if source.collect_tables {
                tables.push(CommonTables::capture(owner, population.cac_tables));
            }
        }
        let path = plan.common;
        let captured = path.as_deref().and_then(|path| {
            capture_common_zone(
                path,
                Some(family),
                "supplemental common",
                "supplemental common",
                progress,
                &mut products.report,
            )
        });
        let mut captured_census = None;
        if let Some((owner, mut census)) = captured {
            if owner != family {
                products.report.push(format!(
                    "common source owner mismatch: planned {}, captured {}",
                    family.as_str(),
                    owner.as_str()
                ));
            } else {
                for source in &tables {
                    if source.owner() == Some(owner) {
                        census.weapons.apply_stats_tables(&source.tables);
                    }
                }
                tables.push(CommonTables::capture(
                    Some(owner),
                    std::mem::take(&mut census.cac_tables),
                ));
                match asset_game::supplemental_weapon_tables(owner, &root) {
                    Ok(extra) => {
                        products.report.extend(extra.report);
                        census.weapons.apply_stats_tables(&extra.tables);
                        tables.push(CommonTables::capture(Some(owner), extra.tables));
                    }
                    Err(error) => products.report.push(error.into()),
                }
                products.report.append(&mut census.report);
                if let Some(plan) = hold_image_plan(
                    "supplemental common",
                    census.pending_images.take(),
                    load_jobs::open(JobKind::ImageDecode).namespace(owner.as_str()),
                ) {
                    products.images.push((owner.as_str(), plan));
                }
                captured_census = Some(census);
            }
        }
        products.sources.push(SupplementalCommonSource {
            family,
            path,
            census: captured_census,
            startup,
            startup_light_defs,
            tables,
        });
    }
    products
}

pub(super) async fn walk_startup_material_zones(
    map_path: Option<&PathBuf>,
    progress: &LoadProgress,
    profile: asset_core::RuntimeProfile,
) -> (
    MaterialCatalog,
    Vec<String>,
    CommonTables,
    Vec<asset_world::CapturedLightDef>,
    crate::ScriptSources,
    XAnimBuild,
) {
    let startup_zones = profile.startup_zones();
    let Some(map_path) = map_path else {
        return (
            MaterialCatalog::default(),
            Vec::new(),
            CommonTables::default(),
            Vec::new(),
            crate::ScriptSources::default(),
            XAnimBuild::default(),
        );
    };
    let games = games_root().ok();

    let opened = startup_zones
        .iter()
        .copied()
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
        .collect::<Vec<_>>();
    let mut seed = MaterialCatalog::default();
    let mut report = Vec::new();
    let mut reuse_mat = 0usize;
    let mut reuse_img = 0usize;
    let mut zones_ok = 0usize;
    let mut stats = CommonTables::default();
    let mut light_defs = Vec::new();
    let mut scripts = crate::ScriptSources::default();
    let mut xanims = XAnimBuild::default();
    for (zone, task) in startup_zones.iter().copied().zip(opened) {
        match task.await {
            Ok((path, image)) => {
                let envelope = peek_zone_version(&path)
                    .map(|v| format!("{v:#x}"))
                    .unwrap_or_else(|| "unreadable".into());
                let Some(selected) = lane(image.game) else {
                    report.push(format!("zone lane not installed: {}", image.game.as_str()));
                    continue;
                };
                let pop = selected.load_material_population(&path, &image, progress, seed);
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
                xanims.absorb(pop.xanims);
                if matches!(zone, "code_post_gfx_mp" | "code_post_gfx") {
                    stats = CommonTables::capture(Some(image.game), pop.cac_tables);
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
        "startup material walk: zones={zones_ok} fpv=0 weapons=0 (startup population sink; no common census)",
    ));
    report.push(format!(
        "s2 startup walk: reuse_mat={reuse_mat} reuse_img={reuse_img} pool_mat={} pool_img={}",
        seed.materials.len(),
        seed.images.len(),
    ));
    (seed, report, stats, light_defs, scripts, xanims)
}

pub(super) fn load_localized_strings_beside(
    zone_ff: &std::path::Path,
    report: &mut Vec<String>,
    stage: &asset_transport::progress::StageHandle,
    profile: asset_core::RuntimeProfile,
) -> LocalizeCatalog {
    let mut catalog = LocalizeCatalog::default();
    let root = match games_root() {
        Ok(root) => root,
        Err(error) => {
            report.push(format!("localize gap: IW4L_GAMES unset ({error})"));
            return catalog;
        }
    };
    let runtime_language = find_runtime_common_mp(&root, zone_ff)
        .ok()
        .and_then(|zone| zone.path.parent()?.file_name()?.to_str().map(str::to_owned));
    let planned = if profile == asset_core::RuntimeProfile::Campaign {
        let mut plan = asset_game::LocalizationPlan::default();
        for name in [
            "code_post_gfx",
            "localized_code_post_gfx",
            "common",
            "localized_common",
            "patch",
        ] {
            if let Ok(found) = find_zone_for_tree(zone_ff, name) {
                plan.sources.push((asset_core::FamilyId::IW4, found));
            }
        }
        plan
    } else {
        asset_game::plan_localization(&root, zone_ff, runtime_language.as_deref())
    };
    report.extend(planned.report);
    let plan = planned.sources;
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
