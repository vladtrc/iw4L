pub const GFX_VIEW_MODE_2D: i32 = 2;

pub const GFX_VIEW_MODE_NONE: i32 = 0;

pub const GFX_VIEW_MODE_3D: i32 = 1;

pub const GFX_SCENE_DEF_DWORDS: usize = 10;

pub const GFX_VIEWPARMS_DWORDS: usize = 0x54;

pub const GFX_VIEWPARMS_SIZE: usize = GFX_VIEWPARMS_DWORDS * 4;

pub const GFX_VIEWPARMS_INV_VP: usize = 0xC0;

pub const GFX_VIEWPARMS_INV_VP_M33: usize = 0xFC;

pub const GFX_VIEWPARMS_ORIGIN: usize = 0x100;

pub const GFX_VIEWPARMS_VIEW: usize = 0x00;

pub const GFX_VIEWPARMS_PROJECTION: usize = 0x40;

pub const GFX_VIEWPARMS_VIEW_PROJECTION: usize = 0x80;

pub const FLOAT64_ONE: f32 = 1.0;

pub const SET2D_M30_BIAS: f32 = -1.0;

pub const SET2D_M11_SCALE: f32 = -2.0;

pub const GFX_VIEWPORT_FULL: i32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxViewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxCmdBufSource2d {
    pub view_mode: i32,

    pub viewport_is_dirty: u8,

    pub eye_offset: [f32; 4],

    pub viewport_select: i32,

    pub render_target_width: i32,

    pub render_target_height: i32,

    pub scene_viewport: GfxViewport,

    pub view_parms: [u32; GFX_VIEWPARMS_DWORDS],

    pub scene_def: [u32; GFX_SCENE_DEF_DWORDS],

    pub skinned_placement_origin: [f32; 3],

    pub material_time: u32,

    pub nearplane_org: [f32; 4],

    pub nearplane_dx: [f32; 4],

    pub nearplane_dy: [f32; 4],
}

