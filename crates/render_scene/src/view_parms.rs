use bevy::prelude::*;

use crate::frustum::{FrustumSpec, perspective_frustum_planes};

#[must_use]
pub fn pack_live_view_parms(
    view_from_world: Mat4,
    clip_from_view: Mat4,
    clip_from_world: Mat4,
    view_origin: Vec3,
) -> [u32; hud_iw4::GFX_VIEWPARMS_DWORDS] {
    use hud_iw4::{
        GFX_VIEWPARMS_INV_VP, GFX_VIEWPARMS_PROJECTION, GFX_VIEWPARMS_VIEW,
        GFX_VIEWPARMS_VIEW_PROJECTION, gfx_viewparms_write_matrix, gfx_viewparms_write_origin,
    };

    let mut parms = [0u32; hud_iw4::GFX_VIEWPARMS_DWORDS];
    let _ = gfx_viewparms_write_matrix(
        &mut parms,
        GFX_VIEWPARMS_VIEW,
        &view_from_world.to_cols_array(),
    );
    let _ = gfx_viewparms_write_matrix(
        &mut parms,
        GFX_VIEWPARMS_PROJECTION,
        &clip_from_view.to_cols_array(),
    );
    let _ = gfx_viewparms_write_matrix(
        &mut parms,
        GFX_VIEWPARMS_VIEW_PROJECTION,
        &clip_from_world.to_cols_array(),
    );
    let det = clip_from_world.determinant();
    if det != 0.0 && det.is_finite() {
        let inv = clip_from_world.inverse();
        let _ = gfx_viewparms_write_matrix(&mut parms, GFX_VIEWPARMS_INV_VP, &inv.to_cols_array());
    }
    gfx_viewparms_write_origin(&mut parms, view_origin.to_array(), 1.0);
    parms
}

#[must_use]
pub fn host_clip_from_view(fov: f32, aspect: f32, near: f32) -> Mat4 {
    Mat4::perspective_infinite_reverse_rh(fov, aspect, near)
}

#[derive(Resource, Clone, Debug)]
pub struct PreparedSceneView {
    pub ready: bool,
    pub eye: Vec3,

    pub forward: Vec3,
    pub view_from_world: Mat4,
    pub clip_from_view: Mat4,
    pub clip_from_world: Mat4,
    pub frustum_planes: Vec<[f32; 4]>,
    pub fov: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    pub rt_w: i32,
    pub rt_h: i32,
    pub parms: [u32; hud_iw4::GFX_VIEWPARMS_DWORDS],

    pub portal_bevels: Option<dpvs_iw4::PortalBevels>,

    pub depth_hack_near: f32,

    pub scene_viewport: hud_iw4::GfxViewport,
}

