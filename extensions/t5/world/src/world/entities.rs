use asset_world::*;

pub fn flag_descriptors_t5(s: &ext_t5_fastfile::ZoneStream<'_>) -> Vec<FlagDescriptor> {
    let Some(text) = entity_string_t5(s) else {
        return Vec::new();
    };
    parse_flag_descriptors(text)
}

pub fn map_script_structs_t5(s: &ext_t5_fastfile::ZoneStream<'_>) -> Vec<MapScriptStruct> {
    let Some(text) = entity_string_t5(s) else {
        return Vec::new();
    };
    parse_map_script_structs(text)
}

pub fn map_use_triggers_t5(s: &ext_t5_fastfile::ZoneStream<'_>) -> Vec<MapUseTrigger> {
    let Some(text) = entity_string_t5(s) else {
        return Vec::new();
    };
    parse_map_use_triggers(text)
}

pub fn minimap_corners_t5(s: &ext_t5_fastfile::ZoneStream<'_>) -> Option<MinimapCorners> {
    let text = entity_string_t5(s)?;
    parse_minimap_corners(text)
}

pub fn worldspawn_north_yaw_t5(s: &ext_t5_fastfile::ZoneStream<'_>) -> Option<f32> {
    parse_worldspawn_north_yaw(entity_string_t5(s)?)
}

pub fn intermission_view_t5(s: &ext_t5_fastfile::ZoneStream<'_>) -> Option<IntermissionView> {
    let text = entity_string_t5(s)?;
    parse_intermission_view(text)
}

pub fn dm_spawn_points_t5(s: &ext_t5_fastfile::ZoneStream<'_>) -> Vec<SpawnPoint> {
    let Some(text) = entity_string_t5(s) else {
        return Vec::new();
    };
    super::policy::spawn_points(text)
}

pub fn script_model_placements_t5(
    s: &ext_t5_fastfile::ZoneStream<'_>,
) -> Vec<ScriptModelPlacement> {
    let Some(text) = entity_string_t5(s) else {
        return Vec::new();
    };
    parse_script_model_placements(text)
}

pub fn script_brush_model_placements_t5(
    s: &ext_t5_fastfile::ZoneStream<'_>,
) -> Vec<ScriptBrushModelPlacement> {
    let Some(text) = entity_string_t5(s) else {
        return Vec::new();
    };
    parse_script_brush_model_placements(text)
}

pub fn map_ents_entity_string_t5<'a>(s: &'a ext_t5_fastfile::ZoneStream<'_>) -> Option<&'a str> {
    entity_string_t5(s)
}

fn entity_string_t5<'a>(s: &'a ext_t5_fastfile::ZoneStream<'_>) -> Option<&'a str> {
    let map_ents = s.map_ents()?;
    let ptr = map_ents.entity_string?;
    let bytes = s.slice_at(ptr, 0, map_ents.entity_chars).ok()?;
    core::str::from_utf8(bytes).ok()
}
