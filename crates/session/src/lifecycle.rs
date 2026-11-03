use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, TaskPool};
use frame::ScopeApp;
use frame::{
    AppScreen, ClassSelectHandoff, InstalledMatch, LaunchIdentity, MapLoadApproved, MapLoadFailed,
    MatchKey, Retiring, RuntimeRole, TeardownReason,
};
use net::{AuthorityLoadHold, ClientSet, PresentedSnapshot};

#[derive(Resource, Default, Debug)]
pub struct TeardownRequest(pub Option<TeardownReason>);

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct LiveWorldIdentity {
    pub match_key: MatchKey,
}

#[derive(Resource, Debug, Default)]
pub struct SessionSwapRequest {
    pending: Option<PendingSessionSwap>,
    completed: Option<SessionSwapResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSwapTarget {
    Zone(String),

    Demo {
        name: String,
        zone: String,
        quit_on_end: bool,
    },

    Menu {
        leave_session: bool,
    },
}

#[derive(Debug)]
struct PendingSessionSwap {
    target: SessionSwapTarget,
    phase: SessionSwapPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionSwapPhase {
    Requested,
    WaitingForTeardown,
    WaitingForInstall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSwapResult {
    Installed { zone: String },
    Menu,
    Failed { zone: String, error: String },
}

impl SessionSwapRequest {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn load_pending(&self) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            pending.phase == SessionSwapPhase::WaitingForInstall
                && !matches!(pending.target, SessionSwapTarget::Menu { .. })
        })
    }

    pub(crate) fn can_install(&self) -> bool {
        self.pending.is_none() || self.load_pending()
    }

    pub fn request_zone(&mut self, zone: String) -> Result<(), String> {
        self.begin(SessionSwapTarget::Zone(zone))
    }

    pub fn request_demo(
        &mut self,
        name: String,
        zone: String,
        quit_on_end: bool,
    ) -> Result<(), String> {
        self.begin(SessionSwapTarget::Demo {
            name,
            zone,
            quit_on_end,
        })
    }

    pub fn request_menu(&mut self) -> Result<(), String> {
        self.request_return(false)
    }

    pub fn request_leave(&mut self) -> Result<(), String> {
        self.request_return(true)
    }

    fn request_return(&mut self, leave_session: bool) -> Result<(), String> {
        let Some(pending) = self.pending.as_mut() else {
            return self.begin(SessionSwapTarget::Menu { leave_session });
        };
        let verb = if leave_session {
            "disconnect"
        } else {
            "match end"
        };
        match &mut pending.target {
            SessionSwapTarget::Menu {
                leave_session: already,
            } => {
                *already = *already || leave_session;
                Ok(())
            }
            SessionSwapTarget::Zone(zone) => {
                diag::info!(
                    Sim,
                    "session: {verb} superseded in-flight map `{zone}` → menu"
                );
                pending.target = SessionSwapTarget::Menu { leave_session };
                Ok(())
            }
            SessionSwapTarget::Demo { name, .. } => {
                diag::info!(
                    Sim,
                    "session: {verb} superseded in-flight demo `{name}` → menu"
                );
                pending.target = SessionSwapTarget::Menu { leave_session };
                Ok(())
            }
        }
    }

    fn begin(&mut self, target: SessionSwapTarget) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("session swap already in progress".into());
        }
        let target_s = match &target {
            SessionSwapTarget::Zone(zone) => format!("zone:{zone}"),
            SessionSwapTarget::Demo { name, .. } => format!("demo:{name}"),
            SessionSwapTarget::Menu {
                leave_session: true,
            } => "leave".into(),
            SessionSwapTarget::Menu { .. } => "menu".into(),
        };
        self.pending = Some(PendingSessionSwap {
            target,
            phase: SessionSwapPhase::Requested,
        });
        perf::instant!("swap", "iw4l.sim", |ctx| {
            let (phase, target): (&str, &str) = ("requested", &target_s);
            ctx.arg("phase", phase);
            ctx.arg("target", target);
        });
        Ok(())
    }

    pub fn take_completed(&mut self) -> Option<SessionSwapResult> {
        self.completed.take()
    }
}

