use crate::{FxCatalog, FxModelCatalog, OwnedFxImpactTable, TeamIcons};

#[derive(Default)]
pub struct GameLoadCapture {
    pub fx: FxCatalog,
    pub fx_models: FxModelCatalog,
    pub impact_fx: Option<OwnedFxImpactTable>,
    pub t5_teamset: Option<String>,
    pub team_icons: TeamIcons,
}
