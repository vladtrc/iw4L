use assets::{
    ClipCollision, DestructibleDeathHint, LoadingScreen, MatchLoadAbort, MatchType10SoundHints,
    PreparedDestructibleDeath, PreparedFpvMeshes, PreparedMatchReady, PreparedSpawn,
    PreparedWeapons, PreparedXAnims, WeaponRegistry, stamp_destructible_death_edges,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::{
    CacWeaponOffer, ClassSelectHandoff, HasWorld, HostClassLoadouts, LaunchIdentity, LaunchReport,
    MatchInstalled, WorldGeneration,
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

pub fn authoritative_class_lock_reason(
    names: [&str; 4],
    weapons: &WeaponRegistry,
    combat: &[sim::WeaponCombatFacts],
    equipment: &[sim::EquipmentRuntimeFacts],
) -> Option<String> {
    let mut reason = None;
    for name in &names[..2] {
        let id = match weapons.resolve_index(name) {
            Ok(None) => continue,
            Ok(Some(id)) => id,
            Err(_) => {
                append_lock_reason(&mut reason, format!("{name}:catalog.unknown"));
                continue;
            }
        };
        if !combat
            .get(id as usize)
            .copied()
            .is_some_and(sim::WeaponCombatFacts::is_usable)
        {
            append_lock_reason(&mut reason, format!("{name}:validated.missing_profile"));
        }
    }

    for name in &names[2..] {
        let id = match weapons.resolve_index(name) {
            Ok(None) => continue,
            Ok(Some(id)) => id,
            Err(_) => {
                append_lock_reason(&mut reason, format!("{name}:catalog.unknown"));
                continue;
            }
        };
        if !equipment
            .get(id as usize)
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
    names: [&str; 4],
    perks: [&str; 3],
    deathstreak: &str,
    weapons: &WeaponRegistry,
    combat: &[sim::WeaponCombatFacts],
    equipment: &[sim::EquipmentRuntimeFacts],
) -> AuthoritativeClassProjection {
    let resolve = |name: &str| -> (u32, bool) {
        match weapons.resolve_index(name) {
            Ok(None) => (0, false),
            Ok(Some(id)) => (id, false),
            Err(_) => (0, true),
        }
    };
    let resolved = names.map(resolve);
    let mut def =
        sim::ClassDef::primary_secondary(sim::ClassId(class_id), 1, resolved[0].0, resolved[1].0);
    def.lethal = resolved[2].0;
    def.tactical = resolved[3].0;
    let mut lock_reason = authoritative_class_lock_reason(names, weapons, combat, equipment);

    if resolved.into_iter().any(|(_, bad)| bad) {
        append_lock_reason(&mut lock_reason, "catalog.unknown".to_owned());
    }
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
    abort: Option<Res<'w, MatchLoadAbort>>,
    host_classes: Option<Res<'w, HostClassLoadouts>>,
    catalog: Option<Res<'w, assets::MenuCatalog>>,
    identity: Option<Res<'w, LaunchIdentity>>,
    install_stage: Local<'s, Option<assets::LoadStage>>,
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
    world: ResMut<'w, AuthorityWorld>,
    input_gate: ResMut<'w, AuthorityInputGate>,
    load_hold: ResMut<'w, AuthorityLoadHold>,
}

#[derive(SystemParam)]
pub struct MatchInstallPresentation<'w> {
    scene: ResMut<'w, WorldScene>,
    probe: Option<ResMut<'w, LaunchReport>>,
    camera: ResMut<'w, SimCamera>,
    class_handoff: ResMut<'w, ClassSelectHandoff>,
}

#[derive(SystemParam)]
pub struct MatchInstallOccupancy<'w> {
    has_world: ResMut<'w, HasWorld>,
    generation: ResMut<'w, WorldGeneration>,
}

pub fn apply_prepared_match(
    mut commands: Commands,
    source: PreparedMatchSource,
    mut console: MatchInstallConsole,
    authority: MatchInstallAuthority,
    presentation: MatchInstallPresentation,
    occupancy: MatchInstallOccupancy,
    mode_selection: Option<Res<sim::HostGameModeSelection>>,
    mut installed: MessageWriter<MatchInstalled>,
) {
    let PreparedMatchSource {
        mut ready,
        swap,
        bridge,
        intent,
        mut loading,
        abort,
        host_classes,
        catalog,
        identity,
        mut install_stage,
    } = source;
    let MatchInstallAuthority {
        world: mut sim,
        mut input_gate,
        mut load_hold,
    } = authority;
    let MatchInstallPresentation {
        mut scene,
        mut probe,
        camera: mut sim_cam,
        mut class_handoff,
    } = presentation;
    let MatchInstallOccupancy {
        mut has_world,
        generation: mut world_generation,
    } = occupancy;
    let stale_key = !ready.load_key.match_key.is_none()
        && bridge.as_ref().is_none_or(|bridge| {
            let state = bridge.state();
            !state.in_match()
                || state.identity().match_key() != ready.load_key.match_key
                || (ready.load_key.incarnation != 0
                    && ready.load_key.incarnation != bridge.incarnation())
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
        let _ = install_stage.take();
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
        let _ = install_stage.take();
        commands.remove_resource::<PreparedMatchReady>();
        commands.remove_resource::<MatchLoadAbort>();
        diag::info!(
            World,
            "match install: aborted request #{dropped} (`{zone}`) — not occupying the menu"
        );
        return;
    }
    let Some(probe) = probe.as_deref_mut() else {
        return;
    };

    if loading.is_some() && install_stage.is_none() {
        if let Some(loading) = loading.as_deref_mut() {
            *install_stage = Some(loading.progress.stage("installing match"));
        }
        diag::info!(
            World,
            "match install: yield one frame so the overlay can name the main-thread hitch"
        );
        return;
    }
    let _install_stage = install_stage.take();
    let install_started = std::time::Instant::now();
    let request_id = ready.request_id;
    let load_key = ready.load_key;
    let zone = std::mem::take(&mut ready.zone);
    let mut prepared = std::mem::take(&mut ready.prepared);
    commands.remove_resource::<PreparedMatchReady>();

    let weapons = PreparedWeapons(prepared.weapons);
    let fpv_meshes = PreparedFpvMeshes(prepared.fpv_meshes);
    let bodies = assets::PreparedBodies(prepared.bodies);
    let world_weapons = assets::PreparedWorldWeapons(prepared.world_weapons);
    let projectile_meshes = assets::PreparedProjectileMeshes(prepared.projectile_meshes);
    let xmodel_walk = std::mem::take(&mut prepared.xmodel_walk);
    let mut xanims = PreparedXAnims(prepared.xanims);
    let death = PreparedDestructibleDeath(stamp_destructible_death_edges(
        &xanims.0,
        &prepared.world.map_xmodel_scene_assets,
        &gsc_death_hints(),
    ));
    log_destructible_death_assets(&death);
    let player_anim_sources = prepared.player_anim_sources;
    let prepared_map = prepared.prepared_map;
    let spawn_count = prepared_map.spawns.len();
    commands.insert_resource(assets::SessionCompass {
        corners: prepared_map.minimap_corners,
        north_yaw: prepared_map.north_yaw,
        declaration: prepared_map.compass.clone(),
    });
    commands.insert_resource(assets::SessionMapScriptSound(
        prepared_map.script_sound.clone(),
    ));
    commands.insert_resource(assets::SessionTeamIcons(prepared_map.team_icons.clone()));
    commands.insert_resource(assets::PreparedLocalizedStrings(std::mem::take(
        &mut prepared.strings,
    )));
    let kind = match match_kind(mode_selection.as_deref()) {
        Ok(kind) => kind,
        Err(gap) => {
            probe.sim_gap = gap;
            diag::info!(Sim, "match install refused: {gap}");
            *has_world = HasWorld(false);
            if let Some(loading) = loading.as_deref_mut() {
                loading.fail(gap);
            }
            console.release_startup();
            return;
        }
    };
    apply_gameobjects_main(&mut prepared.world, kind);
    let flag_descriptors = std::mem::take(&mut prepared.world.flag_descriptors);
    let map_use_triggers = std::mem::take(&mut prepared.world.map_use_triggers);
    let map_doors = crate::map_doors::prepare(&zone, &prepared.world, &map_use_triggers)
        .expect("authored Radiation door bindings");
    let objective_setup: Result<_, String> = (|| {
        let visuals = crate::objectives::prepare(&mut prepared.world, &map_use_triggers, kind)?;
        let mut flags = [String::new(), String::new(), String::new()];
        let mut attackers = gamemode_iw4::Team::Allies;
        if kind == gamemode_iw4::GameModeKind::Domination {
            let catalog = catalog.as_deref().ok_or("DOM faction catalog missing")?;
            let arena = catalog
                .rawfile_text("mp/basemaps.arena")
                .map(str::to_owned)
                .or_else(|| {
                    identity
                        .as_deref()
                        .and_then(|id| assets::read_basemaps_arena(&id.games_root))
                })
                .ok_or("DOM arena faction definitions missing")?;
            flags = crate::objectives::flag_models(catalog, &arena, &zone)?;
            for model in &flags {
                if !matches!(
                    prepared.world.map_xmodel_scene_assets.get_name(model),
                    Some(assets::MapXModelSceneAsset::Iw4(_))
                ) {
                    return Err(format!("DOM flag model unavailable: {model}"));
                }
            }
        }
        if kind == gamemode_iw4::GameModeKind::Demolition {
            attackers = match prepared_map.script_sound.attackers.as_deref() {
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
            probe.sim_gap = "objective mode content unavailable";
            diag::info!(Sim, "objective match refused: {error}");
            *has_world = HasWorld(false);
            if let Some(loading) = loading.as_deref_mut() {
                loading.fail(error);
            }
            console.release_startup();
            return;
        }
    };
    let authority_models = authority_entity_model_install(&prepared.world);
    let animated =
        collect_animated_prop_anims(&prepared.world.script_model_instances, &mut xanims.0);
    let model_spawns = script_model_spawns(&prepared.world.script_model_instances);
    let fx_catalog = PreparedFxCatalog(std::mem::take(&mut prepared.world.fx));
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
    let loaded_scene =
        match world_scene_from_draw(prepared.world, &authority_models.installed_owners) {
            Ok(scene) => scene,
            Err(error) => {
                let gap = format!("match install refused: {error}");
                diag::info!(World, "{gap}");
                *has_world = HasWorld(false);
                if let Some(loading) = loading.as_deref_mut() {
                    loading.fail(gap);
                }
                console.release_startup();
                return;
            }
        };
    commands.insert_resource(fx_catalog);
    commands.insert_resource(type10);
    commands.insert_resource(fx_models);
    commands.insert_resource(impact_fx);
    commands.insert_resource(tracers);

    *scene = loaded_scene;
    sim.0.set_weapon_def_scales(weapons.0.scales_table());
    let combat = combat_table::from_registry(&weapons.0);
    sim.0.set_weapon_combat_table(combat.clone());
    sim.0
        .set_bullet_pen_facts(combat_table::pen_from_registry(&weapons.0));
    sim.0.set_penetration_table(prepared.pen_table);
    sim.0.set_pen_table_loaded(prepared.pen_table_loaded);
    sim.0.set_player_kit_collisions(
        player_kit_collision(&bodies.0, false),
        player_kit_collision(&bodies.0, true),
    );
    if let Some(Ok(tree)) = player_anim_sources.compiled() {
        if let Ok(definition) =
            tree.to_runtime_definition(|_, name| xanims.0.clip(assets::AssetNamespace::Iw4, name))
        {
            let names = tree.nodes().iter().map(|node| node.name.clone()).collect();
            sim.0.set_player_anim_tree(Some(definition), names);
        }
    }
    sim.0
        .set_mantle_xanims(sim::MantleXAnimBind::from_clips(|fast, i| {
            let name = sim::MantleXAnimBind::clip_name(fast, i)?;
            xanims
                .0
                .clip(assets::AssetNamespace::Iw4, name)
                .map(|clip| (*clip).clone())
        }));
    sim.0
        .set_weapon_script_names(weapons.0.script_names_table());
    install_team_voice_prefixes(&mut sim.0, catalog.as_deref(), identity.as_deref(), &zone);
    let equipment = combat_table::equipment_from_registry(&weapons.0);
    sim.0.set_equipment_runtime_table(equipment.clone());
    let weapon_reg = weapons.0.clone();
    let mut primary = Vec::new();
    let mut secondary = Vec::new();
    let mut lethal = Vec::new();
    let mut tactical = Vec::new();
    let mut excluded = Vec::new();
    let loadout_rows = weapon_reg.loadout_catalog();
    let mut attachment_variants = std::collections::HashMap::<u32, Vec<String>>::new();
    for row in &loadout_rows {
        if let assets::LoadoutCatalogKind::AttachmentVariant { base_id } = row.kind {
            attachment_variants
                .entry(base_id)
                .or_default()
                .push(row.key.to_string());
        }
    }
    for variants in attachment_variants.values_mut() {
        variants.sort();
        variants.dedup();
    }
    for row in loadout_rows {
        let offer = CacWeaponOffer {
            key: row.key.to_string(),
            item_group: row.item_group.clone(),
            attachment_variants: attachment_variants.remove(&row.id).unwrap_or_default(),
        };
        let key = offer.key.clone();
        match row.kind {
            assets::LoadoutCatalogKind::Primary => primary.push(offer),
            assets::LoadoutCatalogKind::Secondary => secondary.push(offer),
            assets::LoadoutCatalogKind::Equipment { offhand_class } => {
                match assets::cac_offhand_bucket(offhand_class) {
                    Some(assets::CacOffhandBucket::Lethal) => lethal.push(offer),
                    Some(assets::CacOffhandBucket::Tactical) => tactical.push(offer),
                    None => excluded.push((
                        key,
                        format!("unsupported retail offhandClass {offhand_class}"),
                    )),
                }
            }
            assets::LoadoutCatalogKind::AttachmentVariant { base_id } => excluded.push((
                key,
                format!(
                    "attachment variant of {} (available through typed CAC attachment picker)",
                    weapon_reg.name_of(base_id)
                ),
            )),
            assets::LoadoutCatalogKind::NonPlayer => excluded.push((
                key,
                "not structurally player/create-a-class eligible".to_owned(),
            )),
        }
    }
    commands.insert_resource(weapons);
    commands.insert_resource(fpv_meshes);
    commands.insert_resource(bodies);
    commands.insert_resource(world_weapons);
    commands.insert_resource(projectile_meshes);
    commands.insert_resource(xmodel_walk);
    if let Some(Ok(parsed)) = player_anim_sources.parsed_script() {
        let tree = player_anim_sources
            .compiled()
            .and_then(|c| c.as_ref().ok())
            .map(|t| t.as_ref());
        let slots = map_anim_items(&parsed.slots, tree, &xanims.0);
        let events = map_event_items(&parsed.events, tree, &xanims.0);
        sim.0
            .set_player_anim_script(Some(sim::PlayerAnimScript::from_tables(slots, events)));
    }
    commands.insert_resource(xanims);
    commands.insert_resource(death);
    commands.insert_resource(player_anim_sources);
    input_gate.cmds_enabled = false;

    probe.world_report = prepared.report;
    log_world_report(&probe.world_report);
    let clip_for_dynent = prepared.clip.clone();
    commands.insert_resource(DynEntPhysWorld::default());
    commands.insert_resource(DynEntPhysClip::from_clip(clip_for_dynent));
    let (sim_gap, lock_reasons, bot_class_ids) = install_clip_and_player(
        &mut sim.0,
        prepared.clip,
        authority_models,
        scene.intermission_view.map(|view| sim::AuthoredSpawnPoint {
            classname: gamemode_iw4::playerlogic::MP_GLOBAL_INTERMISSION.to_owned(),
            origin: view.origin,
            angles: view.angles,
            script_linkto: String::new(),
        }),
        &prepared_map.spawns,
        &weapon_reg,
        &combat,
        &equipment,
        &mut sim_cam,
        &mut input_gate,
        host_classes.as_deref(),
        kind,
        &map_use_triggers,
        &flag_descriptors,
    );
    sim.0.objectives.flag_models = objective_flags;
    sim.0.objectives.attackers = objective_attackers;
    let defenders = sim.0.objectives.defenders();
    for site in &mut sim.0.objectives.bombs {
        site.view.owner = defenders;
    }
    for (source, intact, destroyed) in objective_visuals {
        if let Some(site) = sim
            .0
            .objectives
            .bombs
            .iter_mut()
            .find(|b| b.view.model_source == source)
        {
            site.intact_sources = intact;
            site.destroyed_sources = destroyed;
        }
    }
    commands.insert_resource(bots::BotClassPool {
        ready: true,
        ids: bot_class_ids,
    });

    load_hold.0 = sim.0.clip_brush_count() > 0;
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
        sim.0.world_objects_mut().install_glass_panes(panes);
    }
    sim.0.start_script_model_play_anims(animated.rows);
    spawn_script_model_movers(&mut sim.0, &model_spawns);
    if let Some(doors) = map_doors {
        crate::map_doors::install(&mut sim.0, doors).expect("Radiation switch clip bounds");
    }
    stamp_script_mover_numbers(&mut scene, &sim.0);
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
            sim.0.script_mover_count(),
            sim::gentity_spawn_base()
        );
    }
    probe.sim_gap = sim_gap;
    *class_handoff = ClassSelectHandoff {
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
        &weapon_reg,
        &combat,
        &equipment,
        sim.0.content_digest(),
    )
    .expect("installed weapon registry must have unique durable source keys");

    let components = sim.0.content_components();
    diag::info!(
        Sim,
        "session manifest: ruleset=iw4 map={:?} weapons={} digest={:016x} \
         gameplay={:016x} c_map={:016x} c_models={:016x} c_weapons={:016x} c_classes={:016x}",
        manifest.map,
        manifest.weapons.len(),
        manifest.digest,
        sim.0.content_digest(),
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
            commands.insert_resource(descriptor);
        }
        None => {
            diag::error!(
                Sim,
                "session install has no match descriptor (map unverified); remote join will not arm"
            );
        }
    }
    commands.insert_resource(manifest);

    diag::info!(
        World,
        "match install: {:.1}ms on the main thread (scene handoff, sim boot, catalogs)",
        install_started.elapsed().as_secs_f32() * 1000.0
    );
    let drawable = !scene.batches.is_empty();
    perf::match_installed(&zone, i64::from(drawable));
    installed.write(MatchInstalled {
        request_id,
        load_key,
        zone,
        spawn_count,
        drawable,
    });
    commands.insert_resource(crate::LiveWorldIdentity { load_key });

    *has_world = HasWorld(drawable);
    *world_generation = WorldGeneration::from_install(request_id);

    if drawable {
        sim_cam.freeze_fly = true;
        diag::info!(
            Sim,
            "match: {} ({}) — world installed ({} dm spawn points); class select waits for admission",
            kind.display_name(),
            kind.token(),
            spawn_count
        );
    } else {
        diag::error!(
            World,
            "map load failed: no drawable world; see world report in the game log"
        );
        if let Some(loading) = loading.as_deref_mut() {
            loading.fail("No drawable world. See world report in the game log.");
        }
    }

    console.release_startup();
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
    xanims: &mut assets::XAnimCatalog,
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

fn gsc_death_hints() -> [DestructibleDeathHint; 3] {
    let pickup = sim::VehicleDestructibleKind::Pickup;
    let truck = sim::VehicleDestructibleKind::MovingTruck;
    [
        DestructibleDeathHint {
            kind: pickup.as_str(),
            clip: pickup.definition().death.clip,
            husk: pickup.definition().death.husk,
        },
        DestructibleDeathHint {
            kind: truck.as_str(),
            clip: truck.definition().death.clip,
            husk: truck.definition().death.husk,
        },
        DestructibleDeathHint {
            kind: "explodable_barrel",
            clip: "",
            husk: sim::EXPLODABLE_BARREL_HUSK,
        },
    ]
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
    const DEFS: &[&str] = &[
        "smoke/car_damage_blacksmoke_fire",
        "explosions/small_vehicle_explosion",
        "explosions/vehicle_explosion_medium",
        sim::EXPLODABLE_BARREL_DEATH_FX,
        sim::EXPLODABLE_BARREL_BURN_START_FX,
        sim::EXPLODABLE_BARREL_BURN_LOOP_FX,
    ];
    for name in DEFS {
        let status = if fx.0.resolve_def(name).is_some() {
            "captured"
        } else if fx.0.get(name).is_some() {
            "empty"
        } else {
            "MISSING"
        };
        diag::info!(World, "destructible fx {name}: {status}");
    }
    let tanker = gamemode_iw4::dd::PLANTED_BOMB_EXPLODE_FX_PATH.expect("GSC-cited");
    let tanker_status = if fx.0.resolve_def(tanker).is_some() {
        "captured"
    } else if fx.0.get(tanker).is_some() {
        "empty"
    } else {
        "MISSING"
    };
    diag::info!(World, "suitcase explode fx {tanker}: {tanker_status}");
}

struct AuthorityEntityModelInstall {
    capabilities: Vec<sim::EntityCollisionCapabilities>,
    vehicles: Vec<(sim::ScriptModelId, sim::VehicleDestructibleKind, [f32; 3])>,
    toys: Vec<(sim::ScriptModelId, sim::ToyDestructibleKind, [f32; 3])>,
    barrels: Vec<(sim::ScriptModelId, [f32; 3])>,
    installed_owners: Vec<(assets::ScriptModelId, sim::AuthorityModelOwner)>,
    ambiguous_brush_links: usize,
    standalone_brush_links: usize,
}

fn authority_entity_model_install(world: &assets::PreparedWorld) -> AuthorityEntityModelInstall {
    let mut vehicles = Vec::new();
    let mut toys = Vec::new();
    let mut barrels = Vec::new();
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
            let capability = match world.map_xmodel_scene_assets.get(&instance.current_model) {
                Some(
                    assets::MapXModelSceneAsset::Iw4(model)
                    | assets::MapXModelSceneAsset::Iw5(model)
                    | assets::MapXModelSceneAsset::T5(model),
                ) => model.retained_capability().map(std::sync::Arc::new),
                Some(assets::MapXModelSceneAsset::Unavailable { .. }) | None => None,
            };
            let mut dobj = sim::AuthorityDObjState::new_dirty(
                instance.current_model.0.clone(),
                capability,
                instance.transform.to_matrix(),
            );

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
        standalone_brush_links += 1;
    }
    AuthorityEntityModelInstall {
        capabilities,
        vehicles,
        toys,
        barrels,
        installed_owners,
        ambiguous_brush_links,
        standalone_brush_links,
    }
}

