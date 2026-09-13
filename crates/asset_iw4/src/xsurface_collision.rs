#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct XSurfaceCollisionNode {
    pub mins: [u16; 3],

    pub maxs: [u16; 3],

    pub child_begin_index: u16,

    pub child_count: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct XSurfaceCollisionLeaf {
    pub triangle_begin_index: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XSurfaceCollisionRangeError {
    RigidTriangleRange,
    EmptyNodeTree,
    EmptyChildRange,
    NodeRange,
    LeafRange,
    TriangleRange,
}

pub fn validate_xsurface_collision_ranges(
    nodes: &[XSurfaceCollisionNode],
    leafs: &[XSurfaceCollisionLeaf],
    triangle_begin: usize,
    triangle_end: usize,
) -> Result<(), XSurfaceCollisionRangeError> {
    if nodes.is_empty() {
        return Err(XSurfaceCollisionRangeError::EmptyNodeTree);
    }
    for node in nodes {
        let child_count = usize::from(node.child_count & 0x7fff);
        if child_count == 0 {
            return Err(XSurfaceCollisionRangeError::EmptyChildRange);
        }
        let child_end = usize::from(node.child_begin_index)
            .checked_add(child_count)
            .ok_or(XSurfaceCollisionRangeError::NodeRange)?;
        if node.child_count & 0x8000 == 0 {
            if child_end > nodes.len() {
                return Err(XSurfaceCollisionRangeError::NodeRange);
            }
        } else if child_end > leafs.len() {
            return Err(XSurfaceCollisionRangeError::LeafRange);
        }
    }
    for leaf in leafs {
        let encoded = leaf.triangle_begin_index;
        let begin = usize::from(encoded & 0x7fff);
        let count = if encoded & 0x8000 == 0 { 1 } else { 2 };
        let end = begin
            .checked_add(count)
            .ok_or(XSurfaceCollisionRangeError::TriangleRange)?;
        if begin < triangle_begin || end > triangle_end {
            return Err(XSurfaceCollisionRangeError::TriangleRange);
        }
    }
    Ok(())
}
