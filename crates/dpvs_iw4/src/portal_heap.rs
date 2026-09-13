pub const QUEUED_PORTAL_POOL: usize = 256;

pub const HULL_POINTS_POOL_BYTES: usize = 0x20000;

pub const HULL_POINTS_POOL_STRIDE: usize = 0x200;

pub const HULL_POOL_NULL: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Default)]
pub struct PortalHeapNode {
    pub slot: u16,
    pub dist: f32,
}

#[must_use]
pub fn furthest_point_on_winding(plane: [f32; 4], points: &[[f32; 3]]) -> f32 {
    let n = points.len();
    if n == 0 {
        return 0.0;
    }
    let dist = |p: [f32; 3]| plane[0] * p[0] + plane[1] * p[1] + plane[2] * p[2] + plane[3];
    let first = dist(points[0]);
    let last = dist(points[n - 1]);
    if first <= last {
        let mut max = last;
        let mut i = n.saturating_sub(2);
        while i > 0 {
            let d = dist(points[i]);
            if d < max {
                break;
            }
            max = d;
            i -= 1;
        }
        max
    } else {
        let mut max = first;
        let mut i = 1usize;
        while i + 1 < n {
            let d = dist(points[i]);
            if d < max {
                break;
            }
            max = d;
            i += 1;
        }
        max
    }
}

#[must_use]
pub fn dpvs_view_plane(forward: [f32; 3], eye: [f32; 3]) -> [f32; 4] {
    let d = -(forward[0] * eye[0] + forward[1] * eye[1] + forward[2] * eye[2]);
    [forward[0], forward[1], forward[2], d]
}

pub fn heap_push(heap: &mut [PortalHeapNode], n: &mut usize, node: PortalHeapNode) -> bool {
    if *n >= heap.len() || *n >= QUEUED_PORTAL_POOL {
        return false;
    }
    let mut i = *n;
    *n += 1;
    while i > 0 {
        let parent = (i - 1) >> 1;
        if node.dist >= heap[parent].dist {
            break;
        }
        heap[i] = heap[parent];
        i = parent;
    }
    heap[i] = node;
    true
}

#[must_use]
pub fn heap_pop(heap: &mut [PortalHeapNode], n: &mut usize) -> Option<PortalHeapNode> {
    if *n == 0 {
        return None;
    }
    let out = heap[0];
    *n -= 1;
    if *n == 0 {
        return Some(out);
    }
    let last = heap[*n];
    let mut i = 0usize;
    loop {
        let mut child = 2 * i + 1;
        if child > *n {
            break;
        }
        if child < *n && heap[child].dist > heap[child + 1].dist {
            child += 1;
        }
        if heap[child].dist >= last.dist {
            break;
        }
        heap[i] = heap[child];
        i = child;
    }
    heap[i] = last;
    Some(out)
}
