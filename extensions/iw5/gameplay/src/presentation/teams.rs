use crate::zone::decoder::{Iw5ZoneImageExt, Iw5ZoneMemory};
use asset_game::*;
use std::path::Path;
pub const POLICY: TeamFamily = TeamFamily {
    bind_objectives: true,
    allow_absent_flag_base_fx: false,
    icon_alias_suffixes: &[],
    announcer: Some(super::announcer::route),
    objectives: None,
    sources: Some(load_iw5_team_sources),
};
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

pub fn load_iw5_team_sources(zone_path: &Path) -> (Option<String>, Option<CapturedStringTable>) {
    let Ok(image) = asset_transport::open_zone(zone_path) else {
        return (None, None);
    };
    let Ok(header) = image.iw5_header() else {
        return (None, None);
    };
    let mut memory = Iw5ZoneMemory::for_header(&header);
    let Ok(mut stream) = memory.stream(&image.bytes) else {
        return (None, None);
    };
    let mut sink = Iw5TeamIconSink::default();
    let _ = ext_iw5_fastfile::load_zone(&mut stream, &mut sink);
    (sink.arena_text, sink.faction_table)
}

#[derive(Default)]
struct Iw5TeamIconSink {
    arena_text: Option<String>,
    faction_table: Option<CapturedStringTable>,
}

impl ext_iw5_fastfile::AssetSink for Iw5TeamIconSink {
    fn set_script_strings(&mut self, _strings: ext_iw5_fastfile::ScriptStrings) {}
    fn load_asset(
        &mut self,
        s: &mut ext_iw5_fastfile::ZoneStream<'_>,
        _index: usize,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        ext_iw5_fastfile::load_asset_at_observed(s, ty, slot, self)?;
        Ok(())
    }
}

impl ext_iw5_fastfile::AssetLinkSink for Iw5TeamIconSink {
    fn loaded(
        &mut self,
        _s: &ext_iw5_fastfile::ZoneStream<'_>,
        _ty: ext_iw5_fastfile::AssetType,
        _slot: ext_iw5_fastfile::Ptr,
        _insert: Option<ext_iw5_fastfile::Ptr>,
    ) -> ext_iw5_fastfile::Result<()> {
        Ok(())
    }
    fn alias(
        &mut self,
        _ty: ext_iw5_fastfile::AssetType,
        _slot: ext_iw5_fastfile::Ptr,
        _target: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        Ok(())
    }
    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib: bool,
    ) -> ext_iw5_fastfile::Result<()> {
        if is_basemaps_arena_name(name) {
            if let Some(text) = asset_world::decode_rawfile_text(data, zlib) {
                self.arena_text = Some(text);
            }
        }
        Ok(())
    }
    fn capture_string_table(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        header: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        let name = match s.ptr_at(header, 0).ok() {
            Some(ext_iw5_fastfile::ZonePtr::Offset(p)) => {
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
            Some(ext_iw5_fastfile::ZonePtr::Offset(p)) => s.resolve_alias(p),
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
        let cell_sz = s.layout(ext_iw5_fastfile::size::STRING_TABLE_CELL, 16);
        for i in 0..cells_n {
            let cell = arr.at(i * cell_sz);
            let value = match s.ptr_at(cell, 0).ok() {
                Some(ext_iw5_fastfile::ZonePtr::Offset(p)) => {
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
