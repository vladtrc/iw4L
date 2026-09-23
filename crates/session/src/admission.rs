use assets::LoadingScreen;
use bevy::prelude::*;
use frame::{AppScreen, ClientSet, HasWorld, RuntimeRole};
use net::{AuthorityLoadHold, ClientAdmission, SignonPhase, SignonState};
use render_frontend::prepare::scene::world::WorldScene;

use crate::LiveWorldIdentity;

pub fn update_admission(
    mut signon: ResMut<SignonState>,
    mut admission: ResMut<ClientAdmission>,
    role: Res<RuntimeRole>,
    hold: Option<Res<AuthorityLoadHold>>,
    has_world: Option<Res<HasWorld>>,
    scene: Option<Res<WorldScene>>,
    audio: Option<Res<audio::AudioReady>>,
    mut live: Option<ResMut<LiveWorldIdentity>>,
) {
    if let (Some(live), Some(installed)) = (live.as_mut(), admission.core.installed())
        && live.load_key.local_load_request_id == installed.local_load_request_id
        && live.load_key.match_key.is_none()
    {
        live.load_key = installed;
    }
    let audio_ready = audio.is_none_or(|ready| ready.0);
    let presentation_ready = scene.is_some_and(|scene| scene.spawned) && audio_ready;
    if presentation_ready && let Some(live) = live.as_ref() {
        admission.core.apply_presentation(live.load_key);
    }
    let world_installed = has_world.is_some_and(|world| world.0);
    let authority_ready = !hold.is_some_and(|hold| hold.0);
    admission
        .core
        .apply_local_authority_ready(authority_ready && world_installed);
    let admitted = match *role {
        RuntimeRole::Client => admission.core.class_select_allowed(),
        RuntimeRole::Listen | RuntimeRole::Dedicated => admission.core.local_class_select_allowed(),
        RuntimeRole::Replay => presentation_ready,
    };
    if signon.admitted != admitted {
        signon.admitted = admitted;
        diag::info!(
            Sim,
            "admission admitted={admitted} role={role:?} phase={:?} presentation={presentation_ready} audio={audio_ready}",
            signon.phase
        );
    }
}

pub fn drive_class_select_screen(
    mut screen: ResMut<AppScreen>,
    mut loading: Option<ResMut<LoadingScreen>>,
    mut load: Option<ResMut<assets::MapLoadProcess>>,
    signon: Res<SignonState>,
    has_world: Option<Res<HasWorld>>,
) {
    let world_installed = has_world.is_some_and(|world| world.0);
    let admitted = signon.may_select_class();
    if signon.phase.is_failed() {
        if let Some(loading) = loading.as_deref_mut()
            && let SignonPhase::Failed(reason) = &signon.phase
        {
            if loading.failure().is_none() {
                loading.fail(reason.to_string());
            }
        }
        if let Some(load) = load.as_deref_mut() {
            load.fail();
        }
        if matches!(*screen, AppScreen::ClassSelect | AppScreen::InGame) {
            *screen = AppScreen::MainMenu;
        }
        return;
    }
    if !admitted || !world_installed {
        // Local preparation can be finished long before the session says this
        // client may continue. That wait is the load's own row, so the table
        // can say what is holding it rather than showing everything green.
        if world_installed && let Some(load) = load.as_deref_mut() {
            load.await_admission();
        }
        return;
    }
    // The load ends here, with or without a screen over it.
    if let Some(load) = load.as_deref_mut() {
        load.finish();
    }
    // The overlay comes down on its own terms, the class-select hop on the
    // screen's. A replay is already `InGame` by the time this runs — the first
    // presented snapshot arms it in `ClientSet::Present`, one set ahead of here —
    // and gating the overlay on the screen too left it up for the whole demo,
    // hiding the world behind `UiLayer::Loading` and holding back map ambience.
    if let Some(loading) = loading.as_deref_mut() {
        loading.finish();
    }
    if matches!(*screen, AppScreen::Loading | AppScreen::MainMenu) {
        *screen = AppScreen::ClassSelect;
    }
}

pub fn register_admission(app: &mut App) {
    app.add_systems(
        Update,
        update_admission
            .in_set(ClientSet::Present)
            .before(crate::local_arm::arm_local_from_presented),
    )
    .add_systems(Update, drive_class_select_screen.in_set(ClientSet::Ui));
}
