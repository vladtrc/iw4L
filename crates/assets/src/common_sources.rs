use asset_core::FamilyId;
use asset_transport::GamesRoot;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Debug)]
pub struct CommonStartupSource {
    pub path: PathBuf,
    pub collect_tables: bool,
}

#[derive(Default, Debug)]
pub struct CommonSourcePlan {
    pub common: Option<PathBuf>,
    pub startup: Vec<CommonStartupSource>,
    pub report: Vec<String>,
}

pub type CommonSourcePlanner = fn(&GamesRoot, Option<&Path>) -> Result<CommonSourcePlan, String>;
pub type MaterialDonorPlanner = fn(&Path, Option<&Path>) -> Result<Option<PathBuf>, String>;

#[derive(Clone, Copy)]
pub struct CommonSourceFamily {
    pub publication_order: u16,
    pub supplemental: Option<CommonSourcePlanner>,
    pub material_seed: Option<CommonSourcePlanner>,
    pub same_tree_material_donor: Option<MaterialDonorPlanner>,
}

pub const REFERENCE_COMMON_SOURCE_FAMILY: CommonSourceFamily = CommonSourceFamily {
    publication_order: 0,
    supplemental: None,
    material_seed: None,
    same_tree_material_donor: None,
};

#[derive(Default)]
pub struct CommonSourceFamilies(BTreeMap<FamilyId, CommonSourceFamily>);
impl CommonSourceFamilies {
    pub fn register(&mut self, id: FamilyId, policy: CommonSourceFamily) {
        assert!(
            self.0.insert(id, policy).is_none(),
            "duplicate common source family"
        );
    }
}
static FAMILIES: OnceLock<CommonSourceFamilies> = OnceLock::new();
pub fn install_common_source_families(families: CommonSourceFamilies) -> Result<(), &'static str> {
    FAMILIES
        .set(families)
        .map_err(|_| "common source families already installed")
}
pub fn common_source_family(id: FamilyId) -> Option<&'static CommonSourceFamily> {
    FAMILIES.get()?.0.get(&id)
}
pub fn common_source_families() -> impl Iterator<Item = (FamilyId, &'static CommonSourceFamily)> {
    FAMILIES
        .get()
        .into_iter()
        .flat_map(|families| families.0.iter().map(|(&id, policy)| (id, policy)))
}

/// Resolve installed, enabled same-tree policies without choosing an arbitrary
/// donor when two policies disagree.
pub fn plan_common_material_donor(
    zone: &Path,
    runtime_common: Option<&Path>,
    enabled: impl Fn(FamilyId) -> bool,
    report: &mut Vec<String>,
) -> Option<PathBuf> {
    let mut selected = None;
    for (family, policy) in common_source_families() {
        if !enabled(family) {
            continue;
        }
        let Some(plan) = policy.same_tree_material_donor else {
            continue;
        };
        match plan(zone, runtime_common) {
            Ok(Some(path)) => {
                if selected.as_ref().is_some_and(|previous| *previous != path) {
                    report.push(
                        "foreign data common: conflicting installed material donor plans".into(),
                    );
                    return None;
                }
                selected = Some(path);
            }
            Ok(None) => {}
            Err(error) => report.push(error),
        }
    }
    selected
}

/// Ordered material seed plans contributed by installed families.
pub fn plan_common_material_seeds(
    root: &GamesRoot,
    runtime: Option<&Path>,
    report: &mut Vec<String>,
) -> Vec<(FamilyId, CommonSourcePlan)> {
    common_source_families()
        .filter_map(|(id, policy)| {
            let planner = policy.material_seed?;
            match planner(root, runtime) {
                Ok(plan) => Some((id, plan)),
                Err(error) => {
                    report.push(format!("{} material seed: {error}", id.as_str()));
                    None
                }
            }
        })
        .collect()
}
