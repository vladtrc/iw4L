pub const FX_GLASS_INIT_PIECE_STATE: usize = 0x34;

pub const FX_GLASS_PIECE_PLACE: usize = 0x20;

pub const FX_GLASS_PIECE_STATE: usize = 0x20;

pub const FX_GLASS_PIECE_DYNAMICS: usize = 0x24;

pub const FX_GLASS_FREE_SENTINEL: u32 = 0xffff;

pub const FX_GLASS_FALL_TIME_NEVER: i32 = 0x7fff_ffff;

pub const FX_GLASS_LINK_ORG_FREE: f32 = 262144.0;

pub const FX_GLASS_GEOMETRY_DATA: usize = 4;
pub const FX_GLASS_DEF: usize = 0x24;

pub const FX_GLASS_INIT_ORIGIN: usize = 0x10;

pub const FX_GLASS_INIT_PLACE_BYTES: usize = FX_GLASS_PIECE_PLACE;
pub const FX_GLASS_INIT_TEXCOORD: usize = 0x20;
pub const FX_GLASS_INIT_SUPPORT_MASK: usize = 0x28;
pub const FX_GLASS_INIT_AREA_X2: usize = 0x2c;
pub const FX_GLASS_INIT_DEF_INDEX: usize = 0x30;
pub const FX_GLASS_INIT_VERT_COUNT: usize = 0x31;
pub const FX_GLASS_INIT_FAN_DATA_COUNT: usize = 0x32;

pub const FX_GLASS_STATE_INIT_INDEX: usize = 0x0c;
pub const FX_GLASS_STATE_GEO_DATA_START: usize = 0x0e;
pub const FX_GLASS_STATE_DEF_INDEX: usize = 0x10;
pub const FX_GLASS_STATE_VERT_COUNT: usize = 0x16;
pub const FX_GLASS_STATE_HOLE_DATA_COUNT: usize = 0x17;
pub const FX_GLASS_STATE_CRACK_DATA_COUNT: usize = 0x18;
pub const FX_GLASS_STATE_FAN_DATA_COUNT: usize = 0x19;
pub const FX_GLASS_STATE_FLAGS: usize = 0x1a;
pub const FX_GLASS_STATE_AREA_X2: usize = 0x1c;

pub const FX_GLASS_STATE_SUPPORT_MASK: usize = 8;

pub const FX_GLASS_DYN_FALL_TIME: usize = 0;

pub const FX_GLASS_DYN_VEL: usize = 0x0c;

pub const FX_GLASS_DYN_PHYS_OBJ: usize = 4;

pub const FX_GLASS_DYN_AVEL: usize = 0x18;

pub const FX_GLASS_FALL_GRAVITY: f32 = 800.0;

pub const FX_GLASS_TRACE_INTERVAL_MSEC: u32 = 100;

pub const FX_GLASS_MSEC_TO_SEC: f32 = 0.001;

pub const FX_GLASS_AVEL_HALF: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxGlassResetPiece {
    pub place: [u8; FX_GLASS_PIECE_PLACE],
    pub state: [u8; FX_GLASS_PIECE_STATE],
    pub next_geo_start: u16,
    pub half_thickness: Option<f32>,
}

