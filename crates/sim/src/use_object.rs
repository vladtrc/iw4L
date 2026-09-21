use crate::frame::FrameWorld;
use entity_iw4::{
    TEAM_ALLIES, TEAM_AXIS, sv_link_entity_needs_rotated_radius, sv_link_entity_world_bounds,
};
use gamemode_iw4::{
    CreateUseTriggerKind, DOM_FLAG_SCORE_POINTS, GameObjectTeam, InteractTeam, OwnedDomFlag,
    ProxClaimTeam, ProxThinkInput, ProxThinkOutcome, SCORE_CAPTURE_POINTS, TRIGGER_RADIUS, Team,
    TouchCredit, TriggerRadiusError, USE_HOLD_TICK_MS, USE_HOLD_WEAPON_WAIT_MAX_MS,
    UseCallbackKind, UseHoldLoopInput, UseHoldLoopState, UseHoldLoopTick, UseNotifySlots,
    UseScriptCall, can_interact_with, capture_player_score_points, capture_rank_xp_amount,
    capture_splash_optional, claim_team_from_owner, create_use_trigger_kind, earliest_claim_player,
    is_capture_touch, is_owned_dom_flag, on_use_sounds, on_use_status_lines,
    owner_team_from_capturer, prox_begin_calls, prox_complete_calls, prox_instant_use_calls,
    prox_unclaim_calls, prox_use_update_call, scoring_team, set_claim_team_resets_progress,
    set_use_time_ms, sort_owned_oldest_first, team_flag_count, teambased_rank_xp_allowed,
    trigger_radius_box, update_use_rate, use_hold_loop_tick, use_object_prox_think_body,
    use_type_after_hold_calls, use_type_begin_calls, world_aabb_from_link_bounds,
};
use math_iw4::snap_angles;
use playerstate_iw4::{ENTITYNUM_NONE, PM_TYPE_DEAD, PlayerState, buttons};

use crate::ClientLifecycle;
use crate::bullet_collision::{PLAYER_MAXS, PLAYER_MINS};
use crate::gentity::UsePress;
use crate::world::{ClientId, SimState, Tick};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UseTriggerKind {
    #[default]
    Use,
    Proximity,
}

