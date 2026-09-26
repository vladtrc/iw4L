#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UniqueGroup {
    Assault,
    Smg,
    Lmg,
    Sniper,
    Riot,
    MachinePistol,
    Shotgun,
    Pistol,
    Projectile,
    Cqb,
    Special,
}

impl UniqueGroup {
    #[must_use]
    pub fn item_group(self) -> &'static str {
        match self {
            Self::Assault => "weapon_assault",
            Self::Smg => "weapon_smg",
            Self::Lmg => "weapon_lmg",
            Self::Sniper => "weapon_sniper",
            Self::Riot => "weapon_riot",
            Self::MachinePistol => "weapon_machine_pistol",
            Self::Shotgun => "weapon_shotgun",
            Self::Pistol => "weapon_pistol",
            Self::Projectile => "weapon_projectile",
            Self::Cqb => "weapon_cqb",
            Self::Special => "weapon_special",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UniqueWeapon {
    pub key: &'static str,
    pub group: UniqueGroup,
}

pub const UNIQUE_WEAPONS: &[UniqueWeapon] = &[
    UniqueWeapon {
        key: "iw4:weapon/masada_mp",
        group: UniqueGroup::Assault,
    },
    UniqueWeapon {
        key: "iw4:weapon/fn2000_mp",
        group: UniqueGroup::Assault,
    },
    UniqueWeapon {
        key: "iw4:weapon/tavor_mp",
        group: UniqueGroup::Assault,
    },
    UniqueWeapon {
        key: "iw4:weapon/kriss_mp",
        group: UniqueGroup::Smg,
    },
    UniqueWeapon {
        key: "iw4:weapon/uzi_mp",
        group: UniqueGroup::Smg,
    },
    UniqueWeapon {
        key: "iw4:weapon/mp5k_mp",
        group: UniqueGroup::Smg,
    },
    UniqueWeapon {
        key: "iw4:weapon/mg4_mp",
        group: UniqueGroup::Lmg,
    },
    UniqueWeapon {
        key: "iw4:weapon/sa80_mp",
        group: UniqueGroup::Lmg,
    },
    UniqueWeapon {
        key: "iw4:weapon/rpd_mp",
        group: UniqueGroup::Lmg,
    },
    UniqueWeapon {
        key: "iw4:weapon/cheytac_mp",
        group: UniqueGroup::Sniper,
    },
    UniqueWeapon {
        key: "iw4:weapon/wa2000_mp",
        group: UniqueGroup::Sniper,
    },
    UniqueWeapon {
        key: "iw4:weapon/m21_mp",
        group: UniqueGroup::Sniper,
    },
    UniqueWeapon {
        key: "iw4:weapon/riotshield_mp",
        group: UniqueGroup::Riot,
    },
    UniqueWeapon {
        key: "iw4:weapon/striker_mp",
        group: UniqueGroup::Shotgun,
    },
    UniqueWeapon {
        key: "iw4:weapon/ranger_mp",
        group: UniqueGroup::Shotgun,
    },
    UniqueWeapon {
        key: "iw4:weapon/model1887_mp",
        group: UniqueGroup::Shotgun,
    },
    UniqueWeapon {
        key: "iw4:weapon/usp_mp",
        group: UniqueGroup::Pistol,
    },
    UniqueWeapon {
        key: "iw4:weapon/coltanaconda_mp",
        group: UniqueGroup::Pistol,
    },
    UniqueWeapon {
        key: "iw4:weapon/deserteaglegold_mp",
        group: UniqueGroup::Pistol,
    },
    UniqueWeapon {
        key: "iw4:weapon/pp2000_mp",
        group: UniqueGroup::MachinePistol,
    },
    UniqueWeapon {
        key: "iw4:weapon/tmp_mp",
        group: UniqueGroup::MachinePistol,
    },
    UniqueWeapon {
        key: "iw4:weapon/beretta393_mp",
        group: UniqueGroup::MachinePistol,
    },
    UniqueWeapon {
        key: "iw4:weapon/at4_mp",
        group: UniqueGroup::Projectile,
    },
    UniqueWeapon {
        key: "iw4:weapon/m79_mp",
        group: UniqueGroup::Projectile,
    },
    UniqueWeapon {
        key: "iw4:weapon/javelin_mp",
        group: UniqueGroup::Projectile,
    },
    UniqueWeapon {
        key: "t5:weapon/galil_mp",
        group: UniqueGroup::Assault,
    },
    UniqueWeapon {
        key: "t5:weapon/commando_mp",
        group: UniqueGroup::Assault,
    },
    UniqueWeapon {
        key: "t5:weapon/g11_mp",
        group: UniqueGroup::Assault,
    },
    UniqueWeapon {
        key: "t5:weapon/spectre_mp",
        group: UniqueGroup::Smg,
    },
    UniqueWeapon {
        key: "t5:weapon/mpl_mp",
        group: UniqueGroup::Smg,
    },
    UniqueWeapon {
        key: "t5:weapon/pm63_mp",
        group: UniqueGroup::Smg,
    },
    UniqueWeapon {
        key: "t5:weapon/hk21_mp",
        group: UniqueGroup::Lmg,
    },
    UniqueWeapon {
        key: "t5:weapon/stoner63_mp",
        group: UniqueGroup::Lmg,
    },
    UniqueWeapon {
        key: "t5:weapon/rpk_mp",
        group: UniqueGroup::Lmg,
    },
    UniqueWeapon {
        key: "t5:weapon/l96a1_mp",
        group: UniqueGroup::Sniper,
    },
    UniqueWeapon {
        key: "t5:weapon/psg1_mp",
        group: UniqueGroup::Sniper,
    },
    UniqueWeapon {
        key: "t5:weapon/dragunov_mp",
        group: UniqueGroup::Sniper,
    },
    UniqueWeapon {
        key: "t5:weapon/spas_mp",
        group: UniqueGroup::Cqb,
    },
    UniqueWeapon {
        key: "t5:weapon/hs10_mp",
        group: UniqueGroup::Cqb,
    },
    UniqueWeapon {
        key: "t5:weapon/ithaca_mp",
        group: UniqueGroup::Cqb,
    },
    UniqueWeapon {
        key: "t5:weapon/python_mp",
        group: UniqueGroup::Pistol,
    },
    UniqueWeapon {
        key: "t5:weapon/cz75_mp",
        group: UniqueGroup::Pistol,
    },
    UniqueWeapon {
        key: "t5:weapon/makarov_mp",
        group: UniqueGroup::Pistol,
    },
    UniqueWeapon {
        key: "t5:weapon/knife_ballistic_mp",
        group: UniqueGroup::Special,
    },
    UniqueWeapon {
        key: "t5:weapon/crossbow_mp",
        group: UniqueGroup::Special,
    },
    UniqueWeapon {
        key: "t5:weapon/china_lake_mp",
        group: UniqueGroup::Projectile,
    },
    UniqueWeapon {
        key: "t5:weapon/m72_law_mp",
        group: UniqueGroup::Projectile,
    },
    UniqueWeapon {
        key: "t5:weapon/strela3_mp",
        group: UniqueGroup::Projectile,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_type95_mp",
        group: UniqueGroup::Assault,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_fad_mp",
        group: UniqueGroup::Assault,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_mk14_mp",
        group: UniqueGroup::Assault,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_mp7_mp",
        group: UniqueGroup::Smg,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_pp90m1_mp",
        group: UniqueGroup::Smg,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_mk46_mp",
        group: UniqueGroup::Lmg,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_pecheneg_mp",
        group: UniqueGroup::Lmg,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_mg36_mp",
        group: UniqueGroup::Lmg,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_msr_mp",
        group: UniqueGroup::Sniper,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_as50_mp",
        group: UniqueGroup::Sniper,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_rsass_mp",
        group: UniqueGroup::Sniper,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_ksg_mp",
        group: UniqueGroup::Shotgun,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_usas12_mp",
        group: UniqueGroup::Shotgun,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_1887_mp",
        group: UniqueGroup::Shotgun,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_p99_mp",
        group: UniqueGroup::Pistol,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_fnfiveseven_mp",
        group: UniqueGroup::Pistol,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_mp412_mp",
        group: UniqueGroup::Pistol,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_fmg9_mp",
        group: UniqueGroup::MachinePistol,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_g18_mp",
        group: UniqueGroup::MachinePistol,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_skorpion_mp",
        group: UniqueGroup::MachinePistol,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_xm25_mp",
        group: UniqueGroup::Projectile,
    },
    UniqueWeapon {
        key: "iw5:weapon/iw5_smaw_mp",
        group: UniqueGroup::Projectile,
    },
    UniqueWeapon {
        key: "iw5:weapon/m320_mp",
        group: UniqueGroup::Projectile,
    },
];

#[must_use]
pub fn family_stem(name: &str) -> String {
    let stem = name
        .rsplit('/')
        .next()
        .unwrap_or(name)
        .rsplit(':')
        .next()
        .unwrap_or(name);
    let stem = stem.strip_suffix("_mp").unwrap_or(stem);
    let stem = stem.strip_prefix("iw5_").unwrap_or(stem);
    match stem {
        "acr" => "masada".to_owned(),
        "usp45" => "usp".to_owned(),
        "spas" => "spas12".to_owned(),
        "1887" => "model1887".to_owned(),
        _ => stem.to_owned(),
    }
}
