use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{Task, futures_lite::future};
use frame::{ClientSet, LaunchIdentity, MapLoadApproved, MapLoadFailed};

use crate::map_load_process::MapLoadProcess;
use crate::merged_map::MergedMap;
use crate::session_load::{
    MatchLoadOutcome, PreparedMatch, atlas_load, load_prepared_match_with_atlas,
};
use asset_transport::progress::LoadProgress;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapLoadApproval;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchLoadDispatch;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MatchLoadBusy(pub bool);

fn loading(world: &World) -> bool {
    *world.resource::<State<frame::MatchScope>>().get() == frame::MatchScope::Loading
}

#[derive(Resource)]
pub struct MatchLoadRequest {
    pub match_key: frame::MatchKey,

    pub zone: String,
    pub zone_ff: Result<PathBuf, String>,
    pub common_mp: Result<PathBuf, String>,
    pub progress: LoadProgress,
    pub families: frame::AssetFamilies,
    /// The merged map `zone` is world 0 of, if the request named one.
    pub merged: Option<MergedMap>,
}

#[derive(Resource)]
struct MatchLoadTask {
    task: Task<Result<Option<PreparedMatchReady>, MapLoadFailed>>,

    match_key: frame::MatchKey,
    zone: String,
    progress: LoadProgress,
}

#[derive(Resource, Clone, Debug)]
pub struct MatchLoadAccepted {
    pub match_key: frame::MatchKey,
    pub zone: String,
    pub families: frame::AssetFamilies,
}

#[derive(Resource)]
pub struct PreparedMatchReady {
    pub match_key: frame::MatchKey,
    pub zone: String,
    pub prepared: Option<PreparedMatch>,
}

#[derive(Resource)]
pub struct PreparedMatchSound {
    pub namespace: asset_core::FamilyId,
    pub match_key: frame::MatchKey,
    pub zone: String,
    pub common_profile_id: u64,
    pub sound: Result<asset_audio::SoundCatalog, String>,
    pub gaps: usize,
}

fn start_match_load(
    mut commands: Commands,
    identity: Option<Res<LaunchIdentity>>,
    request: Option<Res<MatchLoadRequest>>,
    busy: Option<Res<MatchLoadTask>>,
    retiring: Option<Res<frame::Retiring>>,
    mut inflight: ResMut<MatchLoadBusy>,
    mut failed: MessageWriter<MapLoadFailed>,
) {
    let Some(request) = request else {
        return;
    };
    if busy.is_some() {
        return;
    }
    if retiring.as_ref().is_some_and(|retiring| retiring.busy()) {
        return;
    }
    if let Some(error) = request
        .zone_ff
        .as_ref()
        .err()
        .or_else(|| request.common_mp.as_ref().err())
    {
        failed.write(MapLoadFailed {
            match_key: request.match_key,
            zone: request.zone.clone(),
            error: error.clone(),
        });
        commands.remove_resource::<MatchLoadRequest>();
        inflight.0 = false;
        return;
    }
    let zone_ff = request.zone_ff.clone();
    let common_mp = request.common_mp.clone();
    let progress = request.progress.clone();

    let match_key = request.match_key;
    let zone = request.zone.clone();
    let zone_accepted = zone.clone();
    let zone_task = zone.clone();
    let cancel_handle = progress.clone();
    // The process outlives the overlay and every task this spawns: it is what
    // the table reads, what admission finishes, and what the bench still has
    // once the screen is gone.
    let process = MapLoadProcess::new(match_key, zone.clone(), cancel_handle.clone());
    commands.queue(move |world: &mut World| {
        if loading(world) {
            world.insert_resource(process);
        }
    });

    let families = request.families.clone();
    let accepted_families = families.clone();
    let games = identity.map(|identity| asset_transport::GamesRoot(identity.games_root.clone()));
    let atlas = atlas_load(games.as_ref(), request.merged.as_ref());
    // What the match installs the sim's atlas from (`session::atlas`).
    let merged = request.merged.clone();
    commands.queue(move |world: &mut World| match merged {
        Some(merged) => world.insert_resource(merged),
        None => {
            world.remove_resource::<MergedMap>();
        }
    });
    let task = crate::session_load::load_pool().spawn(async move {
        let sound_path = zone_ff.clone();
        match load_prepared_match_with_atlas(
            zone_ff,
            common_mp,
            progress.clone(),
            families.clone(),
            atlas,
        )
        .await
        {
            MatchLoadOutcome::Ready(mut prepared) => {
                if progress.is_canceled() {
                    return Ok(None);
                }
                prepared.sound = Some(match (games, sound_path) {
                    (Some(games), Ok(path)) => {
                        let sources =
                            asset_audio::gather_sound_sources_filtered(&games, &path, |ns| {
                                families.enabled(ns.as_str())
                            });
                        let map = prepared
                            .sound
                            .take()
                            .unwrap_or_else(|| Err("the map zone never opened".into()));
                        let bank = asset_audio::compose_sound_bank(
                            sources,
                            &zone,
                            prepared
                                .prepared_map
                                .namespace
                                .expect("prepared map family"),
                            map,
                        );
                        prepared.sound_gaps = bank.gaps.len();
                        prepared.script_sound_aliases = Some(bank.catalog.script_alias_looping());
                        Ok(bank.catalog)
                    }
                    _ => Err("no launch identity or map path to compose the sound bank".into()),
                });
                if progress.is_canceled() {
                    return Ok(None);
                }
                Ok(Some(PreparedMatchReady {
                    match_key,
                    zone,
                    prepared: Some(*prepared),
                }))
            }
            MatchLoadOutcome::Canceled => Ok(None),
            MatchLoadOutcome::Refused(error) => Err(MapLoadFailed {
                match_key,
                zone,
                error,
            }),
        }
    });
    inflight.0 = true;
    commands.queue(move |world: &mut World| {
        if !loading(world) {
            return;
        }
        frame::scope::insert(
            world,
            MatchLoadAccepted {
                match_key,
                zone: zone_accepted,
                families: accepted_families,
            },
            frame::MatchScope::Loading,
        );
    });
    commands.queue(move |world: &mut World| {
        if !loading(world) {
            cancel_handle.cancel();
            return;
        }
        frame::scope::insert(
            world,
            MatchLoadTask {
                task,
                match_key,
                zone: zone_task,
                progress: cancel_handle,
            },
            frame::MatchScope::Loading,
        );
    });
    commands.remove_resource::<MatchLoadRequest>();
}

