use asset_transport::{GamesRoot, find_common_mp_for_envelope, find_zone_file_version};
use assets::{CommonSourceFamily, CommonSourcePlan, CommonStartupSource};
use std::path::Path;

pub const POLICY: CommonSourceFamily = CommonSourceFamily {
    publication_order: 20,
    supplemental: Some(plan),
    material_seed: Some(plan),
    same_tree_material_donor: None,
};

fn plan(root: &GamesRoot, runtime: Option<&Path>) -> Result<CommonSourcePlan, String> {
    let common = find_common_mp_for_envelope(root, ext_t5_fastfile::ZONE_VERSION_PC)?;
    if runtime == Some(common.path.as_path()) {
        return Ok(CommonSourcePlan::default());
    }
    let mut plan = CommonSourcePlan {
        common: Some(common.path),
        ..Default::default()
    };
    for zone in ["code_post_gfx_mp", "localized_code_post_gfx_mp", "patch_mp"] {
        match find_zone_file_version(root, zone, ext_t5_fastfile::ZONE_VERSION_PC) {
            Ok(source) => plan.startup.push(CommonStartupSource {
                path: source.path,
                collect_tables: zone == "code_post_gfx_mp",
            }),
            Err(error) => plan
                .report
                .push(format!("t5 leftover startup gfx gap: {zone}: {error}")),
        }
    }
    Ok(plan)
}
