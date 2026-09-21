//! Shard geometry data: the packed `geoData` layout, triangulation and containment.
//!
//! A piece state names four counts — vertex, hole, crack and fan words — and the
//! piece's `geoData` run holds them back to back in that order:
//!
//! * `vertCount` words: the outer contour, one packed `[i16; 2]` per word.
//! * `holeDataCount` words: per hole a count word followed by that many vertex words.
//! * `crackDataCount` words: per crack an `FxGlassCrackHeader` word
//!   (`uniqueVertCount` u16, `beginVertIndex` u8, `endVertIndex` u8) followed by
//!   `uniqueVertCount` vertex words. `0xff` in either end index means the crack tip is
//!   free rather than anchored on an existing vertex.
//! * `fanDataCount` words: the triangulation, three vertex indices per triangle packed
//!   one byte each, four bytes to a word, zero padded. Indices address the outer
//!   contour first and then each hole's vertices in the order the holes appear, which
//!   is the same order [`fx_glass_decode_geo`] rebuilds them in, so the triangle count
//!   is implied by the contour sizes and needs no stored count.
//!
//! Cracked shards are concave and may enclose detached cracked islands, so neither
//! the tessellator nor repeated-hit testing can treat a piece as a convex fan from
//! vertex zero; both read the stored triangulation and holes from here.

use crate::glass::{
    FX_GLASS_GEOMETRY_DATA, FX_GLASS_PIECE_STATE, FX_GLASS_STATE_CRACK_DATA_COUNT,
    FX_GLASS_STATE_FAN_DATA_COUNT, FX_GLASS_STATE_HOLE_DATA_COUNT, fx_glass_geo_vert,
    fx_glass_pack_geo_vert, fx_glass_state_geo_start, fx_glass_state_vert_count,
};

/// `FxGlassShard::geoData` is 231 words, so no piece can exceed that.
pub const FX_GLASS_SHARD_GEO_MAX: usize = 231;
/// Outer contour plus every hole contour.
pub const FX_GLASS_SHARD_VERT_MAX: usize = 96;
pub const FX_GLASS_SHARD_HOLE_MAX: usize = 8;
pub const FX_GLASS_SHARD_CRACK_MAX: usize = 32;
/// A polygon with `v` ring vertices and `h` holes triangulates into `v + 2h - 2` faces.
pub const FX_GLASS_SHARD_TRI_MAX: usize = FX_GLASS_SHARD_VERT_MAX + 2 * FX_GLASS_SHARD_HOLE_MAX;
/// Bridging a hole duplicates two ring vertices.
const RING_MAX: usize = FX_GLASS_SHARD_VERT_MAX + 2 * FX_GLASS_SHARD_HOLE_MAX;

/// A crack tip that is not anchored on a contour vertex.
pub const FX_GLASS_CRACK_VERT_FREE: u8 = 0xff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxGlassGeoSpan {
    pub start: u8,
    pub count: u8,
}

/// One decoded open crack: a polyline that starts and ends either on a contour vertex
/// or in free space inside the piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxGlassGeoCrack {
    pub begin_vert: u8,
    pub end_vert: u8,
    pub span: FxGlassGeoSpan,
}

/// Fully decoded piece geometry in packed parent-local units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxGlassPieceGeo {
    /// Outer contour first, then each hole's contour, then each crack's own vertices.
    pub verts: [[i16; 2]; FX_GLASS_SHARD_VERT_MAX],
    pub vert_n: usize,
    pub holes: [FxGlassGeoSpan; FX_GLASS_SHARD_HOLE_MAX],
    pub hole_n: usize,
    pub cracks: [FxGlassGeoCrack; FX_GLASS_SHARD_CRACK_MAX],
    pub crack_n: usize,
    pub tris: [[u8; 3]; FX_GLASS_SHARD_TRI_MAX],
    pub tri_n: usize,
    /// Total vertices addressed by `tris`: the outer contour plus every hole contour.
    pub border_vert_n: usize,
}