impl From<CreateUseTriggerKind> for UseTriggerKind {
    fn from(kind: CreateUseTriggerKind) -> Self {
        match kind {
            CreateUseTriggerKind::Use => UseTriggerKind::Use,
            CreateUseTriggerKind::Proximity => UseTriggerKind::Proximity,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UseObject {
    pub id: u32,
    pub kind: UseTriggerKind,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],

    pub cylinder: Option<[f32; 2]>,
    pub use_time_ms: i32,
    pub use_weapon: Option<u32>,
    pub owner_team: GameObjectTeam,
    pub interact_team: InteractTeam,
    pub in_use: bool,
    pub cur_progress: i32,
    pub use_rate: f32,
    pub claim: ProxClaimTeam,
    pub last_claim: ProxClaimTeam,
    pub last_claim_time_ms: i32,
    pub claim_player: Option<ClientId>,

    pub touching: Vec<(ClientId, ProxClaimTeam, i32)>,

    pub bound_entnum: Option<i32>,
    pub notify_slots: UseNotifySlots,
    pub callback_kind: UseCallbackKind,

    pub capture_time_ms: Option<i32>,

    pub script_label: gamemode_iw4::ScriptLabel,

    pub script_origin: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UseObjectInstall {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],

    pub cylinder: Option<[f32; 2]>,
    pub use_time_ms: i32,
    pub use_weapon: Option<u32>,
    pub kind: UseTriggerKind,
    pub owner_team: GameObjectTeam,
    pub interact_team: InteractTeam,
    pub bound_entnum: Option<i32>,
    pub notify_slots: UseNotifySlots,
    pub callback_kind: UseCallbackKind,
    pub script_label: gamemode_iw4::ScriptLabel,

    pub script_origin: [f32; 3],
}

impl UseObjectInstall {
    pub const fn with_notify_slots(mut self, notify_slots: UseNotifySlots) -> Self {
        self.notify_slots = notify_slots;
        self
    }

    pub const fn with_callback_kind(mut self, callback_kind: UseCallbackKind) -> Self {
        self.callback_kind = callback_kind;
        self
    }

    pub const fn with_dom_flag_callbacks(self) -> Self {
        self.with_notify_slots(UseNotifySlots::dom_flag())
            .with_callback_kind(UseCallbackKind::DomFlag)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapUseBindError {
    MissingRadius,
    MissingHeight,
    MissingBrushBox,
    UnknownEnt,
}

impl From<TriggerRadiusError> for MapUseBindError {
    fn from(err: TriggerRadiusError) -> Self {
        match err {
            TriggerRadiusError::MissingRadius => MapUseBindError::MissingRadius,
            TriggerRadiusError::MissingHeight => MapUseBindError::MissingHeight,
        }
    }
}

pub fn world_aabb_from_r_box(
    origin: [f32; 3],
    angles: [f32; 3],
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    if sv_link_entity_needs_rotated_radius(snap_angles(angles), box_half) {
        panic!("SV_LinkEntity yaw/pitch radius Bounds when r.half is non-zero");
    }
    let bounds = sv_link_entity_world_bounds(origin, box_mid, box_half);
    world_aabb_from_link_bounds(bounds.mid, bounds.half)
}

pub fn trigger_radius_world_aabb(
    origin: [f32; 3],
    radius: Option<f32>,
    height: Option<f32>,
) -> Result<([f32; 3], [f32; 3]), TriggerRadiusError> {
    let (box_mid, box_half) = trigger_radius_box(radius, height)?;
    Ok(world_aabb_from_r_box(origin, [0.0; 3], box_mid, box_half))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UseHoldSession {
    pub object: u32,
    pub client: ClientId,
    pub loop_state: UseHoldLoopState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseCancelReason {
    Dead,
    LeftTrigger,
    ReleasedUse,
    Melee,
    ThrowingGrenade,
    WeaponWaitTimeout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseObjectEvent {
    Began {
        object: u32,
        client: u32,
    },
    Progress {
        object: u32,
        client: u32,
        cur_progress: i32,
        use_time: i32,
    },
    Completed {
        object: u32,
        client: u32,
    },
    Cancelled {
        object: u32,
        client: u32,
        reason: UseCancelReason,
    },
    Claimed {
        object: u32,
        client: u32,
        team: u8,
    },
    Unclaimed {
        object: u32,
        team: u8,
    },
    Contested {
        object: u32,
        cur_progress: i32,
    },
    OnBeginUse {
        object: u32,
        client: u32,
    },
    OnEndUse {
        object: u32,
        team: u8,
        client: Option<u32>,
        success: bool,
    },
    OnUse {
        object: u32,
        client: u32,
    },
    OnUseUpdate {
        object: u32,
        team: u8,
        progress_milli: i32,
        change_milli: i32,
    },

    UseCallbackBlocked {
        object: u32,
        kind: UseCallbackKind,
    },

    FlagCaptured {
        object: u32,
        client: u32,
        old_owner: GameObjectTeam,
        new_owner: GameObjectTeam,
        owned_count: i32,
    },

    DomTeamScore {
        team: u8,
        added: i32,
        total: i32,
    },

    FlagCaptureXp {
        object: u32,
        client: u32,
        earliest: u32,
        added: i32,
        total: i32,
    },

    CaptureSplash {
        object: u32,
        client: u32,
        optional: i32,
    },

    CaptureRankXp {
        object: u32,
        client: u32,
        amount: i32,
    },

    CaptureCallout {
        object: u32,
        client: u32,
        label: gamemode_iw4::ScriptLabel,
    },

    StatusDialog {
        object: u32,
        kind: gamemode_iw4::DomStatusDialogKind,
        team: GameObjectTeam,
        label: gamemode_iw4::ScriptLabel,
    },

    FlagCompass {
        object: u32,
        owner: GameObjectTeam,
        label: gamemode_iw4::ScriptLabel,
    },

    ObjectiveSound {
        object: u32,
        taken_team: GameObjectTeam,
        lost_team: Option<GameObjectTeam>,
        lost: bool,
    },

    FlagBaseEffect {
        object: u32,
        owner: GameObjectTeam,
        spawned: bool,
    },
}

fn team_u8(team: Team) -> u8 {
    team as u8
}

fn teambased_player_counts(world: &FrameWorld) -> (u32, u32) {
    let mut allies = 0u32;
    let mut axis = 0u32;
    for id in world.client_ids_sorted() {
        match world.client_meta(id).map(|m| m.client_state_team) {
            Some(TEAM_ALLIES) => allies += 1,
            Some(TEAM_AXIS) => axis += 1,
            _ => {}
        }
    }
    (allies, axis)
}

fn apply_dom_on_use_body(world: &mut FrameWorld, object: u32, player: u32) {
    let pers = pers_team(world, ClientId(player));
    let old_owner = world
        .use_object(object)
        .map(|row| row.owner_team)
        .unwrap_or(GameObjectTeam::Neutral);
    let new_owner = match owner_team_from_capturer(pers) {
        Ok(team) => team,
        Err(_) => {
            panic!("onUse assert team != neutral; TEAM_FREE cannot capture");
        }
    };
    world.set_use_owner_team(object, new_owner);
    let t = world.entity_kernel().level_time_ms();
    world.set_use_capture_time(object, t);
    world.set_use_start_spawns(false);
    let label = world
        .use_object(object)
        .map(|row| row.script_label)
        .unwrap_or_else(gamemode_iw4::ScriptLabel::empty);
    let owners: Vec<GameObjectTeam> = world
        .use_objects()
        .iter()
        .filter(|row| row.callback_kind == UseCallbackKind::DomFlag)
        .map(|row| row.owner_team)
        .collect();
    let owned_count = team_flag_count(&owners, new_owner);
    let flags_size = owners.len() as i32;
    let sounds = on_use_sounds(old_owner, new_owner);
    world.push_use_event(UseObjectEvent::ObjectiveSound {
        object,
        taken_team: sounds.taken_team,
        lost_team: sounds.lost_team,
        lost: sounds.lost.is_some(),
    });
    world.push_use_event(UseObjectEvent::FlagCompass {
        object,
        owner: new_owner,
        label,
    });
    world.push_use_event(UseObjectEvent::FlagBaseEffect {
        object,
        owner: new_owner,
        spawned: false,
    });
    if let Some(lines) = on_use_status_lines(old_owner, new_owner, owned_count, flags_size, label) {
        let now = world.entity_kernel().level_time_ms();
        for line in lines {
            if world.take_status_dialog(line.team, now, true) {
                world.push_use_event(UseObjectEvent::StatusDialog {
                    object,
                    kind: line.kind,
                    team: line.team,
                    label: line.label,
                });
            }
        }
    }
    if matches!(old_owner, GameObjectTeam::Axis | GameObjectTeam::Allies) {
        world.set_best_spawn_flag(old_owner, object);
    }
    if let Some(team) = claim_team_from_owner(new_owner)
        && let Some(row) = world.use_object(object).cloned()
    {
        let credits: Vec<TouchCredit> = row
            .touching
            .iter()
            .map(|(id, team, start)| TouchCredit {
                client: id.0,
                team: *team,
                start_time_ms: *start,
                alive: world
                    .player(*id)
                    .is_some_and(|ps| is_really_alive(world, *id, ps)),
            })
            .collect();
        let claim_alive = row.claim_player.is_some_and(|id| {
            world
                .player(id)
                .is_some_and(|ps| is_really_alive(world, id, ps))
        });
        let earliest =
            earliest_claim_player(row.claim_player.map(|id| id.0), claim_alive, &credits)
                .unwrap_or(player);
        let points = capture_player_score_points();
        let (allies_n, axis_n) = teambased_player_counts(world);
        let rank_xp = teambased_rank_xp_allowed(allies_n, axis_n);
        let splash_optional = capture_splash_optional();
        let time_ms = world.match_elapsed_ms() as i32;
        for credit in credits
            .iter()
            .copied()
            .filter(|row| is_capture_touch(*row, team))
        {
            world.push_use_event(UseObjectEvent::CaptureSplash {
                object,
                client: credit.client,
                optional: splash_optional,
            });
            let cpm = world.apply_capture_pace(ClientId(credit.client), time_ms);
            if rank_xp {
                world.push_use_event(UseObjectEvent::CaptureRankXp {
                    object,
                    client: credit.client,
                    amount: capture_rank_xp_amount(cpm as f32),
                });
            }
            let total = world.give_player_objective_score(ClientId(credit.client), points);
            world.push_use_event(UseObjectEvent::FlagCaptureXp {
                object,
                client: credit.client,
                earliest,
                added: SCORE_CAPTURE_POINTS,
                total,
            });
        }
        world.push_use_event(UseObjectEvent::CaptureCallout {
            object,
            client: earliest,
            label,
        });
    }
    world.push_use_event(UseObjectEvent::FlagCaptured {
        object,
        client: player,
        old_owner,
        new_owner,
        owned_count,
    });
}

fn emit_script_calls(
    world: &mut FrameWorld,
    object: u32,
    kind: UseCallbackKind,
    calls: impl IntoIterator<Item = Option<UseScriptCall>>,
) {
    for call in calls.into_iter().flatten() {
        match call {
            UseScriptCall::BeginUse { player } => {
                world.push_use_event(UseObjectEvent::OnBeginUse {
                    object,
                    client: player,
                });
            }
            UseScriptCall::EndUse {
                team,
                player,
                success,
            } => {
                world.push_use_event(UseObjectEvent::OnEndUse {
                    object,
                    team: team_u8(team),
                    client: player,
                    success,
                });
            }
            UseScriptCall::Use { player } => {
                world.push_use_event(UseObjectEvent::OnUse {
                    object,
                    client: player,
                });
                match kind {
                    UseCallbackKind::Unbound => {}
                    UseCallbackKind::DomFlag => apply_dom_on_use_body(world, object, player),
                    UseCallbackKind::DemBombzone => {
                        world.push_use_event(UseObjectEvent::UseCallbackBlocked { object, kind });
                        if kind.on_use_gap() == Some(gamemode_iw4::ScriptGap::DemOnUseObject) {
                            world
                                .script_gaps_mut()
                                .raise(gamemode_iw4::ScriptGapCause::DemOnUseObject { object });
                        }
                    }
                }
            }
            UseScriptCall::UseUpdate {
                team,
                progress_milli,
                change_milli,
            } => {
                world.push_use_event(UseObjectEvent::OnUseUpdate {
                    object,
                    team: team_u8(team),
                    progress_milli,
                    change_milli,
                });
            }
        }
    }
}

fn aabb_overlap(
    a_origin: [f32; 3],
    a_mins: [f32; 3],
    a_maxs: [f32; 3],
    b_origin: [f32; 3],
    b_mins: [f32; 3],
    b_maxs: [f32; 3],
) -> bool {
    for i in 0..3 {
        let a_min = a_origin[i] + a_mins[i];
        let a_max = a_origin[i] + a_maxs[i];
        let b_min = b_origin[i] + b_mins[i];
        let b_max = b_origin[i] + b_maxs[i];
        if a_min > b_max || a_max < b_min {
            return false;
        }
    }
    true
}

fn is_really_alive(world: &crate::world::SimState, id: ClientId, ps: &PlayerState) -> bool {
    world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        && ps.health >= 1
        && ps.pm_type < PM_TYPE_DEAD
}

fn is_touching(ps: &PlayerState, object: &UseObject) -> bool {
    origin_touching(ps.origin, object)
}

pub(crate) fn origin_touching(origin: [f32; 3], object: &UseObject) -> bool {
    if let Some([radius, height]) = object.cylinder {
        let mid = std::array::from_fn(|i| origin[i] + (PLAYER_MINS[i] + PLAYER_MAXS[i]) * 0.5);
        let half = std::array::from_fn(|i| (PLAYER_MAXS[i] - PLAYER_MINS[i]) * 0.5);
        return gamemode_iw4::use_bind::cylinder_contact(
            mid,
            half,
            object.script_origin,
            radius,
            height,
        );
    }
    aabb_overlap(
        origin,
        PLAYER_MINS,
        PLAYER_MAXS,
        [0.0, 0.0, 0.0],
        object.mins,
        object.maxs,
    )
}

fn on_ground(ps: &PlayerState) -> bool {
    ps.ground_entity_num != ENTITYNUM_NONE
}

fn is_killstreak_weapon(world: &crate::world::SimState, weapon: u32) -> bool {
    world.weapon_script_name(weapon).contains("killstreak")
}

fn pers_team(world: &crate::world::SimState, id: ClientId) -> Team {
    match world.client_meta(id).map(|m| m.client_state_team) {
        Some(entity_iw4::TEAM_AXIS) => Team::Axis,
        Some(entity_iw4::TEAM_ALLIES) => Team::Allies,
        _ => Team::Free,
    }
}

pub(crate) fn eligible_begin(
    world: &crate::world::SimState,
    id: ClientId,
    ps: &PlayerState,
    object: &UseObject,
) -> bool {
    object.kind == UseTriggerKind::Use
        && is_really_alive(world, id, ps)
        && is_touching(ps, object)
        && on_ground(ps)
        && !is_killstreak_weapon(world, ps.weapon)
        && can_interact_with(
            object.interact_team,
            object.owner_team,
            pers_team(world, id),
        )
}

fn cancel_reason(input: &UseHoldLoopInput, state: &UseHoldLoopState) -> UseCancelReason {
    if !input.alive {
        UseCancelReason::Dead
    } else if !input.touching {
        UseCancelReason::LeftTrigger
    } else if !input.use_pressed {
        UseCancelReason::ReleasedUse
    } else if input.melee_pressed {
        UseCancelReason::Melee
    } else if input.throwing_grenade {
        UseCancelReason::ThrowingGrenade
    } else if state.wait_for_weapon && state.timed_out_ms > USE_HOLD_WEAPON_WAIT_MAX_MS {
        UseCancelReason::WeaponWaitTimeout
    } else {
        UseCancelReason::ReleasedUse
    }
}

fn loop_input(
    world: &FrameWorld,
    id: ClientId,
    ps: &PlayerState,
    object: &UseObject,
    buttons_now: u32,
) -> UseHoldLoopInput {
    let weapon_ready = match object.use_weapon {
        None => true,
        Some(want) => ps.weapon == want,
    };
    UseHoldLoopInput {
        alive: is_really_alive(world, id, ps),
        touching: is_touching(ps, object),
        use_pressed: buttons_now & buttons::USE != 0,
        throwing_grenade: world.use_throwing_grenade(id),
        melee_pressed: buttons_now & buttons::MELEE_CHARGE != 0,
        weapon_ready,
        use_time: object.use_time_ms,
        objective_scaler: world.use_objective_scaler(id),
    }
}

fn buttons_for(cmds: &[(u32, u32)], id: ClientId) -> u32 {
    cmds.iter()
        .find(|(c, _)| *c == id.0)
        .map(|(_, b)| *b)
        .unwrap_or(0)
}

fn finish_cancel(world: &mut FrameWorld, session: UseHoldSession, reason: UseCancelReason) {
    let team = pers_team(world, session.client);
    let object = world.use_object(session.object).cloned();
    world.finish_use_hold(session.loop_state.cur_progress, false);
    if let Some(object) = object {
        emit_script_calls(
            world,
            session.object,
            object.callback_kind,
            use_type_after_hold_calls(
                object.notify_slots,
                object.use_time_ms,
                team,
                session.client.0,
                false,
            ),
        );
    }
    world.push_use_event(UseObjectEvent::Cancelled {
        object: session.object,
        client: session.client.0,
        reason,
    });
}

fn advance_use_hold(world: &mut FrameWorld, cmds: &[(u32, u32)]) {
    let Some(session) = world.use_hold() else {
        return;
    };
    let Some(ps) = world.player(session.client).copied() else {
        finish_cancel(world, session, UseCancelReason::Dead);
        return;
    };
    let Some(object) = world.use_object(session.object).cloned() else {
        world.clear_use_hold();
        return;
    };
    let bits = buttons_for(cmds, session.client);
    let input = loop_input(world, session.client, &ps, &object, bits);
    match use_hold_loop_tick(session.loop_state, &input) {
        UseHoldLoopTick::Continue(next) => {
            world.set_use_hold_state(next);
            world.push_use_event(UseObjectEvent::Progress {
                object: session.object,
                client: session.client.0,
                cur_progress: next.cur_progress,
                use_time: object.use_time_ms,
            });
        }
        UseHoldLoopTick::Completed(done) => {
            world.finish_use_hold(done.cur_progress, false);
            emit_script_calls(
                world,
                session.object,
                object.callback_kind,
                use_type_after_hold_calls(
                    object.notify_slots,
                    object.use_time_ms,
                    pers_team(world, session.client),
                    session.client.0,
                    true,
                ),
            );
            world.push_use_event(UseObjectEvent::Completed {
                object: session.object,
                client: session.client.0,
            });
        }
        UseHoldLoopTick::Cancelled => {
            let reason = cancel_reason(&input, &session.loop_state);
            finish_cancel(world, session, reason);
        }
    }
}

fn begin_use_from_presses(world: &mut FrameWorld, presses: &[UsePress], cmds: &[(u32, u32)]) {
    if world.use_hold().is_some() {
        return;
    }
    for press in presses {
        if !press.edge {
            continue;
        }
        let id = ClientId(press.client);
        let Some(ps) = world.player(id).copied() else {
            continue;
        };
        let Some(object_id) = world.first_eligible_use_object(id, &ps) else {
            continue;
        };
        let Some(object) = world.use_object(object_id).cloned() else {
            continue;
        };
        if object.use_time_ms <= 0 {
            emit_script_calls(
                world,
                object_id,
                object.callback_kind,
                use_type_after_hold_calls(
                    object.notify_slots,
                    object.use_time_ms,
                    pers_team(world, id),
                    id.0,
                    true,
                ),
            );
            world.push_use_event(UseObjectEvent::Completed {
                object: object_id,
                client: id.0,
            });
            continue;
        }
        world.begin_use_hold(object_id, id);
        emit_script_calls(
            world,
            object_id,
            object.callback_kind,
            use_type_begin_calls(object.notify_slots, object.use_time_ms, id.0),
        );
        world.push_use_event(UseObjectEvent::Began {
            object: object_id,
            client: id.0,
        });
        let session = world.use_hold().expect("begin_use_hold");
        let bits = buttons_for(cmds, id);
        let input = loop_input(world, id, &ps, &object, bits);
        match use_hold_loop_tick(session.loop_state, &input) {
            UseHoldLoopTick::Continue(next) => {
                world.set_use_hold_state(next);
                world.push_use_event(UseObjectEvent::Progress {
                    object: object_id,
                    client: id.0,
                    cur_progress: next.cur_progress,
                    use_time: object.use_time_ms,
                });
            }
            UseHoldLoopTick::Completed(done) => {
                world.finish_use_hold(done.cur_progress, false);
                emit_script_calls(
                    world,
                    object_id,
                    object.callback_kind,
                    use_type_after_hold_calls(
                        object.notify_slots,
                        object.use_time_ms,
                        pers_team(world, id),
                        id.0,
                        true,
                    ),
                );
                world.push_use_event(UseObjectEvent::Completed {
                    object: object_id,
                    client: id.0,
                });
            }
            UseHoldLoopTick::Cancelled => {
                let reason = cancel_reason(&input, &session.loop_state);
                finish_cancel(world, session, reason);
            }
        }
        break;
    }
}

fn touching_counts(touching: &[(ClientId, ProxClaimTeam, i32)]) -> (i32, i32) {
    let mut axis = 0i32;
    let mut allies = 0i32;
    for (_, team, _) in touching {
        match team {
            ProxClaimTeam::Axis => axis += 1,
            ProxClaimTeam::Allies => allies += 1,
            ProxClaimTeam::None => {}
        }
    }
    (axis, allies)
}

fn sync_proximity(world: &mut FrameWorld) {
    let now = world.entity_kernel().level_time_ms();
    let ids: Vec<u32> = world
        .use_objects()
        .iter()
        .filter(|object| object.kind == UseTriggerKind::Proximity)
        .map(|object| object.id)
        .collect();
    for object_id in ids {
        let Some(object) = world.use_object(object_id).cloned() else {
            continue;
        };
        let mut next_touch = Vec::new();
        for client in world.client_ids_sorted() {
            let Some(ps) = world.player(client) else {
                continue;
            };
            if !is_really_alive(world, client, ps) || !is_touching(ps, &object) {
                continue;
            }
            let Some(team) = ProxClaimTeam::from_pers(pers_team(world, client)) else {
                continue;
            };
            let start = object
                .touching
                .iter()
                .find(|(id, _, _)| *id == client)
                .map(|(_, _, start)| *start)
                .unwrap_or(now);
            next_touch.push((client, team, start));
        }
        let (n_axis, n_allies) = touching_counts(&next_touch);
        let scalers: Vec<f32> = next_touch
            .iter()
            .filter(|(_, team, _)| *team == object.claim)
            .map(|(id, _, _)| world.use_objective_scaler(*id))
            .collect();
        let use_rate = update_use_rate(object.claim, n_axis, n_allies, &scalers);
        world.set_use_touching(object_id, next_touch, use_rate);

        let object = world.use_object(object_id).cloned().expect("prox object");
        if object.claim == ProxClaimTeam::None {
            if let Some((client, team, _)) =
                object.touching.iter().copied().find(|(client, _, _)| {
                    can_interact_with(
                        object.interact_team,
                        object.owner_team,
                        pers_team(world, *client),
                    )
                })
            {
                let reset = set_claim_team_resets_progress(
                    object.claim,
                    object.last_claim,
                    object.last_claim_time_ms,
                    now,
                    team,
                );
                world.set_use_claim(object_id, team, Some(client), now, reset);
                world.push_use_event(UseObjectEvent::Claimed {
                    object: object_id,
                    client: client.0,
                    team: team.as_team().map(|t| t as u8).unwrap_or(0),
                });
                emit_script_calls(
                    world,
                    object_id,
                    object.callback_kind,
                    prox_begin_calls(object.notify_slots, object.use_time_ms, client.0),
                );
            }
        }

        let object = world.use_object(object_id).cloned().expect("prox object");
        let (n_axis, n_allies) = touching_counts(&object.touching);
        let touching_claim = match object.claim {
            ProxClaimTeam::None => 0,
            ProxClaimTeam::Axis => n_axis,
            ProxClaimTeam::Allies => n_allies,
        };
        let scalers: Vec<f32> = object
            .touching
            .iter()
            .filter(|(_, team, _)| *team == object.claim)
            .map(|(id, _, _)| world.use_objective_scaler(*id))
            .collect();
        let use_rate = update_use_rate(object.claim, n_axis, n_allies, &scalers);
        world.set_use_rate(object_id, use_rate);
        if object.claim != ProxClaimTeam::None && use_rate == 0.0 && touching_claim != 0 {
            world.push_use_event(UseObjectEvent::Contested {
                object: object_id,
                cur_progress: object.cur_progress,
            });
        }
        let outcome = use_object_prox_think_body(&ProxThinkInput {
            use_time: object.use_time_ms,
            cur_progress: object.cur_progress,
            use_rate,
            claim: object.claim,
            touching_claim,
        });
        match outcome {
            ProxThinkOutcome::Idle => {}
            ProxThinkOutcome::Progress {
                cur_progress,
                use_rate: _,
            } => {
                world.set_use_progress(object_id, cur_progress);
                let client = object.claim_player.map(|c| c.0).unwrap_or(0);
                world.push_use_event(UseObjectEvent::Progress {
                    object: object_id,
                    client,
                    cur_progress,
                    use_time: object.use_time_ms,
                });
                if let Some(team) = object.claim.as_team() {
                    emit_script_calls(
                        world,
                        object_id,
                        object.callback_kind,
                        [prox_use_update_call(
                            object.notify_slots,
                            team,
                            cur_progress,
                            object.use_time_ms,
                            use_rate,
                        )],
                    );
                }
            }
            ProxThinkOutcome::Unclaimed => {
                let team = object.claim.as_team().map(|t| t as u8).unwrap_or(0);
                if let Some(claim_team) = object.claim.as_team() {
                    emit_script_calls(
                        world,
                        object_id,
                        object.callback_kind,
                        prox_unclaim_calls(
                            object.notify_slots,
                            claim_team,
                            object.claim_player.map(|c| c.0),
                        ),
                    );
                }
                world.set_use_claim(object_id, ProxClaimTeam::None, None, now, false);
                world.push_use_event(UseObjectEvent::Unclaimed {
                    object: object_id,
                    team,
                });
            }
            ProxThinkOutcome::Completed => {
                let credit = object
                    .claim_player
                    .filter(|id| {
                        world
                            .player(*id)
                            .is_some_and(|ps| is_really_alive(world, *id, ps))
                    })
                    .map(|id| id.0);
                let client = credit.unwrap_or(0);
                if object.use_time_ms == 0 {
                    emit_script_calls(
                        world,
                        object_id,
                        object.callback_kind,
                        prox_instant_use_calls(object.notify_slots, credit),
                    );
                } else if let Some(claim_team) = object.claim.as_team() {
                    emit_script_calls(
                        world,
                        object_id,
                        object.callback_kind,
                        prox_complete_calls(object.notify_slots, claim_team, credit),
                    );
                }
                world.set_use_progress(object_id, 0);
                world.set_use_claim(object_id, ProxClaimTeam::None, None, now, false);
                world.push_use_event(UseObjectEvent::Completed {
                    object: object_id,
                    client,
                });
            }
        }
    }
}

pub fn bind_map_use_object(
    classname: &str,
    origin: [f32; 3],
    angles: [f32; 3],
    radius: Option<f32>,
    height: Option<f32>,
    box_mid: Option<[f32; 3]>,
    box_half: Option<[f32; 3]>,
    bound_entnum: Option<i32>,
    use_time_seconds: f32,
    script_label: &str,
) -> Result<UseObjectInstall, MapUseBindError> {
    let kind = UseTriggerKind::from(create_use_trigger_kind(classname));
    let (mins, maxs) = if classname == TRIGGER_RADIUS {
        trigger_radius_world_aabb(origin, radius, height)?
    } else {
        let Some(mid) = box_mid else {
            return Err(MapUseBindError::MissingBrushBox);
        };
        let Some(half) = box_half else {
            return Err(MapUseBindError::MissingBrushBox);
        };
        if half == [0.0, 0.0, 0.0] {
            return Err(MapUseBindError::MissingBrushBox);
        }
        world_aabb_from_r_box(origin, angles, mid, half)
    };
    let (owner_team, interact_team) = match kind {
        UseTriggerKind::Use => (GameObjectTeam::Neutral, InteractTeam::Any),
        UseTriggerKind::Proximity => (GameObjectTeam::Neutral, InteractTeam::Enemy),
    };
    Ok(UseObjectInstall {
        mins,
        maxs,
        cylinder: if classname == TRIGGER_RADIUS {
            Some([
                radius.ok_or(MapUseBindError::MissingRadius)?,
                height.ok_or(MapUseBindError::MissingHeight)?,
            ])
        } else {
            None
        },
        use_time_ms: set_use_time_ms(use_time_seconds),
        use_weapon: None,
        kind,
        owner_team,
        interact_team,
        bound_entnum,
        notify_slots: UseNotifySlots {
            on_begin_use: false,
            on_end_use: false,
            on_use: false,
            on_use_update: false,
        },
        callback_kind: UseCallbackKind::Unbound,
        script_label: gamemode_iw4::get_label(script_label),
        script_origin: origin,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomFlagInstallError {
    Bootstrap(gamemode_iw4::DomFlagBootstrapError),
    Bind(MapUseBindError),
}

pub fn install_dom_flags(
    world: &mut SimState,
    ents: &[gamemode_iw4::DomFlagMapEnt<'_>],
) -> Result<Vec<u32>, DomFlagInstallError> {
    let mut order = [0usize; gamemode_iw4::MAX_DOM_FLAGS];
    let n = gamemode_iw4::collect_dom_flag_indices(ents, &mut order)
        .map_err(DomFlagInstallError::Bootstrap)?;
    let mut ids = Vec::with_capacity(n);
    for index in order.iter().take(n) {
        let ent = &ents[*index];
        let spec = bind_map_use_object(
            ent.classname,
            ent.origin,
            ent.angles,
            ent.radius,
            ent.height,
            None,
            None,
            None,
            gamemode_iw4::DOM_FLAG_SET_USE_TIME_SECONDS,
            ent.script_label,
        )
        .map_err(DomFlagInstallError::Bind)?
        .with_dom_flag_callbacks();
        ids.push(world.install_use_object(spec));
    }
    Ok(ids)
}

fn volume_from_entnum(world: &FrameWorld, number: i32) -> Option<([f32; 3], [f32; 3])> {
    let mover = world.script_mover_by_number(number)?;
    Some(world_aabb_from_r_box(
        mover.state.tr_base,
        mover.state.apos_tr_base,
        mover.box_mid,
        mover.box_half,
    ))
}

fn refresh_bound_volumes(world: &mut FrameWorld) {
    let jobs: Vec<(u32, i32)> = world
        .use_objects()
        .iter()
        .filter_map(|object| object.bound_entnum.map(|number| (object.id, number)))
        .collect();
    for (id, number) in jobs {
        if let Some((mins, maxs)) = volume_from_entnum(world, number) {
            world.set_use_volume(id, mins, maxs);
        }
    }
}

pub(crate) fn phase_use_objects(
    world: &mut FrameWorld,
    tick: Tick,
    msec: u32,
    presses: &[UsePress],
    cmds: &[(u32, u32)],
) {
    world.set_use_script_tick(tick);
    world.clear_use_events();
    if msec != USE_HOLD_TICK_MS as u32 {
        if world.use_hold().is_some() || !world.use_objects().is_empty() {
            panic!("useHoldThinkLoop wait is 0.05s; this step is not 50 ms");
        }
        return;
    }
    refresh_bound_volumes(world);
    advance_use_hold(world, cmds);
    begin_use_from_presses(world, presses, cmds);
    sync_proximity(world);
    phase_update_dom_scores(world);
}

fn phase_update_dom_scores(world: &mut FrameWorld) {
    let now = world.entity_kernel().level_time_ms();
    if !world.take_dom_score_due(now) {
        return;
    }
    let mut flags: Vec<OwnedDomFlag> = world
        .use_objects()
        .iter()
        .filter(|row| {
            row.callback_kind == UseCallbackKind::DomFlag
                && is_owned_dom_flag(row.owner_team, row.capture_time_ms)
        })
        .filter_map(|row| {
            Some(OwnedDomFlag {
                object: row.id,
                owner: row.owner_team,
                capture_time_ms: row.capture_time_ms?,
            })
        })
        .collect();
    sort_owned_oldest_first(&mut flags, now);
    for flag in flags {
        let Some(team) = scoring_team(flag.owner) else {
            continue;
        };
        let total = world.grant_dom_objective_point(team, now);
        crate::score::evaluate_team_score_limit_soon(world, total);
        world.push_use_event(UseObjectEvent::DomTeamScore {
            team: match team {
                gamemode_iw4::ScoringTeam::Axis => Team::Axis as u8,
                gamemode_iw4::ScoringTeam::Allies => Team::Allies as u8,
            },
            added: DOM_FLAG_SCORE_POINTS,
            total,
        });
    }
}
