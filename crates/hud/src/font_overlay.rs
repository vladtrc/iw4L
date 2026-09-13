use bevy::prelude::*;
use bevy::ui::{Display, FocusPolicy};

use crate::gpu_list::GpuListLatch;

pub(crate) const HUD_SMALL_FONT: &str = "fonts/hudsmallfont";

pub(crate) fn spawn_overlay(root: &mut ChildSpawnerCommands, marker: impl Bundle) {
    root.spawn((
        marker,
        GpuListLatch::default(),
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        FocusPolicy::Pass,
    ));
}