impl Default for FxGlassPieceGeo {
    fn default() -> Self {
        Self {
            verts: [[0; 2]; FX_GLASS_SHARD_VERT_MAX],
            vert_n: 0,
            holes: [FxGlassGeoSpan { start: 0, count: 0 }; FX_GLASS_SHARD_HOLE_MAX],
            hole_n: 0,
            cracks: [FxGlassGeoCrack {
                begin_vert: FX_GLASS_CRACK_VERT_FREE,
                end_vert: FX_GLASS_CRACK_VERT_FREE,
                span: FxGlassGeoSpan { start: 0, count: 0 },
            }; FX_GLASS_SHARD_CRACK_MAX],
            crack_n: 0,
            tris: [[0; 3]; FX_GLASS_SHARD_TRI_MAX],
            tri_n: 0,
            border_vert_n: 0,
        }
    }
}

impl FxGlassPieceGeo {
    pub fn outer(&self) -> &[[i16; 2]] {
        &self.verts[..self.vert_n]
    }

    pub fn span(&self, span: FxGlassGeoSpan) -> &[[i16; 2]] {
        let start = usize::from(span.start);
        &self.verts[start..start + usize::from(span.count)]
    }

    pub fn border_verts(&self) -> &[[i16; 2]] {
        &self.verts[..self.border_vert_n]
    }

    pub fn triangles(&self) -> &[[u8; 3]] {
        &self.tris[..self.tri_n]
    }

    pub fn holes(&self) -> &[FxGlassGeoSpan] {
        &self.holes[..self.hole_n]
    }

    pub fn cracks(&self) -> &[FxGlassGeoCrack] {
        &self.cracks[..self.crack_n]
    }
}

pub fn fx_glass_pack_geo_count(count: u16) -> [u8; FX_GLASS_GEOMETRY_DATA] {
    let c = count.to_le_bytes();
    [c[0], c[1], 0, 0]
}

pub fn fx_glass_geo_count(word: &[u8; FX_GLASS_GEOMETRY_DATA]) -> u16 {
    u16::from_le_bytes([word[0], word[1]])
}

pub fn fx_glass_pack_crack_header(
    unique_vert_count: u16,
    begin_vert: u8,
    end_vert: u8,
) -> [u8; FX_GLASS_GEOMETRY_DATA] {
    let c = unique_vert_count.to_le_bytes();
    [c[0], c[1], begin_vert, end_vert]
}

/// Words needed to hold `tri_n` packed triangles.
pub fn fx_glass_fan_word_count(tri_n: usize) -> usize {
    (tri_n * 3).div_ceil(FX_GLASS_GEOMETRY_DATA)
}

/// Triangles a contour of `border_vert_n` vertices with `hole_n` holes produces.
pub fn fx_glass_tri_count(border_vert_n: usize, hole_n: usize) -> usize {
    (border_vert_n + 2 * hole_n).saturating_sub(2)
}

