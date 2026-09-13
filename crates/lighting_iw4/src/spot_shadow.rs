pub const GFX_SHADOWABLE_SLOT_STRIDE: usize = 0x1e0;

pub const GFX_SPOT_SHADOW_CMDBUF_ROW_STRIDE: usize = 0x94;

pub const GFX_SPOT_SHADOW_RT_LARGE: u8 = 10;

pub const GFX_SPOT_SHADOW_RT_SMALL: u8 = 11;

pub const GFX_SPOT_SHADOW_VIEWPORT_SMALL: u32 = 0x200;

pub const GFX_SPOT_SHADOW_VIEWPORT_LARGE: u32 = 0x400;

pub const SPOT_SHADOW_PIXEL_SCALE: f32 = 0.25;

pub const SPOT_SHADOW_HALF: f32 = 0.5;

pub const SPOT_SHADOW_PIXEL_SCALE_NEG: f32 = -0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpotShadowSlotPlan {
    pub render_target_id: u8,
    pub viewport_x: u32,
    pub viewport_y: u32,
    pub viewport_size: u32,
    pub atlas_columns: u32,
    pub clear: bool,
}

impl SpotShadowSlotPlan {
    pub fn pixel_adjust(self) -> [f32; 4] {
        let size = self.viewport_size as f32;
        let cols = self.atlas_columns as f32;
        [
            SPOT_SHADOW_PIXEL_SCALE / size,
            SPOT_SHADOW_HALF / (cols * size),
            SPOT_SHADOW_HALF / size,
            SPOT_SHADOW_PIXEL_SCALE_NEG / (cols * size),
        ]
    }
}

pub fn spot_shadow_slot_plan(fast: bool, slot_index: u32, rand_zero: bool) -> SpotShadowSlotPlan {
    if !fast || slot_index > 1 {
        let clear = if !fast {
            slot_index == 0
        } else if slot_index == 2 && rand_zero {
            true
        } else {
            false
        };
        SpotShadowSlotPlan {
            render_target_id: GFX_SPOT_SHADOW_RT_SMALL,
            viewport_x: 0,
            viewport_y: GFX_SPOT_SHADOW_VIEWPORT_SMALL * slot_index,
            viewport_size: GFX_SPOT_SHADOW_VIEWPORT_SMALL,
            atlas_columns: 4,
            clear,
        }
    } else {
        SpotShadowSlotPlan {
            render_target_id: GFX_SPOT_SHADOW_RT_LARGE,
            viewport_x: 0,
            viewport_y: GFX_SPOT_SHADOW_VIEWPORT_LARGE * slot_index,
            viewport_size: GFX_SPOT_SHADOW_VIEWPORT_LARGE,
            atlas_columns: 2,
            clear: slot_index == 0,
        }
    }
}

pub fn spot_shadow_consume_order(render_target_ids: &[u8]) -> impl Iterator<Item = usize> + '_ {
    let small = render_target_ids
        .iter()
        .enumerate()
        .filter(|(_, id)| **id == GFX_SPOT_SHADOW_RT_SMALL)
        .map(|(i, _)| i);
    let large = render_target_ids
        .iter()
        .enumerate()
        .filter(|(_, id)| **id == GFX_SPOT_SHADOW_RT_LARGE)
        .map(|(i, _)| i);
    small.chain(large)
}

#[must_use]
pub fn spot_shadow_packed_lists_ready(work_entry_n: u32) -> bool {
    work_entry_n != 0
}

pub const SPOT_SHADOW_BSP_PRETESS_CLAIM: i32 = 0x600;

#[must_use]
pub fn spot_shadow_scene_dobj_vis(vis: &[u8], slot: u32, scene_index: u32) -> bool {
    let i = (slot as usize)
        .saturating_mul(SPOT_SHADOW_SCENE_DOBJ_VIS_STRIDE)
        .saturating_add(scene_index as usize);
    vis.get(i).copied() == Some(1)
}

