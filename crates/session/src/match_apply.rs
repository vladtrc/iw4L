use asset_game::WeaponRegistry;
use asset_world::{ClipCollision, SpawnPoint};
use assets::{
    LoadingScreen, MatchType10SoundHints, PreparedDestructibleDeath, PreparedFpvMeshes,
    PreparedWeapons, PreparedXAnims,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::{
    CacWeaponOffer, ClassSelectHandoff, HostClassLoadouts, InstalledMatch, LaunchIdentity,
    LaunchReport, WorldProducts,
};
use net::{AuthorityInputGate, AuthorityLoadHold, AuthorityWorld, ClientSet};

use crate::SessionContentManifest;
use crate::match_load::PreparedMatchReady;
use crate::weapon_content;
use render_frontend::adapters::anim::dyn_ent::{DynEntPhysClip, DynEntPhysWorld};
use render_frontend::prepare::scene::camera::SimCamera;
use render_frontend::prepare::scene::world::{WorldScene, world_scene_from_draw};
use render_fx::{PreparedFxCatalog, PreparedFxModels, PreparedImpactFx, PreparedTracers};
use std::sync::Arc;

use crate::loadout::{AuthoritativeClassProjection, ClassRow, project_class};
use crate::startup::{PendingConsoleLines, StartupCommands};

fn log_world_report(report: &[String]) {
    if report.is_empty() {
        diag::info!(World, "world report: no rows");
        return;
    }
    diag::info!(World, "world report: {} rows", report.len());
    for row in report {
        diag::info!(World, "world report: {row}");
    }
}

#[derive(SystemParam)]
pub struct PreparedMatchSource<'w, 's> {
    ready: ResMut<'w, PreparedMatchReady>,
    audio: Option<Res<'w, audio::LoadingAudio>>,
    audio_candidate: Option<Res<'w, audio::PreparedMatchAudio>>,
    audio_silent: Option<Res<'w, audio::AudioSilent>>,
    swap: Res<'w, crate::SessionSwapRequest>,
    bridge: Option<Res<'w, net::MasterBridge>>,
    intent: Option<Res<'w, net::MasterLaunchIntent>>,
    loading: Option<ResMut<'w, LoadingScreen>>,
    load: Option<Res<'w, assets::MapLoadProcess>>,
    host_classes: Option<Res<'w, HostClassLoadouts>>,
    catalog: Option<Res<'w, asset_game::MenuCatalog>>,
    identity: Option<Res<'w, LaunchIdentity>>,
    merged: Option<Res<'w, assets::MergedMap>>,
    install_stage: Local<'s, Option<asset_transport::StageHandle>>,
}

#[derive(SystemParam)]
pub struct MatchInstallConsole<'w> {
    startup: ResMut<'w, StartupCommands>,
    pending: ResMut<'w, PendingConsoleLines>,
}

impl MatchInstallConsole<'_> {
    fn release_startup(&mut self) {
        self.pending.0.append(&mut self.startup.lines);
    }
}

#[derive(SystemParam)]
pub struct MatchInstallAuthority<'w> {
    input_gate: Res<'w, AuthorityInputGate>,
    cheats: Option<Res<'w, sim::HostCheats>>,
    profile: Option<Res<'w, sim::LocalPlayerProfile>>,
    previous: Option<Res<'w, AuthorityWorld>>,
    account: Option<Res<'w, crate::LocalAccount>>,
    local: Option<Res<'w, net::LocalPresentClient>>,
}

#[derive(SystemParam)]
pub struct MatchInstallPresentation<'w> {
    probe: Option<ResMut<'w, LaunchReport>>,
    camera: Res<'w, SimCamera>,
    player_data: Res<'w, sim::LocalPlayerData>,
}

#[derive(SystemParam)]
pub struct MatchInstallPolicy<'w> {
    role: Res<'w, frame::RuntimeRole>,
    mode_selection: Option<Res<'w, sim::HostGameModeSelection>>,
    rules: Option<Res<'w, frame::HostMatchRules>>,
}

impl MatchInstallPolicy<'_> {
    fn preflight(
        &self,
        prepared: assets::PreparedMatch,
        zone: &str,
        merged: Option<&assets::MergedMap>,
        catalog: Option<&asset_game::MenuCatalog>,
        identity: Option<&LaunchIdentity>,
    ) -> Result<MatchInstallPlan, InstallRefusal> {
        // A merged map's own dvars (`"dvars"` in its recipe) go over the host's.
        let rules = merged
            .map(|merged| merged_rules(self.rules.as_deref(), merged))
            .or_else(|| self.rules.as_deref().cloned());
        preflight_match_install(
            prepared,
            zone,
            self.mode_selection.as_deref(),
            rules.as_ref(),
            catalog,
            identity,
            *self.role,
        )
    }
}

