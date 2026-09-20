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

/// A cascade owns at most the two arc sides, the four sides of its own
/// rectangle and, for the far cascade, the seam against the near one.
pub const SUN_SHADOW_MAX_CLIP_PLANES: usize = 10;

/// The volume a cascade collects casters from, as world-space planes that hold
/// for the points inside it.
#[derive(Clone, Copy, Debug)]
pub struct SunShadowClipPlanes {
    planes: [[f32; 4]; SUN_SHADOW_MAX_CLIP_PLANES],
    count: u8,
}

impl SunShadowClipPlanes {
    pub const EMPTY: Self = Self {
        planes: [[0.0; 4]; SUN_SHADOW_MAX_CLIP_PLANES],
        count: 0,
    };

    /// Refuses once full. Dropping a plane widens the volume, which costs draw
    /// calls; admitting an eleventh would need a wider array here and in DPVS.
    pub fn push(&mut self, plane: [f32; 4]) -> bool {
        let index = self.count as usize;
        if index == SUN_SHADOW_MAX_CLIP_PLANES {
            return false;
        }
        self.planes[index] = plane;
        self.count += 1;
        true
    }

    #[must_use]
    pub fn as_slice(&self) -> &[[f32; 4]] {
        &self.planes[..self.count as usize]
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.count as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SunShadowPartition {
    pub clip_from_world: glam::Mat4,
    pub view: glam::Mat4,
    pub projection: glam::Mat4,
    pub polygon_offset: [f32; 4],
    pub viewport: SunShadowViewport,

    pub clip_planes: SunShadowClipPlanes,
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
