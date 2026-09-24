use bevy::camera::{Camera3d, Camera3dDepthLoadOp};
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_resource::*;
use bevy::render::renderer::RenderDevice;
use bevy::render::texture::TextureCache;
use bevy::render::view::{Msaa, ViewDepthTexture};

pub const SCENE_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth24PlusStencil8;

#[derive(Component)]
pub struct SceneDepthTexture {
    pub texture: Texture,
    attachment: ViewDepthTexture,
    sampled: TextureView,
}

impl SceneDepthTexture {
    pub fn view(&self) -> &TextureView {
        &self.sampled
    }

    pub fn get_attachment(&self, store: StoreOp) -> RenderPassDepthStencilAttachment<'_> {
        let mut attachment = self.attachment.get_attachment(store);
        attachment.stencil_ops = Some(Operations {
            load: if attachment
                .depth_ops
                .as_ref()
                .is_some_and(|ops| matches!(ops.load, LoadOp::Clear(_)))
            {
                LoadOp::Clear(0)
            } else {
                LoadOp::Load
            },
            store: StoreOp::Store,
        });
        attachment
    }
}

pub(crate) fn prepare_scene_depth(
    mut commands: Commands,
    device: Res<RenderDevice>,
    mut cache: ResMut<TextureCache>,
    mut sampled_views: Local<std::collections::HashMap<TextureId, TextureView>>,
    views: Query<(Entity, &ExtractedCamera, &Camera3d, &Msaa)>,
) {
    let mut textures = std::collections::HashMap::new();
    let mut live = Vec::new();
    for (entity, camera, settings, msaa) in &views {
        let Some(size) = camera.physical_target_size else {
            continue;
        };
        let texture = textures
            .entry((camera.target.clone(), *msaa))
            .or_insert_with(|| {
                cache.get(
                    &device,
                    TextureDescriptor {
                        label: Some("scene_depth_stencil"),
                        size: Extent3d {
                            width: size.x,
                            height: size.y,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: msaa.samples(),
                        dimension: TextureDimension::D2,
                        format: SCENE_DEPTH_FORMAT,
                        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                )
            })
            .clone();
        let id = texture.texture.id();
        live.push(id);
        let sampled = sampled_views
            .entry(id)
            .or_insert_with(|| {
                texture.texture.create_view(&TextureViewDescriptor {
                    aspect: TextureAspect::DepthOnly,
                    ..Default::default()
                })
            })
            .clone();
        let raw = texture.texture.clone();
        commands.entity(entity).insert(SceneDepthTexture {
            texture: raw,
            sampled,
            attachment: ViewDepthTexture::new(
                texture,
                match settings.depth_load_op {
                    Camera3dDepthLoadOp::Clear(value) => Some(value),
                    Camera3dDepthLoadOp::Load => None,
                },
            ),
        });
    }
    sampled_views.retain(|id, _| live.contains(id));
}

pub(crate) fn stencil_state(bits: u32) -> StencilState {
    if bits & 0x40 == 0 {
        return StencilState::default();
    }
    let op = |value: u32| match value & 7 {
        0 => StencilOperation::Keep,
        1 => StencilOperation::Zero,
        2 => StencilOperation::Replace,
        3 => StencilOperation::IncrementClamp,
        4 => StencilOperation::DecrementClamp,
        5 => StencilOperation::Invert,
        6 => StencilOperation::IncrementWrap,
        _ => StencilOperation::DecrementWrap,
    };
    let face = |shift: u32| {
        let value = bits >> shift;
        StencilFaceState {
            pass_op: op(value),
            fail_op: op(value >> 3),
            depth_fail_op: op(value >> 6),
            compare: match (value >> 9) & 7 {
                0 => CompareFunction::Never,
                1 => CompareFunction::Less,
                2 => CompareFunction::Equal,
                3 => CompareFunction::LessEqual,
                4 => CompareFunction::Greater,
                5 => CompareFunction::NotEqual,
                6 => CompareFunction::GreaterEqual,
                _ => CompareFunction::Always,
            },
        }
    };
    let front = face(8);
    StencilState {
        front,
        back: if bits & 0x80 != 0 { face(20) } else { front },
        read_mask: 0xff,
        write_mask: 0xff,
    }
}