impl PreparedMatchSource<'_, '_> {
    fn install(
        mut self,
        mut commands: Commands,
        mut console: MatchInstallConsole,
        authority: MatchInstallAuthority,
        presentation: MatchInstallPresentation,
        policy: MatchInstallPolicy,
        mut failed: MessageWriter<frame::MapLoadFailed>,
    ) {
        let MatchInstallAuthority {
            input_gate,
            cheats,
            profile,
            previous,
            account,
            local,
        } = authority;
        let allow_debug_actions = cheats.is_some_and(|cheats| cheats.0);
        let MatchInstallPresentation {
            mut probe,
            camera: sim_cam,
            player_data,
        } = presentation;
        let stale_key = !self.ready.match_key.is_none()
            && self.bridge.as_ref().is_none_or(|bridge| {
                let state = bridge.state();
                !state.in_match() || state.identity().match_key() != self.ready.match_key
            });
        if !self.swap.can_install() || stale_key {
            diag::info!(
                World,
                "match install: discarded stale load {:?} ({})",
                self.ready.match_key,
                self.ready.zone
            );
            if let Some(stage) = self.install_stage.take() {
                stage.cancel();
            }
            commands.remove_resource::<PreparedMatchReady>();
            return;
        }
        if self.ready.match_key.is_none()
            && self.intent.as_ref().is_some_and(|intent| intent.is_join())
        {
            return;
        }
        if policy.role.runs_client()
            && self
                .audio
                .as_ref()
                .is_none_or(|audio| !audio.owns(self.ready.match_key))
        {
            return;
        }
        match self.audio.as_ref().map(|audio| audio.status()) {
            Some(audio::LoadingAudioStatus::Ready) => {
                if policy.role.runs_client()
                    && self.audio_silent.is_none()
                    && self
                        .audio_candidate
                        .as_ref()
                        .is_none_or(|candidate| candidate.match_key != self.ready.match_key)
                {
                    return;
                }
            }
            Some(audio::LoadingAudioStatus::Failed(error)) => {
                failed.write(frame::MapLoadFailed {
                    match_key: self.ready.match_key,
                    zone: self.ready.zone.clone(),
                    error: error.clone(),
                });
                commands.remove_resource::<PreparedMatchReady>();
                return;
            }
            _ => return,
        }
        // The stage opens a frame before the work so the row is already up when
        // the main thread stops answering — the yield is the point, not an
        // accident of where the handle was created.
        let live_load = self
            .load
            .as_deref()
            .filter(|process| !process.is_complete())
            .map(|process| &process.progress);
        if live_load.is_some() && self.install_stage.is_none() {
            if let Some(progress) = live_load {
                *self.install_stage = Some(progress.begin(asset_transport::StageId::Install, None));
            }
            diag::info!(
                World,
                "match install: yield one frame so the overlay can name the main-thread hitch"
            );
            return;
        }
        let install_stage = self.install_stage.take();
        let install_started = std::time::Instant::now();

        let match_key = self.ready.match_key;
        let zone = std::mem::take(&mut self.ready.zone);
        let Some(mut prepared) = self.ready.prepared.take() else {
            return;
        };
        commands.remove_resource::<PreparedMatchReady>();
        let prepared_atlas = self.ready.atlas.take();

        let world_report = std::mem::take(&mut prepared.report);
        log_world_report(&world_report);
        let map_shocks: Vec<(String, String)> = prepared
            .scripts
            .shocks()
            .map(|(name, text)| (name.to_owned(), text.to_owned()))
            .collect();

        let plan = match policy.preflight(
            prepared,
            &zone,
            self.merged.as_deref(),
            self.catalog.as_deref(),
            self.identity.as_deref(),
        ) {
            Ok(plan) => plan,
            Err(refusal) => {
                diag::warn!(World, "match install refused: {}", refusal.error);
                if let Some(probe) = probe.as_deref_mut() {
                    probe.world_report = world_report;
                }
                if let Some(stage) = install_stage {
                    stage.fail();
                }
                if let Some(loading) = self.loading.as_deref_mut() {
                    loading.fail(refusal.error.clone());
                }
                failed.write(frame::MapLoadFailed {
                    match_key,
                    zone,
                    error: refusal.error,
                });
                console.release_startup();
                return;
            }
        };
        let prepared_install = (|| -> Result<bevy::ecs::world::CommandQueue, InstallRefusal> {
            let mut install = bevy::ecs::world::CommandQueue::default();
            install.push(audio::stage_prepared_match_audio);
            let MatchInstallPlan {
                ui_images,
                capabilities,
                scripts,
                script_level,
                script_entries,
                script_dvars,
                account_defaults,
                script_sound_aliases,
                objective_weapons,
                kind,
                gametype,
                scene: loaded_scene,
                weapons,
                killstreaks,
                fpv_meshes,
                bodies,
                soldiers,
                world_weapons,
                projectile_meshes,
                xmodel_walk,
                xanims,
                death,
                player_anim_sources,
                mut prepared_map,
                strings,
                clip,
                pen_table,
                pen_table_loaded,
                lochit_table,
                tracers,
                fx_catalog,
                type10,
                fx_models,
                impact_fx,
                mut authority_models,
                model_spawns,
            } = plan;
            let spawn_count = prepared_map.spawns.len();

            let mut local_player_data = player_data.clone();
            let definitions = script_level
                .schemas
                .get("mp/playerdata.def")
                .ok_or_else(|| {
                    InstallRefusal::new("Local progression: player data schema is missing")
                })?;
            let ranks = weapons
                .registry()
                .rank_progression()
                .map_err(|error| InstallRefusal::new(format!("Local progression: {error}")))?;
            local_player_data
                .install(
                    definitions,
                    ranks,
                    account
                        .as_ref()
                        .and_then(|account| account.snapshot.as_ref()),
                )
                .map_err(|error| InstallRefusal::new(format!("Local progression: {error:?}")))?;
            local_player_data.install_unlock_data(
                account_defaults.clone(),
                script_level
                    .tables
                    .get("mp/allchallengestable.csv")
                    .cloned(),
            );
            for name in ["mp/ranktable.csv", "mp/rankicontable.csv"] {
                if let Some(table) = script_level.tables.get(name) {
                    local_player_data.install_rank_table(name, table.clone());
                }
            }
            diag::info!(
                Sim,
                "local progression prepared before admission: rank={}",
                local_player_data
                    .progression
                    .as_ref()
                    .expect("installed progression")
                    .rank
            );
            let rank_tables = [
                weapons.registry().shared_rank_table(),
                weapons.registry().shared_rank_icons(),
            ]
            .map(|table| table.expect("validated rank presentation").clone());
            install.push(move |world: &mut World| {
                world
                    .resource_mut::<crate::scope_control::InstallAnnouncement>()
                    .0
                    .push(move |world: &mut World| {
                        if let Some(mut catalog) =
                            world.get_resource_mut::<asset_game::MenuCatalog>()
                        {
                            for table in rank_tables {
                                catalog.insert_string_table(table);
                            }
                        }
                        world.insert_resource(local_player_data);
                    });
            });

            let facts = std::mem::take(&mut prepared_map.facts);
            let airstrike_height = facts.airstrike_height;
            stage_resource(&mut install, crate::scope_control::MatchUiImages(ui_images));
            stage_resource(
                &mut install,
                assets::SessionCompass {
                    corners: facts.minimap_corners,
                    north_yaw: facts.north_yaw,
                    declaration: facts.compass,
                },
            );
            stage_resource(
                &mut install,
                asset_audio::SessionMapScriptSound(facts.script_sound),
            );
            stage_resource(
                &mut install,
                asset_game::SessionTeamSettings(facts.team_settings),
            );
            stage_resource(&mut install, assets::PreparedLocalizedStrings(strings));
            stage_resource(&mut install, fx_catalog);
            stage_resource(&mut install, type10);
            stage_resource(&mut install, fx_models);
            stage_resource(&mut install, impact_fx);
            stage_resource(&mut install, tracers);

            let mut scene = loaded_scene;
            scene.readiness = frame::WorldReadiness::new(frame::ReadinessState::Pending);
            let mut sim_cam = *sim_cam;
            let mut input_gate = *input_gate;
            let sim_weapons = weapon_content::compile(
                weapons.registry(),
                &world_weapons.0,
                lochit_table,
                objective_weapons,
                pen_table,
                pen_table_loaded,
            )
            .map_err(|error| {
                InstallRefusal::new(format!("Invalid simulation weapons: {error:?}"))
            })?;
            let mut content = sim::SimContentBuilder::for_match(Arc::clone(sim_weapons.content()));
            content
                .set_map(Arc::clone(&prepared_map.content))
                .map_err(|error| InstallRefusal::new(format!("Invalid map content: {error}")))?;
            content.set_script_sound_aliases(script_sound_aliases);
            content.set_script_fx(capabilities.script_fx.clone());
            stage_resource(&mut install, capabilities);
            let mut sim = sim::SimWorld::new();
            if policy.role.runs_authority()
                && let Some(previous) = previous.as_ref()
            {
                sim.set_persistent_data(previous.0.persistent_data().for_new_match());
            }
            if *policy.role == frame::RuntimeRole::Listen {
                let profile = previous
                    .as_ref()
                    .map(|authority| authority.0.local_player_profile())
                    .or_else(|| profile.as_deref().copied())
                    .unwrap_or(sim::LocalPlayerProfile::default());
                sim.set_local_player_profile(profile);
            }
            content.set_player_kit_collisions(
                player_kit_collision(soldiers.side(false).ok().map(Arc::as_ref))?,
                player_kit_collision(soldiers.side(true).ok().map(Arc::as_ref))?,
            );
            if let Some(Ok(tree)) = player_anim_sources.compiled() {
                for axis in [false, true] {
                    if let Ok(soldier) = soldiers.side(axis)
                        && let Ok(animation) = soldier.animation()
                        && let Ok(definition) = animation.runtime()
                    {
                        if axis {
                            content.set_player_axis_anim_tree(Some(Arc::clone(definition)));
                        } else {
                            let names = animation
                                .tree()
                                .nodes()
                                .iter()
                                .map(|node| node.name.clone())
                                .collect();
                            content.set_player_anim_tree(Some(Arc::clone(definition)), names);
                        }
                    }
                }
                if let Some(Ok(parsed)) = player_anim_sources.parsed_script() {
                    content.set_player_anim_properties(
                        (0..tree.nodes().len())
                            .map(|index| parsed.animation_properties(index as u16))
                            .collect(),
                    );
                }
            }

            let anim_namespace = prepared_map.namespace.expect("installed map family");
            content.set_script_model_anims(xanims.0.names().filter_map(|name| {
                let parts = &xanims.0.get(anim_namespace, name)?.parts;
                let frequency = if parts.numframes > 0 && parts.framerate > 0.0 {
                    parts.framerate / f32::from(parts.numframes)
                } else {
                    0.0
                };
                Some((
                    name.to_owned(),
                    sim::ScriptModelPlayAnim {
                        looping: parts.flags & 1 != 0,
                        frequency,
                    },
                ))
            }));
            content.set_mantle_xanims(sim::MantleXAnimBind::from_clips(|fast, i| {
                let name = sim::MantleXAnimBind::clip_name(fast, i)?;
                xanims
                    .0
                    .clip(asset_core::FamilyId::IW4, name)
                    .map(|clip| (*clip).clone())
            }));
            content.set_vehicle_turrets(weapons.registry().vehicle_turrets());
            content.set_vehicle_accel(
                weapons
                    .registry()
                    .vehicle_accel()
                    .map(|(name, accel)| (name.to_owned(), accel)),
            );
            content.set_vehicle_compass(
                weapons
                    .registry()
                    .vehicle_compass()
                    .map(|(name, icons, size)| (name.to_owned(), (icons.clone(), size))),
            );
            install_team_voice_prefixes(
                &mut content,
                self.catalog.as_deref(),
                self.identity.as_deref(),
                &zone,
            );
            let (atlas, atlas_clips) = prepared_atlas
                .map(|prepared| (prepared.atlas, prepared.clips))
                .unzip();
            content.set_atlas(atlas);
            stage_resource(
                &mut install,
                atlas_clips
                    .unwrap_or_else(render_frontend::adapters::anim::killcam::AtlasClips::default),
            );
            install_shocks(&mut content, self.catalog.as_deref(), &map_shocks);
            let mut primary = Vec::new();
            let mut secondary = Vec::new();
            let mut lethal = Vec::new();
            let mut tactical = Vec::new();
            let mut excluded = Vec::new();
            let families = weapons.registry().weapon_families();
            for family in families.offered() {
                let offer = CacWeaponOffer {
                    key: family.key.asset_key(),
                    item_group: Some(family.item_group.clone()),
                    attachments: family
                        .attachments
                        .iter()
                        .map(|choice| choice.name.clone())
                        .collect(),
                };
                match family.slot {
                    asset_game::FamilySlot::Primary => primary.push(offer),
                    asset_game::FamilySlot::Secondary => secondary.push(offer),
                    asset_game::FamilySlot::Lethal => lethal.push(offer),
                    asset_game::FamilySlot::Tactical => tactical.push(offer),
                    asset_game::FamilySlot::Other => {}
                }
            }
            excluded.extend(families.excluded().iter().cloned());
            stage_resource(&mut install, fpv_meshes);
            stage_resource(&mut install, bodies);
            stage_resource(&mut install, world_weapons);
            stage_resource(&mut install, projectile_meshes);
            stage_resource(&mut install, xmodel_walk);
            if let Some(Ok(parsed)) = player_anim_sources.parsed_script() {
                let tree = player_anim_sources
                    .compiled()
                    .and_then(|c| c.as_ref().ok())
                    .map(|t| t.as_ref());
                let slots = map_anim_items(&parsed.slots, tree, &xanims.0);
                let events = map_event_items(&parsed.events, tree, &xanims.0);
                content.set_player_anim_script(Some(sim::PlayerAnimScript::from_tables(
                    slots, events,
                )));
            }
            stage_resource(&mut install, xanims);
            stage_resource(&mut install, death);
            stage_resource(&mut install, player_anim_sources);
            stage_resource(&mut install, soldiers);
            input_gate.local_cmds_enabled = false;

            stage_resource(&mut install, DynEntPhysWorld::default());
            stage_resource(&mut install, DynEntPhysClip(clip.clone()));
            let brush_movers = std::mem::take(&mut authority_models.brush_movers);
            let intermission_view = scene.intermission_view.map(|view| sim::AuthoredSpawnPoint {
                classname: gamemode_iw4::playerlogic::MP_GLOBAL_INTERMISSION.to_owned(),
                origin: view.origin,
                angles: view.angles,
                script_linkto: String::new(),
                script_destructable_area: String::new(),
            });
            let players = MatchPlayerBootstrap {
                weapons: weapons.registry(),
                sim_weapons: &sim_weapons,
                host_classes: self.host_classes.as_deref(),
            };
            let clip = install_match_collision(
                &mut sim,
                content,
                clip,
                authority_models,
                players.weapons,
                players.sim_weapons,
            )?;
            let (sim_gap, lock_reasons) = players.bootstrap(
                &mut sim,
                MatchMapBootstrap {
                    clip,
                    intermission_view,
                    airstrike_height,
                    spawns: &prepared_map.spawns,
                },
                &mut sim_cam,
                &mut input_gate,
                kind,
                allow_debug_actions,
            )?;
            let script_facts =
                script_install_facts(&zone, gametype, &scripts, script_entries.len());
            if *policy.role == frame::RuntimeRole::Listen {
                sim.register_local_presentation_dvars(local.as_ref().map(|local| local.0));
            }
            sim.install_gsc_program(
                scripts,
                sim::script::NativeRegistry::default(),
                script_level,
            )
            .map_err(|e| script_refusal(&zone, gametype, "install", &e))?;
            if *policy.role == frame::RuntimeRole::Listen
                && let (Some(account), Some(local)) = (account.as_ref(), local.as_ref())
            {
                account
                    .bind_or_initialize(
                        sim.persistent_data_mut(),
                        local.0,
                        account_defaults.as_ref(),
                    )
                    .map_err(|error| {
                        InstallRefusal::new(format!("local account binding: {error:?}"))
                    })?;
            }
            for (name, value) in &script_dvars {
                sim.set_gsc_dvar(name, value);
            }
            stage_resource(&mut install, net::ArchiveBudget::from_dvars(&script_dvars));
            for entry in script_entries {
                sim.start_gsc(&entry, sim::script::Value::level(), Vec::new())
                    .map_err(|e| script_refusal(&zone, gametype, "entry", &e))?;
            }
            let load_hold = AuthorityLoadHold(true);
            sim.world_objects_mut()
                .set_map_round_epoch(match_key.match_epoch);
            spawn_script_model_movers(&mut sim, &model_spawns)?;
            for (id, cmodel, origin, angles) in brush_movers {
                sim.spawn_brush_mover(id, cmodel, origin, angles)
                    .map_err(|error| {
                        InstallRefusal::new(format!(
                            "brush model entity allocation refused: {error:?}"
                        ))
                    })?;
            }
            stamp_script_mover_numbers(&mut scene, &sim);
            if !model_spawns.is_empty() {
                diag::info!(
                    World,
                    "script_model spawn: {} ET_SCRIPTMOVER from {}",
                    sim.script_mover_count(),
                    sim::gentity_spawn_base()
                );
            }

            let class_handoff = ClassSelectHandoff {
                pending: !scene.batches.is_empty(),
                primary,
                secondary,
                lethal,
                tactical,
                excluded,
                lock_reasons,
            };

            let manifest = SessionContentManifest::build(
                &prepared_map,
                weapons.registry(),
                &sim_weapons,
                sim.content_digest(),
            )
            .map_err(|error| InstallRefusal::new(format!("Invalid content manifest: {error:?}")))?;
            stage_resource(&mut install, weapons);
            stage_resource(&mut install, killstreaks);

            let components = sim.content_components();
            diag::info!(
                Sim,
                "session manifest: ruleset=iw4 map={:?} weapons={} digest={:016x} \
             gameplay={:016x} c_map={:016x} c_models={:016x} c_weapons={:016x} c_classes={:016x}",
                manifest.map,
                manifest.weapons.len(),
                manifest.digest,
                sim.content_digest(),
                components.map,
                components.models,
                components.weapons,
                components.classes
            );
            match manifest.match_descriptor(components) {
                Some(descriptor) => {
                    diag::info!(
                        Sim,
                        "match descriptor: map={:016x} weapons={:016x} classes={:016x}",
                        descriptor.map,
                        descriptor.weapons,
                        descriptor.classes
                    );
                    stage_resource(&mut install, descriptor);
                }
                None => return Err(InstallRefusal::new("Required match descriptor is missing")),
            }
            stage_resource(&mut install, manifest);

            diag::info!(
                World,
                "match preparation: {:.1}ms on the main thread (scene handoff, sim boot, catalogs)",
                install_started.elapsed().as_secs_f32() * 1000.0
            );
            if policy.role.runs_client() {
                let client = local.as_ref().map_or(sim::ClientId(0), |local| local.0);
                let mut state = net::ClientPrediction::new(client);
                state.arm();
                stage_resource(&mut install, net::ClientLevel::from_world(&sim));
                stage_resource(&mut install, net::ClientCollision(sim.collision_scene()));
                stage_resource(&mut install, net::ClientPredictionState(state));
            }
            if policy.role.runs_authority() || *policy.role == frame::RuntimeRole::Replay {
                stage_resource(&mut install, AuthorityWorld(sim));
            }
            sim_cam.freeze_fly = true;
            stage_resource(&mut install, WorldProducts::from_walk(scene.products_id));
            stage_resource(&mut install, scene);
            install.push(move |world: &mut World| {
                world
                    .resource_mut::<crate::scope_control::InstallAnnouncement>()
                    .0
                    .push(move |world: &mut World| {
                        world.insert_resource(sim_cam);
                    });
            });
            install.push(move |world: &mut World| {
                world
                    .resource_mut::<crate::scope_control::InstallAnnouncement>()
                    .0
                    .push(move |world: &mut World| {
                        world.insert_resource(input_gate);
                    });
            });
            install.push(move |world: &mut World| {
                world
                    .resource_mut::<crate::scope_control::InstallAnnouncement>()
                    .0
                    .push(move |world: &mut World| {
                        world.insert_resource(load_hold);
                    });
            });
            install.push(move |world: &mut World| {
                world
                    .resource_mut::<crate::scope_control::InstallAnnouncement>()
                    .0
                    .push(move |world: &mut World| {
                        world.insert_resource(class_handoff);
                    });
            });
            stage_resource(&mut install, crate::LiveWorldIdentity { match_key });
            let installed_zone = zone.clone();
            install.push(move |world: &mut World| {
                world.resource_mut::<crate::scope_control::InstallAnnouncement>().0.push(move |world: &mut World| {
                if let Some(mut probe) = world.get_resource_mut::<LaunchReport>() {
                    probe.sim_gap = sim_gap.into();
                    probe.world_report = world_report;
                }
                diag::script_boundary("installed", &script_facts);
                diag::info!(Sim, "match: {} ({}) — world installed ({} dm spawn points); class select waits for admission", kind.display_name(), kind.token(), spawn_count);
                perf::instant!("match_installed", "iw4l.sim", |ctx| { let (zone, has_world,): (&str, i64,) = (&installed_zone, 1,);
    ctx.arg("zone", zone);
            ctx.arg("has_world", has_world);
    });
                frame::scope::insert(world, InstalledMatch {
                    match_key,
                    zone: installed_zone,
                    spawn_count,
                }, frame::MatchScope::Live);
                });
            });

            Ok(install)
        })();
        match prepared_install {
            Ok(install) => {
                commands.queue(move |world: &mut World| {
                    let accepts = world.resource::<crate::SessionSwapRequest>().can_install();
                    let same_match = match_key.is_none()
                        || world
                            .get_resource::<net::MasterBridge>()
                            .is_some_and(|bridge| {
                                bridge.state().in_match()
                                    && bridge.state().identity().match_key() == match_key
                            });
                    if accepts && same_match {
                        world
                            .resource_mut::<crate::ScopeControl>()
                            .install(match_key, install);
                    }
                });
                if let Some(stage) = install_stage {
                    stage.done();
                }
            }
            Err(refusal) => {
                diag::warn!(World, "match install refused: {}", refusal.error);
                if let Some(stage) = install_stage {
                    stage.fail();
                }
                if let Some(loading) = self.loading.as_deref_mut() {
                    loading.fail(refusal.error.clone());
                }
                failed.write(frame::MapLoadFailed {
                    match_key,
                    zone,
                    error: refusal.error,
                });
            }
        }
        console.release_startup();
    }
}