#[must_use]
pub fn spot_shadow_scene_model_vis(vis: &[u8], slot: u32, scene_index: u32) -> bool {
    let i = (slot as usize)
        .saturating_mul(SPOT_SHADOW_SCENE_MODEL_VIS_STRIDE)
        .saturating_add(scene_index as usize);
    vis.get(i).copied() == Some(1)
}

pub fn spot_shadow_tess_bsp_surfs(walks_primary: bool, surfs: &[u16], out: &mut [u16]) -> usize {
    if !walks_primary {
        return 0;
    }
    let n = surfs.len().min(out.len());
    out[..n].copy_from_slice(&surfs[..n]);
    n
}

pub fn spot_shadow_tess_smodel_ids(walks_primary: bool, ids: &[u16], out: &mut [u16]) -> usize {
    spot_shadow_tess_bsp_surfs(walks_primary, ids, out)
}

#[must_use]
pub fn spot_shadow_smodel_casts(flags: u8) -> bool {
    flags & crate::smodel_lighting::STATIC_MODEL_FLAG_NO_CAST_SHADOW == 0
}

#[must_use]
pub fn spot_shadow_efd0_partition_vis(camera: u8, hide_bit_set: bool) -> u8 {
    if hide_bit_set {
        camera
    } else if camera == 0 {
        1
    } else {
        camera
    }
}

#[must_use]
pub fn spot_shadow_efd0_spot_vis(camera: u8) -> u8 {
    camera
}

#[must_use]
pub fn spot_shadow_efd0_zero_all(byte: u8, ready: bool) -> u8 {
    if (byte & 1) != 0 && !ready { 0 } else { byte }
}

#[must_use]
pub fn spot_shadow_175d0_pose_ok(cull_gate: u32) -> bool {
    cull_gate == 2
}

#[must_use]
pub fn spot_shadow_17410_lod_ok(lod: Option<i8>) -> bool {
    match lod {
        None => true,
        Some(lod) => lod >= 0,
    }
}

pub fn spot_shadow_filter_smodel_ids(
    walks_primary: bool,
    ids: &[u16],
    mut flags_at: impl FnMut(u16) -> Option<u8>,
    out: &mut [u16],
) -> usize {
    if !walks_primary {
        return 0;
    }
    let mut n = 0usize;
    for &id in ids {
        match flags_at(id) {
            Some(flags) if spot_shadow_smodel_casts(flags) => {
                if n < out.len() {
                    out[n] = id;
                    n += 1;
                }
            }
            Some(_) | None => {}
        }
    }
    n
}

pub fn spot_shadow_scene_dobj_vis_write(vis: &mut [u8], slot: u32, scene_index: u32, byte: u8) {
    let i = (slot as usize)
        .saturating_mul(SPOT_SHADOW_SCENE_DOBJ_VIS_STRIDE)
        .saturating_add(scene_index as usize);
    if let Some(slot) = vis.get_mut(i) {
        *slot = byte;
    }
}

pub fn spot_shadow_fill_scene_dobj_vis(
    vis: &mut [u8],
    slot: u32,
    occupancy_entnums: &[u32],
    camera_vis: Option<&[u8]>,
    pose_ok: Option<&[bool]>,
) {
    let Some(cam) = camera_vis else {
        return;
    };
    for (i, &entnum) in occupancy_entnums.iter().enumerate() {
        let mut byte = spot_shadow_efd0_spot_vis(cam.get(entnum as usize).copied().unwrap_or(0));
        if let Some(ok) = pose_ok.and_then(|p| p.get(i)).copied() {
            byte = spot_shadow_efd0_zero_all(byte, ok);
        }
        spot_shadow_scene_dobj_vis_write(vis, slot, i as u32, byte);
    }
}

pub fn spot_shadow_scene_model_vis_write(vis: &mut [u8], slot: u32, scene_index: u32, byte: u8) {
    let i = (slot as usize)
        .saturating_mul(SPOT_SHADOW_SCENE_MODEL_VIS_STRIDE)
        .saturating_add(scene_index as usize);
    if let Some(slot) = vis.get_mut(i) {
        *slot = byte;
    }
}

