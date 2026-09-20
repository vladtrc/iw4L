//! Turning a finished crack graph into shards.
//!
//! Each face of the graph becomes one shard. A boundary component that is not a face
//! of its own — a detached island's outline, or a crack tree that never reached the
//! border — is attached to the face that encloses it, as a hole and as open cracks.
//! A global-to-local vertex map travels with the shard so a crack can name the
//! contour vertex it grows out of.

use crate::glass::{FX_GLASS_GEOMETRY_DATA, fx_glass_geo_vert, fx_glass_pack_geo_vert};
use crate::glass_crack::{
    FX_GLASS_CRACK_EDGE_MAX, FX_GLASS_CRACK_PT_MAX, FX_GLASS_EDGE_NONE, FX_GLASS_EDGE_SUPPORTED,
    FxGlassCrackWork,
};
use crate::glass_geo::{
    FX_GLASS_CRACK_VERT_FREE, FX_GLASS_SHARD_CRACK_MAX, FX_GLASS_SHARD_GEO_MAX,
    FX_GLASS_SHARD_HOLE_MAX, FX_GLASS_SHARD_TRI_MAX, FX_GLASS_SHARD_VERT_MAX, FxGlassGeoSpan,
    fx_glass_contour_area_x2, fx_glass_encode_fans, fx_glass_fan_word_count,
    fx_glass_pack_crack_header, fx_glass_pack_geo_count, fx_glass_pack_verts, fx_glass_triangulate,
};

pub const FX_GLASS_SHARD_MAX: usize = 32;

/// One emitted piece. Geometry is packed parent-local, ready to copy into `geoData`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxGlassShard {
    pub area_x2: f32,
    pub centroid: [f32; 2],
    pub support_mask: u32,
    pub vert_count: u8,
    pub hole_data_count: u8,
    pub crack_data_count: u8,
    pub fan_data_count: u8,
    pub geo_data: [[u8; FX_GLASS_GEOMETRY_DATA]; FX_GLASS_SHARD_GEO_MAX],
    pub geo_data_used: u16,
}

impl Default for FxGlassShard {
    fn default() -> Self {
        Self {
            area_x2: 0.0,
            centroid: [0.0, 0.0],
            support_mask: 0,
            vert_count: 0,
            hole_data_count: 0,
            crack_data_count: 0,
            fan_data_count: 0,
            geo_data: [[0; FX_GLASS_GEOMETRY_DATA]; FX_GLASS_SHARD_GEO_MAX],
            geo_data_used: 0,
        }
    }
}

impl FxGlassShard {
    pub fn geo(&self) -> &[[u8; FX_GLASS_GEOMETRY_DATA]] {
        &self.geo_data[..usize::from(self.geo_data_used)]
    }

    /// Shifts every stored vertex by `-offset`, leaving count and fan words alone.
    ///
    /// Piece geometry is stored about the piece origin, so a shard is recentred once
    /// its centroid is known and the caller moves the origin by the same amount.
    pub fn recenter(&mut self, offset: [i16; 2]) {
        let vert_n = usize::from(self.vert_count);
        let hole_words = usize::from(self.hole_data_count);
        let crack_words = usize::from(self.crack_data_count);
        for i in 0..vert_n {
            self.shift_word(i, offset);
        }
        let mut cursor = vert_n;
        let hole_end = vert_n + hole_words;
        while cursor < hole_end {
            let count = usize::from(u16::from_le_bytes([
                self.geo_data[cursor][0],
                self.geo_data[cursor][1],
            ]));
            cursor += 1;
            for i in cursor..(cursor + count).min(hole_end) {
                self.shift_word(i, offset);
            }
            cursor += count;
        }
        let crack_end = hole_end + crack_words;
        while cursor < crack_end {
            let count = usize::from(u16::from_le_bytes([
                self.geo_data[cursor][0],
                self.geo_data[cursor][1],
            ]));
            cursor += 1;
            for i in cursor..(cursor + count).min(crack_end) {
                self.shift_word(i, offset);
            }
            cursor += count;
        }
        self.centroid[0] -= f32::from(offset[0]);
        self.centroid[1] -= f32::from(offset[1]);
    }

    fn shift_word(&mut self, i: usize, offset: [i16; 2]) {
        let v = fx_glass_geo_vert(&self.geo_data[i]);
        let x = v[0].saturating_sub(offset[0]);
        let y = v[1].saturating_sub(offset[1]);
        self.geo_data[i] = fx_glass_pack_geo_vert(x, y);
    }
}