/// Decodes one piece's geometry run. Pieces written before the crack port carry zero
/// hole, crack and fan counts and decode as a convex fan, so old data still renders.
pub fn fx_glass_decode_geo(
    state: &[u8; FX_GLASS_PIECE_STATE],
    geo: &[[u8; FX_GLASS_GEOMETRY_DATA]],
) -> Option<FxGlassPieceGeo> {
    let vert_n = usize::from(fx_glass_state_vert_count(state));
    let hole_words = usize::from(state[FX_GLASS_STATE_HOLE_DATA_COUNT]);
    let crack_words = usize::from(state[FX_GLASS_STATE_CRACK_DATA_COUNT]);
    let fan_words = usize::from(state[FX_GLASS_STATE_FAN_DATA_COUNT]);
    if !(3..=FX_GLASS_SHARD_VERT_MAX).contains(&vert_n) {
        return None;
    }
    let start = usize::from(fx_glass_state_geo_start(state));
    let total = vert_n
        .checked_add(hole_words)?
        .checked_add(crack_words)?
        .checked_add(fan_words)?;
    let words = geo.get(start..start.checked_add(total)?)?;

    let mut out = FxGlassPieceGeo::default();
    for (i, word) in words[..vert_n].iter().enumerate() {
        out.verts[i] = fx_glass_geo_vert(word);
    }
    out.vert_n = vert_n;
    let mut used = vert_n;

    let mut cursor = vert_n;
    let hole_end = vert_n + hole_words;
    while cursor < hole_end {
        let count = usize::from(fx_glass_geo_count(&words[cursor]));
        cursor += 1;
        if count < 3 || cursor + count > hole_end || out.hole_n >= FX_GLASS_SHARD_HOLE_MAX {
            return None;
        }
        if used + count > FX_GLASS_SHARD_VERT_MAX {
            return None;
        }
        for k in 0..count {
            out.verts[used + k] = fx_glass_geo_vert(&words[cursor + k]);
        }
        out.holes[out.hole_n] = FxGlassGeoSpan {
            start: used as u8,
            count: count as u8,
        };
        out.hole_n += 1;
        used += count;
        cursor += count;
    }
    out.border_vert_n = used;

    let crack_end = hole_end + crack_words;
    while cursor < crack_end {
        let header = &words[cursor];
        let count = usize::from(fx_glass_geo_count(header));
        let begin_vert = header[2];
        let end_vert = header[3];
        cursor += 1;
        if cursor + count > crack_end || out.crack_n >= FX_GLASS_SHARD_CRACK_MAX {
            return None;
        }
        if used + count > FX_GLASS_SHARD_VERT_MAX {
            return None;
        }
        for k in 0..count {
            out.verts[used + k] = fx_glass_geo_vert(&words[cursor + k]);
        }
        out.cracks[out.crack_n] = FxGlassGeoCrack {
            begin_vert,
            end_vert,
            span: FxGlassGeoSpan {
                start: used as u8,
                count: count as u8,
            },
        };
        out.crack_n += 1;
        used += count;
        cursor += count;
    }
    out.vert_n = vert_n;

    let want_tris = fx_glass_tri_count(out.border_vert_n, out.hole_n);
    if fan_words == 0 {
        // No stored triangulation: rebuild one. A pre-crack piece is convex, so the
        // result is the same fan a convex piece always had.
        let n = fx_glass_triangulate(
            &out.verts[..out.border_vert_n],
            &out.holes[..out.hole_n],
            &mut out.tris,
        )?;
        out.tri_n = n;
        return Some(out);
    }
    if fan_words != fx_glass_fan_word_count(want_tris) || want_tris > FX_GLASS_SHARD_TRI_MAX {
        return None;
    }
    let fans = &words[crack_end..crack_end + fan_words];
    for t in 0..want_tris {
        let mut tri = [0u8; 3];
        for (k, slot) in tri.iter_mut().enumerate() {
            let b = t * 3 + k;
            *slot = fans[b / FX_GLASS_GEOMETRY_DATA][b % FX_GLASS_GEOMETRY_DATA];
            if usize::from(*slot) >= out.border_vert_n {
                return None;
            }
        }
        out.tris[t] = tri;
    }
    out.tri_n = want_tris;
    Some(out)
}

/// Packs a triangulation into `out`, returning the word count.
pub fn fx_glass_encode_fans(
    tris: &[[u8; 3]],
    out: &mut [[u8; FX_GLASS_GEOMETRY_DATA]],
) -> Option<usize> {
    let words = fx_glass_fan_word_count(tris.len());
    if out.len() < words {
        return None;
    }
    for word in out[..words].iter_mut() {
        *word = [0; FX_GLASS_GEOMETRY_DATA];
    }
    for (t, tri) in tris.iter().enumerate() {
        for (k, index) in tri.iter().enumerate() {
            let b = t * 3 + k;
            out[b / FX_GLASS_GEOMETRY_DATA][b % FX_GLASS_GEOMETRY_DATA] = *index;
        }
    }
    Some(words)
}

