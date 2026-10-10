use bevy::prelude::*;
use frame::HasWorld;

use crate::{ConsoleCommand, ConsoleDispatch};

use super::echo::ConsoleEcho;

pub(crate) fn route_session_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    mut transition: ResMut<::session::SessionSwapRequest>,
    has_world: Res<HasWorld>,
    role: Res<frame::RuntimeRole>,
    playback: Option<Res<::replay::ReplayPlayback>>,
    bridge: Option<Res<net::MasterBridge>>,
    manifest: Option<Res<::session::SessionContentManifest>>,
    mut dispatch: ResMut<ConsoleDispatch>,
) {
    for cmd in events.read() {
        match cmd.name.as_str() {
            "end_match" => {
                if !cmd.args.is_empty() {
                    echo.write("usage: end_match");
                } else if !has_world.0
                    || !matches!(
                        *role,
                        frame::RuntimeRole::Listen | frame::RuntimeRole::Dedicated
                    )
                {
                    echo.write("Only the match host can end the match");
                } else {
                    match transition.request_menu() {
                        Ok(id) => echo.write(format!(
                            "end_match: returning the session to its lobby (swap #{id})"
                        )),
                        Err(error) => echo.write(format!("end_match: {error}")),
                    }
                }
                dispatch.release();
            }
            "map_restart" => {
                let zone = manifest.as_ref().and_then(|manifest| match &manifest.map {
                    ::session::ManifestFact::Known(map) if has_world.0 => {
                        Some(format!("{}:{}", map.namespace.as_str(), map.name))
                    }
                    _ => None,
                });
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
                        Ok(id) => {
                            echo.write(format!("map_restart: requested `{zone}` (swap #{id})"))
                        }
                        Err(error) => {
                            dispatch.release();
                            echo.write(format!("map_restart: {error}"));
                        }
                    },
                }
            }
            "map" => match cmd.args.as_slice() {
                [zone] => match transition.request_zone(zone.clone()) {
                    Ok(id) => echo.write(format!("map: requested `{zone}` (swap #{id})")),
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
                let in_session = has_world.0
                    || playback.is_some()
                    || transition.dump_id().is_some()
                    || bridge.is_some();
                if !cmd.args.is_empty() {
                    dispatch.release();
                    echo.write("usage: disconnect");
                } else if !in_session {
                    dispatch.release();
                    echo.write("disconnect: no session to leave");
                } else {
                    match transition.request_leave() {
                        Ok(id) => {
                            diag::lifecycle_boundary(
                                "disconnect_requested",
                                &format!(" swap={id}"),
                            );
                            echo.write(format!(
                                "disconnect: waiting for session teardown (swap #{id})"
                            ))
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