pub fn apply_prepared_match(
    commands: Commands,
    source: PreparedMatchSource,
    console: MatchInstallConsole,
    authority: MatchInstallAuthority,
    presentation: MatchInstallPresentation,
    policy: MatchInstallPolicy,
    failed: MessageWriter<frame::MapLoadFailed>,
) {
    source.install(commands, console, authority, presentation, policy, failed);
}

/// Everything the install publishes, assembled while a refusal is still free.
///
/// The plan exists so publication cannot start before the checks are done: the
/// commit half of `apply_prepared_match` has nothing to install until preflight
/// hands one over.
struct MatchInstallPlan {
    capabilities: assets::PreparedCapabilities,
    ui_images: asset_material::UiImagePublication,
    scripts: sim::script::Program,
    script_level: sim::script::LevelData,
    script_entries: Vec<String>,
    script_dvars: Vec<(String, String)>,
    account_defaults: Option<sim::PlayerDataDefaults>,
    script_sound_aliases: Option<std::collections::BTreeMap<String, Option<bool>>>,
    objective_weapons: Vec<(String, u32)>,
    kind: gamemode_iw4::GameModeKind,
    gametype: &'static str,
    scene: WorldScene,
    weapons: PreparedWeapons,
    killstreaks: assets::prepared::PreparedKillstreaks,
    fpv_meshes: PreparedFpvMeshes,
    bodies: assets::PreparedBodies,
    soldiers: asset_game::SoldierPresentations,
    world_weapons: assets::PreparedWorldWeapons,
    projectile_meshes: assets::PreparedProjectileMeshes,
    xmodel_walk: assets::PreparedXModelWalkCensus,
    xanims: PreparedXAnims,
    death: PreparedDestructibleDeath,
    player_anim_sources: asset_anim::PlayerAnimSources,
    prepared_map: assets::PreparedMap,
    strings: asset_game::LocalizeCatalog,
    clip: Option<Arc<ClipCollision>>,
    pen_table: weapon_iw4::PenetrationDepthTable,
    pen_table_loaded: bool,
    lochit_table: Option<[f32; weapon_iw4::HITLOC_COUNT]>,
    tracers: PreparedTracers,
    fx_catalog: PreparedFxCatalog,
    type10: MatchType10SoundHints,
    fx_models: PreparedFxModels,
    impact_fx: PreparedImpactFx,
    authority_models: AuthorityEntityModelInstall,
    model_spawns: Vec<(sim::ScriptModelId, [f32; 3], [f32; 3])>,
}

