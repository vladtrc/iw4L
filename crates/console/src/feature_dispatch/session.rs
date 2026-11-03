use bevy::prelude::*;

use crate::{ConsoleCommand, ConsoleDispatch};

use super::echo::ConsoleEcho;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct SessionCommandTarget<'w> {
    scope: Res<'w, State<frame::MatchScope>>,
    playback: Option<Res<'w, ::replay::ReplayPlayback>>,
    bridge: Option<Res<'w, net::MasterBridge>>,
    manifest: Option<Res<'w, ::session::SessionContentManifest>>,
}

impl SessionCommandTarget<'_> {
    fn restart_zone(&self) -> Option<String> {
        self.manifest
            .as_ref()
            .and_then(|manifest| match &manifest.map {
                ::session::ManifestFact::Known(map)
                    if (*self.scope.get() == frame::MatchScope::Live) =>
                {
                    Some(format!("{}:{}", map.namespace.as_str(), map.name))
                }
                _ => None,
            })
    }

    fn can_leave(&self, transition: &::session::SessionSwapRequest) -> bool {
        (*self.scope.get() == frame::MatchScope::Live)
            || self.playback.is_some()
            || transition.is_pending()
            || self.bridge.is_some()
    }
}

pub(crate) fn route_session_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    mut transition: ResMut<::session::SessionSwapRequest>,
    target: SessionCommandTarget,
    mut dispatch: ResMut<ConsoleDispatch>,
    mut authority: Option<ResMut<net::AuthorityWorld>>,
) {
    for cmd in events.read() {
        match cmd.name.as_str() {
            "round_restart" => {
                let result = if !cmd.args.is_empty() {
                    Err("usage: round_restart".into())
                } else if let Some(authority) = authority.as_deref_mut() {
                    authority.0.request_round_restart()
                } else {
                    Err("round restart needs the match host".into())
                };
                match result {
                    Ok(()) => echo.write("round_restart: requested script round restart"),
                    Err(error) => echo.write(format!("round_restart: {error}")),
                }
            }
            "map_restart" => {
                let zone = target.restart_zone();
                match (cmd.args.is_empty(), zone) {
                    (false, _) => {
                        dispatch.release();
                        echo.write("usage: map_restart");
                    }
                    (true, None) => {
                        dispatch.release();
                        echo.write("map_restart: no map to restart");
                    }
                    (true, Some(zone)) => match transition.request_zone(zone.clone()) {
                        Ok(()) => echo.write(format!("map_restart: requested `{zone}`")),
                        Err(error) => {
                            dispatch.release();
                            echo.write(format!("map_restart: {error}"));
                        }
                    },
                }
            }
            "map" => match cmd.args.as_slice() {
                [zone] => match transition.request_zone(zone.clone()) {
                    Ok(()) => echo.write(format!("map: requested `{zone}`")),
                    Err(error) => {
                        dispatch.release();
                        echo.write(format!("map: {error}"));
                    }
                },
                _ => {
                    dispatch.release();
                    echo.write("usage: map <zone>");
                }
            },
            "disconnect" => {
                let in_session = target.can_leave(&transition);
                if !cmd.args.is_empty() {
                    dispatch.release();
                    echo.write("usage: disconnect");
                } else if !in_session {
                    dispatch.release();
                    echo.write("disconnect: no session to leave");
                } else {
                    match transition.request_leave() {
                        Ok(()) => {
                            diag::lifecycle_boundary("disconnect_requested", "");
                            echo.write("disconnect: waiting for session teardown")
                        }
                        Err(error) => {
                            dispatch.release();
                            echo.write(format!("disconnect: {error}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
