use bevy::prelude::DetectChanges;
use bevy::prelude::Resource;
use frame::{AdmissionKey, InstalledMatch, MatchKey, RuntimeRole};
use sim::ClientId;

use crate::session_core::{
    ClientMatchCore, FailStage, HostWorldReady, SessionFail, confirm_keyed_world_ready,
};
use crate::transport::master::{MasterBridge, MasterBridgeState};
use crate::transport::protocol::HandshakeReject;
use crate::transport::udp_session::UdpClientLink;
use master_protocol::SessionCloseReason;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MatchEpoch(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignonFailReason {
    Handshake(HandshakeReject),
    Transport {
        source: String,
        stage: FailStage,
        match_key: MatchKey,
    },
    SessionClosed(SessionCloseReason),
}

impl core::fmt::Display for SignonFailReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Handshake(reason) => write!(f, "handshake rejected: {reason}"),
            Self::Transport {
                source,
                stage,
                match_key,
            } => {
                write!(
                    f,
                    "{source} (stage {stage:?} epoch={})",
                    match_key.match_epoch
                )
            }
            Self::SessionClosed(SessionCloseReason::HostLeft) => write!(f, "host left the session"),
            Self::SessionClosed(SessionCloseReason::LeaseExpired) => {
                write!(f, "session lease expired")
            }
            Self::SessionClosed(SessionCloseReason::Overload) => {
                write!(f, "session closed: control overload")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum SignonPhase {
    #[default]
    Local,

    CheckingMatch {
        epoch: MatchEpoch,
    },

    LoadingMap {
        epoch: MatchEpoch,
    },

    Syncing {
        epoch: MatchEpoch,
    },

    Active {
        epoch: MatchEpoch,
        client: ClientId,
    },
    Failed(SignonFailReason),
    Cancelled,
}

impl SignonPhase {
    pub const fn may_select_class(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    pub const fn may_offer_connect(&self) -> bool {
        matches!(self, Self::CheckingMatch { .. })
    }

    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    pub fn status_line(&self) -> Option<String> {
        match self {
            Self::CheckingMatch { .. } => {
                Some("waiting for host: handshake not accepted".to_owned())
            }
            Self::LoadingMap { .. } => Some("waiting for host: loading map".to_owned()),
            Self::Syncing { .. } => {
                Some("waiting for host: handshake accepted, applying bootstrap".to_owned())
            }
            Self::Failed(reason) => Some(reason.to_string()),
            Self::Cancelled => Some("signon cancelled".to_owned()),
            Self::Local | Self::Active { .. } => None,
        }
    }
}

pub fn phase_from_udp_link(link: &UdpClientLink, epoch: MatchEpoch) -> SignonPhase {
    if let Some(reason) = link.handshake_reject() {
        return SignonPhase::Failed(SignonFailReason::Handshake(reason));
    }
    match (
        link.connection,
        link.assigned_client,
        link.has_applied_snapshot(),
        link.has_entered_match(),
    ) {
        (Some(_), Some(client), true, true) => SignonPhase::Active { epoch, client },
        (Some(_), _, false, _) | (Some(_), Some(_), true, false) => SignonPhase::Syncing { epoch },
        _ => SignonPhase::CheckingMatch { epoch },
    }
}

pub fn derive_signon(
    role: RuntimeRole,
    link: Option<&UdpClientLink>,
    epoch: MatchEpoch,
) -> SignonPhase {
    if role != RuntimeRole::Client {
        return SignonPhase::Local;
    }
    match link {
        Some(link) => phase_from_udp_link(link, epoch),
        None if epoch.0 != 0 => SignonPhase::LoadingMap { epoch },
        None => SignonPhase::CheckingMatch { epoch },
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct SignonState {
    pub epoch: MatchEpoch,
    pub phase: SignonPhase,
    pub admitted: bool,
}

impl Default for SignonState {
    fn default() -> Self {
        Self {
            epoch: MatchEpoch(0),
            phase: SignonPhase::Local,
            admitted: false,
        }
    }
}

impl SignonState {
    pub fn may_select_class(&self) -> bool {
        self.admitted
    }

    pub(crate) fn set_phase(&mut self, next: SignonPhase) {
        if self.phase == next {
            return;
        }
        match &next {
            SignonPhase::Failed(reason) => {
                diag::error!(
                    Net,
                    "signon failed: {reason} (Connect will not be re-offered)"
                );
            }
            SignonPhase::Active { epoch, client } => {
                diag::info!(
                    Net,
                    "signon Active epoch={} client={} — class select is allowed",
                    epoch.0,
                    client.0
                );
            }
            other => diag::info!(Net, "signon: {other:?}"),
        }
        if let SignonPhase::Active { epoch, .. }
        | SignonPhase::Syncing { epoch }
        | SignonPhase::CheckingMatch { epoch }
        | SignonPhase::LoadingMap { epoch } = &next
        {
            self.epoch = *epoch;
        }
        self.phase = next;
    }
}

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct DeferHostWorldReady {
    until_ms: Option<u64>,
}

impl DeferHostWorldReady {
    pub fn from_env() -> Self {
        let until_ms = std::env::var("IW4L_DEFER_HOST_WORLD_READY_MS")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .map(|ms: u64| now_ms().saturating_add(ms));
        if let Some(until_ms) = until_ms {
            diag::warn!(
                Net,
                "host world ready deferred until {until_ms}ms (IW4L_DEFER_HOST_WORLD_READY_MS)"
            );
        }
        Self { until_ms }
    }

    pub fn blocking(&self, now_ms: u64) -> bool {
        host_world_ready_is_deferred(self.until_ms, now_ms)
    }
}

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct DeferBootstrapApplied(pub bool);

impl DeferBootstrapApplied {
    pub fn from_env() -> Self {
        let held = matches!(
            std::env::var("IW4L_DEFER_BOOTSTRAP_APPLIED").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE")
        );
        if held {
            diag::warn!(Net, "bootstrap Applied held (IW4L_DEFER_BOOTSTRAP_APPLIED)");
        }
        Self(held)
    }
}

pub fn host_world_ready_is_deferred(until_ms: Option<u64>, now_ms: u64) -> bool {
    until_ms.is_some_and(|until_ms| now_ms < until_ms)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ClientAdmission {
    pub core: ClientMatchCore,
}

pub fn live_match_key(bridge: Option<&MasterBridge>) -> MatchKey {
    match bridge.map(|bridge| bridge.state()) {
        Some(state) if state.in_match() => {
            let identity = state.identity();
            if identity.epoch != 0 {
                identity.match_key()
            } else {
                MatchKey::NONE
            }
        }
        _ => MatchKey::NONE,
    }
}

pub fn drive_client_admission_facts(
    mut admission: bevy::prelude::ResMut<ClientAdmission>,
    installed: Option<bevy::prelude::Res<InstalledMatch>>,
    link: Option<bevy::prelude::Res<UdpClientLink>>,
    bridge: Option<bevy::prelude::Res<MasterBridge>>,
) {
    let previous_install = admission.core.installed();
    if let Some(fact) = installed.filter(|fact| fact.is_added()) {
        admission.core.apply_start(fact.match_key);
        admission.core.apply_install(fact.match_key);
    }
    let key = live_match_key(bridge.as_deref());
    if !key.is_none() {
        admission.core.apply_start(key);
    }
    if let Some(bridge) = bridge.as_ref()
        && admission.core.installed() != previous_install
    {
        bridge.set_installed_match(admission.core.installed());
    }
    if let (Some(link), Some(bootstrap_id)) = (
        link.as_deref(),
        link.as_ref().and_then(|link| link.admission_bootstrap_id()),
    ) && link.has_entered_match()
        && link.has_applied_snapshot()
    {
        let member_id = match bridge.as_ref().map(|bridge| bridge.state()) {
            Some(MasterBridgeState::Joined { identity, .. }) => identity.member_id.0,
            _ => [0; 16],
        };
        let key = AdmissionKey {
            match_key: admission.core.match_key(),
            member_id,
            connection_id: link.connection.map(|id| id.0).unwrap_or(0),
            bootstrap_id,
        };
        if let Some(line) = admission.core.apply_adopted(key) {
            diag::info!(Net, "session-transition {line}");
        }
        if let Some(line) = admission.core.apply_enter(key) {
            diag::info!(Net, "session-transition {line}");
        }
    }
}

#[derive(Default)]
pub struct SignonBridgeObservation {
    incarnation: Option<u64>,
    terminal_reported: bool,
}

impl SignonBridgeObservation {
    fn observe(&mut self, current: Option<u64>) -> bool {
        if self.incarnation == current {
            return false;
        }
        self.incarnation = current;
        self.terminal_reported = false;
        true
    }

    fn report_terminal(&mut self) {
        self.terminal_reported = self.incarnation.is_some();
    }
}

pub fn drive_signon(
    dvars: bevy::prelude::Res<frame::UiMenuDvars>,
    mut signon: bevy::prelude::ResMut<SignonState>,
    mut admission: bevy::prelude::ResMut<ClientAdmission>,
    role: bevy::prelude::Res<RuntimeRole>,
    link: Option<bevy::prelude::Res<UdpClientLink>>,
    bridge: Option<bevy::prelude::Res<MasterBridge>>,
    mut observation: bevy::prelude::Local<SignonBridgeObservation>,
) {
    if dvars.get("ui_password_pending") == Some("1") {
        return;
    }
    let current = bridge.as_ref().map(|bridge| bridge.incarnation());
    if observation.observe(current) {
        *signon = SignonState::default();
    }
    if signon.phase.is_failed() {
        return;
    }
    // A bridge fails its own session once. `MasterBridge::fail` cancels the
    // worker but leaves the terminal state on the resource, which outlives the
    // teardown that resets `SignonState` — reading it again would fail the next
    // session's fresh signon with the previous one's message.
    let already_reported = observation.terminal_reported;
    if let Some(bridge) = bridge.as_ref() {
        match bridge.state() {
            MasterBridgeState::Failed { .. } | MasterBridgeState::Closed { .. }
                if already_reported =>
            {
                return;
            }
            MasterBridgeState::Failed { error, identity } => {
                observation.report_terminal();
                let match_key = identity.match_key();
                admission.core.apply_fail(SessionFail {
                    stage: if error.operation == "admission" {
                        FailStage::Admission
                    } else {
                        FailStage::Transport
                    },
                    source: error.to_string(),
                    match_key,
                });
                signon.set_phase(SignonPhase::Failed(SignonFailReason::Transport {
                    source: error.to_string(),
                    stage: FailStage::Transport,
                    match_key,
                }));
                signon.admitted = admission.core.class_select_allowed();
                return;
            }
            MasterBridgeState::Closed { reason, .. } => {
                observation.report_terminal();
                admission.core.apply_close();
                signon.set_phase(SignonPhase::Failed(SignonFailReason::SessionClosed(reason)));
                signon.admitted = admission.core.class_select_allowed();
                return;
            }
            _ => {}
        }
    }
    let epoch = link
        .as_ref()
        .map(|link| MatchEpoch(link.match_epoch()))
        .filter(|epoch| epoch.0 != 0)
        .or_else(|| {
            bridge.as_ref().and_then(|bridge| {
                let state = bridge.state();
                let identity = state.identity();
                (state.in_match() && identity.epoch != 0).then_some(MatchEpoch(identity.epoch))
            })
        })
        .unwrap_or(signon.epoch);
    let next = derive_signon(*role, link.as_deref(), epoch);
    signon.set_phase(next);
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct MapLoadReporter<'w, 's> {
    hold: Option<bevy::prelude::Res<'w, crate::AuthorityLoadHold>>,
    defer: Option<bevy::prelude::Res<'w, DeferHostWorldReady>>,
    sent: bevy::prelude::Local<'s, Option<(u64, u32)>>,
    deferred: bevy::prelude::Local<'s, bool>,
    withheld: bevy::prelude::Local<'s, bool>,
}

impl MapLoadReporter<'_, '_> {
    fn drive(
        mut self,
        bridge: Option<bevy::prelude::Res<MasterBridge>>,
        descriptor: Option<bevy::prelude::Res<crate::MatchDescriptor>>,
        has_world: Option<bevy::prelude::Res<bevy::prelude::State<frame::MatchScope>>>,
        admission: Option<bevy::prelude::Res<ClientAdmission>>,
        signon: bevy::prelude::Res<SignonState>,
    ) {
        let Some(bridge) = bridge else {
            return;
        };
        let installed = has_world.is_some_and(|world| *world.get() == frame::MatchScope::Live);
        let hold = self.hold.is_some_and(|hold| hold.0);
        let live = live_match_key(Some(&bridge));
        let belongs = admission.as_ref().is_some_and(|admission| {
            admission
                .core
                .installed()
                .is_some_and(|key| key == live || (live.is_none() && key.is_none()))
        });
        let Some(descriptor) = descriptor.as_deref() else {
            return;
        };
        match bridge.state() {
            MasterBridgeState::Hosting {
                identity,
                in_match: true,
                ..
            } if identity.epoch != 0 => {
                let Some(ready) = confirm_keyed_world_ready(
                    installed,
                    hold,
                    belongs,
                    HostWorldReady {
                        epoch: identity.epoch,
                        map: descriptor.map,
                        weapons: descriptor.weapons,
                        classes: descriptor.classes,
                    },
                ) else {
                    return;
                };
                if self.defer.is_some_and(|defer| defer.blocking(now_ms())) {
                    if !*self.deferred {
                        diag::info!(
                            Net,
                            "host world ready held; network and control loop still running"
                        );
                        *self.deferred = true;
                    }
                    return;
                }
                let key = (bridge.incarnation(), identity.epoch);
                if *self.sent == Some(key) {
                    return;
                }
                bridge.report_host_world_ready(
                    ready.epoch,
                    crate::MatchDescriptor {
                        map: ready.map,
                        weapons: ready.weapons,
                        classes: ready.classes,
                    },
                    admission
                        .as_ref()
                        .and_then(|admission| admission.core.installed())
                        .expect("confirmed installed host world"),
                );
                *self.sent = Some(key);
            }
            _ => {
                let epoch = signon.epoch.0;
                let Some(ready) = confirm_keyed_world_ready(
                    installed,
                    false,
                    belongs,
                    HostWorldReady {
                        epoch,
                        map: descriptor.map,
                        weapons: descriptor.weapons,
                        classes: descriptor.classes,
                    },
                ) else {
                    // The world is on screen but the report is still held back:
                    // say which gate holds it, or the client sits in LoadingMap
                    // with nothing in the log to read.
                    if installed && !*self.withheld {
                        *self.withheld = true;
                        diag::info!(
                            Net,
                            "map loaded withheld: epoch={epoch} belongs={belongs} installed_key={:?} live={:?}",
                            admission
                                .as_ref()
                                .and_then(|admission| admission.core.installed()),
                            live,
                        );
                    }
                    return;
                };
                *self.withheld = false;
                let key = (bridge.incarnation(), epoch);
                if *self.sent == Some(key) {
                    return;
                }
                bridge.report_map_loaded(
                    ready.epoch,
                    crate::MatchDescriptor {
                        map: ready.map,
                        weapons: ready.weapons,
                        classes: ready.classes,
                    },
                );
                diag::info!(
                    Net,
                    "map loaded reported epoch={} map={:016x} weapons={:016x} classes={:016x} — the host owes a bootstrap offer",
                    ready.epoch,
                    ready.map,
                    ready.weapons,
                    ready.classes
                );
                *self.sent = Some(key);
            }
        }
    }
}

pub fn drive_map_loaded(
    reporter: MapLoadReporter,
    bridge: Option<bevy::prelude::Res<MasterBridge>>,
    descriptor: Option<bevy::prelude::Res<crate::MatchDescriptor>>,
    has_world: Option<bevy::prelude::Res<bevy::prelude::State<frame::MatchScope>>>,
    admission: Option<bevy::prelude::Res<ClientAdmission>>,
    signon: bevy::prelude::Res<SignonState>,
) {
    reporter.drive(bridge, descriptor, has_world, admission, signon);
}

pub fn drive_match_boundary(
    mut hub: Option<bevy::prelude::ResMut<crate::transport::udp_session::UdpAuthorityHub>>,
    mut link: Option<bevy::prelude::ResMut<UdpClientLink>>,
    bridge: Option<bevy::prelude::Res<MasterBridge>>,
    mut live: bevy::prelude::Local<bool>,
) {
    let in_match = bridge
        .as_ref()
        .map(|bridge| bridge.state().in_match())
        .unwrap_or(*live);
    if *live && !in_match {
        if let Some(hub) = hub.as_mut() {
            hub.reset_match();
        }
        if let Some(link) = link.as_mut() {
            link.reset_match();
        }
    }
    *live = in_match;
}

pub(crate) fn release_match_protocol(
    mut admission: bevy::prelude::ResMut<ClientAdmission>,
    mut hub: Option<bevy::prelude::ResMut<crate::transport::udp_session::UdpAuthorityHub>>,
    mut link: Option<bevy::prelude::ResMut<UdpClientLink>>,
) {
    admission.core.retire_match();
    if let Some(hub) = hub.as_mut() {
        hub.reset_match();
    }
    if let Some(link) = link.as_mut() {
        link.reset_match();
    }
}
