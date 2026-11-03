use crate::discover::{GamesRoot, find_zone_file_version, zone_game_for_path};
use crate::iwd::game_main_for_zone;
use asset_core::{FamilyId, families};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamespaceTree {
    pub anchor: PathBuf,
    pub main: Option<PathBuf>,
}

impl NamespaceTree {
    fn from_anchor(anchor: PathBuf) -> Self {
        let main = game_main_for_zone(&anchor).ok();
        Self { anchor, main }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NamespaceTrees {
    trees: BTreeMap<FamilyId, NamespaceTree>,
}

impl NamespaceTrees {
    pub fn discover(root: &GamesRoot) -> Self {
        let mut trees = BTreeMap::new();
        for family in families().iter() {
            if let Ok(found) =
                find_zone_file_version(root, family.anchor_zone(), family.zone_version())
            {
                trees.insert(family.id(), NamespaceTree::from_anchor(found.path));
            }
        }
        Self { trees }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(FamilyId) -> bool) {
        self.trees.retain(|id, _| keep(*id));
    }

    pub fn adopt_zone(&mut self, zone_ff: &Path) {
        if let Some(id) = zone_game_for_path(zone_ff) {
            self.trees
                .insert(id, NamespaceTree::from_anchor(zone_ff.to_owned()));
        }
    }

    pub fn get(&self, id: FamilyId) -> Option<&NamespaceTree> {
        self.trees.get(&id)
    }
    pub fn main_for(&self, id: FamilyId) -> Option<&Path> {
        self.get(id).and_then(|tree| tree.main.as_deref())
    }
    pub fn is_empty(&self) -> bool {
        self.trees.is_empty()
    }
    pub fn present(&self) -> impl Iterator<Item = (FamilyId, &NamespaceTree)> {
        self.trees.iter().map(|(id, tree)| (*id, tree))
    }
    pub fn report_lines(&self) -> Vec<String> {
        families()
            .iter()
            .map(|family| match self.get(family.id()) {
                Some(tree) => format!(
                    "namespace tree {}: main={} anchor={}",
                    family.id().as_str(),
                    tree.main
                        .as_deref()
                        .map_or_else(|| "-".to_owned(), |main| main.display().to_string()),
                    tree.anchor.display()
                ),
                None => format!("namespace tree {}: not installed", family.id().as_str()),
            })
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct NamespaceSoundIwd {
    indices: BTreeMap<FamilyId, crate::iwd::IwdSoundIndex>,
}
impl NamespaceSoundIwd {
    pub fn open(trees: &NamespaceTrees) -> (Self, Vec<String>) {
        let mut opened = Self::default();
        let mut lines = Vec::new();
        for (ns, tree) in trees.present() {
            let Some(main) = tree.main.as_deref() else {
                lines.push(format!(
                    "audio: IWD sounds {}: tree {} has no archive directory",
                    ns.as_str(),
                    tree.anchor.display()
                ));
                continue;
            };
            match crate::iwd::IwdSoundIndex::open(main) {
                Ok(index) => {
                    lines.push(format!(
                        "audio: IWD sounds {} from {} — {} paths",
                        ns.as_str(),
                        index.indexed_dir().display(),
                        index.sound_count()
                    ));
                    opened.indices.insert(ns, index);
                }
                Err(e) => lines.push(format!(
                    "audio: IWD sounds {} unavailable ({e})",
                    ns.as_str()
                )),
            }
        }
        (opened, lines)
    }

    pub fn index(&self, id: FamilyId) -> Option<&crate::iwd::IwdSoundIndex> {
        self.indices.get(&id)
    }
    pub fn read_sound(&self, id: FamilyId, relative: &str) -> Option<Result<Vec<u8>, String>> {
        self.index(id)?.read_sound(relative)
    }
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}
