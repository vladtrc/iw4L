use bevy::prelude::*;
use net::AuthorityWorld;

use crate::ConsoleCommand;

use super::echo::ConsoleEcho;

pub(crate) fn route_hitvol_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    authority: Option<Res<AuthorityWorld>>,
) {
    for cmd in events.read() {
        if cmd.name != "hitvol" {
            continue;
        }
        let Some(authority) = authority.as_deref() else {
            echo.write("hitvol: no authority world on this client");
            continue;
        };
        for line in hitvol_report(&authority.0) {
            echo.write(line);
        }
    }
}

pub(super) fn hitvol_report(world: &sim::SimWorld) -> Vec<String> {
    let mut out = Vec::new();
    for row in world.hitvol_dump() {
        out.push(format!(
            "hitvol client {:?}: geom {} bones {} pose {} body `{}` head `{}` controller {}",
            row.client.map(|c| c.0),
            row.geom,
            row.bone_count,
            row.pose_kind,
            row.body_key,
            row.head_key,
            row.controller
        ));
    }
    out
}