pub fn fx_glass_pack_verts(
    verts: &[[i16; 2]],
    out: &mut [[u8; FX_GLASS_GEOMETRY_DATA]],
) -> Option<usize> {
    if out.len() < verts.len() {
        return None;
    }
    for (dst, v) in out.iter_mut().zip(verts.iter()) {
        *dst = fx_glass_pack_geo_vert(v[0], v[1]);
    }
    Some(verts.len())
}

fn cross(o: [i16; 2], a: [i16; 2], b: [i16; 2]) -> i32 {
    let ax = i32::from(a[0]) - i32::from(o[0]);
    let ay = i32::from(a[1]) - i32::from(o[1]);
    let bx = i32::from(b[0]) - i32::from(o[0]);
    let by = i32::from(b[1]) - i32::from(o[1]);
    ax * by - ay * bx
}

/// Twice the signed area of a closed contour; positive when wound counter-clockwise.
pub fn fx_glass_contour_area_x2(verts: &[[i16; 2]]) -> i32 {
    let mut acc = 0i32;
    for i in 0..verts.len() {
        let a = verts[i];
        let b = verts[(i + 1) % verts.len()];
        acc += i32::from(a[0]) * i32::from(b[1]) - i32::from(b[0]) * i32::from(a[1]);
    }
    acc
}

fn on_segment(a: [i16; 2], b: [i16; 2], p: [i16; 2]) -> bool {
    cross(a, b, p) == 0
        && p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
}

/// True when the open segments `a-b` and `c-d` cross. Shared endpoints do not count.
fn segments_cross(a: [i16; 2], b: [i16; 2], c: [i16; 2], d: [i16; 2]) -> bool {
    if a == c || a == d || b == c || b == d {
        return false;
    }
    let d1 = cross(a, b, c).signum();
    let d2 = cross(a, b, d).signum();
    let d3 = cross(c, d, a).signum();
    let d4 = cross(c, d, b).signum();
    if d1 != d2 && d3 != d4 {
        return true;
    }
    (d1 == 0 && on_segment(a, b, c))
        || (d2 == 0 && on_segment(a, b, d))
        || (d3 == 0 && on_segment(c, d, a))
        || (d4 == 0 && on_segment(c, d, b))
}

/// Crossing-number containment against one closed contour.
pub fn fx_glass_point_in_contour(verts: &[[i16; 2]], p: [f32; 2]) -> bool {
    let mut inside = false;
    let n = verts.len();
    for i in 0..n {
        let a = verts[i];
        let b = verts[(i + 1) % n];
        let ay = f32::from(a[1]);
        let by = f32::from(b[1]);
        if (ay > p[1]) == (by > p[1]) {
            continue;
        }
        let ax = f32::from(a[0]);
        let bx = f32::from(b[0]);
        let x = ax + (p[1] - ay) / (by - ay) * (bx - ax);
        if p[0] < x {
            inside = !inside;
        }
    }
    inside
}

/// Containment for a whole piece: inside the outer contour and outside every hole.
/// Pulls an impact point onto the piece.
///
/// A point already inside the outer contour and outside every hole is left alone;
/// anything else moves to the nearest point on a border, so a graze at the edge of a
/// shard still seeds its cracks on the shard instead of in empty space.
pub fn fx_glass_clamp_to_piece(geo: &FxGlassPieceGeo, p: [f32; 2]) -> [f32; 2] {
    if fx_glass_point_in_piece(geo, p) {
        return p;
    }
    let mut best = p;
    let mut best_d2 = f32::INFINITY;
    let mut consider = |contour: &[[i16; 2]]| {
        for i in 0..contour.len() {
            let a = [f32::from(contour[i][0]), f32::from(contour[i][1])];
            let b = contour[(i + 1) % contour.len()];
            let b = [f32::from(b[0]), f32::from(b[1])];
            let ab = [b[0] - a[0], b[1] - a[1]];
            let ab2 = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if ab2 <= 1e-8 {
                0.0
            } else {
                (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / ab2).clamp(0.0, 1.0)
            };
            let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
            let d2 = (q[0] - p[0]) * (q[0] - p[0]) + (q[1] - p[1]) * (q[1] - p[1]);
            if d2 < best_d2 {
                best_d2 = d2;
                best = q;
            }
        }
    };
    consider(geo.outer());
    for hole in geo.holes() {
        consider(geo.span(*hole));
    }
    best
}