pub fn spot_shadow_fill_scene_model_vis(
    vis: &mut [u8],
    slot: u32,
    occupancy_entnums: &[u32],
    camera_vis: Option<&[u8]>,
    lod_ok: Option<&[bool]>,
) {
    let Some(cam) = camera_vis else {
        return;
    };
    for (i, &entnum) in occupancy_entnums.iter().enumerate() {
        let mut byte = spot_shadow_efd0_spot_vis(cam.get(entnum as usize).copied().unwrap_or(0));
        if let Some(ok) = lod_ok.and_then(|p| p.get(i)).copied() {
            byte = spot_shadow_efd0_zero_all(byte, ok);
        }
        spot_shadow_scene_model_vis_write(vis, slot, i as u32, byte);
    }
}

pub fn spot_shadow_tess_scene_dobj_indices(
    vis: &[u8],
    slot: u32,
    scene_dobj_count: u32,
    out: &mut [u32],
) -> usize {
    let mut n = 0usize;
    let cap = scene_dobj_count.min(SPOT_SHADOW_SCENE_DOBJ_VIS_STRIDE as u32);
    for i in 0..cap {
        if spot_shadow_scene_dobj_vis(vis, slot, i) && n < out.len() {
            out[n] = i;
            n += 1;
        }
    }
    n
}

pub fn spot_shadow_tess_scene_model_indices(
    vis: &[u8],
    slot: u32,
    scene_model_count: u32,
    out: &mut [u32],
) -> usize {
    let mut n = 0usize;
    let cap = scene_model_count.min(SPOT_SHADOW_SCENE_MODEL_VIS_STRIDE as u32);
    for i in 0..cap {
        if spot_shadow_scene_model_vis(vis, slot, i) && n < out.len() {
            out[n] = i;
            n += 1;
        }
    }
    n
}

pub fn spot_shadow_gpu_extent(render_target_id: u8) -> Option<(u32, u32)> {
    match render_target_id {
        GFX_SPOT_SHADOW_RT_SMALL => Some((
            GFX_SPOT_SHADOW_VIEWPORT_SMALL,
            GFX_SPOT_SHADOW_VIEWPORT_SMALL * 4,
        )),
        GFX_SPOT_SHADOW_RT_LARGE => Some((
            GFX_SPOT_SHADOW_VIEWPORT_LARGE,
            GFX_SPOT_SHADOW_VIEWPORT_LARGE * 2,
        )),
        _ => None,
    }
}

pub fn spot_shadow_gpu_targets(render_target_ids: &[u8]) -> impl Iterator<Item = u8> + '_ {
    let mut seen_small = false;
    let mut seen_large = false;
    spot_shadow_consume_order(render_target_ids).filter_map(move |i| {
        let id = render_target_ids[i];
        match id {
            GFX_SPOT_SHADOW_RT_SMALL if !seen_small => {
                seen_small = true;
                Some(id)
            }
            GFX_SPOT_SHADOW_RT_LARGE if !seen_large => {
                seen_large = true;
                Some(id)
            }
            _ => None,
        }
    })
}

pub const SPOT_SHADOW_COS_CLAMP_MIN: f32 = 0.001;

pub const SPOT_SHADOW_COS_CLAMP_MAX: f32 = f32::from_bits(0x3f7f_be77);

pub const SPOT_SHADOW_ZNEAR_ADD: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowViewAxis {
    pub forward: [f32; 3],
    pub up: [f32; 3],
    pub right: [f32; 3],
}

impl SpotShadowViewAxis {
    pub fn as_viewer_axis(self) -> [[f32; 3]; 3] {
        [self.forward, self.up, self.right]
    }
}

pub const SPOT_SHADOW_LOOKUP_AFTER_VIEWPARMS: usize = hud_iw4::GFX_VIEWPARMS_SIZE;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowViewParms {
    pub axis: SpotShadowViewAxis,
    pub origin: [f32; 3],
    pub tan_half_fov: f32,
    pub z_near: f32,
    pub z_far: f32,
    pub projection: [f32; 16],
    pub view: [f32; 16],
    pub view_projection: [f32; 16],
    pub inv_view_projection: [f32; 16],
}

