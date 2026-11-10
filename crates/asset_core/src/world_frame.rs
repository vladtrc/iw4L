//! Actor ownership remains explicit: overlapping placements cannot identify a
//! gravity world from a scene-space point.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFrame {
    origin: [f32; 3],
    /// Native X, Y and Z axes expressed in scene coordinates (columns).
    axes: [[f32; 3]; 3],
}

impl WorldFrame {
    pub fn is_axis_permutation(self) -> bool {
        self.axes.iter().all(|axis| {
            axis.iter().filter(|&&v| v.abs() == 1.0).count() == 1
                && axis.iter().all(|v| *v == 0.0 || v.abs() == 1.0)
        })
    }

    pub fn box_to_local(self, mins: [f32; 3], maxs: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        assert!(
            self.is_axis_permutation(),
            "swept boxes require cardinal world frames"
        );
        let mid = std::array::from_fn(|i| (mins[i] + maxs[i]) * 0.5);
        let half: [f32; 3] = std::array::from_fn(|i| (maxs[i] - mins[i]) * 0.5);
        let mid = self.vector_to_local(mid);
        let half: [f32; 3] = self
            .axes
            .map(|axis| (0..3).map(|i| axis[i].abs() * half[i]).sum());
        (
            std::array::from_fn(|i| mid[i] - half[i]),
            std::array::from_fn(|i| mid[i] + half[i]),
        )
    }

    pub fn rotation_is_identity(self) -> bool {
        self.axes == Self::IDENTITY.axes
    }
    pub fn translated(origin: [f32; 3]) -> Result<Self, String> {
        Self::new(origin, Self::IDENTITY.axes)
    }

    pub fn bounds_to_scene(self, mid: [f32; 3], half: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        let half = std::array::from_fn(|i| (0..3).map(|j| self.axes[j][i].abs() * half[j]).sum());
        (self.point_to_scene(mid), half)
    }

    pub fn plane_to_scene(self, normal: [f32; 3], distance: f32) -> ([f32; 3], f32) {
        let normal = self.vector_to_scene(normal);
        (normal, distance + dot(normal, self.origin))
    }

    pub const IDENTITY: Self = Self {
        origin: [0.0; 3],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    pub fn new(origin: [f32; 3], axes: [[f32; 3]; 3]) -> Result<Self, String> {
        if origin
            .iter()
            .chain(axes.iter().flatten())
            .any(|v| !v.is_finite())
        {
            return Err("non-finite world frame".into());
        }
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                if (dot(axes[i], axes[j]) - expected).abs() > 0.00001 {
                    return Err("world frame axes must be orthonormal".into());
                }
            }
        }
        let [a, b, c] = axes;
        let cross = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        if (dot(cross, c) - 1.0).abs() > 0.00001 {
            return Err("world frame must preserve handedness".into());
        }
        Ok(Self { origin, axes })
    }

    pub fn vector_to_scene(self, vector: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| (0..3).map(|j| self.axes[j][i] * vector[j]).sum())
    }

    pub fn vector_to_local(self, vector: [f32; 3]) -> [f32; 3] {
        self.axes.map(|axis| dot(axis, vector))
    }

    pub fn point_to_scene(self, point: [f32; 3]) -> [f32; 3] {
        let rotated = self.vector_to_scene(point);
        std::array::from_fn(|i| rotated[i] + self.origin[i])
    }

    pub fn point_to_local(self, point: [f32; 3]) -> [f32; 3] {
        self.vector_to_local(std::array::from_fn(|i| point[i] - self.origin[i]))
    }
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

