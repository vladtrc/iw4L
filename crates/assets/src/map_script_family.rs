use crate::{MapFacts, ScriptSources};
use asset_core::FamilyId;
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(Clone, Copy)]
pub struct MapScriptFamily {
    pub compile: Option<fn(&ScriptSources, &str, &MapFacts) -> ScriptSources>,
    pub project_factions: bool,
    pub builtins: &'static [(&'static str, &'static str)],
}
pub const REFERENCE_MAP_SCRIPT_FAMILY: MapScriptFamily = MapScriptFamily {
    compile: None,
    project_factions: false,
    builtins: &[],
};
#[derive(Default)]
pub struct MapScriptFamilies(BTreeMap<FamilyId, MapScriptFamily>);
impl MapScriptFamilies {
    pub fn register(&mut self, id: FamilyId, policy: MapScriptFamily) {
        assert!(!self.0.contains_key(&id), "duplicate map script family");
        self.0.insert(id, policy);
    }
}
static FAMILIES: OnceLock<MapScriptFamilies> = OnceLock::new();
pub fn install_map_script_families(families: MapScriptFamilies) -> Result<(), &'static str> {
    FAMILIES
        .set(families)
        .map_err(|_| "map script families already installed")
}
pub fn map_script_family(id: FamilyId) -> Option<&'static MapScriptFamily> {
    FAMILIES.get()?.0.get(&id)
}
pub fn map_script_builtin_sources() -> impl Iterator<Item = (&'static str, &'static str)> {
    FAMILIES
        .get()
        .into_iter()
        .flat_map(|families| families.0.values())
        .flat_map(|policy| policy.builtins.iter().copied())
}
