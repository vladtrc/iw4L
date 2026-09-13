use bevy::prelude::*;
use net::{SignonPhase, SignonState};
use ui::{
    ClassChangeAllowed, ClassChangeBlockReason, ClassEquipRefusal, ClassSelectHighlight,
    ClassSelectPhase, ClassSelectStatus, PendingClassEquip, SessionClassStore, class_index_by_name,
    commit_class_equip,
};

use crate::{ConsoleCommand, ConsoleDispatch, ConsoleLine, ConsoleSettings, ConsoleState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpawnGate {
    EquipNow,
    WaitForAdmission,
    RefuseNow,
}

pub(crate) fn spawn_gate(change_allowed: bool, background: bool) -> SpawnGate {
    match (change_allowed, background) {
        (true, _) => SpawnGate::EquipNow,
        (false, true) => SpawnGate::RefuseNow,
        (false, false) => SpawnGate::WaitForAdmission,
    }
}

pub(crate) fn route_class_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut dispatch: ResMut<ConsoleDispatch>,
    mut store: ResMut<SessionClassStore>,
    mut highlight: ResMut<ClassSelectHighlight>,
    mut phase: ResMut<ClassSelectPhase>,
    mut pending: ResMut<PendingClassEquip>,
    mut status: ResMut<ClassSelectStatus>,
    mut seq: ResMut<net::ActionRequestIds>,
    change_allowed: Res<ClassChangeAllowed>,
    block_reason: Res<ClassChangeBlockReason>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "class" => {
                let Some(arg) = cmd.args.first() else {
                    for msg in class_listing(&store, highlight.0) {
                        echo(msg, &mut console, &mut line);
                    }
                    continue;
                };
                let Some(index) = class_index_by_name(&store, arg) else {
                    echo(
                        format!("class: unknown_class `{arg}` — `class` lists the presets"),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                if phase.is_pending() {
                    echo(
                        "class: an Equip is already pending — wait for authority".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                highlight.0 = index;
                store.selected = index;
                echo(
                    format!("class: selected {}", describe_slot(&store, index)),
                    &mut console,
                    &mut line,
                );
            }

            "spawn" => {
                if let Some(arg) = cmd.args.first()
                    && class_index_by_name(&store, arg).is_none()
                {
                    echo(
                        format!("spawn: unknown_class `{arg}`"),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                match spawn_gate(change_allowed.0, cmd.background) {
                    SpawnGate::RefuseNow => {
                        let reason = block_reason
                            .0
                            .clone()
                            .unwrap_or_else(|| "class change not allowed".to_owned());
                        echo(format!("spawn: {reason}"), &mut console, &mut line);
                    }
                    SpawnGate::WaitForAdmission => {
                        dispatch.wait_spawn_admit = true;
                        dispatch.wait_spawn_elapsed = 0.0;
                        dispatch.pending_spawn_class = cmd.args.first().cloned();
                        echo(
                            "spawn: waiting for class select".into(),
                            &mut console,
                            &mut line,
                        );
                    }
                    SpawnGate::EquipNow => {
                        commit_spawn(
                            cmd.args.first().map(String::as_str),
                            cmd.background,
                            &mut store,
                            &mut highlight,
                            &mut phase,
                            &mut pending,
                            &mut status,
                            &mut seq,
                            &mut dispatch,
                            &mut console,
                            &mut line,
                            capacity,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn complete_pending_spawn(
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut dispatch: ResMut<ConsoleDispatch>,
    mut store: ResMut<SessionClassStore>,
    mut highlight: ResMut<ClassSelectHighlight>,
    mut phase: ResMut<ClassSelectPhase>,
    mut pending: ResMut<PendingClassEquip>,
    mut status: ResMut<ClassSelectStatus>,
    mut seq: ResMut<net::ActionRequestIds>,
    change_allowed: Res<ClassChangeAllowed>,
    signon: Option<Res<SignonState>>,
) {
    if !dispatch.wait_spawn_admit {
        return;
    }
    let capacity = settings.log_capacity;
    if signon.as_ref().is_some_and(|signon| {
        signon.phase.is_failed() || matches!(signon.phase, SignonPhase::Cancelled)
    }) {
        let reason = signon
            .as_ref()
            .and_then(|signon| signon.phase.status_line())
            .unwrap_or_else(|| "session failed".to_owned());
        dispatch.wait_spawn_admit = false;
        dispatch.pending_spawn_class = None;
        let msg = format!("spawn: aborted — {reason}");
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
        return;
    }
    if !change_allowed.0 {
        return;
    }
    let class = dispatch.pending_spawn_class.take();
    dispatch.wait_spawn_admit = false;
    commit_spawn(
        class.as_deref(),
        false,
        &mut store,
        &mut highlight,
        &mut phase,
        &mut pending,
        &mut status,
        &mut seq,
        &mut dispatch,
        &mut console,
        &mut line,
        capacity,
    );
}

fn commit_spawn(
    class: Option<&str>,
    background: bool,
    store: &mut SessionClassStore,
    highlight: &mut ClassSelectHighlight,
    phase: &mut ClassSelectPhase,
    pending: &mut PendingClassEquip,
    status: &mut ClassSelectStatus,
    seq: &mut net::ActionRequestIds,
    dispatch: &mut ConsoleDispatch,
    console: &mut ConsoleState,
    line: &mut ConsoleLine,
    capacity: usize,
) {
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };
    let index = match class {
        None => highlight.0,
        Some(arg) => match class_index_by_name(store, arg) {
            Some(index) => index,
            None => {
                echo(format!("spawn: unknown_class `{arg}`"), console, line);
                return;
            }
        },
    };
    match commit_class_equip(index, store, highlight, phase, pending, status, seq) {
        Ok(request_id) => {
            if !background {
                dispatch.wait_spawn = true;
                dispatch.wait_spawn_elapsed = 0.0;
                echo(
                    format!(
                        "spawn: equip {} request_id={request_id} -> Pending (sync until InGame)",
                        describe_slot(store, index)
                    ),
                    console,
                    line,
                );
            } else {
                echo(
                    format!(
                        "spawn &: equip {} request_id={request_id} -> Pending (async)",
                        describe_slot(store, index)
                    ),
                    console,
                    line,
                );
            }
        }
        Err(refusal @ ClassEquipRefusal::AlreadyPending { .. }) => {
            echo(format!("spawn: {refusal}"), console, line)
        }
        Err(refusal) => echo(format!("spawn: {refusal}"), console, line),
    }
}

fn describe_slot(store: &SessionClassStore, index: usize) -> String {
    let Some(slot) = store.slots.get(index) else {
        return format!("{index} (missing)");
    };
    let lock = match slot.lock_reason.as_deref() {
        Some(reason) => format!(" LOCKED — {reason}"),
        None => String::new(),
    };
    format!(
        "{index} `{}` primary={} secondary={}{lock}",
        slot.name, slot.primary, slot.secondary
    )
}

fn class_listing(store: &SessionClassStore, highlight: usize) -> Vec<String> {
    let mut lines = Vec::with_capacity(store.slots.len() + 1);
    lines.push(format!(
        "class: {} presets, selected {highlight}, equipped {:?}",
        store.slots.len(),
        store.equipped
    ));
    for index in 0..store.slots.len() {
        let marker = if index == highlight { '>' } else { ' ' };
        lines.push(format!("{marker} {}", describe_slot(store, index)));
    }
    lines
}

pub fn class_completions(store: &SessionClassStore) -> Vec<String> {
    store.slots.iter().map(|slot| slot.name.clone()).collect()
}
