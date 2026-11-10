use assets::LoadingScreen;
use bevy::prelude::*;
use frame::ScopeApp;
use frame::{AppScreen, ClientSet, RuntimeRole};
use net::{AuthorityLoadHold, ClientAdmission, SignonPhase, SignonState};
use render_frontend::prepare::scene::world::WorldScene;

use crate::LiveWorldIdentity;

#[derive(bevy::ecs::system::SystemParam)]
pub struct PresentationReadinessSources<'w> {
    scene: Option<Res<'w, WorldScene>>,
    audio: Option<Res<'w, audio::AudioReady>>,
    headless: Option<Res<'w, frame::Headless>>,
    navigation: Option<Res<'w, frame::BotNavigationReady>>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct AdmissionPublisher<'w> {
    signon: ResMut<'w, SignonState>,
    admission: ResMut<'w, ClientAdmission>,
    hold: Option<ResMut<'w, AuthorityLoadHold>>,
    live: Option<ResMut<'w, LiveWorldIdentity>>,
    authority: Option<ResMut<'w, net::AuthorityWorld>>,
    policy: ResMut<'w, crate::SessionReadinessPolicy>,
}

impl PresentationReadinessSources<'_> {
    fn decide(&self, role: RuntimeRole, installed: bool) -> crate::readiness::ReadinessDecision {
        crate::readiness::decide_readiness(
            role,
            self.headless.is_some(),
            installed,
            self.navigation.as_ref().map(|report| report.0),
            self.scene.as_ref().map(|scene| scene.readiness),
            self.audio.as_ref().map(|report| report.0),
        )
    }
}

impl AdmissionPublisher<'_> {
    fn publish(
        mut self,
        role: Res<RuntimeRole>,
        has_world: Option<Res<State<frame::MatchScope>>>,
        sources: PresentationReadinessSources,
    ) {
        if let (Some(live), Some(installed)) = (self.live.as_mut(), self.admission.core.installed())
            && live.match_key.is_none()
            && !installed.is_none()
        {
            if let Some(authority) = self.authority.as_mut() {
                authority
                    .0
                    .world_objects_mut()
                    .set_map_round_epoch(installed.match_epoch);
            }
            live.match_key = installed;
        }
        let installed = has_world.is_some_and(|world| *world.get() == frame::MatchScope::Live)
            && self
                .live
                .as_ref()
                .is_some_and(|live| self.admission.core.installed() == Some(live.match_key));
        let decision = sources.decide(*role, installed);
        if let Some(hold) = self.hold.as_mut() {
            hold.0 = !decision.advancement;
        }
        if let Some(live) = self.live.as_ref() {
            self.admission
                .core
                .apply_presentation(live.match_key, decision.presentation);
            self.admission
                .core
                .apply_local_authority_ready(live.match_key, decision.advancement);
            if let Some(failure) = decision.failure {
                let failure = net::SessionFail {
                    stage: net::FailStage::Load,
                    source: failure.source().to_owned(),
                    match_key: live.match_key,
                };
                self.admission.core.apply_fail(failure.clone());
                self.signon.phase = SignonPhase::Failed(net::SignonFailReason::Transport {
                    source: failure.source,
                    stage: failure.stage,
                    match_key: failure.match_key,
                });
            }
        }
        let admitted = installed
            && decision.presentation
            && !self.signon.phase.is_failed()
            && match *role {
                RuntimeRole::Client => self.admission.core.class_select_allowed(),
                RuntimeRole::Listen | RuntimeRole::Dedicated => {
                    self.admission.core.local_class_select_allowed()
                }
                RuntimeRole::Replay => !self.signon.phase.is_failed(),
            };
        *self.policy = crate::SessionReadinessPolicy {
            advancement_allowed: decision.advancement,
            presentation_allowed: decision.presentation,
            admission_allowed: admitted,
            input_allowed: admitted && matches!(*role, RuntimeRole::Listen | RuntimeRole::Client),
        };
        let presentation_ready = decision.presentation;
        let audio_ready = sources
            .audio
            .as_ref()
            .is_some_and(|report| report.0.is_ready());
        if self.signon.admitted != admitted {
            self.signon.admitted = admitted;
            diag::info!(
                Sim,
                "admission admitted={admitted} role={role:?} phase={:?} presentation={presentation_ready} audio={audio_ready}",
                self.signon.phase
            );
        }
    }
}

pub fn update_admission(
    publisher: AdmissionPublisher,
    role: Res<RuntimeRole>,
    has_world: Option<Res<State<frame::MatchScope>>>,
    sources: PresentationReadinessSources,
) {
    publisher.publish(role, has_world, sources);
}

pub fn drive_class_select_screen(
    mut screen: ResMut<AppScreen>,
    mut loading: Option<ResMut<LoadingScreen>>,
    mut load: Option<ResMut<assets::MapLoadProcess>>,
    signon: Res<SignonState>,
    has_world: Option<Res<State<frame::MatchScope>>>,
) {
    let world_installed = has_world.is_some_and(|world| *world.get() == frame::MatchScope::Live);
    let admitted = signon.may_select_class();
    if signon.phase.is_failed() {
        if let Some(loading) = loading.as_deref_mut()
            && let SignonPhase::Failed(reason) = &signon.phase
            && loading.failure().is_none()
        {
            loading.fail(reason.to_string());
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
    app.scoped::<crate::SessionReadinessPolicy>(frame::MatchScope::Live)
        .add_systems(
            Update,
            update_admission
                .in_set(frame::InMatch)
                .in_set(ClientSet::Present)
                .before(crate::local_arm::arm_local_from_presented),
        )
        .add_systems(Update, drive_class_select_screen.in_set(ClientSet::Ui));
}
