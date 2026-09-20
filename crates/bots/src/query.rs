use sim::{
    BulletTraceQuery, ColliderId, MASK_PLAYER_SOLID, MASK_SHOT, PLAYER_MAXS, PLAYER_MINS,
    TraceOutcome,
};
use sim::{ClientId, SimWorld};

use crate::observation::{ModeObjective, WeaponClass, WeaponFacts};

/// CONTENTS_GLASS as in `weapon_iw4`. Sight omits it; shots do not.
const CONTENTS_GLASS: u32 = 0x10;
const MASK_SIGHT: u32 = MASK_SHOT & !CONTENTS_GLASS;

/// A query that was refused quota and therefore never executed. It carries no
/// collision meaning: a query that did run reports its real result even when it
/// consumed the last token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryDenied;

pub type QueryResult<T> = Result<T, QueryDenied>;

/// Which part of the bot spent an interface query. Denial is only readable
/// per subsystem, so the counters keep them apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuerySubsystem {
    Perception,
    Connector,
    #[default]
    Execution,
    Tactical,
}

impl QuerySubsystem {
    pub const COUNT: usize = 4;
    pub const ALL: [Self; Self::COUNT] = [
        Self::Perception,
        Self::Connector,
        Self::Execution,
        Self::Tactical,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Perception => "perception",
            Self::Connector => "connector",
            Self::Execution => "execution",
            Self::Tactical => "tactical",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueryCounters {
    pub attempted: [u32; QuerySubsystem::COUNT],
    pub granted: [u32; QuerySubsystem::COUNT],
    pub denied: [u32; QuerySubsystem::COUNT],
    /// Collision operations counted where they enter the trace API. One granted
    /// `walk_hull` issues several of them.
    pub primitives: u32,
}

impl QueryCounters {
    pub const ZERO: Self = Self {
        attempted: [0; QuerySubsystem::COUNT],
        granted: [0; QuerySubsystem::COUNT],
        denied: [0; QuerySubsystem::COUNT],
        primitives: 0,
    };

    fn record(&mut self, subsystem: QuerySubsystem, granted: bool) {
        let i = subsystem.index();
        self.attempted[i] = self.attempted[i].saturating_add(1);
        let side = if granted {
            &mut self.granted[i]
        } else {
            &mut self.denied[i]
        };
        *side = side.saturating_add(1);
    }

    pub fn merge(&mut self, other: &Self) {
        for i in 0..QuerySubsystem::COUNT {
            self.attempted[i] = self.attempted[i].saturating_add(other.attempted[i]);
            self.granted[i] = self.granted[i].saturating_add(other.granted[i]);
            self.denied[i] = self.denied[i].saturating_add(other.denied[i]);
        }
        self.primitives = self.primitives.saturating_add(other.primitives);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceBudget {
    pub remaining: u32,
    pub counters: QueryCounters,
}

impl TraceBudget {
    pub const fn new(remaining: u32) -> Self {
        Self {
            remaining,
            counters: QueryCounters::ZERO,
        }
    }

    fn take(&mut self, subsystem: QuerySubsystem) -> bool {
        let granted = self.remaining != 0;
        if granted {
            self.remaining -= 1;
        }
        self.counters.record(subsystem, granted);
        granted
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
    /// The trace ran and the collision backend could not classify it. This is a
    /// physical answer, not a scheduling signal.
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkSample {
    Clear,
    BreakGlass,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullTrace {
    pub fraction: f32,
    pub normal: [f32; 3],
    pub endpos: [f32; 3],
    pub startsolid: bool,
}

pub trait WorldQuery {
    fn sight_ray(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        ignore: ClientId,
    ) -> QueryResult<SightSample>;
    fn shot_ray(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        ignore: ClientId,
    ) -> QueryResult<SightSample>;
    fn hull_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace>;

    fn navigation_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        self.hull_trace(start, end)
    }

    /// One standing-hull clearance check. Mesh floors put the hull on the
    /// triangle, so a 2-unit lift is the scrape epsilon and 18 is the same step
    /// height the graph already refuses as a walk edge; a pane that only the
    /// navigation trace passes is glass rather than a wall.
    ///
    /// This is composed of the traces above so that every wrapper sees the
    /// collision operations it issues. Backends implement the traces.
    fn walk_hull(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<WalkSample> {
        let mut glass = false;
        for lift in [2.0, 18.0] {
            let a = [start[0], start[1], start[2] + lift];
            let b = [end[0], end[1], end[2] + lift];
            let hit = self.hull_trace(a, b)?;
            if !hit.startsolid && hit.fraction >= 1.0 {
                return Ok(WalkSample::Clear);
            }
            let open = self.navigation_trace(a, b)?;
            glass |= !open.startsolid && open.fraction >= 1.0;
        }
        Ok(if glass {
            WalkSample::BreakGlass
        } else {
            WalkSample::Blocked
        })
    }

    /// Attribute the queries that follow to a subsystem in the shared counters.
    fn enter(&mut self, _subsystem: QuerySubsystem) {}

    fn objective_contains(&self, obj: ModeObjective, feet: [f32; 3]) -> bool {
        let dx = feet[0] - obj.origin[0];
        let dy = feet[1] - obj.origin[1];
        dx * dx + dy * dy <= obj.radius * obj.radius && (feet[2] - obj.origin[2]).abs() <= 44.0
    }

    fn weapon_class(&self, _weapon: u16) -> WeaponClass {
        WeaponClass::Assault
    }

    /// Timing and magazine facts the weapon-readiness skill needs. `None` for an
    /// id the content has no usable weapon for.
    fn weapon_facts(&self, _weapon: u16) -> Option<WeaponFacts> {
        None
    }
}

/// Counts the collision operations a composite check issues without charging
/// them against the interface quota, which stays in walk/trace units.
struct Primitives<'a, W> {
    world: &'a mut W,
    count: u32,
}

impl<W: WorldQuery> WorldQuery for Primitives<'_, W> {
    fn sight_ray(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        ignore: ClientId,
    ) -> QueryResult<SightSample> {
        self.count += 1;
        self.world.sight_ray(start, end, ignore)
    }

    fn shot_ray(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        ignore: ClientId,
    ) -> QueryResult<SightSample> {
        self.count += 1;
        self.world.shot_ray(start, end, ignore)
    }

    fn hull_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        self.count += 1;
        self.world.hull_trace(start, end)
    }

    fn navigation_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        self.count += 1;
        self.world.navigation_trace(start, end)
    }

    fn objective_contains(&self, obj: ModeObjective, feet: [f32; 3]) -> bool {
        self.world.objective_contains(obj, feet)
    }

    fn weapon_class(&self, weapon: u16) -> WeaponClass {
        self.world.weapon_class(weapon)
    }

    fn weapon_facts(&self, weapon: u16) -> Option<WeaponFacts> {
        self.world.weapon_facts(weapon)
    }
}

pub struct Budgeted<'a, W> {
    pub world: &'a mut W,
    pub budget: &'a mut TraceBudget,
    pub subsystem: QuerySubsystem,
}

impl<'a, W> Budgeted<'a, W> {
    pub fn new(world: &'a mut W, budget: &'a mut TraceBudget) -> Self {
        Self {
            world,
            budget,
            subsystem: QuerySubsystem::default(),
        }
    }

    fn grant(&mut self) -> QueryResult<()> {
        if self.budget.take(self.subsystem) {
            Ok(())
        } else {
            Err(QueryDenied)
        }
    }
}

impl<W: WorldQuery> WorldQuery for Budgeted<'_, W> {
    fn sight_ray(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        ignore: ClientId,
    ) -> QueryResult<SightSample> {
        self.grant()?;
        self.budget.counters.primitives += 1;
        self.world.sight_ray(start, end, ignore)
    }

    fn shot_ray(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        ignore: ClientId,
    ) -> QueryResult<SightSample> {
        self.grant()?;
        self.budget.counters.primitives += 1;
        self.world.shot_ray(start, end, ignore)
    }

    fn hull_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        self.grant()?;
        self.budget.counters.primitives += 1;
        self.world.hull_trace(start, end)
    }

