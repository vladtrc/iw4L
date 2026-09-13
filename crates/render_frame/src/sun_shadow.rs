pub const SUN_SHADOW_PARTITION_COUNT: u32 = 2;

pub const SUN_SHADOW_CASTER_TECH: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SunShadowAtlasProfile {
    Large,
    Small,
}

impl SunShadowAtlasProfile {
    pub const fn render_target_id(self) -> u8 {
        match self {
            Self::Large => 10,
            Self::Small => 11,
        }
    }

    pub const fn partition_size(self) -> u32 {
        match self {
            Self::Large => 0x400,
            Self::Small => 0x200,
        }
    }

    pub const fn atlas_columns(self) -> u32 {
        match self {
            Self::Large => 2,
            Self::Small => 4,
        }
    }

    pub const fn atlas_width(self) -> u32 {
        self.partition_size() * self.atlas_columns()
    }

    pub const fn gpu_width(self) -> u32 {
        self.partition_size()
    }

    pub const fn gpu_height(self) -> u32 {
        self.partition_size() * self.atlas_columns()
    }

    pub const fn partition_viewport(self, partition: u32) -> Option<SunShadowViewport> {
        if partition >= SUN_SHADOW_PARTITION_COUNT {
            return None;
        }
        let size = self.partition_size();
        Some(SunShadowViewport {
            x: 0,
            y: partition * size,
            width: size,
            height: size,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SunShadowViewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub const SUN_SHADOW_FORCED_PROFILE: SunShadowAtlasProfile = SunShadowAtlasProfile::Large;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunShadowReceiverConstants {
    pub switch_partition: [f32; 4],
    pub shadowmap_scale: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct SunShadowPartition {
    pub clip_from_world: glam::Mat4,
    pub view: glam::Mat4,
    pub projection: glam::Mat4,
    pub polygon_offset: [f32; 4],
    pub viewport: SunShadowViewport,
}

#[derive(Clone, Copy, Debug)]
pub struct SunShadowForcedFrame {
    pub partitions: [SunShadowPartition; 2],
    pub pixel_adjust: [f32; 4],
    pub lookup: glam::Mat4,
    pub receiver: SunShadowReceiverConstants,
    pub partition_fraction: [f32; 4],

    pub axes: [[f32; 3]; 3],

    pub view_forward: [f32; 3],

    pub view_right: [f32; 3],

    pub tan_half_fov: [f32; 2],

    pub near_shadow_min_dist: f32,

    pub shadow_org_pixel_center: [f32; 2],

    pub projection_offset_clip: [f32; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SunShadowPartitionLists {
    pub surface_vis: Vec<u8>,
    pub smodel_vis: Vec<u8>,
    pub world_surfs: Vec<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SunShadowCasterLists {
    pub partitions: [SunShadowPartitionLists; 2],
}
