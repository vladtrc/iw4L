pub use asset_iw4::{XSurfaceCollisionLeaf, XSurfaceCollisionNode};

const CHILD_LEAF_BIT: u16 = 0x8000;
const NODE_QUEUE_MASK: usize = 0x7f;
const LEAF_QUEUE_MASK: usize = 0x03;
const QUANTIZED_AABB_LIMIT: f64 = 1_000_000.0;

#[derive(Clone, Copy, Debug)]
pub struct XSurfaceCollisionTree<'a> {
    pub trans: [f32; 3],

    pub scale: [f32; 3],

    pub nodes: &'a [XSurfaceCollisionNode],

    pub leafs: &'a [XSurfaceCollisionLeaf],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XSurfaceVisitError {
    EmptyNodeTree,
    EmptyChildRange,
    NodeRange,
    LeafRange,
    NodeQueueFull,
    NodeTraversalLimit,
}

#[derive(Clone, Copy, Debug, Default)]
struct QueueElement {
    begin: usize,
    count: usize,
}

pub fn xsurface_visit_triangles_in_aabb(
    tree: XSurfaceCollisionTree<'_>,
    aabb_mins: [f32; 3],
    aabb_maxs: [f32; 3],
    mut visitor: impl FnMut(u32) -> bool,
) -> Result<bool, XSurfaceVisitError> {
    if tree.nodes.is_empty() {
        return Err(XSurfaceVisitError::EmptyNodeTree);
    }
    let (mins, maxs) = convert_aabb(tree, aabb_mins, aabb_maxs);
    let mut node_queue = [QueueElement::default(); 128];
    let mut pipeline = VisitPipeline::default();
    node_queue[0] = QueueElement { begin: 0, count: 1 };
    let mut node_begin = 0usize;
    let mut node_end = 1usize;
    let mut visited_nodes = 0usize;

    while node_begin != node_end {
        let range = node_queue[node_begin];
        node_begin = (node_begin + 1) & NODE_QUEUE_MASK;
        let end = range
            .begin
            .checked_add(range.count)
            .filter(|end| *end <= tree.nodes.len())
            .ok_or(XSurfaceVisitError::NodeRange)?;
        for node in &tree.nodes[range.begin..end] {
            visited_nodes = visited_nodes.saturating_add(1);
            if visited_nodes > tree.nodes.len() {
                return Err(XSurfaceVisitError::NodeTraversalLimit);
            }
            if !node_overlaps(node, mins, maxs) {
                continue;
            }
            let child_begin = usize::from(node.child_begin_index);
            let child_count = usize::from(node.child_count & !CHILD_LEAF_BIT);
            if child_count == 0 {
                return Err(XSurfaceVisitError::EmptyChildRange);
            }
            let child = QueueElement {
                begin: child_begin,
                count: child_count,
            };
            if node.child_count & CHILD_LEAF_BIT == 0 {
                node_queue[node_end] = child;
                node_end = (node_end + 1) & NODE_QUEUE_MASK;
                if node_begin == node_end {
                    return Err(XSurfaceVisitError::NodeQueueFull);
                }
            } else {
                if !pipeline.push_leaf_range(tree.leafs, child, &mut visitor)? {
                    return Ok(false);
                }
            }
        }
    }

    pipeline.drain(tree.leafs, &mut visitor)
}

fn convert_aabb(
    tree: XSurfaceCollisionTree<'_>,
    aabb_mins: [f32; 3],
    aabb_maxs: [f32; 3],
) -> ([i32; 3], [i32; 3]) {
    let mut mins = [0; 3];
    let mut maxs = [0; 3];
    for axis in 0..3 {
        let trans = f64::from(tree.trans[axis]);
        let scale = f64::from(tree.scale[axis]);
        mins[axis] = quantize((f64::from(aabb_mins[axis]) + trans) * scale - 0.5);
        maxs[axis] = quantize((f64::from(aabb_maxs[axis]) + trans) * scale + 0.5);
    }
    (mins, maxs)
}

fn quantize(value: f64) -> i32 {
    let value = value.clamp(-QUANTIZED_AABB_LIMIT, QUANTIZED_AABB_LIMIT);
    let truncated = value as i32;
    let fraction = value - f64::from(truncated);
    if fraction > 0.5 || (fraction == 0.5 && truncated & 1 != 0) {
        truncated + 1
    } else if fraction < -0.5 || (fraction == -0.5 && truncated & 1 != 0) {
        truncated - 1
    } else {
        truncated
    }
}

