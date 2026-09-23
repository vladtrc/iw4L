use bevy::prelude::Resource;
use frame::{AdmissionKey, MatchInstalled, MatchKey, MatchTornDown};
use sim::ClientId;

use crate::role::RuntimeRole;
use crate::session_core::{
    ClientMatchCore, FailStage, HostWorldReady, SessionFail, confirm_keyed_world_ready,
    format_session_transition,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignonPhase {
    Local,

    CheckingMatch { epoch: MatchEpoch },

    LoadingMap { epoch: MatchEpoch },

    Syncing { epoch: MatchEpoch },

    Active { epoch: MatchEpoch, client: ClientId },
    Failed(SignonFailReason),
    Cancelled,
}

impl Default for SignonPhase {
    fn default() -> Self {
        Self::Local
    }
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

fn owner_match_key(bridge: Option<&MasterBridge>) -> MatchKey {
    bridge
        .map(|bridge| bridge.state().identity().match_key())
        .unwrap_or(MatchKey::NONE)
}

pub fn drive_client_admission_facts(
    mut admission: bevy::prelude::ResMut<ClientAdmission>,
    mut installed: bevy::prelude::MessageReader<MatchInstalled>,
    mut torn: bevy::prelude::MessageReader<MatchTornDown>,
    link: Option<bevy::prelude::Res<UdpClientLink>>,
    bridge: Option<bevy::prelude::Res<MasterBridge>>,
    adopted: bevy::prelude::Res<crate::LastAdoptedSnapshot>,
) {
    for fact in torn.read() {
        admission.core.apply_teardown(*fact);
    }
    for fact in installed.read() {
        admission.core.apply_start(fact.load_key.match_key);
        admission.core.apply_install(fact.load_key);
    }
    if link
        .as_ref()
        .is_some_and(|link| link.has_applied_direct_snapshot())
        && adopted.next().is_some()
        && let Some(load) = admission.core.installed()
    {
        admission.core.apply_direct_adopted(load);
    }
    let key = live_match_key(bridge.as_deref());
    if !key.is_none() {
        admission.core.apply_start(key);
    }
    if let Some(bridge) = bridge.as_ref() {
        bridge.set_installed_load(admission.core.installed());
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

pub fn drive_signon(
    mut signon: bevy::prelude::ResMut<SignonState>,
    mut admission: bevy::prelude::ResMut<ClientAdmission>,
    role: bevy::prelude::Res<RuntimeRole>,
    link: Option<bevy::prelude::Res<UdpClientLink>>,
    bridge: Option<bevy::prelude::Res<MasterBridge>>,
    mut incarnation: bevy::prelude::Local<Option<u64>>,
    mut reported_terminal: bevy::prelude::Local<Option<u64>>,
) {
    let current = bridge.as_ref().map(|bridge| bridge.incarnation());
    if *incarnation != current {
        *incarnation = current;
        *reported_terminal = None;

        *signon = SignonState::default();
    }
    if signon.phase.is_failed() {
        return;
    }
    // A bridge fails its own session once. `MasterBridge::fail` cancels the
    // worker but leaves the terminal state on the resource, which outlives the
    // teardown that resets `SignonState` — reading it again would fail the next
    // session's fresh signon with the previous one's message.
    let already_reported = *reported_terminal == current && current.is_some();
    if let Some(bridge) = bridge.as_ref() {
        match bridge.state() {
            MasterBridgeState::Failed { .. } | MasterBridgeState::Closed { .. }
                if already_reported =>
            {
                return;
            }
            MasterBridgeState::Failed { error, identity } => {
                *reported_terminal = current;
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
                *reported_terminal = current;
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

pub fn drive_map_loaded(
    bridge: Option<bevy::prelude::Res<MasterBridge>>,
    descriptor: Option<bevy::prelude::Res<crate::MatchDescriptor>>,
    has_world: Option<bevy::prelude::Res<frame::HasWorld>>,
    admission: Option<bevy::prelude::Res<ClientAdmission>>,
    signon: bevy::prelude::Res<SignonState>,
    hold: Option<bevy::prelude::Res<crate::AuthorityLoadHold>>,
    defer: Option<bevy::prelude::Res<DeferHostWorldReady>>,
    mut sent: bevy::prelude::Local<Option<(u64, u32)>>,
    mut deferred: bevy::prelude::Local<bool>,
    mut withheld: bevy::prelude::Local<bool>,
) {
    let Some(bridge) = bridge else {
        return;
    };
    let installed = has_world.is_some_and(|world| world.0);
    let hold = hold.is_some_and(|hold| hold.0);
    let live = live_match_key(Some(&bridge));
    let belongs = admission.as_ref().is_some_and(|admission| {
        admission
            .core
            .installed()
            .is_some_and(|key| key.belongs_to(live) || (live.is_none() && key.match_key.is_none()))
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
            if defer.is_some_and(|defer| defer.blocking(now_ms())) {
                if !*deferred {
                    diag::info!(
                        Net,
                        "host world ready held; network and control loop still running"
                    );
                    *deferred = true;
                }
                return;
            }
            let key = (bridge.incarnation(), identity.epoch);
            if *sent == Some(key) {
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
            *sent = Some(key);
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
                if installed && !*withheld {
                    *withheld = true;
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
            *withheld = false;
            let key = (bridge.incarnation(), epoch);
            if *sent == Some(key) {
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
            *sent = Some(key);
        }
    }
}

pub fn drive_match_boundary(
    mut torn: bevy::prelude::MessageReader<MatchTornDown>,
    mut hub: Option<bevy::prelude::ResMut<crate::transport::udp_session::UdpAuthorityHub>>,
    mut link: Option<bevy::prelude::ResMut<UdpClientLink>>,
    bridge: Option<bevy::prelude::Res<MasterBridge>>,
    mut live: bevy::prelude::Local<bool>,
) {
    let live_key = owner_match_key(bridge.as_deref());
    let live_epoch = live_key.match_epoch;
    let mut reset = false;
    let mut end_match = false;
    let mut torn_key = frame::MatchKey::NONE;
    for fact in torn.read() {
        if !crate::session_core::match_key_boundary_applies(fact.match_key, live_key) {
            diag::info!(
                Net,
                "session-transition {}",
                format_session_transition(
                    fact.match_key.session_id,
                    None,
                    fact.match_epoch,
                    None,
                    None,
                    None,
                    None,
                    &format!("live={live_epoch}"),
                    "torn",
                    "ignored",
                    None,
                    "stale world",
                )
            );
            continue;
        }
        reset = true;
        torn_key = fact.match_key;
        end_match = fact.reason.keeps_session();
    }
    let in_match = bridge
        .as_ref()
        .map(|bridge| bridge.state().in_match())
        .unwrap_or(*live);
    if reset || (*live && !in_match) {
        if let Some(hub) = hub.as_mut() {
            hub.reset_match();
        }
        if let Some(link) = link.as_mut() {
            link.reset_match();
        }
        if end_match && let Some(bridge) = bridge.as_ref() {
            bridge.report_match_ended(torn_key);
        }
    }
    *live = in_match;
}
