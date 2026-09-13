use std::collections::HashMap;
use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::class_presets::default_presets;
use crate::class_setup::picker_icon_stems;

#[derive(Resource, Clone, Debug, Default)]
pub struct UiAssetRoot(pub Option<PathBuf>);

#[derive(Resource, Default)]
pub struct ClassSelectIconCache {
    pub images: HashMap<String, Handle<Image>>,

    pub warmed: bool,
}

impl ClassSelectIconCache {
    pub fn get(&self, stem: &str) -> Option<Handle<Image>> {
        self.images.get(stem).cloned()
    }
}

pub fn warm_class_select_icons(
    root: Res<UiAssetRoot>,
    mut cache: ResMut<ClassSelectIconCache>,
    mut images: ResMut<Assets<Image>>,
) {
    if cache.warmed {
        return;
    }
    let Some(games) = root.0.as_ref() else {
        return;
    };
    cache.warmed = true;
    let mut stems = Vec::new();
    for preset in default_presets() {
        if let Some(stem) = cac_weapon_image(preset.primary) {
            stems.push(stem);
        }
        if let Some(stem) = cac_weapon_image(preset.secondary) {
            stems.push(stem);
        }
        if let Some(stem) = cac_weapon_image(preset.lethal) {
            stems.push(stem);
        }
        if let Some(stem) = cac_weapon_image(preset.tactical) {
            stems.push(stem);
        }
        for attach in preset.primary_attachments {
            if let Some(stem) = cac_attachment_image(attach) {
                stems.push(stem);
            }
        }
        for attach in preset.secondary_attachments {
            if let Some(stem) = cac_attachment_image(attach) {
                stems.push(stem);
            }
        }
        for perk in preset.perks {
            stems.push(cac_material_iwd_stem(perk.reference));
        }
        stems.push(cac_material_iwd_stem(preset.deathstreak));
    }
    stems.sort_unstable();
    stems.dedup();
    for stem in picker_icon_stems() {
        if !stems.iter().any(|s| *s == stem) {
            stems.push(stem);
        }
    }

    let mut loaded = 0usize;
    let mut missing = 0usize;
    for stem in stems {
        if cache.images.contains_key(stem) {
            continue;
        }
        match assets::decode_ui_image(games, stem) {
            Ok(Some((width, height, pixels))) => {
                let handle = images.add(rgba_ui_image(width, height, pixels));
                cache.images.insert(stem.to_owned(), handle);
                loaded += 1;
            }
            Ok(None) => {
                missing += 1;
            }
            Err(error) => {
                diag::warn!(Ui, "class icons: {stem}: {error}");
                missing += 1;
            }
        }
    }
    diag::warn!(
        Ui,
        "class icons: warmed {loaded} from IWD ({missing} missing)"
    );
}

pub(crate) fn rgba_ui_image(width: u32, height: u32, pixels: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}

