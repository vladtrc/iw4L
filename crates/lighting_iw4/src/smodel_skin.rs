use crate::SMC_VERT_STRIDE;

pub const SMC_UNIT_VEC_FIXED_SCALE: f32 = 32768.0;

pub const SMC_UNIT_VEC_SCALE_BIAS: i32 = 0xc0;

pub const SMC_UNIT_VEC_CENTER: i32 = 0x7f;

pub const SMC_UNIT_VEC_PACK_BIAS: i32 = 0x3fc0_0000;

pub const SMC_UNIT_VEC_OUT_W: u8 = 0x40;

const PACKED_STRIDE: usize = SMC_VERT_STRIDE as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmcSkinError {
    ShortDest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmcCachedVertLighting {
    Packed([u8; 4]),

    FromHandle {
        lighting_handle: u16,
        div_0x100_by_height: u32,
    },
}

#[must_use]
pub fn setup_transform_unit_vec(m: &[f32; 16]) -> [i32; 9] {
    let scale = |i: usize| libm::roundf(m[i] * SMC_UNIT_VEC_FIXED_SCALE) as i32;
    [
        scale(0),
        scale(1),
        scale(2),
        scale(4),
        scale(5),
        scale(6),
        scale(8),
        scale(9),
        scale(10),
    ]
}

#[must_use]
pub fn local_transform_unit_vec(fixed: &[i32; 9], packed: u32) -> u32 {
    let b0 = (packed & 0xff) as i32;
    let b1 = ((packed >> 8) & 0xff) as i32;
    let b2 = ((packed >> 16) & 0xff) as i32;
    let b3 = (packed >> 24) as i32;
    let scale = b3 + SMC_UNIT_VEC_SCALE_BIAS;
    let ix = (b0 - SMC_UNIT_VEC_CENTER) * scale;
    let iy = (b1 - SMC_UNIT_VEC_CENTER) * scale;
    let iz = (b2 - SMC_UNIT_VEC_CENTER) * scale;
    let pack = |a: i32, b: i32, c: i32| {
        a.wrapping_mul(ix)
            .wrapping_add(b.wrapping_mul(iy))
            .wrapping_add(c.wrapping_mul(iz))
            .wrapping_add(SMC_UNIT_VEC_PACK_BIAS)
            >> 23
    };
    u32::from_le_bytes([
        pack(fixed[0], fixed[3], fixed[6]) as u8,
        pack(fixed[1], fixed[4], fixed[7]) as u8,
        pack(fixed[2], fixed[5], fixed[8]) as u8,
        SMC_UNIT_VEC_OUT_W,
    ])
}

fn lighting_bytes(kind: SmcCachedVertLighting, packed_binormal_bits: u32) -> [u8; 4] {
    match kind {
        SmcCachedVertLighting::Packed(bytes) => bytes,
        SmcCachedVertLighting::FromHandle {
            lighting_handle,
            div_0x100_by_height,
        } => {
            let entry = u32::from(lighting_handle.wrapping_sub(1));
            let b0 = (entry.wrapping_mul(4).wrapping_add(2)) as u8;
            let b1 = (div_0x100_by_height
                .wrapping_mul(2)
                .wrapping_add((entry >> 4) & 0xfc)) as u8;
            let b3 = ((((packed_binormal_bits as i32) >> 30) as u8) & 0xfe).wrapping_add(2);
            [b0, b1, 0x80, b3]
        }
    }
}

fn f32_at(row: &[u8; PACKED_STRIDE], off: usize) -> f32 {
    f32::from_le_bytes([row[off], row[off + 1], row[off + 2], row[off + 3]])
}

fn u32_at(row: &[u8; PACKED_STRIDE], off: usize) -> u32 {
    u32::from_le_bytes([row[off], row[off + 1], row[off + 2], row[off + 3]])
}

fn write_f32(out: &mut [u8], off: usize, v: f32) {
    out[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn write_u32(out: &mut [u8], off: usize, v: u32) {
    out[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

pub fn r_skin_xsurface_static_vert(
    dest: &mut [u8; PACKED_STRIDE],
    src: &[u8; PACKED_STRIDE],
    m: &[f32; 16],
    fixed_norm_axis: &[i32; 9],
    lighting: SmcCachedVertLighting,
) {
    let x = f32_at(src, 0);
    let y = f32_at(src, 4);
    let z = f32_at(src, 8);
    write_f32(dest, 0, m[0] * x + m[4] * y + m[8] * z + m[12]);
    write_f32(dest, 4, m[1] * x + m[5] * y + m[9] * z + m[13]);
    write_f32(dest, 8, m[2] * x + m[6] * y + m[10] * z + m[14]);
    dest[12..16].copy_from_slice(&src[16..20]);
    dest[16..20].copy_from_slice(&src[20..24]);
    write_u32(
        dest,
        20,
        local_transform_unit_vec(fixed_norm_axis, u32_at(src, 24)),
    );
    write_u32(
        dest,
        24,
        local_transform_unit_vec(fixed_norm_axis, u32_at(src, 28)),
    );
    let binormal = u32_at(src, 12);
    let lit = match lighting {
        SmcCachedVertLighting::Packed(_) => lighting_bytes(lighting, 0),
        SmcCachedVertLighting::FromHandle { .. } => lighting_bytes(lighting, binormal),
    };
    dest[28..32].copy_from_slice(&lit);
}

pub fn r_skin_xsurface_static_verts(
    dest: &mut [u8],
    src: &[[u8; PACKED_STRIDE]],
    m: &[f32; 16],
    fixed_norm_axis: &[i32; 9],
    lighting: SmcCachedVertLighting,
) -> Result<(), SmcSkinError> {
    let n = src.len();
    let need = n
        .checked_mul(PACKED_STRIDE)
        .ok_or(SmcSkinError::ShortDest)?;
    if dest.len() < need {
        return Err(SmcSkinError::ShortDest);
    }
    for (i, src_row) in src.iter().enumerate() {
        let off = i * PACKED_STRIDE;
        let mut row = [0u8; PACKED_STRIDE];
        r_skin_xsurface_static_vert(&mut row, src_row, m, fixed_norm_axis, lighting);
        dest[off..off + PACKED_STRIDE].copy_from_slice(&row);
    }
    Ok(())
}

#[must_use]
pub fn r_skin_cached_static_model_cmd_matrix(
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    scale: f32,
) -> [f32; 16] {
    [
        axis[0][0] * scale,
        axis[0][1] * scale,
        axis[0][2] * scale,
        0.0,
        axis[1][0] * scale,
        axis[1][1] * scale,
        axis[1][2] * scale,
        0.0,
        axis[2][0] * scale,
        axis[2][1] * scale,
        axis[2][2] * scale,
        0.0,
        origin[0],
        origin[1],
        origin[2],
        0.0,
    ]
}

pub struct SmcSkinSurface<'a> {
    pub packed: &'a [[u8; PACKED_STRIDE]],
    pub vert_offset: u16,
    pub lighting: SmcCachedVertLighting,
}

pub fn r_skin_cached_static_model_cmd(
    dest: &mut [u8],
    m: &[f32; 16],
    fixed_norm_axis: &[i32; 9],
    surfaces: &[SmcSkinSurface<'_>],
) -> Result<(), SmcSkinError> {
    for surf in surfaces {
        let byte_off = usize::from(surf.vert_offset).saturating_mul(PACKED_STRIDE);
        let need = surf
            .packed
            .len()
            .checked_mul(PACKED_STRIDE)
            .ok_or(SmcSkinError::ShortDest)?;
        let end = byte_off.checked_add(need).ok_or(SmcSkinError::ShortDest)?;
        let slice = dest.get_mut(byte_off..end).ok_or(SmcSkinError::ShortDest)?;
        r_skin_xsurface_static_verts(slice, surf.packed, m, fixed_norm_axis, surf.lighting)?;
    }
    Ok(())
}
