use std::path::Path;

use bevy::prelude::Resource;

use crate::CapturedStringTable;

pub const FACTION_ICON_COL: i32 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaCharsets {
    pub map: String,
    pub allieschar: Option<String>,
    pub axischar: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TeamIcons {
    pub allies: Option<String>,
    pub axis: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Resource)]
pub struct SessionTeamIcons(pub TeamIcons);

fn table_icon(table: &CapturedStringTable, charset: Option<&str>) -> Option<String> {
    let charset = charset.filter(|s| !s.is_empty())?;
    let cell = table.lookup_col(charset, FACTION_ICON_COL);
    (!cell.is_empty()).then(|| cell.to_owned())
}

#[must_use]
pub fn team_icons(
    table: &CapturedStringTable,
    allieschar: Option<&str>,
    axischar: Option<&str>,
) -> TeamIcons {
    TeamIcons {
        allies: table_icon(table, allieschar),
        axis: table_icon(table, axischar),
    }
}

#[must_use]
pub fn team_icons_for_zone(
    table: &CapturedStringTable,
    arena_text: Option<&str>,
    zone: &str,
) -> TeamIcons {
    if zone.is_empty() {
        return TeamIcons::default();
    }
    let Some(row) = arena_text.and_then(|text| arena_charsets(text, zone)) else {
        return TeamIcons::default();
    };
    team_icons(table, row.allieschar.as_deref(), row.axischar.as_deref())
}

fn is_basemaps_arena_name(name: &str) -> bool {
    let n = name.replace('\\', "/");
    n.eq_ignore_ascii_case("mp/basemaps.arena")
        || n.rsplit('/')
            .next()
            .is_some_and(|leaf| leaf.eq_ignore_ascii_case("basemaps.arena"))
}

fn is_faction_table_name(name: &str) -> bool {
    name.replace('\\', "/")
        .eq_ignore_ascii_case("mp/factiontable.csv")
}

pub fn load_iw5_team_icon_sources(
    zone_path: &Path,
) -> (Option<String>, Option<CapturedStringTable>) {
    let Ok(image) = crate::open_zone(zone_path) else {
        return (None, None);
    };
    let Ok(header) = image.iw5_header() else {
        return (None, None);
    };
    let mut memory = crate::Iw5ZoneMemory::for_header(&header);
    let Ok(mut stream) = memory.stream(&image.bytes) else {
        return (None, None);
    };
    let mut sink = Iw5TeamIconSink::default();
    let _ = fastfile_iw5::load_zone(&mut stream, &mut sink);
    (sink.arena_text, sink.faction_table)
}

#[derive(Default)]
struct Iw5TeamIconSink {
    arena_text: Option<String>,
    faction_table: Option<CapturedStringTable>,
}

impl fastfile_iw5::AssetSink for Iw5TeamIconSink {
    fn set_script_strings(&mut self, _strings: fastfile_iw5::ScriptStrings) {}
    fn load_asset(
        &mut self,
        s: &mut fastfile_iw5::ZoneStream<'_>,
        _index: usize,
        ty: fastfile_iw5::AssetType,
        slot: fastfile_iw5::Ptr,
    ) -> fastfile_iw5::Result<()> {
        fastfile_iw5::load_asset_at_observed(s, ty, slot, self)?;
        Ok(())
    }
}

impl fastfile_iw5::AssetLinkSink for Iw5TeamIconSink {
    fn loaded(
        &mut self,
        _s: &fastfile_iw5::ZoneStream<'_>,
        _ty: fastfile_iw5::AssetType,
        _slot: fastfile_iw5::Ptr,
        _insert: Option<fastfile_iw5::Ptr>,
    ) -> fastfile_iw5::Result<()> {
        Ok(())
    }
    fn alias(
        &mut self,
        _ty: fastfile_iw5::AssetType,
        _slot: fastfile_iw5::Ptr,
        _target: fastfile_iw5::Ptr,
    ) -> fastfile_iw5::Result<()> {
        Ok(())
    }
    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib: bool,
    ) -> fastfile_iw5::Result<()> {
        if is_basemaps_arena_name(name) {
            if let Some(text) = crate::decode_rawfile_text(data, zlib) {
                self.arena_text = Some(text);
            }
        }
        Ok(())
    }
    fn capture_string_table(
        &mut self,
        s: &fastfile_iw5::ZoneStream<'_>,
        header: fastfile_iw5::Ptr,
    ) -> fastfile_iw5::Result<()> {
        let name = match s.ptr_at(header, 0).ok() {
            Some(fastfile_iw5::ZonePtr::Offset(p)) => {
                s.cstr(s.resolve_alias(p)).ok().unwrap_or("").to_owned()
            }
            _ => return Ok(()),
        };
        if !is_faction_table_name(&name) {
            return Ok(());
        }
        let columns = s.i32_at(header, s.layout(4, 8)).unwrap_or(0).max(0) as usize;
        let rows = s.i32_at(header, s.layout(8, 12)).unwrap_or(0).max(0) as usize;
        let cells_n = columns.saturating_mul(rows);
        let arr = match s.ptr_at(header, s.layout(12, 16)).ok() {
            Some(fastfile_iw5::ZonePtr::Offset(p)) => s.resolve_alias(p),
            _ => {
                self.faction_table = Some(CapturedStringTable {
                    name,
                    columns,
                    rows,
                    cells: Vec::new(),
                });
                return Ok(());
            }
        };
        let mut cells = Vec::with_capacity(cells_n);
        let cell_sz = s.layout(fastfile_iw5::size::STRING_TABLE_CELL, 16);
        for i in 0..cells_n {
            let cell = arr.at(i * cell_sz);
            let value = match s.ptr_at(cell, 0).ok() {
                Some(fastfile_iw5::ZonePtr::Offset(p)) => {
                    s.cstr(s.resolve_alias(p)).ok().unwrap_or("").to_owned()
                }
                _ => String::new(),
            };
            cells.push(value);
        }
        self.faction_table = Some(CapturedStringTable {
            name,
            columns,
            rows,
            cells,
        });
        Ok(())
    }
}

