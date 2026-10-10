mod catalog;
mod startup;

use game_api::{HudRules, ModeRules, Rule, ScriptProgram, ScriptRequest};

pub struct Iw4;

pub static GAME: Iw4 = Iw4;

impl game_api::GameScripts for Iw4 {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        let startup = startup::Iw4Startup::new(sources, request.gametype, request.map);
        Rule::Known(ScriptProgram {
            catalog: gsc::Catalog::from_list(asset_core::FamilyId::Iw4, catalog::IW4),
            roots: startup.roots,
            entries: startup.entries,
        })
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        &[("sv_maxclients", "18")]
    }

    fn config_defaults(&self) -> &'static [(&'static str, &'static str)] {
        &[("onlinegame", "1")]
    }
}

/// Modern Warfare 2's modes: a prematch countdown, deaths, code-drawn HUD.
const MODE: ModeRules = ModeRules {
    play_starts_on: "prematch_over",
    every_player_downs: Rule::Known(false),
    movement: Rule::Known(()),
    weapons: Rule::Known(()),
    spawn_at_default_health: false,
    connect_team: None,
    scripts_spawn_players: false,
    spawn_classnames: None,
    unlimited: Rule::Known(false),
    limits_from_config: false,
    default_score_limit: None,
    zombie_zone_scripts: false,
    report_builtin_gaps: true,
    waits_for_lobby: false,
    binds_account: true,
    binds_objectives: true,
    hud: HudRules {
        code_hud: Rule::Known(()),
        game_hud_menus: false,
        scoreboard: true,
        scorebar: true,
        compass: true,
        script_text: true,
        material_font_floor: true,
    },
};

impl game_api::GameVision for Iw4 {
    fn shellshock(&self) -> Rule<()> {
        Rule::Known(())
    }
}

impl game_api::GameMenus for Iw4 {
    fn font(&self, font_enum: i32, placement_scale: f32, text_scale: f32) -> Rule<&'static str> {
        Rule::Known(hud_iw4::ui_get_font_handle(
            font_enum,
            placement_scale,
            text_scale,
        ))
    }

    fn layout(&self) -> Rule<&'static dyn game_api::MenuLayout> {
        Rule::Known(&MenuLayout)
    }
}

/// Modern Warfare 2's menu engine placement and text scaling.
pub struct MenuLayout;

impl game_api::MenuLayout for MenuLayout {
    fn text_scale(&self, pixel_height: i32, text_scale: f32) -> f32 {
        hud_iw4::normalized_text_scale(pixel_height, text_scale)
    }
    fn text_height(&self, text_scale: f32) -> f32 {
        hud_iw4::ui_text_height(text_scale)
    }
    fn text_origin(
        &self,
        [x, y, w, h]: [f32; 4],
        align_mode: i32,
        align_x: f32,
        align_y: f32,
        measured_w: f32,
        measured_h: f32,
    ) -> (f32, f32) {
        hud_iw4::item_text_origin(
            x, y, w, h, align_mode, align_x, align_y, measured_w, measured_h,
        )
    }
    fn text_paint_scale(&self, text_scale: f32, menu_scale: f32) -> f32 {
        hud_iw4::item_text_paint_scale(text_scale, menu_scale)
    }
    fn window_rect(&self, [x, y, w, h]: [f32; 4], menu_scale: f32) -> [f32; 4] {
        let (x, y, w, h) = hud_iw4::window_paint_scale_rect(x, y, w, h, menu_scale);
        [x, y, w, h]
    }
}

impl game_api::GameModes for Iw4 {
    fn mode(&self, _gametype: &str) -> Rule<ModeRules> {
        Rule::Known(MODE)
    }

    fn zombies(&self) -> Option<&'static game_api::LibraryMode> {
        None
    }
}
