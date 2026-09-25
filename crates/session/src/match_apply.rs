use assets::{
    ClipCollision, LoadingScreen, MatchLoadAbort, MatchType10SoundHints, PreparedDestructibleDeath,
    PreparedFpvMeshes, PreparedMatchReady, PreparedWeapons, PreparedXAnims, SpawnPoint,
    WeaponRegistry,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::{
    CacWeaponOffer, ClassSelectHandoff, HasWorld, HostClassLoadouts, LaunchIdentity, LaunchReport,
    MatchInstalled, WorldGeneration, WorldProducts,
};
use net::{AuthorityInputGate, AuthorityLoadHold, AuthorityWorld, ClientSet};

use crate::SessionContentManifest;
use crate::combat_table;
use render_frontend::adapters::anim::dyn_ent::{DynEntPhysClip, DynEntPhysWorld};
use render_frontend::prepare::scene::camera::SimCamera;
use render_frontend::prepare::scene::world::{WorldScene, world_scene_from_draw};
use render_fx::{PreparedFxCatalog, PreparedFxModels, PreparedImpactFx, PreparedTracers};
use std::sync::Arc;

#[derive(Resource, Default)]
pub struct StartupCommands {
    pub lines: Vec<String>,
}

#[derive(Resource, Default)]
pub struct PendingConsoleLines(pub Vec<String>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoritativeClassProjection {
    pub def: sim::ClassDef,
    pub lock_reason: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassRow {
    pub weapons: [String; 4],
    pub attachments: [Vec<String>; 2],
    pub perks: [String; 3],
    pub deathstreak: String,
}

pub fn resolve_class_weapon(
    weapons: &WeaponRegistry,
    name: &str,
    attachments: &[String],
    rules: assets::LoadoutRules,
) -> Result<u32, String> {
    if name.is_empty() {
        return if attachments.is_empty() {
            Ok(0)
        } else {
            Err("weapon.unknown_family".to_owned())
        };
    }
    let resolve = |selection: assets::WeaponSelection| {
        weapons
            .resolve_configuration(&selection, rules)
            .map(|resolved| resolved.id)
            .map_err(|refusal| format!("{name}:{}", refusal.code()))
    };
    if let Some(family) = assets::FamilyKey::parse(name)
        && weapons.weapon_families().family(&family).is_some()
    {
        return resolve(assets::WeaponSelection::with(family, attachments));
    }
    let id = match weapons.resolve_index(name) {
        Ok(Some(id)) => id,
        Ok(None) | Err(_) => return Err(format!("{name}:catalog.unknown")),
    };
    match weapons.describe_configuration(id) {
        Some(described) if described.family.is_some() => {
            let mut selection = described.clone();
            for extra in attachments {
                if !selection.attachments.contains(extra) {
                    selection.attachments.push(extra.clone());
                }
            }
            resolve(selection)
        }
        _ if attachments.is_empty() => weapons
            .configuration_admission(id)
            .map(|()| id)
            .map_err(|refusal| format!("{name}:{}", refusal.code())),
        _ => Err(format!("{name}:weapon.unknown_family")),
    }
}

pub fn authoritative_class_lock_reason(
    names: [&str; 4],
    ids: [u32; 4],
    combat: &[sim::WeaponCombatFacts],
    equipment: &[sim::EquipmentRuntimeFacts],
) -> Option<String> {
    let mut reason = None;
    for (name, id) in names[..2].iter().zip(&ids[..2]) {
        if *id == 0 {
            continue;
        }
        if !combat
            .get(*id as usize)
            .copied()
            .is_some_and(sim::WeaponCombatFacts::is_usable)
        {
            append_lock_reason(&mut reason, format!("{name}:validated.missing_profile"));
        }
    }
    for (name, id) in names[2..].iter().zip(&ids[2..]) {
        if *id == 0 {
            continue;
        }
        if !equipment
            .get(*id as usize)
            .copied()
            .is_some_and(sim::EquipmentRuntimeFacts::is_offhand)
        {
            append_lock_reason(&mut reason, format!("{name}:offhand.missing_runtime_facts"));
        }
    }
    reason
}

fn append_lock_reason(reason: &mut Option<String>, item: String) {
    match reason {
        Some(reason) => {
            reason.push_str("; ");
            reason.push_str(&item);
        }
        None => *reason = Some(item),
    }
}

pub fn perk_catalog_id(reference: &str) -> Option<u32> {
    const PERKS: &[&str] = &[
        "specialty_bulletdamage",
        "specialty_fastreload",
        "specialty_coldblooded",
        "specialty_lightweight",
        "specialty_scavenger",
        "specialty_hardline",
        "specialty_heartbreaker",
        "specialty_marathon",
        "specialty_explosivedamage",
        "specialty_extendedmelee",
        "specialty_bulletaccuracy",
        "specialty_bling",
        "specialty_onemanarmy",
        "specialty_localjammer",
        "specialty_detectexplosive",
        "specialty_pistoldeath",
    ];
    PERKS
        .iter()
        .position(|perk| perk.eq_ignore_ascii_case(reference))
        .map(|index| index as u32 + 1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerkRuntimeContract {
    PortExact { rust_owner: &'static str },

    BlockedEvidence { gap_id: &'static str },
}

pub fn perk_runtime_contract(catalog_id: u32) -> Option<PerkRuntimeContract> {
    match catalog_id {
        0 => None,
        2 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "sim::perk_bits_from_class_catalog",
        }),
        1 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "gamemode_iw4::cac_modified_damage",
        }),
        3 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "hud_iw4::radar_contact_trail_visible",
        }),
        4 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "gamemode_iw4::lightweight_move_speed_scale",
        }),
        5 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "sim::item::PERK_SCAVENGER",
        }),
        6 => Some(PerkRuntimeContract::BlockedEvidence {
            gap_id: "perk.hardline",
        }),
        7 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "audio::footstep_aliases",
        }),
        8 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "movement_iw4::sprint_time_remaining",
        }),
        9 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "gamemode_iw4::cac_modified_damage",
        }),
        10 => Some(PerkRuntimeContract::BlockedEvidence {
            gap_id: "perk.commando",
        }),
        11 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "weapon_iw4::perk_weap_spread_multiplier",
        }),
        12 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "gsc:specialty_bling has no perkSetFuncs",
        }),
        13 => Some(PerkRuntimeContract::BlockedEvidence {
            gap_id: "perk.one-man-army",
        }),
        14 => Some(PerkRuntimeContract::PortExact {
            rust_owner: "hud_iw4::cg_radar_jam_intensity",
        }),
        15 => Some(PerkRuntimeContract::BlockedEvidence {
            gap_id: "perk.sitrep",
        }),
        16 => Some(PerkRuntimeContract::BlockedEvidence {
            gap_id: "perk.last-stand",
        }),
        _ => None,
    }
}

pub fn deathstreak_runtime_contract(name: &str) -> Option<PerkRuntimeContract> {
    if name.is_empty() || name == "specialty_null" {
        return None;
    }
    match name {
        "specialty_combathigh" => Some(PerkRuntimeContract::PortExact {
            rust_owner: "gamemode_iw4::cac_modified_damage",
        }),
        "specialty_finalstand" => Some(PerkRuntimeContract::PortExact {
            rust_owner: "sim::apply_damage_attempt",
        }),
        "specialty_copycat" => Some(PerkRuntimeContract::PortExact {
            rust_owner: "sim::apply_use_copycat",
        }),
        _ => Some(PerkRuntimeContract::BlockedEvidence {
            gap_id: "perk.deathstreak",
        }),
    }
}