fn release_match(
    mut control: ResMut<crate::ScopeControl>,
    mut screen: ResMut<AppScreen>,
    mut handoff: ResMut<ClassSelectHandoff>,
    mut identity: Option<ResMut<LaunchIdentity>>,
    mut load_hold: Option<ResMut<AuthorityLoadHold>>,
    bridge: Option<Res<net::MasterBridge>>,
    mut retiring: ResMut<Retiring>,
) {
    let Some(reason) = control.teardown.take() else {
        return;
    };

    if let Some(bridge) = bridge.as_ref() {
        bridge.set_installed_match(None);
    }

    *screen = AppScreen::MainMenu;
    *handoff = ClassSelectHandoff::default();

    if let Some(hold) = load_hold.as_mut() {
        hold.0 = true;
    }

    if let Some(identity) = identity.as_mut() {
        identity.zone.clear();
    }

    perf::instant!("match_torn", "iw4l.sim", |ctx| {
        let reason: &str = reason.label();
        ctx.arg("reason", reason);
    });
    diag::lifecycle_boundary(
        "local_session_revoked",
        &format!(" reason={}", reason.label()),
    );
    retiring.mark_teardown();
}

fn stamp_loaded_zone(
    installed: Option<Res<InstalledMatch>>,
    mut identity: Option<ResMut<LaunchIdentity>>,
) {
    let Some(fact) = installed.filter(|fact| fact.is_added()) else {
        return;
    };
    if let Some(identity) = identity.as_mut() {
        identity.zone = fact.zone.clone();
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct SessionSwapDriver<'w> {
    transition: ResMut<'w, SessionSwapRequest>,
    scope: ResMut<'w, crate::ScopeControl>,
    teardown: ResMut<'w, TeardownRequest>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct SessionSwapEffects<'w> {
    role: Option<ResMut<'w, RuntimeRole>>,
    identity: Option<ResMut<'w, LaunchIdentity>>,
    approved: MessageWriter<'w, MapLoadApproved>,
    leave: Option<ResMut<'w, net::PendingMasterMenuAction>>,
}

impl SessionSwapEffects<'_> {
    fn stamp_role(&mut self, next: RuntimeRole) {
        if let Some(role) = self.role.as_mut() {
            **role = next;
        }
        if let Some(identity) = self.identity.as_mut() {
            identity.role_label = format!("{next:?}");
        }
    }
}

fn teardown_reason_for(target: &SessionSwapTarget) -> TeardownReason {
    match target {
        SessionSwapTarget::Menu {
            leave_session: true,
        } => TeardownReason::Disconnect,
        SessionSwapTarget::Menu { .. } => TeardownReason::MatchEnded,
        SessionSwapTarget::Zone(_) | SessionSwapTarget::Demo { .. } => TeardownReason::Replaced,
    }
}

fn match_key_for_swap(bridge: Option<&net::MasterBridge>) -> MatchKey {
    bridge
        .filter(|bridge| bridge.state().in_match())
        .map_or(MatchKey::NONE, |bridge| {
            bridge.state().identity().match_key()
        })
}

impl SessionSwapEffects<'_> {
    fn occupy(
        &mut self,
        pending: &mut PendingSessionSwap,
        bridge: Option<&net::MasterBridge>,
    ) -> Option<SessionSwapResult> {
        match pending.target.clone() {
            SessionSwapTarget::Menu { leave_session } => {
                self.stamp_role(RuntimeRole::Listen);

                if leave_session
                    && bridge.is_some()
                    && let Some(action) = self.leave.as_mut()
                    && action.0.is_none()
                {
                    action.0 = Some(net::MasterMenuAction::LeaveLobby);
                }
                Some(SessionSwapResult::Menu)
            }
            SessionSwapTarget::Zone(zone) => {
                let keep_client = self
                    .role
                    .as_ref()
                    .is_some_and(|role| **role == RuntimeRole::Client);
                self.stamp_role(if keep_client {
                    RuntimeRole::Client
                } else {
                    RuntimeRole::Listen
                });
                self.approved.write(MapLoadApproved {
                    match_key: match_key_for_swap(bridge),
                    zone,
                });
                pending.phase = SessionSwapPhase::WaitingForInstall;
                None
            }
            SessionSwapTarget::Demo { zone, .. } => {
                self.stamp_role(RuntimeRole::Replay);
                self.approved.write(MapLoadApproved {
                    match_key: match_key_for_swap(bridge),
                    zone,
                });
                pending.phase = SessionSwapPhase::WaitingForInstall;
                None
            }
        }
    }
}

