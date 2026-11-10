use render_scene::SmodelPassMaterial;

use crate::model_draw::{FxModelAssetDraw, FxModelDrawPlan};

pub fn fx_model_packed_mismatch(
    surfaces: &[render_anim::fpv_pose::PosedModelSurface],
) -> Option<(usize, usize, usize)> {
    surfaces.iter().find_map(|surface| {
        let decoded = surface
            .mesh
            .attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION)
            .map_or(0, |positions| positions.len());
        let packed = surface.packed_vertices.len();
        (packed != decoded).then_some((surface.surface_index, packed, decoded))
    })
}

pub fn append_fx_model_asset(
    plan: &mut FxModelDrawPlan,
    model_index: usize,
    lod: u8,
    surfaces: &[render_anim::fpv_pose::PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> Vec<(u32, u32)> {
    let geometry = std::sync::Arc::make_mut(&mut plan.geometry);
    let asset_surfaces = geometry.rows.append_posed(surfaces, materials);
    geometry.assets.push(FxModelAssetDraw {
        model_index,
        lod,
        surfaces: asset_surfaces.clone(),
    });
    let mut rev = plan.revision;
    plan.revisions.topology = geometry.rows.topology();
    plan.revisions.bump_packed_write(&mut rev);
    plan.revision = rev;
    asset_surfaces
}