pub fn deathstreak_lock_reason(name: &str) -> Option<String> {
    match deathstreak_runtime_contract(name) {
        None | Some(PerkRuntimeContract::PortExact { .. }) => None,
        Some(PerkRuntimeContract::BlockedEvidence { gap_id }) => {
            Some(format!("{name}:deathstreak.blocked:{gap_id}"))
        }
    }
}

pub fn project_class(
    class_id: u32,
    row: &ClassRow,
    weapons: &WeaponRegistry,
    combat: &[sim::WeaponCombatFacts],
    equipment: &[sim::EquipmentRuntimeFacts],
) -> AuthoritativeClassProjection {
    let rules = assets::LoadoutRules::for_class(&row.perks[0]);
    let mut lock_reason = None;
    let no_attachments = Vec::new();
    let mut ids = [0u32; 4];
    for (slot, id) in ids.iter_mut().enumerate() {
        let attachments = row.attachments.get(slot).unwrap_or(&no_attachments);
        match resolve_class_weapon(weapons, &row.weapons[slot], attachments, rules) {
            Ok(resolved) => *id = resolved,
            Err(reason) => append_lock_reason(&mut lock_reason, reason),
        }
    }
    let names = [
        row.weapons[0].as_str(),
        row.weapons[1].as_str(),
        row.weapons[2].as_str(),
        row.weapons[3].as_str(),
    ];
    let mut def = sim::ClassDef::primary_secondary(sim::ClassId(class_id), 1, ids[0], ids[1]);
    def.lethal = ids[2];
    def.tactical = ids[3];
    if let Some(reason) = authoritative_class_lock_reason(names, ids, combat, equipment) {
        append_lock_reason(&mut lock_reason, reason);
    }
    let perks = [
        row.perks[0].as_str(),
        row.perks[1].as_str(),
        row.perks[2].as_str(),
    ];
    let deathstreak = row.deathstreak.as_str();
    for (slot, perk) in def.perks.iter_mut().zip(perks) {
        if perk.is_empty() || perk == "specialty_null" {
            continue;
        }
        match perk_catalog_id(perk) {
            Some(id) => {
                *slot = id;
                match perk_runtime_contract(id) {
                    Some(PerkRuntimeContract::PortExact { .. }) => {}
                    Some(PerkRuntimeContract::BlockedEvidence { gap_id }) => {
                        append_lock_reason(
                            &mut lock_reason,
                            format!("{perk}:perk.blocked:{gap_id}"),
                        );
                    }
                    None => append_lock_reason(
                        &mut lock_reason,
                        format!("{perk}:perk.unknown_catalog_entry"),
                    ),
                }
            }
            None => append_lock_reason(
                &mut lock_reason,
                format!("{perk}:perk.unknown_catalog_entry"),
            ),
        }
    }
    def.deathstreak = if deathstreak.is_empty() || deathstreak == "specialty_null" {
        String::new()
    } else {
        deathstreak.to_owned()
    };
    if let Some(reason) = deathstreak_lock_reason(deathstreak) {
        append_lock_reason(&mut lock_reason, reason);
    }
    def.locked = lock_reason.is_some();
    AuthoritativeClassProjection { def, lock_reason }
}

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
    swap: Res<'w, crate::SessionSwapRequest>,
    bridge: Option<Res<'w, net::MasterBridge>>,
    intent: Option<Res<'w, net::MasterLaunchIntent>>,
    loading: Option<ResMut<'w, LoadingScreen>>,
    load: Option<Res<'w, assets::MapLoadProcess>>,
    abort: Option<Res<'w, MatchLoadAbort>>,
    host_classes: Option<Res<'w, HostClassLoadouts>>,
    catalog: Option<Res<'w, assets::MenuCatalog>>,
    identity: Option<Res<'w, LaunchIdentity>>,
    install_stage: Local<'s, Option<assets::StageHandle>>,
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
    role: Res<'w, frame::RuntimeRole>,
    prediction: Option<Res<'w, net::ClientPredictionState>>,
    input_gate: Res<'w, AuthorityInputGate>,
}

#[derive(SystemParam)]
pub struct MatchInstallPresentation<'w> {
    probe: Option<ResMut<'w, LaunchReport>>,
    camera: Res<'w, SimCamera>,
}

#[derive(SystemParam)]
pub struct MatchInstallOccupancy<'w> {
    has_world: ResMut<'w, HasWorld>,
}

