use crate::FamilyId;
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
pub struct GameFolderFamily {
    pub key: &'static str,
    pub title: &'static str,
    pub order: u16,
}
#[derive(Default)]
pub struct GameFolderFamilies(BTreeMap<FamilyId, GameFolderFamily>);
impl GameFolderFamilies {
    pub fn register(&mut self, family: FamilyId, policy: GameFolderFamily) {
        assert!(
            !self.0.values().any(|value| value.key == policy.key),
            "duplicate game folder key"
        );
        assert!(
            self.0.insert(family, policy).is_none(),
            "duplicate game folder family"
        );
    }
}
static FOLDERS: OnceLock<GameFolderFamilies> = OnceLock::new();
pub fn install_game_folder_families(families: GameFolderFamilies) -> Result<(), &'static str> {
    FOLDERS
        .set(families)
        .map_err(|_| "game folder families already installed")
}
pub fn game_folder_family(family: FamilyId) -> Option<&'static GameFolderFamily> {
    FOLDERS.get()?.0.get(&family)
}
pub fn game_folder_families() -> Vec<(FamilyId, &'static GameFolderFamily)> {
    let mut rows: Vec<_> = FOLDERS
        .get()
        .into_iter()
        .flat_map(|registry| registry.0.iter().map(|(&id, policy)| (id, policy)))
        .collect();
    rows.sort_by_key(|(id, policy)| (policy.order, *id));
    rows
}