impl SessionSwapDriver<'_> {
    fn advance(
        mut self,
        has_world: Option<Res<State<frame::MatchScope>>>,
        mut effects: SessionSwapEffects,
        mut failed: MessageReader<MapLoadFailed>,
        mut dvars: ResMut<frame::UiMenuDvars>,
        installed: Option<Res<InstalledMatch>>,
        bridge: Option<Res<net::MasterBridge>>,
    ) {
        let failure = failed
            .read()
            .last()
            .filter(|_| self.transition.can_install());
        let mut finish: Option<SessionSwapResult> = None;
        if let Some(fact) = failure {
            self.scope.return_to_menu(None);
            effects.stamp_role(RuntimeRole::Listen);
            if let Some(identity) = effects.identity.as_mut() {
                identity.zone.clear();
            }
            let error = fact.error.chars().take(420).collect::<String>();
            let details = if fact.error.chars().count() > 420 {
                format!("{error}…\nSee the log for details.")
            } else {
                error
            };
            dvars.set(
                "ui_map_error_reason",
                format!("Could not load map '{}'.\n\n{details}", fact.zone),
            );
            dvars.set("ui_map_error", "1");
            diag::warn!(
                Sim,
                "session: map `{}` failed: {} — returned to menu",
                fact.zone,
                fact.error
            );
            finish = Some(SessionSwapResult::Failed {
                zone: fact.zone.clone(),
                error: fact.error.clone(),
            });
        } else if let Some(pending) = self.transition.pending.as_mut() {
            let expected_teardown = teardown_reason_for(&pending.target);
            // A leave can arrive after a match-end teardown was requested: a return to the menu
            // finishes on whichever teardown comes, but it must still wait for one.
            match pending.phase {
                SessionSwapPhase::Requested
                    if has_world
                        .as_ref()
                        .is_some_and(|world| *world.get() == frame::MatchScope::Live) =>
                {
                    self.teardown.0 = Some(expected_teardown);
                    pending.phase = SessionSwapPhase::WaitingForTeardown;
                }
                SessionSwapPhase::Requested | SessionSwapPhase::WaitingForTeardown
                    if matches!(pending.phase, SessionSwapPhase::Requested)
                        || !has_world
                            .as_ref()
                            .is_some_and(|world| *world.get() == frame::MatchScope::Live) =>
                {
                    if matches!(pending.target, SessionSwapTarget::Menu { .. }) {
                        self.scope.return_to_menu(Some(expected_teardown));
                    } else if !has_world
                        .as_ref()
                        .is_some_and(|world| *world.get() == frame::MatchScope::Loading)
                    {
                        self.scope.begin_load();
                    }
                    finish = effects.occupy(pending, bridge.as_deref());
                }
                SessionSwapPhase::WaitingForInstall
                    if matches!(pending.target, SessionSwapTarget::Menu { .. }) =>
                {
                    self.scope.return_to_menu(Some(expected_teardown));
                    finish = effects.occupy(pending, bridge.as_deref());
                }
                SessionSwapPhase::WaitingForInstall
                    if let Some(fact) = installed.as_ref().filter(|_| {
                        has_world
                            .as_ref()
                            .is_some_and(|scope| *scope.get() == frame::MatchScope::Live)
                    }) =>
                {
                    finish = Some(SessionSwapResult::Installed {
                        zone: fact.zone.clone(),
                    });
                }
                _ => {}
            }
        }
        if let Some(completed) = finish {
            let done_target = match &completed {
                SessionSwapResult::Installed { zone } => format!("installed:{zone}"),
                SessionSwapResult::Menu => "menu".into(),
                SessionSwapResult::Failed { zone, .. } => format!("failed:{zone}"),
            };
            perf::instant!("swap", "iw4l.sim", |ctx| {
                let (phase, target): (&str, &str) = ("done", &done_target);
                ctx.arg("phase", phase);
                ctx.arg("target", target);
            });
            self.transition.pending = None;
            self.transition.completed = Some(completed);
        }
    }
}