pub fn fx_glass_reset_copy_piece(
    init: &[u8; FX_GLASS_INIT_PIECE_STATE],
    piece_index: u16,
    geo_start: u16,
    defs: &[[u8; FX_GLASS_DEF]],
) -> FxGlassResetPiece {
    let mut place = [0u8; FX_GLASS_PIECE_PLACE];
    place.copy_from_slice(&init[..FX_GLASS_INIT_PLACE_BYTES]);

    let mut state = [0u8; FX_GLASS_PIECE_STATE];
    state[..8].copy_from_slice(&init[FX_GLASS_INIT_TEXCOORD..FX_GLASS_INIT_TEXCOORD + 8]);
    state[8..12].copy_from_slice(&init[FX_GLASS_INIT_SUPPORT_MASK..FX_GLASS_INIT_SUPPORT_MASK + 4]);
    write_u16(&mut state, FX_GLASS_STATE_INIT_INDEX, piece_index);
    write_u16(&mut state, FX_GLASS_STATE_GEO_DATA_START, geo_start);
    let def_index = init[FX_GLASS_INIT_DEF_INDEX];
    state[FX_GLASS_STATE_DEF_INDEX] = def_index;
    let vert_count = init[FX_GLASS_INIT_VERT_COUNT];
    let fan_count = init[FX_GLASS_INIT_FAN_DATA_COUNT];
    state[FX_GLASS_STATE_VERT_COUNT] = vert_count;
    state[FX_GLASS_STATE_HOLE_DATA_COUNT] = 0;
    state[FX_GLASS_STATE_CRACK_DATA_COUNT] = 0;
    state[FX_GLASS_STATE_FAN_DATA_COUNT] = fan_count;
    write_u16(&mut state, FX_GLASS_STATE_FLAGS, 0);
    state[FX_GLASS_STATE_AREA_X2..FX_GLASS_STATE_AREA_X2 + 4]
        .copy_from_slice(&init[FX_GLASS_INIT_AREA_X2..FX_GLASS_INIT_AREA_X2 + 4]);

    let next_geo_start = geo_start
        .wrapping_add(u16::from(vert_count))
        .wrapping_add(u16::from(fan_count));
    let half_thickness = defs.get(usize::from(def_index)).map(|d| read_f32(d, 0));

    FxGlassResetPiece {
        place,
        state,
        next_geo_start,
        half_thickness,
    }
}

pub fn fx_glass_reset_copy_geo(dst: &mut [u8], src: &[u8]) {
    let n = dst.len().min(src.len());
    dst[..n].copy_from_slice(&src[..n]);
}

pub fn fx_glass_init_origin(init: &[u8; FX_GLASS_INIT_PIECE_STATE]) -> [f32; 3] {
    [
        read_f32(init, FX_GLASS_INIT_ORIGIN),
        read_f32(init, FX_GLASS_INIT_ORIGIN + 4),
        read_f32(init, FX_GLASS_INIT_ORIGIN + 8),
    ]
}

pub fn fx_glass_place_origin(place: &[u8; FX_GLASS_PIECE_PLACE]) -> [f32; 3] {
    [
        read_f32(place, FX_GLASS_INIT_ORIGIN),
        read_f32(place, FX_GLASS_INIT_ORIGIN + 4),
        read_f32(place, FX_GLASS_INIT_ORIGIN + 8),
    ]
}

pub fn fx_glass_state_geo_start(state: &[u8; FX_GLASS_PIECE_STATE]) -> u16 {
    u16::from_le_bytes([
        state[FX_GLASS_STATE_GEO_DATA_START],
        state[FX_GLASS_STATE_GEO_DATA_START + 1],
    ])
}

pub fn fx_glass_state_vert_count(state: &[u8; FX_GLASS_PIECE_STATE]) -> u8 {
    state[FX_GLASS_STATE_VERT_COUNT]
}

pub fn fx_glass_state_fan_count(state: &[u8; FX_GLASS_PIECE_STATE]) -> u8 {
    state[FX_GLASS_STATE_FAN_DATA_COUNT]
}

pub fn fx_glass_state_def_index(state: &[u8; FX_GLASS_PIECE_STATE]) -> u8 {
    state[FX_GLASS_STATE_DEF_INDEX]
}

pub fn fx_glass_state_flags(state: &[u8; FX_GLASS_PIECE_STATE]) -> u16 {
    u16::from_le_bytes([state[FX_GLASS_STATE_FLAGS], state[FX_GLASS_STATE_FLAGS + 1]])
}

pub fn fx_glass_state_set_flags(state: &mut [u8; FX_GLASS_PIECE_STATE], flags: u16) {
    write_u16(state, FX_GLASS_STATE_FLAGS, flags);
}

pub fn fx_glass_state_support_mask(state: &[u8; FX_GLASS_PIECE_STATE]) -> u32 {
    u32::from_le_bytes([
        state[FX_GLASS_STATE_SUPPORT_MASK],
        state[FX_GLASS_STATE_SUPPORT_MASK + 1],
        state[FX_GLASS_STATE_SUPPORT_MASK + 2],
        state[FX_GLASS_STATE_SUPPORT_MASK + 3],
    ])
}

