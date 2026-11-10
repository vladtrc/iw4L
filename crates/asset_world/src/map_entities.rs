use fastfile_iw4::ZoneStream;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntermissionView {
    pub origin: [f32; 3],

    pub angles: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpawnPoint {
    pub classname: String,

    pub origin: [f32; 3],

    pub angles: [f32; 3],

    pub script_linkto: String,

    pub script_destructable_area: String,
}

impl SpawnPoint {
    pub fn is_initial(&self) -> bool {
        self.classname.ends_with("_start")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScriptModelId(pub(crate) u32);

impl ScriptModelId {
    pub const fn from_source_ordinal(ordinal: u32) -> Self {
        Self(ordinal)
    }

    pub const fn source_ordinal(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptBrushModelPlacement {
    pub source_ordinal: u32,

    pub cmodel_handle: u32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub targetname: String,

    pub gameobject: String,

    pub script_exploder: String,

    pub script_accumulate: Option<i32>,

    pub script_threshold: Option<i32>,

    pub script_destructable_area: String,

    pub script_fxid: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum ScriptBrushModelLink {
    #[default]
    None,
    Linked(ScriptBrushModelPlacement),

    Ambiguous {
        target: String,
        matches: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptModelPlacement {
    pub id: ScriptModelId,

    pub model: String,

    pub origin: [f32; 3],

    pub angles: [f32; 3],

    pub lighting_origin: Option<[f32; 3]>,

    pub gameobject: String,
    pub targetname: String,
    pub script_noteworthy: String,
    pub destructible_type: String,

    pub destructible_def: String,

    pub script_exploder: String,

    pub target: String,

    pub brush_link: ScriptBrushModelLink,

    pub script_accumulate: Option<i32>,

    pub script_threshold: Option<i32>,

    pub script_destructable_area: String,

    pub script_fxid: String,
}

pub fn exploding_prop_machine(
    targetname: &str,
    script_noteworthy: &str,
    destructible_type: &str,
) -> Option<&'static str> {
    if targetname.eq_ignore_ascii_case("explodable_barrel")
        || script_noteworthy.eq_ignore_ascii_case("explodable_barrel")
    {
        return Some("explodable_barrel");
    }
    if targetname.eq_ignore_ascii_case("flammable_crate")
        || script_noteworthy.eq_ignore_ascii_case("flammable_crate")
    {
        return Some("flammable_crate");
    }
    if has_ascii_prefix(destructible_type, "toy_propane") {
        return Some("propane");
    }
    if has_ascii_prefix(destructible_type, "toy_oxygen") {
        return Some("oxygen");
    }
    if destructible_type.eq_ignore_ascii_case("destructible_gaspump") {
        return Some("gaspump");
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapUseTrigger {
    pub target: String,
    pub source_ordinal: u32,
    pub classname: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub targetname: String,
    pub script_label: String,
    pub gameobject: String,
    pub radius: Option<f32>,
    pub height: Option<f32>,

    pub model: String,

    pub target_struct_angles: Option<[f32; 3]>,
}

const MAP_USE_TRIGGER_CLASSNAMES: &[&str] = &[
    "trigger_radius",
    "trigger_use_touch",
    "trigger_use",
    "trigger_multiple",
];

pub fn parse_map_use_triggers(text: &str) -> Vec<MapUseTrigger> {
    let entities: Vec<_> = parse_entities(text).collect();
    let structs: Vec<(&str, [f32; 3])> = entities
        .iter()
        .filter(|entity| entity.classname == Some("script_struct"))
        .filter_map(|entity| Some((entity.targetname?, entity.angles.unwrap_or([0.0; 3]))))
        .collect();
    entities
        .into_iter()
        .enumerate()
        .filter_map(|(ordinal, entity)| {
            let classname = entity.classname?;
            if !MAP_USE_TRIGGER_CLASSNAMES.contains(&classname) {
                return None;
            }
            let target = entity.target.unwrap_or("");
            let target_struct_angles = structs
                .iter()
                .find(|(name, _)| *name == target)
                .map(|(_, angles)| *angles);
            Some(MapUseTrigger {
                target: target.to_owned(),
                source_ordinal: u32::try_from(ordinal).ok()?,
                classname: classname.to_owned(),
                origin: entity.origin?,
                angles: entity.angles.unwrap_or([0.0; 3]),
                targetname: entity.targetname.unwrap_or("").to_owned(),
                script_label: entity.script_label.unwrap_or("").to_owned(),
                gameobject: entity.gameobject.unwrap_or("").to_owned(),
                radius: parse_finite_f32(entity.radius),
                height: parse_finite_f32(entity.height),
                model: entity.model.unwrap_or("").to_owned(),
                target_struct_angles,
            })
        })
        .collect()
}

fn parse_finite_f32(value: Option<&str>) -> Option<f32> {
    value?.trim().parse().ok().filter(|v: &f32| v.is_finite())
}

fn parse_i32(value: &str) -> Option<i32> {
    value.trim().parse().ok()
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlagDescriptor {
    pub origin: [f32; 3],
    pub script_linkname: String,
    pub script_linkto: String,
}

pub fn parse_flag_descriptors(text: &str) -> Vec<FlagDescriptor> {
    parse_entities(text)
        .filter_map(|entity| {
            if entity.targetname != Some("flag_descriptor") {
                return None;
            }
            Some(FlagDescriptor {
                origin: entity.origin?,
                script_linkname: entity.script_linkname.unwrap_or("").to_owned(),
                script_linkto: entity.script_linkto.unwrap_or("").to_owned(),
            })
        })
        .collect()
}

pub fn flag_descriptors(s: &ZoneStream<'_>) -> Vec<FlagDescriptor> {
    let Some(text) = entity_string(s) else {
        return Vec::new();
    };
    parse_flag_descriptors(text)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapScriptStruct {
    pub targetname: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub target: String,
    pub script_int: Option<i32>,
}

pub fn parse_map_script_structs(text: &str) -> Vec<MapScriptStruct> {
    parse_entities(text)
        .filter_map(|entity| {
            if entity.classname != Some("script_struct") {
                return None;
            }
            Some(MapScriptStruct {
                targetname: entity.targetname.unwrap_or("").to_owned(),
                origin: entity.origin?,
                angles: entity.angles.unwrap_or([0.0; 3]),
                target: entity.target.unwrap_or("").to_owned(),
                script_int: entity.script_int,
            })
        })
        .collect()
}

pub fn map_script_structs(s: &ZoneStream<'_>) -> Vec<MapScriptStruct> {
    let Some(text) = entity_string(s) else {
        return Vec::new();
    };
    parse_map_script_structs(text)
}

fn has_ascii_prefix(value: &str, prefix: &str) -> bool {
    value.len() >= prefix.len()
        && value.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

impl ScriptModelPlacement {
    pub fn exploding_prop_machine(&self) -> Option<&'static str> {
        exploding_prop_machine(
            &self.targetname,
            &self.script_noteworthy,
            &self.destructible_type,
        )
    }
}

pub fn intermission_view(s: &ZoneStream<'_>) -> Option<IntermissionView> {
    let text = entity_string(s)?;
    parse_intermission_view(text)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinimapCorners {
    pub a: [f32; 2],
    pub b: [f32; 2],
}

pub const REFERENCE_MP_SPAWN_CLASSNAMES: &[&str] = &[
    "mp_dm_spawn",
    "mp_dm_spawn_start",
    "mp_dom_spawn",
    "mp_dom_spawn_allies_start",
    "mp_dom_spawn_axis_start",
    "mp_dd_spawn_attacker",
    "mp_dd_spawn_attacker_a",
    "mp_dd_spawn_attacker_b",
    "mp_dd_spawn_attacker_start",
    "mp_dd_spawn_defender",
    "mp_dd_spawn_defender_a",
    "mp_dd_spawn_defender_b",
    "mp_dd_spawn_defender_start",
];

pub fn dm_spawn_points(s: &ZoneStream<'_>) -> Vec<SpawnPoint> {
    let Some(text) = entity_string(s) else {
        return Vec::new();
    };
    parse_spawn_points(text, REFERENCE_MP_SPAWN_CLASSNAMES)
}

pub fn minimap_corners(s: &ZoneStream<'_>) -> Option<MinimapCorners> {
    let text = entity_string(s)?;
    parse_minimap_corners(text)
}

pub fn worldspawn_north_yaw(s: &ZoneStream<'_>) -> Option<f32> {
    let text = entity_string(s)?;
    parse_worldspawn_north_yaw(text)
}

pub fn airstrike_height(s: &ZoneStream<'_>) -> Option<f32> {
    parse_airstrike_height(entity_string(s)?)
}

pub fn script_model_placements(s: &ZoneStream<'_>) -> Vec<ScriptModelPlacement> {
    let Some(text) = entity_string(s) else {
        return Vec::new();
    };
    parse_script_model_placements(text)
}

pub fn script_brush_model_placements(s: &ZoneStream<'_>) -> Vec<ScriptBrushModelPlacement> {
    let Some(text) = entity_string(s) else {
        return Vec::new();
    };
    parse_script_brush_model_placements(text)
}

fn entity_string<'a>(s: &'a ZoneStream<'_>) -> Option<&'a str> {
    let map_ents = s.map_ents()?;
    let ptr = map_ents.entity_string?;
    let bytes = s.slice_at(ptr, 0, map_ents.entity_chars).ok()?;
    core::str::from_utf8(bytes).ok()
}

pub fn map_ents_entity_string<'a>(s: &'a ZoneStream<'_>) -> Option<&'a str> {
    entity_string(s)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapEntsKeyCensus {
    pub quoted_pairs: u32,
    pub unquoted_named: u32,
    pub numbered: u32,
}

impl MapEntsKeyCensus {
    pub fn style(self) -> &'static str {
        match (
            self.quoted_pairs > 0,
            self.unquoted_named > 0,
            self.numbered > 0,
        ) {
            (true, false, false) => "quoted",
            (false, true, false) => "unquoted",
            (false, false, true) => "numbered",
            (false, false, false) => "none",
            _ => "mixed",
        }
    }

    pub fn report_line(self) -> String {
        format!(
            "mapents keys={} quoted={} unquoted={} numbered={}",
            self.style(),
            self.quoted_pairs,
            self.unquoted_named,
            self.numbered
        )
    }
}

pub fn census_entity_string_keys(text: &str) -> MapEntsKeyCensus {
    let mut census = MapEntsKeyCensus {
        quoted_pairs: 0,
        unquoted_named: 0,
        numbered: 0,
    };
    for line in text.lines() {
        match entity_string_pair_kind(line) {
            Some("quoted") => census.quoted_pairs += 1,
            Some("unquoted") => census.unquoted_named += 1,
            Some("numbered") => census.numbered += 1,
            _ => {}
        }
    }
    census
}

fn entity_string_pair_kind(line: &str) -> Option<&'static str> {
    let line = line.trim();
    if line.is_empty() || line == "{" || line == "}" || line.starts_with("//") {
        return None;
    }
    if quoted_pair(line).is_some() {
        return Some("quoted");
    }
    let (key, rest) = line.split_once(' ')?;
    if !rest.contains('"') {
        return None;
    }
    if key.chars().all(|c| c.is_ascii_digit()) {
        Some("numbered")
    } else {
        Some("unquoted")
    }
}

pub fn parse_intermission_view(text: &str) -> Option<IntermissionView> {
    for entity in parse_entities(text) {
        if entity.classname == Some("mp_global_intermission") {
            return Some(IntermissionView {
                origin: entity.origin?,
                angles: entity.angles.unwrap_or([0.0, 0.0, 0.0]),
            });
        }
    }
    None
}

pub fn parse_spawn_points(text: &str, classnames: &[&str]) -> Vec<SpawnPoint> {
    let mut out = Vec::new();
    for entity in parse_entities(text) {
        let Some(classname) = entity.classname else {
            continue;
        };
        if !classnames.contains(&classname) {
            continue;
        }
        let Some(origin) = entity.origin else {
            continue;
        };
        out.push(SpawnPoint {
            classname: classname.to_owned(),
            origin,
            angles: entity.angles.unwrap_or([0.0, 0.0, 0.0]),
            script_linkto: entity.script_linkto.unwrap_or("").to_owned(),
            script_destructable_area: entity.script_destructable_area.unwrap_or("").to_owned(),
        });
    }
    out
}

pub fn parse_minimap_corners(text: &str) -> Option<MinimapCorners> {
    let mut corners: Vec<[f32; 2]> = Vec::new();
    for entity in parse_entities(text) {
        if entity.targetname != Some("minimap_corner") {
            continue;
        }
        let Some(origin) = entity.origin else {
            continue;
        };
        corners.push([origin[0], origin[1]]);
    }
    if corners.len() != 2 {
        return None;
    }
    Some(MinimapCorners {
        a: corners[0],
        b: corners[1],
    })
}

pub fn parse_airstrike_height(text: &str) -> Option<f32> {
    parse_entities(text)
        .find(|e| e.targetname == Some("airstrikeheight"))
        .and_then(|e| e.origin)
        .map(|origin| origin[2])
}

pub fn parse_worldspawn_north_yaw(text: &str) -> Option<f32> {
    parse_entities(text)
        .find(|e| e.classname == Some("worldspawn"))
        .and_then(|e| e.north_yaw)
}

pub fn parse_script_model_placements(text: &str) -> Vec<ScriptModelPlacement> {
    let entities = parse_entities(text).collect::<Vec<_>>();
    let mut out = Vec::new();
    for (source_ordinal, entity) in entities.iter().enumerate() {
        if entity.classname != Some("script_model") {
            continue;
        }
        let Some(model) = entity
            .model
            .filter(|m| !m.is_empty() && !m.starts_with('*'))
        else {
            continue;
        };
        let Some(origin) = entity.origin else {
            continue;
        };
        let Ok(source_ordinal) = u32::try_from(source_ordinal) else {
            continue;
        };
        let target = entity.target.unwrap_or("");
        let linked = entities
            .iter()
            .enumerate()
            .filter_map(|(ordinal, candidate)| {
                (candidate.classname == Some("script_brushmodel")
                    && !target.is_empty()
                    && candidate.targetname == Some(target))
                .then_some((ordinal, candidate))
            })
            .filter_map(|(ordinal, candidate)| {
                let handle = candidate.model?.strip_prefix('*')?.parse().ok()?;
                Some(ScriptBrushModelPlacement {
                    source_ordinal: u32::try_from(ordinal).ok()?,
                    cmodel_handle: handle,
                    origin: candidate.origin?,
                    angles: candidate.angles.unwrap_or([0.0; 3]),
                    targetname: target.to_owned(),
                    gameobject: candidate.gameobject.unwrap_or("").to_owned(),
                    script_exploder: effective_script_exploder(
                        candidate.script_prefab_exploder,
                        candidate.script_exploder,
                    ),
                    script_accumulate: candidate.script_accumulate,
                    script_threshold: candidate.script_threshold,
                    script_destructable_area: candidate
                        .script_destructable_area
                        .unwrap_or("")
                        .to_owned(),
                    script_fxid: candidate.script_fxid.unwrap_or("").to_owned(),
                })
            })
            .collect::<Vec<_>>();
        let brush_link = match linked.len() {
            0 => ScriptBrushModelLink::None,
            1 => ScriptBrushModelLink::Linked(linked.into_iter().next().unwrap()),
            matches => ScriptBrushModelLink::Ambiguous {
                target: target.to_owned(),
                matches,
            },
        };
        out.push(ScriptModelPlacement {
            id: ScriptModelId::from_source_ordinal(source_ordinal),
            model: model.to_owned(),
            origin,
            angles: entity.angles.unwrap_or([0.0; 3]),
            lighting_origin: entity.lt_origin,
            gameobject: entity.gameobject.unwrap_or("").to_owned(),
            targetname: entity.targetname.unwrap_or("").to_owned(),
            script_noteworthy: entity.script_noteworthy.unwrap_or("").to_owned(),
            destructible_type: entity.destructible_type.unwrap_or("").to_owned(),
            destructible_def: entity.destructible_def.unwrap_or("").to_owned(),
            script_exploder: effective_script_exploder(
                entity.script_prefab_exploder,
                entity.script_exploder,
            ),
            target: target.to_owned(),
            brush_link,
            script_accumulate: entity.script_accumulate,
            script_threshold: entity.script_threshold,
            script_destructable_area: entity.script_destructable_area.unwrap_or("").to_owned(),
            script_fxid: entity.script_fxid.unwrap_or("").to_owned(),
        });
    }
    out
}

pub fn parse_script_brush_model_placements(text: &str) -> Vec<ScriptBrushModelPlacement> {
    parse_entities(text)
        .enumerate()
        .filter_map(|(ordinal, entity)| {
            if entity.classname != Some("script_brushmodel") {
                return None;
            }
            let handle = entity.model?.strip_prefix('*')?.parse().ok()?;
            if handle == 0 {
                return None;
            }
            Some(ScriptBrushModelPlacement {
                source_ordinal: u32::try_from(ordinal).ok()?,
                cmodel_handle: handle,
                origin: entity.origin?,
                angles: entity.angles.unwrap_or([0.0; 3]),
                targetname: entity.targetname.unwrap_or("").to_owned(),
                gameobject: entity.gameobject.unwrap_or("").to_owned(),
                script_exploder: effective_script_exploder(
                    entity.script_prefab_exploder,
                    entity.script_exploder,
                ),
                script_accumulate: entity.script_accumulate,
                script_threshold: entity.script_threshold,
                script_destructable_area: entity.script_destructable_area.unwrap_or("").to_owned(),
                script_fxid: entity.script_fxid.unwrap_or("").to_owned(),
            })
        })
        .collect()
}

struct RawEntity<'a> {
    classname: Option<&'a str>,
    origin: Option<[f32; 3]>,
    angles: Option<[f32; 3]>,
    model: Option<&'a str>,
    lt_origin: Option<[f32; 3]>,
    gameobject: Option<&'a str>,
    targetname: Option<&'a str>,
    script_noteworthy: Option<&'a str>,
    destructible_type: Option<&'a str>,
    destructible_def: Option<&'a str>,
    script_exploder: Option<&'a str>,
    script_prefab_exploder: Option<&'a str>,
    target: Option<&'a str>,
    north_yaw: Option<f32>,
    radius: Option<&'a str>,
    height: Option<&'a str>,
    script_label: Option<&'a str>,
    script_linkname: Option<&'a str>,
    script_linkto: Option<&'a str>,
    script_accumulate: Option<i32>,
    script_threshold: Option<i32>,
    script_destructable_area: Option<&'a str>,
    script_fxid: Option<&'a str>,
    script_int: Option<i32>,
}

fn parse_entities(text: &str) -> impl Iterator<Item = RawEntity<'_>> + '_ {
    text.split('}').filter_map(|entity| {
        let mut classname = None;
        let mut origin = None;
        let mut angles = None;
        let mut model = None;
        let mut lt_origin = None;
        let mut gameobject = None;
        let mut targetname = None;
        let mut script_noteworthy = None;
        let mut destructible_type = None;
        let mut destructible_def = None;
        let mut script_exploder = None;
        let mut script_prefab_exploder = None;
        let mut target = None;
        let mut north_yaw = None;
        let mut radius = None;
        let mut height = None;
        let mut script_label = None;
        let mut script_linkname = None;
        let mut script_linkto = None;
        let mut script_accumulate = None;
        let mut script_threshold = None;
        let mut script_destructable_area = None;
        let mut script_fxid = None;
        let mut script_int = None;
        let mut saw_key = false;
        for line in entity.lines() {
            let Some((key, value)) = entity_pair(line) else {
                continue;
            };
            saw_key = true;
            match key {
                EntityKey::Classname => classname = Some(value),
                EntityKey::Origin => origin = parse_vec3(value),
                EntityKey::Angles => angles = parse_vec3(value),
                EntityKey::Model => model = Some(value),
                EntityKey::LtOrigin => lt_origin = parse_vec3(value),
                EntityKey::Gameobject => gameobject = Some(value),
                EntityKey::Targetname => targetname = Some(value),
                EntityKey::ScriptNoteworthy => script_noteworthy = Some(value),
                EntityKey::DestructibleType => destructible_type = Some(value),
                EntityKey::DestructibleDef => destructible_def = Some(value),
                EntityKey::ScriptExploder => script_exploder = Some(value),
                EntityKey::ScriptPrefabExploder => script_prefab_exploder = Some(value),
                EntityKey::Target => target = Some(value),
                EntityKey::NorthYaw => north_yaw = value.trim().parse().ok(),
                EntityKey::Radius => radius = Some(value),
                EntityKey::Height => height = Some(value),
                EntityKey::ScriptLabel => script_label = Some(value),
                EntityKey::ScriptLinkname => script_linkname = Some(value),
                EntityKey::ScriptLinkto => script_linkto = Some(value),
                EntityKey::ScriptAccumulate => script_accumulate = parse_i32(value),
                EntityKey::ScriptThreshold => script_threshold = parse_i32(value),
                EntityKey::ScriptDestructableArea => script_destructable_area = Some(value),
                EntityKey::ScriptFxid => script_fxid = Some(value),
                EntityKey::ScriptInt => script_int = parse_i32(value),
                EntityKey::Other => {}
            }
        }
        saw_key.then_some(RawEntity {
            classname,
            origin,
            angles,
            model,
            lt_origin,
            gameobject,
            targetname,
            script_noteworthy,
            destructible_type,
            destructible_def,
            script_exploder,
            script_prefab_exploder,
            target,
            north_yaw,
            radius,
            height,
            script_label,
            script_linkname,
            script_linkto,
            script_accumulate,
            script_threshold,
            script_destructable_area,
            script_fxid,
            script_int,
        })
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntityKey {
    Classname,
    Origin,
    Angles,
    Model,
    LtOrigin,
    Gameobject,
    Targetname,
    ScriptNoteworthy,
    DestructibleType,
    DestructibleDef,
    ScriptExploder,
    ScriptPrefabExploder,
    Target,

    NorthYaw,
    Radius,
    Height,
    ScriptLabel,
    ScriptLinkname,
    ScriptLinkto,
    ScriptAccumulate,
    ScriptThreshold,
    ScriptDestructableArea,
    ScriptFxid,
    ScriptInt,
    Other,
}

fn entity_pair(line: &str) -> Option<(EntityKey, &str)> {
    if let Some((key, value)) = quoted_pair(line) {
        return Some((named_key(key), value));
    }
    numbered_pair(line)
}

fn named_key(key: &str) -> EntityKey {
    match key {
        "classname" => EntityKey::Classname,
        "origin" => EntityKey::Origin,
        "angles" => EntityKey::Angles,
        "model" => EntityKey::Model,
        "ltOrigin" => EntityKey::LtOrigin,
        "script_gameobjectname" => EntityKey::Gameobject,
        "targetname" => EntityKey::Targetname,
        "script_noteworthy" => EntityKey::ScriptNoteworthy,
        "destructible_type" => EntityKey::DestructibleType,
        "destructibledef" => EntityKey::DestructibleDef,
        "script_exploder" => EntityKey::ScriptExploder,
        "script_prefab_exploder" => EntityKey::ScriptPrefabExploder,
        "target" => EntityKey::Target,
        "northyaw" => EntityKey::NorthYaw,
        "radius" => EntityKey::Radius,
        "height" => EntityKey::Height,
        "script_label" => EntityKey::ScriptLabel,
        "script_linkname" => EntityKey::ScriptLinkname,
        "script_linkto" => EntityKey::ScriptLinkto,
        "script_accumulate" => EntityKey::ScriptAccumulate,
        "script_threshold" => EntityKey::ScriptThreshold,
        "script_destructable_area" => EntityKey::ScriptDestructableArea,
        "script_fxid" => EntityKey::ScriptFxid,
        "script_int" => EntityKey::ScriptInt,
        _ => EntityKey::Other,
    }
}

fn numbered_pair(line: &str) -> Option<(EntityKey, &str)> {
    let line = line.trim();
    let (key, rest) = line.split_once(' ')?;

    let key = named_key(key);
    let mut quotes = rest.match_indices('"').map(|(index, _)| index);
    let a = quotes.next()?;
    let b = quotes.next()?;
    Some((key, &rest[a + 1..b]))
}

fn quoted_pair(line: &str) -> Option<(&str, &str)> {
    let mut quotes = line.match_indices('"').map(|(index, _)| index);
    let a = quotes.next()?;
    let b = quotes.next()?;
    let c = quotes.next()?;
    let d = quotes.next()?;
    Some((&line[a + 1..b], &line[c + 1..d]))
}

fn effective_script_exploder(prefab: Option<&str>, exploder: Option<&str>) -> String {
    prefab.or(exploder).unwrap_or("").to_owned()
}

fn parse_vec3(value: &str) -> Option<[f32; 3]> {
    let mut values = value.split_whitespace().map(str::parse::<f32>);
    let result = [
        values.next()?.ok()?,
        values.next()?.ok()?,
        values.next()?.ok()?,
    ];
    (values.next().is_none() && result.iter().all(|value| value.is_finite())).then_some(result)
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapTriggerHull {
    pub mid: [f32; 3],
    pub half: [f32; 3],
    pub slabs: Vec<([f32; 3], f32, f32)>,
}
pub fn trigger_models(s: &ZoneStream<'_>) -> Vec<Vec<MapTriggerHull>> {
    let Some(geo) = s.map_ents() else {
        return Vec::new();
    };
    (0..geo.trigger_model_count)
        .map(|n| capture_trigger_hulls(s, geo, n).unwrap_or(Vec::new()))
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapSunStage {
    pub origin: [f32; 3],
    pub trigger: Vec<MapTriggerHull>,
    pub sun_primary_light: u8,
}

impl MapSunStage {
    pub fn contains(&self, point: [f32; 3]) -> bool {
        let p: [f32; 3] = std::array::from_fn(|i| point[i] - self.origin[i]);
        self.trigger.iter().any(|hull| {
            (0..3).all(|i| (p[i] - hull.mid[i]).abs() <= hull.half[i])
                && hull.slabs.iter().all(|&(dir, at, width)| {
                    let along: f32 = (0..3).map(|i| dir[i] * p[i]).sum();
                    (along - at).abs() <= width
                })
        })
    }
}

pub fn sun_stages(s: &ZoneStream<'_>) -> Vec<MapSunStage> {
    let Some(geo) = s.map_ents() else {
        return Vec::new();
    };
    let Some(stages) = geo.stages else {
        return Vec::new();
    };
    let stride = s.layout(asset_iw4::size::STAGE, 24);
    let ptr = s.layout(4, 8);
    (0..geo.stage_count)
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
                trigger: capture_trigger_hulls(s, geo, trigger).unwrap_or(Vec::new()),
                sun_primary_light: s.u8_at(stage, ptr + 14).ok()?,
            })
        })
        .collect()
}

fn capture_trigger_hulls(
    s: &ZoneStream<'_>,
    g: fastfile_iw4::MapEntsGeometry,
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

pub type EntityBlock = Vec<(String, String)>;
pub fn entity_value<'a>(block: &'a EntityBlock, key: &str) -> Option<&'a str> {
    block
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}
pub fn set_entity_value(block: &mut EntityBlock, key: &str, value: String) {
    match block.iter_mut().find(|(k, _)| k == key) {
        Some((_, v)) => *v = value,
        None => block.push((key.to_owned(), value)),
    }
}

#[derive(Clone, Copy)]
pub struct OvertimeEntityRule {
    pub target: &'static str,
    pub label: &'static str,
    pub replacement: &'static str,
    pub gameobject: &'static str,
}

pub struct EntityRules {
    pub overtime: Option<OvertimeEntityRule>,
    pub gametype_aliases: &'static [(&'static str, &'static str)],
    pub augment: fn(&[EntityBlock], &mut Vec<EntityBlock>),
}

pub fn normalize_map_entities(text: &str, rules: &EntityRules) -> String {
    let mut blocks: Vec<EntityBlock> = Vec::new();
    let mut open: Option<EntityBlock> = None;
    for line in text.lines() {
        match (line.trim(), open.as_mut()) {
            ("{", None) => open = Some(Vec::new()),
            ("}", Some(_)) => blocks.extend(open.take()),
            (trimmed, Some(block)) => {
                let mut quoted = trimmed.split('"').skip(1).step_by(2);
                if let (Some(key), Some(value)) = (quoted.next(), quoted.next()) {
                    block.push((key.to_owned(), value.to_owned()));
                }
            }
            _ => {}
        }
    }

    let mut flag_visuals: Vec<(String, String)> = Vec::new();
    let mut spawned: Vec<EntityBlock> = Vec::new();
    for block in &mut blocks {
        let team = entity_value(block, "script_team").map(str::to_owned);
        if let Some(suffix) =
            entity_value(block, "classname").and_then(|c| c.strip_prefix("mp_dem_spawn_"))
        {
            set_entity_value(block, "classname", format!("mp_dd_spawn_{suffix}"));
        }
        if let Some(overtime) = rules.overtime
            && entity_value(block, "targetname") == Some(overtime.target)
            && entity_value(block, "script_label") == Some(overtime.label)
        {
            set_entity_value(block, "targetname", overtime.replacement.to_owned());
            set_entity_value(
                block,
                "script_gameobjectname",
                overtime.gameobject.to_owned(),
            );
        }
        match (entity_value(block, "targetname"), team.as_deref()) {
            (Some("bombzone_dem"), _) => {
                set_entity_value(block, "targetname", "dd_bombzone".to_owned())
            }
            (Some("ctf_flag_zone_trig"), Some(team)) => {
                set_entity_value(block, "targetname", format!("ctf_zone_{team}"));
                if entity_value(block, "height").is_none() {
                    set_entity_value(block, "height", "128".to_owned());
                }
            }
            (Some("ctf_flag_pickup_trig"), Some(team)) => match entity_value(block, "target") {
                Some(target) => flag_visuals.push((target.to_owned(), format!("ctf_flag_{team}"))),
                None => {
                    spawned.push(
                        [("classname", "script_model".to_owned())]
                            .into_iter()
                            .chain(["origin", "angles"].into_iter().filter_map(|key| {
                                Some((key, entity_value(block, key)?.to_owned()))
                            }))
                            .chain([("targetname", format!("ctf_flag_{team}"))])
                            .map(|(k, v)| (k.to_owned(), v))
                            .collect(),
                    )
                }
            },
            _ => {}
        }
        if let Some(names) = entity_value(block, "script_gameobjectname") {
            let mut tokens: Vec<&str> = Vec::new();
            for token in names.split_whitespace() {
                let token = match token {
                    "bombzone_dem" => "dd_bombzone",
                    token => token,
                };
                tokens.push(token);
                tokens.extend(
                    rules
                        .gametype_aliases
                        .iter()
                        .find_map(|&(native, reference)| (native == token).then_some(reference)),
                );
            }
            let mut seen = std::collections::BTreeSet::new();
            tokens.retain(|token| seen.insert(*token));
            let names = tokens.join(" ");
            set_entity_value(block, "script_gameobjectname", names);
        }
    }
    for block in &mut blocks {
        if let Some((_, name)) = entity_value(block, "targetname")
            .and_then(|own| flag_visuals.iter().find(|(target, _)| target == own))
        {
            let name = name.clone();
            set_entity_value(block, "targetname", name);
        }
    }
    (rules.augment)(&blocks, &mut spawned);
    blocks.extend(spawned);
    if !blocks
        .iter()
        .any(|block| entity_value(block, "targetname") == Some("care_package"))
    {
        blocks.push(vec![
            ("classname".into(), "script_model".into()),
            ("targetname".into(), "care_package".into()),
            ("target".into(), "care_package_collision".into()),
        ]);
        blocks.push(vec![
            ("classname".into(), "script_brushmodel".into()),
            ("targetname".into(), "care_package_collision".into()),
        ]);
    }

    let mut out = String::with_capacity(text.len() + text.len() / 16);
    for block in blocks {
        out.push_str("{\n");
        for (key, value) in block {
            out.push_str(&format!("\"{key}\" \"{value}\"\n"));
        }
        out.push_str("}\n");
    }
    out
}

pub fn parse_reference_spawn_points(text: &str) -> Vec<SpawnPoint> {
    parse_spawn_points(text, REFERENCE_MP_SPAWN_CLASSNAMES)
}