pub fn fx_glass_point_in_piece(geo: &FxGlassPieceGeo, p: [f32; 2]) -> bool {
    if !fx_glass_point_in_contour(geo.outer(), p) {
        return false;
    }
    for hole in geo.holes() {
        if fx_glass_point_in_contour(geo.span(*hole), p) {
            return false;
        }
    }
    true
}

fn ring_reverse(ring: &mut [u8]) {
    ring.reverse();
}

/// Ear-clips a contour with holes. `verts` holds the outer contour followed by each
/// hole's contour; `holes` names the hole spans. Returns the triangle count.
pub fn fx_glass_triangulate(
    verts: &[[i16; 2]],
    holes: &[FxGlassGeoSpan],
    out: &mut [[u8; 3]],
) -> Option<usize> {
    let outer_n = verts.len() - holes.iter().map(|h| usize::from(h.count)).sum::<usize>();
    if !(3..=FX_GLASS_SHARD_VERT_MAX).contains(&verts.len()) || outer_n < 3 {
        return None;
    }
    let mut ring = [0u8; RING_MAX];
    let mut ring_n = outer_n;
    for (i, slot) in ring[..outer_n].iter_mut().enumerate() {
        *slot = i as u8;
    }
    if fx_glass_contour_area_x2(&verts[..outer_n]) < 0 {
        ring_reverse(&mut ring[..outer_n]);
    }
    for hole in holes {
        ring_n = bridge_hole(verts, &mut ring, ring_n, *hole)?;
    }
    ear_clip(verts, &mut ring[..ring_n], out)
}