pub fn fx_glass_state_set_support_mask(state: &mut [u8; FX_GLASS_PIECE_STATE], mask: u32) {
    let b = mask.to_le_bytes();
    state[FX_GLASS_STATE_SUPPORT_MASK..FX_GLASS_STATE_SUPPORT_MASK + 4].copy_from_slice(&b);
}

pub fn fx_glass_state_area_x2(state: &[u8; FX_GLASS_PIECE_STATE]) -> f32 {
    f32::from_le_bytes([
        state[FX_GLASS_STATE_AREA_X2],
        state[FX_GLASS_STATE_AREA_X2 + 1],
        state[FX_GLASS_STATE_AREA_X2 + 2],
        state[FX_GLASS_STATE_AREA_X2 + 3],
    ])
}

pub fn fx_glass_dynamics_software_launch(
    row: &mut [u8; FX_GLASS_PIECE_DYNAMICS],
    fall_time: i32,
    vel: [f32; 3],
    avel: [f32; 3],
) {
    *row = [0u8; FX_GLASS_PIECE_DYNAMICS];
    row[FX_GLASS_DYN_FALL_TIME..FX_GLASS_DYN_FALL_TIME + 4]
        .copy_from_slice(&fall_time.to_le_bytes());
    write_f32_3(row, FX_GLASS_DYN_VEL, vel);
    write_f32_3(row, FX_GLASS_DYN_AVEL, avel);
}

pub fn fx_glass_dynamics_fall_time(row: &[u8; FX_GLASS_PIECE_DYNAMICS]) -> i32 {
    let mut b = [0u8; 4];
    b.copy_from_slice(&row[FX_GLASS_DYN_FALL_TIME..FX_GLASS_DYN_FALL_TIME + 4]);
    i32::from_le_bytes(b)
}

pub fn fx_glass_dynamics_phys_obj(row: &[u8; FX_GLASS_PIECE_DYNAMICS]) -> i32 {
    let mut b = [0u8; 4];
    b.copy_from_slice(&row[FX_GLASS_DYN_PHYS_OBJ..FX_GLASS_DYN_PHYS_OBJ + 4]);
    i32::from_le_bytes(b)
}

pub fn fx_glass_dynamics_vel(row: &[u8; FX_GLASS_PIECE_DYNAMICS]) -> [f32; 3] {
    [
        read_f32(row, FX_GLASS_DYN_VEL),
        read_f32(row, FX_GLASS_DYN_VEL + 4),
        read_f32(row, FX_GLASS_DYN_VEL + 8),
    ]
}

pub fn fx_glass_dynamics_avel(row: &[u8; FX_GLASS_PIECE_DYNAMICS]) -> [f32; 3] {
    [
        read_f32(row, FX_GLASS_DYN_AVEL),
        read_f32(row, FX_GLASS_DYN_AVEL + 4),
        read_f32(row, FX_GLASS_DYN_AVEL + 8),
    ]
}

pub fn fx_glass_ballistic_origin(
    origin: [f32; 3],
    vel: [f32; 3],
    fall_msec: i32,
    start_msec: i32,
    end_msec: i32,
    gravity: f32,
) -> [f32; 3] {
    let dt = (end_msec.wrapping_sub(start_msec) as f32) * FX_GLASS_MSEC_TO_SEC;
    let mid = end_msec.wrapping_add(start_msec) / 2;
    let since = (mid.wrapping_sub(fall_msec) as f32) * FX_GLASS_MSEC_TO_SEC;
    [
        origin[0] + dt * vel[0],
        origin[1] + dt * vel[1],
        origin[2] + dt * (vel[2] - gravity * since),
    ]
}

pub fn fx_glass_software_rotate_quat(quat: [f32; 4], avel: [f32; 3], dt_msec: i32) -> [f32; 4] {
    let len_sq = avel[0] * avel[0] + avel[1] * avel[1] + avel[2] * avel[2];
    if len_sq <= 0.0 {
        return quat;
    }
    let len = libm::sqrtf(len_sq);
    let half = len * (dt_msec as f32) * FX_GLASS_MSEC_TO_SEC * FX_GLASS_AVEL_HALF;
    let s = libm::sinf(half);
    let c = libm::cosf(half);
    let inv = s / len;
    crate::fx_quat_mul(quat, [avel[0] * inv, avel[1] * inv, avel[2] * inv, c])
}

