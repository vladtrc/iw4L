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
    clear_pipeline: Option<bevy::render::render_resource::RenderPipeline>,
    smodel_index_epochs: [Vec<Buffer>; lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP],
    smodel_index_epoch_bytes: [Vec<u64>; lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP],
    xmodel_index_epochs: [Vec<Buffer>; lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP],
    xmodel_index_epoch_bytes: [Vec<u64>; lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP],
}

impl ShadowmapSpotGpu {
    pub(super) fn clear_pipeline(&self) -> Option<&bevy::render::render_resource::RenderPipeline> {
        self.clear_pipeline.as_ref()
    }

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
            .expect("emitted spot-shadow slot is in the slot table");
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
            .expect("emitted spot-shadow slot is in the slot table");
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
    if gpu.clear_pipeline.is_none() {
        gpu.clear_pipeline = Some(create_slot_clear_pipeline(device));
    }
    ensure_one(&mut gpu.rt10, device, GFX_SPOT_SHADOW_RT_LARGE);
    ensure_one(&mut gpu.rt11, device, GFX_SPOT_SHADOW_RT_SMALL);
}

const SLOT_CLEAR_SHADER: &str = r#"
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
    return vec4(p[index], 1., 1.);
}
@fragment fn fs() -> @location(0) f32 { return 1.; }
"#;

fn create_slot_clear_pipeline(
    device: &RenderDevice,
) -> bevy::render::render_resource::RenderPipeline {
    use bevy::render::render_resource::*;
    let shader = device.create_and_validate_shader_module(ShaderModuleDescriptor {
        label: Some("spot shadow tile clear"),
        source: ShaderSource::Wgsl(SLOT_CLEAR_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("spot shadow tile clear"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    device.create_render_pipeline(&RawRenderPipelineDescriptor {
        label: Some("spot shadow tile clear"),
        layout: Some(&layout),
        vertex: RawVertexState {
            module: &shader,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(RawFragmentState {
            module: &shader,
            entry_point: Some("fs"),
            targets: &[Some(ColorTargetState {
                format: SHADOWMAP_SPOT_COLOR_FORMAT,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(DepthStencilState {
            format: SHADOWMAP_SPOT_DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(CompareFunction::Always),
            stencil: StencilState {
                front: StencilFaceState {
                    compare: CompareFunction::Always,
                    pass_op: StencilOperation::Replace,
                    ..Default::default()
                },
                back: StencilFaceState {
                    compare: CompareFunction::Always,
                    pass_op: StencilOperation::Replace,
                    ..Default::default()
                },
                read_mask: 0xff,
                write_mask: 0xff,
            },
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}
