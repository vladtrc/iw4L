use dpvs_iw4::{
    AabbNodeView, AabbSphereBits, DpvsPlanes, aabb_tree_set_sorted_span_bits, msb_iter,
    words_for_bits,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkBoxSurfacesCensus {
    pub cell_n: u32,
    pub cell_hit: u32,
    pub trees_walked: u32,
    pub aabb_bit_n: u32,
    pub smodel_bit_n: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct MarkCellAabbTree<'a> {
    pub nodes: &'a [AabbNodeView],
    pub smodel_indexes: &'a [u16],
}

pub struct MarkBoxSurfaces<'a> {
    pub planes: DpvsPlanes<'a>,
    pub origin: [f32; 3],
    pub radius: f32,
    pub trees: &'a [MarkCellAabbTree<'a>],
    pub surf_bits: &'a mut [u32],
    pub surf_bit_count: u32,
    pub smodel_bits: Option<&'a mut [u32]>,
    pub smodel_bit_count: u32,
    pub cell_bits: &'a mut [u32],
}

pub fn fx_mark_box_surfaces(q: &mut MarkBoxSurfaces<'_>) -> MarkBoxSurfacesCensus {
    let cell_n = q.planes.cell_count;
    let mut census = MarkBoxSurfacesCensus {
        cell_n,
        ..MarkBoxSurfacesCensus::default()
    };
    if cell_n == 0 {
        return census;
    }
    for w in q.cell_bits.iter_mut() {
        *w = 0;
    }
    for w in q.surf_bits.iter_mut() {
        *w = 0;
    }
    if let Some(bits) = q.smodel_bits.as_mut() {
        for w in bits.iter_mut() {
            *w = 0;
        }
    }
    census.cell_hit = q
        .planes
        .cells_overlapping_sphere(q.origin, q.radius, q.cell_bits);
    let radius_sq = q.radius * q.radius;
    let origin = q.origin;
    let surf_bit_count = q.surf_bit_count;
    let smodel_bit_count = q.smodel_bit_count;
    for cell in msb_iter(q.cell_bits, cell_n as usize) {
        let Some(tree) = q.trees.get(cell) else {
            continue;
        };
        census.trees_walked = census.trees_walked.saturating_add(1);
        let mut span = AabbSphereBits {
            nodes: tree.nodes,
            origin,
            radius_sq,
            surf_bits: q.surf_bits,
            surf_bit_count,
            smodel_indexes: tree.smodel_indexes,
            smodel_bits: q.smodel_bits.as_mut().map(|b| &mut **b),
            smodel_bit_count,
        };
        aabb_tree_set_sorted_span_bits(&mut span);
    }
    census.aabb_bit_n = q.surf_bits.iter().map(|w| w.count_ones()).sum();
    census.smodel_bit_n = q
        .smodel_bits
        .as_ref()
        .map(|b| b.iter().map(|w| w.count_ones()).sum())
        .unwrap_or(0);
    census
}

#[inline]
pub fn fx_mark_box_surfaces_words(count: u32) -> usize {
    words_for_bits(count as usize)
}