pub fn fx_glass_trace_phase(piece: u32, interval: u32) -> u32 {
    if interval == 0 {
        return 0;
    }
    piece.wrapping_mul(0x11) % interval
}

pub fn fx_glass_last_trace_tick(t: i32, phase: u32, interval: u32) -> i32 {
    if interval == 0 {
        return t;
    }
    let interval = interval as i32;
    let phase = phase as i32;
    ((interval - phase).wrapping_add(t) / interval - 1) * interval + phase
}

pub fn fx_glass_software_trace_due(
    piece: u32,
    start_msec: i32,
    current_msec: i32,
    interval: u32,
) -> bool {
    let phase = fx_glass_trace_phase(piece, interval);
    fx_glass_last_trace_tick(current_msec, phase, interval)
        > fx_glass_last_trace_tick(start_msec, phase, interval)
}

pub fn fx_glass_place_set_origin(place: &mut [u8; FX_GLASS_PIECE_PLACE], origin: [f32; 3]) {
    write_f32_3(place, FX_GLASS_INIT_ORIGIN, origin);
}

pub fn fx_glass_place_set_quat(place: &mut [u8; FX_GLASS_PIECE_PLACE], quat: [f32; 4]) {
    let x = quat[0].to_le_bytes();
    let y = quat[1].to_le_bytes();
    let z = quat[2].to_le_bytes();
    let w = quat[3].to_le_bytes();
    place[0..4].copy_from_slice(&x);
    place[4..8].copy_from_slice(&y);
    place[8..12].copy_from_slice(&z);
    place[12..16].copy_from_slice(&w);
}

fn write_f32_3(bytes: &mut [u8], off: usize, v: [f32; 3]) {
    for i in 0..3 {
        let b = v[i].to_le_bytes();
        bytes[off + i * 4..off + i * 4 + 4].copy_from_slice(&b);
    }
}

pub fn fx_glass_pack_geo_vert(ix: i16, iy: i16) -> [u8; FX_GLASS_GEOMETRY_DATA] {
    let x = ix.to_le_bytes();
    let y = iy.to_le_bytes();
    [x[0], x[1], y[0], y[1]]
}

pub fn fx_glass_state_init_index(state: &[u8; FX_GLASS_PIECE_STATE]) -> u16 {
    u16::from_le_bytes([
        state[FX_GLASS_STATE_INIT_INDEX],
        state[FX_GLASS_STATE_INIT_INDEX + 1],
    ])
}

pub fn fx_glass_place_quat(place: &[u8; FX_GLASS_PIECE_PLACE]) -> [f32; 4] {
    [
        read_f32(place, 0),
        read_f32(place, 4),
        read_f32(place, 8),
        read_f32(place, 12),
    ]
}

pub fn fx_glass_def_tex_vecs(def: &[u8; FX_GLASS_DEF]) -> [[f32; 2]; 2] {
    [
        [read_f32(def, 4), read_f32(def, 8)],
        [read_f32(def, 12), read_f32(def, 16)],
    ]
}

pub fn fx_glass_def_color_rgba(def: &[u8; FX_GLASS_DEF]) -> [u8; 4] {
    [def[20], def[21], def[22], def[23]]
}

pub const FX_GLASS_VERT_SCALE: f32 = 0.03125;

pub fn fx_unit_quat_to_axis(q: [f32; 4]) -> [[f32; 3]; 3] {
    let [x, y, z, w] = q;
    let x2 = x + x;
    let y2 = y + y;
    let z2 = z + z;
    let xx = x2 * x;
    let yy = y2 * y;
    let zz = z2 * z;
    let xy = x2 * y;
    let xz = x2 * z;
    let xw = x2 * w;
    let yz = y2 * z;
    let yw = y2 * w;
    let zw = z2 * w;
    [
        [1.0 - (yy + zz), xy + zw, xz - yw],
        [xy - zw, 1.0 - (xx + zz), xw + yz],
        [xz + yw, yz - xw, 1.0 - (yy + xx)],
    ]
}

