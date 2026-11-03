use asset_model::pack_unit_vec;
pub fn unpack_unit_vec_t6(packed: u32) -> [f32; 3] {
    let axis = |shift: u32| {
        let bits = (packed >> shift) & 0x3FF;
        let raw = bits
            .wrapping_sub(2 * (bits & 0x200))
            .wrapping_add(0x4040_0000);
        (f32::from_bits(raw) - 3.0) * 8208.031
    };
    [axis(0), axis(10), axis(20)]
}

pub fn repack_vertex_t6(mut vertex: [u8; 32]) -> [u8; 32] {
    let word = |v: &[u8; 32], o: usize| u32::from_le_bytes(v[o..o + 4].try_into().unwrap());
    let uv = word(&vertex, 20);
    vertex[20..24].copy_from_slice(&uv.rotate_left(16).to_le_bytes());
    for o in [24, 28] {
        let packed = pack_unit_vec(unpack_unit_vec_t6(word(&vertex, o)));
        vertex[o..o + 4].copy_from_slice(&packed.to_le_bytes());
    }
    vertex
}