fn install_clip_and_player(
    sim: &mut sim::SimWorld,
    clip: Option<ClipCollision>,
    authority_models: AuthorityEntityModelInstall,
    intermission_view: Option<sim::AuthoredSpawnPoint>,
    spawns_in: &[PreparedSpawn],
    weapons: &WeaponRegistry,
    combat: &[sim::WeaponCombatFacts],
    equipment: &[sim::EquipmentRuntimeFacts],
    sim_cam: &mut SimCamera,
    input_gate: &mut AuthorityInputGate,
    host_classes: Option<&HostClassLoadouts>,
    kind: gamemode_iw4::GameModeKind,
    map_use_triggers: &[assets::MapUseTrigger],
    flag_descriptors: &[assets::FlagDescriptor],
) -> (&'static str, Vec<Option<String>>, Vec<sim::ClassId>) {
    sim.world_objects_mut()
        .install_vehicle_destructibles(authority_models.vehicles);
    sim.world_objects_mut()
        .install_toy_destructibles(authority_models.toys);
    sim.world_objects_mut()
        .install_explodable_barrels(authority_models.barrels);
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

    let Some(clip) = clip else {
        diag::info!(
            Sim,
            "spawn: no clip brushes — leaving fly camera in control"
        );
        return (
            "clipmap brushes not retained — fly camera only; sim::step not driving the eye",
            Vec::new(),
            Vec::new(),
        );
    };
    let count = clip.brushes.len();
    let node_count = clip.nodes.len();
    let leaf_count = clip.leaves.len();
    let vert_count = clip.verts.len();
    let tri_count = clip.tri_indices.len() / 3;
    let smodel_count = clip.static_models.len();
    let mut brushes: Vec<sim::SimBrush> = clip
        .brushes
        .into_iter()
        .map(|brush| sim::SimBrush {
            planes: brush.planes,
            contents: brush.contents,
            plane_surface_flags: brush.plane_surface_flags,
            glass_encoded: brush.glass_encoded,
        })
        .collect();
    if let Some(floor) = synthetic_spawn_floor(spawns_in, tri_count, count) {
        diag::info!(
            Sim,
            "spawn: synthetic floor under authored DM feet (no mesh tris / sparse brushes)"
        );
        brushes.push(floor);
    }
    let bsp = sim::SimClipBsp {
        nodes: clip
            .nodes
            .into_iter()
            .map(|n| sim::ClipNode {
                plane: n.plane,
                children: n.children,
            })
            .collect(),
        leaves: clip
            .leaves
            .into_iter()
            .map(|l| sim::ClipLeaf {
                first_brush: l.first_brush,
                num_brushes: l.num_brushes,
                first_coll_aabb_index: l.first_coll_aabb_index,
                coll_aabb_count: l.coll_aabb_count,
            })
            .collect(),
        leafbrushes: clip.leafbrushes,
    };
    let mesh = sim::SimClipMesh {
        verts: clip.verts,
        tri_indices: clip.tri_indices,
        tri_surface_flags: clip.tri_surface_flags,
        tri_content_flags: clip.tri_content_flags,
        aabb_trees: clip.aabb_trees,
        partitions: clip.partitions,
        aabb_roots: clip.aabb_roots,
        static_models: clip
            .static_models
            .into_iter()
            .map(|sm| sim::SimStaticModel {
                index: sm.index,
                name: sm.name,
                model: sm.model,
            })
            .collect(),
        ..Default::default()
    };
    let cmodel_count = clip.cmodels.len();
    let cmodels = sim::SimClipCmodels {
        models: clip
            .cmodels
            .into_iter()
            .map(|c| sim::ClipCmodel {
                mins: c.mins,
                maxs: c.maxs,
                radius: c.radius,
                first_brush: c.first_brush,
                num_brushes: c.num_brushes,
            })
            .collect(),
    };
    sim.set_clip_map(brushes, bsp, mesh, cmodels);
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
        })
        .collect();
    let rows = bootstrap_class_rows(host_classes);
    let projected: Vec<AuthoritativeClassProjection> = rows
        .iter()
        .enumerate()
        .map(|(index, (names, perks, deathstreak))| {
            project_class(
                index as u32,
                [&names[0], &names[1], &names[2], &names[3]],
                [&perks[0], &perks[1], &perks[2]],
                deathstreak,
                weapons,
                combat,
                equipment,
            )
        })
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
        ..Default::default()
    }) {
        diag::info!(Sim, "bootstrap: {err}");
        return (
            "MatchBootstrap refused — class/spawn path unavailable",
            lock_reasons,
            Vec::new(),
        );
    }

    if kind == gamemode_iw4::GameModeKind::Domination {
        match install_dom_flags_from_mapents(sim, map_use_triggers) {
            Ok(ids) => {
                diag::info!(Sim, "dom flags: {} MapEnts volumes", ids.len());
                sim.rebuild_dom_spawn_graph();
            }
            Err(err) => {
                diag::info!(Sim, "dom flags refused: {err:?}");
                return (
                    "dom.gsc onStartGameType flag bootstrap refused",
                    lock_reasons,
                    bot_class_ids,
                );
            }
        }
    }

    crate::objectives::install(sim, weapons, map_use_triggers, kind)
        .expect("authored objective bindings");

    sim_cam.enabled = false;
    input_gate.cmds_enabled = false;
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
    (gap, lock_reasons, bot_class_ids)
}

