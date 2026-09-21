use bevy::prelude::*;
use render_frontend::{
    assemble::drawsurf::{dof::GlowDvars, film_vision_view::FilmVisionView},
    prepare::scene::{view_parms::PreparedSceneView, world::WorldScene},
};

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    registry.register(
        crate::CommandSpec::new("visionSetNaked")
            .usage("visionSetNaked <preset> [seconds] — apply a loaded vision preset"),
    );
    registry.register(
        crate::CommandSpec::new("visionReset")
            .usage("visionReset [seconds] — restore the map vision"),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn route(
    mut commands: MessageReader<ConsoleCommand>,
    scene: Res<WorldScene>,
    view: Res<PreparedSceneView>,
    clock: Res<net::CgFrameClock>,
    glow: Res<GlowDvars>,
    mut film: ResMut<FilmVisionView>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
) {
    for cmd in commands.read() {
        let reset = match cmd.name.as_str() {
            "visionSetNaked" => false,
            "visionReset" => true,
            _ => continue,
        };
        let result = (|| -> Result<String, String> {
            if !view.ready {
                return Err("vision: no ready world view".into());
            }
            let (name, duration) = match (reset, cmd.args.as_slice()) {
                (true, []) => ("", None),
                (true, [duration]) => ("", Some(duration)),
                (false, [name]) => (name.as_str(), None),
                (false, [name, duration]) => (name.as_str(), Some(duration)),
                _ => {
                    return Err(
                        "usage: visionSetNaked <preset> [seconds] | visionReset [seconds]".into(),
                    );
                }
            };
            let seconds = duration
                .map_or(Ok(1.0), |s| s.parse::<f64>())
                .map_err(|_| "vision: invalid transition duration")?;
            if !seconds.is_finite() || seconds < 0.0 || seconds * 1000.0 > i32::MAX as f64 {
                return Err(
                    "vision: duration must be finite, nonnegative and fit milliseconds".into(),
                );
            }
            let preset = if name.is_empty() {
                if scene.film_vision.is_none() {
                    return Err("vision: map preset is unavailable".into());
                }
                None
            } else {
                let normalized = name.replace('\\', "/").to_ascii_lowercase();
                let stem = normalized.strip_prefix("vision/").unwrap_or(&normalized);
                let stem = stem.strip_suffix(".vision").unwrap_or(stem);
                let key = format!("vision/{stem}.vision");
                Some(match scene.film_visions.get(&key) {
                    Some(Ok(vision)) => *vision,
                    Some(Err(error)) => return Err(format!("vision: {key}: {error:?}")),
                    None => return Err(format!("vision: preset {key} is not loaded")),
                })
            };
            film.select(
                scene.film_vision,
                preset,
                clock.time(),
                (seconds * 1000.0).round() as i32,
                glow.allowed,
                glow.allowed_script_forced,
            );
            Ok(format!(
                "vision: {} transition={seconds}s",
                if name.is_empty() { "map" } else { name }
            ))
        })();
        let msg = result.unwrap_or_else(|error| error);
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, settings.log_capacity);
    }
}
