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

pub const REFERENCE_BOT_WEAPONS: &[UniqueWeapon] = &[
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

/// Installed weapon pools, visited in family-token order.
#[derive(Default)]
pub struct BotWeaponFamilies {
    families: std::collections::BTreeMap<asset_core::FamilyId, &'static [UniqueWeapon]>,
}

impl BotWeaponFamilies {
    pub fn register(
        &mut self,
        id: asset_core::FamilyId,
        weapons: &'static [UniqueWeapon],
    ) -> Result<(), &'static str> {
        if self.families.contains_key(&id) {
            return Err("duplicate bot weapon family");
        }
        for weapon in weapons {
            if weapon.key.split_once(':').map(|(owner, _)| owner) != Some(id.as_str()) {
                return Err("bot weapon belongs to another family");
            }
        }
        self.families.insert(id, weapons);
        Ok(())
    }
    pub fn iter(&self) -> impl Iterator<Item = &'static UniqueWeapon> + '_ {
        self.families.values().flat_map(|rows| rows.iter())
    }
}

static BOT_WEAPONS: std::sync::OnceLock<BotWeaponFamilies> = std::sync::OnceLock::new();

pub fn install_bot_weapon_families(families: BotWeaponFamilies) -> Result<(), &'static str> {
    BOT_WEAPONS
        .set(families)
        .map_err(|_| "bot weapon families already installed")
}

pub fn bot_weapons() -> impl Iterator<Item = &'static UniqueWeapon> {
    BOT_WEAPONS
        .get()
        .expect("builder must install bot weapons")
        .iter()
}
