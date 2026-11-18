//! Fullscreen render resolution: the world camera renders the chosen number of
//! rows at the window's aspect into an image that a present camera stretches
//! over the window.

use bevy::camera::{CameraOutputMode, ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{
    BlendComponent, BlendFactor, BlendOperation, BlendState, TextureFormat,
};
use bevy::render::view::DebandDither;
use bevy::ui::UiTargetCamera;
use bevy::ui::widget::NodeImageMode;
use bevy::window::{PrimaryWindow, WindowRef};
use render_scene::FpvLens;

#[derive(Component)]
struct PresentCamera;

#[derive(Component)]
struct PresentImage;

#[derive(Resource, Default)]
struct ScaledTarget(Option<(UVec2, Handle<Image>)>);

pub(crate) fn register(app: &mut App) {
    app.init_resource::<ScaledTarget>()
        .add_systems(Update, sync_render_resolution);
}

fn scaled_size(settings: &frame::GameSettings, window: UVec2) -> Option<UVec2> {
    let rows = settings.resolution.height;
    if !settings.fullscreen || rows == 0 || window.y == 0 || rows >= window.y {
        return None;
    }
    let columns =
        (u64::from(window.x) * u64::from(rows) + u64::from(window.y) / 2) / u64::from(window.y);
    Some(UVec2::new(u32::try_from(columns).ok()?.max(1), rows))
}

fn target_image(size: UVec2) -> Image {
    let mut image = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    image.data = None;
    image.sampler = ImageSampler::linear();
    image
}

/// The scene and HUD passes leave arbitrary alpha behind (sky, compass) that an
/// opaque window ignores. The image is blended onto the window, so its alpha is
/// kept at the cleared 1.0.
fn opaque_output() -> CameraOutputMode {
    CameraOutputMode::Write {
        blend_state: Some(BlendState {
            color: BlendComponent::REPLACE,
            alpha: BlendComponent {
                src_factor: BlendFactor::Zero,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
        }),
        clear_color: ClearColorConfig::Custom(Color::BLACK),
    }
}

#[derive(SystemParam)]
struct ScaledPresentation<'w, 's> {
    commands: Commands<'w, 's>,
    presents: Query<'w, 's, Entity, With<PresentCamera>>,
    nodes: Query<'w, 's, Entity, With<PresentImage>>,
    images: ResMut<'w, Assets<Image>>,
    scaled: ResMut<'w, ScaledTarget>,
}

impl ScaledPresentation<'_, '_> {
    fn clear(&mut self) {
        let Self {
            commands,
            presents,
            nodes,
            scaled,
            ..
        } = self;
        if scaled.0.take().is_some() {
            for entity in presents.iter().chain(nodes.iter()) {
                commands.entity(entity).despawn();
            }
        }
    }

    fn ensure(&mut self, size: UVec2) -> Option<&Handle<Image>> {
        let Self {
            commands,
            presents,
            nodes,
            images,
            scaled,
        } = self;
        if scaled
            .0
            .as_ref()
            .is_none_or(|(current, _)| *current != size)
        {
            let handle = images.add(target_image(size));
            for entity in nodes.iter() {
                commands.entity(entity).despawn();
            }
            let camera = presents.iter().next().unwrap_or_else(|| {
                commands
                    .spawn((
                        PresentCamera,
                        Camera2d,
                        Camera {
                            order: 1,
                            clear_color: ClearColorConfig::Custom(Color::BLACK),
                            ..default()
                        },
                        // Msaa stays at its default: 2D cameras on one window share a
                        // depth texture keyed by target alone, so their sample counts
                        // must agree with the overlay camera's.
                        Tonemapping::None,
                        DebandDither::Disabled,
                    ))
                    .id()
            });
            commands.spawn((
                PresentImage,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                ImageNode::new(handle.clone()).with_mode(NodeImageMode::Stretch),
                UiTargetCamera(camera),
                Pickable::IGNORE,
            ));
            scaled.0 = Some((size, handle));
        }
        scaled.0.as_ref().map(|(_, handle)| handle)
    }
}

fn sync_render_resolution(
    settings: Option<Res<frame::GameSettings>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut lenses: Query<(&mut Camera, &mut RenderTarget, &mut Projection), With<FpvLens>>,
    mut presentation: ScaledPresentation,
) {
    let want = match (settings, windows.single()) {
        (Some(settings), Ok(window)) => scaled_size(
            &settings,
            UVec2::new(window.physical_width(), window.physical_height()),
        )
        .map(|size| (size, window.height())),
        _ => None,
    };
    let Some((size, logical_height)) = want.filter(|_| !lenses.is_empty()) else {
        presentation.clear();
        for (mut camera, mut target, mut projection) in &mut lenses {
            if !matches!(*target, RenderTarget::Window(WindowRef::Primary)) {
                *target = RenderTarget::Window(WindowRef::Primary);
                camera.output_mode = CameraOutputMode::default();
                // The camera recomputes its target size only for changed windows,
                // images or projections, not for a replaced target.
                projection.set_changed();
            }
        }
        return;
    };
    let Some(handle) = presentation.ensure(size) else {
        return;
    };
    // Logical size matches the window, so HUD projections of world points line up.
    let scale_factor = size.y as f32 / logical_height.max(1.0);
    for (mut camera, mut target, mut projection) in &mut lenses {
        let current = matches!(
            &*target,
            RenderTarget::Image(image)
                if image.handle == *handle && image.scale_factor == scale_factor
        );
        if !current {
            *target = RenderTarget::Image(ImageRenderTarget {
                handle: handle.clone(),
                scale_factor,
            });
            camera.output_mode = opaque_output();
            projection.set_changed();
        }
    }
}