/// One boundary component of the graph, after out-and-back crack spurs are removed.
#[derive(Clone, Copy)]
struct Component {
    contour_start: u16,
    contour_len: u16,
    spur_start: u16,
    spur_len: u16,
    area_x2: f32,
    /// Index of the face this component belongs to, or `u8::MAX` when it is a face.
    parent: u8,
}

struct Extract {
    contour: [u16; FX_GLASS_CRACK_EDGE_MAX],
    contour_n: u16,
    spurs: [u16; FX_GLASS_CRACK_EDGE_MAX],
    spurs_n: u16,
    comps: [Component; FX_GLASS_SHARD_MAX],
    comp_n: u8,
}

/// Splits every loop into its simple contour and its crack spurs, then decides which
/// components are faces and which are enclosed by one.
fn split_components(work: &FxGlassCrackWork) -> Extract {
    let mut ex = Extract {
        contour: [0; FX_GLASS_CRACK_EDGE_MAX],
        contour_n: 0,
        spurs: [0; FX_GLASS_CRACK_EDGE_MAX],
        spurs_n: 0,
        comps: [Component {
            contour_start: 0,
            contour_len: 0,
            spur_start: 0,
            spur_len: 0,
            area_x2: 0.0,
            parent: u8::MAX,
        }; FX_GLASS_SHARD_MAX],
        comp_n: 0,
    };
    let mut stack = [0u16; FX_GLASS_CRACK_EDGE_MAX];
    for li in 0..work.loop_count.min(FX_GLASS_SHARD_MAX as u8) {
        let first = work.loop_start_edge(li);
        if first == FX_GLASS_EDGE_NONE {
            continue;
        }
        let spur_start = ex.spurs_n;
        let mut sp = 0usize;
        let mut e = first;
        loop {
            let row = work.edges[usize::from(e)];
            if sp > 0 && work.edges[usize::from(stack[sp - 1])].twin == e {
                sp -= 1;
                ex.spurs[usize::from(ex.spurs_n)] = stack[sp];
                ex.spurs_n += 1;
            } else {
                stack[sp] = e;
                sp += 1;
            }
            e = row.next;
            if e == first || e == FX_GLASS_EDGE_NONE {
                break;
            }
        }
        // The chain is circular, so the seam can cancel too.
        let mut lo = 0usize;
        while sp - lo >= 2 && work.edges[usize::from(stack[sp - 1])].twin == stack[lo] {
            ex.spurs[usize::from(ex.spurs_n)] = stack[sp - 1];
            ex.spurs_n += 1;
            sp -= 1;
            lo += 1;
        }
        let contour_start = ex.contour_n;
        let mut area = 0.0f32;
        for e in stack[lo..sp].iter().copied() {
            ex.contour[usize::from(ex.contour_n)] = e;
            ex.contour_n += 1;
            let row = work.edges[usize::from(e)];
            let a = work.pts[usize::from(row.i0)];
            let b = work.pts[usize::from(row.i1)];
            area += a[0] * b[1] - b[0] * a[1];
        }
        ex.comps[usize::from(ex.comp_n)] = Component {
            contour_start,
            contour_len: ex.contour_n - contour_start,
            spur_start,
            spur_len: ex.spurs_n - spur_start,
            area_x2: area,
            parent: u8::MAX,
        };
        ex.comp_n += 1;
    }
    assign_parents(work, &mut ex);
    ex
}

fn assign_parents(work: &FxGlassCrackWork, ex: &mut Extract) {
    for ci in 0..usize::from(ex.comp_n) {
        let comp = ex.comps[ci];
        if comp.area_x2 > 0.0 {
            continue;
        }
        let Some(probe) = component_probe(work, ex, &comp) else {
            continue;
        };
        let mut best: Option<(u8, f32)> = None;
        for pi in 0..usize::from(ex.comp_n) {
            if pi == ci {
                continue;
            }
            let parent = ex.comps[pi];
            if parent.area_x2 <= 0.0 {
                continue;
            }
            if !contour_contains(work, ex, &parent, probe) {
                continue;
            }
            if best.is_none_or(|(_, a)| parent.area_x2 < a) {
                best = Some((pi as u8, parent.area_x2));
            }
        }
        if let Some((pi, _)) = best {
            ex.comps[ci].parent = pi;
        }
    }
}

