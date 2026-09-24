use fastfile_iw4::{FxImpactTableGeometry, Result, ZonePtr, ZoneStream};
use fx_iw4::{
    FX_IMPACT_ENTRY_SIZE, FX_IMPACT_FLESH_COUNT, FX_IMPACT_NONFLESH_COUNT, FX_IMPACT_TABLE_ROWS,
    FX_SURF_TYPE_FLESH,
};

#[derive(Clone, Debug, Default)]
pub struct OwnedFxImpactEntry {
    pub nonflesh: [String; FX_IMPACT_NONFLESH_COUNT],
    pub flesh: [String; FX_IMPACT_FLESH_COUNT],
}

#[derive(Clone, Debug, Default)]
pub struct OwnedFxImpactTable {
    pub t5: bool,
    pub name: String,
    pub entries: Vec<OwnedFxImpactEntry>,
    pub capture_gaps: usize,
}

impl OwnedFxImpactTable {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn row_count(&self) -> usize {
        self.entries.len()
    }

    pub fn namespace(&self) -> crate::AssetNamespace {
        if self.t5 {
            crate::AssetNamespace::T5
        } else {
            crate::AssetNamespace::Iw4
        }
    }

    pub fn effect_name(
        &self,
        row: usize,
        surf_type: usize,
        flesh_slot: Option<usize>,
    ) -> Option<crate::FxName<'_>> {
        let entry = self.entries.get(row)?;
        let name = if surf_type == FX_SURF_TYPE_FLESH {
            entry.flesh.get(flesh_slot?)?.as_str()
        } else {
            if surf_type >= FX_IMPACT_NONFLESH_COUNT {
                return None;
            }
            entry.nonflesh[surf_type].as_str()
        };
        (!name.is_empty()).then(|| crate::FxName::new(self.namespace(), name))
    }
}

#[derive(Clone, Debug, Default)]
pub struct ImpactFxCatalog {
    pub table: Option<OwnedFxImpactTable>,
    pub capture_gaps: usize,
}

impl ImpactFxCatalog {
    pub fn take_table(&mut self) -> Option<OwnedFxImpactTable> {
        self.table.take()
    }

    pub fn capture(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: FxImpactTableGeometry,
        fx: &crate::FxCatalog,
    ) -> Result<()> {
        if self.table.is_some() {
            self.capture_gaps += 1;
            return Ok(());
        }
        let name = match geometry.name {
            Some(p) => s.cstr(p).unwrap_or("").to_owned(),
            None => String::new(),
        };
        if geometry.row_count == 0 {
            self.capture_gaps += 1;
            return Ok(());
        }
        let mut entries = Vec::with_capacity(geometry.row_count.min(FX_IMPACT_TABLE_ROWS));
        let mut gaps = 0usize;
        for i in 0..geometry.row_count.min(FX_IMPACT_TABLE_ROWS) {
            let e = geometry.entries.at(i * s.layout(FX_IMPACT_ENTRY_SIZE, 280));
            let mut nonflesh = std::array::from_fn(|_| String::new());
            let mut flesh = std::array::from_fn(|_| String::new());
            for (j, slot) in nonflesh.iter_mut().enumerate() {
                match fx_name_at_cell(s, fx, e.at(j * s.pointer_bytes())) {
                    Ok(n) => *slot = n,
                    Err(()) => gaps += 1,
                }
            }
            for (j, slot) in flesh.iter_mut().enumerate() {
                match fx_name_at_cell(s, fx, e.at(s.layout(124, 248) + j * s.pointer_bytes())) {
                    Ok(n) => *slot = n,
                    Err(()) => gaps += 1,
                }
            }
            entries.push(OwnedFxImpactEntry { nonflesh, flesh });
        }
        self.capture_gaps += gaps;
        self.table = Some(OwnedFxImpactTable {
            t5: false,
            name,
            entries,
            capture_gaps: gaps,
        });
        Ok(())
    }
}

fn fx_name_at_cell(
    s: &ZoneStream<'_>,
    fx: &crate::FxCatalog,
    cell: fastfile_iw4::Ptr,
) -> core::result::Result<String, ()> {
    if matches!(s.ptr_at(cell, 0), Ok(ZonePtr::Null)) {
        return Ok(String::new());
    }
    match fx.name_at_slot(cell) {
        Some(n) => Ok(n.to_owned()),
        None => Err(()),
    }
}

impl ImpactFxCatalog {
    pub fn capture_t5(
        &mut self,
        s: &fastfile_t5::ZoneStream<'_>,
        name: fastfile_t5::Ptr,
        entries: fastfile_t5::Ptr,
        fx: &crate::FxCatalog,
    ) -> fastfile_t5::Result<()> {
        let mut rows = Vec::with_capacity(fastfile_t5::size::FX_IMPACT_ENTRY_COUNT);
        let mut gaps = 0;
        for row in 0..fastfile_t5::size::FX_IMPACT_ENTRY_COUNT {
            let mut result = OwnedFxImpactEntry::default();
            {
                for (column, cell) in result
                    .nonflesh
                    .iter_mut()
                    .chain(result.flesh.iter_mut())
                    .enumerate()
                {
                    let ptr = entries.at(row * fastfile_t5::size::FX_IMPACT_ENTRY + column * 4);
                    if s.ptr_at(ptr, 0)? != fastfile_t5::ZonePtr::Null {
                        if let Some(name) = fx.name_at_slot(fastfile_iw4::Ptr {
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
        self.capture_gaps += gaps;
        self.table = Some(OwnedFxImpactTable {
            t5: true,
            name: s.cstr(name)?.to_owned(),
            entries: rows,
            capture_gaps: gaps,
        });
        Ok(())
    }
}

impl OwnedFxImpactTable {
    pub fn impact_row(&self, impact_type: i32, exit: bool) -> Option<usize> {
        if !self.t5 {
            return fx_iw4::fx_impact_table_row(impact_type, exit);
        }
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
}
