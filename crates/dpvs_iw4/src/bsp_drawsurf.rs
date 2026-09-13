use crate::GfxDrawSurf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxSurfaceDrawFields {
    pub first_vertex: u32,

    pub vertex_count: u16,

    pub tri_count: u16,

    pub base_index: u32,

    pub lightmap_index: u8,

    pub reflection_probe_index: u8,

    pub primary_light_index: u8,
}

pub trait BspSurfaceDrawFields: Copy {
    fn first_vertex(self) -> u32;
    fn tri_count(self) -> u16;
    fn base_index(self) -> u32;
    fn lightmap_index(self) -> u8;
    fn reflection_probe_index(self) -> u8;
    fn primary_light_index(self) -> u8;
}

impl BspSurfaceDrawFields for GfxSurfaceDrawFields {
    fn first_vertex(self) -> u32 {
        self.first_vertex
    }

    fn tri_count(self) -> u16 {
        self.tri_count
    }

    fn base_index(self) -> u32 {
        self.base_index
    }

    fn lightmap_index(self) -> u8 {
        self.lightmap_index
    }

    fn reflection_probe_index(self) -> u8 {
        self.reflection_probe_index
    }

    fn primary_light_index(self) -> u8 {
        self.primary_light_index
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BspDrawSurfKind {
    LitOpaque,

    LitTrans,

    Emissive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BspDrawSurfRanges {
    pub lit_opaque: (u32, u32),

    pub lit_trans: (u32, u32),

    pub emissive: (u32, u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BspDrawSurfRun<K = BspDrawSurfKind> {
    pub kind: K,
    pub first_surf: u16,
    pub surf_count: u16,

    pub draw_surf: GfxDrawSurf,

    pub setup_key_changed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BspDrawSurfCensus {
    pub range_n: u32,
    pub visible_n: u32,
    pub admitted_n: u32,
    pub run_n: u32,

    pub input_gap_n: u32,

    pub output_overflow_n: u32,
}

const BSP_KEY_LOW_MASK: u32 = 0xc0ff_0000;
const BSP_KEY_HIGH_MASK: u32 = 0x001f_e3ff;

#[must_use]
pub const fn bsp_draw_surf_setup_key(draw_surf: GfxDrawSurf) -> (u32, u32) {
    (
        draw_surf.packed as u32 & BSP_KEY_LOW_MASK,
        (draw_surf.packed >> 32) as u32 & BSP_KEY_HIGH_MASK,
    )
}

#[must_use]
pub fn bsp_draw_surf_run_continues<S: BspSurfaceDrawFields>(
    previous: S,
    previous_draw_surf: GfxDrawSurf,
    current: S,
    current_draw_surf: GfxDrawSurf,
) -> bool {
    let previous_key = bsp_draw_surf_setup_key(previous_draw_surf);
    let current_key = bsp_draw_surf_setup_key(current_draw_surf);
    current.base_index() == previous.base_index() + previous.tri_count() as u32 * 3
        && current.first_vertex() == previous.first_vertex()
        && current.lightmap_index() == previous.lightmap_index()
        && current.reflection_probe_index() == previous.reflection_probe_index()
        && current.primary_light_index() == previous.primary_light_index()
        && current_key.0 == previous_key.0
        && current_key.1 == previous_key.1
}

pub fn add_bsp_draw_surfs_camera<S: BspSurfaceDrawFields, K: Copy>(
    kind: K,
    begin: u32,
    end: u32,
    surface_vis: &[u8],
    surfaces: &[S],
    draw_surfs: &[GfxDrawSurf],
    out: &mut [BspDrawSurfRun<K>],
) -> BspDrawSurfCensus {
    let mut census = BspDrawSurfCensus {
        range_n: end.saturating_sub(begin),
        ..BspDrawSurfCensus::default()
    };
    let mut previous: Option<(u32, S, GfxDrawSurf)> = None;
    let mut run_visible_n = 0u32;
    let mut run_key_changed = false;
    let mut run_draw_surf = GfxDrawSurf::from_packed(0);

    let mut flush = |last_surf: u32,
                     count: u32,
                     key_changed: bool,
                     draw_surf: GfxDrawSurf,
                     census: &mut BspDrawSurfCensus| {
        if count == 0 {
            return;
        }
        let first = last_surf.saturating_add(1).saturating_sub(count);
        let Some(first_surf) = u16::try_from(first).ok() else {
            census.input_gap_n = census.input_gap_n.saturating_add(count);
            return;
        };
        let Some(surf_count) = u16::try_from(count).ok() else {
            census.input_gap_n = census.input_gap_n.saturating_add(count);
            return;
        };
        if u16::try_from(last_surf).is_err() {
            census.input_gap_n = census.input_gap_n.saturating_add(count);
            return;
        }
        let slot = census.run_n as usize;
        census.run_n = census.run_n.saturating_add(1);
        if let Some(dest) = out.get_mut(slot) {
            *dest = BspDrawSurfRun {
                kind,
                first_surf,
                surf_count,
                draw_surf,
                setup_key_changed: key_changed,
            };
        } else {
            census.output_overflow_n = census.output_overflow_n.saturating_add(1);
        }
    };

    for surf in begin..end {
        let i = surf as usize;
        let Some(&vis) = surface_vis.get(i) else {
            census.input_gap_n = census.input_gap_n.saturating_add(1);
            continue;
        };
        if vis == 0 {
            continue;
        }
        census.visible_n = census.visible_n.saturating_add(1);
        let (Some(&surface), Some(&draw_surf)) = (surfaces.get(i), draw_surfs.get(i)) else {
            census.input_gap_n = census.input_gap_n.saturating_add(1);
            continue;
        };
        census.admitted_n = census.admitted_n.saturating_add(1);

        if let Some((previous_surf, previous_surface, previous_draw_surf)) = previous
            && !bsp_draw_surf_run_continues(
                previous_surface,
                previous_draw_surf,
                surface,
                draw_surf,
            )
        {
            flush(
                previous_surf,
                run_visible_n,
                run_key_changed,
                run_draw_surf,
                &mut census,
            );
            run_key_changed =
                bsp_draw_surf_setup_key(previous_draw_surf) != bsp_draw_surf_setup_key(draw_surf);
            run_visible_n = 0;
        }
        if run_visible_n == 0 {
            run_draw_surf = draw_surf;
        }
        run_visible_n = run_visible_n.saturating_add(1);
        previous = Some((surf, surface, draw_surf));
    }

    if let Some((last_surf, _, _)) = previous {
        flush(
            last_surf,
            run_visible_n,
            run_key_changed,
            run_draw_surf,
            &mut census,
        );
    }
    census
}
