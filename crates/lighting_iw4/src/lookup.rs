use crate::pick::light_grid_primary_light_sun_band;

#[inline]
pub fn light_grid_lookup_remap_primary(
    picked: u8,
    fallback_primary: u8,
    header_byte0_nonzero: bool,
    sun_primary_first: u32,
) -> u8 {
    let sun_band = light_grid_primary_light_sun_band(sun_primary_first);
    let picked_u = u32::from(picked);
    if picked_u < sun_band {
        if header_byte0_nonzero && (picked == 0 || sun_primary_first < picked_u) {
            fallback_primary
        } else {
            picked
        }
    } else {
        picked
            .wrapping_sub(0xff)
            .wrapping_add(sun_primary_first as u8)
    }
}

#[inline]
pub fn light_grid_lookup_corner_wants_trace(
    corner_primary: u8,
    selected_primary: u8,
    sun_band: u32,
) -> bool {
    corner_primary == 0 || (u32::from(corner_primary) >= sun_band && selected_primary != 0)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightGridLookupWeights {
    pub matching_primary: f32,

    pub traced_influence: f32,

    pub total: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightGridLookupCorner {
    pub colors_index: u16,
    pub primary_light: u8,
    pub weight: f32,

    pub trace_allows: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightGridLookupColorAccum {
    pub indices: [u16; 8],
    pub weights: [f32; 8],
    pub count: u32,
}

impl LightGridLookupColorAccum {
    pub const fn new() -> Self {
        Self {
            indices: [0; 8],
            weights: [0.0; 8],
            count: 0,
        }
    }

    pub fn add(&mut self, colors_index: u16, weight: f32) {
        let n = self.count as usize;
        for i in 0..n {
            if self.indices[i] == colors_index {
                self.weights[i] += weight;
                return;
            }
        }
        if n < 8 {
            self.indices[n] = colors_index;
            self.weights[n] = weight;
            self.count = (n as u32) + 1;
        }
    }
}

pub fn light_grid_lookup_accumulate_corner(
    weights: &mut LightGridLookupWeights,
    colors: &mut LightGridLookupColorAccum,
    corner: &LightGridLookupCorner,
    selected_primary: u8,
    sun_band: u32,
) {
    if corner.primary_light == selected_primary {
        weights.matching_primary += corner.weight;
    } else if light_grid_lookup_corner_wants_trace(corner.primary_light, selected_primary, sun_band)
        && corner.trace_allows
    {
        weights.traced_influence += corner.weight;
    }
    weights.total += corner.weight;
    colors.add(corner.colors_index, corner.weight);
}

pub fn light_grid_lookup_accumulate_corners(
    corners: &[LightGridLookupCorner],
    selected_primary: u8,
    sun_band: u32,
) -> (LightGridLookupWeights, LightGridLookupColorAccum) {
    let mut weights = LightGridLookupWeights::default();
    let mut colors = LightGridLookupColorAccum::new();
    for corner in corners {
        light_grid_lookup_accumulate_corner(
            &mut weights,
            &mut colors,
            corner,
            selected_primary,
            sun_band,
        );
    }
    (weights, colors)
}