/// Nudge off the component's own boundary, smaller than the packed vertex grid so it
/// cannot cross anything else.
const PROBE_OFFSET: f32 = 0.01;

/// A point strictly inside the enclosing face.
///
/// The face of a half-edge lies to its left, so stepping off the midpoint of one of
/// the component's edges along that side lands in the parent and on no contour at all.
/// Taking the midpoint itself would land on the shared boundary whenever two
/// components are the two sides of the same crack, and the containment test there
/// answers by rounding rather than by geometry.
fn component_probe(work: &FxGlassCrackWork, ex: &Extract, comp: &Component) -> Option<[f32; 2]> {
    let e = if comp.contour_len > 0 {
        ex.contour[usize::from(comp.contour_start)]
    } else if comp.spur_len > 0 {
        ex.spurs[usize::from(comp.spur_start)]
    } else {
        return None;
    };
    let row = work.edges[usize::from(e)];
    let a = work.pts[usize::from(row.i0)];
    let b = work.pts[usize::from(row.i1)];
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = libm::sqrtf(d[0] * d[0] + d[1] * d[1]);
    if len <= 1e-6 {
        return None;
    }
    Some([
        (a[0] + b[0]) * 0.5 - d[1] / len * PROBE_OFFSET,
        (a[1] + b[1]) * 0.5 + d[0] / len * PROBE_OFFSET,
    ])
}

fn contour_contains(work: &FxGlassCrackWork, ex: &Extract, comp: &Component, p: [f32; 2]) -> bool {
    let mut inside = false;
    for k in 0..usize::from(comp.contour_len) {
        let e = ex.contour[usize::from(comp.contour_start) + k];
        let row = work.edges[usize::from(e)];
        let a = work.pts[usize::from(row.i0)];
        let b = work.pts[usize::from(row.i1)];
        if (a[1] > p[1]) == (b[1] > p[1]) {
            continue;
        }
        let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
        if p[0] < x {
            inside = !inside;
        }
    }
    inside
}

/// Builds one shard per face of `work`. Returns how many were written.
pub fn fx_glass_extract_shards(work: &FxGlassCrackWork, out: &mut [FxGlassShard]) -> usize {
    let ex = split_components(work);
    let mut written = 0usize;
    for ci in 0..usize::from(ex.comp_n) {
        if ex.comps[ci].area_x2 <= 0.0 || written >= out.len() {
            continue;
        }
        if build_shard(work, &ex, ci as u8, &mut out[written]) {
            written += 1;
        }
    }
    written
}

struct Builder {
    verts: [[i16; 2]; FX_GLASS_SHARD_VERT_MAX],
    vert_n: usize,
    local_of: [u8; FX_GLASS_CRACK_PT_MAX],
    holes: [FxGlassGeoSpan; FX_GLASS_SHARD_HOLE_MAX],
    hole_n: usize,
}

impl Builder {
    fn push(&mut self, work: &FxGlassCrackWork, global: u8) -> Option<u8> {
        if self.vert_n >= FX_GLASS_SHARD_VERT_MAX {
            return None;
        }
        let local = self.vert_n as u8;
        self.verts[self.vert_n] = work.packed_pts[usize::from(global)];
        self.local_of[usize::from(global)] = local;
        self.vert_n += 1;
        Some(local)
    }
}

