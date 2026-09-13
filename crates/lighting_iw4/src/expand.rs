use crate::accumulate::LIGHT_GRID_COLORS_BYTE_COUNT;

pub const MODEL_LIGHTING_TILE_DIM: usize = 4;

pub const MODEL_LIGHTING_TILE_TEXELS: usize = 64;

pub const MODEL_LIGHTING_TILE_BYTES: usize = MODEL_LIGHTING_TILE_TEXELS * 4;

pub const LIGHT_GRID_SHELL_DIRS: usize = 56;

pub const LIGHT_GRID_EXPAND_SHELL_TO_TEXEL: [u8; MODEL_LIGHTING_TILE_TEXELS] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 0, 3, 21, 22, 12, 15,
    23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 40, 43, 33, 34, 52, 55, 35, 36, 37, 38, 39, 40, 41, 42,
    43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55,
];

pub const LIGHT_GRID_COMPRESS_DIRS: [usize; 8] = [0, 3, 12, 15, 40, 43, 52, 55];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelLightingExpandPitch {
    pub row_pitch: i32,

    pub slice_minus_3_rows: i32,
}

#[inline]
pub const fn model_lighting_expand_pitch(
    row_pitch: i32,
    slice_pitch: i32,
) -> ModelLightingExpandPitch {
    ModelLightingExpandPitch {
        row_pitch,
        slice_minus_3_rows: slice_pitch.wrapping_sub(row_pitch.wrapping_mul(3)),
    }
}

pub const MODEL_LIGHTING_PACKED_ROW_PITCH: i32 = 16;
pub const MODEL_LIGHTING_PACKED_SLICE_PITCH: i32 = 64;

pub const MODEL_LIGHTING_COORD_BIAS: f64 = 2.0;

pub const MODEL_LIGHTING_INV_ATLAS_WIDTH: f64 = 0.00390625;

pub const MODEL_LIGHTING_VOLUME_W: f32 = 0.5;

pub const MODEL_LIGHTING_PACKED_W: u8 = 0x80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelLightingPackedCoords {
    pub bytes: [u8; 4],
}

pub const fn model_lighting_packed_coords_from_entry(
    entry: u32,
    image_height: u32,
) -> Option<ModelLightingPackedCoords> {
    if image_height == 0 || (image_height & (image_height - 1)) != 0 {
        return None;
    }
    if image_height > MODEL_LIGHTING_ATLAS_WIDTH {
        return None;
    }
    let x_pixel = 4 * (entry & 0x3f);
    let x = x_pixel.wrapping_add(2);
    let y = ((entry >> 4) & !3).wrapping_add(2 * (MODEL_LIGHTING_ATLAS_WIDTH / image_height));
    if x > 0xff || y > 0xff {
        return None;
    }
    Some(ModelLightingPackedCoords {
        bytes: [x as u8, y as u8, MODEL_LIGHTING_PACKED_W, 0],
    })
}

