//! The single background session load: asset products, atlas preparation and
//! sound composition complete before publication to the main schedule.
use asset_transport::progress::LoadProgress;
use assets::session_load::{
    MatchLoadOutcome, PreparedMatch, atlas_load, load_prepared_match_with_atlas,
};
use assets::{
    MapLoadApproval, MapLoadProcess, MatchLoadAccepted, MatchLoadBusy, MatchLoadDispatch,
    MatchLoadRequest, MergedMap, PreparedMatchSound,
};
use bevy::prelude::*;
use bevy::tasks::{Task, futures_lite::future};
use frame::{ClientSet, LaunchIdentity, MapLoadFailed};

fn loading(world: &World) -> bool {
    *world.resource::<State<frame::MatchScope>>().get() == frame::MatchScope::Loading
}

#[derive(Resource)]
struct MatchLoadTask {
    task: Task<Result<Option<PreparedMatchReady>, MapLoadFailed>>,
    match_key: frame::MatchKey,
    zone: String,
    progress: LoadProgress,
}

#[derive(Resource)]
pub(crate) struct PreparedMatchReady {
    pub match_key: frame::MatchKey,
    pub zone: String,
    pub prepared: Option<PreparedMatch>,
    pub atlas: Option<crate::atlas::PreparedAtlas>,
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
    let merged = request.merged.clone();
    let publish_merged = merged.clone();
    commands.queue(move |world: &mut World| match publish_merged {
        Some(merged) => world.insert_resource(merged),
        None => {
            world.remove_resource::<MergedMap>();
        }
    });
    let task = assets::session_load::load_pool().spawn(async move {
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
                let atlas = crate::atlas::prepare_atlas(
                    merged.as_ref(),
                    std::mem::take(&mut prepared.atlas_worlds),
                    &mut prepared.clip,
                    &progress,
                );
                if progress.is_canceled() {
                    return Ok(None);
                }
                let atlas = atlas.map_err(|error| MapLoadFailed {
                    match_key,
                    zone: zone.clone(),
                    error,
                })?;
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
                    atlas,
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

pub(crate) fn register(app: &mut App) {
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
            start_match_load.run_if(in_state(frame::MatchScope::Loading)),
            poll_match_load.after(start_match_load),
        )
            .in_set(MatchLoadDispatch)
            .after(MapLoadApproval)
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
