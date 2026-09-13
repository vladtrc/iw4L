use std::collections::HashMap;
use std::fmt::Write;

use frame::{AdmissionKey, LocalLoadKey, MatchKey};
use master_protocol::{AdmissionFailure, MemberId};

use crate::transport::bootstrap::epoch_applies;

fn hex16(bytes: &[u8; 16]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(32), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

fn dash_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".into())
}

fn dash_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".into())
}

pub(crate) fn format_session_transition(
    session_id: [u8; 16],
    incarnation: Option<u64>,
    epoch: u32,
    local_load_id: Option<u64>,
    member_id: Option<[u8; 16]>,
    connection_id: Option<u64>,
    bootstrap_id: Option<u32>,
    stage_before: &str,
    event: &str,
    stage_after: &str,
    elapsed_ms: Option<u64>,
    failure_source: &str,
) -> String {
    format!(
        "session_id={} incarnation={} epoch={epoch} local_load_id={} member_id={} connection_id={} bootstrap_id={} stage_before={stage_before} event={event} stage_after={stage_after} elapsed_ms={} failure_source={failure_source}",
        hex16(&session_id),
        dash_u64(incarnation),
        dash_u64(local_load_id),
        member_id.map(|id| hex16(&id)).unwrap_or_else(|| "-".into()),
        dash_u64(connection_id),
        dash_u32(bootstrap_id),
        dash_u64(elapsed_ms),
    )
}