#[must_use]
pub fn arena_charsets(text: &str, map: &str) -> Option<ArenaCharsets> {
    parse_arena(text)
        .into_iter()
        .find(|row| row.map.eq_ignore_ascii_case(map))
}

#[must_use]
pub fn parse_arena(text: &str) -> Vec<ArenaCharsets> {
    let mut rows = Vec::new();
    for block in text.split('{').skip(1) {
        let body = block.split('}').next().unwrap_or(block);
        let mut map = None;
        let mut allieschar = None;
        let mut axischar = None;
        for raw in body.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(key) = parts.next() else {
                continue;
            };
            let value = unquote(parts.collect::<Vec<_>>().join(" ").as_str());
            if value.is_empty() {
                continue;
            }
            match key {
                "map" => map = Some(value),
                "allieschar" => allieschar = Some(value),
                "axischar" => axischar = Some(value),
                _ => {}
            }
        }
        if let Some(map) = map {
            rows.push(ArenaCharsets {
                map,
                allieschar,
                axischar,
            });
        }
    }
    rows
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').to_owned()
}

#[must_use]
pub fn t5_teamset_key_from_rawfile(name: &str) -> Option<&str> {
    let file = name.rsplit(['/', '\\']).next()?;
    let stem = file.strip_prefix("_teamset_")?.strip_suffix(".gsc")?;
    (!stem.is_empty()).then_some(stem)
}

#[must_use]
pub fn t5_teamset_from_map_gsc(script: &str) -> Option<&str> {
    let lower = script.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("_teamset_") {
        let start = from + rel + "_teamset_".len();
        let Some(rest) = script.get(start..) else {
            return None;
        };
        let Some(end) = rest.find("::") else {
            from = start;
            continue;
        };
        let name = rest[..end].trim();
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            from = start + end + 2;
            continue;
        }
        if rest[end..].to_ascii_lowercase().starts_with("::level_init") {
            return Some(name);
        }
        from = start + end + 2;
    }
    None
}

#[must_use]
pub fn t5_icons_from_teamset_gsc(script: &str) -> TeamIcons {
    TeamIcons {
        allies: crate::game_nested_string_assignment(script, &["icons", "allies"])
            .map(str::to_owned),
        axis: crate::game_nested_string_assignment(script, &["icons", "axis"]).map(str::to_owned),
    }
}

#[must_use]
pub fn t5_teamset_from_rawfile(name: &str, data: &[u8], zlib_compressed: bool) -> Option<String> {
    if !asset_audio::is_map_main_script(name) {
        return None;
    }
    let text = crate::decode_rawfile_text(data, zlib_compressed)?;
    t5_teamset_from_map_gsc(&text).map(str::to_owned)
}

#[must_use]
pub fn t5_icons_from_teamset_rawfile(
    name: &str,
    data: &[u8],
    zlib_compressed: bool,
) -> Option<(String, TeamIcons)> {
    let key = t5_teamset_key_from_rawfile(name)?.to_owned();
    let text = crate::decode_rawfile_text(data, zlib_compressed)?;
    let icons = t5_icons_from_teamset_gsc(&text);
    (icons.allies.is_some() || icons.axis.is_some()).then_some((key, icons))
}

pub fn read_basemaps_arena(games_root: &Path) -> Option<String> {
    read_iwd_named(games_root, "mp/basemaps.arena").and_then(|bytes| String::from_utf8(bytes).ok())
}

pub fn read_iwd_named(games_root: &Path, want: &str) -> Option<Vec<u8>> {
    asset_transport::read_iwd_named(games_root, want)
}
