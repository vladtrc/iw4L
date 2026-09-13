use anim_iw4::xmodel_no_scale_bit;

use crate::weapon_catalog::{LoadoutCatalogKind, WeaponRegistry};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentVisual {
    pub name: String,

    pub show_tags: Vec<String>,

    pub hide_tags: Vec<String>,
}

pub fn attachment_visual_from_hide_tags(
    token: &str,
    bare_hide: &[String],
    variant_hide: &[String],
) -> AttachmentVisual {
    let show_tags = bare_hide
        .iter()
        .filter(|tag| {
            !variant_hide
                .iter()
                .any(|hidden| hidden.eq_ignore_ascii_case(tag))
        })
        .cloned()
        .collect();
    let hide_tags = variant_hide
        .iter()
        .filter(|tag| {
            !bare_hide
                .iter()
                .any(|hidden| hidden.eq_ignore_ascii_case(tag))
        })
        .cloned()
        .collect();
    AttachmentVisual {
        name: token.to_owned(),
        show_tags,
        hide_tags,
    }
}

pub fn attachment_catalog(registry: &WeaponRegistry, weapon_id: u32) -> Vec<AttachmentVisual> {
    let bare_id = bare_weapon_id(registry, weapon_id).unwrap_or(weapon_id);
    let bare_name = registry.name_of(bare_id);

    let base = bare_name.strip_suffix("_mp").unwrap_or(bare_name);
    if base.is_empty() {
        return Vec::new();
    }
    let prefix = format!("{base}_");
    let bare_hide = registry.hide_tags_of(bare_id);
    let bare_gun = registry.gun_xmodel_of(bare_id);
    let bare_def = registry.weap_def_of(bare_id);

    let mut out = Vec::new();
    for row in registry.loadout_catalog() {
        let LoadoutCatalogKind::AttachmentVariant { base_id } = row.kind else {
            continue;
        };
        if base_id != bare_id {
            continue;
        }
        let Some(token) = row.name.strip_prefix(&prefix) else {
            continue;
        };
        let token = token.strip_suffix("_mp").unwrap_or(token);

        if token.is_empty() || token.contains('_') {
            continue;
        }
        if let (Some(bare_g), Some(var_g)) = (bare_gun, registry.gun_xmodel_of(row.id)) {
            if bare_g != var_g {
                continue;
            }
        }
        if let (Some(bare_d), Some(var_d)) = (bare_def, registry.weap_def_of(row.id)) {
            if bare_d != var_d {
                continue;
            }
        }
        out.push(attachment_visual_from_hide_tags(
            token,
            bare_hide,
            registry.hide_tags_of(row.id),
        ));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn resolve_weapon_for_attachments(
    registry: &WeaponRegistry,
    bare_id: u32,
    tokens: &[String],
) -> Option<u32> {
    let bare_name = registry.name_of(bare_id);
    let base = bare_name.strip_suffix("_mp").unwrap_or(bare_name);
    if base.is_empty() {
        return None;
    }
    if tokens.is_empty() {
        return Some(bare_id);
    }
    let mut wanted: Vec<String> = tokens.iter().map(|t| t.to_ascii_lowercase()).collect();
    wanted.sort_unstable();
    wanted.dedup();

    let bare_def = registry.weap_def_of(bare_id);
    let bare_namespace = registry.namespace_of(bare_id);
    let prefix = format!("{base}_");
    for id in 1..=registry.len() as u32 {
        if registry.namespace_of(id) != bare_namespace {
            continue;
        }
        let name = registry.name_of(id);
        let Some(mid) = name.strip_prefix(&prefix) else {
            continue;
        };
        let mid = mid.strip_suffix("_mp").unwrap_or(mid);
        if let (Some(bare_d), Some(var_d)) = (bare_def, registry.weap_def_of(id)) {
            if bare_d != var_d {
                continue;
            }
        }
        let mut parts: Vec<String> = mid
            .split('_')
            .filter(|p| !p.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
        parts.sort_unstable();
        if parts == wanted {
            return Some(id);
        }
    }
    None
}

pub fn bare_weapon_id(registry: &WeaponRegistry, weapon_id: u32) -> Option<u32> {
    for row in registry.loadout_catalog() {
        if row.id == weapon_id {
            return match row.kind {
                LoadoutCatalogKind::AttachmentVariant { base_id } => Some(base_id),
                _ => Some(weapon_id),
            };
        }
    }
    if weapon_id != 0 && !registry.name_of(weapon_id).is_empty() {
        Some(weapon_id)
    } else {
        None
    }
}

pub fn effective_hide_tags(registry: &WeaponRegistry, weapon_id: u32) -> Vec<String> {
    registry.hide_tags_of(weapon_id).to_vec()
}

pub fn bone_has_hidden_ancestor(
    bone_names: &[String],
    parent_of: impl Fn(usize) -> Option<usize>,
    bone: usize,
    hidden_tags: &[String],
) -> bool {
    let mut current = Some(bone);
    while let Some(index) = current {
        if bone_names
            .get(index)
            .is_some_and(|name| hidden_tags.iter().any(|t| t.eq_ignore_ascii_case(name)))
        {
            return true;
        }
        current = parent_of(index);
    }
    false
}

pub fn surface_visible(
    part_bits: &[u32; 6],
    bone_names: &[String],
    parent_of: impl Fn(usize) -> Option<usize>,
    hidden_tags: &[String],
) -> bool {
    if hidden_tags.is_empty() {
        return true;
    }
    !(0..bone_names.len()).any(|bone| {
        xmodel_no_scale_bit(part_bits, bone)
            && bone_has_hidden_ancestor(bone_names, &parent_of, bone, hidden_tags)
    })
}