    fn navigation_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        self.grant()?;
        self.budget.counters.primitives += 1;
        self.world.navigation_trace(start, end)
    }

    fn walk_hull(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<WalkSample> {
        self.grant()?;
        let mut probe = Primitives {
            world: self.world,
            count: 0,
        };
        let sample = probe.walk_hull(start, end);
        let issued = probe.count;
        self.budget.counters.primitives += issued;
        sample
    }

    fn enter(&mut self, subsystem: QuerySubsystem) {
        self.subsystem = subsystem;
    }

    fn objective_contains(&self, obj: ModeObjective, feet: [f32; 3]) -> bool {
        self.world.objective_contains(obj, feet)
    }

    fn weapon_class(&self, weapon: u16) -> WeaponClass {
        self.world.weapon_class(weapon)
    }

    fn weapon_facts(&self, weapon: u16) -> Option<WeaponFacts> {
        self.world.weapon_facts(weapon)
    }
}

impl WorldQuery for SimWorld {
    fn sight_ray(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        ignore: ClientId,
    ) -> QueryResult<SightSample> {
        Ok(classify_shot(self.sensor_trace(BulletTraceQuery {
            start,
            end,
            mask: MASK_SIGHT,
            ignore: Some(ignore),
            ignore_hit: None,
        })))
    }

