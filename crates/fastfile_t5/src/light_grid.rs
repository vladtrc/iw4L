pub fn decode_light_grid_colors(bytes: &[u8]) -> Option<[[f32; 3]; 56]> {
    let bytes = bytes.get(..168)?;
    let mut colors = [[0.0; 3]; 56];
    for (out, packed) in colors.iter_mut().zip(bytes.chunks_exact(3)) {
        let bits = u32::from_le_bytes([packed[0], packed[1], packed[2], 0]);

        let y = (bits >> 12) as f32 * f32::from_bits(0x39800801);
        let y = y * y * 31.875;
        let r = ((bits >> 6) & 63) as f32 * f32::from_bits(0x3c820821) * y * 4.0;
        let b = (bits & 63) as f32 * f32::from_bits(0x3c820821) * y * 4.0;
        *out = [r, ((y - r * 0.25) - b * 0.25) * 2.0, b];
    }
    Some(colors)
}