impl PreparedSceneView {
    #[must_use]
    pub fn tan_half_fov_y(&self) -> f32 {
        (self.fov * 0.5).tan()
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct RZnearDvar {
    pub value: f32,
}

impl Default for RZnearDvar {
    fn default() -> Self {
        Self {
            value: hud_iw4::R_ZNEAR_DEFAULT,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct RZnearDepthhackDvar {
    pub value: f32,
}

impl Default for RZnearDepthhackDvar {
    fn default() -> Self {
        Self {
            value: hud_iw4::R_ZNEAR_DEPTHHACK_DEFAULT,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct RSubwindowDvar {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Default for RSubwindowDvar {
    fn default() -> Self {
        let [left, right, top, bottom] = hud_iw4::R_SUBWINDOW_DEFAULT;
        Self {
            left,
            right,
            top,
            bottom,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LockPvsView {
    pub eye: Vec3,
    pub forward: Vec3,
    pub frustum_planes: Vec<[f32; 4]>,
    pub portal_bevels: Option<dpvs_iw4::PortalBevels>,
    pub fov: f32,
    pub aspect: f32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct RLockPvs {
    pub enabled: bool,

    pub recapture: bool,
    pub frozen: Option<LockPvsView>,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SmEnableDvar {
    pub enabled: Option<bool>,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SmSunEnableDvar {
    pub enabled: Option<bool>,
}

impl RLockPvs {
    pub fn apply_after_stamp(&mut self, live: &PreparedSceneView) {
        if !self.enabled {
            self.frozen = None;
            self.recapture = false;
            return;
        }
        if self.recapture || self.frozen.is_none() {
            self.frozen = Some(LockPvsView {
                eye: live.eye,
                forward: live.forward,
                frustum_planes: live.frustum_planes.clone(),
                portal_bevels: live.portal_bevels,
                fov: live.fov,
                aspect: live.aspect,
            });
            self.recapture = false;
        }
    }

    #[must_use]
    pub fn dpvs_eye(&self, live: &PreparedSceneView) -> Vec3 {
        match (self.enabled, &self.frozen) {
            (true, Some(f)) => f.eye,
            _ => live.eye,
        }
    }

    #[must_use]
    pub fn dpvs_forward(&self, live: &PreparedSceneView) -> Vec3 {
        match (self.enabled, &self.frozen) {
            (true, Some(f)) => f.forward,
            _ => live.forward,
        }
    }

    #[must_use]
    pub fn dpvs_frustum<'a>(&'a self, live: &'a PreparedSceneView) -> &'a [[f32; 4]] {
        match (self.enabled, &self.frozen) {
            (true, Some(f)) => f.frustum_planes.as_slice(),
            _ => live.frustum_planes.as_slice(),
        }
    }

    #[must_use]
    pub fn dpvs_portal_bevels(&self, live: &PreparedSceneView) -> Option<dpvs_iw4::PortalBevels> {
        match (self.enabled, &self.frozen) {
            (true, Some(f)) => f.portal_bevels,
            _ => live.portal_bevels,
        }
    }

    #[must_use]
    pub fn dpvs_fov_aspect(&self, live: &PreparedSceneView) -> (f32, f32) {
        match (self.enabled, &self.frozen) {
            (true, Some(f)) => (f.fov, f.aspect),
            _ => (live.fov, live.aspect),
        }
    }
}

impl Default for PreparedSceneView {
    fn default() -> Self {
        Self {
            ready: false,
            eye: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            view_from_world: Mat4::IDENTITY,
            clip_from_view: Mat4::IDENTITY,
            clip_from_world: Mat4::IDENTITY,
            frustum_planes: Vec::new(),
            fov: 0.0,
            aspect: 0.0,
            near: 0.0,
            far: 0.0,
            rt_w: 0,
            rt_h: 0,
            parms: [0u32; hud_iw4::GFX_VIEWPARMS_DWORDS],
            portal_bevels: None,
            depth_hack_near: hud_iw4::R_ZNEAR_DEPTHHACK_DEFAULT,
            scene_viewport: hud_iw4::GfxViewport::default(),
        }
    }
}

#[must_use]
pub fn lens_world_from_parent_child(parent: Transform, child: Transform) -> Mat4 {
    parent.to_matrix() * child.to_matrix()
}

#[must_use]
pub fn prepare_scene_view(
    world_from_local: Mat4,
    fov: f32,
    aspect: f32,
    near: f32,
    far: f32,
    rt_w: i32,
    rt_h: i32,
    dpvs_z_near: f32,
    depth_hack_near: f32,
    scene_viewport: hud_iw4::GfxViewport,
) -> PreparedSceneView {
    let eye = world_from_local.transform_point3(Vec3::ZERO);
    let view_from_world = world_from_local.inverse();
    let clip_from_view = host_clip_from_view(fov, aspect, near);
    let clip_from_world = clip_from_view * view_from_world;
    let forward = world_from_local.transform_vector3(Vec3::NEG_Z);
    let right = world_from_local.transform_vector3(Vec3::X);
    let up = world_from_local.transform_vector3(Vec3::Y);
    let mut frustum_planes = perspective_frustum_planes(FrustumSpec {
        eye,
        forward,
        right,
        up,
        fov_y_rad: fov,
        aspect,
        near,
        far,
    })
    .to_vec();
    if no_frustum_env() {
        frustum_planes.clear();
    } else if frustum_nearfar_env() {
        frustum_planes.truncate(2);
    }
    let parms = pack_live_view_parms(view_from_world, clip_from_view, clip_from_world, eye);
    let tan_half_y = (fov * 0.5).tan();
    let tan_half_x = tan_half_y * aspect;
    let axis = [
        [forward.x, forward.y, forward.z],
        [right.x, right.y, right.z],
        [up.x, up.y, up.z],
    ];
    let portal_bevels = hud_iw4::r_set_view_parms_matrices(
        eye.to_array(),
        axis,
        tan_half_x,
        tan_half_y,
        dpvs_z_near,
    )
    .map(|(vp, inv)| dpvs_iw4::portal_bevels_from_d3d_row_major(&vp, &inv));
    PreparedSceneView {
        ready: true,
        eye,
        forward,
        view_from_world,
        clip_from_view,
        clip_from_world,
        frustum_planes,
        fov,
        aspect,
        near,
        far,
        rt_w,
        rt_h,
        parms,
        portal_bevels,
        depth_hack_near,
        scene_viewport,
    }
}

fn no_frustum_env() -> bool {
    matches!(
        std::env::var("IW4L_NO_FRUSTUM").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

fn frustum_nearfar_env() -> bool {
    matches!(
        std::env::var("IW4L_FRUSTUM_NEARFAR").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}
