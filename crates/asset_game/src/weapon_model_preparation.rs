/// Inputs selected by a native family before FPV assembly preparation.
pub struct WeaponModelPreparation<'a> {
    pub family: asset_core::FamilyId,
    pub hands: Option<&'a str>,
    pub notetrack_convention: crate::NotetrackConvention,
}