pub fn cac_weapon_image(weapon_name: &str) -> Option<&'static str> {
    let weapon_name = weapon_name.rsplit('/').next().unwrap_or(weapon_name);
    let stem = weapon_name
        .strip_suffix("_mp")
        .unwrap_or(weapon_name)
        .to_ascii_lowercase();
    Some(match stem.as_str() {
        "ak47" => "weapon_ak47",
        "m4" => "weapon_m4carbine",
        "famas" => "weapon_famas",
        "scar" => "weapon_scar_h",
        "tar21" => "weapon_tavor",
        "fal" => "weapon_fnfal",
        "m16" => "weapon_m16a4",
        "masada" => "weapon_masada",
        "fn2000" => "weapon_fn2000",
        "ump45" => "weapon_ump45_iron",
        "mp5k" => "weapon_mp5k",
        "uzi" => "weapon_mini_uzi",
        "p90" => "weapon_p90",
        "kriss" => "weapon_kriss",
        "rpd" => "weapon_rpd",
        "sa80" => "weapon_sa80",
        "mg4" => "weapon_mg4",
        "m240" => "weapon_m240",
        "aug" => "weapon_steyraug",
        "barrett" => "weapon_barrett50cal",
        "cheytac" => "weapon_cheytac_scope",
        "wa2000" => "weapon_wa2000",
        "m21" => "weapon_m14ebr",
        "ranger" => "weapon_ranger",
        "model1887" => "weapon_model1887",
        "striker" => "weapon_striker",
        "aa12" => "weapon_aa12",
        "m1014" => "weapon_benelli_m4",
        "spas12" => "weapon_spas12",
        "usp" => "weapon_usp_45",
        "beretta" => "weapon_m9beretta",
        "deserteagle" => "weapon_desert_eagle",
        "coltanaconda" => "weapon_colt_anaconda",
        "glock" => "weapon_glock",
        "beretta393" => "weapon_beretta393",
        "pp2000" => "weapon_pp2000",
        "tmp" => "weapon_tmp",
        "at4" => "weapon_at4",
        "rpg" => "weapon_rpg7",
        "stinger" => "weapon_stinger",
        "javelin" => "weapon_javelin",
        "riotshield" => "weapon_riot_shield",
        "semtex" => "cardicon_semtex",
        "frag_grenade" => "weapon_fraggrenade",
        "throwingknife" => "cardicon_throwing_knive",
        "claymore" => "weapon_claymore",
        "c4" => "weapon_c4",
        "flash_grenade" => "weapon_flashbang",
        "smoke_grenade" => "weapon_smokegrenade",
        "concussion_grenade" => "weapon_concgrenade",
        "flare" => "specialty_tactical_insert",
        _ => return None,
    })
}

pub fn cac_material_iwd_stem(material: &str) -> &str {
    match material {
        "specialty_onemanarmy" => "specialty_one_man_army",
        "specialty_coldblooded" => "specialty_cold_blooded",
        "specialty_dangerclose" => "specialty_danger_close",
        "specialty_localjammer" => "specialty_scrambler",
        "equipment_frag" => "weapon_fraggrenade",
        "equipment_semtex" => "cardicon_semtex",
        "equipment_c4" => "weapon_c4",
        "equipment_claymore" => "weapon_claymore",
        "equipment_throwing_knife" => "cardicon_throwing_knive",
        "equipment_flare" => "specialty_tactical_insert",
        "weapon_semtex" => "cardicon_semtex",
        "weapon_cheytac" => "weapon_cheytac_scope",
        "killiconmelee" => "cardicon_throwing_knive",
        other => other,
    }
}

pub fn cac_attachment_image(token: &str) -> Option<&'static str> {
    let token = token.rsplit('/').next().unwrap_or(token);
    let normalized = token
        .trim()
        .strip_suffix("_mp")
        .unwrap_or(token.trim())
        .to_ascii_lowercase();
    let token = normalized
        .split('_')
        .rev()
        .find(|part| attachment_material(part).is_some())
        .unwrap_or(normalized.as_str());
    attachment_material(token)
}

fn attachment_material(token: &str) -> Option<&'static str> {
    Some(match token {
        "acog" => "weapon_attachment_acog",
        "eotech" => "weapon_attachment_eotech",
        "reflex" => "weapon_attachment_reflex",
        "thermal" => "weapon_attachment_thermal",
        "silencer" | "suppressor" => "weapon_attachment_suppressor",
        "grip" => "weapon_attachment_grip",
        "fmj" => "weapon_attachment_fmj",
        "xmags" | "mags" => "weapon_attachment_mags",
        "gl" | "m203" | "gp25" => "weapon_attachment_m203",
        "shotgun" => "weapon_attachment_shotgun",
        "heartbeat" => "weapon_attachment_heartbeat",
        "akimbo" => "weapon_attachment_akimbo",
        "tactical" => "weapon_attachment_tactical",
        _ => return None,
    })
}

pub fn pretty_weapon_name(weapon: &str) -> String {
    let weapon = weapon.rsplit('/').next().unwrap_or(weapon);
    weapon
        .strip_suffix("_mp")
        .unwrap_or(weapon)
        .replace('_', " ")
        .to_ascii_uppercase()
}
