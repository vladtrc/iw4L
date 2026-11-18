use asset_material::MaterialCatalog;
use asset_model::{FpvMeshBuild, WorldWeaponBuild};
use ext_t6_material::capture::bind_native_materials;

pub(super) fn bind_t6_content(
    content: super::super::T6Content,
    materials: &mut MaterialCatalog,
    fpv: &mut FpvMeshBuild,
    world: &mut WorldWeaponBuild,
    refusals: &mut Vec<String>,
) -> String {
    let bound = bind_native_materials(
        &content.path,
        &content.materials,
        &content.techsets,
        materials,
        refusals,
    );
    let count = content.models.len();
    for mut model in content.models {
        model.skel.surface_materials = model
            .surface_materials
            .iter()
            .map(|name| {
                name.as_ref()
                    .and_then(|name| bound.get(name))
                    .copied()
                    .map(asset_core::WalkLocalMaterialIndex::from_walk)
            })
            .collect();
        if model.view {
            fpv.insert_in(crate::ID, model.skel, Some(materials));
        } else {
            world.insert_in(crate::ID, model.skel, Some(materials));
        }
    }
    materials.resolve_technique_set_edges();
    format!(
        "t6 native content: {count} models, {} materials",
        bound.len()
    )
}
