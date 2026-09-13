#[derive(Debug, Clone, Copy)]
pub struct PerkPreset {
    pub reference: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ClassPreset {
    pub name: &'static str,
    pub primary: &'static str,
    pub primary_attachments: &'static [&'static str],
    pub secondary: &'static str,
    pub secondary_attachments: &'static [&'static str],
    pub lethal: &'static str,
    pub tactical: &'static str,
    pub perks: &'static [PerkPreset],

    pub deathstreak: &'static str,
}

pub fn default_presets() -> &'static [ClassPreset] {
    &DEFAULTS
}

pub fn preset_index(name: &str) -> Option<usize> {
    default_presets()
        .iter()
        .position(|preset| preset.name.eq_ignore_ascii_case(name))
}

pub fn preset_at(index: usize) -> Option<&'static ClassPreset> {
    default_presets().get(index)
}

const DEFAULTS: [ClassPreset; 5] = [
    ClassPreset {
        name: "assault",
        primary: "iw4:weapon/ak47_mp",
        primary_attachments: &["acog", "fmj"],
        secondary: "iw4:weapon/usp_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/semtex_mp",
        tactical: "iw4:weapon/flash_grenade_mp",

        perks: &[
            PerkPreset {
                reference: "specialty_fastreload",
            },
            PerkPreset {
                reference: "specialty_bulletdamage",
            },
            PerkPreset {
                reference: "specialty_bulletaccuracy",
            },
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "specops",
        primary: "iw4:weapon/ump45_mp",
        primary_attachments: &["silencer"],
        secondary: "iw4:weapon/usp_mp",
        secondary_attachments: &["silencer"],
        lethal: "iw4:weapon/throwingknife_mp",
        tactical: "iw4:weapon/smoke_grenade_mp",

        perks: &[
            PerkPreset {
                reference: "specialty_marathon",
            },
            PerkPreset {
                reference: "specialty_lightweight",
            },
            PerkPreset {
                reference: "specialty_heartbreaker",
            },
        ],
        deathstreak: "specialty_finalstand",
    },
    ClassPreset {
        name: "demolitions",
        primary: "iw4:weapon/spas12_mp",
        primary_attachments: &["grip"],
        secondary: "iw4:weapon/deserteagle_mp",
        secondary_attachments: &["fmj"],
        lethal: "iw4:weapon/semtex_mp",
        tactical: "iw4:weapon/flash_grenade_mp",

        perks: &[
            PerkPreset {
                reference: "specialty_scavenger",
            },
            PerkPreset {
                reference: "specialty_explosivedamage",
            },
        ],
        deathstreak: "specialty_combathigh",
    },
    ClassPreset {
        name: "sniper",
        primary: "iw4:weapon/cheytac_mp",
        primary_attachments: &["fmj"],
        secondary: "iw4:weapon/usp_mp",
        secondary_attachments: &["silencer"],
        lethal: "iw4:weapon/throwingknife_mp",

        tactical: "iw4:weapon/smoke_grenade_mp",

        perks: &[
            PerkPreset {
                reference: "specialty_bling",
            },
            PerkPreset {
                reference: "specialty_coldblooded",
            },
            PerkPreset {
                reference: "specialty_localjammer",
            },
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "famas_burst",
        primary: "iw4:weapon/famas_mp",
        primary_attachments: &["eotech"],
        secondary: "iw4:weapon/beretta_mp",
        secondary_attachments: &["silencer"],
        lethal: "iw4:weapon/semtex_mp",
        tactical: "iw4:weapon/concussion_grenade_mp",

        perks: &[
            PerkPreset {
                reference: "specialty_scavenger",
            },
            PerkPreset {
                reference: "specialty_bulletdamage",
            },
        ],
        deathstreak: "specialty_copycat",
    },
];
