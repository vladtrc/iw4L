use asset_model::model_lighting::*;
use lighting_iw4::{LIGHT_GRID_COLORS_BYTE_COUNT, lighting_origin_from_inst_bytes};
pub fn capture_light_grid(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    grid: ext_t5_fastfile::GfxLightGridGeometry,
) -> Option<OwnedLightGrid> {
    if grid.row_axis > 2 || grid.col_axis > 2 {
        return None;
    }
    let (Some(row_data_start), Some(raw_row_data), Some(entries), Some(colors)) = (
        grid.row_data_start,
        grid.raw_row_data,
        grid.entries,
        grid.colors,
    ) else {
        return None;
    };
    let row_data_start = stream
        .slice_at(row_data_start, 0, grid.row_count * 2)
        .ok()?
        .to_vec();
    let raw_row_data = stream
        .slice_at(raw_row_data, 0, grid.raw_row_data_size)
        .ok()?
        .to_vec();
    let entries = stream
        .slice_at(entries, 0, grid.entry_count * 4)
        .ok()?
        .to_vec();
    let colors = stream
        .slice_at(colors, 0, grid.color_count * LIGHT_GRID_COLORS_BYTE_COUNT)
        .ok()?
        .to_vec();
    Some(OwnedLightGrid {
        hero_lights: Vec::new(),
        mins: grid.mins,
        maxs: grid.maxs,
        row_axis: grid.row_axis as usize,
        col_axis: grid.col_axis as usize,
        color_count: grid.color_count as u32,
        has_light_regions: grid.has_light_regions,
        sun_primary_light_index: grid.sun_primary_light_index,
        row_data_start,
        raw_row_data,
        entries,
        colors,
        codec: &CODEC,
    })
}
pub fn collect_t5_smodel_lighting_origins(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    world: ext_t5_fastfile::GfxWorldGeometry,
) -> Vec<(usize, [f32; 3])> {
    let Some(smodel_insts) = world.smodel_insts else {
        return Vec::new();
    };
    let stride = ext_t5_fastfile::size::GFX_STATIC_MODEL_INST;
    let mut out = Vec::with_capacity(world.smodel_count);
    for i in 0..world.smodel_count {
        let inst = smodel_insts.at(i * stride);
        let Ok(bytes) = stream.slice_at(inst, 0, stride) else {
            continue;
        };
        let Some(origin) = lighting_origin_from_inst_bytes(bytes) else {
            continue;
        };
        out.push((i, origin));
    }
    out
}
pub const CODEC: LightGridCodec = LightGridCodec {
    id: "t5-y12u6w6",
    row_bytes: LIGHT_GRID_COLORS_BYTE_COUNT,
    linear_blend: true,
    baked_visibility: false,
    transparent_interior: false,
    supports_palette: false,
    texel_decode_scale: 1.0,
    reflection_probe_alpha_weight: 0.0,
    decode: |bytes| {
        Some(DecodedGridColors {
            directional: ext_t5_fastfile::light_grid::decode_light_grid_colors(bytes)?,
            coefficients: None,
        })
    },
    quantize: |value| {
        ((value * f32::from_bits(0x3d008081)).min(1.0).sqrt() * 255.0).round_ties_even() as u8
    },
    pack_sh: None,
};