pub const LOADING_DEADLINE_MS: u64 = 120_000;
pub const RUNNING_SILENCE_MS: u64 = 60_000;
pub const PEER_ADMISSION_DEADLINE_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressPhase {
    Idle,
    Loading,
    Running,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressWatch {
    pub phase: ProgressPhase,
    phase_started_at: u64,
    last_progress_ms: u64,
}

impl Default for ProgressWatch {
    fn default() -> Self {
        Self {
            phase: ProgressPhase::Idle,
            phase_started_at: 0,
            last_progress_ms: 0,
        }
    }
}

impl ProgressWatch {
    pub fn beat(&mut self, now_ms: u64) {
        if now_ms > self.last_progress_ms {
            self.last_progress_ms = now_ms;
        }
    }

    pub fn set_phase(&mut self, phase: ProgressPhase, now_ms: u64) {
        self.phase = phase;
        self.phase_started_at = now_ms;
        self.last_progress_ms = now_ms;
    }

    pub fn phase_started_at(&self) -> u64 {
        self.phase_started_at
    }

    pub fn expired(&self, now_ms: u64) -> bool {
        match self.phase {
            ProgressPhase::Idle => false,
            ProgressPhase::Loading => {
                now_ms.saturating_sub(self.phase_started_at) >= LOADING_DEADLINE_MS
            }
            ProgressPhase::Running => {
                now_ms.saturating_sub(self.last_progress_ms) >= RUNNING_SILENCE_MS
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailStage {
    Transport,
    Load,
    Authority,
    Admission,
    AdvertLease,
    Overload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionFail {
    pub stage: FailStage,
    pub source: String,
    pub match_key: MatchKey,
}

impl core::fmt::Display for SessionFail {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} at {:?} epoch={}: {}",
            stage_label(self.stage),
            self.stage,
            self.match_key.match_epoch,
            self.source
        )
    }
}

fn stage_label(stage: FailStage) -> &'static str {
    match stage {
        FailStage::Transport => "transport",
        FailStage::Load => "load",
        FailStage::Authority => "authority",
        FailStage::Admission => "admission",
        FailStage::AdvertLease => "advert-lease",
        FailStage::Overload => "overload",
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostWorldReady {
    pub epoch: u32,
    pub map: u64,
    pub weapons: u64,
    pub classes: u64,
}

pub fn map_loaded_matches_host(
    live_epoch: u32,
    host: Option<HostWorldReady>,
    loaded: HostWorldReady,
) -> bool {
    let Some(host) = host else {
        return false;
    };
    epoch_applies(loaded.epoch, live_epoch)
        && epoch_applies(host.epoch, live_epoch)
        && host == loaded
}

pub fn match_boundary_applies(torn_epoch: u32, live_epoch: u32) -> bool {
    if torn_epoch == 0 {
        return live_epoch == 0;
    }
    torn_epoch == live_epoch
}

pub fn match_key_boundary_applies(torn: MatchKey, live: MatchKey) -> bool {
    if torn.is_none() {
        return live.is_none();
    }
    torn == live
}

pub fn class_select_allowed(
    session_open: bool,
    installed_belongs_to_match: bool,
    presentation_ready: bool,
    admitted: bool,
    failed: bool,
) -> bool {
    session_open && installed_belongs_to_match && presentation_ready && admitted && !failed
}

pub fn confirm_keyed_world_ready(
    installed: bool,
    authority_hold: bool,
    installed_belongs_to_live_match: bool,
    fact: HostWorldReady,
) -> Option<HostWorldReady> {
    if !installed || authority_hold || !installed_belongs_to_live_match || fact.epoch == 0 {
        return None;
    }
    Some(fact)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchPhase {
    None,
    Loading,
    Running,
    Ending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerPhase {
    Waiting,
    Syncing { bootstrap_id: u32 },
    Admitted { bootstrap_id: u32 },
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerAdmission {
    pub phase: PeerPhase,
    pub map_loaded: Option<HostWorldReady>,
    pub started_at_ms: u64,
    pub connection_id: Option<u64>,
}

impl Default for PeerAdmission {
    fn default() -> Self {
        Self {
            phase: PeerPhase::Waiting,
            map_loaded: None,
            started_at_ms: 0,
            connection_id: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostMatchEvent {
    Start {
        match_key: MatchKey,
        now_ms: u64,
    },
    AuthorityReady {
        ready: HostWorldReady,
        now_ms: u64,
    },
    AuthorityProgress {
        now_ms: u64,
    },
    MapLoaded {
        member: MemberId,
        loaded: HostWorldReady,
        now_ms: u64,
    },
    BootstrapPrepared {
        match_key: MatchKey,
        member: MemberId,
        bootstrap_id: u32,
        connection_id: Option<u64>,
    },
    Applied {
        member: MemberId,
        bootstrap_id: u32,
        connection_id: Option<u64>,
    },
    MatchEnded {
        match_key: MatchKey,
    },
    PeerCancelled {
        member: MemberId,
    },
    Close,
    Tick {
        now_ms: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostMatchEffect {
    PublishReady {
        match_key: MatchKey,
        ready: HostWorldReady,
    },
    PublishEnd {
        match_key: MatchKey,
    },
    FlushBootstraps {
        member: MemberId,
    },
    Enter {
        member: MemberId,
        bootstrap_id: u32,
        match_key: MatchKey,
    },

    CancelPeer {
        member: MemberId,
        reason: AdmissionFailure,
        match_key: MatchKey,
    },
    Fail {
        fail: SessionFail,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostMatchApply {
    pub accepted: bool,
    pub effects: Vec<HostMatchEffect>,
    pub transition: String,
}

#[derive(Clone, Debug)]
pub struct HostMatchCore {
    session_open: bool,
    match_key: MatchKey,
    phase: MatchPhase,
    host_world: Option<HostWorldReady>,
    authority_ready: bool,
    peers: HashMap<MemberId, PeerAdmission>,
    progress: ProgressWatch,
    incarnation: u64,
    local_load_id: Option<u64>,
}

impl Default for HostMatchCore {
    fn default() -> Self {
        Self {
            session_open: true,
            match_key: MatchKey::NONE,
            phase: MatchPhase::None,
            host_world: None,
            authority_ready: false,
            peers: HashMap::new(),
            progress: ProgressWatch::default(),
            incarnation: 0,
            local_load_id: None,
        }
    }
}

struct TransitionIds {
    member: Option<MemberId>,
    bootstrap_id: Option<u32>,
    connection_id: Option<u64>,
    now_ms: Option<u64>,
    elapsed_from: u64,
}

impl TransitionIds {
    fn from_event(event: &HostMatchEvent, elapsed_from: u64) -> Self {
        match event {
            HostMatchEvent::Start { now_ms, .. }
            | HostMatchEvent::AuthorityReady { now_ms, .. }
            | HostMatchEvent::AuthorityProgress { now_ms }
            | HostMatchEvent::Tick { now_ms } => Self {
                member: None,
                bootstrap_id: None,
                connection_id: None,
                now_ms: Some(*now_ms),
                elapsed_from,
            },
            HostMatchEvent::MapLoaded { member, now_ms, .. } => Self {
                member: Some(*member),
                bootstrap_id: None,
                connection_id: None,
                now_ms: Some(*now_ms),
                elapsed_from,
            },
            HostMatchEvent::BootstrapPrepared {
                member,
                bootstrap_id,
                connection_id,
                ..
            } => Self {
                member: Some(*member),
                bootstrap_id: Some(*bootstrap_id),
                connection_id: *connection_id,
                now_ms: None,
                elapsed_from,
            },
            HostMatchEvent::Applied {
                member,
                bootstrap_id,
                connection_id,
            } => Self {
                member: Some(*member),
                bootstrap_id: Some(*bootstrap_id),
                connection_id: *connection_id,
                now_ms: None,
                elapsed_from,
            },
            HostMatchEvent::PeerCancelled { member } => Self {
                member: Some(*member),
                bootstrap_id: None,
                connection_id: None,
                now_ms: None,
                elapsed_from,
            },
            HostMatchEvent::MatchEnded { .. } | HostMatchEvent::Close => Self {
                member: None,
                bootstrap_id: None,
                connection_id: None,
                now_ms: None,
                elapsed_from,
            },
        }
    }
}

fn peer_ready_effect(
    match_key: MatchKey,
    host: HostWorldReady,
    member: MemberId,
    peer: &mut PeerAdmission,
) -> Option<HostMatchEffect> {
    if matches!(
        peer.phase,
        PeerPhase::Failed | PeerPhase::Cancelled | PeerPhase::Admitted { .. }
    ) {
        return None;
    }
    let loaded = peer.map_loaded?;
    if !epoch_applies(loaded.epoch, match_key.match_epoch) {
        return None;
    }
    let reason = if host.map != loaded.map {
        Some(AdmissionFailure::MapContent {
            host: host.map,
            peer: loaded.map,
        })
    } else if host.weapons != loaded.weapons {
        Some(AdmissionFailure::WeaponRegistry {
            host: host.weapons,
            peer: loaded.weapons,
        })
    } else if host.classes != loaded.classes {
        Some(AdmissionFailure::ClassRegistry {
            host: host.classes,
            peer: loaded.classes,
        })
    } else {
        None
    };
    Some(match reason {
        Some(reason) => {
            peer.phase = PeerPhase::Failed;
            HostMatchEffect::CancelPeer {
                member,
                reason,
                match_key,
            }
        }
        None => HostMatchEffect::FlushBootstraps { member },
    })
}

impl HostMatchCore {
    pub fn match_key(&self) -> MatchKey {
        self.match_key
    }

    pub fn phase(&self) -> MatchPhase {
        self.phase
    }

    pub fn host_world(&self) -> Option<HostWorldReady> {
        self.host_world
    }

    pub fn authority_ready(&self) -> bool {
        self.authority_ready
    }

    pub fn peer(&self, member: MemberId) -> Option<&PeerAdmission> {
        self.peers.get(&member)
    }

    pub fn set_incarnation(&mut self, incarnation: u64) {
        self.incarnation = incarnation;
    }

    pub fn set_local_load_id(&mut self, local_load_id: Option<u64>) {
        self.local_load_id = local_load_id;
    }

    pub fn local_load_id(&self) -> Option<u64> {
        self.local_load_id
    }

    pub fn apply(&mut self, event: HostMatchEvent) -> HostMatchApply {
        let before = self.snapshot_label();
        let ids = TransitionIds::from_event(&event, self.progress.phase_started_at());
        if !self.session_open {
            return self.reject(&ids, before, "session closed");
        }
        match event {
            HostMatchEvent::Start { match_key, now_ms } => {
                if match_key.is_none() {
                    return self.reject(&ids, before, "start without match key");
                }
                if self.phase == MatchPhase::Loading && self.match_key == match_key {
                    return self.finish(&ids, before, "start already loading", Vec::new());
                }
                if self.phase == MatchPhase::Running && self.match_key == match_key {
                    return self.finish(&ids, before, "start already running", Vec::new());
                }
                self.match_key = match_key;
                self.phase = MatchPhase::Loading;
                self.host_world = None;
                self.authority_ready = false;
                self.peers.clear();
                self.progress.set_phase(ProgressPhase::Loading, now_ms);
                self.finish(&ids, before, "start loading", Vec::new())
            }
            HostMatchEvent::AuthorityReady { ready, now_ms } => {
                if !epoch_applies(ready.epoch, self.match_key.match_epoch) {
                    return self.reject(&ids, before, "stale host world");
                }
                if self.phase != MatchPhase::Loading && self.phase != MatchPhase::Running {
                    return self.reject(&ids, before, "host world without match");
                }
                self.host_world = Some(ready);
                self.authority_ready = true;
                if self.phase == MatchPhase::Loading {
                    self.phase = MatchPhase::Running;
                    self.progress.set_phase(ProgressPhase::Running, now_ms);
                    for peer in self.peers.values_mut() {
                        peer.started_at_ms = now_ms;
                    }
                } else {
                    self.progress.beat(now_ms);
                }
                let match_key = self.match_key;
                let effects = self
                    .peers
                    .iter_mut()
                    .filter_map(|(member, peer)| peer_ready_effect(match_key, ready, *member, peer))
                    .collect();
                self.finish(&ids, before, "authority ready", effects)
            }
            HostMatchEvent::AuthorityProgress { now_ms } => {
                if self.phase == MatchPhase::Running {
                    self.progress.beat(now_ms);
                }
                self.finish(&ids, before, "authority progress", Vec::new())
            }
            HostMatchEvent::MapLoaded {
                member,
                loaded,
                now_ms,
            } => {
                if !epoch_applies(loaded.epoch, self.match_key.match_epoch)
                    || !matches!(self.phase, MatchPhase::Loading | MatchPhase::Running)
                {
                    return self.reject(&ids, before, "stale map loaded");
                }
                let peer = self.peers.entry(member).or_default();
                if matches!(peer.phase, PeerPhase::Failed | PeerPhase::Cancelled) {
                    return self.reject(&ids, before, "map loaded after admission ended");
                }
                if self.phase == MatchPhase::Running && peer.started_at_ms == 0 {
                    peer.started_at_ms = now_ms;
                }
                peer.map_loaded = Some(loaded);
                let effects = self
                    .host_world
                    .and_then(|host| peer_ready_effect(self.match_key, host, member, peer))
                    .into_iter()
                    .collect();
                self.finish(&ids, before, "client map loaded", effects)
            }

            HostMatchEvent::BootstrapPrepared {
                match_key,
                member,
                bootstrap_id,
                connection_id,
            } => {
                if match_key != self.match_key
                    || match_key.is_none()
                    || !matches!(self.phase, MatchPhase::Loading | MatchPhase::Running)
                {
                    return self.reject(&ids, before, "stale bootstrap completion");
                }
                let Some(peer) = self.peers.get_mut(&member) else {
                    return self.reject(&ids, before, "bootstrap for unknown peer");
                };
                if matches!(
                    peer.phase,
                    PeerPhase::Admitted { .. } | PeerPhase::Failed | PeerPhase::Cancelled
                ) {
                    return self.reject(&ids, before, "bootstrap after admission ended");
                }
                if connection_id.is_some() {
                    peer.connection_id = connection_id;
                }
                peer.phase = PeerPhase::Syncing { bootstrap_id };
                self.finish(&ids, before, "bootstrap prepared", Vec::new())
            }
            HostMatchEvent::Applied {
                member,
                bootstrap_id,
                connection_id,
            } => {
                // The event carries no match key, so the peer's phase is the
                // whole guard — which means nothing may be written before it
                // has passed. A retired match's completion used to land its
                // connection id on the live peer on its way to being refused.
                let Some(phase) = self.peers.get(&member).map(|peer| peer.phase) else {
                    return self.reject(&ids, before, "applied unknown peer");
                };
                let repeat = match phase {
                    PeerPhase::Syncing {
                        bootstrap_id: pending,
                    } if pending == bootstrap_id => false,
                    PeerPhase::Admitted {
                        bootstrap_id: pending,
                    } if pending == bootstrap_id => true,
                    _ => return self.reject(&ids, before, "applied mismatch"),
                };
                let match_key = self.match_key;
                if let Some(peer) = self.peers.get_mut(&member) {
                    if connection_id.is_some() {
                        peer.connection_id = connection_id;
                    }
                    peer.phase = PeerPhase::Admitted { bootstrap_id };
                }
                self.finish(
                    &ids,
                    before,
                    if repeat {
                        "duplicate applied repeats enter"
                    } else {
                        "enter committed"
                    },
                    vec![HostMatchEffect::Enter {
                        member,
                        bootstrap_id,
                        match_key,
                    }],
                )
            }
            HostMatchEvent::MatchEnded { match_key } => {
                if !match_key_boundary_applies(match_key, self.match_key)
                    || !matches!(self.phase, MatchPhase::Loading | MatchPhase::Running)
                {
                    return self.reject(&ids, before, "stale match end");
                }
                self.phase = MatchPhase::None;
                self.authority_ready = false;
                self.host_world = None;
                self.peers.clear();
                self.progress.set_phase(ProgressPhase::Idle, 0);
                self.finish(&ids, before, "end match", Vec::new())
            }
            HostMatchEvent::PeerCancelled { member } => {
                if let Some(peer) = self.peers.get_mut(&member) {
                    peer.phase = PeerPhase::Cancelled;
                }
                self.finish(
                    &ids,
                    before,
                    "peer cancelled",
                    vec![HostMatchEffect::CancelPeer {
                        member,
                        reason: AdmissionFailure::Cancelled,
                        match_key: self.match_key,
                    }],
                )
            }
            HostMatchEvent::Close => {
                self.session_open = false;
                self.phase = MatchPhase::None;
                self.finish(&ids, before, "close session", Vec::new())
            }
            HostMatchEvent::Tick { now_ms } => {
                let mut effects = Vec::new();
                if self.progress.expired(now_ms) {
                    let stage = match self.phase {
                        MatchPhase::Loading => FailStage::Load,
                        MatchPhase::Running => FailStage::Authority,
                        MatchPhase::None | MatchPhase::Ending => FailStage::Load,
                    };
                    let source = match stage {
                        FailStage::Load => "host load deadline",
                        _ => "authority progress silence",
                    };
                    effects.push(HostMatchEffect::Fail {
                        fail: SessionFail {
                            stage,
                            source: source.into(),
                            match_key: self.match_key,
                        },
                    });
                }
                let mut expired = Vec::new();
                if self.phase == MatchPhase::Running {
                    for (member, peer) in &self.peers {
                        if matches!(peer.phase, PeerPhase::Waiting | PeerPhase::Syncing { .. })
                            && peer.started_at_ms != 0
                            && now_ms.saturating_sub(peer.started_at_ms)
                                >= PEER_ADMISSION_DEADLINE_MS
                        {
                            expired.push(*member);
                        }
                    }
                }
                for member in expired {
                    if let Some(peer) = self.peers.get_mut(&member) {
                        peer.phase = PeerPhase::Failed;
                    }
                    effects.push(HostMatchEffect::CancelPeer {
                        member,
                        reason: AdmissionFailure::Deadline,
                        match_key: self.match_key,
                    });
                }
                self.finish(&ids, before, "tick", effects)
            }
        }
    }

    fn reject(&self, ids: &TransitionIds, before: String, reason: &'static str) -> HostMatchApply {
        HostMatchApply {
            accepted: false,
            ..self.finish(ids, before, reason, Vec::new())
        }
    }

    fn snapshot_label(&self) -> String {
        format!(
            "open={},epoch={},phase={:?},authority={}",
            self.session_open, self.match_key.match_epoch, self.phase, self.authority_ready
        )
    }

    fn finish(
        &self,
        ids: &TransitionIds,
        before: String,
        reason: &'static str,
        effects: Vec<HostMatchEffect>,
    ) -> HostMatchApply {
        let member = ids.member.or_else(|| {
            effects.iter().find_map(|effect| match effect {
                HostMatchEffect::FlushBootstraps { member }
                | HostMatchEffect::Enter { member, .. }
                | HostMatchEffect::CancelPeer { member, .. } => Some(*member),
                HostMatchEffect::Fail { .. }
                | HostMatchEffect::PublishReady { .. }
                | HostMatchEffect::PublishEnd { .. } => None,
            })
        });
        let bootstrap_id = ids.bootstrap_id.or_else(|| {
            effects.iter().find_map(|effect| match effect {
                HostMatchEffect::Enter { bootstrap_id, .. } => Some(*bootstrap_id),
                _ => None,
            })
        });
        let connection_id = ids.connection_id.or_else(|| {
            member.and_then(|member| self.peers.get(&member).and_then(|peer| peer.connection_id))
        });
        let elapsed_ms = ids.now_ms.map(|now| now.saturating_sub(ids.elapsed_from));
        let failure_source = effects
            .iter()
            .find_map(|effect| match effect {
                HostMatchEffect::Fail { fail } => Some(fail.source.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "-".into());
        HostMatchApply {
            accepted: true,
            effects,
            transition: format_session_transition(
                self.match_key.session_id,
                (self.incarnation != 0).then_some(self.incarnation),
                self.match_key.match_epoch,
                self.local_load_id,
                member.map(|id| id.0),
                connection_id,
                bootstrap_id,
                &before,
                reason,
                &self.snapshot_label(),
                elapsed_ms,
                &failure_source,
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClientMatchCore {
    session_open: bool,
    match_key: MatchKey,
    retired_match: Option<MatchKey>,
    installed: Option<LocalLoadKey>,
    presentation_ready: bool,
    local_authority_ready: bool,
    adopted: Option<AdmissionKey>,
    direct_adopted: Option<LocalLoadKey>,
    enter: Option<AdmissionKey>,
    failed: Option<SessionFail>,
}

impl Default for ClientMatchCore {
    fn default() -> Self {
        Self {
            session_open: true,
            match_key: MatchKey::NONE,
            retired_match: None,
            installed: None,
            presentation_ready: false,
            local_authority_ready: false,
            adopted: None,
            direct_adopted: None,
            enter: None,
            failed: None,
        }
    }
}

impl ClientMatchCore {
    pub fn apply_teardown(&mut self, fact: frame::MatchTornDown) {
        if !match_key_boundary_applies(fact.match_key, self.match_key)
            || self
                .installed
                .is_some_and(|load| fact.world_generation.0 != Some(load.local_load_request_id))
        {
            return;
        }
        *self = Self {
            session_open: self.session_open,
            retired_match: (!fact.match_key.is_none())
                .then_some(fact.match_key)
                .or(self.retired_match),
            ..Self::default()
        };
    }

    pub fn apply_install(&mut self, load_key: LocalLoadKey) {
        if !self.session_open || self.retired_match == Some(load_key.match_key) {
            return;
        }
        let mut load_key = load_key;
        if load_key.match_key.is_none() && !self.match_key.is_none() {
            load_key.match_key = self.match_key;
        }
        if load_key.match_key != self.match_key && !self.match_key.is_none() {
            return;
        }
        self.installed = Some(load_key);
    }

    pub fn apply_presentation(&mut self, load_key: LocalLoadKey) {
        if self.installed == Some(load_key) {
            self.presentation_ready = true;
        }
    }

    pub fn apply_start(&mut self, match_key: MatchKey) {
        if !self.session_open || match_key.is_none() || self.retired_match == Some(match_key) {
            return;
        }
        if self.match_key == match_key {
            return;
        }

        if self.match_key.is_none() {
            self.match_key = match_key;
            if let Some(installed) = &mut self.installed
                && installed.match_key.is_none()
            {
                installed.match_key = match_key;
            }
            return;
        }
        self.installed = None;
        self.presentation_ready = false;
        self.local_authority_ready = false;
        self.adopted = None;
        self.direct_adopted = None;
        self.enter = None;
        self.failed = None;
        self.match_key = match_key;
    }

    pub fn apply_local_authority_ready(&mut self, ready: bool) {
        self.local_authority_ready = ready;
    }

    pub fn installed(&self) -> Option<LocalLoadKey> {
        self.installed
    }

    pub fn match_key(&self) -> MatchKey {
        self.match_key
    }

    pub fn apply_direct_adopted(&mut self, load: LocalLoadKey) {
        if self.match_key.is_none() && load.match_key.is_none() && self.installed == Some(load) {
            self.direct_adopted = Some(load);
        }
    }

    pub fn apply_adopted(&mut self, key: AdmissionKey) -> Option<String> {
        if key.match_key != self.match_key {
            return None;
        }
        if self.adopted == Some(key) {
            return None;
        }
        let before = self.snapshot_label();
        self.adopted = Some(key);
        Some(self.transition_line(&before, "client adopted", Some(key)))
    }

    pub fn apply_enter(&mut self, key: AdmissionKey) -> Option<String> {
        if self.adopted != Some(key) {
            return None;
        }
        if self.enter == Some(key) {
            return None;
        }
        let before = self.snapshot_label();
        self.enter = Some(key);
        Some(self.transition_line(&before, "client enter", Some(key)))
    }

    pub fn apply_fail(&mut self, fail: SessionFail) {
        if self.failed.is_none() {
            self.failed = Some(fail);
        }
    }

    pub fn apply_close(&mut self) {
        self.session_open = false;
    }

    pub fn class_select_allowed(&self) -> bool {
        let direct = self.match_key.is_none()
            && self.direct_adopted.is_some()
            && self.direct_adopted == self.installed;
        class_select_allowed(
            self.session_open,
            direct
                || self
                    .installed
                    .is_some_and(|key| key.belongs_to(self.match_key)),
            self.presentation_ready,
            direct || self.enter.is_some(),
            self.failed.is_some(),
        )
    }

    pub fn local_class_select_allowed(&self) -> bool {
        class_select_allowed(
            self.session_open,
            self.installed.is_some(),
            self.presentation_ready,
            self.local_authority_ready && self.installed.is_some(),
            self.failed.is_some(),
        )
    }

    pub fn failed(&self) -> Option<&SessionFail> {
        self.failed.as_ref()
    }

    fn snapshot_label(&self) -> String {
        format!(
            "open={},epoch={},installed={},presentation={},adopted={},enter={}",
            self.session_open,
            self.match_key.match_epoch,
            self.installed.is_some(),
            self.presentation_ready,
            self.adopted.is_some(),
            self.enter.is_some(),
        )
    }

    fn transition_line(
        &self,
        before: &str,
        event: &'static str,
        key: Option<AdmissionKey>,
    ) -> String {
        let key = key.or(self.adopted).or(self.enter);
        format_session_transition(
            self.match_key.session_id,
            self.installed.map(|load| load.incarnation),
            self.match_key.match_epoch,
            self.installed.map(|load| load.local_load_request_id),
            key.map(|key| key.member_id),
            key.map(|key| key.connection_id),
            key.map(|key| key.bootstrap_id),
            before,
            event,
            &self.snapshot_label(),
            None,
            self.failed
                .as_ref()
                .map(|fail| fail.source.as_str())
                .unwrap_or("-"),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionEvent {
    AuthoritativeState {
        session_id: [u8; 16],
        member: MemberId,
        revision: u64,
        match_epoch: u32,
        in_match: bool,
        start_nonce: u32,
        map: String,
        mode: String,
        members: Vec<MemberId>,
        skip_votes: u8,
    },
    EnterMatch {
        epoch: u32,
        bootstrap_id: u32,
    },
    MatchEnded {
        epoch: u32,
    },
    CloseSession,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionView {
    pub map: String,
    pub mode: String,
    pub members: Vec<MemberId>,
    pub skip_votes: u8,
    pub start_nonce: u32,
    pub in_match: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionApply {
    pub view: SessionView,
    pub changed: bool,
    pub enter_bootstrap: Option<u32>,
    pub closed: bool,

    pub transition: String,
}

#[derive(Clone, Debug)]
pub struct SessionCore {
    session_open: bool,
    applied_state: Option<(u32, u64)>,
    view: SessionView,
    match_phase: MatchPhase,
    session_id: [u8; 16],
    incarnation: u64,
    local_member: Option<MemberId>,
}

impl Default for SessionCore {
    fn default() -> Self {
        Self {
            session_open: true,
            applied_state: None,
            view: SessionView::default(),
            match_phase: MatchPhase::None,
            session_id: [0; 16],
            incarnation: 0,
            local_member: None,
        }
    }
}

impl SessionCore {
    pub fn view(&self) -> &SessionView {
        &self.view
    }

    pub fn match_phase(&self) -> MatchPhase {
        self.match_phase
    }

    pub fn for_connection(session_id: [u8; 16], incarnation: u64) -> Self {
        Self {
            session_id,
            incarnation,
            ..Self::default()
        }
    }

    pub fn apply(&mut self, event: SessionEvent) -> SessionApply {
        let before = self.snapshot_label();
        let event_label = event_label(&event);
        if !self.session_open {
            return self.finish(before, event_label, false, None, "session closed");
        }
        match event {
            SessionEvent::AuthoritativeState {
                session_id,
                member,
                revision,
                match_epoch,
                in_match,
                start_nonce,
                map,
                mode,
                members,
                skip_votes,
            } => {
                if session_id == [0; 16]
                    || (self.session_id != [0; 16] && self.session_id != session_id)
                    || member.0 == [0; 16]
                    || self.local_member.is_some_and(|local| local != member)
                    || start_nonce != match_epoch
                    || (in_match && match_epoch == 0)
                {
                    return self.finish(
                        before,
                        event_label,
                        false,
                        None,
                        "state identity mismatch",
                    );
                }
                if !state_is_newer(self.applied_state, revision, match_epoch) {
                    return self.finish(before, event_label, false, None, "stale state");
                }
                let same_match = self.view.in_match && self.view.start_nonce == match_epoch;
                self.session_id = session_id;
                self.local_member = Some(member);
                self.applied_state = Some((match_epoch, revision));
                self.view = SessionView {
                    map,
                    mode,
                    members,
                    skip_votes,
                    start_nonce,
                    in_match,
                };
                self.match_phase = if in_match {
                    if same_match && self.match_phase == MatchPhase::Running {
                        MatchPhase::Running
                    } else {
                        MatchPhase::Loading
                    }
                } else {
                    MatchPhase::None
                };
                self.finish(before, event_label, true, None, "-")
            }
            SessionEvent::EnterMatch {
                epoch,
                bootstrap_id,
            } => {
                if !epoch_applies(epoch, self.view.start_nonce) || !self.view.in_match {
                    return self.finish(before, event_label, false, None, "enter epoch mismatch");
                }
                self.match_phase = MatchPhase::Running;
                self.finish(before, event_label, false, Some(bootstrap_id), "-")
            }
            SessionEvent::MatchEnded { epoch } => {
                if !match_boundary_applies(epoch, self.view.start_nonce) {
                    return self.finish(before, event_label, false, None, "stale match end");
                }
                if !self.view.in_match {
                    return self.finish(before, event_label, false, None, "-");
                }
                self.view.in_match = false;
                self.match_phase = MatchPhase::None;
                self.finish(before, event_label, true, None, "-")
            }
            SessionEvent::CloseSession => {
                self.session_open = false;
                self.view.in_match = false;
                self.match_phase = MatchPhase::None;
                self.finish(before, event_label, true, None, "-")
            }
        }
    }

    fn snapshot_label(&self) -> String {
        format!(
            "open={},epoch={},in_match={},phase={:?},rev={:?}",
            self.session_open,
            self.view.start_nonce,
            self.view.in_match,
            self.match_phase,
            self.applied_state
        )
    }

    fn finish(
        &self,
        before: String,
        event: &'static str,
        changed: bool,
        enter_bootstrap: Option<u32>,
        failure_source: &'static str,
    ) -> SessionApply {
        let after = self.snapshot_label();
        SessionApply {
            view: self.view.clone(),
            changed,
            enter_bootstrap,
            closed: !self.session_open,
            transition: format_session_transition(
                self.session_id,
                (self.incarnation != 0).then_some(self.incarnation),
                self.view.start_nonce,
                None,
                self.local_member.map(|id| id.0),
                None,
                enter_bootstrap,
                &before,
                event,
                &after,
                None,
                failure_source,
            ),
        }
    }
}

fn event_label(event: &SessionEvent) -> &'static str {
    match event {
        SessionEvent::AuthoritativeState { .. } => "state",
        SessionEvent::EnterMatch { .. } => "enter committed",
        SessionEvent::MatchEnded { .. } => "end-match",
        SessionEvent::CloseSession => "close-session",
    }
}

pub fn state_is_newer(applied: Option<(u32, u64)>, revision: u64, epoch: u32) -> bool {
    match applied {
        None => true,
        Some((prev_epoch, _)) if epoch != prev_epoch => {
            epoch.wrapping_sub(prev_epoch) < (1_u32 << 31)
        }
        Some((_, prev_rev)) => {
            revision != prev_rev && revision.wrapping_sub(prev_rev) < (1_u64 << 63)
        }
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;

    const SESSION: [u8; 16] = [1; 16];

    fn member() -> MemberId {
        MemberId([2; 16])
    }

    fn world(epoch: u32) -> HostWorldReady {
        HostWorldReady {
            epoch,
            map: 0xa,
            weapons: 0xb,
            classes: 0xc,
        }
    }

    fn state(epoch: u32, revision: u64) -> SessionEvent {
        SessionEvent::AuthoritativeState {
            session_id: SESSION,
            member: member(),
            revision,
            match_epoch: epoch,
            in_match: true,
            start_nonce: epoch,
            map: "mp_rust".into(),
            mode: "dm".into(),
            members: vec![member()],
            skip_votes: 0,
        }
    }

    fn admission(match_key: MatchKey) -> AdmissionKey {
        AdmissionKey {
            match_key,
            member_id: member().0,
            connection_id: 9,
            bootstrap_id: 7,
        }
    }

    /// Everything about the host's admission that a stale event could move.
    fn host_snapshot(
        host: &HostMatchCore,
    ) -> (
        MatchKey,
        MatchPhase,
        bool,
        Option<HostWorldReady>,
        Option<PeerAdmission>,
    ) {
        (
            host.match_key(),
            host.phase(),
            host.authority_ready(),
            host.host_world(),
            host.peer(member()).cloned(),
        )
    }

    /// Everything about the client's that one could.
    fn client_snapshot(
        session: &SessionCore,
        client: &ClientMatchCore,
    ) -> (
        MatchPhase,
        SessionView,
        MatchKey,
        Option<LocalLoadKey>,
        bool,
    ) {
        (
            session.match_phase(),
            session.view().clone(),
            client.match_key(),
            client.installed(),
            client.class_select_allowed(),
        )
    }

    /// The three cores that decide, independently and from different messages,
    /// whether a player is in a match: the host's admission, the client's view
    /// of the session, and the client's own load. A replacement match has to
    /// retire all three, and nothing the retired match says afterwards — a
    /// bootstrap that completed late, a teardown, a load that finished — may
    /// move any of them. Before the session identity travelled whole, a late
    /// bootstrap could create a peer in the match that replaced it.
    #[test]
    fn a_replacement_match_retires_all_three_cores_and_the_old_one_can_no_longer_move_them() {
        let first = MatchKey::new(SESSION, 4);
        let second = MatchKey::new(SESSION, 5);

        // --- the host admits the member into match 4, effect by effect.
        let mut host = HostMatchCore::default();
        assert!(
            host.apply(HostMatchEvent::Start {
                match_key: first,
                now_ms: 0,
            })
            .accepted
        );
        assert_eq!(host.phase(), MatchPhase::Loading);

        let ready = host.apply(HostMatchEvent::AuthorityReady {
            ready: world(4),
            now_ms: 1,
        });
        assert!(ready.accepted);
        assert!(ready.effects.is_empty(), "no peer has loaded yet");
        assert_eq!(host.phase(), MatchPhase::Running);

        let loaded = host.apply(HostMatchEvent::MapLoaded {
            member: member(),
            loaded: world(4),
            now_ms: 2,
        });
        assert_eq!(
            loaded.effects,
            vec![HostMatchEffect::FlushBootstraps { member: member() }]
        );

        assert!(
            host.apply(HostMatchEvent::BootstrapPrepared {
                match_key: first,
                member: member(),
                bootstrap_id: 7,
                connection_id: Some(9),
            })
            .accepted
        );
        assert_eq!(
            host.peer(member()).unwrap().phase,
            PeerPhase::Syncing { bootstrap_id: 7 }
        );

        let entered = host.apply(HostMatchEvent::Applied {
            member: member(),
            bootstrap_id: 7,
            connection_id: Some(9),
        });
        assert_eq!(
            entered.effects,
            vec![HostMatchEffect::Enter {
                member: member(),
                bootstrap_id: 7,
                match_key: first,
            }]
        );
        assert_eq!(
            host.peer(member()).unwrap().phase,
            PeerPhase::Admitted { bootstrap_id: 7 }
        );

        // --- the same match, as the client's two cores see it.
        let mut session = SessionCore::for_connection(SESSION, 5);
        assert!(session.apply(state(4, 1)).changed);
        assert_eq!(session.match_phase(), MatchPhase::Loading);
        assert_eq!(
            session
                .apply(SessionEvent::EnterMatch {
                    epoch: 4,
                    bootstrap_id: 7,
                })
                .enter_bootstrap,
            Some(7)
        );
        assert_eq!(session.match_phase(), MatchPhase::Running);

        let mut client = ClientMatchCore::default();
        client.apply_start(first);
        let load = LocalLoadKey::from_request(11, first, 5);
        client.apply_install(load);
        client.apply_presentation(load);
        assert!(client.apply_adopted(admission(first)).is_some());
        assert!(client.apply_enter(admission(first)).is_some());
        assert!(client.class_select_allowed());

        // --- match 5 replaces it everywhere.
        assert!(
            host.apply(HostMatchEvent::Start {
                match_key: second,
                now_ms: 10,
            })
            .accepted
        );
        assert!(session.apply(state(5, 2)).changed);
        client.apply_start(second);

        // The client is loading again, and holds nothing from match 4.
        assert_eq!(session.match_phase(), MatchPhase::Loading);
        assert_eq!(client.match_key(), second);
        assert_eq!(client.installed(), None);
        assert!(!client.class_select_allowed());
        // The host kept no peer either: admission is per match.
        assert_eq!(host.phase(), MatchPhase::Loading);
        assert!(host.peer(member()).is_none());
        assert!(!host.authority_ready());

        // The member loads match 5 as well, so the host has a live peer again.
        // That is the only state in which the stale events below are dangerous:
        // with no peer they are refused for want of one, and the checks that
        // matter never run.
        assert!(
            host.apply(HostMatchEvent::AuthorityReady {
                ready: world(5),
                now_ms: 11,
            })
            .accepted
        );
        assert_eq!(
            host.apply(HostMatchEvent::MapLoaded {
                member: member(),
                loaded: world(5),
                now_ms: 12,
            })
            .effects,
            vec![HostMatchEffect::FlushBootstraps { member: member() }]
        );
        assert_eq!(host.peer(member()).unwrap().phase, PeerPhase::Waiting);

        // --- everything match 4 still has in flight, replayed. Not one of it
        // is allowed to touch a core, and the rejections say so out loud.
        let host_before = host_snapshot(&host);
        let client_before = client_snapshot(&session, &client);

        for stale in [
            HostMatchEvent::BootstrapPrepared {
                match_key: first,
                member: member(),
                bootstrap_id: 7,
                connection_id: Some(9),
            },
            HostMatchEvent::Applied {
                member: member(),
                bootstrap_id: 7,
                connection_id: Some(9),
            },
            HostMatchEvent::MapLoaded {
                member: member(),
                loaded: world(4),
                now_ms: 11,
            },
            HostMatchEvent::AuthorityReady {
                ready: world(4),
                now_ms: 11,
            },
            HostMatchEvent::MatchEnded { match_key: first },
        ] {
            let apply = host.apply(stale);
            assert!(!apply.accepted, "match 4 moved the host: {apply:?}");
            assert!(
                apply.effects.is_empty(),
                "match 4 produced {:?}",
                apply.effects
            );
            assert_eq!(host_snapshot(&host), host_before);
        }

        // The client's side of the same: a load that finished for match 4, the
        // admission it was granted there, and the state message that carried it.
        client.apply_install(load);
        client.apply_presentation(load);
        assert_eq!(client.apply_adopted(admission(first)), None);
        assert_eq!(client.apply_enter(admission(first)), None);
        assert!(!session.apply(state(4, 3)).changed);
        assert!(
            !session
                .apply(SessionEvent::EnterMatch {
                    epoch: 4,
                    bootstrap_id: 7
                })
                .changed
        );
        assert_eq!(client_snapshot(&session, &client), client_before);

        // And the boundary helper the three of them share agrees.
        assert!(!match_key_boundary_applies(first, second));
        assert!(match_key_boundary_applies(second, second));
    }
}