pub fn apply_prepared_match(
    mut commands: Commands,
    source: PreparedMatchSource,
    mut console: MatchInstallConsole,
    authority: MatchInstallAuthority,
    presentation: MatchInstallPresentation,
    occupancy: MatchInstallOccupancy,
    mode_selection: Option<Res<sim::HostGameModeSelection>>,
    mut failed: MessageWriter<frame::MapLoadFailed>,
) {
    let PreparedMatchSource {
        mut ready,
        swap,
        bridge,
        intent,
        mut loading,
        load,
        abort,
        host_classes,
        catalog,
        identity,
        mut install_stage,
    } = source;
    let MatchInstallAuthority {
        role,
        prediction,
        input_gate,
    } = authority;
    let MatchInstallPresentation {
        mut probe,
        camera: sim_cam,
    } = presentation;
    let MatchInstallOccupancy { mut has_world } = occupancy;
    let stale_key = !ready.load_key.match_key.is_none()
        && bridge.as_ref().is_none_or(|bridge| {
            let state = bridge.state();
            !state.in_match()
                || state.identity().match_key() != ready.load_key.match_key
                || ready.load_key.incarnation != bridge.incarnation()
        });
    if !swap.accepts_install(ready.request_id)
        || stale_key
        || ready.load_key.local_load_request_id != ready.request_id
    {
        diag::info!(
            World,
            "match install: discarded stale load {:?} ({})",
            ready.load_key,
            ready.zone
        );
        if let Some(stage) = install_stage.take() {
            stage.cancel();
        }
        commands.remove_resource::<PreparedMatchReady>();
        return;
    }
    if ready.load_key.match_key.is_none() && intent.as_ref().is_some_and(|intent| intent.is_join())
    {
        return;
    }
    if abort.is_some_and(|abort| abort.0 == ready.request_id) {
        let dropped = ready.request_id;
        let zone = ready.zone.clone();
        if let Some(stage) = install_stage.take() {
            stage.cancel();
        }
        commands.remove_resource::<PreparedMatchReady>();
        commands.remove_resource::<MatchLoadAbort>();
        diag::info!(
            World,
            "match install: aborted request #{dropped} (`{zone}`) — not occupying the menu"
        );
        return;
    }
    // The stage opens a frame before the work so the row is already up when
    // the main thread stops answering — the yield is the point, not an
    // accident of where the handle was created.
    let live_load = load
        .as_deref()
        .filter(|process| !process.is_complete())
        .map(|process| &process.progress);
    if live_load.is_some() && install_stage.is_none() {
        if let Some(progress) = live_load {
            *install_stage = Some(progress.begin(assets::StageId::Install, None));
        }
        diag::info!(
            World,
            "match install: yield one frame so the overlay can name the main-thread hitch"
        );
        return;
    }
    let install_stage = install_stage.take();
    let install_started = std::time::Instant::now();
    let request_id = ready.request_id;
    let load_key = ready.load_key;
    let zone = std::mem::take(&mut ready.zone);
    let mut prepared = std::mem::take(&mut ready.prepared);
    commands.remove_resource::<PreparedMatchReady>();

    let world_report = std::mem::take(&mut prepared.report);
    log_world_report(&world_report);

    let plan = match preflight_match_install(
        prepared,
        &zone,
        mode_selection.as_deref(),
        catalog.as_deref(),
        identity.as_deref(),
    ) {
        Ok(plan) => plan,
        Err(refusal) => {
            diag::warn!(World, "match install refused: {}", refusal.error);
            if let Some(probe) = probe.as_deref_mut() {
                if let Some(gap) = refusal.sim_gap {
                    probe.sim_gap = gap;
                }
                probe.world_report = world_report;
            }
            *has_world = HasWorld(false);
            if let Some(stage) = install_stage {
                stage.fail();
            }
            if let Some(loading) = loading.as_deref_mut() {
                loading.fail(refusal.error.clone());
            }
            failed.write(frame::MapLoadFailed {
                request_id,
                load_key,
                zone,
                error: refusal.error,
            });
            console.release_startup();
            return;
        }
    };
    let prepared_install = (|| -> Result<bevy::ecs::world::CommandQueue, InstallRefusal> {
        let mut install = bevy::ecs::world::CommandQueue::default();
        let MatchInstallPlan {
            kind,
            scene: loaded_scene,
            weapons,
            fpv_meshes,
            bodies,
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
            authority_models,
            animated,
            model_spawns,
            map_doors,
            radiation_diggers,
            radiation_moving_diggers,
            radiation_conveyer,
            radiation_lights,
            map_use_triggers,
            flag_descriptors,
            objective_visuals,
            objective_flags,
            objective_attackers,
        } = plan;
        let spawn_count = prepared_map.spawns.len();

        let facts = std::mem::take(&mut prepared_map.facts);
        let airstrike_height = facts.airstrike_height;
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
            assets::SessionMapScriptSound(facts.script_sound),
        );
        stage_resource(
            &mut install,
            assets::SessionTeamSettings(facts.team_settings),
        );
        stage_resource(&mut install, assets::PreparedLocalizedStrings(strings));
        stage_resource(&mut install, fx_catalog);
        stage_resource(&mut install, type10);
        stage_resource(&mut install, fx_models);
        stage_resource(&mut install, impact_fx);
        stage_resource(&mut install, tracers);

        let mut scene = loaded_scene;
        let mut sim_cam = *sim_cam;
        let mut input_gate = *input_gate;
        let mut content = sim::SimContentBuilder::default();
        let mut sim = sim::SimWorld::new();
        content.set_weapon_def_scales(weapons.0.scales_table());
        let combat = combat_table::from_registry(&weapons.0, lochit_table);
        content.set_weapon_combat_table(combat.clone());
        content.set_weapon_runnable_table(weapons.0.runnable_table());
        content.set_weapon_transition_groups(weapons.0.configuration_transition_groups());
        content.set_bullet_pen_facts(combat_table::pen_from_registry(&weapons.0));
        content.set_penetration_table(pen_table);
        content.set_pen_table_loaded(pen_table_loaded);
        content.set_player_kit_collisions(
            player_kit_collision(&bodies.0, false),
            player_kit_collision(&bodies.0, true),
        );
        if let Some(Ok(tree)) = player_anim_sources.compiled() {
            if let Ok(definition) = tree
                .to_runtime_definition(|_, name| xanims.0.clip(assets::AssetNamespace::Iw4, name))
            {
                let names = tree.nodes().iter().map(|node| node.name.clone()).collect();
                content.set_player_anim_tree(Some(definition), names);
            }
        }
        content.set_mantle_xanims(sim::MantleXAnimBind::from_clips(|fast, i| {
            let name = sim::MantleXAnimBind::clip_name(fast, i)?;
            xanims
                .0
                .clip(assets::AssetNamespace::Iw4, name)
                .map(|clip| (*clip).clone())
        }));
        content.set_weapon_script_names(weapons.0.script_names_table());
        install_team_voice_prefixes(&mut content, catalog.as_deref(), identity.as_deref(), &zone);
        let equipment = combat_table::equipment_from_registry(&weapons.0);
        content.set_equipment_runtime_table(equipment.clone());
        let mut primary = Vec::new();
        let mut secondary = Vec::new();
        let mut lethal = Vec::new();
        let mut tactical = Vec::new();
        let mut excluded = Vec::new();
        let families = weapons.0.weapon_families();
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
                assets::FamilySlot::Primary => primary.push(offer),
                assets::FamilySlot::Secondary => secondary.push(offer),
                assets::FamilySlot::Lethal => lethal.push(offer),
                assets::FamilySlot::Tactical => tactical.push(offer),
                assets::FamilySlot::Other => {}
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
            content.set_player_anim_script(Some(sim::PlayerAnimScript::from_tables(slots, events)));
        }
        stage_resource(&mut install, xanims);
        stage_resource(&mut install, death);
        stage_resource(&mut install, player_anim_sources);
        input_gate.local_cmds_enabled = false;

        stage_resource(&mut install, DynEntPhysWorld::default());
        stage_resource(&mut install, DynEntPhysClip(clip.clone()));
        let (sim_gap, lock_reasons, bot_class_ids) = install_clip_and_player(
            &mut sim,
            content,
            clip,
            authority_models,
            scene.intermission_view.map(|view| sim::AuthoredSpawnPoint {
                classname: gamemode_iw4::playerlogic::MP_GLOBAL_INTERMISSION.to_owned(),
                origin: view.origin,
                angles: view.angles,
                script_linkto: String::new(),
                script_destructable_area: String::new(),
            }),
            airstrike_height,
            &prepared_map.spawns,
            &weapons.0,
            &combat,
            &equipment,
            &mut sim_cam,
            &mut input_gate,
            host_classes.as_deref(),
            kind,
            &map_use_triggers,
            &flag_descriptors,
        )?;
        sim.objectives.flag_models = objective_flags;
        sim.objectives.attackers = objective_attackers;
        let defenders = sim.objectives.defenders();
        for site in &mut sim.objectives.bombs {
            site.view.owner = defenders;
        }
        for (source, intact, destroyed) in objective_visuals {
            if let Some(site) = sim
                .objectives
                .bombs
                .iter_mut()
                .find(|b| b.view.model_source == source)
            {
                site.intact_sources = intact;
                site.destroyed_sources = destroyed;
            }
        }
        stage_resource(
            &mut install,
            bots::BotClassPool {
                ready: true,
                ids: bot_class_ids,
            },
        );

        let load_hold = AuthorityLoadHold(sim.clip_brush_count() > 0);
        if let Some(glass) = scene.fx_glass.as_ref() {
            let panes = (0..glass.piece_places.len())
                .filter_map(|i| {
                    let (origin, axis_s, axis_t) = glass.pane_basis(i)?;
                    Some((
                        i as u32,
                        sim::GlassPaneBasis {
                            origin,
                            axis_s,
                            axis_t,
                        },
                    ))
                })
                .collect();
            sim.world_objects_mut().install_glass_panes(panes);
        }
        sim.world_objects_mut()
            .set_map_round_epoch(load_key.match_key.match_epoch);
        sim.start_script_model_play_anims(animated.rows);
        spawn_script_model_movers(&mut sim, &model_spawns);
        crate::map_diggers::install(&mut sim, radiation_diggers);
        crate::map_moving_diggers::install(&mut sim, radiation_moving_diggers);
        if let Some(belt) = radiation_conveyer {
            crate::map_conveyer::install(&mut sim, belt);
        }
        if let Some(doors) = map_doors {
            crate::map_doors::install(&mut sim, doors).map_err(|error| {
                InstallRefusal::new(format!("Door installation refused: {error:?}"))
            })?;
        }
        if let Some(lights) = radiation_lights {
            crate::map_lights::install(&mut sim, lights);
        }
        stamp_script_mover_numbers(&mut scene, &sim);
        if animated.started > 0 || animated.missing_table > 0 {
            diag::info!(
                World,
                "animated_model ScriptModelPlayAnim: {} looping leaves, {} without anim_prop_models row",
                animated.started,
                animated.missing_table
            );
        }
        if animated.toy_started > 0 || animated.toy_missing > 0 {
            diag::info!(
                World,
                "toy_fan ScriptModelPlayAnim: {} looping leaves (state-0 mpAnim), {} without idle clip",
                animated.toy_started,
                animated.toy_missing
            );
        }
        if !model_spawns.is_empty() {
            diag::info!(
                World,
                "script_model G_Spawn: {} ET_SCRIPTMOVER from {}",
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
            &weapons.0,
            &combat,
            &equipment,
            sim.content_digest(),
        )
        .map_err(|error| InstallRefusal::new(format!("Invalid content manifest: {error:?}")))?;
        stage_resource(&mut install, weapons);

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
        if role.runs_authority() || *role == frame::RuntimeRole::Replay {
            if let Some(prediction) = prediction.as_ref() {
                let mut state = net::ClientPrediction::new(prediction.0.local());
                state.arm_from_content(&sim);
                stage_resource(&mut install, net::ClientPredictionState(state));
            }
            stage_resource(&mut install, AuthorityWorld(sim));
        } else if let Some(prediction) = prediction.as_ref() {
            install.push(|world: &mut World| {
                world.remove_resource::<AuthorityWorld>();
            });
            let mut state = net::ClientPrediction::new(prediction.0.local());
            state.install_world(sim);
            stage_resource(&mut install, net::ClientPredictionState(state));
        } else {
            return Err(InstallRefusal::new(
                "No simulation owner for this execution mode",
            ));
        }
        sim_cam.freeze_fly = true;
        stage_resource(&mut install, WorldProducts::from_walk(scene.products_id));
        stage_resource(&mut install, scene);
        stage_resource(&mut install, sim_cam);
        stage_resource(&mut install, input_gate);
        stage_resource(&mut install, load_hold);
        stage_resource(&mut install, class_handoff);
        stage_resource(&mut install, crate::LiveWorldIdentity { load_key });
        stage_resource(&mut install, HasWorld(true));
        stage_resource(&mut install, WorldGeneration::from_install(request_id));
        let installed_zone = zone.clone();
        install.push(move |world: &mut World| {
            if let Some(mut probe) = world.get_resource_mut::<LaunchReport>() {
                probe.sim_gap = sim_gap;
                probe.world_report = world_report;
            }
            diag::info!(Sim, "match: {} ({}) — world installed ({} dm spawn points); class select waits for admission", kind.display_name(), kind.token(), spawn_count);
            perf::match_installed(&installed_zone, 1);
            world.write_message(MatchInstalled {
                request_id,
                load_key,
                zone: installed_zone,
                spawn_count,
            });
        });

        Ok(install)
    })();
    match prepared_install {
        Ok(mut install) => {
            commands.queue(move |world: &mut World| {
                let accepts = world
                    .resource::<crate::SessionSwapRequest>()
                    .accepts_install(request_id);
                let same_match = load_key.match_key.is_none()
                    || world
                        .get_resource::<net::MasterBridge>()
                        .is_some_and(|bridge| {
                            bridge.state().in_match()
                                && bridge.state().identity().match_key() == load_key.match_key
                                && bridge.incarnation() == load_key.incarnation
                        });
                if accepts && same_match {
                    install.apply(world);
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
            if let Some(loading) = loading.as_deref_mut() {
                loading.fail(refusal.error.clone());
            }
            failed.write(frame::MapLoadFailed {
                request_id,
                load_key,
                zone,
                error: refusal.error,
            });
        }
    }
    console.release_startup();
}

/// Everything the install publishes, assembled while a refusal is still free.
///
/// The plan exists so publication cannot start before the checks are done: the
/// commit half of `apply_prepared_match` has nothing to install until preflight
/// hands one over.
struct MatchInstallPlan {
    kind: gamemode_iw4::GameModeKind,
    scene: WorldScene,
    weapons: PreparedWeapons,
    fpv_meshes: PreparedFpvMeshes,
    bodies: assets::PreparedBodies,
    world_weapons: assets::PreparedWorldWeapons,
    projectile_meshes: assets::PreparedProjectileMeshes,
    xmodel_walk: assets::PreparedXModelWalkCensus,
    xanims: PreparedXAnims,
    death: PreparedDestructibleDeath,
    player_anim_sources: assets::PlayerAnimSources,
    prepared_map: assets::PreparedMap,
    strings: assets::LocalizeCatalog,
    clip: Option<Arc<ClipCollision>>,
    pen_table: sim::PenetrationDepthTable,
    pen_table_loaded: bool,
    lochit_table: Option<[f32; sim::HITLOC_COUNT]>,
    tracers: PreparedTracers,
    fx_catalog: PreparedFxCatalog,
    type10: MatchType10SoundHints,
    fx_models: PreparedFxModels,
    impact_fx: PreparedImpactFx,
    authority_models: AuthorityEntityModelInstall,
    animated: AnimatedPropAnims,
    model_spawns: Vec<(sim::ScriptModelId, [f32; 3], [f32; 3])>,
    map_doors: Option<sim::MapDoors>,
    radiation_diggers: Vec<sim::RadiationDigger>,
    radiation_moving_diggers: Vec<sim::RadiationMovingDigger>,
    radiation_conveyer: Option<sim::RadiationConveyer>,
    radiation_lights: Option<sim::RadiationLights>,
    map_use_triggers: Vec<assets::MapUseTrigger>,
    flag_descriptors: Vec<assets::FlagDescriptor>,
    objective_visuals: Vec<(u32, Vec<u32>, Vec<u32>)>,
    objective_flags: [String; 3],
    objective_attackers: gamemode_iw4::Team,
}

/// A refusal of the whole request: the loading screen shows `error`, the swap
/// completes on it, and nothing of the match has been published.
struct InstallRefusal {
    error: String,
    sim_gap: Option<&'static str>,
}

impl InstallRefusal {
    fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            sim_gap: None,
        }
    }

    fn with_gap(error: impl Into<String>, gap: &'static str) -> Self {
        Self {
            error: error.into(),
            sim_gap: Some(gap),
        }
    }
}

