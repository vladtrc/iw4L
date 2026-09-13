pub use render_scene::{
    LockPvsView, PreparedSceneView, RLockPvs, RSubwindowDvar, RZnearDepthhackDvar, RZnearDvar,
    SmEnableDvar, SmSunEnableDvar, host_clip_from_view, lens_world_from_parent_child,
    pack_live_view_parms, prepare_scene_view,
};

use bevy::camera::Viewport;
use bevy::prelude::*;

use crate::prepare::scene::camera::{FlyCamera, FpvLens};

pub(crate) fn stamp_prepared_scene_view(
    mut view: ResMut<PreparedSceneView>,
    mut lock_pvs: ResMut<RLockPvs>,
    mut lod_parms: (
        ResMut<crate::prepare::scene::smodel_geom_cache::LodRampDvar>,
        ResMut<crate::prepare::scene::smodel_geom_cache::LodRampSkinnedDvar>,
    ),
    znear: Res<RZnearDvar>,
    fog: Res<crate::assemble::drawsurf::fog::FogDvars>,
    map_fog: Option<Res<crate::assemble::drawsurf::MapFrameFog>>,
    clock: Option<Res<net::CgFrameClock>>,
    depthhack: Res<RZnearDepthhackDvar>,
    subwindow: Res<RSubwindowDvar>,
    hosts: Query<&Transform, With<FlyCamera>>,
    mut lenses: Query<(&Transform, &Projection, &mut Camera), With<FpvLens>>,
) {
    *view = PreparedSceneView::default();
    let Ok(parent) = hosts.single() else {
        return;
    };
    let Ok((child, projection, mut camera)) = lenses.single_mut() else {
        return;
    };
    let Projection::Perspective(persp) = projection else {
        return;
    };
    let mut aspect = persp.aspect_ratio;
    if aspect < 1e-3
        && let Some(vw) = camera.logical_viewport_size()
        && vw.y > 0.0
    {
        aspect = vw.x / vw.y;
    }
    if aspect < 1e-3 {
        aspect = 16.0 / 9.0;
    }
    let (rt_w, rt_h) = camera
        .physical_target_size()
        .and_then(|rt| i32::try_from(rt.x).ok().zip(i32::try_from(rt.y).ok()))
        .unwrap_or((0, 0));
    let scene_viewport = hud_iw4::r_subwindow_to_viewport(
        subwindow.left,
        subwindow.right,
        subwindow.top,
        subwindow.bottom,
        rt_w,
        rt_h,
    );
    if hud_iw4::r_subwindow_is_full(
        subwindow.left,
        subwindow.right,
        subwindow.top,
        subwindow.bottom,
    ) || scene_viewport.width <= 0
        || scene_viewport.height <= 0
        || scene_viewport.x < 0
        || scene_viewport.y < 0
    {
        camera.viewport = None;
    } else {
        camera.viewport = Some(Viewport {
            physical_position: UVec2::new(scene_viewport.x as u32, scene_viewport.y as u32),
            physical_size: UVec2::new(scene_viewport.width as u32, scene_viewport.height as u32),
            depth: 0.0..1.0,
        });
    }

    camera.clear_color = match map_fog
        .as_ref()
        .map(|f| f.sample(clock.as_ref().map(|c| c.time()).unwrap_or(0)))
    {
        Some(f) if fog.zfar > 0.0 && f.density() != 0.0 => {
            let rgb = f
                .color_rgb
                .map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5).floor() / 255.0);
            bevy::camera::ClearColorConfig::Custom(Color::linear_rgba(rgb[0], rgb[1], rgb[2], 1.0))
        }
        _ => bevy::camera::ClearColorConfig::Default,
    };
    let dpvs_z_near = hud_iw4::r_znear_from_refdef(0.0, znear.value);
    *view = prepare_scene_view(
        lens_world_from_parent_child(*parent, *child),
        persp.fov,
        aspect,
        persp.near,
        if fog.zfar > 0.0 { fog.zfar } else { persp.far },
        rt_w,
        rt_h,
        dpvs_z_near,
        depthhack.value,
        scene_viewport,
    );
    lock_pvs.apply_after_stamp(&view);

    let (rigid, skinned) = &mut lod_parms;
    crate::prepare::scene::smodel_geom_cache::update_lod_parms(
        view.tan_half_fov_y(),
        rigid,
        skinned,
    );
}
