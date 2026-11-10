use bevy::prelude::*;
use frame::FxSoundPublished;
use fx::{FxMsec, set_presentation_clock};
use net::{
    ClientSet, FrameClock, GameActive, LastAdoptedSnapshot, advance_cg_frame_clock,
    reconcile_prediction,
};
use render_anim::sync_camera_from_presented;
use render_scene::{FlyCamera, WorldPresentFacts};

use crate::{FxCameraOrigin, HostFxSystem, PreparedFxCatalog, PresentedVehicleFx};

pub fn register_fx_orchestration(app: &mut App) {
    app.add_systems(
        Update,
        (
            latch_cgame_active
                .in_set(frame::InMatch)
                .in_set(ClientSet::Reconcile)
                .after(reconcile_prediction)
                .before(advance_cg_frame_clock),
            stamp_presentation_clock
                .in_set(frame::InMatch)
                .in_set(ClientSet::Reconcile)
                .after(advance_cg_frame_clock),
        ),
    )
    .add_systems(
        PreUpdate,
        clear_presented_vehicle_fx
            .in_set(frame::InMatch)
            .in_set(ClientSet::Receive),
    )
    .add_systems(
        Update,
        stamp_fx_camera_origin
            .in_set(frame::InMatch)
            .after(sync_camera_from_presented)
            .in_set(ClientSet::Present),
    )
    .add_systems(
        Update,
        play_pending_fx_sounds
            .in_set(frame::InMatch)
            .before(FxSoundPublished)
            .in_set(ClientSet::Effects),
    );
}

pub fn stamp_fx_camera_origin(
    cameras: Query<&Transform, With<FlyCamera>>,
    mut origin: ResMut<FxCameraOrigin>,
) {
    origin.0 = cameras
        .iter()
        .next()
        .map(|transform| transform.translation.to_array())
        .unwrap_or([0.0; 3]);
}

fn latch_cgame_active(
    facts: Res<WorldPresentFacts>,
    adopted: Option<Res<LastAdoptedSnapshot>>,
    mut active: ResMut<GameActive>,
) {
    let spawned = facts.spawned;
    let has_snap = adopted.as_ref().is_some_and(|snap| snap.next().is_some());
    active.0 = GameActive::from_first_snapshot(spawned, has_snap);
}

fn stamp_presentation_clock(mut host: ResMut<HostFxSystem>, clock: Res<FrameClock>) {
    let msec = FxMsec(clock.time());
    set_presentation_clock(&mut host.0, msec);
}

fn play_pending_fx_sounds(
    mut host: ResMut<HostFxSystem>,
    catalog: Option<Res<PreparedFxCatalog>>,
    bank: Option<Res<audio::SoundBank>>,
    clock: Res<FrameClock>,
    mut output: MessageWriter<audio::AliasCommand>,
) {
    let Some(catalog) = catalog else {
        return;
    };
    let Some(bank) = bank else {
        return;
    };
    let batch = std::mem::take(&mut host.0.pending_sounds);
    for req in batch {
        if req.msec_begin < clock.old_time() {
            continue;
        }
        match crate::present::catalog_lookup(&catalog.0, req.catalog_index)
            .and_then(|parent| parent.elems.get(req.def_index as usize))
            .map(|elem| elem.sound_in_bank(req.random_seed, &bank.0))
        {
            None | Some(asset_game::FxBankSound::Gap) => {
                diag::warn_once!(
                    World,
                    "elem {def_index} is Sound and its Bound alias could not play",
                    def_index = req.def_index
                );
            }
            Some(asset_game::FxBankSound::Silent) => {}
            Some(asset_game::FxBankSound::Play { namespace, alias }) => {
                output.write(audio::AliasCommand::Play(audio::PlayAlias {
                    event: None,
                    namespace,
                    alias: alias.to_owned(),
                    fallback: None,
                    origin_inches: Some(req.origin),
                    snd_ent: Some(fx_iw4::FX_ENTITYNUM_WORLD),
                }));
            }
        }
    }
}

fn clear_presented_vehicle_fx(mut presented: ResMut<PresentedVehicleFx>) {
    presented.by_id.clear();
}
