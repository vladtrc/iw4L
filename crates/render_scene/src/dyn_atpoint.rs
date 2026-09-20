use bevy::prelude::*;
use lighting_iw4::{ComPrimaryLightCull, LIGHT_GRID_ATPOINT_EMPTY_PRIMARY, LightRegionHull};

#[derive(Resource, Clone, Debug, Default)]
pub struct DynAtPointLookup {
    primary_light_cull: Vec<ComPrimaryLightCull>,
    sun_primary_light_count: u32,
    light_region_hulls: Option<Vec<Vec<assets::WorldLightRegionHull>>>,
}

impl DynAtPointLookup {
    pub fn replace(
        &mut self,
        primary_light_cull: Vec<ComPrimaryLightCull>,
        sun_primary_light_count: u32,
        light_region_hulls: Option<Vec<Vec<assets::WorldLightRegionHull>>>,
    ) {
        self.primary_light_cull = primary_light_cull;
        self.sun_primary_light_count = sun_primary_light_count;
        self.light_region_hulls = light_region_hulls;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn fallback(&self, mid: [f32; 3], box_half: Option<[f32; 3]>) -> u8 {
        let (Some(half), Some(regions)) = (box_half, self.light_region_hulls.as_ref()) else {
            return LIGHT_GRID_ATPOINT_EMPTY_PRIMARY;
        };
        for (i, (light, hulls)) in self
            .primary_light_cull
            .iter()
            .zip(regions)
            .enumerate()
            .skip(self.sun_primary_light_count.saturating_add(1) as usize)
        {
            if !lighting_iw4::cull_box_from_primary_light(light, mid, half)
                && !region_culls_box(hulls, light.origin, mid, half)
            {
                return i as u8;
            }
        }
        0
    }

    #[must_use]
    pub fn sun_primary(&self) -> u32 {
        self.sun_primary_light_count
    }

    #[must_use]
    pub fn primary_count(&self) -> u32 {
        self.primary_light_cull.len() as u32
    }

    pub fn link_dyn_ent_primary(
        &self,
        mid: [f32; 3],
        half: [f32; 3],
        mut on_bit: impl FnMut(u32, bool),
    ) -> u8 {
        let mut closest = 0;
        let mut best = lighting_iw4::DYN_ENT_PRIMARY_LIGHT_LINK_DIST2_INIT;
        for (i, light) in self
            .primary_light_cull
            .iter()
            .enumerate()
            .skip(self.sun_primary_light_count.saturating_add(1) as usize)
        {
            let region = self
                .light_region_hulls
                .as_ref()
                .and_then(|regions| regions.get(i));
            let set = !lighting_iw4::cull_box_from_primary_light(light, mid, half)
                && region.is_some_and(|hulls| !region_culls_box(hulls, light.origin, mid, half));
            on_bit(i as u32, set);
            if set {
                let distance = lighting_iw4::dyn_ent_primary_light_link_dist2(light.origin, mid);
                if distance < best {
                    best = distance;
                    closest = i as u8;
                }
            }
        }
        closest
    }
}

fn region_culls_box(
    hulls: &[assets::WorldLightRegionHull],
    origin: [f32; 3],
    mid: [f32; 3],
    half: [f32; 3],
) -> bool {
    lighting_iw4::light_region_culls_box(
        hulls.iter().map(|h| LightRegionHull {
            kdop_mid: h.kdop_mid,
            kdop_half: h.kdop_half,
            axes: &h.axes,
        }),
        origin,
        mid,
        half,
    )
}
