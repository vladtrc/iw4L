use sim::{
    BulletTraceQuery, ColliderId, MASK_PLAYER_SOLID, MASK_SHOT, PLAYER_MAXS, PLAYER_MINS,
    TraceOutcome,
};
use sim::{ClientId, SimWorld};

use crate::observation::WeaponClass;

/// CONTENTS_GLASS as in `weapon_iw4`. Sight omits it; shots do not.
const CONTENTS_GLASS: u32 = 0x10;
const MASK_SIGHT: u32 = MASK_SHOT & !CONTENTS_GLASS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceBudget {
    pub remaining: u32,
}

impl TraceBudget {
    pub const fn new(remaining: u32) -> Self {
        Self { remaining }
    }

    pub fn take(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObstacleKind {
    World,
    StaticOrLinked,
    Glass,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SightSample {
    Clear,
    HitPlayer {
        client: ClientId,
        fraction: f32,
    },
    Blocked {
        fraction: f32,
        obstacle: ObstacleKind,
    },
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkSample {
    Clear,
    Blocked,
    BudgetExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullTrace {
    pub fraction: f32,
    pub endpos: [f32; 3],
    pub startsolid: bool,
}

pub trait WorldQuery {
    fn sight_ray(&mut self, start: [f32; 3], end: [f32; 3], ignore: ClientId) -> SightSample;
    fn shot_ray(&mut self, start: [f32; 3], end: [f32; 3], ignore: ClientId) -> SightSample;
    fn hull_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> HullTrace;

    fn walk_hull(&mut self, start: [f32; 3], end: [f32; 3]) -> WalkSample {
        // Mesh floors put the hull on the triangle. A 2-unit lift is a scrape
        // epsilon; 18 is the same step height the graph already refuses as a
        // walk edge. Neither is a beeline through a wall.
        for lift in [2.0, 18.0] {
            let hit = self.hull_trace(
                [start[0], start[1], start[2] + lift],
                [end[0], end[1], end[2] + lift],
            );
            if !hit.startsolid && hit.fraction >= 1.0 {
                return WalkSample::Clear;
            }
        }
        WalkSample::Blocked
    }

    fn weapon_class(&self, _weapon: u16) -> WeaponClass {
        WeaponClass::Assault
    }
}

pub struct Budgeted<'a, W> {
    pub world: &'a mut W,
    pub budget: &'a mut TraceBudget,
}

impl<W: WorldQuery> WorldQuery for Budgeted<'_, W> {
    fn sight_ray(&mut self, start: [f32; 3], end: [f32; 3], ignore: ClientId) -> SightSample {
        if !self.budget.take() {
            return SightSample::Unknown;
        }
        self.world.sight_ray(start, end, ignore)
    }

    fn shot_ray(&mut self, start: [f32; 3], end: [f32; 3], ignore: ClientId) -> SightSample {
        if !self.budget.take() {
            return SightSample::Unknown;
        }
        self.world.shot_ray(start, end, ignore)
    }

    fn hull_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> HullTrace {
        if !self.budget.take() {
            return HullTrace {
                fraction: 0.0,
                endpos: start,
                startsolid: true,
            };
        }
        self.world.hull_trace(start, end)
    }

    fn walk_hull(&mut self, start: [f32; 3], end: [f32; 3]) -> WalkSample {
        if !self.budget.take() {
            return WalkSample::BudgetExhausted;
        }
        self.world.walk_hull(start, end)
    }

    fn weapon_class(&self, weapon: u16) -> WeaponClass {
        self.world.weapon_class(weapon)
    }
}

impl WorldQuery for SimWorld {
    fn sight_ray(&mut self, start: [f32; 3], end: [f32; 3], ignore: ClientId) -> SightSample {
        classify_shot(self.sensor_trace(BulletTraceQuery {
            start,
            end,
            mask: MASK_SIGHT,
            ignore: Some(ignore),
            ignore_hit: None,
        }))
    }

    fn shot_ray(&mut self, start: [f32; 3], end: [f32; 3], ignore: ClientId) -> SightSample {
        classify_shot(self.sensor_trace(BulletTraceQuery {
            start,
            end,
            mask: MASK_SHOT,
            ignore: Some(ignore),
            ignore_hit: None,
        }))
    }

    fn hull_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> HullTrace {
        let hit = self.trace_world(start, end, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID);
        HullTrace {
            fraction: hit.fraction,
            endpos: hit.endpos,
            startsolid: hit.allsolid != 0 || hit.startsolid != 0,
        }
    }

    fn weapon_class(&self, weapon: u16) -> WeaponClass {
        WeaponClass::from_script_name(self.weapon_script_name(u32::from(weapon)))
    }
}

fn classify_shot(outcome: TraceOutcome) -> SightSample {
    match outcome {
        TraceOutcome::Miss { .. } => SightSample::Clear,
        TraceOutcome::Hit {
            fraction,
            collider: ColliderId::Player { client, .. },
            ..
        } => SightSample::HitPlayer { client, fraction },
        TraceOutcome::Hit {
            fraction,
            collider: ColliderId::World { glass_encoded, .. },
            ..
        } => SightSample::Blocked {
            fraction,
            obstacle: if glass_encoded != 0 {
                ObstacleKind::Glass
            } else {
                ObstacleKind::World
            },
        },
        TraceOutcome::Hit {
            fraction,
            collider: ColliderId::EntityDObjBone { .. } | ColliderId::EntityLinkedBrush { .. },
            ..
        } => SightSample::Blocked {
            fraction,
            obstacle: ObstacleKind::StaticOrLinked,
        },
        TraceOutcome::StartSolid { .. } => SightSample::Blocked {
            fraction: 0.0,
            obstacle: ObstacleKind::Other,
        },
        TraceOutcome::Invalid { .. } => SightSample::Unknown,
    }
}