/// A refusal of the whole request: the loading screen shows `error`, the swap
/// completes on it, and nothing of the match has been published.
struct InstallRefusal {
    error: String,
}

impl InstallRefusal {
    fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
        }
    }
}

fn script_install_facts(
    zone: &str,
    gametype: &str,
    program: &sim::script::Program,
    entries: usize,
) -> String {
    let fingerprint: String = program
        .fingerprint()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(
        " map={zone} gametype={gametype} fingerprint={fingerprint} modules={} functions={} natives={} entries={entries}",
        program.modules().len(),
        program.function_count(),
        program.native_count(),
    )
}

const MATCH_CONFIG: &str = "default_xboxlive.cfg";

fn config_sets(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.split("//").next()?.trim();
            let rest = line
                .strip_prefix("set ")
                .or_else(|| line.strip_prefix("seta "))?
                .trim_start();
            let (name, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let value = value.trim().trim_matches('"');
            (!name.is_empty()).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect()
}

fn merged_rules(
    host: Option<&frame::HostMatchRules>,
    merged: &assets::MergedMap,
) -> frame::HostMatchRules {
    let mut rules = host.cloned().unwrap_or_else(frame::HostMatchRules::default);
    for (name, value) in &merged.scene.dvars {
        rules.0.retain(|(set, _)| !set.eq_ignore_ascii_case(name));
        rules.0.push((name.clone(), value.clone()));
    }
    rules
}

fn script_dvar_overrides() -> Vec<(String, String)> {
    std::env::var("IW4L_SCRIPT_DVARS")
        .map(|text| config_sets(&text.replace(';', "\n")))
        .unwrap_or(Vec::new())
}

fn script_refusal(
    zone: &str,
    gametype: &str,
    stage: &str,
    fault: &sim::script::Fault,
) -> InstallRefusal {
    let text = fault.to_string();
    let head = text.lines().next().unwrap_or("").replace('"', "'");
    diag::script_boundary(
        "refused",
        &format!(" map={zone} gametype={gametype} stage={stage} fault=\"{head}\""),
    );
    InstallRefusal::new(format!("GSC {stage}: {text}"))
}

fn preflight_match_install(
    mut prepared: assets::PreparedMatch,
    zone: &str,
    mode_selection: Option<&sim::HostGameModeSelection>,
    rules: Option<&frame::HostMatchRules>,
    catalog: Option<&asset_game::MenuCatalog>,
    identity: Option<&LaunchIdentity>,
    role: frame::RuntimeRole,
) -> Result<MatchInstallPlan, InstallRefusal> {
    if prepared
        .clip
        .as_ref()
        .is_none_or(|clip| clip.coverage != asset_world::CollisionCoverage::Complete)
    {
        return Err(InstallRefusal::new(
            "Required collision geometry has incomplete query coverage",
        ));
    }
    let capabilities = assets::PreparedCapabilities::prepare(&prepared);
    capabilities.validate(role).map_err(InstallRefusal::new)?;
    let killstreaks = assets::prepared::PreparedKillstreaks(
        prepared
            .scripts
            .tables()
            .get("mp/killstreaktable.csv")
            .map_or_else(Vec::new, |table| {
                let mut names: Vec<String> = (0..table.rows)
                    .filter_map(|row| {
                        let cells = table
                            .cells
                            .get(row * table.columns..(row + 1) * table.columns)?;
                        cells.get(4)?.parse::<i32>().ok()?;
                        let weapon = cells.get(12)?;
                        if weapon.is_empty()
                            || prepared.weapons.resolve_index(weapon).ok().flatten()? == 0
                        {
                            return None;
                        }
                        let name = cells.get(1)?;
                        (!name.is_empty()
                            && name != "none"
                            && sim::menu_response_field(name).is_some())
                        .then(|| name.clone())
                    })
                    .collect();
                names.sort();
                names.dedup();
                names
            }),
    );
    let weapons = PreparedWeapons::for_match(prepared.weapons);
    let fpv_meshes = PreparedFpvMeshes(prepared.fpv_meshes);
    let bodies = assets::PreparedBodies(prepared.bodies);
    let soldiers = prepared.soldiers;
    let world_weapons = assets::PreparedWorldWeapons(Arc::new(prepared.world_weapons));
    let projectile_meshes = assets::PreparedProjectileMeshes(prepared.projectile_meshes);
    let xmodel_walk = std::mem::take(&mut prepared.xmodel_walk);
    let xanims = PreparedXAnims(prepared.xanims);
    let death = PreparedDestructibleDeath(std::mem::take(&mut prepared.destructible_death));
    log_destructible_death_assets(&death);
    let player_anim_sources = prepared.player_anim_sources;
    if !soldiers.owned_by(&bodies.0, &fpv_meshes.0, &xanims.0, &player_anim_sources) {
        return Err(InstallRefusal::new(
            "soldier presentation owners differ from match catalogs",
        ));
    }
    let mut prepared_map = prepared.prepared_map;
    if prepared_map.namespace == Some(asset_core::FamilyId::IW4)
        && let Some(catalog) = catalog
        && let Some(table) = catalog.string_table(gamemode_iw4::FACTION_TABLE)
    {
        let arena = catalog
            .rawfile_text("mp/basemaps.arena")
            .map(str::to_owned)
            .or_else(|| identity.and_then(|id| asset_game::read_basemaps_arena(&id.games_root)));
        prepared_map.facts.team_settings = asset_game::team_settings_for_zone(
            table,
            asset_core::FamilyId::IW4,
            arena.as_deref(),
            zone,
        );
    }
    let kind = mode_selection.map_or_else(sim::host_game_mode_kind, |selection| selection.kind());
    let mut objective_weapons = Vec::new();
    let mut absent_effects = std::collections::BTreeSet::new();
    if let Some(namespace) = prepared_map.namespace
        && let Some(team_policy) = asset_game::team_family(namespace)
        && team_policy.bind_objectives
        && !matches!(
            kind,
            gamemode_iw4::GameModeKind::FreeForAll | gamemode_iw4::GameModeKind::TeamDeathmatch
        )
    {
        let catalog = catalog.ok_or_else(|| {
            InstallRefusal::new("IW4 menu catalog missing for objective bindings".to_owned())
        })?;
        let table = catalog
            .string_table(gamemode_iw4::FACTION_TABLE)
            .ok_or_else(|| {
                InstallRefusal::new("IW4 faction table missing for objective bindings".to_owned())
            })?;
        let (allies, axis) = iw4_charsets(catalog, identity, zone);
        let iw4 =
            asset_game::ObjectiveVisuals::from_faction_table(table, Some(&allies), Some(&axis));
        let realm = &prepared_map.facts.objective_visuals;
        let bindings = objective_bindings(&iw4, realm, kind, team_policy)?;
        absent_effects.extend(bindings.absent_effects);
        let scene = &mut prepared.world.map_xmodel_scene_assets;
        let mut served = Vec::new();
        let mut missing = Vec::new();
        let native_model = |asset: Option<&asset_world::MapXModelSceneAsset>| {
            matches!(asset,
                Some(asset_world::MapXModelSceneAsset::Captured { family, .. })
                    if Some(*family) == prepared_map.namespace)
        };
        for (from, to) in bindings.models {
            let native = native_model(scene.get_name(to));
            if !native || !scene.alias(from, to) {
                missing.push(to);
            } else {
                served.push(format!("{from}->{to}"));
            }
        }
        for (from, to) in bindings.optional_models {
            if native_model(scene.get_name(to)) && scene.alias(from, to) {
                served.push(format!("{from}->{to}"));
            }
        }
        for (from, to) in bindings.effects {
            match prepared.fx.alias_for_map(from, to) {
                true => served.push(format!("{from}->{to}")),
                false => missing.push(to),
            }
        }
        for (from, to) in bindings.weapons {
            let weapon = objective_weapon_binding(weapons.registry(), namespace, to);
            match weapon {
                Some(id) => {
                    objective_weapons.push((from.to_owned(), id));
                    served.push(format!("{from}->{}:{to}#{id}", namespace.as_str()));
                }
                None => missing.push(to),
            }
        }
        diag::info!(
            World,
            "objective visuals from the map's game: served={served:?} missing={missing:?}"
        );
        if !missing.is_empty() {
            return Err(InstallRefusal::new(format!(
                "objective assets missing from the map's game: {}",
                missing.join(", ")
            )));
        }
    }
    let strings = std::mem::take(&mut prepared.strings);
    struct Sources(assets::ScriptSources);
    impl sim::script::SourceResolver for Sources {
        fn read(&self, module: &str) -> Result<String, String> {
            self.0
                .read(module)
                .map(|bytes| sim::script::decode_source(&bytes))
        }
        fn read_bytes(&self, module: &str) -> Result<Vec<u8>, String> {
            self.0.read(module)
        }
        fn origin(&self, module: &str) -> sim::script::SourceOrigin {
            match self.0.origin(module) {
                Some(assets::ScriptSourceOrigin::Packaged) => sim::script::SourceOrigin::Packaged,
                Some(assets::ScriptSourceOrigin::BuiltIn) => sim::script::SourceOrigin::BuiltIn,
                None => sim::script::SourceOrigin::External,
            }
        }
    }
    let sources = Sources(std::mem::take(&mut prepared.scripts));
    let gametype = kind
        .script_tokens()
        .iter()
        .copied()
        .find(|token| {
            sources
                .0
                .read(&format!("maps/mp/gametypes/{token}"))
                .is_ok()
        })
        .unwrap_or(kind.token());
    let account_defaults =
        sources
            .0
            .config("mp/stats_init.cfg")
            .map(|config| sim::PlayerDataDefaults {
                config: config.to_owned(),
                class_names: std::array::from_fn(|index| {
                    let key = format!("CLASS_SLOT{}", index + 1);
                    strings
                        .raw_text(&key)
                        .filter(|bytes| bytes.first().is_some_and(|byte| *byte != 0))
                        .map_or_else(|| key.into_bytes(), |bytes| bytes.to_vec())
                }),
            });
    let keys = match sources.0.config("radiant/keys.txt") {
        Some(text) => sim::script::parse_radiant_keys(text).unwrap_or_else(|error| {
            diag::warn!(Sim, "gsc: radiant/keys.txt: {error}");
            Default::default()
        }),
        None => {
            diag::warn!(
                Sim,
                "gsc: radiant/keys.txt is not in the zones; map keys stay strings"
            );
            Default::default()
        }
    };
    let mut script_level = sim::script::LevelData {
        absent_effects,
        player_data_defaults: account_defaults.clone(),
        schemas: sources.0.schemas().clone(),
        entities: sim::script::parse_entity_string(sources.0.entities().unwrap_or("")),
        keys,
        tables: sources
            .0
            .tables()
            .iter()
            .map(|(name, table)| {
                let table = sim::script::StringTable {
                    columns: table.columns,
                    rows: table.rows,
                    cells: table.cells.clone(),
                };
                (name.clone(), table)
            })
            .collect(),
    };
    if let Some(table) = script_level.tables.get_mut("mp/statstable.csv") {
        let reference = asset_game::STATS_REF_COL as usize;
        let group = asset_game::STATS_GROUP_COL as usize;
        if table.columns > reference.max(group) {
            let mut known: std::collections::BTreeSet<_> = (0..table.rows)
                .filter_map(|row| table.cell(row, reference))
                .map(str::to_ascii_lowercase)
                .collect();
            for (_, source) in asset_game::WeaponUnlockContent::tables(weapons.registry().as_ref())
            {
                if !asset_game::is_stats_table_name(&source.name) {
                    continue;
                }
                for row in 0..source.rows as i32 {
                    let key = source.cell(row, asset_game::STATS_REF_COL);
                    let category = source.cell(row, asset_game::STATS_GROUP_COL);
                    if key.is_empty()
                        || !category.starts_with("weapon_")
                        || !known.insert(key.to_ascii_lowercase())
                    {
                        continue;
                    }
                    let mut cells = vec![String::new(); table.columns];
                    cells[reference] = key.to_owned();
                    cells[group] = category.to_owned();
                    table.cells.extend(cells);
                    table.rows += 1;
                }
            }
        }
    }
    let shared_ranks = weapons.registry().shared_rank_table().map_err(|error| {
        InstallRefusal::new(format!("Invalid shared rank progression: {error}"))
    })?;
    script_level.tables.insert(
        "mp/ranktable.csv".into(),
        sim::script::StringTable {
            columns: shared_ranks.columns,
            rows: shared_ranks.rows,
            cells: shared_ranks.cells.clone(),
        },
    );
    let shared_icons = weapons
        .registry()
        .shared_rank_icons()
        .map_err(|error| InstallRefusal::new(format!("Invalid shared rank icons: {error}")))?;
    script_level.tables.insert(
        "mp/rankicontable.csv".into(),
        sim::script::StringTable {
            columns: shared_icons.columns,
            rows: shared_icons.rows,
            cells: shared_icons.cells.clone(),
        },
    );
    let startup = sim::script::Iw4Startup::new(&sources, gametype, zone);
    let roots: Vec<&str> = startup.roots.iter().map(String::as_str).collect();
    let scripts = sim::script::Program::load(&sources, &roots, &sim::script::Catalog::iw4())
        .map_err(|e| script_refusal(zone, gametype, "compile", &e))?;
    let config = sources
        .0
        .config(MATCH_CONFIG)
        .or_else(|| catalog.and_then(|c| c.rawfile_text(MATCH_CONFIG)))
        .map(str::to_owned)
        .or_else(|| {
            identity
                .and_then(|id| asset_game::read_iwd_named(&id.games_root, MATCH_CONFIG))
                .and_then(|bytes| String::from_utf8(bytes).ok())
        });
    let mut script_dvars = match config {
        Some(text) => config_sets(&text),
        None => {
            diag::warn!(
                Sim,
                "gsc: {MATCH_CONFIG} is in neither the zones nor the iwds"
            );
            Vec::new()
        }
    };
    if !script_dvars
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("onlinegame"))
    {
        script_dvars.push(("onlinegame".into(), "1".into()));
    }
    script_dvars.push(("mapname".into(), zone.to_owned()));
    script_dvars.push(("g_gametype".into(), gametype.to_owned()));
    script_dvars.push(("sv_maxclients".into(), "18".into()));
    for (name, value) in rules.map_or(&[][..], |rules| &rules.0) {
        match script_dvars
            .iter_mut()
            .find(|(set, _)| set.eq_ignore_ascii_case(name))
        {
            Some((_, set)) => set.clone_from(value),
            None => script_dvars.push((name.clone(), value.clone())),
        }
    }
    if let Some(rules) = rules {
        diag::info!(
            Sim,
            "gsc: {} host rule dvars over {MATCH_CONFIG}",
            rules.0.len()
        );
    }
    script_dvars.extend(script_dvar_overrides());
    let script_entries = startup.entries;
    let authority_models = authority_entity_model_install(&prepared.world);
    let model_spawns = script_model_spawns(&prepared.world.script_model_instances);
    let fx_catalog = PreparedFxCatalog(std::mem::take(&mut prepared.fx));
    let type10 = MatchType10SoundHints(
        fx_catalog
            .0
            .unique_type10_sound_hints()
            .into_iter()
            .map(str::to_owned)
            .collect(),
    );
    let fx_models = PreparedFxModels(std::mem::take(&mut prepared.world.fx_models));
    let impact_fx = PreparedImpactFx {
        map: std::mem::take(&mut prepared.world.impact_fx),
        families: std::mem::take(&mut prepared.world.family_impact_fx),
    };
    let tracers = PreparedTracers(std::mem::take(&mut prepared.tracers));
    let scene = match world_scene_from_draw(
        prepared.world,
        prepared.materials,
        &authority_models.installed_owners,
    ) {
        Ok(scene) => scene,
        Err(error) => {
            let gap = format!("match install refused: {error}");
            diag::info!(World, "{gap}");
            return Err(InstallRefusal::new(gap));
        }
    };
    if scene.batches.is_empty() {
        diag::error!(
            World,
            "map load failed: no drawable world; see world report in the game log"
        );
        return Err(InstallRefusal::new(
            "No drawable world. See world report in the game log.",
        ));
    }
    Ok(MatchInstallPlan {
        capabilities,
        ui_images: prepared.ui_images,
        script_sound_aliases: prepared.script_sound_aliases,
        scripts,
        script_level,
        script_entries,
        script_dvars,
        account_defaults,
        objective_weapons,
        kind,
        gametype,
        scene,
        weapons,
        killstreaks,
        fpv_meshes,
        bodies,
        soldiers,
        world_weapons,
        projectile_meshes,
        xmodel_walk,
        xanims,
        death,
        player_anim_sources,
        prepared_map,
        strings,
        clip: prepared.clip,
        pen_table: prepared.pen_table,
        pen_table_loaded: prepared.pen_table_loaded,
        lochit_table: prepared.lochit_table,
        tracers,
        fx_catalog,
        type10,
        fx_models,
        impact_fx,
        authority_models,
        model_spawns,
    })
}

