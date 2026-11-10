use bevy::prelude::*;

pub struct RenderGpuPlugin;

impl Plugin for RenderGpuPlugin {
    fn build(&self, app: &mut App) {
        let skin = crate::SkinSupport::default();
        app.insert_resource(skin.clone());
        app.add_plugins(bevy::render::extract_component::ExtractComponentPlugin::<
            frame::DisplayEncodedCamera,
        >::default());
        match crate::SamplerTable::captured_2026_08_11() {
            Ok(table) => {
                let adapted = table.host_adapted_rows();
                if !adapted.is_empty() {
                    diag::info!(
                        World,
                        "drawsurf sampler host adaptation: {} of 24 table rows lift mip filter to linear for anisotropy (rows={adapted:?})",
                        adapted.len()
                    );
                }
                app.insert_resource(table);
            }
            Err(cause) => diag::warn!(World, "drawsurf sampler profile: cause={cause:?}"),
        }
        crate::drawsurf::register_drawsurf_render(app);
        crate::drawsurf::scope::register(app);
        app.init_resource::<crate::GpuSubmitReady>()
            .init_resource::<crate::ColourWorkingSet>();
        let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) else {
            return;
        };
        render_app
            .insert_resource(skin)
            .init_resource::<crate::GpuSubmitReady>()
            .init_resource::<crate::ColourWorkingSet>()
            .init_resource::<crate::WorldPipelineWarmup>()
            .add_systems(
                bevy::render::ExtractSchedule,
                extract_display_encoded_formats
                    .after(bevy::render::camera::extract_cameras)
                    .before(bevy::ui_render::extract_ui_camera_view),
            )
            .add_systems(
                bevy::render::Render,
                resolve_display_encoded_views
                    .in_set(bevy::render::RenderSystems::CreateViews)
                    .after(bevy::render::view::ResolveCompositingSpaces),
            );
    }
}

#[expect(
    clippy::type_complexity,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn extract_display_encoded_formats(
    cameras: bevy::render::Extract<
        Query<
            (
                bevy::render::sync_world::RenderEntity,
                Has<bevy::camera::Hdr>,
            ),
            With<frame::DisplayEncodedCamera>,
        >,
    >,
    mut formats: ResMut<bevy::render::camera::CameraMainPassTextureFormats>,
) {
    // UI extraction must see the same unorm format as the custom scene passes.
    for (camera, hdr) in &cameras {
        if !hdr && let Some(format) = formats.get_mut(&camera) {
            *format = bevy::render::render_resource::TextureFormat::Rgba8Unorm;
        }
    }
}

fn resolve_display_encoded_views(
    mut views: Query<
        (
            &bevy::render::camera::ExtractedCamera,
            &mut bevy::render::view::ExtractedView,
            &mut bevy::render::view::ResolvedCompositingSpace,
        ),
        With<frame::DisplayEncodedCamera>,
    >,
) {
    // The custom scene and HUD passes write sRGB values into unorm targets;
    // Bevy's Camera3d resolver assumes linear output from its stock passes.
    // Deferred camera extraction must finish before the view format is changed.
    for (camera, mut view, mut space) in &mut views {
        if !camera.hdr {
            view.target_format = bevy::render::render_resource::TextureFormat::Rgba8Unorm;
        }
        space.0 = Some(bevy::camera::CompositingSpace::Srgb);
    }
}
