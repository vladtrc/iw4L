//! Shared coordinate layout for maps in an atlas.

/// Leave room for native map coordinates on either side of each origin.
/// A 32768-unit stride put Nightclub's playable x=-16384 at the world boundary.
const ATLAS_SLAB: f32 = 262144.0;

/// Refuse maps whose native clip would overlap another world's coordinate domain.
pub fn atlas_bounds_fit(min_x: f32, max_x: f32) -> bool {
    min_x.is_finite()
        && max_x.is_finite()
        && min_x <= max_x
        && min_x > -ATLAS_SLAB * 0.5 + 1024.0
        && max_x < ATLAS_SLAB * 0.5 - 1024.0
}

/// The incoming physical half-space must exist at both ends of a route.
/// A flipped pair carries the incoming front onto the destination front.
pub fn portal_side_allowed(
    entered_two_sided: bool,
    exit_two_sided: bool,
    flip: bool,
    entered_front: bool,
) -> bool {
    (entered_two_sided || entered_front) && (exit_two_sided || entered_front == flip)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PortalClearance {
    #[default]
    Walk,
    Flight,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorldId(pub usize);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PortalId(pub usize);

impl WorldId {
    pub const ROOT: Self = Self(0);
    pub const fn index(self) -> usize {
        self.0
    }
}

impl PortalId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WorldLayout;

impl WorldLayout {
    pub fn origin(self, world: WorldId) -> [f32; 3] {
        [world.0 as f32 * ATLAS_SLAB, 0.0, 0.0]
    }

    pub fn world_at(self, point: [f32; 3]) -> WorldId {
        WorldId((point[0] / ATLAS_SLAB).round().max(0.0) as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalTransform {
    pub from: WorldId,
    pub to: WorldId,
    pub yaw: f32,
    pub entry: [f32; 3],
    pub exit: [f32; 3],
}

impl PortalTransform {
    pub fn vector(self, vector: [f32; 3]) -> [f32; 3] {
        let (sin, cos) = self.yaw.sin_cos();
        [
            cos * vector[0] - sin * vector[1],
            sin * vector[0] + cos * vector[1],
            vector[2],
        ]
    }

    pub fn point(self, point: [f32; 3]) -> [f32; 3] {
        let from = WorldLayout.origin(self.from);
        let to = WorldLayout.origin(self.to);
        let local = std::array::from_fn(|i| (point[i] - from[i]) - self.entry[i]);
        let rotated = self.vector(local);
        std::array::from_fn(|i| (rotated[i] + self.exit[i]) + to[i])
    }
}

impl std::fmt::Display for WorldId {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(out)
    }
}
impl std::fmt::Display for PortalId {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(out)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalDoor {
    pub two_sided: bool,
    pub world: WorldId,
    pub base: [f32; 3],
    pub normal: [f32; 3],
    pub width: f32,
    pub height: f32,
}
