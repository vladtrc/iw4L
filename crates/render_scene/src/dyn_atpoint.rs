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
        match self.walk_trace(mid, box_half) {
            Some(trace) => trace.walk as u8,
            None => LIGHT_GRID_ATPOINT_EMPTY_PRIMARY,
        }
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
        let Some(region_lists) = self.light_region_hulls.as_ref() else {
            return lighting_iw4::dyn_ent_primary_light_link(
                &self.primary_light_cull,
                self.sun_primary_light_count,
                None,
                mid,
                half,
                |light, set| on_bit(light, set),
            )
            .closest;
        };
        let hulls: Vec<Vec<LightRegionHull<'_>>> = region_lists
            .iter()
            .map(|list| {
                list.iter()
                    .map(|h| LightRegionHull {
                        kdop_mid: h.kdop_mid,
                        kdop_half: h.kdop_half,
                        axes: &h.axes,
                    })
                    .collect()
            })
            .collect();
        let refs: Vec<lighting_iw4::LightRegionHulls<'_>> =
            hulls.iter().map(|v| v.as_slice()).collect();
        lighting_iw4::dyn_ent_primary_light_link(
            &self.primary_light_cull,
            self.sun_primary_light_count,
            Some(&refs),
            mid,
            half,
            |light, set| on_bit(light, set),
        )
        .closest
    }

    fn walk_trace(
        &self,
        mid: [f32; 3],
        box_half: Option<[f32; 3]>,
    ) -> Option<lighting_iw4::NonSunPrimaryWalkTrace> {
        let half = box_half?;
        let region_lists = self.light_region_hulls.as_ref()?;
        let hulls: Vec<Vec<LightRegionHull<'_>>> = region_lists
            .iter()
            .map(|list| {
                list.iter()
                    .map(|h| LightRegionHull {
                        kdop_mid: h.kdop_mid,
                        kdop_half: h.kdop_half,
                        axes: &h.axes,
                    })
                    .collect()
            })
            .collect();
        let refs: Vec<lighting_iw4::LightRegionHulls<'_>> =
            hulls.iter().map(|v| v.as_slice()).collect();
        Some(lighting_iw4::non_sun_primary_light_walk_trace(
            &self.primary_light_cull,
            self.sun_primary_light_count,
            Some(&refs),
            mid,
            half,
        ))
    }
}