fn run_session_swap(
    driver: SessionSwapDriver,
    has_world: Option<Res<State<frame::MatchScope>>>,
    effects: SessionSwapEffects,
    failed: MessageReader<MapLoadFailed>,
    dvars: ResMut<frame::UiMenuDvars>,
    installed: Option<Res<InstalledMatch>>,
    bridge: Option<Res<net::MasterBridge>>,
) {
    driver.advance(has_world, effects, failed, dvars, installed, bridge);
}

fn run_exit_level(
    mut called: MessageReader<frame::ExitLevelCalled>,
    mut transition: ResMut<SessionSwapRequest>,
) {
    if called.read().next().is_none() {
        return;
    }
    match transition.request_menu() {
        Ok(()) => diag::info!(Sim, "session: exitLevel( false ) → lobby"),
        Err(err) => diag::info!(Sim, "session: exitLevel refused — {err}"),
    }
}

fn retire_handed_over(mut retiring: ResMut<Retiring>, scope: Res<State<frame::MatchScope>>) {
    let (batch, teardown) = retiring.take_batch();
    if batch.is_empty() {
        return;
    }
    let values = batch.len();
    let guard = retiring.ship(teardown);
    let reclaim_heap = *scope.get() == frame::MatchScope::Absent;
    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            drop(batch);
            if guard.settle() == Some(0) {
                if reclaim_heap {
                    let trim_ms = diag::release_freed_heap().as_secs_f32() * 1000.0;
                    diag::info!(
                        Sim,
                        "session: released freed teardown heap in {trim_ms:.1} ms"
                    );
                }
                diag::lifecycle_boundary("retirement_settled", &format!(" values={values}"));
            }
        })
        .detach();
    let (jobs, total, waiting) = retiring.counts();
    diag::info!(
        Sim,
        "session: retired {values} value(s) off the frame (batches={jobs} values={total} waiting={waiting})"
    );
    if teardown {
        diag::lifecycle_boundary(
            "old_runtime_teardown_complete",
            &format!(" values={values} in_flight={}", retiring.in_flight()),
        );
    }
}

fn joined_in_match(state: Option<&net::MasterBridgeState>) -> Option<bool> {
    match state {
        Some(net::MasterBridgeState::Joined { in_match, .. }) => Some(*in_match),
        _ => None,
    }
}

fn peer_should_leave_match(was_in_match: bool, in_match: bool) -> bool {
    was_in_match && !in_match
}

fn journal_has_match_ended(presented: Option<&PresentedSnapshot>) -> bool {
    presented.and_then(|p| p.snapshot()).is_some_and(|snap| {
        snap.meta
            .journal
            .iter()
            .any(|r| matches!(r.event, sim::SimEvent::MatchEnded { .. }))
    })
}

fn udp_peer_exit_applies(role: RuntimeRole, master_joined: bool, has_world: bool) -> bool {
    role == RuntimeRole::Client && !master_joined && has_world
}

fn udp_match_ended_rising(latched: bool, journal_has: bool) -> bool {
    journal_has && !latched
}

fn connection_notice(reason: &str) -> String {
    let reason = reason
        .strip_prefix("handshake rejected: ")
        .or_else(|| reason.strip_prefix("join admission: "))
        .unwrap_or(reason);
    let message = if reason.starts_with("class catalog mismatch") {
        "Your class rules differ from the host's. Both players need the same game build."
    } else if reason.starts_with("map content mismatch") {
        "Your map data differs from the host's. Both players need the same map data to join."
    } else if reason.starts_with("weapon table mismatch") {
        "Your weapon data differs from the host's. Check that both players use the same build and game content."
    } else {
        reason
    };
    if message.chars().count() > 420 {
        format!(
            "{}…\nSee the log for details.",
            message.chars().take(420).collect::<String>()
        )
    } else {
        message.to_owned()
    }
}

