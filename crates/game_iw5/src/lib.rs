use game_api::{ModeRules, Rule, ScriptProgram, ScriptRequest, unknown};

pub struct Iw5;

pub static GAME: Iw5 = Iw5;

impl game_api::GameScripts for Iw5 {
    fn program(
        &self,
        _request: &ScriptRequest<'_>,
        _sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        Rule::Unknown(unknown!(
            "iw5.scripts.gametypes",
            "Modern Warfare 3 gametype scripts need Modern Warfare 3's builtin catalog, natives and match flow",
            "Modern Warfare 3's builtin list and the natives behind it"
        ))
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    fn config_defaults(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

impl game_api::GameVision for Iw5 {
    fn shellshock(&self) -> Rule<()> {
        Rule::Unknown(unknown!(
            "iw5.vision.shellshock",
            "Modern Warfare 3's shellshock files and how its screen draws a shellshock",
            "Modern Warfare 3's .shock format and shellshock screen rules"
        ))
    }
}

impl game_api::GameMenus for Iw5 {
    fn font(&self, _font_enum: i32, _placement_scale: f32, _text_scale: f32) -> Rule<&'static str> {
        Rule::Unknown(unknown!(
            "iw5.hud.menu_font",
            "the font a Modern Warfare 3 menu textfont names",
            "Modern Warfare 3's menu font table"
        ))
    }

    fn layout(&self) -> Rule<&'static dyn game_api::MenuLayout> {
        Rule::Unknown(unknown!(
            "iw5.hud.menu_layout",
            "how Modern Warfare 3's menu engine places an item and scales its text",
            "Modern Warfare 3's menu engine rules: text origin, text scale, screen placement"
        ))
    }
}

impl game_api::GameModes for Iw5 {
    fn mode(&self, _gametype: &str) -> Rule<ModeRules> {
        Rule::Unknown(unknown!(
            "iw5.scripts.gametypes",
            "Modern Warfare 3 gametype scripts need Modern Warfare 3's builtin catalog, natives and match flow",
            "Modern Warfare 3's builtin list and the natives behind it"
        ))
    }

    fn zombies(&self) -> Option<&'static game_api::LibraryMode> {
        None
    }
}
