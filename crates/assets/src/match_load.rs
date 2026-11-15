use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{Task, futures_lite::future};
use frame::{ClientSet, LaunchIdentity, MapLoadApproved, MapLoadFailed};

use crate::map_load_process::MapLoadProcess;
use crate::session_load::{MatchLoadOutcome, PreparedMatch, load_prepared_match};
use asset_transport::progress::LoadProgress;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapLoadApproval;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchLoadDispatch;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MatchLoadBusy(pub bool);

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchLoadAbort(pub u64);

#[derive(Resource, Default)]
pub struct CurrentMatchLoad(pub Option<frame::LocalLoadKey>);

fn current_load(world: &World, key: frame::LocalLoadKey) -> bool {
    world.resource::<CurrentMatchLoad>().0 == Some(key)
        && world
            .get_resource::<MatchLoadAbort>()
            .is_none_or(|abort| abort.0 != key.local_load_request_id)
}

#[derive(Resource)]
pub struct MatchLoadRequest {
    pub request_id: u64,
    pub load_key: frame::LocalLoadKey,

    pub zone: String,
    pub zone_ff: Result<PathBuf, String>,
    pub common_mp: Result<PathBuf, String>,
    pub progress: LoadProgress,
    pub families: frame::AssetFamilies,
}

#[derive(Resource)]
struct MatchLoadTask {
    task: Task<Result<Option<PreparedMatchReady>, MapLoadFailed>>,
    request_id: u64,
    load_key: frame::LocalLoadKey,
    zone: String,
    progress: LoadProgress,
}

#[derive(Resource, Clone, Debug)]
pub struct MatchLoadAccepted {
    pub request_id: u64,
    pub load_key: frame::LocalLoadKey,
    pub zone: String,
    pub families: frame::AssetFamilies,
}

#[derive(Resource)]
pub struct PreparedMatchReady {
    pub request_id: u64,
    pub load_key: frame::LocalLoadKey,
    pub zone: String,
    pub prepared: Option<PreparedMatch>,
}

#[derive(Resource)]
pub struct PreparedMatchSound {
    pub namespace: asset_core::AssetNamespace,
    pub load_key: frame::LocalLoadKey,
    pub zone: String,
    pub common_profile_id: u64,
    pub products_id: u64,
    pub sound: Result<asset_audio::SoundCatalog, String>,
    pub gaps: usize,
}