#[must_use]
pub fn spot_shadow_near_bias(light_index: u32, world_plus_0x20: u32, dvar: f32) -> f32 {
    if light_index < world_plus_0x20 {
        0.0
    } else {
        dvar
    }
}

#[must_use]
pub fn spot_shadow_z_near(near_bias: f32) -> f32 {
    near_bias + SPOT_SHADOW_ZNEAR_ADD
}

#[must_use]
pub fn spot_shadow_tan_half_fov(cos: f32) -> f32 {
    let c = if cos < SPOT_SHADOW_COS_CLAMP_MIN {
        SPOT_SHADOW_COS_CLAMP_MIN
    } else if cos > SPOT_SHADOW_COS_CLAMP_MAX {
        SPOT_SHADOW_COS_CLAMP_MAX
    } else {
        cos
    };
    libm::sqrtf(1.0 - c * c) / c
}

#[must_use]
pub fn spot_shadow_view_axis(dir: [f32; 3]) -> SpotShadowViewAxis {
    let forward = [-dir[0], -dir[1], -dir[2]];
    let right = fx_iw4::fx_perpendicular_vector(forward);
    let up = [
        right[1] * forward[2] - right[2] * forward[1],
        right[2] * forward[0] - right[0] * forward[2],
        right[0] * forward[1] - right[1] * forward[0],
    ];
    SpotShadowViewAxis { forward, up, right }
}

#[must_use]
pub fn spot_shadow_view_parms(
    origin: [f32; 3],
    dir: [f32; 3],
    cos: f32,
    z_far: f32,
    near_bias: f32,
) -> Option<SpotShadowViewParms> {
    let axis = spot_shadow_view_axis(dir);
    let tan = spot_shadow_tan_half_fov(cos);
    let z_near = spot_shadow_z_near(near_bias);
    let projection = hud_iw4::r_setup_finite_projection_matrix(tan, tan, z_near, z_far)?;
    let view = hud_iw4::r_matrix_for_viewer(axis.as_viewer_axis());
    let (view_projection, inv_view_projection) =
        hud_iw4::r_compose_view_projection(&view, &projection, origin)?;
    Some(SpotShadowViewParms {
        axis,
        origin,
        tan_half_fov: tan,
        z_near,
        z_far,
        projection,
        view,
        view_projection,
        inv_view_projection,
    })
}

fn mat4_mul_row(m: &[f32; 16], v: [f32; 4]) -> [f32; 4] {
    [
        m[12] * v[3] + m[8] * v[2] + v[0] * m[0] + m[4] * v[1],
        m[13] * v[3] + m[9] * v[2] + m[5] * v[1] + m[1] * v[0],
        m[14] * v[3] + m[10] * v[2] + m[6] * v[1] + m[2] * v[0],
        m[15] * v[3] + m[11] * v[2] + m[7] * v[1] + m[3] * v[0],
    ]
}

#[must_use]
pub fn spot_shadow_overlay_atlas_y(plan: SpotShadowSlotPlan) -> (u32, u32) {
    (plan.viewport_size * plan.atlas_columns, plan.viewport_y)
}

#[must_use]
pub fn spot_shadow_overlay_matrix(
    view: &[f32; 16],
    origin: [f32; 3],
    atlas_y: u32,
    offset_y: u32,
) -> Option<[f32; 16]> {
    if atlas_y == 0 {
        return None;
    }
    let a = atlas_y as f32;
    let b = offset_y as f32;
    let x_scale = SPOT_SHADOW_HALF;
    let x_shift = SPOT_SHADOW_HALF;
    let y_scale = -SPOT_SHADOW_HALF / a;
    let y_shift = (b + SPOT_SHADOW_HALF) / a;
    let t = mat4_mul_row(view, [-origin[0], -origin[1], -origin[2], 1.0]);
    let mut rows = [[0.0f32; 4]; 4];
    for r in 0..3 {
        rows[r] = [
            view[r * 4],
            view[r * 4 + 1],
            view[r * 4 + 2],
            view[r * 4 + 3],
        ];
    }
    rows[3] = t;
    let mut out = [0.0f32; 16];
    for r in 0..4 {
        let row = rows[r];
        out[r * 4] = x_scale * row[0] + x_shift * row[3];
        out[r * 4 + 1] = y_scale * row[1] + y_shift * row[3];
        out[r * 4 + 2] = row[2];
        out[r * 4 + 3] = row[3];
    }
    Some(out)
}