pub fn install_script_model_id(content: asset_world::ScriptModelId) -> sim::ScriptModelId {
    sim::ScriptModelId::from_authored_source_ordinal(content.source_ordinal())
}

struct ObjectiveBindings<'a> {
    models: Vec<(&'a str, &'a str)>,
    optional_models: Vec<(&'a str, &'a str)>,
    effects: Vec<(&'a str, &'a str)>,
    weapons: Vec<(&'a str, &'a str)>,
    absent_effects: Vec<String>,
}

fn objective_bindings<'a>(
    host: &'a asset_game::ObjectiveVisuals,
    native: &'a asset_game::ObjectiveVisuals,
    kind: gamemode_iw4::GameModeKind,
    policy: &asset_game::TeamFamily,
) -> Result<ObjectiveBindings<'a>, InstallRefusal> {
    use gamemode_iw4::GameModeKind;
    let pair = |from: &'a Option<String>, to: &'a Option<String>| {
        from.as_deref()
            .filter(|name| !name.is_empty())
            .zip(to.as_deref().filter(|name| !name.is_empty()))
            .ok_or_else(|| {
                InstallRefusal::new("required objective binding missing for the map's game")
            })
    };
    let mut bindings = ObjectiveBindings {
        models: Vec::new(),
        optional_models: host
            .crate_model
            .iter()
            .chain(&host.crate_overlay)
            .zip(native.crate_model.iter().chain(&native.crate_overlay))
            .filter_map(|(from, to)| Some((from.as_deref()?, to.as_deref()?)))
            .collect(),
        effects: Vec::new(),
        weapons: Vec::new(),
        absent_effects: Vec::new(),
    };
    match kind {
        GameModeKind::SearchAndDestroy | GameModeKind::Sabotage | GameModeKind::Demolition => {
            bindings.models.push(pair(&host.bomb, &native.bomb)?);
            bindings
                .effects
                .push(pair(&host.bomb_explosion_fx, &native.bomb_explosion_fx)?);
            bindings
                .weapons
                .push(pair(&host.plant_weapon, &native.plant_weapon)?);
            bindings
                .weapons
                .push(pair(&host.defuse_weapon, &native.defuse_weapon)?);
        }
        GameModeKind::Domination | GameModeKind::CaptureTheFlag => {
            if kind == GameModeKind::Domination {
                bindings
                    .models
                    .push(pair(&host.neutral_flag, &native.neutral_flag)?);
            }
            for index in 0..2 {
                bindings
                    .models
                    .push(pair(&host.flag[index], &native.flag[index])?);
                if kind == GameModeKind::CaptureTheFlag {
                    bindings
                        .models
                        .push(pair(&host.flag_carry[index], &native.flag_carry[index])?);
                }
                if policy.allow_absent_flag_base_fx && native.flag_base_fx[index].is_none() {
                    let from = host.flag_base_fx[index]
                        .as_ref()
                        .ok_or_else(|| InstallRefusal::new("host flag effect binding missing"))?;
                    bindings.absent_effects.push(from.clone());
                } else {
                    bindings.effects.push(pair(
                        &host.flag_base_fx[index],
                        &native.flag_base_fx[index],
                    )?);
                }
            }
        }
        _ => {}
    }
    Ok(bindings)
}