fn start_match_load(
    mut commands: Commands,
    identity: Option<Res<LaunchIdentity>>,
    request: Option<Res<MatchLoadRequest>>,
    busy: Option<Res<MatchLoadTask>>,
    retiring: Option<Res<frame::Retiring>>,
    abort: Option<Res<MatchLoadAbort>>,
    mut waiting_for_retirement: Local<Option<u64>>,
    mut inflight: ResMut<MatchLoadBusy>,
    mut failed: MessageWriter<MapLoadFailed>,
) {
    let Some(request) = request else {
        return;
    };
    if abort
        .as_ref()
        .is_some_and(|abort| abort.0 == request.request_id)
    {
        *waiting_for_retirement = None;
        let load_key = request.load_key;
        commands.queue(move |world: &mut World| {
            if world
                .get_resource::<MatchLoadRequest>()
                .is_some_and(|request| request.load_key == load_key)
            {
                world.remove_resource::<MatchLoadRequest>();
            }
        });
        if busy.is_none() {
            inflight.0 = false;
        }
        diag::info!(
            World,
            "match load: abort request #{} before the walk started",
            request.request_id
        );
        return;
    }
    if busy.is_some() {
        return;
    }
    if retiring.as_ref().is_some_and(|retiring| retiring.busy()) {
        if *waiting_for_retirement != Some(request.request_id) {
            diag::info!(
                World,
                "match load: request #{} waits for retired resources",
                request.request_id
            );
            *waiting_for_retirement = Some(request.request_id);
        }
        return;
    }
    if *waiting_for_retirement == Some(request.request_id) {
        diag::info!(
            World,
            "match load: request #{} resumes after retirement",
            request.request_id
        );
    }
    *waiting_for_retirement = None;
    if let Some(error) = request
        .zone_ff
        .as_ref()
        .err()
        .or_else(|| request.common_mp.as_ref().err())
    {
        failed.write(MapLoadFailed {
            request_id: request.request_id,
            load_key: request.load_key,
            zone: request.zone.clone(),
            error: error.clone(),
        });
        let load_key = request.load_key;
        commands.queue(move |world: &mut World| {
            if world
                .get_resource::<MatchLoadRequest>()
                .is_some_and(|request| request.load_key == load_key)
            {
                world.remove_resource::<MatchLoadRequest>();
            }
        });
        inflight.0 = false;
        return;
    }
    let zone_ff = request.zone_ff.clone();
    let common_mp = request.common_mp.clone();
    let progress = request.progress.clone();
    let request_id = request.request_id;
    let load_key = request.load_key;
    let zone = request.zone.clone();
    let zone_accepted = zone.clone();
    let zone_task = zone.clone();
    let cancel_handle = progress.clone();
    // The process outlives the overlay and every task this spawns: it is what
    // the table reads, what admission finishes, and what the bench still has
    // once the screen is gone.
    let process = MapLoadProcess::new(request_id, load_key, zone.clone(), cancel_handle.clone());
    commands.queue(move |world: &mut World| {
        if current_load(world, load_key) {
            world.insert_resource(process);
        }
    });

    let families = request.families;
    let games = identity.map(|identity| asset_transport::GamesRoot(identity.games_root.clone()));
    let task = crate::session_load::load_pool().spawn(async move {
        let sound_path = zone_ff.clone();
        match load_prepared_match(zone_ff, common_mp, progress.clone(), families).await {
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
                        prepared.script_sound_length =
                            Some(bank.catalog.script_alias_length(games));
                        Ok(bank.catalog)
                    }
                    _ => Err("no launch identity or map path to compose the sound bank".into()),
                });
                if progress.is_canceled() {
                    return Ok(None);
                }
                Ok(Some(PreparedMatchReady {
                    request_id,
                    load_key,
                    zone,
                    prepared: Some(prepared),
                }))
            }
            MatchLoadOutcome::Canceled => Ok(None),
            MatchLoadOutcome::Refused(error) => Err(MapLoadFailed {
                request_id,
                load_key,
                zone,
                error,
            }),
        }
    });
    inflight.0 = true;
    commands.queue(move |world: &mut World| {
        if !current_load(world, load_key) {
            return;
        }
        frame::scope::insert(
            world,
            MatchLoadAccepted {
                request_id,
                load_key,
                zone: zone_accepted,
                families,
            },
            frame::MatchScope::Loading,
        );
    });
    commands.queue(move |world: &mut World| {
        if !current_load(world, load_key) {
            cancel_handle.cancel();
            return;
        }
        frame::scope::insert(
            world,
            MatchLoadTask {
                task,
                request_id,
                load_key,
                zone: zone_task,
                progress: cancel_handle,
            },
            frame::MatchScope::Loading,
        );
    });
    commands.queue(move |world: &mut World| {
        if world
            .get_resource::<MatchLoadRequest>()
            .is_some_and(|request| request.load_key == load_key)
        {
            world.remove_resource::<MatchLoadRequest>();
        }
    });
}

