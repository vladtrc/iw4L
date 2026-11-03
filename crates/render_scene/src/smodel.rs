use bevy::prelude::*;
use render_material::RuntimeMaterialCatalog;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct SmodelPassMaterial {
    pub model_lighting_required: bool,
    pub color: Option<Handle<Image>>,
    pub specular: Option<Handle<Image>>,
    pub probe: Option<Handle<Image>>,

    pub atlas: Option<Handle<Image>>,
    pub alpha_mode: AlphaMode,

    pub draw_mode: Option<asset_material::MaterialDrawMode>,

    pub cull_mode: Option<bevy::render::render_resource::Face>,
    pub env_map_parms: [f32; 4],

    pub lighting_lookup_scale: [f32; 4],
    pub atlas_lookup: [f32; 4],

    pub sort_key: u8,

    pub material_sorted_index: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct FamilyLodView {
    policy: asset_model::ModelLodPolicy,
    pub controls: asset_model::LodControls,
    pub projection: asset_model::LodProjectionState,
}

#[derive(Clone, Debug)]
pub struct LodViewProfiles(Arc<BTreeMap<asset_core::FamilyId, FamilyLodView>>);

impl LodViewProfiles {
    pub fn new(skinned: bool) -> Self {
        Self(Arc::new(
            asset_model::model_families()
                .iter()
                .map(|(id, family)| {
                    (
                        id,
                        FamilyLodView {
                            policy: family.lod,
                            controls: if skinned {
                                family.lod.skinned
                            } else {
                                family.lod.rigid
                            },
                            projection: asset_model::LodProjectionState::default(),
                        },
                    )
                })
                .collect(),
        ))
    }
    pub fn update(&mut self, tan_half_fov_y: f32) {
        for view in Arc::make_mut(&mut self.0).values_mut() {
            (view.policy.update_view)(&mut view.projection, tan_half_fov_y, view.controls);
        }
    }
    pub fn set_scale(&mut self, scale: f32) {
        for view in Arc::make_mut(&mut self.0).values_mut() {
            view.controls.scale = scale;
        }
    }
    pub fn set_bias(&mut self, bias: f32) {
        for view in Arc::make_mut(&mut self.0).values_mut() {
            view.controls.bias = bias;
        }
    }
    pub fn snapshot(&self) -> Arc<BTreeMap<asset_core::FamilyId, FamilyLodView>> {
        self.0.clone()
    }
    pub fn summary(&self) -> String {
        self.0
            .iter()
            .map(|(id, view)| {
                format!(
                    "{} scale/bias/applied={}/{}/{}",
                    id.as_str(),
                    view.controls.scale,
                    view.controls.bias,
                    view.projection.applied_scale
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LodViewKey {
    eye_bits: Option<[u32; 3]>,
    ramp_bits: [Option<u32>; 3],
    world_unit_bits: Option<u32>,
    no_cull_out: bool,
    family_bits: Vec<(asset_core::FamilyId, [u32; 6])>,
}

#[derive(Clone, Debug)]
pub struct LodRampArgs {
    pub scale_mid: Option<f32>,
    pub bias_mid: Option<f32>,
    pub scale_last: Option<f32>,
    pub world_unit: Option<f32>,
    pub profiles: Arc<BTreeMap<asset_core::FamilyId, FamilyLodView>>,
    pub no_lod_cull_out: bool,
}
impl Default for LodRampArgs {
    fn default() -> Self {
        Self {
            scale_mid: None,
            bias_mid: None,
            scale_last: None,
            world_unit: None,
            profiles: LodViewProfiles::new(false).snapshot(),
            no_lod_cull_out: false,
        }
    }
}
impl LodRampArgs {
    pub fn view_key(&self, eye: Option<[f32; 3]>) -> LodViewKey {
        LodViewKey {
            eye_bits: eye.map(|eye| eye.map(f32::to_bits)),
            ramp_bits: [
                self.scale_mid.map(f32::to_bits),
                self.bias_mid.map(f32::to_bits),
                self.scale_last.map(f32::to_bits),
            ],
            world_unit_bits: self.world_unit.map(f32::to_bits),
            no_cull_out: self.no_lod_cull_out,
            family_bits: self.family_bits(),
        }
    }
    pub fn family_bits(&self) -> Vec<(asset_core::FamilyId, [u32; 6])> {
        self.profiles
            .iter()
            .map(|(&id, view)| {
                (
                    id,
                    [
                        view.controls.scale.to_bits(),
                        view.controls.bias.to_bits(),
                        view.controls.fov_threshold.to_bits(),
                        view.projection.applied_scale.to_bits(),
                        view.projection.distance_scale.to_bits(),
                        view.projection.distance_bias.to_bits(),
                    ],
                )
            })
            .collect()
    }
}

pub fn smodel_camera_lod(
    lod: Option<asset_model::ModelLodSelector>,
    origin: [f32; 3],
    scale: f32,
    eye: Option<Vec3>,
    ramp: &LodRampArgs,
) -> Option<u8> {
    let (Some(eye), Some(lod)) = (eye, lod) else {
        return Some(0);
    };
    let view = ramp.profiles.get(&lod.family)?;
    (view.policy.select)(
        lod,
        asset_model::LodSelectionSample {
            offset: [eye.x - origin[0], eye.y - origin[1], eye.z - origin[2]],
            placement_scale: scale,
            scale_mid: ramp.scale_mid,
            bias_mid: ramp.bias_mid,
            scale_last: ramp.scale_last,
            world_unit: ramp.world_unit,
            no_cull_out: ramp.no_lod_cull_out,
        },
        view.projection,
    )
}

pub fn runtime_cull_face(cull_mode: Option<u8>) -> Option<bevy::render::render_resource::Face> {
    match cull_mode {
        Some(0) => Some(bevy::render::render_resource::Face::Back),
        Some(1) => Some(bevy::render::render_resource::Face::Front),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct AuthoredMaps {
    pub color: Option<Handle<Image>>,
    pub specular: Option<Handle<Image>>,
    pub alpha_mode: AlphaMode,

    pub draw_mode: Option<asset_material::MaterialDrawMode>,
    pub cull_mode: Option<bevy::render::render_resource::Face>,
    pub env_map_parms: [f32; 4],
}

pub fn runtime_maps(
    material: Option<assets::MaterialIndex>,
    catalog: &RuntimeMaterialCatalog,
    exact_handles: &[Option<Handle<Image>>],
) -> AuthoredMaps {
    let runtime = material.and_then(|id| catalog.derived(id));
    let tex = |semantic: u8| {
        runtime.and_then(|m| {
            m.texture_semantic(semantic)
                .and_then(|id| exact_handles.get(id.0 as usize).and_then(Clone::clone))
        })
    };
    AuthoredMaps {
        color: tex(asset_material::TS_COLOR_MAP),
        specular: tex(asset_material::TS_SPECULAR_MAP),
        alpha_mode: AlphaMode::Opaque,
        draw_mode: None,
        cull_mode: runtime.and_then(|m| runtime_cull_face(m.cull_mode)),
        env_map_parms: runtime.map(|m| m.env_map_parms()).unwrap_or([0.0; 4]),
    }
}

#[derive(Resource, Clone, Debug)]
pub struct LodRampSkinnedDvar {
    pub scale_mid: Option<f32>,
    pub bias_mid: Option<f32>,
    pub scale_last: Option<f32>,
    pub world_unit: Option<f32>,
    pub profiles: LodViewProfiles,
}
impl Default for LodRampSkinnedDvar {
    fn default() -> Self {
        Self {
            scale_mid: None,
            bias_mid: None,
            scale_last: None,
            world_unit: None,
            profiles: LodViewProfiles::new(true),
        }
    }
}
impl LodRampSkinnedDvar {
    pub fn args(&self) -> LodRampArgs {
        LodRampArgs {
            scale_mid: self.scale_mid,
            bias_mid: self.bias_mid,
            scale_last: self.scale_last,
            world_unit: self.world_unit,
            profiles: self.profiles.snapshot(),
            no_lod_cull_out: false,
        }
    }
}