fn objective_weapon_binding(
    registry: &WeaponRegistry,
    family: asset_core::FamilyId,
    script_name: &str,
) -> Option<u32> {
    registry
        .published_weapons()
        .find(|weapon| {
            let id = weapon.wire_id();
            registry.identity_namespace_of(id) == Some(family)
                && registry.script_name_of(id) == script_name
                && [false, true]
                    .into_iter()
                    .all(|axis| registry.fpv_assemblies_of(id, axis).is_some())
        })
        .map(|weapon| weapon.wire_id())
}

fn player_kit_collision(
    soldier: Option<&asset_game::SoldierPresentation>,
) -> Result<sim::PlayerKitCollision, InstallRefusal> {
    let Some(soldier) = soldier else {
        return Ok(sim::PlayerKitCollision::default());
    };
    let head = soldier.head().map_err(|reason| {
        InstallRefusal::new(format!("soldier head collision refused: {reason}"))
    })?;
    Ok(sim::PlayerKitCollision {
        body_key: soldier.kit().body.clone(),
        body: soldier.body().skel.retained_capability().map(Arc::new),
        head_key: soldier.kit().head.clone().unwrap_or(String::new()),
        head: head
            .and_then(|head| head.entry().skel.retained_capability())
            .map(Arc::new),
    })
}

fn script_model_spawns(
    instances: &[asset_world::ScriptModelSceneInstance],
) -> Vec<(sim::ScriptModelId, [f32; 3], [f32; 3])> {
    instances
        .iter()
        .map(|instance| {
            let (yaw, pitch, roll) = instance.transform.rotation.to_euler(EulerRot::ZYX);
            (
                install_script_model_id(instance.id),
                instance.transform.translation.to_array(),
                [pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees()],
            )
        })
        .collect()
}

fn spawn_script_model_movers(
    sim: &mut sim::SimWorld,
    models: &[(sim::ScriptModelId, [f32; 3], [f32; 3])],
) -> Result<(), InstallRefusal> {
    for (id, origin, angles) in models {
        sim.spawn_script_mover(*id, *origin, *angles)
            .map_err(|error| {
                InstallRefusal::new(format!("script model entity allocation refused: {error:?}"))
            })?;
    }
    Ok(())
}

fn stamp_script_mover_numbers(scene: &mut WorldScene, sim: &sim::SimWorld) {
    for instance in &mut scene.script_model_instances {
        let id = install_script_model_id(instance.id);
        instance.gentity_number = sim.gentity_number(id).and_then(|n| u16::try_from(n).ok());
    }
}

fn map_anim_items(
    slots: &[(u8, u8, Vec<asset_anim::ParsedAnimItem>)],
    tree: Option<&asset_anim::CompiledAnimTreeDefinition>,
    catalog: &asset_anim::XAnimCatalog,
) -> Vec<(u8, u8, Vec<sim::AnimScriptItem>)> {
    slots
        .iter()
        .map(|(state, movetype, items)| {
            (
                *state,
                *movetype,
                items
                    .iter()
                    .map(|item| map_script_item(item, tree, catalog))
                    .collect(),
            )
        })
        .collect()
}

fn map_event_items(
    events: &[(u8, Vec<asset_anim::ParsedAnimItem>)],
    tree: Option<&asset_anim::CompiledAnimTreeDefinition>,
    catalog: &asset_anim::XAnimCatalog,
) -> Vec<(u8, Vec<sim::AnimScriptItem>)> {
    events
        .iter()
        .map(|(event, items)| {
            (
                *event,
                items
                    .iter()
                    .map(|item| map_script_item(item, tree, catalog))
                    .collect(),
            )
        })
        .collect()
}

fn map_script_item(
    item: &asset_anim::ParsedAnimItem,
    tree: Option<&asset_anim::CompiledAnimTreeDefinition>,
    catalog: &asset_anim::XAnimCatalog,
) -> sim::AnimScriptItem {
    sim::AnimScriptItem {
        skip: item.skip,
        conditions: item
            .conditions
            .iter()
            .map(|c| sim::AnimScriptCondition {
                index: c.index,
                bitflags: c.bitflags,
                bits: c.bits,
                value: c.value,
            })
            .collect(),
        commands: item
            .commands
            .iter()
            .map(|cmd| sim::AnimScriptCommand {
                body_part: cmd.body_part,
                anim_index: cmd.anim_index,
                duration_ms: command_duration_ms(tree, catalog, cmd),
            })
            .collect(),
    }
}