fn run_peer_lobby_return(
    signon: Res<net::SignonState>,
    bridge: Option<Res<net::MasterBridge>>,
    mut transition: ResMut<SessionSwapRequest>,
    mut dvars: ResMut<frame::UiMenuDvars>,
    mut live: Local<bool>,
    mut terminal_cleanup: Local<Option<u64>>,
) {
    if dvars.get("ui_password_pending") == Some("1") {
        return;
    }
    let held = bridge.as_ref().map(|b| b.state());
    if signon.phase.is_failed()
        || matches!(
            held.as_ref(),
            Some(
                net::MasterBridgeState::Failed { .. }
                    | net::MasterBridgeState::Closed { .. }
                    | net::MasterBridgeState::Left { .. }
            )
        )
    {
        let incarnation = bridge
            .as_ref()
            .map(|bridge| bridge.incarnation())
            .unwrap_or(0);
        if *terminal_cleanup != Some(incarnation) {
            match transition.request_menu() {
                Ok(()) => {
                    *terminal_cleanup = Some(incarnation);
                    // Keep the notice outside cgame: teardown clears SignonState
                    // and the menu stack before the frontend becomes interactive.
                    let reason = match held.as_ref() {
                        Some(net::MasterBridgeState::Failed { error, .. }) => {
                            Some(error.source.clone())
                        }
                        Some(net::MasterBridgeState::Closed { reason, .. }) => {
                            Some(net::SignonFailReason::SessionClosed(*reason).to_string())
                        }
                        Some(net::MasterBridgeState::Left { .. }) => None,
                        _ => match &signon.phase {
                            net::SignonPhase::Failed(net::SignonFailReason::Transport {
                                source,
                                ..
                            }) => Some(source.clone()),
                            net::SignonPhase::Failed(reason) => Some(reason.to_string()),
                            _ => None,
                        },
                    };
                    if let Some(reason) = reason {
                        dvars.set("partyend_reason", connection_notice(&reason));
                        dvars.set("ui_connection_error", "1");
                    }
                    diag::info!(Sim, "session: peer terminal → lobby");
                }
                Err(err) => diag::info!(Sim, "session: peer terminal lobby refused — {err}"),
            }
        }
        *live = false;
        return;
    }
    *terminal_cleanup = None;
    let Some(in_match) = joined_in_match(held.as_ref()) else {
        *live = false;
        return;
    };
    if peer_should_leave_match(*live, in_match) {
        match transition.request_menu() {
            Ok(()) => diag::info!(Sim, "session: peer match ended → lobby"),
            Err(err) => diag::info!(Sim, "session: peer lobby return refused — {err}"),
        }
    }
    *live = in_match;
}

