use asset_world::*;

pub fn flag_descriptors_iw5(s: &ext_iw5_fastfile::ZoneStream<'_>) -> Vec<FlagDescriptor> {
    let Some(text) = entity_string_iw5(s) else {
        return Vec::new();
    };
    parse_flag_descriptors(&decode_entity_keys(text))
}

pub fn map_script_structs_iw5(s: &ext_iw5_fastfile::ZoneStream<'_>) -> Vec<MapScriptStruct> {
    let Some(text) = entity_string_iw5(s) else {
        return Vec::new();
    };
    parse_map_script_structs(&decode_entity_keys(text))
}

pub fn map_use_triggers_iw5(s: &ext_iw5_fastfile::ZoneStream<'_>) -> Vec<MapUseTrigger> {
    let Some(text) = entity_string_iw5(s) else {
        return Vec::new();
    };
    parse_map_use_triggers(&decode_entity_keys(text))
}

pub fn airstrike_height_iw5(s: &ext_iw5_fastfile::ZoneStream<'_>) -> Option<f32> {
    parse_airstrike_height(&decode_entity_keys(entity_string_iw5(s)?))
}

pub fn minimap_corners_iw5(s: &ext_iw5_fastfile::ZoneStream<'_>) -> Option<MinimapCorners> {
    let text = entity_string_iw5(s)?;
    parse_minimap_corners(&decode_entity_keys(text))
}

pub fn worldspawn_north_yaw_iw5(s: &ext_iw5_fastfile::ZoneStream<'_>) -> Option<f32> {
    parse_worldspawn_north_yaw(&decode_entity_keys(entity_string_iw5(s)?))
}

pub fn dm_spawn_points_iw5(s: &ext_iw5_fastfile::ZoneStream<'_>) -> Vec<SpawnPoint> {
    let Some(text) = entity_string_iw5(s) else {
        return Vec::new();
    };
    parse_spawn_points(&decode_entity_keys(text), REFERENCE_MP_SPAWN_CLASSNAMES)
}

pub fn intermission_view_iw5(s: &ext_iw5_fastfile::ZoneStream<'_>) -> Option<IntermissionView> {
    let text = entity_string_iw5(s)?;
    parse_intermission_view(&decode_entity_keys(text))
}

pub fn script_model_placements_iw5(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
) -> Vec<ScriptModelPlacement> {
    let Some(text) = entity_string_iw5(s) else {
        return Vec::new();
    };
    parse_script_model_placements(&decode_entity_keys(text))
}

pub fn script_brush_model_placements_iw5(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
) -> Vec<ScriptBrushModelPlacement> {
    let Some(text) = entity_string_iw5(s) else {
        return Vec::new();
    };
    parse_script_brush_model_placements(&decode_entity_keys(text))
}

pub fn map_ents_entity_string_iw5<'a>(s: &'a ext_iw5_fastfile::ZoneStream<'_>) -> Option<&'a str> {
    entity_string_iw5(s)
}

fn entity_string_iw5<'a>(s: &'a ext_iw5_fastfile::ZoneStream<'_>) -> Option<&'a str> {
    let map_ents = s.map_ents()?;
    let ptr = map_ents.entity_string?;
    let bytes = s.slice_at(ptr, 0, map_ents.entity_chars).ok()?;
    core::str::from_utf8(bytes).ok()
}

pub fn sun_stages_iw5(s: &ext_iw5_fastfile::ZoneStream<'_>) -> Vec<MapSunStage> {
    let Some(clip) = s.clip_map() else {
        return Vec::new();
    };
    let Some(stages) = clip.stages else {
        return Vec::new();
    };
    let stride = s.layout(ext_iw5_fastfile::size::STAGE, 24);
    let ptr = s.layout(4, 8);
    (0..clip.stage_count)
        .map_while(|n| {
            let stage = stages.at(n * stride);
            let origin = [
                s.f32_at(stage, ptr).ok()?,
                s.f32_at(stage, ptr + 4).ok()?,
                s.f32_at(stage, ptr + 8).ok()?,
            ];
            let trigger = usize::from(s.u16_at(stage, ptr + 12).ok()?);
            Some(MapSunStage {
                origin,
                trigger: capture_iw5_trigger_hulls(s, clip.stage_trigger, trigger)
                    .unwrap_or(Vec::new()),
                sun_primary_light: s.u8_at(stage, ptr + 14).ok()?,
            })
        })
        .collect()
}

