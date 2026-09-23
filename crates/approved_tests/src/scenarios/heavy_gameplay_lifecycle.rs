//! `heavy_gameplay_lifecycle` — owner-approved. This file is the one place its
//! data lives.

use crate::scenario::{
    MapB, Phase, Place, ResolvedScene, Scenario, Scene, Weapon, capture_commands,
};

pub const SCENARIO: Scenario = Scenario {
    name: "heavy_gameplay_lifecycle",
    gametype: "dm",
    players: 16,
    class: "assault",
    map_a: "mp_overgrown",
    map_b: MapB::FromInstalled { game: "iw4" },
    scenes: SCENES,
    phase_timeout_secs: 600,
    quit_timeout_secs: 30,
};

const SCENES: &[Scene] = &[
    Scene {
        name: "forward_right_fire_turn_left",
        place: Place::Random,
        weapon: None,
        hold: &["+forward", "+moveright", "+attack", "+left"],
        pulse: None,
        ticks: 100,
    },
    Scene {
        name: "forward_left_fire_turn_right",
        place: Place::Random,
        weapon: Some(Weapon {
            give: "rpg",
            fallback: "class primary",
        }),
        hold: &["+forward", "+moveleft", "+attack", "+right"],
        pulse: None,
        ticks: 100,
    },
    Scene {
        name: "sprint_jump",
        place: Place::Random,
        weapon: None,
        hold: &["+forward", "+sprint"],
        pulse: Some(("+gostand", 20)),
        ticks: 100,
    },
];

pub fn phases(map_b: &str, scenes_a: &[ResolvedScene], scenes_b: &[ResolvedScene]) -> Vec<Phase> {
    let s = &SCENARIO;
    let bots = s.players.saturating_sub(1);
    let populate = || {
        vec![
            format!("spawn {}", s.class),
            "force_match_start".into(),
            format!("bot add {bots}"),
            "wait 40t".into(),
        ]
    };
    let mut out = vec![
        Phase::new("a.cold_load", vec!["wait world".into()]),
        captured("capture_00", "00_cold_load"),
        Phase::new("a.populate", populate()),
    ];
    out.extend(
        scenes_a
            .iter()
            .map(|scene| Phase::new(scene.phase(), scene.commands.clone())),
    );
    out.push(captured("capture_01", "01_overgrown_gameplay"));
    out.push(Phase::new(
        "disconnect",
        vec!["disconnect".into(), "wait torn".into()],
    ));
    out.push(Phase::new("menu_observation", vec!["wait 5s".into()]));
    out.push(captured("capture_02", "02_after_disconnect"));
    out.push(Phase::new(
        "b.load",
        vec![format!("map {map_b}"), "wait world".into()],
    ));
    out.push(Phase::new("b.populate", populate()));
    out.extend(
        scenes_b
            .iter()
            .map(|scene| Phase::new(scene.phase(), scene.commands.clone())),
    );
    out.push(captured("capture_03", "03_second_map_gameplay"));
    out.push(Phase::new("quit", vec!["finish_run".into()]));
    out
}

fn captured(name: &str, capture: &'static str) -> Phase {
    Phase {
        name: name.into(),
        commands: capture_commands(capture),
        capture: Some(capture),
    }
}
