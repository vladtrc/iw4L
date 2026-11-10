use asset_model::model_lighting::*;
pub const CODEC: LightGridCodec = LightGridCodec {
    id: "t6-coefficients",
    row_bytes: ext_t6_fastfile::light_grid::COEFFICIENT_ROW_BYTES,
    linear_blend: true,
    baked_visibility: true,
    transparent_interior: true,
    supports_palette: true,
    texel_decode_scale: 1.0,
    reflection_probe_alpha_weight: 1.0,
    decode: |bytes| {
        let coefficients = ext_t6_fastfile::light_grid::decode_coefficients(bytes)?;
        Some(DecodedGridColors {
            directional: ext_t6_fastfile::light_grid::directional_colors(&coefficients),
            coefficients: Some(coefficients),
        })
    },
    quantize: |value| ((value.clamp(0.0, 32.0) * 0.03125).sqrt() * 255.0) as u8,
    pack_sh: Some(ext_t6_fastfile::light_grid::lighting_sh),
};