pub fn trigger_models_iw5(s: &ext_iw5_fastfile::ZoneStream<'_>) -> Vec<Vec<MapTriggerHull>> {
    let Some(geo) = s.map_ents() else {
        return Vec::new();
    };
    (0..geo.trigger_model_count)
        .map(|n| capture_iw5_trigger_hulls(s, geo, n).unwrap_or(Vec::new()))
        .collect()
}

fn capture_iw5_trigger_hulls(
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    g: ext_iw5_fastfile::MapEntsGeometry,
    index: usize,
) -> Option<Vec<MapTriggerHull>> {
    if index >= g.trigger_model_count {
        return None;
    }
    let model = g.trigger_models?.at(index * 8);
    let count = usize::from(s.u16_at(model, 4).ok()?);
    let first = usize::from(s.u16_at(model, 6).ok()?);
    if first.checked_add(count)? > g.trigger_hull_count || count == 0 {
        return None;
    }
    let mut out = Vec::new();
    for n in first..first + count {
        let hull = g.trigger_hulls?.at(n * 32);
        let mid = [
            s.f32_at(hull, 0).ok()?,
            s.f32_at(hull, 4).ok()?,
            s.f32_at(hull, 8).ok()?,
        ];
        let half = [
            s.f32_at(hull, 12).ok()?,
            s.f32_at(hull, 16).ok()?,
            s.f32_at(hull, 20).ok()?,
        ];
        let count = usize::from(s.u16_at(hull, 28).ok()?);
        let first = usize::from(s.u16_at(hull, 30).ok()?);
        if first.checked_add(count)? > g.trigger_slab_count {
            return None;
        }
        let mut slabs = Vec::new();
        for i in first..first + count {
            let slab = g.trigger_slabs?.at(i * 20);
            slabs.push((
                [
                    s.f32_at(slab, 0).ok()?,
                    s.f32_at(slab, 4).ok()?,
                    s.f32_at(slab, 8).ok()?,
                ],
                s.f32_at(slab, 12).ok()?,
                s.f32_at(slab, 16).ok()?,
            ));
        }
        out.push(MapTriggerHull { mid, half, slabs });
    }
    Some(out)
}

const IW5_KEYS: &[(&str, &str)] = &[
    ("1668", "classname"),
    ("1669", "origin"),
    ("1670", "model"),
    ("1671", "spawnflags"),
    ("1672", "target"),
    ("1673", "targetname"),
    ("1676", "dmg"),
    ("1677", "angles"),
    ("1679", "script_linkname"),
    ("1774", "script_noteworthy"),
    ("1782", "radius"),
    ("1783", "height"),
    ("2009", "script_exploder"),
    ("2328", "script_linkto"),
    ("2369", "destructible_type"),
    ("2814", "ltOrigin"),
    ("7864", "script_prefab_exploder"),
    ("11039", "animation"),
    ("11848", "script_gameobjectname"),
    ("11996", "script_label"),
    ("12906", "script_type"),
];

fn iw5_key_name(key: &str) -> Option<&'static str> {
    IW5_KEYS
        .iter()
        .find(|(id, _)| *id == key)
        .map(|(_, name)| *name)
}

pub fn decode_entity_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    for line in text.lines() {
        let trimmed = line.trim();
        match trimmed.split_once(' ') {
            Some((key, value)) if key.bytes().all(|b| b.is_ascii_digit()) && !key.is_empty() => {
                let name = iw5_key_name(key).unwrap_or(key);
                out.push('"');
                out.push_str(name);
                out.push_str("\" ");
                out.push_str(value.trim());
            }
            _ => out.push_str(trimmed),
        }
        out.push('\n');
    }
    out
}
