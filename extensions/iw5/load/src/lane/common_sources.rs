use asset_transport::{GamesRoot, find_common_mp_for_envelope};
use assets::{CommonSourceFamily, CommonSourcePlan, CommonStartupSource};
use std::path::Path;

pub const POLICY: CommonSourceFamily = CommonSourceFamily {
    publication_order: 10,
    supplemental: Some(plan),
    material_seed: None,
    same_tree_material_donor: Some(material_donor),
};

fn plan(root: &GamesRoot, runtime: Option<&Path>) -> Result<CommonSourcePlan, String> {
    let common = find_common_mp_for_envelope(root, ext_iw5_fastfile::ZONE_VERSION_PC)?;
    if runtime == Some(common.path.as_path()) {
        return Ok(CommonSourcePlan::default());
    }
    let mut plan = CommonSourcePlan {
        common: Some(common.path),
        ..Default::default()
    };
    match asset_transport::find_zone_for_tree(
        plan.common.as_deref().expect("common source"),
        "code_post_gfx_mp",
    ) {
        Ok(source) => plan.startup.push(CommonStartupSource {
            path: source.path,
            collect_tables: true,
        }),
        Err(error) => plan
            .report
            .push(format!("IW5 startup code_post_gfx_mp: {error}")),
    }
    Ok(plan)
}

fn material_donor(
    map: &Path,
    runtime: Option<&Path>,
) -> Result<Option<std::path::PathBuf>, String> {
    let donor = asset_transport::find_common_mp_for_zone(map)
        .map_err(|error| format!("foreign data common: no same-tree common_mp ({error})"))?;
    if runtime == Some(donor.path.as_path()) {
        return Ok(None);
    }
    match asset_transport::peek_zone_version(&donor.path) {
        Some(ext_iw5_fastfile::ZONE_VERSION_PC) => Ok(Some(donor.path)),
        Some(version) => Err(format!(
            "foreign data common: skip envelope {version:#x} at {} (IW5 material data only)",
            donor.path.display()
        )),
        None => Err(format!(
            "foreign data common: unreadable envelope at {}",
            donor.path.display()
        )),
    }
}
