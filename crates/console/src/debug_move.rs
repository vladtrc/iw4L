use crate::plugin::WaitKind;
use bevy::prelude::*;
use net::PresentedSnapshot;
use sim::{ClientAction, ClientLifecycle, MatchPhase, SpawnPick};
use ui::UiLayer;

use crate::feature_dispatch::DebugPosOverlay;
use crate::{ConsoleCommand, ConsoleDispatch, ConsoleLine, ConsoleRegistry, plugin::WaitMovePose};

#[derive(Component)]
pub(crate) struct ShowposHud;

pub(crate) fn spawn_showpos_hud(commands: &mut Commands, font: Handle<Font>) {
    commands.spawn((
        ShowposHud,
        UiLayer::Overlay,
        Visibility::Hidden,
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(12),
            padding: UiRect::all(px(6)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.03, 0.04, 0.72)),
        GlobalZIndex(19_000),
        Text::new(""),
        TextFont {
            font: font.into(),
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.85, 0.40)),
    ));
}

pub(crate) fn register_debug_move_commands(registry: &mut ConsoleRegistry) {
    if registry.resolve("showpos").is_none() {
        registry.register(
            crate::CommandSpec::new("showpos")
                .alias("debug_pos")
                .usage("showpos [on|off] — origin overlay (bare also prints once)"),
        );
    }
    if registry.resolve("move").is_none() {
        registry.register(
            crate::CommandSpec::new("move")
                .alias("tp")
                .usage("move <x> <y> <z> [yaw] [pitch] — write player origin (needs cheats)"),
        );
    }
    if registry.resolve("god").is_none() {
        registry.register(
            crate::CommandSpec::new("god").usage("god — toggle invincibility (needs cheats)"),
        );
    }
    if registry.resolve("projectile_flight").is_none() {
        registry.register(crate::CommandSpec::new("projectile_flight").usage(
            "projectile_flight — toggle bouncing projectiles and travel through portals (requires cheats)",
        ));
    }
    if registry.resolve("kill").is_none() {
        registry
            .register(crate::CommandSpec::new("kill").usage(
                "kill — ForceDeath the local player (needs cheats; invented, not COMMANDS)",
            ));
    }
    if registry.resolve("force_spawn").is_none() {
        registry.register(crate::CommandSpec::new("force_spawn").usage(
            "force_spawn [random <seed> | at <x> <y> <z> [yaw]] — respawn from any state through spawn resolution (needs cheats)",
        ));
    }
    if registry.resolve("damage").is_none() {
        registry.register(
            crate::CommandSpec::new("damage").usage(
                "damage [amount] — subtract Alive health (default 40; needs cheats; invented)",
            ),
        );
    }
    if registry.resolve("look").is_none() {
        registry.register(
            crate::CommandSpec::new("look")
                .usage("look <yaw> <pitch> — write LookState + authority angles (needs cheats)"),
        );
    }
    if registry.resolve("name").is_none() {
        registry.register(
            crate::CommandSpec::new("name")
                .usage("name [string] — write clientState.name[16] for the local client"),
        );
    }
    if registry.resolve("nudge").is_none() {
        registry.register(crate::CommandSpec::new("nudge").usage(
            "nudge <dx> <dy> <dz> — authority-only origin shift, no teleport bit (needs cheats)",
        ));
    }
    if registry.resolve("end_grace_period").is_none() {
        registry.register(crate::CommandSpec::new("end_grace_period").usage(
            "end_grace_period — the stock grace period ends within a second (needs cheats; invented)",
        ));
    }
    if registry.resolve("force_match_start").is_none() {
        registry.register(crate::CommandSpec::new("force_match_start").usage(
            "force_match_start — skip waitForPlayers and matchStartTimer (needs cheats; invented)",
        ));
    }
}

