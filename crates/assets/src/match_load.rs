use std::path::PathBuf;

use bevy::prelude::*;
use frame::{ClientSet, LaunchIdentity, MapLoadApproved, MapLoadFailed};

use crate::merged_map::MergedMap;
use asset_transport::progress::LoadProgress;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapLoadApproval;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchLoadDispatch;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MatchLoadBusy(pub bool);

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

#[derive(Resource, Clone, Debug)]
pub struct MatchLoadAccepted {
    pub match_key: frame::MatchKey,
    pub zone: String,
    pub families: frame::AssetFamilies,
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
                    .scene
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

pub fn register_match_load_systems(app: &mut App) {
    app.add_systems(
        PreUpdate,
        approve_map_load
            .in_set(MapLoadApproval)
            .after(frame::SessionSwapApplied)
            .in_set(ClientSet::Load),
    );
}
