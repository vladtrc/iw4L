#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TweakCategory {
    Game,
    Team,
    Player,
    Weapon,
    Hardpoint,
    Hud,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tweakable {
    pub category: TweakCategory,
    pub name: &'static str,
    pub dvar: &'static str,
    pub value: i32,
}

pub const PC_INIT: &[Tweakable] = &[
    Tweakable {
        category: TweakCategory::Game,
        name: "playerwaittime",
        dvar: "scr_game_playerwaittime",
        value: 15,
    },
    Tweakable {
        category: TweakCategory::Game,
        name: "matchstarttime",
        dvar: "scr_game_matchstarttime",
        value: 5,
    },
    Tweakable {
        category: TweakCategory::Game,
        name: "onlyheadshots",
        dvar: "scr_game_onlyheadshots",
        value: 0,
    },
    Tweakable {
        category: TweakCategory::Game,
        name: "allowkillcam",
        dvar: "scr_game_allowkillcam",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Game,
        name: "spectatetype",
        dvar: "scr_game_spectatetype",
        value: 2,
    },
    Tweakable {
        category: TweakCategory::Game,
        name: "deathpointloss",
        dvar: "scr_game_deathpointloss",
        value: 0,
    },
    Tweakable {
        category: TweakCategory::Game,
        name: "suicidepointloss",
        dvar: "scr_game_suicidepointloss",
        value: 0,
    },
    Tweakable {
        category: TweakCategory::Team,
        name: "teamkillpointloss",
        dvar: "scr_team_teamkillpointloss",
        value: 0,
    },
    Tweakable {
        category: TweakCategory::Team,
        name: "fftype",
        dvar: "scr_team_fftype",
        value: 0,
    },
    Tweakable {
        category: TweakCategory::Team,
        name: "teamkillspawndelay",
        dvar: "scr_team_teamkillspawndelay",
        value: 0,
    },
    Tweakable {
        category: TweakCategory::Player,
        name: "maxhealth",
        dvar: "scr_player_maxhealth",
        value: 100,
    },
    Tweakable {
        category: TweakCategory::Player,
        name: "healthregentime",
        dvar: "scr_player_healthregentime",
        value: 5,
    },
    Tweakable {
        category: TweakCategory::Player,
        name: "forcerespawn",
        dvar: "scr_player_forcerespawn",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Weapon,
        name: "allowfrag",
        dvar: "scr_weapon_allowfrags",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Weapon,
        name: "allowsmoke",
        dvar: "scr_weapon_allowsmoke",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Weapon,
        name: "allowflash",
        dvar: "scr_weapon_allowflash",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Weapon,
        name: "allowc4",
        dvar: "scr_weapon_allowc4",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Weapon,
        name: "allowclaymores",
        dvar: "scr_weapon_allowclaymores",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Weapon,
        name: "allowrpgs",
        dvar: "scr_weapon_allowrpgs",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Weapon,
        name: "allowmines",
        dvar: "scr_weapon_allowmines",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Hardpoint,
        name: "allowartillery",
        dvar: "scr_hardpoint_allowartillery",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Hardpoint,
        name: "allowuav",
        dvar: "scr_hardpoint_allowuav",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Hardpoint,
        name: "allowsupply",
        dvar: "scr_hardpoint_allowsupply",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Hardpoint,
        name: "allowhelicopter",
        dvar: "scr_hardpoint_allowhelicopter",
        value: 1,
    },
    Tweakable {
        category: TweakCategory::Hud,
        name: "showobjicons",
        dvar: "ui_hud_showobjicons",
        value: 1,
    },
];

pub const CONSOLE_GRACEPERIOD_SECONDS: i32 = 15;

pub fn tweakable_value(category: TweakCategory, name: &str) -> Option<i32> {
    PC_INIT
        .iter()
        .find(|row| row.category == category && row.name == name)
        .map(|row| row.value)
}
