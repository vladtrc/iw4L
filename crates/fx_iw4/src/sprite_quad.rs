pub const FX_SPRITE_QUAD_LOCAL_XY: [[f32; 2]; 4] =
    [[-0.5, -0.5], [-0.5, 0.5], [0.5, 0.5], [0.5, -0.5]];

pub const FX_SPRITE_QUAD_UV_FULL: [[f32; 2]; 4] = [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];

pub const FX_SPRITE_QUAD_INDICES: [u16; 6] = [0, 1, 2, 2, 3, 0];

#[inline]
pub fn fx_sprite_quad_indices(base: u32) -> [u32; 6] {
    [
        base,
        base.wrapping_add(1),
        base.wrapping_add(2),
        base.wrapping_add(2),
        base.wrapping_add(3),
        base,
    ]
}
