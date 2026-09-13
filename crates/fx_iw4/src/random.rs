use crate::pool::FX_RAND_TABLE_MOD;

const FX_RANDOM_TABLE_BYTES: &[u8] = include_bytes!("../data/fx_random_table.bin");

pub const FX_RANDOM_TABLE_FLOATS: usize = 508;

pub const FX_RAND_CH_ATLAS: u32 = 22;

pub const FX_RAND_CH_LIFE: u32 = 17;

pub const FX_RAND_CH_DELAY: u32 = 18;

pub const FX_RAND_CH_ONESHOT_COUNT: u32 = 19;

pub const FX_RAND_CH_EMIT_DIST: u32 = 20;

pub const FX_RAND_CH_VISUAL: u32 = 21;

pub fn fx_elem_visual_index(visual_count: u8, random_seed: u32) -> usize {
    let count = usize::from(visual_count);
    if count <= 1 {
        return 0;
    }
    let lo = u32::from(fx_random_table_u16(random_seed, FX_RAND_CH_VISUAL));
    ((count as u32).wrapping_mul(lo) >> 16) as usize
}

pub const FX_RAND_CH_COLOR: u32 = 23;

pub const FX_RAND_CH_INITIAL_ROTATION: u32 = 24;

pub const FX_RAND_CH_ROTATION_DELTA: u32 = 25;

pub const FX_RAND_CH_SIZE0: u32 = 26;

pub const FX_RAND_CH_SCALE: u32 = 28;

pub const FX_RAND_CH_SPAWN_ORIGIN_X: u32 = 6;

pub const FX_RAND_CH_SPAWN_ORIGIN_Y: u32 = 7;

pub const FX_RAND_CH_SPAWN_ORIGIN_Z: u32 = 8;

pub const FX_RAND_CH_SPAWN_OFFSET_YAW: u32 = 9;

pub const FX_RAND_CH_SPAWN_OFFSET_HEIGHT: u32 = 10;

pub const FX_RAND_CH_SPAWN_OFFSET_RADIUS: u32 = 11;

pub const FX_RAND_CH_SPAWN_ANGLES_PITCH: u32 = 12;

pub const FX_RAND_CH_SPAWN_ANGLES_YAW: u32 = 13;

pub const FX_RAND_CH_SPAWN_ANGLES_ROLL: u32 = 14;

pub const FX_RAND_CH_GRAVITY: u32 = 15;

pub const FX_RAND_CH_REFLECTION: u32 = 16;

pub const FX_RAND_CH_ANG_VEL_PITCH: u32 = 3;

pub const FX_RAND_CH_ANG_VEL_YAW: u32 = 4;

pub const FX_RAND_CH_ANG_VEL_ROLL: u32 = 5;

#[inline]
pub fn fx_effect_random_seed_from_msec(msec_begin: i32) -> u16 {
    let x = (msec_begin as u32)
        .wrapping_mul(0x343fd)
        .wrapping_add(0x269ec3);
    ((((x >> 17) as u32).wrapping_mul(FX_RAND_TABLE_MOD)) >> 15) as u16
}

#[inline]
pub fn fx_effect_random_seed_from_rand(rand_i32: i32) -> u16 {
    let prod = rand_i32.wrapping_mul(FX_RAND_TABLE_MOD as i32);
    let adj = prod.wrapping_add((prod >> 31) & 0x7fff);
    (adj >> 15) as u16
}

#[inline]
pub fn fx_elem_random_seed(effect_seed: u16, sequence: u8, msec_begin: i32) -> u32 {
    (u32::from(effect_seed)
        .wrapping_add(u32::from(sequence).wrapping_mul(0x128))
        .wrapping_add(msec_begin as u32))
        % FX_RAND_TABLE_MOD
}

#[inline]
pub fn fx_trail_random_seed(effect_seed: u16, sequence: i8) -> u32 {
    let seq_term = (sequence as i32).wrapping_mul(0x128) as u32;
    u32::from(effect_seed).wrapping_add(seq_term) % FX_RAND_TABLE_MOD
}

#[inline]
fn table_index(seed: u32, channel: u32) -> usize {
    let base = seed % FX_RAND_TABLE_MOD;
    (base + channel) as usize
}

#[inline]
pub fn fx_random_table_f32(seed: u32, channel: u32) -> f32 {
    let idx = table_index(seed, channel);
    let off = idx * 4;
    debug_assert!(off + 4 <= FX_RANDOM_TABLE_BYTES.len());
    let b = [
        FX_RANDOM_TABLE_BYTES[off],
        FX_RANDOM_TABLE_BYTES[off + 1],
        FX_RANDOM_TABLE_BYTES[off + 2],
        FX_RANDOM_TABLE_BYTES[off + 3],
    ];
    f32::from_le_bytes(b)
}

#[inline]
pub fn fx_random_table_u16(seed: u32, channel: u32) -> u16 {
    let idx = table_index(seed, channel);
    let off = idx * 4;
    debug_assert!(off + 2 <= FX_RANDOM_TABLE_BYTES.len());
    u16::from_le_bytes([FX_RANDOM_TABLE_BYTES[off], FX_RANDOM_TABLE_BYTES[off + 1]])
}
