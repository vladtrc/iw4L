use glam::Mat4;

use crate::SmodelPretessRange;
use crate::geometry::SurfaceSamplerInputs;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BspCameraLane {
    LitOpaque,
    LitTrans,
    Decal,
    Emissive,
}

#[derive(Clone, Copy, Debug)]
pub enum RetainedDrawKind {
    World {
        surf: u16,
        run: u16,
        run_off: u32,

        bsp_kind: Option<BspCameraLane>,

        bsp_run_first: Option<u16>,

        setup_key_changed: bool,

        world_from_local: Mat4,
    },
    Smodel {
        surface: u32,
        material: u32,
        world_from_local: Mat4,
        lighting_handle: u32,

        packed_lighting: Option<[u8; 4]>,

        placement: u32,

        stream: Option<lighting_iw4::SmodelSurfPath>,

        cache_index: Option<u16>,

        pretess: Option<SmodelPretessRange>,
    },

    XModel {
        surface: u32,
        material: u32,

        object_id: u16,
        world_from_local: Mat4,
        lighting_handle: u32,

        packed_lighting: Option<[u8; 4]>,

        is_scope: bool,

        scene_entnum: Option<u32>,
    },

    CodeMesh {
        draw: u32,
        material: u32,
        arg_count: u8,
        args: [[f32; 4]; 2],
    },

    ParticleCloud {
        draw: u32,
        material: u32,
        clouds: [fx_iw4::GfxParticleCloud; 3],
    },

    MarkMesh {
        draw: u32,
        material: u32,
        packed: bool,
        lighting_handle: u32,
    },

    Glass {
        draw: u32,
        material: u32,
        lighting_handle: u32,
    },
}

impl RetainedDrawKind {
    pub const fn world(surf: u16) -> Self {
        Self::world_with_pose(surf, Mat4::IDENTITY)
    }

    pub const fn world_with_pose(surf: u16, world_from_local: Mat4) -> Self {
        Self::World {
            surf,
            run: 1,
            run_off: 0,
            bsp_kind: None,
            bsp_run_first: None,
            setup_key_changed: false,
            world_from_local,
        }
    }

    pub const fn bsp_world(
        surf: u16,
        run: u16,
        bsp_kind: BspCameraLane,
        bsp_run_first: u16,
        setup_key_changed: bool,
    ) -> Self {
        Self::World {
            surf,
            run,
            run_off: 0,
            bsp_kind: Some(bsp_kind),
            bsp_run_first: Some(bsp_run_first),
            setup_key_changed,
            world_from_local: Mat4::IDENTITY,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RetainedDrawItem {
    pub material_id: Option<render_material::MaterialAssetId>,

    pub key: u64,

    pub material_rank: u32,
    pub kind: RetainedDrawKind,

    pub surface_samplers: SurfaceSamplerInputs,

    pub camera_region: Option<u8>,
}

impl RetainedDrawItem {
    #[must_use]
    pub const fn host_sort_key(&self) -> (u32, u32, u32) {
        (
            (self.key >> 42) as u32,
            self.material_rank,
            (self.key & 0x3fff_ffff) as u32,
        )
    }
}

pub const XMODEL_OBJECT_ID_VIEWMODEL: u16 = 1;

pub const RENDER_FX_DEPTH_HACK: u32 = 2;

#[must_use]
pub const fn host_viewmodel_render_fx_flags(object_id: u16) -> u32 {
    if object_id == XMODEL_OBJECT_ID_VIEWMODEL {
        RENDER_FX_DEPTH_HACK
    } else {
        0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightAttenuationBind {
    pub image: Option<u32>,
    pub sampler: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct T5LightFalloffPack {
    pub diffuse: Option<[f32; 4]>,
    pub specular: Option<[f32; 4]>,
    pub attenuation: Option<[f32; 4]>,
    pub falloff: Option<[f32; 4]>,
    pub a_ab_b: Option<[f32; 4]>,
    pub angle_z: Option<f32>,
    pub cookie0: Option<[f32; 4]>,
    pub cookie1: Option<[f32; 4]>,
    pub cookie2: Option<[f32; 4]>,
}