fn approve_map_load(
    mut commands: Commands,
    mut approved: MessageReader<MapLoadApproved>,
    mut failed: MessageWriter<MapLoadFailed>,
    identity: Res<LaunchIdentity>,
    role: Res<frame::RuntimeRole>,
    dvars: Res<frame::UiMenuDvars>,
    server_families: Option<Res<frame::AssetFamilies>>,
    abort: Option<Res<MatchLoadAbort>>,
    mut current: ResMut<CurrentMatchLoad>,
) {
    for request in approved.read() {
        current.0 = Some(request.load_key);
        if abort
            .as_ref()
            .is_some_and(|abort| abort.0 == request.request_id)
        {
            diag::info!(
                World,
                "match load: abort approved request #{} (`{}`) — no walk",
                request.request_id,
                request.zone
            );
            continue;
        }
        let games = asset_transport::GamesRoot(identity.games_root.clone());
        let campaign = asset_transport::is_campaign_level(&games, &request.zone);
        asset_transport::set_campaign_zone_set(campaign);
        if campaign {
            diag::info!(
                World,
                "match load: `{}` is a campaign level; campaign zone set and dropping common/resident",
                request.zone
            );
            crate::session_load::forget_common();
            crate::session_load::clear_resident_map();
        }
        let trees = asset_transport::NamespaceTrees::discover(&games);
        let have =
            frame::AssetFamilies::from_families(trees.present().map(|(ns, _)| ns.as_str())).0;
        let selection = if campaign {
            Ok(frame::AssetFamilies::iw4())
        } else if *role == frame::RuntimeRole::Client || !request.load_key.match_key.is_none() {
            server_families
                .as_deref()
                .copied()
                .ok_or_else(|| "Server asset families were not supplied".to_owned())
        } else {
            frame::AssetFamilies::from_inventory(have, &dvars)
        };
        let selection = selection.and_then(|families| {
            families.require_map(&request.zone)?;
            if let Some(message) = families.missing_notice(have) {
                return Err(message);
            }
            Ok(families)
        });
        let families = match selection {
            Ok(families) => families,
            Err(error) => {
                failed.write(MapLoadFailed {
                    request_id: request.request_id,
                    load_key: request.load_key,
                    zone: request.zone.clone(),
                    error,
                });
                continue;
            }
        };
        let found = asset_transport::find_zone_file(&games, &request.zone);
        let zone = found
            .as_ref()
            .map(|zone| zone.zone_name.clone())
            .unwrap_or_else(|_| request.zone.clone());
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
                request_id: request.request_id,
                load_key: request.load_key,
                zone,
                error,
            });
            continue;
        }
        // Its own state, never the live map's: tasks from the request before
        // this one still hold handles, and they must not land in these rows.
        let progress = LoadProgress::new(request.request_id);
        let candidate = MatchLoadRequest {
            request_id: request.request_id,
            load_key: request.load_key,
            zone,
            zone_ff,
            common_mp,
            progress,
            families,
        };
        commands.queue(move |world: &mut World| {
            if current_load(world, candidate.load_key) {
                world.insert_resource(candidate);
            }
        });
    }
}