#[must_use]
pub fn spot_shadow_lookup_matrix(
    parms: &SpotShadowViewParms,
    plan: SpotShadowSlotPlan,
) -> Option<[f32; 16]> {
    let (atlas_y, offset_y) = spot_shadow_overlay_atlas_y(plan);
    spot_shadow_overlay_matrix(&parms.view_projection, parms.origin, atlas_y, offset_y)
}

pub const SPOT_SHADOW_ENT_FLAG_SKIP: u32 = 0x0208_0000;

pub const SPOT_SHADOW_ENTNUM_MASK: u32 = 0xfff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotShadowEntAdmit {
    SkipNoBounds,

    SkipCulled,

    Admit,
}

pub const SPOT_SHADOW_SCENE_DOBJ_VIS_STRIDE: usize = 0x200;

pub const SPOT_SHADOW_SCENE_MODEL_VIS_STRIDE: usize = 0x400;

pub const SPOT_SHADOW_CFG_INDEX_ENTS: u32 = 0x800;

pub const SPOT_SHADOW_ENT_MARK_LEN: usize = 0x1000;

pub const SPOT_SHADOW_ENT_SKIN_WAIT: i32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotShadowCasterKind {
    SceneEnt,
    Smodel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotShadowAddCaster {
    SkipFlag,
    SkipLocalClient,
    SkipUnlit,

    EnqueueWorker5,

    MarkSmodel,
}

#[must_use]
pub fn spot_shadow_entnum(info: u32) -> u32 {
    (info >> 7) & SPOT_SHADOW_ENTNUM_MASK
}

#[must_use]
pub fn spot_shadow_ent_flags_allow(info: u32) -> bool {
    info & SPOT_SHADOW_ENT_FLAG_SKIP == 0
}

#[must_use]
pub fn spot_shadow_ent_admit(
    bounds_ok: bool,
    light_origin: [f32; 3],
    light_dir: [f32; 3],
    light_cos: f32,
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> SpotShadowEntAdmit {
    if !bounds_ok {
        return SpotShadowEntAdmit::SkipNoBounds;
    }
    if crate::cull::cull_box_from_cone(light_origin, light_dir, light_cos, box_mid, box_half) {
        SpotShadowEntAdmit::SkipCulled
    } else {
        SpotShadowEntAdmit::Admit
    }
}

#[must_use]
pub fn spot_shadow_take_slot(shadowable_count: &mut u32) -> Option<u32> {
    let i = *shadowable_count;
    if i >= crate::spot_shadow_choose::SPOT_SHADOW_SM_LIGHT_CAP as u32 {
        return None;
    }
    *shadowable_count = i + 1;
    Some(i)
}

#[must_use]
pub fn spot_shadow_emit_walks_casters(light_index: u32, world_plus_0x20: u32) -> bool {
    light_index < world_plus_0x20
}

#[must_use]
pub fn spot_shadow_primary_vis_bit_index(
    view: u32,
    entnum: u32,
    light_index: u32,
    sun_primary: u32,
    primary_count: u32,
    cfg_index_ents: u32,
) -> Option<u32> {
    let sun1 = sun_primary.wrapping_add(1) as i32;
    let span = (primary_count as i32).wrapping_sub(sun1);
    let mixed = (cfg_index_ents as i32)
        .wrapping_mul(view as i32)
        .wrapping_add(entnum as i32);
    let i = mixed
        .wrapping_mul(span)
        .wrapping_sub(sun1)
        .wrapping_add(light_index as i32);
    if i < 0 { None } else { Some(i as u32) }
}

#[must_use]
pub fn spot_shadow_primary_vis_bit(words: &[u32], bit_index: u32) -> bool {
    let w = (bit_index >> 5) as usize;
    w < words.len() && (words[w] & (1u32 << (bit_index & 31))) != 0
}

pub fn spot_shadow_primary_vis_write(words: &mut [u32], bit_index: u32, lit: bool) {
    let w = (bit_index >> 5) as usize;
    let Some(slot) = words.get_mut(w) else {
        return;
    };
    let mask = 1u32 << (bit_index & 31);
    if lit {
        *slot |= mask;
    } else {
        *slot &= !mask;
    }
}

pub fn spot_shadow_link_primary_vis(
    words: &mut [u32],
    view: u32,
    entnum: u32,
    origin: [f32; 3],
    extra: f32,
    lights: &[crate::cull::ComPrimaryLightCull],
    sun_primary: u32,
) {
    let primary_count = lights.len() as u32;
    let mut light_index = sun_primary.saturating_add(1);
    while light_index < primary_count {
        let Some(bit) = spot_shadow_primary_vis_bit_index(
            view,
            entnum,
            light_index,
            sun_primary,
            primary_count,
            SPOT_SHADOW_CFG_INDEX_ENTS,
        ) else {
            light_index += 1;
            continue;
        };
        let light = lights[light_index as usize];
        let dx = origin[0] - light.origin[0];
        let dy = origin[1] - light.origin[1];
        let dz = origin[2] - light.origin[2];
        let d2 = dx * dx + dy * dy + dz * dz;
        let r = light.radius + extra;
        let inside = d2 < r * r;
        let lit = if !inside {
            false
        } else if light.light_type == crate::scene_light::GFX_LIGHT_TYPE_SPOT
            && light.cos_half_fov_expanded > 0.0
            && crate::cull::cull_point_from_cone_expanded(
                light.origin,
                light.direction,
                light.cos_half_fov_expanded,
                origin,
                extra,
            )
        {
            false
        } else {
            true
        };
        spot_shadow_primary_vis_write(words, bit, lit);
        light_index += 1;
    }
}

#[must_use]
pub fn spot_shadow_dyn_brush_vis_bit_index(
    brush_id: u32,
    light_index: u32,
    sun_primary: u32,
    primary_count: u32,
) -> Option<u32> {
    let sun1 = sun_primary.wrapping_add(1) as i32;
    let span = (primary_count as i32).wrapping_sub(sun1);
    let i = (brush_id as i32)
        .wrapping_mul(span)
        .wrapping_sub(sun1)
        .wrapping_add(light_index as i32);
    if i < 0 { None } else { Some(i as u32) }
}

pub fn spot_shadow_link_dyn_brush_vis(
    words: &mut [u32],
    brush_id: u32,
    origin: [f32; 3],
    extra: f32,
    lights: &[crate::cull::ComPrimaryLightCull],
    sun_primary: u32,
) {
    let primary_count = lights.len() as u32;
    let mut light_index = sun_primary.saturating_add(1);
    while light_index < primary_count {
        let Some(bit) =
            spot_shadow_dyn_brush_vis_bit_index(brush_id, light_index, sun_primary, primary_count)
        else {
            light_index += 1;
            continue;
        };
        let light = lights[light_index as usize];
        let dx = origin[0] - light.origin[0];
        let dy = origin[1] - light.origin[1];
        let dz = origin[2] - light.origin[2];
        let d2 = dx * dx + dy * dy + dz * dz;
        let r = light.radius + extra;
        let inside = d2 < r * r;
        let lit = if !inside {
            false
        } else if light.light_type == crate::scene_light::GFX_LIGHT_TYPE_SPOT
            && light.cos_half_fov_expanded > 0.0
            && crate::cull::cull_point_from_cone_expanded(
                light.origin,
                light.direction,
                light.cos_half_fov_expanded,
                origin,
                extra,
            )
        {
            false
        } else {
            true
        };
        spot_shadow_primary_vis_write(words, bit, lit);
        light_index += 1;
    }
}

#[must_use]
pub fn spot_shadow_dyn_brush_tess_admits(walks_primary: bool, vis_bit: bool) -> bool {
    walks_primary && vis_bit
}

pub const SPOT_SHADOW_LINK_ENTITY_PAD: f32 = 16.0;

pub const SPOT_SHADOW_PLAYER_LINK_CG_FLAGS: u32 = 0x1800;

pub const SPOT_SHADOW_ENTITY_ORIGIN_TRACK_ENTS: u32 = 0x800;

#[must_use]
pub fn spot_shadow_entity_origin_track_index(local_client: u32, entnum: u32) -> Option<usize> {
    if entnum >= SPOT_SHADOW_ENTITY_ORIGIN_TRACK_ENTS {
        return None;
    }
    local_client
        .checked_mul(SPOT_SHADOW_ENTITY_ORIGIN_TRACK_ENTS)?
        .checked_add(entnum)
        .map(|i| i as usize)
}

pub const SPOT_SHADOW_ENTITY_RELINK_THRESH_SQ: f32 = 256.0;

#[must_use]
pub fn spot_shadow_entity_should_relink(last: [f32; 3], now: [f32; 3], thresh_sq: f32) -> bool {
    let dx = now[0] - last[0];
    let dy = now[1] - last[1];
    let dz = now[2] - last[2];
    thresh_sq < dx * dx + dy * dy + dz * dz
}

#[must_use]
pub fn spot_shadow_link_extra_player(dobj_radius: f32) -> f32 {
    dobj_radius
}

#[must_use]
pub fn spot_shadow_link_extra_entity(dobj_radius: f32) -> f32 {
    dobj_radius + SPOT_SHADOW_LINK_ENTITY_PAD
}

#[must_use]
pub fn spot_shadow_player_vis_link_allows(
    cg_flags: u32,
    entnum: i32,
    local_client_entnum: i32,
) -> bool {
    (cg_flags & SPOT_SHADOW_PLAYER_LINK_CG_FLAGS) != 0 && entnum == local_client_entnum
}

#[must_use]
pub fn spot_shadow_primary_vis_word_count(
    view_count: u32,
    ent_count: u32,
    sun_primary: u32,
    primary_count: u32,
) -> usize {
    if view_count == 0 || ent_count == 0 || primary_count == 0 {
        return 0;
    }
    let Some(bit) = spot_shadow_primary_vis_bit_index(
        view_count - 1,
        ent_count - 1,
        primary_count.saturating_sub(1),
        sun_primary,
        primary_count,
        SPOT_SHADOW_CFG_INDEX_ENTS,
    ) else {
        return 0;
    };
    (bit as usize / 32) + 1
}

pub fn spot_shadow_link_player(
    words: &mut [u32],
    view: u32,
    entnum: u32,
    origin: [f32; 3],
    dobj_radius: f32,
    lights: &[crate::cull::ComPrimaryLightCull],
    sun_primary: u32,
) {
    spot_shadow_link_primary_vis(
        words,
        view,
        entnum,
        origin,
        spot_shadow_link_extra_player(dobj_radius),
        lights,
        sun_primary,
    );
}

pub fn spot_shadow_link_entity(
    words: &mut [u32],
    view: u32,
    entnum: u32,
    origin: [f32; 3],
    dobj_radius: f32,
    lights: &[crate::cull::ComPrimaryLightCull],
    sun_primary: u32,
) {
    spot_shadow_link_primary_vis(
        words,
        view,
        entnum,
        origin,
        spot_shadow_link_extra_entity(dobj_radius),
        lights,
        sun_primary,
    );
}

pub fn spot_shadow_mark_ent(row: &mut [u8], entnum: u32) {
    if let Some(slot) = row.get_mut(entnum as usize) {
        *slot = 1;
    }
}

#[must_use]
pub fn spot_shadow_ent_marked(row: &[u8], entnum: u32) -> bool {
    row.get(entnum as usize).copied().unwrap_or(0) != 0
}

pub fn spot_shadow_ent_cmd_mark(row: &mut [u8], admit: SpotShadowEntAdmit, entnum: u32) {
    if admit == SpotShadowEntAdmit::Admit {
        spot_shadow_mark_ent(row, entnum);
    }
}

#[must_use]
pub fn spot_shadow_ent_cmd_pose_acquire(admit: SpotShadowEntAdmit) -> bool {
    admit == SpotShadowEntAdmit::Admit
}

#[must_use]
pub fn spot_shadow_xmodel_is_caster(row: &[u8], scene_entnum: Option<u32>) -> bool {
    match scene_entnum {
        Some(entnum) => spot_shadow_ent_marked(row, entnum),
        None => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowSceneSlot {
    pub info: u32,
    pub box_mid: Option<[f32; 3]>,
    pub box_half: Option<[f32; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotShadowOccupancyError {
    VisUnread,
}

pub fn spot_shadow_fill_occupancy(
    slots: &[SpotShadowSceneSlot],
    vis: Option<&[u32]>,
    view: u32,
    light_index: u32,
    sun_primary: u32,
    primary_count: u32,
    out: &mut [SpotShadowCasterIn],
) -> Result<usize, SpotShadowOccupancyError> {
    let Some(words) = vis else {
        return Err(SpotShadowOccupancyError::VisUnread);
    };
    let n = slots.len().min(out.len());
    for (i, slot) in slots.iter().take(n).enumerate() {
        let entnum = spot_shadow_entnum(slot.info);
        let vis_lit = spot_shadow_primary_vis_bit_index(
            view,
            entnum,
            light_index,
            sun_primary,
            primary_count,
            SPOT_SHADOW_CFG_INDEX_ENTS,
        )
        .is_some_and(|bit| spot_shadow_primary_vis_bit(words, bit));
        let bounds_ok = slot.box_mid.is_some() && slot.box_half.is_some();
        out[i] = SpotShadowCasterIn {
            info: slot.info,
            vis_lit,
            bounds_ok,
            box_mid: slot.box_mid.unwrap_or([0.0; 3]),
            box_half: slot.box_half.unwrap_or([0.0; 3]),
        };
    }
    Ok(n)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowCasterIn {
    pub info: u32,
    pub vis_lit: bool,
    pub bounds_ok: bool,
    pub box_mid: [f32; 3],
    pub box_half: [f32; 3],
}

pub fn spot_shadow_walk_casters(
    walks_casters: bool,
    light: &crate::spot_shadow_choose::SpotShadowableLight,
    ents: &[SpotShadowCasterIn],
    smodels: &[SpotShadowCasterIn],
    local_client_entnum: u32,
    mark_row: &mut [u8],
) {
    if !walks_casters {
        return;
    }
    for e in ents {
        if spot_shadow_add_caster(
            SpotShadowCasterKind::SceneEnt,
            e.info,
            local_client_entnum,
            e.vis_lit,
        ) != SpotShadowAddCaster::EnqueueWorker5
        {
            continue;
        }
        let admit = spot_shadow_ent_admit(
            e.bounds_ok,
            light.origin,
            light.dir,
            light.cos_half_fov_expanded,
            e.box_mid,
            e.box_half,
        );
        spot_shadow_ent_cmd_mark(mark_row, admit, spot_shadow_entnum(e.info));
    }
    for s in smodels {
        if spot_shadow_add_caster(
            SpotShadowCasterKind::Smodel,
            s.info,
            local_client_entnum,
            s.vis_lit,
        ) != SpotShadowAddCaster::MarkSmodel
        {
            continue;
        }
        spot_shadow_mark_ent(mark_row, spot_shadow_entnum(s.info));
    }
}

#[must_use]
pub fn spot_shadow_add_caster(
    kind: SpotShadowCasterKind,
    info: u32,
    local_client_entnum: u32,
    vis_lit: bool,
) -> SpotShadowAddCaster {
    if !spot_shadow_ent_flags_allow(info) {
        return SpotShadowAddCaster::SkipFlag;
    }
    let entnum = spot_shadow_entnum(info);
    if entnum == local_client_entnum {
        return SpotShadowAddCaster::SkipLocalClient;
    }
    if !vis_lit {
        return SpotShadowAddCaster::SkipUnlit;
    }
    match kind {
        SpotShadowCasterKind::SceneEnt => SpotShadowAddCaster::EnqueueWorker5,
        SpotShadowCasterKind::Smodel => SpotShadowAddCaster::MarkSmodel,
    }
}