fn build_shard(work: &FxGlassCrackWork, ex: &Extract, ci: u8, shard: &mut FxGlassShard) -> bool {
    let comp = ex.comps[usize::from(ci)];
    let outer_n = usize::from(comp.contour_len);
    if !(3..=FX_GLASS_SHARD_VERT_MAX).contains(&outer_n) {
        return false;
    }
    *shard = FxGlassShard::default();
    let mut b = Builder {
        verts: [[0; 2]; FX_GLASS_SHARD_VERT_MAX],
        vert_n: 0,
        local_of: [FX_GLASS_CRACK_VERT_FREE; FX_GLASS_CRACK_PT_MAX],
        holes: [FxGlassGeoSpan { start: 0, count: 0 }; FX_GLASS_SHARD_HOLE_MAX],
        hole_n: 0,
    };
    // Outer contour, and the support bits it inherits. Only a supported pane edge
    // carries that class, so a crack never fabricates support.
    let mut support_mask = 0u32;
    for k in 0..outer_n {
        let e = ex.contour[usize::from(comp.contour_start) + k];
        let row = work.edges[usize::from(e)];
        if b.push(work, row.i0).is_none() {
            return false;
        }
        if k < 32 && row.kind == FX_GLASS_EDGE_SUPPORTED {
            support_mask |= 0x8000_0000u32 >> k;
        }
    }

    // Enclosed components with area become holes; the rest only contribute cracks.
    for hi in 0..usize::from(ex.comp_n) {
        let hole = ex.comps[hi];
        if hole.parent != ci || hole.contour_len < 3 {
            continue;
        }
        if b.hole_n >= FX_GLASS_SHARD_HOLE_MAX {
            break;
        }
        let start = b.vert_n;
        for k in 0..usize::from(hole.contour_len) {
            let e = ex.contour[usize::from(hole.contour_start) + k];
            let row = work.edges[usize::from(e)];
            if b.push(work, row.i0).is_none() {
                return false;
            }
        }
        b.holes[b.hole_n] = FxGlassGeoSpan {
            start: start as u8,
            count: hole.contour_len as u8,
        };
        b.hole_n += 1;
    }
    let border_vert_n = b.vert_n;

    // Triangulate before the cracks add vertices, so triangle indices only ever name
    // border vertices and fit the byte-packed fan words.
    let mut tris = [[0u8; 3]; FX_GLASS_SHARD_TRI_MAX];
    let Some(tri_n) =
        fx_glass_triangulate(&b.verts[..border_vert_n], &b.holes[..b.hole_n], &mut tris)
    else {
        return false;
    };

    // Crack chains, own spurs first so branch roots are mapped before their branches.
    let mut crack_words = 0usize;
    let mut crack_headers = [([0u8; FX_GLASS_GEOMETRY_DATA], 0u8, 0u8); FX_GLASS_SHARD_CRACK_MAX];
    let mut crack_header_n = 0usize;
    let mut consumed = [false; FX_GLASS_CRACK_EDGE_MAX];
    let mut crack_verts = [[0i16; 2]; FX_GLASS_SHARD_VERT_MAX];
    let mut crack_vert_n = 0usize;
    let mut sources = [(0u16, 0u16); FX_GLASS_SHARD_MAX + 1];
    let mut source_n = 0usize;
    sources[source_n] = (comp.spur_start, comp.spur_len);
    source_n += 1;
    for hi in 0..usize::from(ex.comp_n) {
        let child = ex.comps[hi];
        if child.parent != ci || source_n >= sources.len() {
            continue;
        }
        sources[source_n] = (child.spur_start, child.spur_len);
        source_n += 1;
        if child.contour_len > 0 && child.contour_len < 3 {
            // A degenerate island contour is only a crack, not a hole.
            sources[source_n - 1] = (child.contour_start, child.contour_len);
        }
    }

    for pass in 0..2 {
        for si in 0..source_n {
            let (start, len) = sources[si];
            for k in 0..usize::from(len) {
                let idx = usize::from(start) + k;
                let e = if si == 0 || sources[si].0 != comp.contour_start {
                    ex.spurs[idx]
                } else {
                    ex.contour[idx]
                };
                if consumed[usize::from(e)] {
                    continue;
                }
                let row = work.edges[usize::from(e)];
                let rooted = b.local_of[usize::from(row.i0)] != FX_GLASS_CRACK_VERT_FREE;
                if pass == 0 && !rooted {
                    continue;
                }
                let begin_vert = b.local_of[usize::from(row.i0)];
                let chain_start = crack_vert_n;
                let mut tip = e;
                let mut end_vert = FX_GLASS_CRACK_VERT_FREE;
                loop {
                    consumed[usize::from(tip)] = true;
                    let tip_row = work.edges[usize::from(tip)];
                    let known = b.local_of[usize::from(tip_row.i1)];
                    if known != FX_GLASS_CRACK_VERT_FREE {
                        end_vert = known;
                        break;
                    }
                    if crack_vert_n >= FX_GLASS_SHARD_VERT_MAX - border_vert_n {
                        break;
                    }
                    crack_verts[crack_vert_n] = work.packed_pts[usize::from(tip_row.i1)];
                    let local = border_vert_n + crack_vert_n;
                    if local > usize::from(FX_GLASS_CRACK_VERT_FREE) {
                        break;
                    }
                    b.local_of[usize::from(tip_row.i1)] = local as u8;
                    crack_vert_n += 1;
                    let Some(next) =
                        next_spur(work, ex, &sources[..source_n], &consumed, comp, tip_row.i1)
                    else {
                        break;
                    };
                    tip = next;
                }
                let unique = (crack_vert_n - chain_start) as u16;
                if unique == 0 && end_vert == FX_GLASS_CRACK_VERT_FREE {
                    continue;
                }
                if crack_header_n >= crack_headers.len() {
                    break;
                }
                crack_headers[crack_header_n] = (
                    fx_glass_pack_crack_header(unique, begin_vert, end_vert),
                    chain_start as u8,
                    unique as u8,
                );
                crack_header_n += 1;
                crack_words += 1 + usize::from(unique);
            }
        }
    }

    // Lay the geometry run out: verts, holes, cracks, fans.
    let hole_words: usize = b.holes[..b.hole_n]
        .iter()
        .map(|h| 1 + usize::from(h.count))
        .sum();
    let fan_words = fx_glass_fan_word_count(tri_n);
    let total = outer_n + hole_words + crack_words + fan_words;
    if total > FX_GLASS_SHARD_GEO_MAX
        || hole_words > 255
        || crack_words > 255
        || fan_words > 255
        || outer_n > 255
    {
        return false;
    }
    let mut w = 0usize;
    if fx_glass_pack_verts(&b.verts[..outer_n], &mut shard.geo_data[w..]).is_none() {
        return false;
    }
    w += outer_n;
    for hole in &b.holes[..b.hole_n] {
        shard.geo_data[w] = fx_glass_pack_geo_count(u16::from(hole.count));
        w += 1;
        let start = usize::from(hole.start);
        let count = usize::from(hole.count);
        if fx_glass_pack_verts(&b.verts[start..start + count], &mut shard.geo_data[w..]).is_none() {
            return false;
        }
        w += count;
    }
    for (header, start, count) in &crack_headers[..crack_header_n] {
        shard.geo_data[w] = *header;
        w += 1;
        let start = usize::from(*start);
        let count = usize::from(*count);
        if fx_glass_pack_verts(&crack_verts[start..start + count], &mut shard.geo_data[w..])
            .is_none()
        {
            return false;
        }
        w += count;
    }
    if fx_glass_encode_fans(&tris[..tri_n], &mut shard.geo_data[w..]).is_none() {
        return false;
    }
    w += fan_words;

    let mut area = 0.0f32;
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    for k in 0..outer_n {
        let a = b.verts[k];
        let c = b.verts[(k + 1) % outer_n];
        let ax = f32::from(a[0]);
        let ay = f32::from(a[1]);
        let cxx = f32::from(c[0]);
        let cyy = f32::from(c[1]);
        let w2 = ax * cyy - cxx * ay;
        area += w2;
        cx += (ax + cxx) * w2;
        cy += (ay + cyy) * w2;
    }
    shard.centroid = if area.abs() > 1e-4 {
        [cx / (3.0 * area), cy / (3.0 * area)]
    } else {
        [0.0, 0.0]
    };
    // Glass a shard does not have cannot hold it up or weigh it down, so a hole comes
    // straight off the area the size and fringe rules are decided on.
    for hi in 0..b.hole_n {
        let span = b.holes[hi];
        let start = usize::from(span.start);
        let ring = &b.verts[start..start + usize::from(span.count)];
        area -= fx_glass_contour_area_x2(ring).unsigned_abs() as f32;
    }
    shard.area_x2 = area;
    shard.support_mask = support_mask;
    shard.vert_count = outer_n as u8;
    shard.hole_data_count = hole_words as u8;
    shard.crack_data_count = crack_words as u8;
    shard.fan_data_count = fan_words as u8;
    shard.geo_data_used = w as u16;
    true
}

/// The next unconsumed crack edge leaving `v`, staying inside this shard's sources.
fn next_spur(
    work: &FxGlassCrackWork,
    ex: &Extract,
    sources: &[(u16, u16)],
    consumed: &[bool; FX_GLASS_CRACK_EDGE_MAX],
    comp: Component,
    v: u8,
) -> Option<u16> {
    for (si, (start, len)) in sources.iter().enumerate() {
        for k in 0..usize::from(*len) {
            let idx = usize::from(*start) + k;
            let e = if si == 0 || *start != comp.contour_start {
                ex.spurs[idx]
            } else {
                ex.contour[idx]
            };
            if consumed[usize::from(e)] {
                continue;
            }
            if work.edges[usize::from(e)].i0 == v {
                return Some(e);
            }
        }
    }
    None
}