fn poll_match_load(
    mut commands: Commands,
    mut task: Option<ResMut<MatchLoadTask>>,
    mut inflight: ResMut<MatchLoadBusy>,
    abort: Option<Res<MatchLoadAbort>>,
    mut failed: MessageWriter<MapLoadFailed>,
    current: Res<CurrentMatchLoad>,
) {
    let Some(task) = task.as_deref_mut() else {
        return;
    };

    if (current.0 != Some(task.load_key)
        || abort
            .as_ref()
            .is_some_and(|abort| abort.0 == task.request_id))
        && !task.progress.is_canceled()
    {
        task.progress.cancel();
        diag::info!(
            World,
            "match load: cancel in-flight walk request #{} — the walk returns at its next checkpoint",
            task.request_id
        );
    }
    let polled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        future::block_on(future::poll_once(&mut task.task))
    }));
    let outcome = match polled {
        Ok(None) => return,
        Ok(Some(outcome)) => outcome,
        Err(panic) => Err(walk_panic_failure(task, panic.as_ref())),
    };
    let request_id = task.request_id;
    let load_key = task.load_key;
    inflight.0 = false;
    commands.queue(move |world: &mut World| {
        if world
            .get_resource::<MatchLoadTask>()
            .is_some_and(|task| task.load_key == load_key)
        {
            world.remove_resource::<MatchLoadTask>();
        }
        if world
            .get_resource::<MatchLoadAccepted>()
            .is_some_and(|accepted| accepted.load_key == load_key)
        {
            world.remove_resource::<MatchLoadAccepted>();
        }
    });
    if current.0 != Some(load_key) {
        return;
    }
    let outcome = match outcome {
        Ok(ready) => ready,
        Err(error) => {
            if !abort
                .as_ref()
                .is_some_and(|abort| abort.0 == error.request_id)
            {
                failed.write(error);
            }
            return;
        }
    };
    let Some(mut ready) = outcome else {
        diag::info!(
            World,
            "match load: walk request #{request_id} returned canceled — nothing prepared"
        );
        return;
    };
    if abort.is_some_and(|abort| abort.0 == ready.request_id) {
        diag::info!(
            World,
            "match load: discarded aborted walk request #{} (`{}`)",
            ready.request_id,
            ready.zone
        );
        return;
    }
    let Some(prepared) = ready.prepared.as_mut() else {
        return;
    };
    let Some(namespace) = prepared.prepared_map.namespace else {
        diag::error!(World, "prepared match has no family");
        return;
    };
    let map_sound = PreparedMatchSound {
        namespace,
        load_key: ready.load_key,
        zone: ready.zone.clone(),
        common_profile_id: prepared.materials.common_profile_id,
        products_id: prepared.materials.products_id,
        gaps: prepared.sound_gaps,
        sound: prepared
            .sound
            .take()
            .unwrap_or_else(|| Err("the map zone never opened".to_owned())),
    };
    commands.queue(move |world: &mut World| {
        if !current_load(world, ready.load_key) {
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
        "match load: walk request #{} (`{}`) panicked: {message} — reported as a failed map load",
        task.request_id,
        task.zone
    );
    MapLoadFailed {
        request_id: task.request_id,
        load_key: task.load_key,
        zone: task.zone.clone(),
        error: format!("The load stopped on an internal error: {message}"),
    }
}

fn expire_match_load_abort(
    mut commands: Commands,
    abort: Option<Res<MatchLoadAbort>>,
    task: Option<Res<MatchLoadTask>>,
    request: Option<Res<MatchLoadRequest>>,
    ready: Option<Res<PreparedMatchReady>>,
) {
    if abort.is_none() {
        return;
    }
    if task.is_some() || request.is_some() || ready.is_some() {
        return;
    }
    let request_id = abort.expect("checked abort").0;
    commands.queue(move |world: &mut World| {
        if world
            .get_resource::<MatchLoadAbort>()
            .is_some_and(|abort| abort.0 == request_id)
            && !world.contains_resource::<MatchLoadTask>()
            && !world.contains_resource::<MatchLoadRequest>()
            && !world.contains_resource::<PreparedMatchReady>()
        {
            world.remove_resource::<MatchLoadAbort>();
        }
    });
}

pub fn register_match_load_systems(app: &mut App) {
    if !app.is_plugin_added::<frame::ScopePlugin>() {
        app.add_plugins(frame::ScopePlugin);
    }
    app.add_systems(
        OnExit(frame::MatchScope::Loading),
        cancel_loading.in_set(frame::ScopeSet::Release),
    );
    app.init_resource::<CurrentMatchLoad>()
        .init_resource::<MatchLoadBusy>()
        .add_systems(
            Update,
            (
                approve_map_load.in_set(MapLoadApproval),
                (
                    start_match_load.run_if(in_state(frame::MatchScope::Loading)),
                    poll_match_load.after(start_match_load),
                    expire_match_load_abort.after(poll_match_load),
                )
                    .in_set(MatchLoadDispatch)
                    .after(MapLoadApproval),
            )
                .after(frame::SessionSwapApplied)
                .in_set(ClientSet::Load),
        );
}

fn cancel_loading(task: Option<Res<MatchLoadTask>>, mut busy: ResMut<MatchLoadBusy>) {
    if let Some(task) = task {
        task.progress.cancel();
    }
    busy.0 = false;
}
