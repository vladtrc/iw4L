use bevy::prelude::Node;
use bevy::ui::Display;

pub(crate) fn adopt_display(node: &mut Node, want: Display) {
    if node.display != want {
        node.display = want;
    }
}
