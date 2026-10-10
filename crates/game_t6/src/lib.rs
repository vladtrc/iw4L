mod startup;

use game_api::{HudRules, LibraryMode, ModeRules, Rule, ScriptProgram, ScriptRequest, unknown};

pub struct T6;

pub static GAME: T6 = T6;

impl game_api::GameScripts for T6 {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        _sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        let zombies_map = request.map.starts_with("zm_");
        match (zombies_map, request.gametype) {
            (false, "dm" | "war") | (true, "zclassic") => {}
            (true, _) => {
                return Rule::Unknown(unknown!(
                    "t6.scripts.zm_gametypes",
                    "Black Ops 2 zombies maps run only Classic (zclassic); Grief, Turned and the TranZit locations have no rules",
                    "Black Ops 2's zombies submode and location rules"
                ));
            }
            (false, "zclassic") => {
                return Rule::Unknown(unknown!(
                    "t6.scripts.zclassic_on_mp",
                    "Classic zombies needs a Black Ops 2 zombies map",
                    "a zombies map (zm_*)"
                ));
            }
            (false, _) => {
                return Rule::Unknown(unknown!(
                    "t6.scripts.gametypes",
                    "Black Ops 2 multiplayer modes other than Free for All and Team Deathmatch have no rules",
                    "Black Ops 2's rules for that mode"
                ));
            }
        }
        let startup = startup::T6Startup::new(request.map);
        Rule::Known(ScriptProgram {
            catalog: gsc::Catalog::from_list(asset_core::FamilyId::T6, &[]),
            roots: startup.roots,
            entries: startup.entries,
        })
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    fn config_defaults(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

/// IW4L's Free for All rules for Black Ops 2 (docs/fidelity/t6.md).
const FREE_FOR_ALL_MODE: ModeRules = ModeRules {
    default_score_limit: Some(30),
    ..MULTIPLAYER_MODE
};

/// IW4L's Team Deathmatch rules for Black Ops 2 (docs/fidelity/t6.md).
const TEAM_DEATHMATCH_MODE: ModeRules = ModeRules {
    default_score_limit: Some(75),
    ..MULTIPLAYER_MODE
};

const MULTIPLAYER_MODE: ModeRules = ModeRules {
    play_starts_on: "prematch_over",
    every_player_downs: Rule::Known(false),
    movement: Rule::Unknown(unknown!(
        "t6.movement.player",
        "Black Ops 2's player movement",
        "Black Ops 2's player movement rules"
    )),
    weapons: Rule::Unknown(unknown!(
        "t6.weapons.state_machine",
        "Black Ops 2's weapon state machine",
        "Black Ops 2's weapon rules"
    )),
    spawn_at_default_health: false,
    connect_team: None,
    scripts_spawn_players: false,
    spawn_classnames: None,
    unlimited: Rule::Known(false),
    limits_from_config: true,
    default_score_limit: None,
    zombie_zone_scripts: false,
    report_builtin_gaps: false,
    waits_for_lobby: false,
    binds_account: false,
    binds_objectives: true,
    hud: HudRules {
        code_hud: Rule::Unknown(unknown!(
            "t6.hud.code_hud",
            "Black Ops 2's code-drawn HUD; IW4L's own Black Ops 2 HUD (ui/src/t6_hud.rs) draws instead",
            "Black Ops 2's HUD rules"
        )),
        game_hud_menus: false,
        scoreboard: true,
        scorebar: true,
        compass: true,
        script_text: true,
        material_font_floor: true,
    },
};

const ZCLASSIC_MODE: ModeRules = ModeRules {
    spawn_classnames: Some(&["initial_spawn_points", "info_player_start"]),
    unlimited: Rule::Known(true),
    ..MULTIPLAYER_MODE
};

impl game_api::GameVision for T6 {
    fn shellshock(&self) -> Rule<()> {
        Rule::Unknown(unknown!(
            "t6.vision.shellshock",
            "Black Ops 2's shellshock files and how its screen draws a shellshock",
            "Black Ops 2's .shock format and shellshock screen rules"
        ))
    }
}

impl game_api::GameMenus for T6 {
    fn font(&self, _font_enum: i32, _placement_scale: f32, _text_scale: f32) -> Rule<&'static str> {
        Rule::Unknown(unknown!(
            "t6.hud.menu_font",
            "the font a Black Ops 2 menu textfont names",
            "Black Ops 2's menu font table"
        ))
    }

    fn layout(&self) -> Rule<&'static dyn game_api::MenuLayout> {
        Rule::Unknown(unknown!(
            "t6.hud.menu_layout",
            "how Black Ops 2's menu engine places an item and scales its text",
            "Black Ops 2's menu engine rules: text origin, text scale, screen placement"
        ))
    }
}

impl game_api::GameModes for T6 {
    fn mode(&self, gametype: &str) -> Rule<ModeRules> {
        match gametype {
            "dm" => Rule::Known(FREE_FOR_ALL_MODE),
            "war" => Rule::Known(TEAM_DEATHMATCH_MODE),
            "zclassic" => Rule::Known(ZCLASSIC_MODE),
            _ => Rule::Unknown(unknown!(
                "t6.scripts.gametypes",
                "Black Ops 2 multiplayer modes other than Free for All and Team Deathmatch have no rules",
                "Black Ops 2's rules for that mode"
            )),
        }
    }

    fn zombies(&self) -> Option<&'static LibraryMode> {
        Some(&ZOMBIES_LIBRARY)
    }
}

static ZOMBIES_LIBRARY: LibraryMode = LibraryMode {
    gametype: "zclassic",
    note: "Survival rules written for IW4L; map quests, special enemies and scripted events are not in yet (docs/fidelity/t6.md).",
    maps: &[
        ("zm_nuked", "NUKETOWN ZOMBIES"),
        ("zm_transit", "TRANZIT"),
        ("zm_highrise", "DIE RISE"),
        ("zm_prison", "MOB OF THE DEAD"),
        ("zm_buried", "BURIED"),
        ("zm_tomb", "ORIGINS"),
    ],
};
