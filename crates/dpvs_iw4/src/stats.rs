#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WalkStats {
    pub cells_visited: u32,
    pub portals_seen: u32,

    pub skip_ancestor: u32,

    pub skip_facing: u32,

    pub skip_clip: u32,

    pub chop_empty: u32,
    pub enqueued: u32,

    pub child_planes_unported: u32,

    pub child_clip_bevel_unread: u32,

    pub stepped_through: u32,

    pub hull_union: u32,

    pub hull_overflow: u32,

    pub hull_fail: u32,
    pub walk_limit_hits: u32,
}