pub fn fx_glass_geo_vert(word: &[u8; FX_GLASS_GEOMETRY_DATA]) -> [i16; 2] {
    [
        i16::from_le_bytes([word[0], word[1]]),
        i16::from_le_bytes([word[2], word[3]]),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxGlassIntactVert {
    pub xyz: [f32; 3],
    pub uv: [f32; 2],
}

pub fn fx_glass_intact_verts(
    place: &[u8; FX_GLASS_PIECE_PLACE],
    state: &[u8; FX_GLASS_PIECE_STATE],
    geo: &[[u8; FX_GLASS_GEOMETRY_DATA]],
    def: &[u8; FX_GLASS_DEF],
    out: &mut [FxGlassIntactVert],
) -> Option<usize> {
    let vert_n = usize::from(fx_glass_state_vert_count(state));
    if vert_n < 3 || out.len() < vert_n {
        return None;
    }
    let start = usize::from(fx_glass_state_geo_start(state));
    let end = start.checked_add(vert_n)?;
    let slice = geo.get(start..end)?;
    let origin = fx_glass_place_origin(place);
    let axis = fx_unit_quat_to_axis(fx_glass_place_quat(place));
    let ax = [
        axis[0][0] * FX_GLASS_VERT_SCALE,
        axis[0][1] * FX_GLASS_VERT_SCALE,
        axis[0][2] * FX_GLASS_VERT_SCALE,
    ];
    let ay = [
        axis[1][0] * FX_GLASS_VERT_SCALE,
        axis[1][1] * FX_GLASS_VERT_SCALE,
        axis[1][2] * FX_GLASS_VERT_SCALE,
    ];
    let tex = fx_glass_def_tex_vecs(def);
    let u0 = read_f32(state, 0);
    let v0 = read_f32(state, 4);
    for (dst, word) in out.iter_mut().zip(slice.iter()) {
        let [ix, iy] = fx_glass_geo_vert(word);
        let x = ix as f32;
        let y = iy as f32;
        dst.xyz = [
            origin[0] + x * ax[0] + y * ay[0],
            origin[1] + x * ax[1] + y * ay[1],
            origin[2] + x * ax[2] + y * ay[2],
        ];
        dst.uv = [
            tex[0][0] * x + tex[0][1] * y + u0,
            tex[1][0] * x + tex[1][1] * y + v0,
        ];
    }
    Some(vert_n)
}

pub fn fx_glass_intact_fan_indices(vert_n: u8, out: &mut [u16]) -> Option<usize> {
    let n = u16::from(vert_n);
    if n < 3 {
        return None;
    }
    let want = usize::from(n.saturating_sub(2)) * 3;
    if out.len() < want {
        return None;
    }
    let mut w = 0usize;
    let mut i = 1u16;
    while i + 1 < n {
        out[w] = 0;
        out[w + 1] = i;
        out[w + 2] = i + 1;
        w += 3;
        i += 1;
    }
    Some(want)
}

fn read_f32(bytes: &[u8], off: usize) -> f32 {
    f32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

fn write_u16(bytes: &mut [u8], off: usize, v: u16) {
    let b = v.to_le_bytes();
    bytes[off] = b[0];
    bytes[off + 1] = b[1];
}

pub fn fx_glass_in_use_word(piece: u32) -> usize {
    (piece >> 5) as usize
}

pub fn fx_glass_in_use_mask(piece: u32) -> u32 {
    0x8000_0000 >> (piece & 31)
}

pub fn fx_glass_set_in_use(words: &mut [u32], piece: u32) {
    let i = fx_glass_in_use_word(piece);
    if let Some(word) = words.get_mut(i) {
        *word |= fx_glass_in_use_mask(piece);
    }
}

pub fn fx_glass_clear_in_use(words: &mut [u32], piece: u32) {
    let i = fx_glass_in_use_word(piece);
    if let Some(word) = words.get_mut(i) {
        *word &= !fx_glass_in_use_mask(piece);
    }
}

pub fn fx_glass_is_in_use(words: &[u32], piece: u32) -> bool {
    words
        .get(fx_glass_in_use_word(piece))
        .is_some_and(|word| word & fx_glass_in_use_mask(piece) != 0)
}

pub fn fx_glass_place_next_free(place: &[u8; FX_GLASS_PIECE_PLACE]) -> u32 {
    u32::from_le_bytes([place[0], place[1], place[2], place[3]])
}

pub fn fx_glass_place_set_next_free(place: &mut [u8; FX_GLASS_PIECE_PLACE], next: u32) {
    let b = next.to_le_bytes();
    place[0] = b[0];
    place[1] = b[1];
    place[2] = b[2];
    place[3] = b[3];
}

pub fn fx_glass_dynamics_init_row(row: &mut [u8; FX_GLASS_PIECE_DYNAMICS]) {
    *row = [0u8; FX_GLASS_PIECE_DYNAMICS];
    row[..4].copy_from_slice(&FX_GLASS_FALL_TIME_NEVER.to_le_bytes());
}

pub fn fx_glass_reset_free_list(
    places: &mut [[u8; FX_GLASS_PIECE_PLACE]],
    link_org: &mut [[f32; 3]],
    init_piece_count: u32,
    piece_limit: u32,
) -> u32 {
    if piece_limit == 0 || init_piece_count >= piece_limit {
        return FX_GLASS_FREE_SENTINEL;
    }
    let mut i = init_piece_count;
    if i != piece_limit - 1 {
        loop {
            if let Some(org) = link_org.get_mut(i as usize) {
                *org = [
                    FX_GLASS_LINK_ORG_FREE,
                    FX_GLASS_LINK_ORG_FREE,
                    FX_GLASS_LINK_ORG_FREE,
                ];
            }
            i += 1;
            if let Some(place) = places.get_mut((i - 1) as usize) {
                fx_glass_place_set_next_free(place, i);
            }
            if i == piece_limit - 1 {
                break;
            }
        }
    }
    if let Some(place) = places.get_mut(i as usize) {
        fx_glass_place_set_next_free(place, FX_GLASS_FREE_SENTINEL);
    }
    init_piece_count
}

pub fn fx_glass_alloc_piece(
    places: &mut [[u8; FX_GLASS_PIECE_PLACE]],
    states: &mut [[u8; FX_GLASS_PIECE_STATE]],
    is_in_use: &mut [u32],
    first_free: &mut u32,
    active: &mut u32,
    geo_count: &mut u32,
    geo_limit: u32,
    vert: u8,
    hole: u8,
    crack: u8,
    fan: u8,
) -> u32 {
    let piece = *first_free;
    if piece == FX_GLASS_FREE_SENTINEL {
        return FX_GLASS_FREE_SENTINEL;
    }
    let need = *geo_count + u32::from(vert) + u32::from(hole) + u32::from(crack) + u32::from(fan);
    if geo_limit < need {
        return FX_GLASS_FREE_SENTINEL;
    }
    let Some(place) = places.get_mut(piece as usize) else {
        return FX_GLASS_FREE_SENTINEL;
    };
    *first_free = fx_glass_place_next_free(place);
    *active = active.saturating_add(1);
    fx_glass_set_in_use(is_in_use, piece);
    let Some(state) = states.get_mut(piece as usize) else {
        return FX_GLASS_FREE_SENTINEL;
    };
    state[FX_GLASS_STATE_VERT_COUNT] = vert;
    state[FX_GLASS_STATE_HOLE_DATA_COUNT] = hole;
    state[FX_GLASS_STATE_CRACK_DATA_COUNT] = crack;
    state[FX_GLASS_STATE_FAN_DATA_COUNT] = fan;
    write_u16(state, FX_GLASS_STATE_GEO_DATA_START, *geo_count as u16);
    write_u16(state, FX_GLASS_STATE_FLAGS, 0);
    *geo_count = need;
    piece
}

pub fn fx_glass_free_piece(
    places: &mut [[u8; FX_GLASS_PIECE_PLACE]],
    is_in_use: &mut [u32],
    first_free: &mut u32,
    active: &mut u32,
    piece: u32,
) -> bool {
    let Some(place) = places.get_mut(piece as usize) else {
        return false;
    };
    *active = active.saturating_sub(1);
    fx_glass_place_set_next_free(place, *first_free);
    *first_free = piece;
    fx_glass_clear_in_use(is_in_use, piece);
    true
}
