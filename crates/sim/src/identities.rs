pub use playerstate_iw4::{
    FireCause, LifeSequence, MatchRng, PendingBrass, RNG_DOMAIN_SCHEME, RngDomain, ShotId,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventSequence(pub u32);

impl EventSequence {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    pub fn is_newer_than(self, other: Self) -> bool {
        let distance = self.0.wrapping_sub(other.0);
        distance != 0 && distance < (1 << 31)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionSequence(pub u32);

impl ActionSequence {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PelletId(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScriptModelId(pub(crate) u32);

impl ScriptModelId {
    pub const fn from_wire(ordinal: u32) -> Self {
        Self(ordinal)
    }

    pub const fn to_wire(self) -> u32 {
        self.0
    }

    pub const fn from_authored_source_ordinal(ordinal: u32) -> Self {
        Self(ordinal)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectileId(pub u32);

impl ProjectileId {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DamageSource {
    Shot(ShotId),
    Projectile(ProjectileId),

    Radius(ScriptModelId),

    Melee,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatchPhase {
    #[default]
    Warmup,
    Playing,

    Intermission,
    PostGame,
}