pub(crate) fn route_debug_move_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: crate::feature_dispatch::ConsoleEcho,
    mut debug_pos: ResMut<DebugPosOverlay>,
    presentation: net::LocalPresentation,
    mut authority: Option<ResMut<net::AuthorityWorld>>,
    mut target: crate::client_actions::ClientActionTarget,
    mut dispatch: ResMut<ConsoleDispatch>,
) {
    for cmd in events.read() {
        match cmd.name.as_str() {
            "showpos" | "debug_pos" => match cmd.args.first().map(String::as_str) {
                None => {
                    debug_pos.0 = true;
                    echo.write(format_showpos(
                        &presentation.presented,
                        presentation.client_id(),
                    ));
                }
                Some("on") | Some("1") => {
                    debug_pos.0 = true;
                    echo.write("showpos on");
                }
                Some("off") | Some("0") => {
                    debug_pos.0 = false;
                    echo.write("showpos off");
                }
                Some(other) => echo.write(format!("usage: showpos [on|off] (got `{other}`)")),
            },
            "move" | "tp" => {
                match parse_move(&cmd.args, &presentation.presented, presentation.client_id()) {
                    Err(msg) => echo.write(msg),
                    Ok((origin, angles)) => {
                        if !alive(&presentation.presented, presentation.client_id()) {
                            echo.write("move: not Alive — spawn a class first");
                            continue;
                        }
                        if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                            echo.write("move: cheats are off");
                            continue;
                        }
                        let Some(mut inbox) = target.queue() else {
                            echo.write("move: no action inbox (not a listen host)");
                            continue;
                        };
                        if let Err(error) = inbox.push(
                            presentation.client_id(),
                            ClientAction::Move { origin, angles },
                        ) {
                            echo.write(format!("move: {error}"));
                            continue;
                        }

                        arm_wait_move(
                            &mut dispatch,
                            cmd.background,
                            presentation.client_id(),
                            origin,
                            angles,
                            "move",
                        );
                        echo.write(format!(
                            "move: queued ({:.1} {:.1} {:.1}) yaw={:.0} pitch={:.0}{}",
                            origin[0],
                            origin[1],
                            origin[2],
                            angles[1],
                            angles[0],
                            if cmd.background {
                                " (async)"
                            } else {
                                " (sync until presented pose)"
                            }
                        ));
                    }
                }
            }
            "look" => {
                match parse_look(&cmd.args, &presentation.presented, presentation.client_id()) {
                    Err(msg) => echo.write(msg),
                    Ok((origin, angles)) => {
                        if !alive(&presentation.presented, presentation.client_id()) {
                            echo.write("look: not Alive — spawn a class first");
                            continue;
                        }
                        if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                            echo.write("look: cheats are off");
                            continue;
                        }
                        let Some(mut inbox) = target.queue() else {
                            echo.write("look: no action inbox (not a listen host)");
                            continue;
                        };
                        if let Err(error) = inbox.push(
                            presentation.client_id(),
                            ClientAction::Move { origin, angles },
                        ) {
                            echo.write(format!("look: {error}"));
                            continue;
                        }
                        arm_wait_move(
                            &mut dispatch,
                            cmd.background,
                            presentation.client_id(),
                            origin,
                            angles,
                            "look",
                        );
                        echo.write(format!(
                            "look: queued yaw={:.0} pitch={:.0}{}",
                            angles[1],
                            angles[0],
                            if cmd.background {
                                " (async)"
                            } else {
                                " (sync until presented pose)"
                            }
                        ));
                    }
                }
            }
            "god" => {
                if !cmd.args.is_empty() {
                    echo.write("usage: god");
                    continue;
                }
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo.write("god: cheats are off");
                    continue;
                }
                if !alive(&presentation.presented, presentation.client_id()) {
                    echo.write("god: not Alive — spawn a class first");
                    continue;
                }
                let Some(mut inbox) = target.queue() else {
                    echo.write("god: no action inbox");
                    continue;
                };
                if let Err(error) = inbox.push(presentation.client_id(), ClientAction::ToggleGod) {
                    echo.write(format!("god: {error}"));
                    continue;
                }
                echo.write("god: queued toggle");
            }
            "end_grace_period" => {
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo.write("end_grace_period: cheats are off");
                    continue;
                }
                let Some(mut inbox) = target.queue() else {
                    echo.write("end_grace_period: no action inbox");
                    continue;
                };
                if let Err(error) =
                    inbox.push(presentation.client_id(), ClientAction::EndGracePeriod)
                {
                    echo.write(format!("end_grace_period: {error}"));
                    continue;
                }
                echo.write("end_grace_period: queued");
            }
            "projectile_flight" => {
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo.write("projectile_flight: cheats are off");
                    continue;
                }
                let Some(mut inbox) = target.queue() else {
                    echo.write("projectile_flight: no action inbox");
                    continue;
                };
                if let Err(error) = inbox.push(
                    presentation.client_id(),
                    ClientAction::ToggleProjectileFlight,
                ) {
                    echo.write(format!("projectile_flight: {error}"));
                    continue;
                }
                echo.write("projectile_flight: queued toggle");
            }
            "kill" => {
                if !alive(&presentation.presented, presentation.client_id()) {
                    echo.write("kill: not Alive — spawn a class first");
                    continue;
                }
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo.write("kill: cheats are off");
                    continue;
                }
                let Some(mut inbox) = target.queue() else {
                    echo.write("kill: no action inbox (not a listen host)");
                    continue;
                };
                if let Err(error) = inbox.push(presentation.client_id(), ClientAction::ForceDeath) {
                    echo.write(format!("kill: {error}"));
                    continue;
                }
                echo.write("kill: queued ForceDeath");
            }
            "force_spawn" => {
                let pick = match parse_force_spawn(&cmd.args) {
                    Ok(pick) => pick,
                    Err(error) => {
                        echo.write(format!("force_spawn: {error}"));
                        continue;
                    }
                };
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo.write("force_spawn: cheats are off");
                    continue;
                }
                let Some(mut inbox) = target.queue() else {
                    echo.write("force_spawn: no action inbox (not a listen host)");
                    continue;
                };
                if let Err(error) =
                    inbox.push(presentation.client_id(), ClientAction::ForceSpawn { pick })
                {
                    echo.write(format!("force_spawn: {error}"));
                    continue;
                }
                if !cmd.background {
                    let life = authority
                        .as_ref()
                        .and_then(|a| a.0.client_meta(presentation.client_id()))
                        .map(|m| m.life_sequence)
                        .unwrap_or(sim::LifeSequence(0));
                    dispatch.wait(WaitKind::Alive(presentation.client_id(), life));
                }
                echo.write(format!("force_spawn: queued {pick:?}"));
            }
            "damage" => {
                let amount = match cmd.args.first() {
                    None => Ok(40),
                    Some(raw) => raw
                        .parse::<i32>()
                        .map_err(|_| format!("usage: damage [amount] (got `{raw}`)")),
                };
                let amount = match amount {
                    Ok(n) if n > 0 => n,
                    Ok(_) => {
                        echo.write("damage: amount must be > 0");
                        continue;
                    }
                    Err(msg) => {
                        echo.write(msg);
                        continue;
                    }
                };
                if !alive(&presentation.presented, presentation.client_id()) {
                    echo.write("damage: not Alive — spawn a class first");
                    continue;
                }
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo.write("damage: cheats are off");
                    continue;
                }
                let Some(mut inbox) = target.queue() else {
                    echo.write("damage: no action inbox (not a listen host)");
                    continue;
                };
                if let Err(error) = inbox.push(
                    presentation.client_id(),
                    ClientAction::DebugDamage { amount },
                ) {
                    echo.write(format!("damage: {error}"));
                    continue;
                }
                echo.write(format!("damage: queued {amount}"));
            }
            "nudge" => {
                if cmd.args.len() != 3 {
                    echo.write("usage: nudge <dx> <dy> <dz>");
                    continue;
                }
                if !alive(&presentation.presented, presentation.client_id()) {
                    echo.write("nudge: not Alive — spawn a class first");
                    continue;
                }
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo.write("nudge: cheats are off");
                    continue;
                }
                let parse = |s: &str| {
                    s.parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| format!("nudge: not a finite number `{s}`"))
                };
                let delta = match (
                    parse(&cmd.args[0]),
                    parse(&cmd.args[1]),
                    parse(&cmd.args[2]),
                ) {
                    (Ok(x), Ok(y), Ok(z)) => [x, y, z],
                    (Err(msg), _, _) | (_, Err(msg), _) | (_, _, Err(msg)) => {
                        echo.write(msg);
                        continue;
                    }
                };
                let Some(authority) = authority.as_deref_mut() else {
                    echo.write("nudge: no authority world (not a listen host)");
                    continue;
                };
                authority
                    .0
                    .gate_nudge_origin(presentation.client_id(), delta);
                echo.write(format!(
                    "nudge: authority origin += ({:.1} {:.1} {:.1})",
                    delta[0], delta[1], delta[2]
                ));
            }
            "name" => match cmd.args.as_slice() {
                [] => {
                    let shown = presentation
                        .presented
                        .snapshot()
                        .and_then(|s| s.meta.for_client(presentation.client_id()))
                        .and_then(|m| entity_iw4::client_state_name(&m.name).map(str::to_owned));
                    match shown {
                        Some(n) => echo.write(format!("name is \"{n}\"")),
                        None => echo.write("name is \"\""),
                    }
                }
                [raw] => {
                    let Some(mut inbox) = target.queue() else {
                        echo.write("name: no action inbox (not a listen host)");
                        continue;
                    };
                    let packed = entity_iw4::pack_client_state_name(raw);
                    if let Err(error) = inbox.push(
                        presentation.client_id(),
                        ClientAction::SetName { name: packed },
                    ) {
                        echo.write(format!("name: {error}"));
                        continue;
                    }
                    echo.write(format!("name: queued `{raw}`"));
                }
                _ => echo.write("usage: name [string]"),
            },
            "force_match_start" => {
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo.write("force_match_start: cheats are off");
                    continue;
                }
                let Some(world) = authority.as_ref() else {
                    echo.write("force_match_start: no authority world (not a listen host)");
                    continue;
                };
                match world.0.phase() {
                    MatchPhase::Playing => {
                        echo.write("force_match_start: already playing");
                        continue;
                    }
                    MatchPhase::Warmup => {}
                    other => {
                        echo.write(format!(
                            "force_match_start: match is not in warmup (phase={other:?})"
                        ));
                        continue;
                    }
                }
                let Some(mut inbox) = target.queue() else {
                    echo.write("force_match_start: no action inbox (not a listen host)");
                    continue;
                };
                if let Err(error) = inbox.push(
                    presentation.client_id(),
                    ClientAction::SetMatchPhase {
                        phase: MatchPhase::Playing,
                    },
                ) {
                    echo.write(format!("force_match_start: {error}"));
                    continue;
                }
                if !cmd.background {
                    dispatch.wait(WaitKind::Playing);
                    echo.write(
                        "force_match_start: queued SetMatchPhase Playing (sync until Playing)",
                    );
                } else {
                    echo.write("force_match_start &: queued SetMatchPhase Playing (async)");
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn update_showpos_overlay(
    debug_pos: Res<DebugPosOverlay>,
    player: net::LocalPresentation,
    clock: Res<net::AuthorityClock>,
    time: Res<Time>,
    actions: Res<net::ClientActionInput>,
    mut line: ResMut<ConsoleLine>,
    mut hud: Query<(&mut Text, &mut Visibility), With<ShowposHud>>,
) {
    let net::LocalPresentation { presented, local } = player;
    if !debug_pos.0 {
        for (_, mut vis) in &mut hud {
            *vis = Visibility::Hidden;
        }
        return;
    }
    let text = format_showpos_live(&presented, local.0, clock.tick, &time, &actions);
    line.0 = text.clone();
    for (mut hud_text, mut vis) in &mut hud {
        *vis = Visibility::Visible;
        **hud_text = text.clone();
    }
}

fn alive(presented: &PresentedSnapshot, id: sim::ClientId) -> bool {
    presented
        .snapshot()
        .and_then(|s| s.meta.for_client(id))
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
}

fn format_showpos(presented: &PresentedSnapshot, id: sim::ClientId) -> String {
    match presented.alive_player(id) {
        Some(ps) => {
            let eye_z = ps.origin[2] + ps.view_height_current;
            format!(
                "showpos origin={:.1} {:.1} {:.1}  eye={:.1} {:.1} {:.1}  yaw={:.0} pitch={:.0}",
                ps.origin[0],
                ps.origin[1],
                ps.origin[2],
                ps.origin[0],
                ps.origin[1],
                eye_z,
                ps.viewangles[1],
                ps.viewangles[0]
            )
        }
        None => "showpos: not Alive".into(),
    }
}

fn format_showpos_live(
    presented: &PresentedSnapshot,
    id: sim::ClientId,
    tick: u32,
    time: &Time,
    actions: &net::ClientActionInput,
) -> String {
    let life = presented
        .snapshot()
        .and_then(|s| s.meta.for_client(id))
        .map(|m| format!("{:?}", m.lifecycle))
        .unwrap_or_else(|| "—".into());
    let (origin, eye, yaw, pitch, vz, ground) = match presented.alive_player(id) {
        Some(ps) => {
            let eye_z = ps.origin[2] + ps.view_height_current;
            (
                format!(
                    "{:.0} {:.0} {:.0}",
                    ps.origin[0], ps.origin[1], ps.origin[2]
                ),
                format!("{:.0} {:.0} {:.0}", ps.origin[0], ps.origin[1], eye_z),
                ps.viewangles[1],
                ps.viewangles[0],
                ps.velocity[2],
                ps.ground_entity_num,
            )
        }
        None => ("—".into(), "—".into(), 0.0, 0.0, 0.0, -1),
    };
    let fps = if time.delta_secs() > 0.0 {
        1.0 / time.delta_secs()
    } else {
        0.0
    };
    let mut held_names = Vec::new();
    actions.client.kb.visit_active_names(|n| held_names.push(n));
    let held = if held_names.is_empty() {
        "—".into()
    } else {
        held_names.join(",")
    };
    format!(
        "showpos tick={tick} life={life} origin={origin} eye={eye} yaw={yaw:.0} pitch={pitch:.0} vz={vz:.0} ground={ground} held=[{held}] fps={fps:.0}"
    )
}

fn parse_move(
    args: &[String],
    presented: &PresentedSnapshot,
    id: sim::ClientId,
) -> Result<([f32; 3], [f32; 3]), String> {
    if args.len() < 3 || args.len() > 5 {
        return Err("usage: move <x> <y> <z> [yaw] [pitch]".into());
    }
    let parse = |s: &str| {
        s.parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("move: not a finite number `{s}`"))
    };
    let origin = [parse(&args[0])?, parse(&args[1])?, parse(&args[2])?];
    let current = presented
        .alive_player(id)
        .map(|ps| ps.viewangles)
        .unwrap_or([0.0, 0.0, 0.0]);
    let mut angles = current;
    if args.len() >= 4 {
        angles[1] = parse(&args[3])?;
    }
    if args.len() == 5 {
        angles[0] = parse(&args[4])?;
    }
    Ok((origin, angles))
}

