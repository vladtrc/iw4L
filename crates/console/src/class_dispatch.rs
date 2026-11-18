use crate::plugin::{Hold, WaitKind};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use net::{SignonPhase, SignonState};
use ui::{
    ClassChangeAllowed, ClassChangeBlockReason, ClassSelectHighlight, ClassSelectPhase,
    ClassSelectStatus, PendingClassEquip, SessionClassStore, class_index_by_name,
    commit_class_equip,
};

use crate::feature_dispatch::ConsoleEcho;
use crate::{ConsoleCommand, ConsoleDispatch};

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

#[derive(SystemParam)]
pub(crate) struct ClassSelection<'w> {
    store: ResMut<'w, SessionClassStore>,
    highlight: ResMut<'w, ClassSelectHighlight>,
    phase: ResMut<'w, ClassSelectPhase>,
    pending: ResMut<'w, PendingClassEquip>,
    status: ResMut<'w, ClassSelectStatus>,
}

impl ClassSelection<'_> {
    fn commit_spawn(
        &mut self,
        class: Option<&str>,
        background: bool,
        dispatch: &mut ConsoleDispatch,
        echo: &mut ConsoleEcho<'_>,
    ) {
        let index = match class {
            None => self.highlight.0,
            Some(arg) => match class_index_by_name(&self.store, arg).or_else(|| arg.parse().ok()) {
                Some(index) => index,
                None => {
                    echo.write(format!("spawn: unknown_class `{arg}`"));
                    return;
                }
            },
        };
        match commit_class_equip(
            index,
            &mut self.store,
            &mut self.highlight,
            &mut self.phase,
            &mut self.pending,
            &mut self.status,
        ) {
            Ok(()) => {
                if !background {
                    dispatch.wait(WaitKind::Spawn);
                    echo.write(format!(
                        "spawn: equip {} -> Pending (sync until InGame)",
                        describe_slot(&self.store, index)
                    ));
                } else {
                    echo.write(format!(
                        "spawn &: equip {} -> Pending (async)",
                        describe_slot(&self.store, index)
                    ));
                }
            }
            Err(refusal) => echo.write(format!("spawn: {refusal}")),
        }
    }
}

pub(crate) fn route_class_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    mut dispatch: ResMut<ConsoleDispatch>,
    mut selection: ClassSelection,
    change_allowed: Res<ClassChangeAllowed>,
    block_reason: Res<ClassChangeBlockReason>,
) {
    for cmd in events.read() {
        match cmd.name.as_str() {
            "class" => {
                let Some(arg) = cmd.args.first() else {
                    for msg in class_listing(&selection.store, selection.highlight.0) {
                        echo.write(msg);
                    }
                    continue;
                };
                let Some(index) = class_index_by_name(&selection.store, arg) else {
                    echo.write(format!(
                        "class: unknown_class `{arg}` — `class` lists the presets"
                    ));
                    continue;
                };
                if selection.phase.is_pending() {
                    echo.write("class: an Equip is already pending — wait for authority");
                    continue;
                }
                selection.highlight.0 = index;
                selection.store.selected = index;
                echo.write(format!(
                    "class: selected {}",
                    describe_slot(&selection.store, index)
                ));
            }
            "spawn" => {
                match spawn_gate(
                    change_allowed.0 && !selection.store.slots.is_empty(),
                    cmd.background,
                ) {
                    SpawnGate::RefuseNow => {
                        let reason = block_reason
                            .0
                            .clone()
                            .unwrap_or_else(|| "class change not allowed".to_owned());
                        echo.write(format!("spawn: {reason}"));
                    }
                    SpawnGate::WaitForAdmission => {
                        dispatch.wait(WaitKind::SpawnAdmission(cmd.args.first().cloned()));
                        echo.write("spawn: waiting for class select");
                    }
                    SpawnGate::EquipNow => selection.commit_spawn(
                        cmd.args.first().map(String::as_str),
                        cmd.background,
                        &mut dispatch,
                        &mut echo,
                    ),
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn complete_pending_spawn(
    mut echo: ConsoleEcho,
    mut dispatch: ResMut<ConsoleDispatch>,
    mut selection: ClassSelection,
    change_allowed: Res<ClassChangeAllowed>,
    signon: Option<Res<SignonState>>,
) {
    if !matches!(
        dispatch.hold.as_ref().map(|hold| &hold.until),
        Some(WaitKind::SpawnAdmission(_))
    ) {
        return;
    }
    if signon.as_ref().is_some_and(|signon| {
        signon.phase.is_failed() || matches!(signon.phase, SignonPhase::Cancelled)
    }) {
        let reason = signon
            .as_ref()
            .and_then(|signon| signon.phase.status_line())
            .unwrap_or_else(|| "session failed".to_owned());
        dispatch.release();
        echo.write(format!("spawn: aborted — {reason}"));
        return;
    }
    if !change_allowed.0 || selection.store.slots.is_empty() {
        return;
    }
    let Some(Hold {
        until: WaitKind::SpawnAdmission(class),
        ..
    }) = dispatch.hold.take()
    else {
        return;
    };
    selection.commit_spawn(class.as_deref(), false, &mut dispatch, &mut echo);
}

fn describe_slot(store: &SessionClassStore, index: usize) -> String {
    let Some(slot) = store.slots.get(index) else {
        return format!("{index} (missing)");
    };
    let lock = match store.lock_reason(index) {
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
        "class: {} presets, selected {highlight}",
        store.slots.len()
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