/// Joins one hole into the ring with a bridge, duplicating the two bridged vertices.
fn bridge_hole(
    verts: &[[i16; 2]],
    ring: &mut [u8; RING_MAX],
    ring_n: usize,
    hole: FxGlassGeoSpan,
) -> Option<usize> {
    let start = usize::from(hole.start);
    let count = usize::from(hole.count);
    if count < 3 || ring_n + count + 2 > RING_MAX {
        return None;
    }
    let mut hole_ring = [0u8; FX_GLASS_SHARD_VERT_MAX];
    for (k, slot) in hole_ring[..count].iter_mut().enumerate() {
        *slot = (start + k) as u8;
    }
    // A hole must run opposite to the outer contour so the bridged ring stays simple.
    let mut area = 0i32;
    for k in 0..count {
        let a = verts[start + k];
        let b = verts[start + (k + 1) % count];
        area += i32::from(a[0]) * i32::from(b[1]) - i32::from(b[0]) * i32::from(a[1]);
    }
    if area > 0 {
        ring_reverse(&mut hole_ring[..count]);
    }
    // Bridge from the hole's rightmost vertex to the nearest ring vertex it can see.
    let mut hk = 0usize;
    for k in 1..count {
        if verts[usize::from(hole_ring[k])] > verts[usize::from(hole_ring[hk])] {
            hk = k;
        }
    }
    let hv = hole_ring[hk];
    let hp = verts[usize::from(hv)];
    let mut order = [0u8; RING_MAX];
    for (i, slot) in order[..ring_n].iter_mut().enumerate() {
        *slot = i as u8;
    }
    let key = |i: u8| -> i32 {
        let p = verts[usize::from(ring[usize::from(i)])];
        let dx = i32::from(p[0]) - i32::from(hp[0]);
        let dy = i32::from(p[1]) - i32::from(hp[1]);
        dx * dx + dy * dy
    };
    order[..ring_n].sort_unstable_by_key(|i| key(*i));

    let mut chosen = None;
    for slot in &order[..ring_n] {
        let ri = usize::from(*slot);
        let rv = ring[ri];
        let rp = verts[usize::from(rv)];
        if rp == hp {
            continue;
        }
        let mut blocked = false;
        for i in 0..ring_n {
            let a = verts[usize::from(ring[i])];
            let b = verts[usize::from(ring[(i + 1) % ring_n])];
            if segments_cross(hp, rp, a, b) {
                blocked = true;
                break;
            }
        }
        if !blocked {
            for k in 0..count {
                let a = verts[usize::from(hole_ring[k])];
                let b = verts[usize::from(hole_ring[(k + 1) % count])];
                if segments_cross(hp, rp, a, b) {
                    blocked = true;
                    break;
                }
            }
        }
        if !blocked {
            chosen = Some(ri);
            break;
        }
    }
    let ri = chosen?;

    let mut merged = [0u8; RING_MAX];
    let mut n = 0usize;
    for slot in &ring[..=ri] {
        merged[n] = *slot;
        n += 1;
    }
    for k in 0..count {
        merged[n] = hole_ring[(hk + k) % count];
        n += 1;
    }
    merged[n] = hv;
    n += 1;
    for slot in &ring[ri..ring_n] {
        merged[n] = *slot;
        n += 1;
    }
    ring[..n].copy_from_slice(&merged[..n]);
    Some(n)
}

fn point_in_tri(a: [i16; 2], b: [i16; 2], c: [i16; 2], p: [i16; 2]) -> bool {
    let d1 = cross(a, b, p);
    let d2 = cross(b, c, p);
    let d3 = cross(c, a, p);
    let neg = d1 < 0 || d2 < 0 || d3 < 0;
    let pos = d1 > 0 || d2 > 0 || d3 > 0;
    !(neg && pos)
}

fn ear_clip(verts: &[[i16; 2]], ring: &mut [u8], out: &mut [[u8; 3]]) -> Option<usize> {
    let mut n = ring.len();
    if n < 3 || out.len() < n - 2 {
        return None;
    }
    let mut written = 0usize;
    let mut guard = n * n + 16;
    let mut i = 0usize;
    while n > 3 {
        guard -= 1;
        if guard == 0 {
            // Degenerate ring: fall back to a fan so the piece still has triangles.
            break;
        }
        let ia = ring[(i + n - 1) % n];
        let ib = ring[i % n];
        let ic = ring[(i + 1) % n];
        let a = verts[usize::from(ia)];
        let b = verts[usize::from(ib)];
        let c = verts[usize::from(ic)];
        if cross(a, b, c) <= 0 {
            i = (i + 1) % n;
            continue;
        }
        let mut blocked = false;
        for iv in ring[..n].iter().copied() {
            if iv == ia || iv == ib || iv == ic {
                continue;
            }
            if point_in_tri(a, b, c, verts[usize::from(iv)]) {
                blocked = true;
                break;
            }
        }
        if blocked {
            i = (i + 1) % n;
            continue;
        }
        out[written] = [ia, ib, ic];
        written += 1;
        for k in i % n..n - 1 {
            ring[k] = ring[k + 1];
        }
        n -= 1;
        if i >= n {
            i = 0;
        }
    }
    while n > 3 {
        // Guard tripped: close the remainder with a fan from the first ring vertex.
        out[written] = [ring[0], ring[n - 2], ring[n - 1]];
        written += 1;
        n -= 1;
    }
    out[written] = [ring[0], ring[1], ring[2]];
    written += 1;
    Some(written)
}
