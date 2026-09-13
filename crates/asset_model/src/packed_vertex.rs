pub fn unpack_unit_vec(packed: u32) -> [f32; 3] {
    let bytes = packed.to_le_bytes();
    let scale = (f32::from(bytes[3]) + 192.0) / 32_385.0;
    [
        (f32::from(bytes[0]) - 127.0) * scale,
        (f32::from(bytes[1]) - 127.0) * scale,
        (f32::from(bytes[2]) - 127.0) * scale,
    ]
}

pub fn normalize_or_up(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length == 0.0 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / length, v[1] / length, v[2] / length]
    }
}

pub fn half_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = (bits >> 10) & 0x1f;
    let mantissa = u32::from(bits & 0x03ff);
    let value = match exponent {
        0 if mantissa == 0 => sign,
        0 => {
            let mut mantissa = mantissa;
            let mut exponent = -14i32;
            while mantissa & 0x0400 == 0 {
                mantissa <<= 1;
                exponent -= 1;
            }
            sign | (((exponent + 127) as u32) << 23) | ((mantissa & 0x03ff) << 13)
        }
        0x1f => sign | 0x7f80_0000 | (mantissa << 13),
        _ => sign | ((u32::from(exponent) + 112) << 23) | (mantissa << 13),
    };
    f32::from_bits(value)
}

pub fn unpack_packed_tex_coords(packed: u32) -> [f32; 2] {
    [
        half_to_f32((packed >> 16) as u16),
        half_to_f32(packed as u16),
    ]
}

pub fn unpack_color_u8(packed: u32) -> [u8; 4] {
    let [b, g, r, a] = packed.to_le_bytes();
    [r, g, b, a]
}

pub fn unpack_color(packed: u32) -> [f32; 4] {
    let [r, g, b, a] = unpack_color_u8(packed);
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        f32::from(a) / 255.0,
    ]
}
