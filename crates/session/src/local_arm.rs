use bevy::prelude::*;
use frame::{AppScreen, ClassEquipResolved, HasWorld, LaunchReport};
use net::{
    AuthorityInputGate, AuthorityLoadHold, ClientActionInbox, ClientPredictionState, ClientSet,
    LocalPresentClient, LookState, PresentedSnapshot, RuntimeRole, SignonState,
    look_angles_from_degrees,
};
use sim::ClientAction;

use render_frontend::adapters::anim::fpv_present::LocalSpawnArmed;
use render_frontend::prepare::scene::camera::SimCamera;

pub fn arm_local_from_presented(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    has_world: Option<Res<HasWorld>>,
    signon: Res<SignonState>,
    mut armed: ResMut<LocalSpawnArmed>,
    mut screen: ResMut<AppScreen>,
    mut sim_cam: ResMut<SimCamera>,
    mut look: ResMut<LookState>,
    mut input_gate: ResMut<AuthorityInputGate>,
    role: Option<Res<RuntimeRole>>,
) {
    if armed.0 {
        return;
    }

    if !has_world.is_some_and(|world| world.0)
        || (role
            .as_ref()
            .is_some_and(|role| **role != RuntimeRole::Replay)
            && !signon.may_select_class())
    {
        return;
    }
    let Some(ps) = presented.alive_player(local.0) else {
        return;
    };

    look.angles = look_angles_from_degrees(std::array::from_fn(|axis| {
        ps.viewangles[axis] - ps.delta_angles[axis]
    }));
    sim_cam.enabled = true;
    sim_cam.freeze_fly = false;

    if !role.is_some_and(|role| *role == RuntimeRole::Replay) {
        input_gate.local_cmds_enabled = true;
    }
    *screen = AppScreen::InGame;

    diag::info!(
        Fpv,
        "spawn: presented Alive at [{:.1}, {:.1}, {:.1}] yaw={:.0} weapon={}",
        ps.origin[0],
        ps.origin[1],
        ps.origin[2],
        ps.viewangles[1],
        ps.weapon
    );
    armed.0 = true;
}

pub fn join_local_on_class_select(
    screen: Res<AppScreen>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut actions: ResMut<ClientActionInbox>,
    mut request_ids: ResMut<net::ActionRequestIds>,
    role: Option<Res<RuntimeRole>>,
    hold: Option<Res<AuthorityLoadHold>>,
    signon: Option<Res<SignonState>>,
    mut joining: Local<Option<sim::ActionRequestId>>,
) {
    if !matches!(*screen, AppScreen::ClassSelect) {
        *joining = None;
        return;
    }
    if role.is_some_and(|role| *role == RuntimeRole::Replay) {
        return;
    }
    if signon.is_some_and(|s| !s.may_select_class()) {
        return;
    }

    if hold.is_some_and(|h| h.0) {
        return;
    }
    if presented
        .snapshot()
        .and_then(|s| s.meta.for_client(local.0))
        .is_some()
    {
        *joining = None;
        return;
    }

    if let Some(request_id) = *joining {
        if actions
            .pending_for(local.0)
            .any(|action| sim::action_request_id(&action) == request_id)
        {
            return;
        }

        *joining = None;
    }
    let request_id = request_ids.allocate();
    match actions.push(local.0, ClientAction::JoinMatch { request_id }) {
        Ok(()) => {
            *joining = Some(request_id);
            diag::info!(
                Fpv,
                "class select: queued JoinMatch request_id={request_id} \
                 (meta-only; spectator pm_type is a named gap)"
            );
        }
        Err(error) => diag::warn!(Fpv, "class select: JoinMatch not queued — {error}"),
    }
}

pub fn sync_prediction_metrics_to_probe(
    prediction: Res<ClientPredictionState>,
    mut report: Option<ResMut<LaunchReport>>,
) {
    if !prediction.is_changed() {
        return;
    }
    let Some(report) = report.as_deref_mut() else {
        return;
    };
    report.prediction_metrics = Some(prediction.0.metrics().report_line());
}

pub fn register_local_arm_systems(app: &mut App) {
    app.add_systems(
        Update,
        (
            arm_local_from_presented.after(ClassEquipResolved),
            join_local_on_class_select,
        )
            .in_set(ClientSet::Present),
    )
    .add_systems(
        Update,
        sync_prediction_metrics_to_probe.in_set(ClientSet::Diag),
    );
}