fn parse_look(
    args: &[String],
    presented: &PresentedSnapshot,
    id: sim::ClientId,
) -> Result<([f32; 3], [f32; 3]), String> {
    if args.len() != 2 {
        return Err("usage: look <yaw> <pitch>".into());
    }
    let parse = |s: &str| {
        s.parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("look: not a finite number `{s}`"))
    };
    let Some(ps) = presented.alive_player(id) else {
        return Err("look: not Alive — spawn a class first".into());
    };
    let mut angles = ps.viewangles;
    angles[1] = parse(&args[0])?;
    angles[0] = parse(&args[1])?;
    Ok((ps.origin, angles))
}

fn arm_wait_move(
    dispatch: &mut ConsoleDispatch,
    background: bool,
    client: sim::ClientId,
    origin: [f32; 3],
    angles: [f32; 3],
    verb: &str,
) {
    if background {
        diag::info!(Console, "{verb} &: async — FIFO not blocked");
        return;
    }
    dispatch.wait(WaitKind::Move(WaitMovePose {
        client,
        origin,
        angles,
    }));
}

pub(crate) fn presented_matches_move(presented: &PresentedSnapshot, pose: WaitMovePose) -> bool {
    let Some(ps) = presented.alive_player(pose.client) else {
        return false;
    };
    pose_matches_move(ps.origin, ps.viewangles, pose.origin, pose.angles)
}

