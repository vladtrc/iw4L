use bevy::prelude::*;
use sim::ClientAction;

use crate::{ConsoleCommand, ConsoleRegistry};

#[derive(Resource, Default)]
pub(crate) struct SavedPosition {
    saved: Option<([f32; 3], [f32; 3])>,
    on_spawn: bool,
}

pub(crate) fn register_saved_position_commands(registry: &mut ConsoleRegistry) {
    for (name, usage) in [
        ("save", "save — remember the player's position and view"),
        (
            "load",
            "load — move the player to the saved position (needs cheats)",
        ),
        (
            "loadspawn",
            "loadspawn [on|off] — move the player to the saved position on every respawn (needs cheats)",
        ),
    ] {
        if registry.resolve(name).is_none() {
            registry.register(crate::CommandSpec::new(name).usage(usage));
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct SavedPositionLoader<'w> {
    authority: Option<Res<'w, net::AuthorityWorld>>,
    actions: crate::client_actions::ClientActionTarget<'w>,
}

impl SavedPositionLoader<'_> {
    fn load(
        &mut self,
        command: &str,
        saved: Option<([f32; 3], [f32; 3])>,
        client: sim::ClientId,
        echo: &mut crate::feature_dispatch::ConsoleEcho,
    ) {
        let Some((origin, angles)) = saved else {
            echo.write(format!("{command}: nothing saved — use save first"));
            return;
        };
        if self
            .authority
            .as_ref()
            .is_some_and(|a| !a.0.cheats_enabled())
        {
            echo.write(format!("{command}: cheats are off"));
            return;
        }
        let Some(mut inbox) = self.actions.queue() else {
            echo.write(format!("{command}: no action inbox (not a listen host)"));
            return;
        };
        if let Err(error) = inbox.push(client, ClientAction::Move { origin, angles }) {
            echo.write(format!("{command}: {error}"));
            return;
        }
        echo.write(format!(
            "{command}: moved to {:.1} {:.1} {:.1} yaw={:.0} pitch={:.0}",
            origin[0], origin[1], origin[2], angles[1], angles[0]
        ));
    }
}

pub(crate) fn route_saved_position_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut lives: MessageReader<frame::LifeStarted>,
    mut echo: crate::feature_dispatch::ConsoleEcho,
    mut position: ResMut<SavedPosition>,
    presentation: net::LocalPresentation,
    mut loader: SavedPositionLoader,
) {
    for cmd in events.read() {
        match cmd.name.as_str() {
            "save" => match presentation.alive_player() {
                Some(ps) => {
                    position.saved = Some((ps.origin, ps.viewangles));
                    echo.write(format!(
                        "save: {:.1} {:.1} {:.1} yaw={:.0} pitch={:.0}",
                        ps.origin[0],
                        ps.origin[1],
                        ps.origin[2],
                        ps.viewangles[1],
                        ps.viewangles[0]
                    ));
                }
                None => echo.write("save: not Alive — spawn a class first"),
            },
            "load" => {
                if presentation.alive_player().is_none() {
                    echo.write("load: not Alive — spawn a class first");
                    continue;
                }
                loader.load("load", position.saved, presentation.client_id(), &mut echo);
            }
            "loadspawn" => {
                let on = match cmd.args.first().map(String::as_str) {
                    None => !position.on_spawn,
                    Some("on" | "1") => true,
                    Some("off" | "0") => false,
                    Some(other) => {
                        echo.write(format!("usage: loadspawn [on|off] (got `{other}`)"));
                        continue;
                    }
                };
                position.on_spawn = on;
                echo.write(match (on, position.saved.is_some()) {
                    (true, true) => "loadspawn on: respawns move to the saved position",
                    (true, false) => "loadspawn on: nothing saved yet — use save",
                    (false, _) => "loadspawn off",
                });
            }
            _ => {}
        }
    }

    let respawned = lives
        .read()
        .any(|life| life.client == presentation.client_id().0);
    if respawned && position.on_spawn && position.saved.is_some() {
        loader.load(
            "loadspawn",
            position.saved,
            presentation.client_id(),
            &mut echo,
        );
    }
}
