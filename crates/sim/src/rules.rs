use gamemode_iw4::ffa;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreeForAllRules {
    pub gametype_token: &'static str,
    pub display_name: &'static str,
    pub score_limit: i32,
    pub time_limit_ms: u32,
}

impl FreeForAllRules {
    pub const fn retail_defaults() -> Self {
        Self {
            gametype_token: ffa::GAMETYPE_TOKEN,
            display_name: ffa::DISPLAY_NAME,
            score_limit: ffa::SCORE_LIMIT,
            time_limit_ms: ffa::TIME_LIMIT_MS,
        }
    }
}

pub const FFA: FreeForAllRules = FreeForAllRules::retail_defaults();