fn run_udp_peer_lobby_return(
    role: Res<RuntimeRole>,
    presented: Option<Res<PresentedSnapshot>>,
    bridge: Option<Res<net::MasterBridge>>,
    has_world: Option<Res<State<frame::MatchScope>>>,
    mut transition: ResMut<SessionSwapRequest>,
    mut latched: Local<bool>,
) {
    let master_joined = joined_in_match(bridge.as_ref().map(|b| b.state()).as_ref()).is_some();
    let has_world = has_world.is_some_and(|h| *h.get() == frame::MatchScope::Live);
    if !udp_peer_exit_applies(*role, master_joined, has_world) {
        *latched = false;
        return;
    }
    let journal_has = journal_has_match_ended(presented.as_deref());
    if udp_match_ended_rising(*latched, journal_has) {
        match transition.request_menu() {
            Ok(()) => diag::info!(Sim, "session: udp MatchEnded → lobby"),
            Err(err) => diag::info!(Sim, "session: udp lobby return refused — {err}"),
        }
    }
    if journal_has {
        *latched = true;
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct MasterMatchFollower<'w, 's> {
    commands: Commands<'w, 's>,
    offers: ResMut<'w, net::MasterMatchStart>,
    merged: Option<Res<'w, assets::MergedMap>>,
    bridge: Option<Res<'w, net::MasterBridge>>,
    swap: ResMut<'w, SessionSwapRequest>,
    role: ResMut<'w, RuntimeRole>,
    pending: Local<'s, Option<(net::MasterMatchOffer, bool)>>,
}

impl MasterMatchFollower<'_, '_> {
    fn follow(
        mut self,
        identity: Res<LaunchIdentity>,
        live: Option<Res<LiveWorldIdentity>>,
        has_world: Res<State<frame::MatchScope>>,
        busy: Option<Res<assets::MatchLoadBusy>>,
        request: Option<Res<assets::MatchLoadRequest>>,
        ready: Option<Res<crate::match_load::PreparedMatchReady>>,
    ) {
        if let Some(offer) = self.offers.take() {
            *self.pending = Some((offer, false));
        }
        let Some((offer, retiring)) = self.pending.as_mut() else {
            return;
        };
        let Some(bridge) = self.bridge else {
            *self.pending = None;
            return;
        };
        let state = bridge.state();
        if !state.in_match() || state.identity().match_key() != offer.match_key {
            *self.pending = None;
            return;
        }
        let host = matches!(state, net::MasterBridgeState::Hosting { .. });
        if (*has_world.get() == frame::MatchScope::Live)
            && live.as_ref().is_some_and(|live| {
                live.match_key == offer.match_key
                    || (host
                        && live.match_key.is_none()
                        && self
                            .merged
                            .as_ref()
                            .map_or(identity.zone.as_str(), |merged| merged.recording_name())
                            == offer.map)
            })
        {
            *self.pending = None;
            return;
        }
        let loading = busy.is_some_and(|busy| busy.0) || request.is_some() || ready.is_some();
        if !*retiring
            && ((*has_world.get() == frame::MatchScope::Live)
                || loading
                || self.swap.pending.is_some())
        {
            if let Err(error) = self.swap.request_menu() {
                diag::warn!(Net, "master match teardown refused: {error}");
                return;
            }
            *retiring = true;
            return;
        }
        if (*has_world.get() == frame::MatchScope::Live) || loading || self.swap.pending.is_some() {
            return;
        }
        let Some(mode) = sim::HostGameModeSelection::from_token(&offer.mode) else {
            diag::warn!(Net, "master match mode refused: {}", offer.mode);
            *self.pending = None;
            return;
        };
        *self.role = if host {
            RuntimeRole::Listen
        } else {
            RuntimeRole::Client
        };
        self.commands.insert_resource(mode);
        self.commands.insert_resource(offer.families.clone());
        match self.swap.request_zone(offer.map.clone()) {
            Ok(()) => {
                diag::info!(
                    Net,
                    "master match {}:{} -> session load {}",
                    bridge.incarnation(),
                    offer.match_key.match_epoch,
                    offer.map
                );
                *self.pending = None;
            }
            Err(error) => diag::warn!(Net, "master match load refused: {error}"),
        }
    }
}

fn follow_master_match(
    follower: MasterMatchFollower,
    identity: Res<LaunchIdentity>,
    live: Option<Res<LiveWorldIdentity>>,
    has_world: Res<State<frame::MatchScope>>,
    busy: Option<Res<assets::MatchLoadBusy>>,
    request: Option<Res<assets::MatchLoadRequest>>,
    ready: Option<Res<crate::match_load::PreparedMatchReady>>,
) {
    follower.follow(identity, live, has_world, busy, request, ready);
}

pub fn register_lifecycle(app: &mut App) {
    app.init_resource::<TeardownRequest>()
        .init_resource::<frame::UiMenuDvars>()
        .init_resource::<SessionSwapRequest>()
        .init_resource::<Retiring>()
        .add_message::<MapLoadApproved>()
        .scoped_message::<MapLoadFailed>(frame::MatchScope::Loading)
        .configure_sets(PreUpdate, frame::SessionSwapApplied.in_set(ClientSet::Load))
        .add_systems(
            PreUpdate,
            (
                follow_master_match,
                run_exit_level,
                run_peer_lobby_return,
                run_udp_peer_lobby_return,
                request_teardown,
                run_session_swap,
            )
                .chain()
                .in_set(frame::SessionSwapApplied),
        )
        .add_systems(
            PreUpdate,
            stamp_loaded_zone
                .after(crate::apply_prepared_match)
                .in_set(ClientSet::Load),
        )
        .add_systems(
            OnExit(frame::MatchScope::Live),
            release_match.in_set(frame::ScopeSet::Release),
        )
        .add_systems(Update, retire_handed_over.in_set(ClientSet::Diag));
}

fn request_teardown(mut request: ResMut<TeardownRequest>, mut scope: ResMut<crate::ScopeControl>) {
    if let Some(reason) = request.0.take() {
        scope.tear_down(reason);
    }
}
