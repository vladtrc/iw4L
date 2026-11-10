use asset_core::FamilyId;
use asset_game::CapturedStringTable;

/// Table ownership is established by the zone that was actually opened.
#[derive(Default)]
pub(crate) struct CommonTables {
    owner: Option<FamilyId>,
    pub(crate) tables: Vec<CapturedStringTable>,
}

impl CommonTables {
    pub(crate) fn owner(&self) -> Option<FamilyId> {
        self.owner
    }
    pub(crate) fn capture(owner: Option<FamilyId>, tables: Vec<CapturedStringTable>) -> Self {
        assert!(
            owner.is_some() || tables.is_empty(),
            "common tables lack a captured owner"
        );
        Self { owner, tables }
    }

    pub(crate) fn into_owned(self) -> impl Iterator<Item = (FamilyId, CapturedStringTable)> {
        self.tables
            .into_iter()
            .map(move |table| (self.owner.expect("captured table owner"), table))
    }
}