fn command_duration_ms(
    tree: Option<&asset_anim::CompiledAnimTreeDefinition>,
    catalog: &asset_anim::XAnimCatalog,
    cmd: &asset_anim::ParsedAnimCommand,
) -> i32 {
    if let Some(ms) = cmd.duration_ms {
        return if ms <= 0 { 500 } else { ms };
    }
    let Some(node) = tree.and_then(|t| t.node(cmd.anim_index)) else {
        return 500;
    };
    let Some(captured) = catalog.get(asset_core::FamilyId::IW4, &node.name) else {
        return 500;
    };
    if captured.parts.framerate > 0.0 {
        let ms = (captured.parts.numframes as f32 / captured.parts.framerate * 1000.0) as i32;
        if ms <= 0 { 500 } else { ms }
    } else {
        500
    }
}

fn log_destructible_death_assets(death: &PreparedDestructibleDeath) {
    for row in &death.0 {
        diag::info!(
            World,
            "destructible death clip {}: {}",
            row.clip_hint,
            row.clip.edge_kind()
        );
        diag::info!(
            World,
            "destructible death husk {}: {}",
            row.husk_hint,
            row.husk.edge_kind()
        );
    }
}

struct AuthorityEntityModelInstall {
    capabilities: Vec<sim::EntityCollisionCapabilities>,
    brush_movers: Vec<(sim::ScriptModelId, u32, [f32; 3], [f32; 3])>,
    models: std::collections::BTreeMap<
        String,
        Option<std::sync::Arc<xmodel_runtime::RetainedModelCapability>>,
    >,
    installed_owners: Vec<(asset_world::ScriptModelId, sim::AuthorityModelOwner)>,
    ambiguous_brush_links: usize,
    standalone_brush_links: usize,
}

fn retained(
    asset: Option<&asset_world::MapXModelSceneAsset>,
) -> Option<std::sync::Arc<xmodel_runtime::RetainedModelCapability>> {
    match asset {
        Some(asset_world::MapXModelSceneAsset::Captured { skel, .. }) => {
            skel.retained_capability().map(std::sync::Arc::new)
        }
        Some(asset_world::MapXModelSceneAsset::Unavailable { .. }) | None => None,
    }
}

fn authority_entity_model_install(world: &assets::PreparedWorld) -> AuthorityEntityModelInstall {
    let mut installed_owners = Vec::new();
    let mut ambiguous_brush_links = 0;
    let mut capabilities: Vec<sim::EntityCollisionCapabilities> = world
        .script_model_instances
        .iter()
        .map(|instance| {
            let sim_id = install_script_model_id(instance.id);
            let owner = sim::AuthorityModelOwner::ScriptModel(sim_id);
            installed_owners.push((instance.id, owner));
            if matches!(
                instance.metadata.brush_link,
                asset_world::ScriptBrushModelLink::Ambiguous { .. }
            ) {
                ambiguous_brush_links += 1;
            }
            let mut dobj = sim::AuthorityDObjState::new_dirty(
                instance.current_model.0.clone(),
                retained(world.map_xmodel_scene_assets.get(&instance.current_model)),
                instance.transform.to_matrix(),
            );
            dobj.semantic_state.hide_part_bits = instance.dobj_state.hide_part_bits;
            dobj.pose_request.hide_part_bits = instance.dobj_state.hide_part_bits;
            let family = match world.map_xmodel_scene_assets.get(&instance.current_model) {
                Some(asset_world::MapXModelSceneAsset::Captured { family, .. }) => Some(*family),
                _ => world.source_namespace,
            };
            if let Some(family) = family {
                sim::model_behaviors::initialize(family, &mut dobj, &instance.metadata);
            }
            sim::EntityCollisionCapabilities::current_tick(owner, Some(dobj), Vec::new())
        })
        .collect();
    let mut brush_movers = Vec::new();
    for brush in &world.script_brush_models {
        let sim_id = install_script_model_id(asset_world::ScriptModelId::from_source_ordinal(
            brush.source_ordinal,
        ));
        capabilities.push(sim::EntityCollisionCapabilities::current_tick(
            sim::AuthorityModelOwner::ScriptModel(sim_id),
            None,
            vec![sim::LinkedBrushCollisionBrush {
                cmodel_handle: brush.cmodel_handle,
                origin: brush.origin,
                angles: brush.angles,
            }],
        ));
        brush_movers.push((sim_id, brush.cmodel_handle, brush.origin, brush.angles));
    }
    let standalone_brush_links = brush_movers.len();
    let models = world
        .map_xmodel_scene_assets
        .iter()
        .map(|(key, asset)| (key.0.clone(), retained(Some(asset))))
        .collect();
    AuthorityEntityModelInstall {
        capabilities,
        brush_movers,
        models,
        installed_owners,
        ambiguous_brush_links,
        standalone_brush_links,
    }
}

/// A map's world collision in the simulation's types: brushes, BSP, mesh and
/// cmodels, in the map's native coordinates. The match installs it; tools
/// that read a map without a match trace it with `sim::trace_world_clip`.
pub fn sim_clip_map(
    clip: &ClipCollision,
) -> (
    Vec<sim::SimBrush>,
    sim::SimClipBsp,
    sim::SimClipMesh,
    sim::SimClipCmodels,
) {
    let brushes: Vec<sim::SimBrush> = clip
        .brushes
        .iter()
        .map(|brush| sim::SimBrush {
            planes: brush.planes.clone(),
            contents: brush.contents,
            plane_surface_flags: brush.plane_surface_flags.clone(),
            glass_encoded: brush.glass_encoded,
        })
        .collect();
    let bsp = sim::SimClipBsp {
        nodes: clip
            .nodes
            .iter()
            .map(|n| clipmap_iw4::ClipNode {
                plane: n.plane,
                children: n.children,
            })
            .collect(),
        leaves: clip
            .leaves
            .iter()
            .map(|l| clipmap_iw4::ClipLeaf {
                first_brush: l.first_brush,
                num_brushes: l.num_brushes,
                first_coll_aabb_index: l.first_coll_aabb_index,
                coll_aabb_count: l.coll_aabb_count,
            })
            .collect(),
        leafbrushes: clip.leafbrushes.clone(),
    };
    let mesh = sim::SimClipMesh {
        tables: Arc::clone(&clip.mesh),
        static_models: clip
            .static_models
            .iter()
            .map(|sm| sim::SimStaticModel {
                index: sm.index,
                name: sm.name.clone(),
                model: sm.model.clone(),
            })
            .collect(),
        ..Default::default()
    };
    let cmodels = sim::SimClipCmodels {
        models: clip
            .cmodels
            .iter()
            .map(|c| clipmap_iw4::ClipCmodel {
                mins: c.mins,
                maxs: c.maxs,
                radius: c.radius,
                first_brush: c.first_brush,
                num_brushes: c.num_brushes,
            })
            .collect(),
        triggers: clip
            .trigger_models
            .iter()
            .map(|hulls| {
                hulls
                    .iter()
                    .map(|h| sim::SimTriggerHull {
                        mid: h.mid,
                        half: h.half,
                        slabs: h.slabs.clone(),
                    })
                    .collect()
            })
            .collect(),
    };
    (brushes, bsp, mesh, cmodels)
}

struct MatchMapBootstrap<'a> {
    clip: Arc<ClipCollision>,
    intermission_view: Option<sim::AuthoredSpawnPoint>,
    airstrike_height: Option<f32>,
    spawns: &'a [SpawnPoint],
}

struct MatchPlayerBootstrap<'a> {
    weapons: &'a WeaponRegistry,
    sim_weapons: &'a weapon_content::PreparedSimWeapons,
    host_classes: Option<&'a HostClassLoadouts>,
}

