use asset_transport::{GamesRoot, find_common_mp_for_envelope};
use assets::{CommonSourceFamily, CommonSourcePlan};
use std::path::Path;

pub const POLICY: CommonSourceFamily = CommonSourceFamily {
    publication_order: 30,
    supplemental: Some(plan),
    material_seed: None,
    same_tree_material_donor: None,
};

fn plan(root: &GamesRoot, runtime: Option<&Path>) -> Result<CommonSourcePlan, String> {
    let common = find_common_mp_for_envelope(root, ext_t6_fastfile::ZONE_VERSION_PC)?;
    let _ = runtime;
    let plan = CommonSourcePlan {
        common: Some(common.path),
        ..Default::default()
    };
    Ok(plan)
}