/// The owning world is a field, not something to guess from `xyz[0]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimPoint {
    world: crate::atlas::WorldId,
    xyz: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalPoint {
    world: crate::atlas::WorldId,
    xyz: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScenePoint {
    xyz: [f32; 3],
}

impl SimPoint {
    pub const fn new(world: crate::atlas::WorldId, xyz: [f32; 3]) -> Self {
        Self { world, xyz }
    }

    pub fn from_slab(xyz: [f32; 3]) -> Self {
        Self::new(crate::atlas::WorldLayout.world_at(xyz), xyz)
    }

    pub const fn world(self) -> crate::atlas::WorldId {
        self.world
    }

    pub const fn xyz(self) -> [f32; 3] {
        self.xyz
    }

    pub fn to_local(self) -> LocalPoint {
        let origin = crate::atlas::WorldLayout.origin(self.world);
        LocalPoint {
            world: self.world,
            xyz: [
                self.xyz[0] - origin[0],
                self.xyz[1] - origin[1],
                self.xyz[2] - origin[2],
            ],
        }
    }
}

impl LocalPoint {
    pub const fn new(world: crate::atlas::WorldId, xyz: [f32; 3]) -> Self {
        Self { world, xyz }
    }

    pub const fn world(self) -> crate::atlas::WorldId {
        self.world
    }

    pub const fn xyz(self) -> [f32; 3] {
        self.xyz
    }

    pub fn to_sim(self) -> SimPoint {
        let origin = crate::atlas::WorldLayout.origin(self.world);
        SimPoint {
            world: self.world,
            xyz: [
                self.xyz[0] + origin[0],
                self.xyz[1] + origin[1],
                self.xyz[2] + origin[2],
            ],
        }
    }
}

impl ScenePoint {
    pub const fn from_xyz(xyz: [f32; 3]) -> Self {
        Self { xyz }
    }

    pub const fn xyz(self) -> [f32; 3] {
        self.xyz
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum SceneSpace {
    #[default]
    PortalAtlas,
    Shared(Vec<WorldFrame>),
}

impl SceneSpace {
    pub fn simulation_point(&self, point: [f32; 3]) -> [f32; 3] {
        self.to_scene(SimPoint::new(self.simulation_owner(point), point))
            .xyz()
    }

    pub fn to_scene(&self, point: SimPoint) -> ScenePoint {
        self.local_to_scene(point.to_local())
    }

    pub fn local_to_scene(&self, point: LocalPoint) -> ScenePoint {
        let world = point.world();
        let xyz = match self {
            Self::PortalAtlas => point.to_sim().xyz(),
            Self::Shared(_) => self
                .frame(world)
                .expect("simulation point references a validated world")
                .point_to_scene(point.xyz()),
        };
        ScenePoint::from_xyz(xyz)
    }

    pub fn scene_to_local(&self, world: crate::atlas::WorldId, point: ScenePoint) -> LocalPoint {
        let xyz = match self {
            Self::PortalAtlas => SimPoint::new(world, point.xyz()).to_local().xyz(),
            Self::Shared(_) => self
                .frame(world)
                .expect("scene point references a validated world")
                .point_to_local(point.xyz()),
        };
        LocalPoint::new(world, xyz)
    }

    fn clamp_owner(&self, world: crate::atlas::WorldId) -> crate::atlas::WorldId {
        match self {
            Self::PortalAtlas => world,
            Self::Shared(frames) => {
                crate::atlas::WorldId(world.0.min(frames.len().saturating_sub(1)))
            }
        }
    }

    pub fn simulation_owner(&self, point: [f32; 3]) -> crate::atlas::WorldId {
        self.clamp_owner(SimPoint::from_slab(point).world())
    }

    /// Ray endpoints can leave their owner's slab. Retain the source world
    /// explicitly instead of guessing a different owner from a distant endpoint.
    pub fn simulation_point_in(&self, world: crate::atlas::WorldId, point: [f32; 3]) -> [f32; 3] {
        self.to_scene(SimPoint::new(world, point)).xyz()
    }

    pub fn simulation_vector(&self, owner_origin: [f32; 3], vector: [f32; 3]) -> [f32; 3] {
        self.vector_to_scene(self.simulation_owner(owner_origin), vector)
    }

    pub fn vector_to_scene(&self, world: crate::atlas::WorldId, vector: [f32; 3]) -> [f32; 3] {
        if !matches!(self, Self::Shared(_)) {
            return vector;
        }
        self.frame(world)
            .expect("simulation vector references a validated world")
            .vector_to_scene(vector)
    }

    /// Validate at both recipe and installation boundaries; programmatic scenes
    /// must obey the same collision invariants as JSON recipes.
    pub fn validate(&self, worlds: usize, _portals: usize) -> Result<(), String> {
        if !(1..=255).contains(&worlds) {
            return Err("scene needs 1..255 worlds".into());
        }
        if let Self::Shared(frames) = self {
            if frames.len() != worlds {
                return Err("shared space needs one placement per world".into());
            }
            if frames.iter().any(|frame| !frame.is_axis_permutation()) {
                return Err(
                    "shared space currently requires cardinal rotations (multiples of 90 degrees)"
                        .into(),
                );
            }
            if frames[0] != WorldFrame::IDENTITY {
                return Err("shared space currently requires an identity root placement".into());
            }
        }
        Ok(())
    }

    pub fn frame(&self, world: crate::atlas::WorldId) -> Result<WorldFrame, String> {
        match self {
            Self::PortalAtlas => WorldFrame::translated(crate::atlas::WorldLayout.origin(world)),
            Self::Shared(frames) => frames
                .get(world.index())
                .copied()
                .ok_or_else(|| "world placement is unavailable".into()),
        }
    }
}
