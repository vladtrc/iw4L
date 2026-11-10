use super::*;

static NEXT_COMMON_PROFILE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommonKey {
    pub(super) runtime: Option<PathBuf>,
    pub(super) foreign: Option<PathBuf>,
    source_revision: u64,
    collision_adapter_version: u32,
    families: frame::AssetFamilies,
    profile: asset_core::RuntimeProfile,
}

impl CommonKey {
    pub(super) fn for_match(
        zone_ff: Option<&Path>,
        runtime: Option<&Path>,
        report: &mut Vec<String>,
        families: frame::AssetFamilies,
        profile: asset_core::RuntimeProfile,
    ) -> Self {
        let source_revision =
            games_root().map_or(0, |games| asset_transport::installation_revision(&games.0));
        Self {
            collision_adapter_version: asset_world::COLLISION_ADAPTER_VERSION,
            source_revision,
            runtime: runtime.map(Path::to_path_buf),
            foreign: if profile == asset_core::RuntimeProfile::Campaign {
                None
            } else {
                resolve_foreign_material_donor(zone_ff, runtime, report, families.clone())
            },
            families,
            profile,
        }
    }

    fn shell(games: &asset_transport::GamesRoot, report: &mut Vec<String>) -> Self {
        let runtime = match find_common_mp_for_envelope(games, fastfile_iw4::ZONE_VERSION_PC) {
            Ok(found) => Some(found.path),
            Err(error) => {
                report.push(format!("CAC iw4: {error}"));
                None
            }
        };
        Self {
            collision_adapter_version: asset_world::COLLISION_ADAPTER_VERSION,
            source_revision: asset_transport::installation_revision(&games.0),
            runtime,
            foreign: None,
            families: frame::AssetFamilies::all_installed(),
            profile: asset_core::RuntimeProfile::Multiplayer,
        }
    }
}

impl std::fmt::Display for CommonKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.runtime {
            Some(path) => write!(f, "{}", path.display())?,
            None => write!(f, "<no runtime {}>", self.profile.common_zone())?,
        }
        if let Some(foreign) = &self.foreign {
            write!(f, " + {}", foreign.display())?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) struct CommonCounts {
    pub(super) startup_count: usize,
    pub(super) seed_mat_count: usize,
    pub(super) seed_reuse_mat: usize,
    pub(super) seed_xanim_n: usize,
    pub(super) foreign_count: usize,
    pub(super) seed_mat: usize,
    pub(super) seed_img: usize,
    pub(super) common_reuse_mat: usize,
    pub(super) common_reuse_img: usize,
}

#[derive(Clone)]
pub(super) struct CommonMaterialDonor {
    pub(super) family: asset_core::FamilyId,
    pub(super) materials: MaterialCatalog,
    pub(super) scene: asset_world::MapXModelSceneCatalog,
}

#[derive(Clone)]
pub(super) struct CommonProducts {
    pub(super) campaign_hud: Option<asset_game::CampaignHudCatalog>,
    pub(super) scripts: crate::ScriptSources,
    pub(super) material_seed: MaterialCatalog,
    pub(super) map_payloads: super::map_common_payloads::MapCommonPayloads,
    pub(super) scene_models: Vec<asset_world::MapXModelSceneCatalog>,
    pub(super) light_defs: Vec<asset_world::CapturedLightDef>,
    pub(super) pen_table: weapon_iw4::PenetrationDepthTable,
    pub(super) pen_table_loaded: bool,
    pub(super) lochit_table: Option<[f32; weapon_iw4::HITLOC_COUNT]>,
    pub(super) tracers: asset_game::TracerCatalog,
    pub(super) xmodel_walk: crate::PreparedXModelWalkCensus,
    pub(super) s1_common_bytes: usize,
    pub(super) teamsets: std::collections::HashMap<String, asset_game::MapTeamSettings>,
    pub(super) film_visions: std::collections::BTreeMap<
        String,
        Result<asset_world::FilmVision, asset_world::FilmVisionParseError>,
    >,
    pub(super) weapons: WeaponBuild,
    pub(super) fpv_meshes: FpvMeshBuild,
    pub(super) world_weapons: WorldWeaponBuild,
    pub(super) projectile_meshes: ProjectileMeshBuild,
    pub(super) xanims: XAnimBuild,
    pub(super) fx: FxCatalog,
    pub(super) fx_models: asset_game::FxModelCatalog,
    pub(super) impact_fx: Option<asset_game::OwnedFxImpactTable>,
    pub(super) seed_xanims: XAnimBuild,
    pub(super) seed_fx: FxCatalog,
    pub(super) seed_impact_fx: Vec<asset_game::OwnedFxImpactTable>,
    pub(super) material_donors: Vec<CommonMaterialDonor>,
    pub(super) strings: LocalizeCatalog,
    pub(super) counts: CommonCounts,
    pub(super) report: Vec<String>,
    pub(super) localize_report: Vec<String>,
}