fn preflight_match_install(
    mut prepared: assets::PreparedMatch,
    zone: &str,
    mode_selection: Option<&sim::HostGameModeSelection>,
    catalog: Option<&assets::MenuCatalog>,
    identity: Option<&LaunchIdentity>,
) -> Result<MatchInstallPlan, InstallRefusal> {
    let weapons = PreparedWeapons(prepared.weapons);
    let fpv_meshes = PreparedFpvMeshes(prepared.fpv_meshes);
    let bodies = assets::PreparedBodies(prepared.bodies);
    let world_weapons = assets::PreparedWorldWeapons(prepared.world_weapons);
    let projectile_meshes = assets::PreparedProjectileMeshes(prepared.projectile_meshes);
    let xmodel_walk = std::mem::take(&mut prepared.xmodel_walk);
    let xanims = PreparedXAnims(prepared.xanims);
    let death = PreparedDestructibleDeath(std::mem::take(&mut prepared.destructible_death));
    log_destructible_death_assets(&death);
    let player_anim_sources = prepared.player_anim_sources;
    let mut prepared_map = prepared.prepared_map;
    if prepared_map.namespace == Some(assets::AssetNamespace::Iw4)
        && let Some(catalog) = catalog
        && let Some(table) = catalog.string_table(gamemode_iw4::FACTION_TABLE)
    {
        let arena = catalog
            .rawfile_text("mp/basemaps.arena")
            .map(str::to_owned)
            .or_else(|| identity.and_then(|id| assets::read_basemaps_arena(&id.games_root)));
        prepared_map.facts.team_settings = assets::team_settings_for_zone(
            table,
            assets::AssetNamespace::Iw4,
            arena.as_deref(),
            zone,
        );
    }
    let strings = std::mem::take(&mut prepared.strings);
    let kind = match match_kind(mode_selection) {
        Ok(kind) => kind,
        Err(gap) => {
            diag::info!(Sim, "match install refused: {gap}");
            return Err(InstallRefusal::with_gap(gap, gap));
        }
    };
    apply_gameobjects_main(&mut prepared.world, kind);
    let flag_descriptors = std::mem::take(&mut prepared.world.flag_descriptors);
    let map_use_triggers = std::mem::take(&mut prepared.world.map_use_triggers);
    let map_doors = crate::map_doors::prepare(zone, &prepared.world, &map_use_triggers)
        .map_err(|error| InstallRefusal::new(format!("Door preparation refused: {error:?}")))?;
    let radiation_lights = crate::map_lights::prepare(zone, &prepared.world);
    let radiation_diggers = crate::map_diggers::prepare(zone, &prepared.world);
    let radiation_moving_diggers = crate::map_moving_diggers::prepare(zone, &prepared.world);
    let radiation_conveyer = crate::map_conveyer::prepare(zone, &map_use_triggers);
    let objective_setup: Result<_, String> = (|| {
        let visuals = crate::objectives::prepare(&mut prepared.world, &map_use_triggers, kind)?;
        let mut flags = [String::new(), String::new(), String::new()];
        let mut attackers = gamemode_iw4::Team::Allies;
        if kind == gamemode_iw4::GameModeKind::Domination {
            let catalog = catalog.ok_or("DOM faction catalog missing")?;
            let arena = catalog
                .rawfile_text("mp/basemaps.arena")
                .map(str::to_owned)
                .or_else(|| identity.and_then(|id| assets::read_basemaps_arena(&id.games_root)));
            flags = crate::objectives::flag_models(catalog, arena.as_deref(), zone)?;
            let neutral = flags[0].clone();
            for (index, model) in flags.iter_mut().enumerate() {
                let captured = matches!(
                    prepared.world.map_xmodel_scene_assets.get_name(model),
                    Some(
                        assets::MapXModelSceneAsset::Iw4(_)
                            | assets::MapXModelSceneAsset::Iw5(_)
                            | assets::MapXModelSceneAsset::T5(_)
                    )
                );
                if !captured
                    && index > 0
                    && prepared_map.namespace != Some(assets::AssetNamespace::Iw4)
                {
                    diag::info!(Sim, "DOM flag model {model} unavailable; using {neutral}");
                    *model = neutral.clone();
                } else if !captured {
                    return Err(format!("DOM flag model unavailable: {model}"));
                }
            }
        }
        if kind == gamemode_iw4::GameModeKind::Demolition {
            attackers = match prepared_map.facts.script_sound.attackers.as_deref() {
                Some("axis") => gamemode_iw4::Team::Axis,
                Some("allies") => gamemode_iw4::Team::Allies,
                _ => return Err("DD map script must declare game[attackers]".into()),
            };
        }
        Ok((visuals, flags, attackers))
    })();
    let (objective_visuals, objective_flags, objective_attackers) = match objective_setup {
        Ok(setup) => setup,
        Err(error) => {
            diag::info!(Sim, "objective match refused: {error}");
            return Err(InstallRefusal::with_gap(
                error,
                "objective mode content unavailable",
            ));
        }
    };
    apply_toy_spawn_models(&mut prepared.world);
    let authority_models = authority_entity_model_install(&prepared.world);
    let animated = collect_animated_prop_anims(&prepared.world.script_model_instances, &xanims.0);
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
    log_destructible_fx_assets(&fx_catalog);
    let impact_fx = PreparedImpactFx(std::mem::take(&mut prepared.world.impact_fx));
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
        kind,
        scene,
        weapons,
        fpv_meshes,
        bodies,
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
        animated,
        model_spawns,
        map_doors,
        radiation_diggers,
        radiation_moving_diggers,
        radiation_conveyer,
        radiation_lights,
        map_use_triggers,
        flag_descriptors,
        objective_visuals,
        objective_flags,
        objective_attackers,
    })
}

