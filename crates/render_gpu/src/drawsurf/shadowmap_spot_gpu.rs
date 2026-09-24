use bevy::prelude::*;
use bevy::render::render_resource::{
    Buffer, Extent3d, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    TextureView, TextureViewDescriptor,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use lighting_iw4::{GFX_SPOT_SHADOW_RT_LARGE, GFX_SPOT_SHADOW_RT_SMALL, spot_shadow_gpu_extent};

use super::shadowmap_sun_gpu::upload_shadow_index_epochs;

pub const SHADOWMAP_SPOT_COLOR_FORMAT: TextureFormat = TextureFormat::R32Float;

pub const SHADOWMAP_SPOT_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth24PlusStencil8;

pub const SHADOWMAP_SPOT_RT10_LABEL: &str = "iw4_shadowmap_spot_rt10";
pub const SHADOWMAP_SPOT_RT11_LABEL: &str = "iw4_shadowmap_spot_rt11";
pub const SHADOWMAP_SUN_LABEL: &str = "iw4_shadowmap_sun";

struct SpotGpuTarget {
    _color: Texture,
    color_view: TextureView,
    _depth: Texture,
    depth_view: TextureView,
    width: u32,
    height: u32,
}

#[derive(Resource, Default)]
pub(super) struct ShadowmapSpotGpu {
    rt10: Option<SpotGpuTarget>,
    rt11: Option<SpotGpuTarget>,
    smodel_index_epochs: [Vec<Buffer>; lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP],
    smodel_index_epoch_bytes: [Vec<u64>; lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP],
    xmodel_index_epochs: [Vec<Buffer>; lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP],
    xmodel_index_epoch_bytes: [Vec<u64>; lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP],
}

impl ShadowmapSpotGpu {
    pub(crate) fn color_view(&self, render_target_id: u8) -> Option<&TextureView> {
        match render_target_id {
            GFX_SPOT_SHADOW_RT_LARGE => self.rt10.as_ref().map(|t| &t.color_view),
            GFX_SPOT_SHADOW_RT_SMALL => self.rt11.as_ref().map(|t| &t.color_view),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub(super) fn depth_view(&self, render_target_id: u8) -> Option<&TextureView> {
        match render_target_id {
            GFX_SPOT_SHADOW_RT_LARGE => self.rt10.as_ref().map(|t| &t.depth_view),
            GFX_SPOT_SHADOW_RT_SMALL => self.rt11.as_ref().map(|t| &t.depth_view),
            _ => None,
        }
    }

    pub(super) fn upload_smodel_index_epochs(
        &mut self,
        slot_index: u32,
        device: &RenderDevice,
        queue: &RenderQueue,
        epochs: &[Vec<u32>],
    ) -> Vec<Buffer> {
        let slot = usize::try_from(slot_index)
            .ok()
            .filter(|&slot| slot < lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP)
            .expect("emitted spot-shadow slot is in the retail slot table");
        upload_shadow_index_epochs(
            &mut self.smodel_index_epochs[slot],
            &mut self.smodel_index_epoch_bytes[slot],
            device,
            queue,
            epochs,
            "iw4_shadowmap_spot_smodel_index_epoch",
        )
    }

    pub(super) fn upload_xmodel_index_epochs(
        &mut self,
        slot_index: u32,
        device: &RenderDevice,
        queue: &RenderQueue,
        epochs: &[Vec<u32>],
    ) -> Vec<Buffer> {
        let slot = usize::try_from(slot_index)
            .ok()
            .filter(|&slot| slot < lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP)
            .expect("emitted spot-shadow slot is in the retail slot table");
        upload_shadow_index_epochs(
            &mut self.xmodel_index_epochs[slot],
            &mut self.xmodel_index_epoch_bytes[slot],
            device,
            queue,
            epochs,
            "iw4_shadowmap_spot_xmodel_index_epoch",
        )
    }
}

fn spot_label(render_target_id: u8) -> Option<(&'static str, &'static str)> {
    match render_target_id {
        GFX_SPOT_SHADOW_RT_LARGE => {
            Some((SHADOWMAP_SPOT_RT10_LABEL, "iw4_shadowmap_spot_rt10_depth"))
        }
        GFX_SPOT_SHADOW_RT_SMALL => {
            Some((SHADOWMAP_SPOT_RT11_LABEL, "iw4_shadowmap_spot_rt11_depth"))
        }
        _ => None,
    }
}

fn ensure_one(
    slot: &mut Option<SpotGpuTarget>,
    device: &RenderDevice,
    render_target_id: u8,
) -> bool {
    let Some((width, height)) = spot_shadow_gpu_extent(render_target_id) else {
        return false;
    };
    let Some((color_label, depth_label)) = spot_label(render_target_id) else {
        return false;
    };
    debug_assert_ne!(color_label, SHADOWMAP_SUN_LABEL);
    let resized = slot
        .as_ref()
        .is_none_or(|target| target.width != width || target.height != height);
    if !resized {
        return false;
    }
    let color = device.create_texture(&TextureDescriptor {
        label: Some(color_label),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: SHADOWMAP_SPOT_COLOR_FORMAT,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&TextureViewDescriptor {
        label: Some(color_label),
        ..Default::default()
    });
    let depth = device.create_texture(&TextureDescriptor {
        label: Some(depth_label),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: SHADOWMAP_SPOT_DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&TextureViewDescriptor {
        label: Some(depth_label),
        ..Default::default()
    });
    *slot = Some(SpotGpuTarget {
        _color: color,
        color_view,
        _depth: depth,
        depth_view,
        width,
        height,
    });
    true
}

pub(super) fn ensure_shadowmap_spot_targets(gpu: &mut ShadowmapSpotGpu, device: &RenderDevice) {
    ensure_one(&mut gpu.rt10, device, GFX_SPOT_SHADOW_RT_LARGE);
    ensure_one(&mut gpu.rt11, device, GFX_SPOT_SHADOW_RT_SMALL);
}
