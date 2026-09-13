use anim_iw4::{
    DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT, DOBJ_RADIUS_PARENT_ROOT, dobj_compute_bounds_radius,
};
use assets::{FpvMeshCatalog, MapXModelAssetKey, MapXModelSceneAsset, MapXModelSceneCatalog};
use lighting_iw4::lighting_query_box_half;

pub fn dobj_lighting_box_half(radii: &[f32], parents: &[u8]) -> Option<[f32; 3]> {
    if radii.is_empty() || radii.len() > DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT {
        return None;
    }
    Some(lighting_query_box_half(dobj_compute_bounds_radius(
        radii, parents,
    )))
}

pub fn script_model_lighting_box_half(
    catalog: &MapXModelSceneCatalog,
    models: &[assets::dobj::DObjModelDescriptor],
) -> Option<[f32; 3]> {
    if models.is_empty() || models.len() > DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT {
        return None;
    }
    let mut radii = [0.0f32; DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT];
    let mut parents = [DOBJ_RADIUS_PARENT_ROOT; DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT];
    for (i, model) in models.iter().enumerate() {
        let asset = catalog.get(&MapXModelAssetKey(model.model.clone()))?;
        let MapXModelSceneAsset::Iw4(skel) = asset else {
            return None;
        };
        radii[i] = skel.radius?;
        parents[i] = match model.parent_model {
            None => DOBJ_RADIUS_PARENT_ROOT,
            Some(parent) => u8::try_from(parent).ok()?,
        };
    }
    dobj_lighting_box_half(&radii[..models.len()], &parents[..models.len()])
}

pub fn fpv_dobj_lighting_box_half(
    catalog: &FpvMeshCatalog,
    ns: assets::AssetNamespace,
    gun_xmodel: &str,
) -> Option<[f32; 3]> {
    let hands = catalog.hands_in(ns)?;
    let gun = catalog.get(ns, gun_xmodel)?;
    let r_hands = hands.skel.radius?;
    let r_gun = gun.skel.radius?;
    dobj_lighting_box_half(&[r_hands, r_gun], &[DOBJ_RADIUS_PARENT_ROOT, 0])
}

pub fn fpv_dobj_skel_radii(
    catalog: &FpvMeshCatalog,
    ns: assets::AssetNamespace,
    gun_xmodel: &str,
) -> (Option<f32>, Option<f32>) {
    let hands = catalog.hands_in(ns).and_then(|h| h.skel.radius);
    let gun = catalog.get(ns, gun_xmodel).and_then(|g| g.skel.radius);
    (hands, gun)
}

pub fn viewmodel_lighting_origin(
    origin: [f32; 3],
    view_height_current: f32,
    view_yaw: f32,
    leanf: f32,
) -> [f32; 3] {
    lighting_iw4::viewmodel_lighting_origin(origin, view_height_current, view_yaw, leanf)
}
