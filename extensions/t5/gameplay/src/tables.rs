pub fn is_cac_table(name: &str) -> bool {
    asset_game::is_stats_table_name(name)
        || name.eq_ignore_ascii_case("mp/perkTable.csv")
        || name.eq_ignore_ascii_case("mp/rankTable.csv")
        || name.eq_ignore_ascii_case("mp/rankIconTable.csv")
        || name.eq_ignore_ascii_case("mp/unlockTable.csv")
        || name.eq_ignore_ascii_case("mp/unlockTableMP.csv")
        || name.eq_ignore_ascii_case("mp/attachmentTable.csv")
        || name.eq_ignore_ascii_case("mp/attachmentCombos.csv")
        || name.eq_ignore_ascii_case("mp/weaponoptions.csv")
        || name.eq_ignore_ascii_case("mp/camoTable.csv")
}

pub fn capture(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    header: ext_t5_fastfile::Ptr,
) -> Option<asset_game::CapturedStringTable> {
    let name = match s.ptr_at(header, 0).ok() {
        Some(ext_t5_fastfile::ZonePtr::Offset(p)) => {
            s.cstr(s.resolve_alias(p)).ok().unwrap_or("").to_owned()
        }
        _ => return None,
    };
    let columns = s.i32_at(header, 4).unwrap_or(0).max(0) as usize;
    let rows = s.i32_at(header, 8).unwrap_or(0).max(0) as usize;
    let cells_n = columns.saturating_mul(rows);
    let arr = match s.ptr_at(header, 12).ok() {
        Some(ext_t5_fastfile::ZonePtr::Offset(p)) => s.resolve_alias(p),
        _ => {
            return Some(asset_game::CapturedStringTable {
                name,
                columns,
                rows,
                cells: Vec::new(),
            });
        }
    };
    let mut cells = Vec::with_capacity(cells_n);
    let cell_sz = ext_t5_fastfile::size::STRING_TABLE_CELL;
    for i in 0..cells_n {
        let cell = arr.at(i * cell_sz);
        let value = match s.ptr_at(cell, 0).ok() {
            Some(ext_t5_fastfile::ZonePtr::Offset(p)) => {
                s.cstr(s.resolve_alias(p)).ok().unwrap_or("").to_owned()
            }
            _ => String::new(),
        };
        cells.push(value);
    }
    Some(asset_game::CapturedStringTable {
        name,
        columns,
        rows,
        cells,
    })
}
