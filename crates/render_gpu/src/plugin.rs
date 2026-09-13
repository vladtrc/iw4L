use bevy::prelude::*;

pub struct RenderGpuPlugin;

impl Plugin for RenderGpuPlugin {
    fn build(&self, app: &mut App) {
        match crate::RetailSamplerTable::captured_2026_08_11() {
            Ok(table) => {
                let adapted = table.host_adapted_rows();
                if !adapted.is_empty() {
                    diag::warn!(
                        World,
                        "drawsurf sampler host adaptation: {} of 24 table rows lift mip filter to linear for anisotropy (rows={adapted:?})",
                        adapted.len()
                    );
                }
                app.insert_resource(table);
            }
            Err(cause) => diag::warn!(
                World,
                "drawsurf retail sampler profile: RED capture=2026-08-11 cause={cause:?}"
            ),
        }
        crate::drawsurf::register_drawsurf_render(app);
        app.init_resource::<crate::GpuSubmitReady>()
            .init_resource::<crate::ColourWorkingSet>();
        let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) else {
            return;
        };
        render_app
            .init_resource::<crate::GpuSubmitReady>()
            .init_resource::<crate::ColourWorkingSet>()
            .init_resource::<crate::WorldPipelineWarmup>();
    }
}