pub fn install_script_model_id(content: assets::ScriptModelId) -> sim::ScriptModelId {
    sim::ScriptModelId::from_authored_source_ordinal(content.source_ordinal())
}

struct AnimatedPropAnims {
    rows: Vec<(sim::ScriptModelId, &'static str, bool, f32)>,
    started: usize,
    missing_table: usize,
    toy_started: usize,
    toy_missing: usize,
}

fn player_kit_collision(bodies: &assets::BodyMeshCatalog, axis: bool) -> sim::PlayerKitCollision {
    let kits = bodies.kits();
    let kit = kits.kit(axis);
    let body_key = kit.map(|kit| kit.body.clone()).unwrap_or_default();
    let head_key = kit.and_then(|kit| kit.head.clone()).unwrap_or_default();
    let body = (!body_key.is_empty())
        .then(|| {
            bodies
                .get(&body_key)
                .and_then(|entry| entry.skel.retained_capability())
                .map(Arc::new)
        })
        .flatten();
    let head = (!head_key.is_empty())
        .then(|| {
            bodies
                .get(&head_key)
                .and_then(|entry| entry.skel.retained_capability())
                .map(Arc::new)
        })
        .flatten();
    sim::PlayerKitCollision {
        body_key,
        body,
        head_key,
        head,
    }
}

fn collect_animated_prop_anims(
    instances: &[assets::ScriptModelSceneInstance],
    xanims: &assets::XAnimCatalog,
) -> AnimatedPropAnims {
    let mut rows = Vec::new();
    let mut missing_table = 0;
    let mut toy_missing = 0;
    let mut started = 0;
    let mut toy_started = 0;
    for instance in instances {
        let machine = sim::animprop_machine(
            &instance.metadata.targetname,
            &instance.metadata.destructible_type,
        );
        let (clip, is_toy) = match machine {
            Some("animated_model") => {
                match sim::mp_clip_for_animated_model(&instance.current_model.0) {
                    Some(clip) => (clip, false),
                    None => {
                        missing_table += 1;
                        continue;
                    }
                }
            }
            Some("toy_fan") => match sim::mp_clip_for_toy_fan(&instance.metadata.destructible_type)
            {
                Some(clip) => (clip, true),
                None => {
                    toy_missing += 1;
                    continue;
                }
            },
            _ => continue,
        };
        let (looping, frequency) = xanims
            .clip(assets::AssetNamespace::Iw4, clip)
            .map(|c| (c.looping, c.frequency()))
            .unwrap_or((false, 0.0));
        rows.push((
            install_script_model_id(instance.id),
            clip,
            looping,
            frequency,
        ));
        if is_toy {
            toy_started += 1;
        } else {
            started += 1;
        }
    }
    AnimatedPropAnims {
        rows,
        started,
        missing_table,
        toy_started,
        toy_missing,
    }
}

fn match_kind(
    selection: Option<&sim::HostGameModeSelection>,
) -> Result<gamemode_iw4::GameModeKind, &'static str> {
    if let Some(selection) = selection {
        return Ok(selection.kind());
    }
    match std::env::var("IW4L_GAMETYPE") {
        Ok(s) if !s.trim().is_empty() => gamemode_iw4::GameModeKind::parse_ascii_ignore_case(&s)
            .ok_or("IW4L_GAMETYPE out of scope — not dm/dd/dom (gsc.mode.out-of-scope)"),
        _ => Ok(gamemode_iw4::GameModeKind::FreeForAll),
    }
}

