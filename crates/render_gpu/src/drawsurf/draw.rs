use bevy::prelude::*;
use bevy::render::{Render, RenderApp, RenderSystems};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ExactColourDrawSet;

pub(crate) fn register_drawsurf_render(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        diag::warn!(
            World,
            "drawsurf: RenderApp missing — exact Colour execution cannot be extracted"
        );
        return;
    };

    render_app
        .init_resource::<super::colour_submit::FocusedOwnerSubmitState>()
        .init_resource::<super::gpu_resources::ExtractedRuntimeImageHandles>()
        .init_resource::<super::gpu_resources::RuntimeUploadedImageRegistry>()
        .add_systems(
            Render,
            super::gpu_resources::prepare_uploaded_image_registry
                .in_set(RenderSystems::PrepareResources),
        );
    render_app.add_systems(
        Render,
        super::scene_depth::prepare_scene_depth.in_set(RenderSystems::PrepareResources),
    );
    super::colour_submit::register(app);
    super::sun_effects::register(app);
    super::postfx::register(app);
    super::iw_tess::register(app);
    super::geometry_diagnostic::register(app);
}