fn node_overlaps(node: &XSurfaceCollisionNode, mins: [i32; 3], maxs: [i32; 3]) -> bool {
    (0..3).all(|axis| {
        maxs[axis] >= i32::from(node.mins[axis]) && mins[axis] <= i32::from(node.maxs[axis])
    })
}

#[derive(Default)]
struct VisitPipeline {
    leaf_queue: [QueueElement; 4],
    leaf_begin: usize,
    leaf_end: usize,
    triangle_queue: [QueueElement; 4],
    triangle_begin: usize,
    triangle_end: usize,
    visitor_queue: [u32; 4],
    visitor_begin: usize,
    visitor_end: usize,
}

impl VisitPipeline {
    fn push_leaf_range(
        &mut self,
        leafs: &[XSurfaceCollisionLeaf],
        range: QueueElement,
        visitor: &mut impl FnMut(u32) -> bool,
    ) -> Result<bool, XSurfaceVisitError> {
        self.leaf_queue[self.leaf_end] = range;
        self.leaf_end = (self.leaf_end + 1) & LEAF_QUEUE_MASK;
        if self.leaf_begin == self.leaf_end {
            self.process_one_leaf_range(leafs, visitor)
        } else {
            Ok(true)
        }
    }

    fn process_one_leaf_range(
        &mut self,
        leafs: &[XSurfaceCollisionLeaf],
        visitor: &mut impl FnMut(u32) -> bool,
    ) -> Result<bool, XSurfaceVisitError> {
        let range = self.leaf_queue[self.leaf_begin];
        self.leaf_begin = (self.leaf_begin + 1) & LEAF_QUEUE_MASK;
        let end = range
            .begin
            .checked_add(range.count)
            .filter(|end| *end <= leafs.len())
            .ok_or(XSurfaceVisitError::LeafRange)?;
        for leaf in &leafs[range.begin..end] {
            let encoded = leaf.triangle_begin_index;
            let range = QueueElement {
                begin: usize::from(encoded & !CHILD_LEAF_BIT),
                count: if encoded & CHILD_LEAF_BIT == 0 { 1 } else { 2 },
            };
            self.triangle_queue[self.triangle_end] = range;
            self.triangle_end = (self.triangle_end + 1) & LEAF_QUEUE_MASK;
            if self.triangle_begin == self.triangle_end
                && !self.process_one_triangle_range(visitor)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn process_one_triangle_range(
        &mut self,
        visitor: &mut impl FnMut(u32) -> bool,
    ) -> Result<bool, XSurfaceVisitError> {
        let range = self.triangle_queue[self.triangle_begin];
        self.triangle_begin = (self.triangle_begin + 1) & LEAF_QUEUE_MASK;
        let end = range
            .begin
            .checked_add(range.count)
            .ok_or(XSurfaceVisitError::LeafRange)?;
        for triangle in range.begin..end {
            self.visitor_queue[self.visitor_end] = triangle as u32;
            self.visitor_end = (self.visitor_end + 1) & LEAF_QUEUE_MASK;
            if self.visitor_begin == self.visitor_end && !self.process_one_visitor(visitor) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn process_one_visitor(&mut self, visitor: &mut impl FnMut(u32) -> bool) -> bool {
        let triangle = self.visitor_queue[self.visitor_begin];
        self.visitor_begin = (self.visitor_begin + 1) & LEAF_QUEUE_MASK;
        visitor(triangle)
    }

    fn drain(
        &mut self,
        leafs: &[XSurfaceCollisionLeaf],
        visitor: &mut impl FnMut(u32) -> bool,
    ) -> Result<bool, XSurfaceVisitError> {
        while self.leaf_begin != self.leaf_end {
            if !self.process_one_leaf_range(leafs, visitor)? {
                return Ok(false);
            }
        }
        while self.triangle_begin != self.triangle_end {
            if !self.process_one_triangle_range(visitor)? {
                return Ok(false);
            }
        }
        while self.visitor_begin != self.visitor_end {
            if !self.process_one_visitor(visitor) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
