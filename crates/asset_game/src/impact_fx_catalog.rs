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

#[derive(Clone, Debug)]
pub struct OwnedFxImpactTable {
    pub namespace: crate::FamilyId,
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

    pub fn namespace(&self) -> crate::FamilyId {
        self.namespace
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
                match name_at_cell(s, fx, e.at(j * s.pointer_bytes())) {
                    Ok(n) => *slot = n,
                    Err(()) => gaps += 1,
                }
            }
            for (j, slot) in flesh.iter_mut().enumerate() {
                match name_at_cell(s, fx, e.at(s.layout(124, 248) + j * s.pointer_bytes())) {
                    Ok(n) => *slot = n,
                    Err(()) => gaps += 1,
                }
            }
            entries.push(OwnedFxImpactEntry { nonflesh, flesh });
        }
        self.capture_gaps += gaps;
        self.table = Some(OwnedFxImpactTable {
            namespace: crate::FamilyId::IW4,
            name,
            entries,
            capture_gaps: gaps,
        });
        Ok(())
    }
}

fn name_at_cell(
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

impl OwnedFxImpactTable {
    pub fn impact_row(&self, impact_type: i32, exit: bool) -> Option<usize> {
        impact_row_resolver(self.namespace).and_then(|resolve| resolve(impact_type, exit))
    }
}

pub type ImpactRowResolver = fn(i32, bool) -> Option<usize>;
pub const REFERENCE_IMPACT_ROW_RESOLVER: ImpactRowResolver = fx_iw4::impact_table_row;

#[derive(Default)]
pub struct ImpactFxFamilies(std::collections::BTreeMap<crate::FamilyId, ImpactRowResolver>);
impl ImpactFxFamilies {
    pub fn register(&mut self, family: crate::FamilyId, resolver: ImpactRowResolver) {
        assert!(
            self.0.insert(family, resolver).is_none(),
            "duplicate impact family"
        );
    }
}
static IMPACT_FAMILIES: std::sync::OnceLock<ImpactFxFamilies> = std::sync::OnceLock::new();
pub fn install_impact_fx_families(
    families: ImpactFxFamilies,
) -> std::result::Result<(), &'static str> {
    IMPACT_FAMILIES
        .set(families)
        .map_err(|_| "impact families already installed")
}
pub fn impact_row_resolver(family: crate::FamilyId) -> Option<ImpactRowResolver> {
    IMPACT_FAMILIES.get()?.0.get(&family).copied()
}
