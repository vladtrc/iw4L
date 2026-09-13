pub mod authority;
pub mod client;
pub mod gaps;
mod observe;
pub mod plugin;
pub mod policy;
pub mod reconciliation;
pub mod role;
pub mod role_matrix;
pub mod schedule;
pub mod session_core;
pub mod signon;
pub mod svc_gamenotify;
pub mod svc_playercard;
pub mod svc_scores;
pub mod svc_sound;
pub mod transport;

pub use authority::actions::{
    ActionAdmission, ActionRequestIds, ClientActionLedger, MAX_REMEMBERED_ACTIONS_PER_CLIENT,
};
pub use authority::inbox::{
    AUTHORITY_HZ, AUTHORITY_MS, ActionEnqueueError, AuthorityClock, ClientActionInbox,
    ClientCommandInbox, GatheredCommands, MAX_PENDING_ACTIONS_PER_CLIENT, MAX_REDUNDANT_CMDS,
    ServerTime, run_fixed_authority_stream,
};
pub use authority::runtime::{
    AuthorityInputGate, AuthorityLoadHold, AuthorityPhaseCensus, AuthorityPhaseTrace,
    AuthorityWorld, ClientShotSamples, DumpDeathLog, DumpGiveLog, FixedUpdateCensus,
    ListenFanoutCensus, NetDiagnostics, PendingAcks, PendingAuthorityInput, PendingStepResult,
    ScriptNotifyEmitStats, ServerTick, ServerTickData, authority_bookkeeping,
    authority_should_tick,
};
pub use client::centity_runtime::{
    CEntityDobjHandle, CEntityFxHandle, CEntityRuntime, CgPlayerDrawGate, CurrentLerpState,
    EFLAGS_DEAD, EFLAGS_TELEPORT, RemoteBodySubmitKind, RemotePoseSample,
    corpse_slot_to_entity_state, player_state_to_entity_state, remote_body_submit_kind,
    remote_body_submits, remote_pose_sample,
};
pub use client::cg_frame::{CgFrameClock, CgameActive, CgameJoinCensus};
pub use client::cls_frame::ClsRealtime;
pub use client::entities::{
    CEntity, CEntityBirthCensus, CEntitySlots, CLIENT_ENTITY_SLOT_COUNT, register_client_entities,
    sync_client_entities,
};
pub use client::entity_event_dispatch::{
    AppliedEntityEventWalk, DispatchedEntityEvent, EntityBulletHit, EntityEjectBrass,
    EntityEventCursor, EntityEventSound, EntityExplosion, EntityGrenadeContact, EntityMeleeBlood,
    EntityMovementSound, EntityObituary, EntityPlayFx, EntityResetAds, EntityWeaponFire,
    UnsupportedEntityEvents, WeaponFirePing, WeaponFirePingBus, register_entity_event_dispatch,
};
pub use client::entity_event_registry::{
    EV_DISPATCH_REGISTRY, EntityEventDispatch, EntityEventRow, ev_dispatch_row,
};
pub use client::frame_census::{ClientPhaseCensus, HUD_STAGE_N, UpdatePhaseCensus};
pub use client::input::{
    ClientActionInput, KEY_FRAME_MSEC_MAX, LookState, accumulate_look, build_usercmd,
    com_frame_time_msec, idle_usercmd, key_frame_msec, look_angles_from_degrees,
};
pub use client::predict::{
    AckMatch, ClientPrediction, CmdSeq, DEFAULT_HISTORY_CAP, MAX_UNACKED_SNAPSHOTS, MoveHistory,
    MoveRecord, PredictionMetrics, ReconcileOutcome, snapshot_ground_e_type,
};
pub use client::predicted_error::PredictedError;
pub use client::presented::{
    CgViewweaponAim, FpvEventCues, FpvHandRecord, FpvRawHandSample, LocalPresentClient,
    PresentLocalCensus, PresentedSnapshot,
};
pub use client::projectiles::{PresentedProjectile, count_throw_rows, merge_presented_projectiles};
pub use client::proxy::{
    AnimGap, FIXED_DELAY_POLICY_REVISION, PROXY_BUFFER_TICKS, PROXY_DELAY_MS,
    PresentationSampleOutcome, PresentationSampleProvenance, PresentationSampleTime, ProxyMode,
    ProxyPolicyRevision, ProxySample, ProxyStarvationReason, RemoteProxy,
};
pub use client::runtime::{
    CgWeaponSelect, ClientClock, ClientCmdTemplate, ClientPhaseTrace, ClientPredictionState,
    ClientReliableAck, ClockTick, LastAdoptedSnapshot, PendingClientSends, PendingPelletFx,
    PendingPresentedEntityEvents, ReceivedTicks, ReliableControlEvent, RemoteProxyState,
    advance_cg_frame_clock, advance_cls_realtime, arm_listen_prediction, cg_cycle_weapon_select,
    cg_follow_held_weapon_select, listen_prediction_needs_content, predict_local_move,
    publish_presented, receive_ticks, reconcile_prediction, register_client_runtime,
    register_listen_prediction_arm, sample_client_input, send_pending_commands,
};
pub use entity_iw4::EntityEventKind;
pub use gaps::{NetGap, NetGapCause, NetIdentityGaps, ScriptNotify};
pub use master_protocol::{AdvertId, ContentFlags, SessionCloseReason};
pub use plugin::NetPlugin;
pub use policy::killcam::{
    ActiveKillcamSkips, PendingDeathTimelines, ScriptKillcamEmitStats, session_from_start_killcam,
    session_from_window_plan, time_until_spawn_seconds, use_button_pressed,
};
pub use policy::seat::{
    ActiveKillcams, KillcamSession, SeatSample, apply_seat_to_snapshot, killcam_seconds_to_ms,
    sample_killcam_seat, snapshot_and_sample_for_viewer, snapshot_for_viewer,
};
pub use reconciliation::{
    CorrectionBoundary, CorrectionRule, IDENTITY_CONTRACT, IdentityContractRow,
    SIDE_EFFECT_CONTRACT, STATE_CONTRACT, SideEffectContractRow, SideEffectReplayRule,
    SnapshotOrder, StateClass, StateContractRow, classify_snapshot,
};
pub use role::RuntimeRole;
pub use schedule::{
    AUTHORITY_TOC, AuthoritySet, CLIENT_TOC, ClientSet, PresentedPublished,
    configure_authority_sets, configure_client_sets,
};
pub use session_core::{
    ClientMatchCore, FailStage, HostMatchApply, HostMatchCore, HostMatchEffect, HostMatchEvent,
    HostWorldReady, MatchPhase, PeerAdmission, PeerPhase, ProgressPhase, ProgressWatch,
    SessionApply, SessionCore, SessionEvent, SessionFail, SessionView, class_select_allowed,
    confirm_keyed_world_ready, map_loaded_matches_host, match_boundary_applies,
    match_key_boundary_applies,
};
pub use signon::{
    ClientAdmission, DeferBootstrapApplied, DeferHostWorldReady, MatchEpoch, SignonFailReason,
    SignonPhase, SignonState, derive_signon, host_world_ready_is_deferred, live_match_key,
    phase_from_udp_link,
};
pub use svc_gamenotify::{
    PendingGameNotify, SVC_DISCONNECT_NOTIFY, SVC_PRINT, SvcGameNotify, client_name_string,
};
pub use svc_playercard::{
    PendingPlayerCard, SVC_CARD_SLOT, SVC_OPEN_MENU, SvcCardSlot, SvcCardSlotCmd, SvcHudSplash,
    SvcOpenMenu, SvcOpenMenuCmd,
};
pub use svc_scores::{
    CgScores, PendingScoreboard, SCORES_REQUEST_MS, SVC_SCORES, format_scoreboard_cmd,
    format_scoreboard_from_snapshot, parse_scoreboard_cmd,
};
pub use svc_sound::{PendingSvcSounds, SVC_PLAY_LOCAL, SVC_STOP_LOCAL, SvcLocalSound, SvcSound};
pub use transport::acked_baseline::{AckedBaselineTable, FlakyDatagramQueue};
pub use transport::archive::{
    ARCHIVE_BYTE_BUDGET, ARCHIVE_CACHED_SNAPSHOT_CLIENTS, ARCHIVE_LOOKUP_WINDOW_TICKS,
    ARCHIVE_MAX_TICKS, ARCHIVE_TICK_MS, ArchiveLookup, ArchiveMetrics, ArchivedFrame, FrameArchive,
    archive_attainable_ticks,
};
pub use transport::bootstrap::BootstrapAck;
pub use transport::delta::{
    ProjectileEntityDelta, SnapshotDecoder, SnapshotDelta, SnapshotEncoder,
};
pub use transport::frame::{
    Frame, FrameSectionBytes, LoopbackTransport, Transport, TransportError,
    authoritative_snapshot_hash, frame_from_acked_tick, frame_from_tick,
};
pub use transport::loopback_live::{ListenLoopback, ReceivedTick};
pub use transport::master::{
    CONTENT_IW4, CONTENT_IW5, CONTENT_T5, MasterAdvert, MasterBridge, MasterBridgeState,
    MasterBrowser, MasterBrowserSnapshot, MasterLaunchIntent, MasterLifecycleFact,
    MasterMatchOffer, MasterMatchStart, MasterMenuAction, PendingMasterMenuAction, SessionIdentity,
    TransportFault, content_inventory, content_names, content_required_by_map,
};
pub use transport::meta_wire::{
    SnapshotMetaSectionBytes, WORLD_SYNC_PERIOD_TICKS, WorldObjectSyncDecoder,
    WorldObjectSyncEncoder, decode_world_object_sync_wire, encode_world_object_sync,
};
pub use transport::netfields::{
    Deviation, PS_FIELD_COUNT, PS_NETFIELDS, PsNetField, Replication, Validation,
    adopt_only_fields, compute_state_hash, player_state_diff, ps_deviation,
};
pub use transport::protocol::{
    ClientPacket, ConnectionId, ConnectionTable, ContentFingerprint, HandshakeHello,
    HandshakeReject, IdentityError, MatchDescriptor, PacketHeader, ProtocolLimits, ServerPacket,
    UDP_IMPLEMENTED, decode_client_packet, decode_server_packet, evaluate_handshake,
};
pub use transport::reliable::{
    ActionVerdict, MAX_PENDING_RELIABLE, ReliableEventHub, ReliableEventQueue, ReliablePayload,
    ReliableRow, decode_reliable_payload, encode_reliable_payload,
};
pub use transport::udp_launch::{UdpLaunchIntent, handshake_hello_for_udp};
pub use transport::udp_session::{CommittedAdmission, UdpAuthorityHub, UdpClientLink};
pub use transport::udp_socket::{DEFAULT_RECV_BUDGET_PER_TICK, UdpDatagramSocket, UdpSendError};
pub use transport::wire::{WireError, WireReader, WireWriter};

pub const PROTOCOL_VERSION: u32 = 73;