fn pose_matches_move(
    have_origin: [f32; 3],
    have_angles: [f32; 3],
    want_origin: [f32; 3],
    want_angles: [f32; 3],
) -> bool {
    (have_origin[0] - want_origin[0]).abs() < 1.0
        && (have_origin[1] - want_origin[1]).abs() < 1.0
        && angle_abs_delta(have_angles[0], want_angles[0]) < 0.5
        && angle_abs_delta(have_angles[1], want_angles[1]) < 0.5
}

fn angle_abs_delta(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d.abs()
}

fn parse_force_spawn(args: &[String]) -> Result<SpawnPick, String> {
    const USAGE: &str = "usage: force_spawn [random <seed> | at <x> <y> <z> [yaw]]";
    let num = |raw: &str| {
        raw.parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("{USAGE} (expected a finite number, got `{raw}`)"))
    };
    match args.first().map(String::as_str) {
        None => Ok(SpawnPick::Seeded(0)),
        Some("random") => {
            let [_, seed] = args else {
                return Err(USAGE.into());
            };
            seed.parse::<u64>()
                .map(SpawnPick::Seeded)
                .map_err(|_| format!("{USAGE} (got `{seed}`)"))
        }
        Some("at") if matches!(args.len(), 4 | 5) => Ok(SpawnPick::At {
            origin: [num(&args[1])?, num(&args[2])?, num(&args[3])?],
            yaw: args
                .get(4)
                .map(|text| num(text))
                .transpose()?
                .unwrap_or(0.0),
        }),
        Some(_) => Err(USAGE.into()),
    }
}
