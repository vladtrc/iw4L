use crate::expand::CONST_SRC_CODE_BASE_LIGHTING_COORDS;

pub const CONST_SRC_CODE_LIGHT_PROBE_AMBIENT: u8 =
    CONST_SRC_CODE_BASE_LIGHTING_COORDS.wrapping_add(1);

pub const LIGHT_PROBE_AMBIENT_RGB_SCALE_BITS: u32 = 0x3c00_8081;

pub const LIGHT_PROBE_AMBIENT_WEIGHT_SCALE_BITS: u32 = 0x3a80_8081;

pub fn light_probe_ambient_from_packed(packed: [u8; 4]) -> [f32; 4] {
    let rgb = f32::from_bits(LIGHT_PROBE_AMBIENT_RGB_SCALE_BITS);
    let weight = f32::from_bits(LIGHT_PROBE_AMBIENT_WEIGHT_SCALE_BITS);
    let scaled_r = rgb * f32::from(packed[0]);
    let scaled_g = rgb * f32::from(packed[1]);
    let scaled_b = rgb * f32::from(packed[2]);
    [
        scaled_r * scaled_r,
        scaled_g * scaled_g,
        scaled_b * scaled_b,
        weight * f32::from(packed[3]),
    ]
}