fn approve_map_load(
    mut commands: Commands,
    mut approved: MessageReader<MapLoadApproved>,
    mut failed: MessageWriter<MapLoadFailed>,
    identity: Res<LaunchIdentity>,
    role: Res<frame::RuntimeRole>,
    dvars: Res<frame::UiMenuDvars>,
    server_families: Option<Res<frame::AssetFamilies>>,
) {
    for request in approved.read() {
        let merged = match MergedMap::find(&identity.artifacts, &request.zone).transpose() {
            Ok(merged) => merged,
            Err(error) => {
                failed.write(MapLoadFailed {
                    match_key: request.match_key,
                    zone: request.zone.clone(),
                    error,
                });
                continue;
            }
        };
        let games = asset_transport::GamesRoot(identity.games_root.clone());
        let trees = asset_transport::NamespaceTrees::discover(&games);
        let have =
            frame::AssetFamilies::from_families(trees.present().map(|(ns, _)| ns.as_str())).0;
        let selection = if *role == frame::RuntimeRole::Client || !request.match_key.is_none() {
            server_families
                .as_deref()
                .cloned()
                .ok_or_else(|| "Server asset families were not supplied".to_owned())
        } else {
            frame::AssetFamilies::from_inventory(&have, &dvars)
        };
        let selection = selection.and_then(|families| {
            match &merged {
                Some(merged) => merged
                    .worlds
                    .iter()
                    .try_for_each(|zone| families.require_map(zone))?,
                None => families.require_map(&request.zone)?,
            }
            if let Some(message) = families.missing_notice(&have) {
                return Err(message);
            }
            Ok(families)
        });
        let families = match selection {
            Ok(families) => families,
            Err(error) => {
                failed.write(MapLoadFailed {
                    match_key: request.match_key,
                    zone: request.zone.clone(),
                    error,
                });
                continue;
            }
        };
        let map_zone = merged
            .as_ref()
            .map_or(request.zone.as_str(), MergedMap::zone);
        let found = asset_transport::find_zone_file(&games, map_zone);
        let zone = found
            .as_ref()
            .map(|zone| zone.zone_name.clone())
            .unwrap_or_else(|_| map_zone.to_owned());
        let zone_ff = found.map(|zone| zone.path);
        let common_mp = match &zone_ff {
            Ok(path) => asset_transport::find_runtime_common_mp(&games, path).map(|zone| zone.path),
            Err(error) => Err(error.clone()),
        };
        let error = zone_ff
            .as_ref()
            .err()
            .or_else(|| common_mp.as_ref().err())
            .cloned();
        if let Some(error) = error {
            failed.write(MapLoadFailed {
                match_key: request.match_key,
                zone,
                error,
            });
            continue;
        }
        // Its own state, never the live map's: tasks from the request before
        // this one still hold handles, and they must not land in these rows.
        let progress = LoadProgress::new();
        let candidate = MatchLoadRequest {
            match_key: request.match_key,
            zone,
            zone_ff,
            common_mp,
            progress,
            families,
            merged,
        };
        commands.queue(move |world: &mut World| {
            world.insert_resource(candidate);
        });
    }
}

