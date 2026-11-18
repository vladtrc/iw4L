#[derive(Clone, Copy)]
pub struct SingleplayerMode {
    pub label: &'static str,
    pub button_label: Option<&'static str>,
    pub entries: &'static [&'static str],
    pub zones: &'static [&'static str],
    pub previews: &'static [&'static str],
}

pub fn campaign_entry_dvars(zone: &str) -> &'static [(&'static str, &'static str)] {
    match zone {
        "ending" | "iw4:ending" => &[("ui_char_museum_mode", "free")],
        _ => &[],
    }
}

impl SingleplayerMode {
    pub fn available(&self) -> bool {
        !self.zones.is_empty()
    }

    pub fn pages(&self, page_size: usize) -> usize {
        self.entries.len().div_ceil(page_size)
    }

    pub fn zone(&self, page: usize, row: usize, page_size: usize) -> Result<&'static str, String> {
        if !self.available() {
            return Err(format!("{} is unavailable", self.label));
        }
        if row >= page_size {
            return Err("Invalid mission row".into());
        }
        self.zones
            .get(page * page_size + row)
            .copied()
            .ok_or_else(|| "Mission is unavailable".into())
    }
}

pub const REFERENCE_SINGLEPLAYER_MODES: &[SingleplayerMode] = &[
    SingleplayerMode {
        label: "MW2 Campaign",
        button_label: None,
        entries: &[
            "S.S.D.D.",
            "Team Player",
            "Cliffhanger",
            "No Russian",
            "Takedown",
            "Wolverines!",
            "The Hornet's Nest",
            "Exodus",
            "The Only Easy Day... Was Yesterday",
            "The Gulag",
            "Of Their Own Accord",
            "Contingency",
            "Second Sun",
            "Whiskey Hotel",
            "Loose Ends",
            "The Enemy of My Enemy",
            "Just Like Old Times",
            "Endgame",
            "Museum",
        ],
        zones: &[
            "iw4:trainer",
            "iw4:roadkill",
            "iw4:cliffhanger",
            "iw4:airport",
            "iw4:favela",
            "iw4:invasion",
            "iw4:favela_escape",
            "iw4:arcadia",
            "iw4:oilrig",
            "iw4:gulag",
            "iw4:dcburning",
            "iw4:contingency",
            "iw4:dcemp",
            "iw4:dc_whitehouse",
            "iw4:estate",
            "iw4:boneyard",
            "iw4:af_caves",
            "iw4:af_chase",
            "iw4:ending",
        ],
        previews: &[
            "iw4:material/mw2_trainer",
            "iw4:material/mw2_roadkill",
            "iw4:material/mw2_cliffhanger",
            "iw4:material/mw2_airport",
            "iw4:material/mw2_favela",
            "iw4:material/mw2_invasion",
            "iw4:material/mw2_favela_escape",
            "iw4:material/mw2_arcadia",
            "iw4:material/mw2_oilrig",
            "iw4:material/mw2_gulag",
            "iw4:material/mw2_dcburning",
            "iw4:material/mw2_contingency",
            "iw4:material/mw2_dcemp",
            "iw4:material/mw2_dc_whitehouse",
            "iw4:material/mw2_estate",
            "iw4:material/mw2_boneyard",
            "iw4:material/mw2_af_caves",
            "iw4:material/mw2_af_chase",
            "iw4:material/mw2_museum",
        ],
    },
    SingleplayerMode {
        label: "MW2 Spec Ops",
        button_label: None,
        entries: &[
            "The Pit",
            "Sniper Fi",
            "O Cristo Redentor",
            "Evasion",
            "Suspension",
            "Overwatch",
            "Body Count",
            "Bomb Squad",
            "Race",
            "Big Brother",
            "Hidden",
            "Breach & Clear",
            "Time Trial",
            "Homeland Security",
            "Snatch & Grab",
            "Wardriving",
            "Wreckage",
            "Acceptable Losses",
            "Terminal",
            "Estate Takedown",
            "Wetwork",
            "High Explosive",
            "Armor Piercing",
        ],
        zones: &[],
        previews: &[
            "iw4:material/so_killspree_trainer",
            "iw4:material/so_rooftop_contingency",
            "iw4:material/so_killspree_favela",
            "iw4:material/so_forest_contingency",
            "iw4:material/so_crossing_so_bridge",
            "iw4:material/so_ac130_co_hunted",
            "iw4:material/so_killspree_invasion",
            "iw4:material/so_defuse_favela_escape",
            "iw4:material/so_snowrace1_cliffhanger",
            "iw4:material/so_chopper_invasion",
            "iw4:material/so_hidden_so_ghillies",
            "iw4:material/so_showers_gulag",
            "iw4:material/so_snowrace2_cliffhanger",
            "iw4:material/so_defense_invasion",
            "iw4:material/so_intel_boneyard",
            "iw4:material/so_download_arcadia",
            "iw4:material/so_demo_so_bridge",
            "iw4:material/so_sabotage_cliffhanger",
            "iw4:material/so_escape_airport",
            "iw4:material/so_takeover_estate",
            "iw4:material/so_assault_oilrig",
            "iw4:material/so_juggernauts_favela",
            "iw4:material/so_takeover_oilrig",
        ],
    },
];