/// What the two runtime owners of the map collision are given: one immutable
/// backing they share, and the static models only the sim traces against.
fn install_match_collision(
    sim: &mut sim::SimWorld,
    mut content: sim::SimContentBuilder,
    clip: Option<Arc<ClipCollision>>,
    authority_models: AuthorityEntityModelInstall,
    weapons: &WeaponRegistry,
    sim_weapons: &weapon_content::PreparedSimWeapons,
) -> Result<Arc<ClipCollision>, InstallRefusal> {
    if !sim_weapons.owned_by(weapons) || !content.uses_weapons(sim_weapons.content()) {
        return Err(InstallRefusal::new(
            "Simulation weapon registry owner mismatch",
        ));
    }
    let clip = clip.ok_or_else(|| InstallRefusal::new("Required collision geometry is missing"))?;
    let static_models = &clip.static_models;
    let count = clip.brushes.len();
    let node_count = clip.nodes.len();
    let leaf_count = clip.leaves.len();
    let vert_count = clip.mesh.verts.len();
    let tri_count = clip.mesh.tri_count();
    let smodel_count = static_models.len();
    if count == 0 && tri_count == 0 {
        return Err(InstallRefusal::new("Required collision geometry is empty"));
    }
    let (brushes, bsp, mesh, cmodels) = sim_clip_map(&clip);
    let cmodel_count = cmodels.models.len();
    content.set_clip_map(brushes, bsp, mesh, cmodels);
    content.set_collision_complete(clip.coverage == asset_world::CollisionCoverage::Complete);
    sim.install_content(content.finish());
    sim.install_model_library(authority_models.models);
    let owner_count = authority_models.capabilities.len();
    let linked_brushes = authority_models
        .capabilities
        .iter()
        .map(|capabilities| capabilities.linked_brushes.len())
        .sum::<usize>();
    sim.install_entity_collision_capabilities(authority_models.capabilities);
    sim::phase_materialize_entity_dobjs(sim);
    let collision_bones = sim
        .entity_collision_capabilities()
        .iter()
        .filter_map(|capabilities| capabilities.dobj.as_ref())
        .filter_map(|state| state.current_collision.as_ref())
        .map(|geometry| geometry.bones.len())
        .sum::<usize>();
    diag::info!(
        Sim,
        "spawn: {} script collision owners, {} nonzero XBoneInfo OBBs, {} linked brush cmodels, {} ambiguous brush links, {} standalone brush cmodels",
        owner_count,
        collision_bones,
        linked_brushes,
        authority_models.ambiguous_brush_links,
        authority_models.standalone_brush_links,
    );

    diag::info!(
        Sim,
        "spawn: {} clip brushes, {} BSP nodes / {} leaves, {} verts / {} tris, {} cmodels, {} smodels",
        count,
        node_count,
        leaf_count,
        vert_count,
        tri_count,
        cmodel_count,
        smodel_count
    );

    Ok(clip)
}

impl MatchPlayerBootstrap<'_> {
    fn bootstrap(
        self,
        sim: &mut sim::SimWorld,
        map: MatchMapBootstrap,
        sim_cam: &mut SimCamera,
        input_gate: &mut AuthorityInputGate,
        kind: gamemode_iw4::GameModeKind,
        allow_debug_actions: bool,
    ) -> Result<(&'static str, Vec<Option<String>>), InstallRefusal> {
        let count = map.clip.brushes.len();
        let spawns: Vec<sim::AuthoredSpawnPoint> = map
            .spawns
            .iter()
            .map(|p| sim::AuthoredSpawnPoint {
                classname: p.classname.clone(),
                origin: p.origin,
                angles: p.angles,
                script_linkto: p.script_linkto.clone(),
                script_destructable_area: p.script_destructable_area.clone(),
            })
            .collect();
        let rows = bootstrap_class_rows(self.host_classes);
        let projected: Vec<AuthoritativeClassProjection> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| project_class(index as u32, row, self.weapons, self.sim_weapons))
            .collect();
        let lock_reasons: Vec<Option<String>> = projected
            .iter()
            .map(|row| row.lock_reason.clone())
            .collect();
        let classes: Vec<sim::ClassDef> = projected.into_iter().map(|row| row.def).collect();
        let bot_classes: Vec<sim::ClassDef> = frame::showcase_classes()
            .iter()
            .enumerate()
            .map(|(index, preset)| {
                let row = ClassRow::from(&preset.into());
                project_class(index as u32, &row, self.weapons, self.sim_weapons).def
            })
            .filter(|class| !class.locked)
            .collect();
        let locked_n = lock_reasons.iter().filter(|r| r.is_some()).count();
        let has_intermission_view = map.intermission_view.is_some();
        if let Err(err) = sim.bootstrap(sim::MatchBootstrap {
            spawns,
            classes,
            bot_classes,
            seed: 0,
            kind,
            score_limit: std::env::var("IW4L_SCORE_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(match kind {
                    gamemode_iw4::GameModeKind::Domination => gamemode_iw4::dom::SCORE_LIMIT,
                    gamemode_iw4::GameModeKind::Demolition => 0,
                    _ => sim::FFA.score_limit,
                }),
            time_limit_ms: match kind {
                gamemode_iw4::GameModeKind::Domination => gamemode_iw4::dom::TIME_LIMIT_MS,
                gamemode_iw4::GameModeKind::Demolition => gamemode_iw4::dd::TIME_LIMIT_MS,
                _ => sim::FFA.time_limit_ms,
            },
            allow_debug_actions,
            intermission_view: map.intermission_view,
            airstrike_height: map.airstrike_height,
        }) {
            diag::info!(Sim, "bootstrap: {err}");
            return Err(InstallRefusal::new(format!(
                "MatchBootstrap refused: {err}"
            )));
        }

        sim_cam.enabled = false;
        input_gate.local_cmds_enabled = false;
        sim_cam.freeze_fly = has_intermission_view;
        diag::info!(
            Sim,
            "spawn: match bootstrapped with {} clip brushes and {} dm spawns ({} / {} classes Locked by coverage)",
            count,
            map.spawns.len(),
            locked_n,
            rows.len()
        );
        let gap = if has_intermission_view {
            "player awaits SelectClass; mp_global_intermission holds the camera until Equip ack"
        } else {
            "player awaits SelectClass; Equip ack enables sim camera (no authored intermission)"
        };
        Ok((gap, lock_reasons))
    }
}

pub(crate) fn bootstrap_class_rows(host: Option<&HostClassLoadouts>) -> Vec<ClassRow> {
    let fallback = HostClassLoadouts::default();
    let host = host.filter(|h| !h.slots.is_empty()).unwrap_or(&fallback);
    host.slots.iter().map(ClassRow::from).collect()
}

fn install_shocks(
    world: &mut sim::SimContentBuilder,
    catalog: Option<&asset_game::MenuCatalog>,
    map_shocks: &[(String, String)],
) {
    let common = catalog.into_iter().flat_map(|catalog| {
        catalog.rawfiles.iter().filter_map(|(path, text)| {
            let lower = path.to_ascii_lowercase();
            let name = lower
                .strip_prefix("shock/")?
                .strip_suffix(".shock")?
                .to_owned();
            Some((name, text.as_str()))
        })
    });
    let map = map_shocks
        .iter()
        .map(|(name, text)| (name.clone(), text.as_str()));
    let mut shocks = std::collections::BTreeMap::new();
    for (name, text) in common.chain(map) {
        match hud_iw4::ShockParams::parse(text) {
            Ok(params) => {
                shocks.insert(name, params);
            }
            Err(error) => diag::warn!(Zone, "shock/{name}.shock: {error}"),
        }
    }
    diag::info!(Zone, "shellshocks: {:?}", shocks.keys().collect::<Vec<_>>());
    world.set_shocks(shocks);
}

fn charsets(arena_text: Option<&str>, zone: &str) -> (String, String) {
    let row = arena_text.and_then(|text| asset_game::arena_charsets(text, zone));
    let allies = row
        .as_ref()
        .and_then(|r| r.allieschar.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| gamemode_iw4::DEFAULT_ALLIES_CHARSET.to_owned());
    let axis = row
        .as_ref()
        .and_then(|r| r.axischar.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| gamemode_iw4::DEFAULT_AXIS_CHARSET.to_owned());
    (allies, axis)
}

fn iw4_charsets(
    catalog: &asset_game::MenuCatalog,
    identity: Option<&LaunchIdentity>,
    zone: &str,
) -> (String, String) {
    let arena_text = catalog
        .rawfile_text("mp/basemaps.arena")
        .map(str::to_owned)
        .or_else(|| identity.and_then(|id| asset_game::read_basemaps_arena(&id.games_root)));
    charsets(arena_text.as_deref(), zone)
}

fn install_team_voice_prefixes(
    world: &mut sim::SimContentBuilder,
    catalog: Option<&asset_game::MenuCatalog>,
    identity: Option<&LaunchIdentity>,
    zone: &str,
) {
    let Some(catalog) = catalog else {
        return;
    };
    let Some(table) = catalog.string_table(gamemode_iw4::FACTION_TABLE) else {
        return;
    };
    let arena_text = catalog
        .rawfile_text("mp/basemaps.arena")
        .map(str::to_owned)
        .or_else(|| identity.and_then(|id| asset_game::read_basemaps_arena(&id.games_root)));
    if let Some(entry) = arena_text
        .as_deref()
        .and_then(|text| asset_game::arena_entry(text, zone))
    {
        world.set_map_custom(entry);
    }
    let (allies_cs, axis_cs) = charsets(arena_text.as_deref(), zone);
    let allies = table.lookup_col(&allies_cs, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    let axis = table.lookup_col(&axis_cs, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    world.set_team_voice_prefixes(
        (!allies.is_empty()).then(|| allies.to_owned()),
        (!axis.is_empty()).then(|| axis.to_owned()),
    );
}

pub fn register_match_apply_systems(app: &mut App) {
    app.init_resource::<StartupCommands>()
        .init_resource::<PendingConsoleLines>()
        .init_resource::<HostClassLoadouts>()
        .add_systems(
            PreUpdate,
            apply_prepared_match
                .after(frame::SessionSwapApplied)
                .run_if(resource_exists::<PreparedMatchReady>)
                .in_set(ClientSet::Load),
        );
}

fn stage_resource<T: Resource>(queue: &mut bevy::ecs::world::CommandQueue, resource: T) {
    queue.push(move |world: &mut World| {
        frame::scope::stage(world, resource);
    });
}
