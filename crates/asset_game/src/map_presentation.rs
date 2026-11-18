use asset_core::FamilyId;
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub struct MapTableSchema {
    pub name: &'static str,
    pub map_column: i32,
    pub title_column: i32,
    pub image_column: i32,
    pub map_prefix: &'static str,
}

#[derive(Clone, Copy)]
pub struct MapPresentationFamily {
    pub singleplayer: &'static [crate::SingleplayerMode],
    /// Material name without the family namespace.
    pub preview: fn(&str) -> String,
    pub table: Option<MapTableSchema>,
}

pub const REFERENCE_MAP_PRESENTATION: MapPresentationFamily = MapPresentationFamily {
    singleplayer: crate::REFERENCE_SINGLEPLAYER_MODES,
    preview: |stem| format!("preview_{stem}"),
    table: None,
};

#[derive(Default)]
pub struct MapPresentationFamilies {
    families: BTreeMap<FamilyId, MapPresentationFamily>,
    singleplayer: Vec<crate::SingleplayerMode>,
}
impl MapPresentationFamilies {
    pub fn register(&mut self, family: FamilyId, policy: MapPresentationFamily) {
        assert!(
            self.families.insert(family, policy).is_none(),
            "duplicate map presentation family"
        );
    }
}
static FAMILIES: OnceLock<MapPresentationFamilies> = OnceLock::new();
pub fn install_map_presentation_families(
    mut families: MapPresentationFamilies,
) -> Result<(), &'static str> {
    families.singleplayer = families
        .families
        .values()
        .flat_map(|policy| policy.singleplayer.iter().copied())
        .collect();
    FAMILIES
        .set(families)
        .map_err(|_| "map presentation families already installed")
}
pub fn map_presentation_family(family: FamilyId) -> Option<&'static MapPresentationFamily> {
    FAMILIES.get()?.families.get(&family)
}

pub fn singleplayer_modes() -> &'static [crate::SingleplayerMode] {
    FAMILIES
        .get()
        .map_or(&[], |families| families.singleplayer.as_slice())
}