impl Default for GfxCmdBufSource2d {
    fn default() -> Self {
        Self {
            view_mode: 0,
            viewport_is_dirty: 0,
            eye_offset: [0.0, 0.0, 0.0, 0.0],
            viewport_select: GFX_VIEWPORT_FULL,
            render_target_width: 0,
            render_target_height: 0,
            scene_viewport: GfxViewport::default(),
            view_parms: [0; GFX_VIEWPARMS_DWORDS],
            scene_def: [0; GFX_SCENE_DEF_DWORDS],
            skinned_placement_origin: [0.0, 0.0, 0.0],
            material_time: 0,
            nearplane_org: [0.0, 0.0, 0.0, 0.0],
            nearplane_dx: [0.0, 0.0, 0.0, 0.0],
            nearplane_dy: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxSet2dMatrices {
    pub view: [f32; 16],
    pub projection: [f32; 16],
    pub view_projection: [f32; 16],
    pub inverse_view_projection: [f32; 16],
}

#[must_use]
pub fn r_get_viewport(source: &GfxCmdBufSource2d) -> GfxViewport {
    if source.viewport_select == GFX_VIEWPORT_FULL {
        GfxViewport {
            x: 0,
            y: 0,
            width: source.render_target_width,
            height: source.render_target_height,
        }
    } else {
        source.scene_viewport
    }
}

#[must_use]
pub fn r_cmd_buf_set_2d_projection(width: i32, height: i32) -> Option<[f32; 16]> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let inv_w = FLOAT64_ONE / (width as f32);
    let inv_h = FLOAT64_ONE / (height as f32);
    let mut m = [0.0f32; 16];
    m[0] = inv_w + inv_w;
    m[5] = SET2D_M11_SCALE * inv_h;
    m[12] = SET2D_M30_BIAS - inv_w;
    m[13] = inv_h + FLOAT64_ONE;
    m[14] = 1.0;
    m[15] = 1.0;
    Some(m)
}

fn identity44() -> [f32; 16] {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

#[must_use]
pub fn r_cmd_buf_set_2d(viewport: &GfxViewport) -> Option<GfxSet2dMatrices> {
    let projection = r_cmd_buf_set_2d_projection(viewport.width, viewport.height)?;
    Some(GfxSet2dMatrices {
        view: identity44(),
        projection,
        view_projection: projection,
        inverse_view_projection: [0.0; 16],
    })
}

#[must_use]
pub fn r_set_2d_clip_xy(x: f32, y: f32, projection: &[f32; 16]) -> (f32, f32) {
    (
        x * projection[0] + projection[12],
        y * projection[5] + projection[13],
    )
}

#[must_use]
pub fn r_set_2d_clip_coeffs(projection: &[f32; 16]) -> [f32; 4] {
    [projection[0], projection[5], projection[12], projection[13]]
}

#[must_use]
pub fn r_set_2d(source: &mut GfxCmdBufSource2d) -> Option<GfxSet2dMatrices> {
    if source.view_mode == GFX_VIEW_MODE_2D {
        return None;
    }
    source.view_mode = GFX_VIEW_MODE_2D;
    source.viewport_is_dirty = 1;
    source.eye_offset = [0.0, 0.0, 0.0, 1.0];
    let vp = r_get_viewport(source);
    r_cmd_buf_set_2d(&vp)
}

#[must_use]
pub fn gfx_viewparms_origin(view_parms: &[u32; GFX_VIEWPARMS_DWORDS]) -> [f32; 3] {
    let i = GFX_VIEWPARMS_ORIGIN / 4;
    [
        f32::from_bits(view_parms[i]),
        f32::from_bits(view_parms[i + 1]),
        f32::from_bits(view_parms[i + 2]),
    ]
}

#[must_use]
pub fn gfx_viewparms_write_matrix(
    view_parms: &mut [u32; GFX_VIEWPARMS_DWORDS],
    byte_off: usize,
    matrix: &[f32; 16],
) -> bool {
    if byte_off % 4 != 0 {
        return false;
    }
    let i = byte_off / 4;
    if i.saturating_add(16) > GFX_VIEWPARMS_DWORDS {
        return false;
    }
    for (k, cell) in matrix.iter().enumerate() {
        view_parms[i + k] = cell.to_bits();
    }
    true
}

pub fn gfx_viewparms_write_origin(
    view_parms: &mut [u32; GFX_VIEWPARMS_DWORDS],
    xyz: [f32; 3],
    w: f32,
) {
    let i = GFX_VIEWPARMS_ORIGIN / 4;
    view_parms[i] = xyz[0].to_bits();
    view_parms[i + 1] = xyz[1].to_bits();
    view_parms[i + 2] = xyz[2].to_bits();
    view_parms[i + 3] = w.to_bits();
}

#[must_use]
pub fn gfx_scene_def_float_time(float_time: f32) -> [u32; GFX_SCENE_DEF_DWORDS] {
    let mut scene = [0u32; GFX_SCENE_DEF_DWORDS];
    scene[1] = float_time.to_bits();
    scene
}

#[must_use]
pub fn r_cmd_buf_set_3d(eye_offset: [f32; 4]) -> [f32; 16] {
    let mut world = identity44();
    world[12] -= eye_offset[0];
    world[13] -= eye_offset[1];
    world[14] -= eye_offset[2];
    world
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxSet3dResult {
    pub world: [f32; 16],
}

#[must_use]
pub fn r_set_3d(
    source: &mut GfxCmdBufSource2d,
    view_parms_3d: &[u32; GFX_VIEWPARMS_DWORDS],
) -> Option<GfxSet3dResult> {
    if source.view_mode == GFX_VIEW_MODE_3D {
        return None;
    }
    source.view_mode = GFX_VIEW_MODE_3D;
    source.view_parms = *view_parms_3d;
    let origin = gfx_viewparms_origin(view_parms_3d);
    source.eye_offset = [origin[0], origin[1], origin[2], 1.0];
    Some(GfxSet3dResult {
        world: r_cmd_buf_set_3d(source.eye_offset),
    })
}

fn view_parm_f32(view_parms: &[u32; GFX_VIEWPARMS_DWORDS], byte: usize) -> f32 {
    f32::from_bits(view_parms[byte / 4])
}

#[must_use]
pub fn r_derive_near_plane_constants(
    view_parms: &[u32; GFX_VIEWPARMS_DWORDS],
) -> Option<GfxNearPlaneConstants> {
    let m33 = view_parm_f32(view_parms, GFX_VIEWPARMS_INV_VP_M33);
    if m33 == 0.0 {
        return None;
    }
    let mut scale = FLOAT64_ONE / m33;
    let origin = gfx_viewparms_origin(view_parms);
    let org = [
        view_parm_f32(view_parms, 0xF0) * scale - origin[0],
        scale * view_parm_f32(view_parms, 0xF4) - origin[1],
        view_parm_f32(view_parms, 0xF8) * scale - origin[2],
        0.0,
    ];
    scale += scale;
    let dx = [
        view_parm_f32(view_parms, GFX_VIEWPARMS_INV_VP) * scale,
        scale * view_parm_f32(view_parms, GFX_VIEWPARMS_INV_VP + 4),
        view_parm_f32(view_parms, GFX_VIEWPARMS_INV_VP + 8) * scale,
        0.0,
    ];
    scale = -scale;
    let dy = [
        scale * view_parm_f32(view_parms, GFX_VIEWPARMS_INV_VP + 0x10),
        scale * view_parm_f32(view_parms, GFX_VIEWPARMS_INV_VP + 0x14),
        view_parm_f32(view_parms, GFX_VIEWPARMS_INV_VP + 0x18) * scale,
        0.0,
    ];
    Some(GfxNearPlaneConstants { org, dx, dy })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxNearPlaneConstants {
    pub org: [f32; 4],
    pub dx: [f32; 4],
    pub dy: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxBeginViewResult {
    pub float_time: f32,
    pub set_3d: GfxSet3dResult,
    pub near_plane: Option<GfxNearPlaneConstants>,
}

#[must_use]
pub fn r_begin_view(
    source: &mut GfxCmdBufSource2d,
    scene_def: &[u32; GFX_SCENE_DEF_DWORDS],
    view_parms_3d: &[u32; GFX_VIEWPARMS_DWORDS],
) -> GfxBeginViewResult {
    source.scene_def = *scene_def;
    source.skinned_placement_origin = [
        f32::from_bits(scene_def[2]),
        f32::from_bits(scene_def[3]),
        f32::from_bits(scene_def[4]),
    ];
    source.view_mode = GFX_VIEW_MODE_NONE;
    let set_3d = r_set_3d(source, view_parms_3d).expect("viewMode was 0");
    let float_time = f32::from_bits(scene_def[1]);
    let near_plane = r_derive_near_plane_constants(&source.view_parms);
    if let Some(np) = near_plane {
        source.nearplane_org = np.org;
        source.nearplane_dx = np.dx;
        source.nearplane_dy = np.dy;
    }
    source.material_time = scene_def[5];
    GfxBeginViewResult {
        float_time,
        set_3d,
        near_plane,
    }
}