fn poll_match_load(
    mut commands: Commands,
    mut task: Option<ResMut<MatchLoadTask>>,
    mut inflight: ResMut<MatchLoadBusy>,
    mut failed: MessageWriter<MapLoadFailed>,
) {
    let Some(task) = task.as_deref_mut() else {
        return;
    };

    let polled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        future::block_on(future::poll_once(&mut task.task))
    }));
    let outcome = match polled {
        Ok(None) => return,
        Ok(Some(outcome)) => outcome,
        Err(panic) => Err(walk_panic_failure(task, panic.as_ref())),
    };

    inflight.0 = false;
    commands.remove_resource::<MatchLoadTask>();
    commands.remove_resource::<MatchLoadAccepted>();
    let outcome = match outcome {
        Ok(ready) => ready,
        Err(error) => {
            failed.write(error);
            return;
        }
    };
    let Some(mut ready) = outcome else {
        diag::info!(World, "match load: canceled walk returned nothing prepared");
        return;
    };
    let Some(prepared) = ready.prepared.as_mut() else {
        return;
    };
    let Some(namespace) = prepared.prepared_map.namespace else {
        diag::error!(World, "prepared match has no family");
        return;
    };
    let map_sound = PreparedMatchSound {
        namespace,
        match_key: ready.match_key,
        zone: ready.zone.clone(),
        common_profile_id: prepared.materials.common_profile_id,
        gaps: prepared.sound_gaps,
        sound: prepared
            .sound
            .take()
            .unwrap_or_else(|| Err("the map zone never opened".to_owned())),
    };
    commands.queue(move |world: &mut World| {
        if !loading(world) {
            return;
        }
        frame::scope::insert(world, map_sound, frame::MatchScope::Loading);
        frame::scope::insert(world, ready, frame::MatchScope::Loading);
    });
}

fn walk_panic_failure(task: &MatchLoadTask, panic: &(dyn std::any::Any + Send)) -> MapLoadFailed {
    let message = panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("no message");
    diag::error!(
        World,
        "match load: walk `{}` panicked: {message} — reported as a failed map load",
        task.zone
    );
    MapLoadFailed {
        match_key: task.match_key,
        zone: task.zone.clone(),
        error: format!("The load stopped on an internal error: {message}"),
    }
}

pub fn register_match_load_systems(app: &mut App) {
    if !app.is_plugin_added::<frame::ScopePlugin>() {
        app.add_plugins(frame::ScopePlugin);
    }
    app.add_systems(
        OnExit(frame::MatchScope::Loading),
        cancel_loading.in_set(frame::ScopeSet::Release),
    );
    app.init_resource::<MatchLoadBusy>().add_systems(
        PreUpdate,
        (
            approve_map_load.in_set(MapLoadApproval),
            (
                start_match_load.run_if(in_state(frame::MatchScope::Loading)),
                poll_match_load.after(start_match_load),
            )
                .in_set(MatchLoadDispatch)
                .after(MapLoadApproval),
        )
            .after(frame::SessionSwapApplied)
            .in_set(ClientSet::Load),
    );
}

fn cancel_loading(
    mut commands: Commands,
    task: Option<Res<MatchLoadTask>>,
    request: Option<Res<MatchLoadRequest>>,
    mut busy: ResMut<MatchLoadBusy>,
) {
    if let Some(task) = task {
        task.progress.cancel();
    }
    if let Some(request) = request {
        request.progress.cancel();
        commands.remove_resource::<MatchLoadRequest>();
    }
    busy.0 = false;
}