#[inline]
pub const fn model_lighting_entry_from_handle(handle: u16) -> Option<u16> {
    handle.checked_sub(1)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelLightingCoords {
    pub u: f32,
    pub v: f32,
    pub w: f32,
    pub q: f32,
}

impl ModelLightingCoords {
    #[inline]
    pub const fn as_array(self) -> [f32; 4] {
        [self.u, self.v, self.w, self.q]
    }
}

pub fn model_lighting_coords_from_handle(
    handle: u16,
    inv_image_height: f32,
) -> Option<ModelLightingCoords> {
    let entry = u32::from(model_lighting_entry_from_handle(handle)?);
    let u = ((entry & 0x3f) * 4) as f32 + (MODEL_LIGHTING_COORD_BIAS as f32);
    let v = ((entry >> 4) & 0xffff_fffc) as f32 + (MODEL_LIGHTING_COORD_BIAS as f32);
    Some(ModelLightingCoords {
        u: u * (MODEL_LIGHTING_INV_ATLAS_WIDTH as f32),
        v: v * inv_image_height,
        w: MODEL_LIGHTING_VOLUME_W,
        q: 1.0,
    })
}

#[inline]
pub fn model_lighting_inv_image_height(image_height: u32) -> Option<f32> {
    if image_height == 0 {
        None
    } else {
        Some(1.0 / (image_height as f32))
    }
}

pub const MODEL_LIGHTING_ATLAS_WIDTH: u32 = 256;

pub const MODEL_LIGHTING_ATLAS_DEPTH: u32 = 4;

pub const MODEL_LIGHTING_SMODEL_ENTRY_LIMIT_FLOOR: u32 = 0x800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelLightingAtlasDims {
    pub max_client_views: u32,

    pub xmodel_entry_limit: u32,

    pub smodel_entry_limit: u32,

    pub total_entry_limit: u32,

    pub image_height: u32,
}

pub const fn model_lighting_atlas_dims(max_client_views: u32) -> Option<ModelLightingAtlasDims> {
    if max_client_views == 0 {
        return None;
    }
    let xmodel_entry_limit = max_client_views << 10;

    let mut total_bits = 32u32 - xmodel_entry_limit.leading_zeros();
    let mut smodel_entry_limit = (1u32 << total_bits).wrapping_sub(xmodel_entry_limit);
    while smodel_entry_limit < MODEL_LIGHTING_SMODEL_ENTRY_LIMIT_FLOOR {
        smodel_entry_limit = smodel_entry_limit.wrapping_add(1u32 << total_bits);
        total_bits += 1;
        if total_bits >= 31 {
            return None;
        }
    }
    let total_entry_limit = 1u32 << total_bits;
    let image_height = 1u32 << (total_bits - 4);
    Some(ModelLightingAtlasDims {
        max_client_views,
        xmodel_entry_limit,
        smodel_entry_limit,
        total_entry_limit,
        image_height,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelLightingCapExceeded {
    pub lit_slots: u32,
    pub smodel_entry_limit: u32,
    pub max_client_views: u32,
}

pub const fn check_smodel_lit_within_retail_cap(
    lit_slots: u32,
    max_client_views: u32,
) -> Result<(), ModelLightingCapExceeded> {
    match model_lighting_atlas_dims(max_client_views) {
        None => Err(ModelLightingCapExceeded {
            lit_slots,
            smodel_entry_limit: 0,
            max_client_views,
        }),
        Some(d) if lit_slots > d.smodel_entry_limit => Err(ModelLightingCapExceeded {
            lit_slots,
            smodel_entry_limit: d.smodel_entry_limit,
            max_client_views,
        }),
        Some(_) => Ok(()),
    }
}

#[inline]
pub const fn model_lighting_atlas_slice_offset(
    entry: ModelLightingTileIndex,
    image_height: u32,
    z: u32,
) -> Option<usize> {
    if z >= MODEL_LIGHTING_ATLAS_DEPTH {
        return None;
    }
    let (x, y) = entry.texel_origin();
    if y + (MODEL_LIGHTING_TILE_DIM as u32) > image_height {
        return None;
    }
    let row_pitch = (MODEL_LIGHTING_ATLAS_WIDTH * 4) as usize;
    let slice_pitch = row_pitch * (image_height as usize);
    Some(z as usize * slice_pitch + y as usize * row_pitch + x as usize * 4)
}

pub fn model_lighting_write_tile_to_atlas(
    dst: &mut [u8],
    image_height: u32,
    entry: ModelLightingTileIndex,
    tile: &[u8; MODEL_LIGHTING_TILE_BYTES],
) -> bool {
    let row_pitch = (MODEL_LIGHTING_ATLAS_WIDTH * 4) as usize;
    let needed = row_pitch * (image_height as usize) * (MODEL_LIGHTING_ATLAS_DEPTH as usize);
    if dst.len() < needed {
        return false;
    }
    let mut z = 0u32;
    while z < MODEL_LIGHTING_ATLAS_DEPTH {
        let Some(base) = model_lighting_atlas_slice_offset(entry, image_height, z) else {
            return false;
        };
        for y in 0..MODEL_LIGHTING_TILE_DIM {
            let src = (z as usize * 16 + y * MODEL_LIGHTING_TILE_DIM) * 4;
            let dst_row = base + y * row_pitch;
            dst[dst_row..dst_row + MODEL_LIGHTING_TILE_DIM * 4]
                .copy_from_slice(&tile[src..src + MODEL_LIGHTING_TILE_DIM * 4]);
        }
        z += 1;
    }
    true
}

pub const CONST_SRC_CODE_BASE_LIGHTING_COORDS: u8 = 0x3A;

pub const CONST_SRC_CODE_LIGHTING_LOOKUP_SCALE: u8 = 0x22;

pub const TEXTURE_SRC_CODE_MODEL_LIGHTING: u8 = 0x3;

pub const TEXTURE_SRC_CODE_LIGHT_ATTENUATION: u8 = 0xD;

pub const TECHNIQUE_LIT: u8 = 0x9;

pub const MODEL_LIGHTING_LOOKUP_SCALE_U: f32 = 0.005859375;

pub const MODEL_LIGHTING_LOOKUP_SCALE_V_FACTOR: f64 = 1.5;

pub const MODEL_LIGHTING_LOOKUP_SCALE_W: f32 = 0.375;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelLightingLookupScale {
    pub u: f32,
    pub v: f32,
    pub w: f32,
    pub q: f32,
}

impl ModelLightingLookupScale {
    #[inline]
    pub const fn as_array(self) -> [f32; 4] {
        [self.u, self.v, self.w, self.q]
    }
}

pub fn model_lighting_lookup_scale(inv_image_height: f32) -> ModelLightingLookupScale {
    ModelLightingLookupScale {
        u: MODEL_LIGHTING_LOOKUP_SCALE_U,
        v: (MODEL_LIGHTING_LOOKUP_SCALE_V_FACTOR as f32) * inv_image_height,
        w: MODEL_LIGHTING_LOOKUP_SCALE_W,
        q: 0.0,
    }
}

#[inline]
pub fn model_lighting_solid_tile_bgra(ground: u32, out: &mut [u8; MODEL_LIGHTING_TILE_BYTES]) {
    let bytes = ground.to_le_bytes();
    let mut i = 0;
    while i < MODEL_LIGHTING_TILE_BYTES {
        out[i] = bytes[0];
        out[i + 1] = bytes[1];
        out[i + 2] = bytes[2];
        out[i + 3] = bytes[3];
        i += 4;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelLightingTileIndex {
    pub raw: u16,
}

impl ModelLightingTileIndex {
    #[inline]
    pub const fn from_raw(raw: u16) -> Self {
        Self { raw }
    }

    #[inline]
    pub const fn from_handle(handle: u16) -> Option<Self> {
        if handle == 0 {
            None
        } else {
            Some(Self {
                raw: handle.wrapping_sub(1),
            })
        }
    }

    #[inline]
    pub const fn low6(self) -> u16 {
        self.raw & 0x3f
    }

    #[inline]
    pub const fn high_aligned(self) -> u16 {
        (self.raw >> 4) & 0xfffc
    }

    #[inline]
    pub const fn tile_x(self) -> u32 {
        self.low6() as u32
    }

    #[inline]
    pub const fn tile_y(self) -> u32 {
        (self.raw as u32) >> 6
    }

    #[inline]
    pub const fn texel_origin(self) -> (u32, u32) {
        (
            self.tile_x() * (MODEL_LIGHTING_TILE_DIM as u32),
            self.tile_y() * (MODEL_LIGHTING_TILE_DIM as u32),
        )
    }

    #[inline]
    pub const fn byte_offset(self, row_pitch: i32) -> i32 {
        row_pitch
            .wrapping_mul(self.high_aligned() as i32)
            .wrapping_add((self.low6() as i32).wrapping_mul(0x10))
    }
}

#[inline]
pub const fn model_lighting_packed_ground_rgbw(r: u8, g: u8, b: u8, weight: u8) -> [u8; 4] {
    [r, g, b, weight]
}

pub fn light_grid_expand_shell_to_tile_rgba(colors: &[u8], alpha: u8, out: &mut [u8]) -> bool {
    if colors.len() < LIGHT_GRID_COLORS_BYTE_COUNT || out.len() < MODEL_LIGHTING_TILE_BYTES {
        return false;
    }
    for (texel, &dir) in LIGHT_GRID_EXPAND_SHELL_TO_TEXEL.iter().enumerate() {
        let src = (dir as usize) * 3;
        let dst = texel * 4;
        out[dst] = colors[src];
        out[dst + 1] = colors[src + 1];
        out[dst + 2] = colors[src + 2];
        out[dst + 3] = alpha;
    }
    true
}

pub type ModelLightingTileRgba =
    [[[[u8; 4]; MODEL_LIGHTING_TILE_DIM]; MODEL_LIGHTING_TILE_DIM]; MODEL_LIGHTING_TILE_DIM];

pub fn light_grid_expand_shell_to_tile_zyx(
    colors: &[u8],
    alpha: u8,
) -> Option<ModelLightingTileRgba> {
    if colors.len() < LIGHT_GRID_COLORS_BYTE_COUNT {
        return None;
    }
    let mut tile: ModelLightingTileRgba =
        [[[[0u8; 4]; MODEL_LIGHTING_TILE_DIM]; MODEL_LIGHTING_TILE_DIM]; MODEL_LIGHTING_TILE_DIM];
    for (texel, &dir) in LIGHT_GRID_EXPAND_SHELL_TO_TEXEL.iter().enumerate() {
        let x = texel & 3;
        let y = (texel >> 2) & 3;
        let z = texel >> 4;
        let src = (dir as usize) * 3;
        tile[z][y][x] = [colors[src], colors[src + 1], colors[src + 2], alpha];
    }
    Some(tile)
}