pub(super) struct KeptImages {
    pub(super) label: &'static str,
    pub(super) namespace: &'static str,
    pub(super) batch: asset_material::material_images::DecodedImageBatch,
    pub(super) job: load_jobs::Job,
}

pub struct CommonSet {
    pub(super) ui_images: asset_material::UiImagePublication,
    pub(super) id: u64,
    pub(super) key: CommonKey,
    pub(super) products: CommonProducts,
    editor_weapons: asset_game::EditorWeaponCatalog,
    donor_images: async_lock::OnceCell<Vec<KeptImages>>,
    donor_plans: std::sync::Mutex<Option<Vec<(&'static str, HeldImagePlan)>>>,
    pub(super) fpv_plan: Option<ImageDemandPlan>,
    pub(super) retained: std::sync::Mutex<asset_material::material_images::PayloadRetention>,
    pub(super) cac_tables: Vec<(asset_core::FamilyId, asset_game::CapturedStringTable)>,
    pub(super) prepared_ms: f32,
    pub(super) ready_at: std::time::Instant,
}

impl CommonSet {
    pub(super) fn start_donor_images(self: &Arc<Self>) {
        let Some(plans) = self
            .donor_plans
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
        else {
            return;
        };
        // A canceled match must not poison the shared image publication.
        let progress = LoadProgress::default();
        let pending: Vec<_> = plans
            .into_iter()
            .map(|(namespace, plan)| (namespace, plan.enqueue(&progress)))
            .collect();
        let keeping = Arc::clone(self);
        load_pool()
            .spawn(async move {
                let mut kept = Vec::new();
                for (namespace, pending) in pending {
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
                let _ = keeping.donor_images.set(kept).await;
            })
            .detach();
    }

    pub(super) async fn donor_images(&self) -> &[KeptImages] {
        self.donor_images.wait().await
    }

    pub(super) fn donor_batches(&self) -> usize {
        self.donor_images.get().map_or(0, Vec::len)
    }

    pub(super) fn retain(&self, batch: &asset_material::material_images::DecodedImageBatch) -> u64 {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .keep(batch)
    }

    pub(super) fn retained_bytes(&self) -> u64 {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .bytes()
    }

    pub(super) fn retained_payloads(&self) -> usize {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .payloads()
    }
}

struct CommonFlight {
    pub(super) key: CommonKey,
    set: async_lock::OnceCell<Option<Arc<CommonSet>>>,
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
        let _ = flight.set.set_blocking(None);
    }
}

pub(super) async fn ensure_common(key: CommonKey) -> (Arc<CommonSet>, &'static str) {
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
                    set: async_lock::OnceCell::new(),
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
                            let _ = flight.set.set(Some(set)).await;
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
    pub player_defaults_config: Option<String>,
    pub challenges: Option<crate::script_sources::ScriptTable>,
    pub player_schemas: std::collections::BTreeMap<String, Arc<structured_data_iw4::DefinitionSet>>,
    pub ui_images: asset_material::UiImagePublication,
    pub weapons: asset_game::EditorWeaponCatalog,
    pub strings: LocalizeCatalog,
    pub tables: Vec<(asset_core::FamilyId, asset_game::CapturedStringTable)>,
    pub report: Vec<String>,
}

pub async fn load_shell_common(games: asset_transport::GamesRoot) -> ShellCommon {
    let mut report = Vec::new();
    let key = CommonKey::shell(&games, &mut report);
    let (common, reach) = ensure_common(key).await;
    let weapons = common.editor_weapons.clone();

    let counts = asset_core::families()
        .iter()
        .map(|family| {
            let id = family.id();
            format!("{}={}", id.as_str(), weapons.namespace_count(id))
        })
        .collect::<Vec<_>>()
        .join(" ");
    report.push(format!(
        "CAC: {reach} common set {}; weapons={} ({counts}) tables={}",
        common.key,
        weapons.len(),
        common.cac_tables.len(),
    ));
    for owner in asset_core::families().iter() {
        let id = owner.id();
        let families = weapons.weapon_families();
        let offered: Vec<_> = families
            .offered()
            .filter(|family| family.key.namespace == id)
            .collect();
        let excluded: Vec<_> = families
            .excluded()
            .iter()
            .filter(|(key, _)| key.split_once(':').is_some_and(|(ns, _)| ns == id.as_str()))
            .collect();
        report.push(format!(
            "CAC {} families: {} offered ({} primary, {} secondary, {} lethal, {} tactical); {} excluded: {:?}",
            id.as_str(),
            offered.len(),
            offered.iter().filter(|f| f.slot == asset_game::FamilySlot::Primary).count(),
            offered.iter().filter(|f| f.slot == asset_game::FamilySlot::Secondary).count(),
            offered.iter().filter(|f| f.slot == asset_game::FamilySlot::Lethal).count(),
            offered.iter().filter(|f| f.slot == asset_game::FamilySlot::Tactical).count(),
            excluded.len(),
            excluded,
        ));
    }
    for (namespace, table) in &common.cac_tables {
        report.push(format!(
            "CAC {}: {} rows={}",
            namespace.as_str(),
            table.name,
            table.rows
        ));
    }
    ShellCommon {
        player_defaults_config: common
            .products
            .scripts
            .config("mp/stats_init.cfg")
            .map(str::to_owned),
        challenges: common
            .products
            .scripts
            .tables()
            .get("mp/allchallengestable.csv")
            .cloned(),
        player_schemas: common.products.scripts.schemas().clone(),
        ui_images: common.ui_images.clone(),
        weapons,
        strings: common.products.strings.clone(),
        tables: common.cac_tables.clone(),
        report,
    }
}

async fn prepare_common(key: CommonKey) -> Arc<CommonSet> {
    let started = std::time::Instant::now();
    let progress = LoadProgress::default();
    let pool = load_pool();
    let anchor = key.runtime.clone();
    let profile = key.profile;
    let startup_walk = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        pool.spawn(
            async move { walk_startup_material_zones(anchor.as_ref(), &progress, profile).await },
        )
    };
    let seeded_prep = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        let enabled = key.families.clone();
        pool.spawn(async move {
            if profile == asset_core::RuntimeProfile::Campaign {
                return SeededCommonPrep::default();
            }
            prepare_seeded_common(anchor.as_deref(), &progress, enabled)
        })
    };
    let supplemental_walk = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        let enabled = key.families.clone();
        pool.spawn(async move {
            if profile == asset_core::RuntimeProfile::Campaign {
                return SupplementalCommonProducts::default();
            }
            walk_supplemental_common(anchor.as_deref(), &progress, enabled)
        })
    };
    let localize_walk = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        pool.spawn(async move {
            let mut report = Vec::new();
            let strings = match anchor {
                Some(path) => {
                    let stage = progress.begin(StageId::Localization, None);
                    let strings =
                        load_localized_strings_beside(&path, &mut report, &stage, profile);
                    stage.done();
                    strings
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
    let campaign_hud_walk = {
        let anchor = anchor.clone();
        pool.spawn(async move {
            let mut report = Vec::new();
            let catalog = if profile == asset_core::RuntimeProfile::Campaign {
                anchor.as_deref().map(|path| {
                    let mut catalog = asset_game::MenuCatalog::default();
                    for name in [
                        "code_post_gfx",
                        "localized_code_post_gfx",
                        "common",
                        "localized_common",
                        "patch",
                    ] {
                        let Ok(found) = find_zone_for_tree(path, name) else {
                            continue;
                        };
                        match asset_game::load_menu_catalog(&found.path) {
                            Ok(part) => {
                                report.push(format!(
                                    "campaign HUD: {name} {} menus, {} fonts",
                                    part.menus.len(),
                                    part.fonts.len()
                                ));
                                catalog.absorb(part);
                            }
                            Err(error) => report.push(format!("campaign HUD gap: {name}: {error}")),
                        }
                    }
                    asset_game::CampaignHudCatalog(Arc::new(catalog))
                })
            } else {
                None
            };
            (catalog, report)
        })
    };
    let (
        material_seed,
        mut common_report,
        runtime_startup_tables,
        startup_light_defs,
        startup_scripts,
        startup_xanims,
    ) = startup_walk.await;
    let startup_count = material_seed.materials.len();
    let SeededCommonProducts {
        materials: mut material_seed,
        sources: seeded_sources,
        tables: mut supplemental_tables,
        images: seed_images,
        report,
    } = walk_seeded_common(seeded_prep.await, &progress, material_seed);
    common_report.extend(report);
    let seed_reuse_mat = material_seed.link_reused_materials;
    let seed_mat_count = material_seed
        .materials
        .len()
        .saturating_sub(startup_count)
        .saturating_add(seed_reuse_mat);
    let SupplementalCommonProducts {
        sources: mut ordinary_sources,
        images,
        report,
    } = supplemental_walk.await;
    let ordinary_images = images;
    let mut donor_plans = Vec::new();
    common_report.extend(report);

    // One common capture supplies both a material seed and weapon products when
    // the requested donor path is already among the supplemental sources.
    let mut foreign_count = 0;
    if let Some(path) = key.foreign.as_deref() {
        if seeded_sources.iter().any(|source| source.path == path) {
            // This source already captured directly into the shared seed.
        } else if let Some(source) = ordinary_sources
            .iter_mut()
            .find(|source| source.path.as_deref() == Some(path))
        {
            if let Some(census) = &mut source.census {
                let materials = std::mem::take(&mut census.material_population);
                foreign_count = materials.materials.len();
                let linked = material_seed.absorb_asset_population(materials);
                census.scene_models.remap_walk_materials(&linked);
            }
        } else if let Some((family, mut census)) = capture_common_zone(
            path,
            None,
            "material donor common",
            "material donor common",
            &progress,
            &mut common_report,
        ) {
            foreign_count = census.material_population.materials.len();
            material_seed.absorb_asset_population(census.material_population);
            common_report.extend(census.report);
            if let Some(plan) = hold_image_plan(
                "material donor common",
                census.pending_images.take(),
                load_jobs::open(JobKind::ImageDecode).namespace(family.as_str()),
            ) {
                donor_plans.push((family.as_str(), plan));
            }
        }
    }
    donor_plans.extend(ordinary_images);
    let mut runtime_family = None;
    let mut runtime = crate::CommonCensus::default();
    if let Some(path) = &anchor {
        let stage = progress.begin_scoped(StageId::CommonAssets, "runtime common", None);
        let opened = open_zone_shared(path);
        finish_zone_open(stage, &opened);
        match opened {
            Ok(image) => match lane(image.game) {
                Some(selected) => {
                    runtime_family = Some(image.game);
                    // Common zones have no world assets from which to infer the match profile.
                    runtime = selected.load_common(
                        path,
                        &image,
                        key.profile,
                        &progress,
                        true,
                        std::mem::take(&mut material_seed),
                    );
                    material_seed = std::mem::take(&mut runtime.material_population);
                }
                None => {
                    common_report.push(format!("zone lane not installed: {}", image.game.as_str()))
                }
            },
            Err(error) => common_report.push(format!("common_mp models: open zone: {error}")),
        }
    }
    common_report.append(&mut runtime.report);
    let fpv_plan = runtime
        .pending_images
        .take()
        .filter(|plan| !plan.is_empty());
    let mut scripts = runtime.scripts;
    scripts.overlay(startup_scripts);
    let common_film_visions = runtime.film_visions;
    let mut common_light_defs = startup_light_defs;
    common_light_defs.extend(runtime.light_defs);
    let common_pen_table = runtime.pen_table;
    let common_pen_loaded = runtime.pen_table_loaded;
    let common_lochit_table = runtime.lochit_table;
    let common_tracers = runtime.tracers;
    let xmodel_walk = runtime.xmodel_walk;
    let s1_common_bytes = runtime.s1_common_bytes;
    let runtime_teamsets = runtime.teamsets;
    let mut teamsets = std::collections::HashMap::new();
    let mut weapons = runtime.weapons;
    weapons.apply_stats_tables(&runtime_startup_tables.tables);
    let mut fpv_meshes = runtime.fpv;
    let mut world_weapons = runtime.world_weapons;
    let mut projectile_meshes = runtime.projectile_meshes;
    let mut xanims = runtime.xanims;
    let startup_anim_added = xanims.absorb(startup_xanims);
    common_report.push(format!(
        "startup animation keys retained: {startup_anim_added}"
    ));
    let mut common_fx = runtime.fx;
    let mut common_fx_models = runtime.fx_models;
    let common_impact = runtime.impact_fx;
    let mut map_payloads = super::map_common_payloads::MapCommonPayloads::default();
    let mut scene_models = Vec::new();
    let mut material_donors = Vec::new();
    let mut source_ui_images = Vec::new();
    if let Some(family) = runtime_family {
        source_ui_images.push((family, runtime.ui_images));
    }
    let mut seed_xanims = XAnimBuild::default();
    let mut seed_fx = FxCatalog::default();
    let mut seed_impact_fx = Vec::new();
    let mut compilers = Vec::new();
    // Publication order belongs to the family, and is independent of the
    // order in which the builder installs families or asynchronous reads finish.
    let mut captures = Vec::new();
    for source in ordinary_sources {
        material_seed.absorb_asset_population(source.startup);
        common_light_defs.extend(source.startup_light_defs);
        supplemental_tables.extend(source.tables);
        if let Some(census) = source.census {
            captures.push((source.family, false, census));
        }
    }
    for source in seeded_sources {
        captures.push((source.family, true, source.census));
    }
    captures.sort_by_key(|(family, _, _)| {
        (
            crate::common_source_family(*family)
                .expect("installed source policy")
                .publication_order,
            *family,
        )
    });
    for (family, seeded, mut census) in captures {
        weapons.absorb(census.weapons);
        fpv_meshes.absorb(census.fpv);
        world_weapons.absorb(census.world_weapons);
        projectile_meshes.absorb(census.projectile_meshes);
        common_fx_models.absorb(census.fx_models);
        common_light_defs.extend(census.light_defs);
        teamsets.extend(census.teamsets);
        map_payloads.insert_surfaces(family, census.shared_surfaces);
        map_payloads.insert_animations(census.player_anim_sources);
        source_ui_images.push((family, census.ui_images));
        compilers.extend(census.preparation);
        seed_impact_fx.extend(census.impact_fx);
        if seeded {
            seed_xanims.absorb(census.xanims);
            seed_fx.absorb(census.fx);
            scene_models.push(census.scene_models);
        } else {
            xanims.absorb(census.xanims);
            common_fx.absorb(census.fx);
            if census.material_population.materials.is_empty() {
                scene_models.push(census.scene_models);
            } else {
                census.material_population.mark_images_common_owned();
                material_donors.push(CommonMaterialDonor {
                    family,
                    materials: census.material_population,
                    scene: census.scene_models,
                });
            }
        }
    }
    teamsets.extend(runtime_teamsets);
    if let Some(family) = runtime_family {
        map_payloads.insert_surfaces(family, runtime.shared_surfaces);
    }
    let mut runtime_animations = runtime.player_anim_sources;
    runtime_animations.compile();
    map_payloads.insert_animations(runtime_animations);
    scene_models.push(runtime.scene_models);
    let seed_xanim_n = seed_xanims.len();
    for compiler in compilers {
        let prepared = compiler.compile(crate::CommonPreparationProducts {
            weapons,
            materials: material_seed,
            fpv: fpv_meshes,
            world: world_weapons,
            projectiles: projectile_meshes,
            xanims,
            fx: common_fx,
            report: Vec::new(),
        });
        common_report.extend(
            prepared
                .refusals
                .into_iter()
                .map(|cause| format!("common family preparation refused: {cause}")),
        );
        seed_impact_fx.extend(prepared.impact_fx);
        let products = prepared.products;
        common_report.extend(products.report);
        (
            weapons,
            material_seed,
            fpv_meshes,
            world_weapons,
            projectile_meshes,
            xanims,
            common_fx,
        ) = (
            products.weapons,
            products.materials,
            products.fpv,
            products.world,
            products.projectiles,
            products.xanims,
            products.fx,
        );
    }
    let common_reuse_mat = material_seed.link_reused_materials;
    let common_reuse_img = material_seed.link_reused_images;
    let seed_mat = material_seed.materials.len();
    let seed_img = material_seed.images.len();
    supplemental_tables.sort_by_key(|source| {
        source.owner().map(|family| {
            (
                crate::common_source_family(family)
                    .expect("installed table owner")
                    .publication_order,
                family,
            )
        })
    });
    let cac_tables: Vec<_> = [
        runtime_startup_tables,
        CommonTables::capture(runtime_family, runtime.cac_tables),
    ]
    .into_iter()
    .chain(supplemental_tables)
    .flat_map(CommonTables::into_owned)
    .collect();
    let mut camouflage_images = Vec::new();
    for (namespace, policy) in asset_game::weapon_policies() {
        if let Some(model) = policy.camouflage.model
            && let Some((_, table)) = cac_tables.iter().find(|(ns, table)| {
                *ns == namespace && table.name.eq_ignore_ascii_case(model.table)
            })
        {
            let dressed =
                weapons.prepare_model_camouflages(namespace, table, &material_seed, &fpv_meshes);
            common_report.push(format!(
                "{}: {dressed} weapon configurations prepared",
                model.label
            ));
        }
        if let Some(material) = policy.camouflage.materials {
            let options = cac_tables
                .iter()
                .find(|(ns, t)| *ns == namespace && t.name.eq_ignore_ascii_case(material.options));
            let choices = cac_tables
                .iter()
                .find(|(ns, t)| *ns == namespace && t.name.eq_ignore_ascii_case(material.choices));
            if let (Some((_, options)), Some((_, choices))) = (options, choices) {
                let dressed = weapons.prepare_material_camouflages(
                    namespace,
                    options,
                    choices,
                    &mut material_seed,
                    &fpv_meshes,
                    &world_weapons,
                );
                common_report.push(format!(
                    "{}: {dressed} weapon configurations prepared",
                    material.label
                ));
                let keys: Vec<_> = (1..=weapons.len() as u32)
                    .flat_map(|id| {
                        weapons
                            .material_camouflages_of(id)
                            .iter()
                            .flat_map(|camo| camo.materials.iter().map(|(_, to)| to.clone()))
                    })
                    .filter(|key| key.namespace == namespace)
                    .collect();
                if let Ok(zone) = games_root().and_then(|root| (material.image_source)(&root)) {
                    let stage = progress.begin_scoped(StageId::Images, material.label, None);
                    let job = load_jobs::open(JobKind::ImageDecode).namespace(namespace.as_str());
                    let plan = asset_material::material_images::plan_material_images_for_keys(
                        &zone.path,
                        &mut material_seed,
                        keys,
                        &stage,
                        load_pool(),
                    );
                    stage.done();
                    if let Some(held) = hold_image_plan(material.label, Some(plan), job) {
                        camouflage_images.push((namespace.as_str(), held));
                    }
                }
            }
        }
    }

    weapons.set_family_tables(cac_tables.clone());
    let configuration_results = weapons.prepare_configurations();
    weapons.resolve_fpv_mesh_edges(&fpv_meshes);
    weapons.resolve_world_model_edges(&world_weapons);

    for (family, prepared) in configuration_results {
        common_report.push(format!(
            "{} configurations: prepared={} refused={} {:?}",
            family.as_str(),
            prepared.prepared,
            prepared.refused.len(),
            prepared.refused
        ));
    }
    let (strings, localize_report) = localize_walk.await;
    common_report.extend(progress.timing_report());
    let prepared_ms = started.elapsed().as_secs_f32() * 1000.0;
    diag::info!(
        World,
        "common set: {key} prepared in {prepared_ms:.0}ms; donor images deferred until a match"
    );

    material_seed.mark_images_common_owned();
    let mut ui_images = asset_material::UiImageBuild::default();
    if let Ok(games) = games_root()
        && let Err(error) = ui_images.retain_archives(&games)
    {
        common_report.push(format!("UI archives: {error}"));
    }
    ui_images.retain_materials(&material_seed);
    for donor in &material_donors {
        ui_images.retain_materials(&donor.materials);
    }
    for (family, images) in source_ui_images {
        ui_images.zone_images(family, images);
    }
    let preview_weapons = weapons.clone().publish_for_editor();
    for family in preview_weapons.weapon_families().families() {
        if let Some(id) = family.base
            && let Some(image) = preview_weapons.hud_icon_image_of(id)
            && !family.image.is_empty()
        {
            ui_images.preview_fallback(family.key.namespace, &family.image, image);
        }
    }
    let (campaign_hud, hud_report) = campaign_hud_walk.await;
    common_report.extend(hud_report);
    Arc::new(CommonSet {
        ui_images: ui_images.publish(),
        editor_weapons: preview_weapons,
        id: NEXT_COMMON_PROFILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        key,
        products: CommonProducts {
            campaign_hud,
            scripts,
            material_seed,
            map_payloads,
            scene_models,
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
            fx: common_fx,
            fx_models: common_fx_models,
            impact_fx: common_impact,
            seed_xanims,
            seed_fx,
            seed_impact_fx,
            material_donors,
            strings,
            counts: CommonCounts {
                startup_count,
                seed_mat_count,
                seed_reuse_mat,
                seed_xanim_n,
                foreign_count,
                seed_mat,
                seed_img,
                common_reuse_mat,
                common_reuse_img,
            },
            report: common_report,
            localize_report,
        },
        donor_images: async_lock::OnceCell::new(),
        donor_plans: std::sync::Mutex::new(Some(
            seed_images
                .into_iter()
                .chain(camouflage_images)
                .chain(donor_plans)
                .collect(),
        )),
        fpv_plan,
        retained: std::sync::Mutex::new(Default::default()),
        cac_tables,
        prepared_ms,
        ready_at: std::time::Instant::now(),
    })
}