fn synthetic_spawn_floor(
    spawns: &[PreparedSpawn],
    mesh_tri_count: usize,
    brush_count: usize,
) -> Option<sim::SimBrush> {
    if spawns.is_empty() {
        return None;
    }
    if std::env::var_os("IW4L_NO_SYNTH_FLOOR").is_some_and(|v| v != "0") {
        diag::info!(
            Sim,
            "spawn: IW4L_NO_SYNTH_FLOOR set — no synthetic slab (author geometry only)"
        );
        return None;
    }
    if mesh_tri_count > 0 || brush_count >= 8 {
        diag::info!(
            Sim,
            "spawn: author clip present (mesh_tris={mesh_tri_count} brushes={brush_count}) — no synthetic slab"
        );
        return None;
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for s in spawns {
        for i in 0..3 {
            min[i] = min[i].min(s.origin[i]);
            max[i] = max[i].max(s.origin[i]);
        }
    }
    let pad = 2048.0_f32;
    let top = min[2] - 1.0;
    let bottom = top - 16.0;
    let mins = [min[0] - pad, min[1] - pad, bottom];
    let maxs = [max[0] + pad, max[1] + pad, top];
    Some(sim::SimBrush {
        planes: vec![
            [1.0, 0.0, 0.0, maxs[0]],
            [-1.0, 0.0, 0.0, -mins[0]],
            [0.0, 1.0, 0.0, maxs[1]],
            [0.0, -1.0, 0.0, -mins[1]],
            [0.0, 0.0, 1.0, maxs[2]],
            [0.0, 0.0, -1.0, -mins[2]],
        ],
        contents: sim::CONTENTS_SOLID,
        plane_surface_flags: vec![0; 6],
        glass_encoded: 0,
    })
}

pub(crate) fn bootstrap_class_rows(
    host: Option<&HostClassLoadouts>,
) -> Vec<([String; 4], [String; 3], String)> {
    let fallback = HostClassLoadouts::default();
    let host = host.filter(|h| !h.slots.is_empty()).unwrap_or(&fallback);
    host.slots
        .iter()
        .map(|slot| {
            (
                [
                    slot.primary.clone(),
                    slot.secondary.clone(),
                    slot.lethal.clone(),
                    slot.tactical.clone(),
                ],
                slot.perks.clone(),
                slot.deathstreak.clone(),
            )
        })
        .collect()
}

fn install_team_voice_prefixes(
    world: &mut sim::SimWorld,
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
