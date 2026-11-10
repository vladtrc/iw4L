use asset_game::*;
pub fn capture(
    catalog: &mut ImpactFxCatalog,
    s: &ext_t5_fastfile::ZoneStream<'_>,
    name: ext_t5_fastfile::Ptr,
    entries: ext_t5_fastfile::Ptr,
    fx: &FxCatalog,
) -> ext_t5_fastfile::Result<()> {
    let mut rows = Vec::with_capacity(ext_t5_fastfile::size::FX_IMPACT_ENTRY_COUNT);
    let mut gaps = 0;
    for row in 0..ext_t5_fastfile::size::FX_IMPACT_ENTRY_COUNT {
        let mut result = OwnedFxImpactEntry::default();
        {
            for (column, cell) in result
                .nonflesh
                .iter_mut()
                .chain(result.flesh.iter_mut())
                .enumerate()
            {
                let ptr = entries.at(row * ext_t5_fastfile::size::FX_IMPACT_ENTRY + column * 4);
                if s.ptr_at(ptr, 0)? != ext_t5_fastfile::ZonePtr::Null {
                    if let Some(name) = fx.name_at_slot(asset_material::AssetPointerIdentity {
                        block: ptr.block,
                        offset: ptr.offset,
                    }) {
                        *cell = name.to_owned();
                    } else {
                        gaps += 1;
                    }
                }
            }
        }
        rows.push(result);
    }
    catalog.capture_gaps += gaps;
    catalog.table = Some(OwnedFxImpactTable {
        namespace: crate::ID,
        name: s.cstr(name)?.to_owned(),
        entries: rows,
        capture_gaps: gaps,
    });
    Ok(())
}

pub fn impact_row(impact_type: i32, exit: bool) -> Option<usize> {
    let exit = usize::from(exit);
    Some(match impact_type {
        1 => exit,
        2 => 3 + exit,
        3 => 7 + exit,
        4 => 9 + exit,
        5 => 5 + exit,
        6 => 11,
        7 | 8 => 12,
        9 => 13,
        10 => 14,
        11 => 15,
        12 => 16,
        13 => 17,
        14 => 19,
        15 => 20,
        _ => return None,
    })
}