fn apply_gameobjects_main(world: &mut assets::PreparedWorld, kind: gamemode_iw4::GameModeKind) {
    let models_before = world.script_model_instances.len();
    let brushes_before = world.script_brush_models.len();
    world
        .script_model_instances
        .retain(|instance| gamemode_iw4::gameobject_survives(&instance.metadata.gameobject, kind));
    for instance in &mut world.script_model_instances {
        let drop_link = match &instance.metadata.brush_link {
            assets::ScriptBrushModelLink::Linked(brush) => {
                !gamemode_iw4::gameobject_survives(&brush.gameobject, kind)
            }
            assets::ScriptBrushModelLink::None | assets::ScriptBrushModelLink::Ambiguous { .. } => {
                false
            }
        };
        if drop_link {
            instance.metadata.brush_link = assets::ScriptBrushModelLink::None;
        }
    }
    world
        .script_brush_models
        .retain(|brush| gamemode_iw4::gameobject_survives(&brush.gameobject, kind));
    diag::info!(
        Sim,
        "gameobjects main ({}): script_models {}/{} kept, *N {}/{} kept, allowed={:?}",
        kind.token(),
        world.script_model_instances.len(),
        models_before,
        world.script_brush_models.len(),
        brushes_before,
        kind.gameobjects_allowed(),
    );
}

fn script_model_spawns(
    instances: &[assets::ScriptModelSceneInstance],
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
) {
    for (id, origin, angles) in models {
        sim.spawn_script_mover(*id, *origin, *angles)
            .expect("G_Spawn exhausted dynamic entity slots while installing script models");
    }
}

fn stamp_script_mover_numbers(scene: &mut WorldScene, sim: &sim::SimWorld) {
    for instance in &mut scene.script_model_instances {
        let id = install_script_model_id(instance.id);
        instance.gentity_number = sim.gentity_number(id).and_then(|n| u16::try_from(n).ok());
    }
}

