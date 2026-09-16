use bevy::prelude::*;
use frame::AppScreen;
use net::{ClientSet, NetDiagnostics};
use replay::{ReplayDiagnostics, ReplayPlayback};
use session::{PendingConsoleLines, StartupCommands};

use crate::debug_move::update_showpos_overlay;
use crate::feature_dispatch::DebugPosOverlay;
use crate::plugin::ConsoleCommandQueue;
use crate::{ConsoleLine, ConsoleQueue};

fn mirror_runtime_diagnostics(
    net: Option<Res<NetDiagnostics>>,
    replay: Option<Res<ReplayDiagnostics>>,
    mut line: ResMut<ConsoleLine>,
) {
    if let Some(net) = net
        && net.is_changed()
        && let Some(msg) = net.line.as_ref()
    {
        line.0 = msg.clone();
    }
    if let Some(replay) = replay
        && replay.is_changed()
        && let Some(msg) = replay.line.as_ref()
    {
        line.0 = msg.clone();
    }
}

fn drain_pending_console_lines(
    mut pending: ResMut<PendingConsoleLines>,
    mut startup: Option<ResMut<StartupCommands>>,
    screen: Option<Res<AppScreen>>,
    mut queue: ResMut<ConsoleQueue>,
) {
    if let Some(startup) = startup.as_mut()
        && !startup.lines.is_empty()
        && screen
            .as_ref()
            .is_some_and(|s| matches!(**s, AppScreen::MainMenu))
    {
        pending.0.extend(startup.lines.drain(..));
    }
    for line in pending.0.drain(..) {
        queue.push_line(&line);
    }
}

fn finish_replay_playback(
    mut playback: Option<ResMut<ReplayPlayback>>,
    mut queue: ResMut<ConsoleCommandQueue>,
    mut swap: Option<ResMut<session::SessionSwapRequest>>,
) {
    let Some(playback) = playback.as_mut() else {
        return;
    };
    if !playback.ended || playback.exit_queued {
        return;
    }
    playback.exit_queued = true;
    if playback.quit_on_end {
        queue.push_script("quit");
        diag::info!(
            Console,
            "play: queued quit after {} ticks",
            playback.ticks()
        );
        return;
    }
    match swap.as_mut() {
        Some(swap) => match swap.request_menu() {
            Ok(id) => diag::info!(
                Console,
                "demo: ended after {} ticks — disconnect to menu (swap #{id})",
                playback.ticks()
            ),
            Err(error) => diag::info!(
                Console,
                "demo: ended after {} ticks — disconnect skipped: {error}",
                playback.ticks()
            ),
        },
        None => diag::warn!(
            Console,
            "demo: ended after {} ticks but SessionSwapRequest is missing",
            playback.ticks()
        ),
    }
}

pub(crate) fn register_diagnostics_mirror(app: &mut App) {
    app.init_resource::<DebugPosOverlay>();
    app.add_systems(
        Update,
        (
            mirror_runtime_diagnostics,
            drain_pending_console_lines,
            finish_replay_playback,
            update_showpos_overlay,
        )
            .in_set(ClientSet::Diag),
    );
}