    fn shot_ray(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        ignore: ClientId,
    ) -> QueryResult<SightSample> {
        Ok(classify_shot(self.sensor_trace(BulletTraceQuery {
            start,
            end,
            mask: MASK_SHOT,
            ignore: Some(ignore),
            ignore_hit: None,
        })))
    }

    fn hull_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        let hit = self.trace_world(start, end, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID);
        Ok(HullTrace {
            normal: hit.normal,
            fraction: hit.fraction,
            endpos: hit.endpos,
            startsolid: hit.allsolid != 0 || hit.startsolid != 0,
        })
    }

    fn navigation_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        let hit = self.trace_navigation(start, end, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID);
        Ok(HullTrace {
            normal: hit.normal,
            fraction: hit.fraction,
            endpos: hit.endpos,
            startsolid: hit.allsolid != 0 || hit.startsolid != 0,
        })
    }

    fn objective_contains(&self, obj: ModeObjective, feet: [f32; 3]) -> bool {
        self.objective_position_in_volume(obj.id, feet)
            .unwrap_or_else(|| {
                let dx = feet[0] - obj.origin[0];
                let dy = feet[1] - obj.origin[1];
                dx * dx + dy * dy <= obj.radius * obj.radius
                    && (feet[2] - obj.origin[2]).abs() <= 44.0
            })
    }

    fn weapon_class(&self, weapon: u16) -> WeaponClass {
        WeaponClass::from_script_name(self.weapon_script_name(u32::from(weapon)))
    }

    fn weapon_facts(&self, weapon: u16) -> Option<WeaponFacts> {
        let facts = self.weapon_combat_facts(u32::from(weapon))?;
        Some(WeaponFacts {
            // weapInventoryType 0 is a carried gun the player can select.
            selectable: facts.inventory_type == 0,
            quick_select: facts.weap_class == PISTOL_WEAP_CLASS,
            clip_size: facts.clip_size,
            raise_time_ms: facts.raise_time_ms,
            quick_raise_time_ms: facts.quick_raise_time_ms,
            drop_time_ms: facts.drop_time_ms,
            quick_drop_time_ms: facts.quick_drop_time_ms,
            reload_time_ms: facts.reload_time_ms,
            reload_empty_time_ms: facts.reload_empty_time_ms,
        })
    }
}

/// Pistols raise and drop on the quick timings during a weapon change.
const PISTOL_WEAP_CLASS: i32 = 5;

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