fn map_anim_items(
    slots: &[(u8, u8, Vec<assets::ParsedAnimItem>)],
    tree: Option<&assets::CompiledAnimTreeDefinition>,
    catalog: &assets::XAnimCatalog,
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
    events: &[(u8, Vec<assets::ParsedAnimItem>)],
    tree: Option<&assets::CompiledAnimTreeDefinition>,
    catalog: &assets::XAnimCatalog,
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
    item: &assets::ParsedAnimItem,
    tree: Option<&assets::CompiledAnimTreeDefinition>,
    catalog: &assets::XAnimCatalog,
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
    tree: Option<&assets::CompiledAnimTreeDefinition>,
    catalog: &assets::XAnimCatalog,
    cmd: &assets::ParsedAnimCommand,
) -> i32 {
    if let Some(ms) = cmd.duration_ms {
        return if ms <= 0 { 500 } else { ms };
    }
    let Some(node) = tree.and_then(|t| t.node(cmd.anim_index)) else {
        return 500;
    };
    let Some(captured) = catalog.get(assets::AssetNamespace::Iw4, &node.name) else {
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

fn log_destructible_fx_assets(fx: &PreparedFxCatalog) {
    const VEHICLE_DEFS: &[&str] = &[
        "smoke/car_damage_blacksmoke_fire",
        "explosions/small_vehicle_explosion",
        "explosions/vehicle_explosion_medium",
        sim::EXPLODABLE_BARREL_DEATH_FX,
        sim::EXPLODABLE_BARREL_BURN_START_FX,
        sim::EXPLODABLE_BARREL_BURN_LOOP_FX,
    ];
    let mut names: Vec<&str> = VEHICLE_DEFS.to_vec();
    for kind in gamemode_iw4::TOY_DESTRUCTIBLE_KINDS {
        for stage in kind.definition().stages {
            names.extend(stage.fx.iter().map(|fx| fx.name));
            names.extend(stage.loop_fx.iter().map(|fx| fx.name));
        }
    }
    names.sort_unstable();
    names.dedup();
    let status_of = |name: &str| {
        let map_ns = assets::fx_body_namespace(fx.0.map_namespace());
        if fx.0.resolve_def_for_map(name).is_some() {
            "captured"
        } else if fx.0.get_in(map_ns, name).is_some()
            || fx.0.get_in(assets::AssetNamespace::Iw4, name).is_some()
        {
            "empty"
        } else {
            "MISSING"
        }
    };
    for name in names {
        let status = status_of(name);
        diag::info!(World, "destructible fx {name}: {status}");
    }
    let tanker = gamemode_iw4::dd::PLANTED_BOMB_EXPLODE_FX_PATH.expect("GSC-cited");
    let tanker_status = status_of(tanker);
    diag::info!(World, "suitcase explode fx {tanker}: {tanker_status}");
}

struct AuthorityEntityModelInstall {
    capabilities: Vec<sim::EntityCollisionCapabilities>,
    vehicles: Vec<(sim::ScriptModelId, sim::VehicleDestructibleKind, [f32; 3])>,
    toys: Vec<(sim::ScriptModelId, sim::ToyDestructibleKind, [f32; 3])>,
    barrels: Vec<(sim::ScriptModelId, [f32; 3])>,
    crates: Vec<sim::FlammableCrateInstall>,
    destructables: Vec<sim::DestructableInstall>,
    installed_owners: Vec<(assets::ScriptModelId, sim::AuthorityModelOwner)>,
    ambiguous_brush_links: usize,
    standalone_brush_links: usize,
}

fn apply_toy_spawn_models(world: &mut assets::PreparedWorld) {
    let remaps: Vec<(usize, String)> = {
        let captured = &world.map_xmodel_scene_assets;
        world
            .script_model_instances
            .iter()
            .enumerate()
            .filter_map(|(index, instance)| {
                let kind =
                    sim::ToyDestructibleKind::from_mapents(&instance.metadata.destructible_type)?;
                let mapent = instance.current_model.0.as_str();
                let spawn = kind
                    .spawn_model_candidates(mapent)
                    .iter()
                    .copied()
                    .find(|name| {
                        matches!(
                            captured.get_name(name),
                            Some(
                                assets::MapXModelSceneAsset::Iw4(_)
                                    | assets::MapXModelSceneAsset::Iw5(_)
                                    | assets::MapXModelSceneAsset::T5(_),
                            )
                        )
                    })?;
                (spawn != mapent).then(|| (index, spawn.to_owned()))
            })
            .collect()
    };
    for (index, model) in remaps {
        world.script_model_instances[index].current_model = assets::MapXModelAssetKey(model);
    }
}

fn attach_swap_capabilities<'a>(
    dobj: &mut sim::AuthorityDObjState,
    world: &assets::PreparedWorld,
    names: impl IntoIterator<Item = &'a str>,
) {
    for name in names {
        let key = assets::MapXModelAssetKey(name.to_owned());
        let capability = match world.map_xmodel_scene_assets.get(&key) {
            Some(
                assets::MapXModelSceneAsset::Iw4(model)
                | assets::MapXModelSceneAsset::Iw5(model)
                | assets::MapXModelSceneAsset::T5(model),
            ) => model.retained_capability().map(std::sync::Arc::new),
            Some(assets::MapXModelSceneAsset::Unavailable { .. }) | None => None,
        };
        dobj.swap_capabilities.push((name.to_owned(), capability));
    }
}

fn authority_entity_model_install(world: &assets::PreparedWorld) -> AuthorityEntityModelInstall {
    let mut vehicles = Vec::new();
    let mut toys = Vec::new();
    let mut barrels = Vec::new();
    let mut crates = Vec::new();
    let mut destructables = Vec::new();
    let mut installed_owners = Vec::new();
    let mut ambiguous_brush_links = 0;
    let mut capabilities: Vec<sim::EntityCollisionCapabilities> = world
        .script_model_instances
        .iter()
        .map(|instance| {
            let sim_id = install_script_model_id(instance.id);
            let owner = sim::AuthorityModelOwner::ScriptModel(sim_id);
            installed_owners.push((instance.id, owner));
            if let Some(kind) =
                sim::VehicleDestructibleKind::from_mapents(&instance.metadata.destructible_type)
            {
                vehicles.push((sim_id, kind, instance.transform.translation.to_array()));
            }
            if let Some(kind) =
                sim::ToyDestructibleKind::from_mapents(&instance.metadata.destructible_type)
            {
                toys.push((sim_id, kind, instance.transform.translation.to_array()));
            }
            if assets::exploding_prop_machine(
                &instance.metadata.targetname,
                &instance.metadata.script_noteworthy,
                &instance.metadata.destructible_type,
            ) == Some("explodable_barrel")
            {
                barrels.push((sim_id, instance.transform.translation.to_array()));
            }
            if assets::exploding_prop_machine(
                &instance.metadata.targetname,
                &instance.metadata.script_noteworthy,
                &instance.metadata.destructible_type,
            ) == Some("flammable_crate")
            {
                crates.push(sim::FlammableCrateInstall {
                    id: sim_id,
                    origin: instance.transform.translation.to_array(),
                });
            }
            if gamemode_iw4::is_destructable_targetname(&instance.metadata.targetname) {
                destructables.push(sim::DestructableInstall {
                    id: sim_id,
                    origin: instance.transform.translation.to_array(),
                    accumulate: instance.metadata.script_accumulate,
                    threshold: instance.metadata.script_threshold,
                    script_destructable_area: instance.metadata.script_destructable_area.clone(),
                    has_fx: !instance.metadata.script_fxid.is_empty(),
                });
            }
            let linked_brushes = match &instance.metadata.brush_link {
                assets::ScriptBrushModelLink::Linked(brush) => {
                    vec![sim::LinkedBrushCollisionBrush {
                        cmodel_handle: brush.cmodel_handle,
                        origin: brush.origin,
                        angles: brush.angles,
                    }]
                }
                assets::ScriptBrushModelLink::Ambiguous { .. } => {
                    ambiguous_brush_links += 1;
                    Vec::new()
                }
                assets::ScriptBrushModelLink::None => Vec::new(),
            };
            let capability = {
                let key = &instance.current_model;
                match world.map_xmodel_scene_assets.get(key) {
                    Some(
                        assets::MapXModelSceneAsset::Iw4(model)
                        | assets::MapXModelSceneAsset::Iw5(model)
                        | assets::MapXModelSceneAsset::T5(model),
                    ) => model.retained_capability().map(std::sync::Arc::new),
                    Some(assets::MapXModelSceneAsset::Unavailable { .. }) | None => None,
                }
            };
            let mut dobj = sim::AuthorityDObjState::new_dirty(
                instance.current_model.0.clone(),
                capability,
                instance.transform.to_matrix(),
            );
            if let Some(kind) =
                sim::VehicleDestructibleKind::from_mapents(&instance.metadata.destructible_type)
            {
                attach_swap_capabilities(&mut dobj, world, [kind.definition().death.husk]);
            } else if let Some(kind) =
                sim::ToyDestructibleKind::from_mapents(&instance.metadata.destructible_type)
            {
                attach_swap_capabilities(&mut dobj, world, kind.definition().stage_models());
            } else if assets::exploding_prop_machine(
                &instance.metadata.targetname,
                &instance.metadata.script_noteworthy,
                &instance.metadata.destructible_type,
            ) == Some("explodable_barrel")
            {
                attach_swap_capabilities(&mut dobj, world, [sim::EXPLODABLE_BARREL_HUSK]);
            } else if assets::exploding_prop_machine(
                &instance.metadata.targetname,
                &instance.metadata.script_noteworthy,
                &instance.metadata.destructible_type,
            ) == Some("flammable_crate")
            {
                attach_swap_capabilities(&mut dobj, world, [gamemode_iw4::FLAMMABLE_CRATE_HUSK]);
            }

            if let Some(definition) = &instance.metadata.t5_destructible {
                sim::t5_destructible::install(&mut dobj, definition.clone());
            }
            dobj.semantic_state.hide_part_bits = instance.dobj_state.hide_part_bits;
            dobj.pose_request.hide_part_bits = instance.dobj_state.hide_part_bits;
            sim::EntityCollisionCapabilities::current_tick(owner, Some(dobj), linked_brushes)
        })
        .collect();
    let mut claimed = std::collections::HashSet::new();
    for instance in &world.script_model_instances {
        if let assets::ScriptBrushModelLink::Linked(brush) = &instance.metadata.brush_link {
            claimed.insert(brush.source_ordinal);
        }
    }
    let mut standalone_brush_links = 0usize;
    for brush in &world.script_brush_models {
        if claimed.contains(&brush.source_ordinal) {
            continue;
        }

        let sim_id = install_script_model_id(assets::ScriptModelId::from_source_ordinal(
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
        if gamemode_iw4::is_destructable_targetname(&brush.targetname) {
            destructables.push(sim::DestructableInstall {
                id: sim_id,
                origin: brush.origin,
                accumulate: brush.script_accumulate,
                threshold: brush.script_threshold,
                script_destructable_area: brush.script_destructable_area.clone(),
                has_fx: !brush.script_fxid.is_empty(),
            });
        }
        standalone_brush_links += 1;
    }
    AuthorityEntityModelInstall {
        capabilities,
        vehicles,
        toys,
        barrels,
        crates,
        destructables,
        installed_owners,
        ambiguous_brush_links,
        standalone_brush_links,
    }
}

/// What the two runtime owners of the map collision are given: one immutable
/// backing they share, and the static models only the sim traces against.
fn install_clip_and_player(
    sim: &mut sim::SimWorld,
    mut content: sim::SimContentBuilder,
    clip: Option<Arc<ClipCollision>>,
    authority_models: AuthorityEntityModelInstall,
    intermission_view: Option<sim::AuthoredSpawnPoint>,
    airstrike_height: Option<f32>,
    spawns_in: &[SpawnPoint],
    weapons: &WeaponRegistry,
    combat: &[sim::WeaponCombatFacts],
    equipment: &[sim::EquipmentRuntimeFacts],
    sim_cam: &mut SimCamera,
    input_gate: &mut AuthorityInputGate,
    host_classes: Option<&HostClassLoadouts>,
    kind: gamemode_iw4::GameModeKind,
    map_use_triggers: &[assets::MapUseTrigger],
    flag_descriptors: &[assets::FlagDescriptor],
) -> Result<(&'static str, Vec<Option<String>>, Vec<sim::ClassId>), InstallRefusal> {
    let clip = clip.ok_or_else(|| InstallRefusal::new("Required collision geometry is missing"))?;
    let static_models = &clip.static_models;
    let count = clip.brushes.len();
    let node_count = clip.nodes.len();
    let leaf_count = clip.leaves.len();
    let vert_count = clip.mesh.verts.len();
    let tri_count = clip.mesh.tri_count();
    let smodel_count = static_models.len();
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
    if count == 0 && tri_count == 0 {
        return Err(InstallRefusal::new("Required collision geometry is empty"));
    }
    let bsp = sim::SimClipBsp {
        nodes: clip
            .nodes
            .iter()
            .map(|n| sim::ClipNode {
                plane: n.plane,
                children: n.children,
            })
            .collect(),
        leaves: clip
            .leaves
            .iter()
            .map(|l| sim::ClipLeaf {
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
        static_models: static_models
            .iter()
            .map(|sm| sim::SimStaticModel {
                index: sm.index,
                name: sm.name.clone(),
                model: sm.model.clone(),
            })
            .collect(),
        ..Default::default()
    };
    let cmodel_count = clip.cmodels.len();
    let cmodels = sim::SimClipCmodels {
        models: clip
            .cmodels
            .iter()
            .map(|c| sim::ClipCmodel {
                mins: c.mins,
                maxs: c.maxs,
                radius: c.radius,
                first_brush: c.first_brush,
                num_brushes: c.num_brushes,
            })
            .collect(),
    };
    content.set_clip_map(brushes, bsp, mesh, cmodels);
    sim.install_content(content.finish());
    sim.world_objects_mut()
        .install_vehicle_destructibles(authority_models.vehicles);
    sim.world_objects_mut()
        .install_toy_destructibles(authority_models.toys);
    sim.world_objects_mut()
        .install_explodable_barrels(authority_models.barrels);
    sim.world_objects_mut()
        .install_flammable_crates(authority_models.crates);
    let missing_tdm = !spawns_in
        .iter()
        .any(|spawn| spawn.classname == gamemode_iw4::SPAWN_TDM);
    let block_area_gap = sim
        .world_objects_mut()
        .install_destructables(authority_models.destructables, missing_tdm);
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

    let spawns: Vec<sim::AuthoredSpawnPoint> = spawns_in
        .iter()
        .map(|p| sim::AuthoredSpawnPoint {
            classname: p.classname.clone(),
            origin: p.origin,
            angles: p.angles,
            script_linkto: p.script_linkto.clone(),
            script_destructable_area: p.script_destructable_area.clone(),
        })
        .collect();
    let rows = bootstrap_class_rows(host_classes);
    let projected: Vec<AuthoritativeClassProjection> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| project_class(index as u32, row, weapons, combat, equipment))
        .collect();
    let lock_reasons: Vec<Option<String>> = projected
        .iter()
        .map(|row| row.lock_reason.clone())
        .collect();
    let mut classes: Vec<sim::ClassDef> = projected.into_iter().map(|row| row.def).collect();
    let locked_n = lock_reasons.iter().filter(|r| r.is_some()).count();
    let unique = crate::bot_loadout::project_unique_bot_classes(
        classes.len() as u32,
        weapons,
        combat,
        equipment,
    );
    let bot_class_ids = unique.class_ids();
    diag::info!(
        Sim,
        "bots: unique loadout primaries={} secondaries={} classes={} (host classes {})",
        unique.primaries.len(),
        unique.secondaries.len(),
        bot_class_ids.len(),
        classes.len()
    );
    classes.extend(unique.classes);
    let has_intermission_view = intermission_view.is_some();
    if let Err(err) = sim.bootstrap(sim::MatchBootstrap {
        spawns,
        flag_descriptors: flag_descriptors
            .iter()
            .map(|row| sim::DomFlagDescriptor {
                origin: row.origin,
                script_linkname: row.script_linkname.clone(),
                script_linkto: row.script_linkto.clone(),
            })
            .collect(),
        classes,
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
        allow_debug_actions: true,
        host_owns_respawn: true,
        intermission_view,
        airstrike_height,
        ..Default::default()
    }) {
        diag::info!(Sim, "bootstrap: {err}");
        return Err(InstallRefusal::new(format!(
            "MatchBootstrap refused: {err}"
        )));
    }

    if block_area_gap {
        sim.script_gaps_mut()
            .raise(gamemode_iw4::ScriptGapCause::BlockAreaMissingTdmSpawns);
    }

    if kind == gamemode_iw4::GameModeKind::Domination {
        match install_dom_flags_from_mapents(sim, map_use_triggers) {
            Ok(ids) => {
                diag::info!(Sim, "dom flags: {} MapEnts volumes", ids.len());
                sim.rebuild_dom_spawn_graph();
            }
            Err(err) => {
                diag::info!(Sim, "dom flags refused: {err:?}");
                return Err(InstallRefusal::new(format!(
                    "DOM flag bootstrap refused: {err:?}"
                )));
            }
        }
    }

    crate::objectives::install(sim, weapons, map_use_triggers, kind).map_err(|error| {
        InstallRefusal::new(format!("Objective installation refused: {error:?}"))
    })?;

    sim_cam.enabled = false;
    input_gate.local_cmds_enabled = false;
    sim_cam.freeze_fly = has_intermission_view;
    diag::info!(
        Sim,
        "spawn: match bootstrapped with {} clip brushes and {} dm spawns ({} / {} classes Locked by coverage)",
        count,
        spawns_in.len(),
        locked_n,
        rows.len()
    );
    let gap = if has_intermission_view {
        "player awaits SelectClass; mp_global_intermission holds the camera until Equip ack"
    } else {
        "player awaits SelectClass; Equip ack enables sim camera (no authored intermission)"
    };
    Ok((gap, lock_reasons, bot_class_ids))
}

pub(crate) fn bootstrap_class_rows(host: Option<&HostClassLoadouts>) -> Vec<ClassRow> {
    let fallback = HostClassLoadouts::default();
    let host = host.filter(|h| !h.slots.is_empty()).unwrap_or(&fallback);
    host.slots
        .iter()
        .map(|slot| ClassRow {
            weapons: [
                slot.primary.clone(),
                slot.secondary.clone(),
                slot.lethal.clone(),
                slot.tactical.clone(),
            ],
            attachments: [
                slot.primary_attachments.clone(),
                slot.secondary_attachments.clone(),
            ],
            perks: slot.perks.clone(),
            deathstreak: slot.deathstreak.clone(),
        })
        .collect()
}

fn install_team_voice_prefixes(
    world: &mut sim::SimContentBuilder,
    catalog: Option<&assets::MenuCatalog>,
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
        .or_else(|| identity.and_then(|id| assets::read_basemaps_arena(&id.games_root)));
    let row = arena_text
        .as_deref()
        .and_then(|text| assets::arena_charsets(text, zone));
    let allies_cs = row
        .as_ref()
        .and_then(|r| r.allieschar.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| gamemode_iw4::DEFAULT_ALLIES_CHARSET.to_owned());
    let axis_cs = row
        .as_ref()
        .and_then(|r| r.axischar.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| gamemode_iw4::DEFAULT_AXIS_CHARSET.to_owned());
    let allies = table.lookup_col(&allies_cs, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    let axis = table.lookup_col(&axis_cs, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    world.set_team_voice_prefixes(
        (!allies.is_empty()).then(|| allies.to_owned()),
        (!axis.is_empty()).then(|| axis.to_owned()),
    );
}

fn map_use_to_dom_ent(row: &assets::MapUseTrigger) -> gamemode_iw4::DomFlagMapEnt<'_> {
    gamemode_iw4::DomFlagMapEnt {
        classname: &row.classname,
        targetname: &row.targetname,
        origin: row.origin,
        angles: row.angles,
        script_label: &row.script_label,
        gameobject: &row.gameobject,
        radius: row.radius,
        height: row.height,
    }
}

fn install_dom_flags_from_mapents(
    sim: &mut sim::SimWorld,
    triggers: &[assets::MapUseTrigger],
) -> Result<Vec<u32>, sim::DomFlagInstallError> {
    let ents: Vec<_> = triggers.iter().map(map_use_to_dom_ent).collect();
    sim.install_dom_flags(&ents)
}

pub fn register_match_apply_systems(app: &mut App) {
    app.init_resource::<StartupCommands>()
        .init_resource::<PendingConsoleLines>()
        .init_resource::<HostClassLoadouts>()
        .add_systems(
            Update,
            apply_prepared_match
                .run_if(resource_exists::<PreparedMatchReady>)
                .in_set(ClientSet::Load),
        );
}

fn stage_resource<T: Resource>(queue: &mut bevy::ecs::world::CommandQueue, resource: T) {
    queue.push(move |world: &mut World| {
        world.insert_resource(resource);
    });
}
