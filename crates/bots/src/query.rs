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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceBudget {
    pub remaining: u32,
}

impl TraceBudget {
    pub const fn new(remaining: u32) -> Self {
        Self { remaining }
    }

    fn take(&mut self) -> bool {
        let granted = self.remaining != 0;
        if granted {
            self.remaining -= 1;
        }
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

    /// Positions at which a target can appear to this observer through doors.
    fn apparent_positions(&self, _observer: [f32; 3], target: [f32; 3]) -> Vec<[f32; 3]> {
        vec![target]
    }

    /// Target just beyond the entrance plane of an explicit portal edge.
    fn portal_waypoint(&self, _entry: [f32; 3], _exit: [f32; 3]) -> Option<[f32; 3]> {
        None
    }

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

pub struct Budgeted<'a, W> {
    pub world: &'a mut W,
    pub budget: &'a mut TraceBudget,
}

impl<'a, W> Budgeted<'a, W> {
    pub fn new(world: &'a mut W, budget: &'a mut TraceBudget) -> Self {
        Self { world, budget }
    }

    fn grant(&mut self) -> QueryResult<()> {
        if self.budget.take() {
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
        self.world.sight_ray(start, end, ignore)
    }

    fn shot_ray(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        ignore: ClientId,
    ) -> QueryResult<SightSample> {
        self.grant()?;
        self.world.shot_ray(start, end, ignore)
    }

    fn hull_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        self.grant()?;
        self.world.hull_trace(start, end)
    }

    fn navigation_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        self.grant()?;
        self.world.navigation_trace(start, end)
    }

    fn walk_hull(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<WalkSample> {
        self.grant()?;
        self.world.walk_hull(start, end)
    }

    fn apparent_positions(&self, observer: [f32; 3], target: [f32; 3]) -> Vec<[f32; 3]> {
        self.world.apparent_positions(observer, target)
    }

    fn portal_waypoint(&self, entry: [f32; 3], exit: [f32; 3]) -> Option<[f32; 3]> {
        self.world.portal_waypoint(entry, exit)
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
            ignore_model: None,
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
            ignore_model: None,
        })))
    }

    fn hull_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        Ok(crate::nav::BakeTraces::bake_hull(self, start, end))
    }

    fn navigation_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> QueryResult<HullTrace> {
        Ok(crate::nav::BakeTraces::bake_navigation(self, start, end))
    }

    fn apparent_positions(&self, observer: [f32; 3], target: [f32; 3]) -> Vec<[f32; 3]> {
        let content = self.content();
        let Some(atlas) = content.atlas() else {
            return vec![target];
        };
        let (here, there) = (
            sim::world_graph::world_at(observer),
            sim::world_graph::world_at(target),
        );
        let eye = [observer[0], observer[1], observer[2] + 60.0];
        let chest = [target[0], target[1], target[2] + 48.0];
        let mut positions = if here == there && atlas.crossing(eye, chest).is_none() {
            vec![target]
        } else {
            Vec::new()
        };
        for (index, pair) in atlas.portals.iter().enumerate() {
            for from_a in [true, false] {
                let (near, far, yaw) = pair.oriented(from_a);
                if near.world.index() != here
                    || far.world.index() != there
                    || self
                        .world_objects()
                        .portal_blocked_exits()
                        .get(index)
                        .is_some_and(|doors| doors[usize::from(from_a)])
                {
                    continue;
                }
                let apparent = sim::world_graph::carry_point(&far, &near, -yaw, target);
                let chest = [apparent[0], apparent[1], apparent[2] + 48.0];
                if atlas
                    .crossing(eye, chest)
                    .is_some_and(|(_, id, side)| id.index() == index && side == from_a)
                {
                    positions.push(apparent);
                }
            }
        }
        positions
    }

    fn portal_waypoint(&self, entry: [f32; 3], exit: [f32; 3]) -> Option<[f32; 3]> {
        let content = self.content();
        let atlas = content.atlas()?;
        let close =
            |a: [f32; 3], b: [f32; 3]| (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>() < 4.0;
        for (index, pair) in atlas.portals.iter().enumerate() {
            for (side, (near, far)) in [(pair.a, pair.b), (pair.b, pair.a)].into_iter().enumerate()
            {
                if self
                    .world_objects()
                    .portal_blocked_exits()
                    .get(index)
                    .is_some_and(|doors| doors[1 - side])
                {
                    continue;
                }
                if close(crate::nav::portal_entrance(near), entry)
                    && close(crate::nav::portal_entrance(far), exit)
                {
                    let offset = sim::world_graph::world_offset(near.world.index());
                    return Some(std::array::from_fn(|i| {
                        near.base[i] + offset[i] - near.normal[i] * 6.0
                            + if i == 2 { 1.5 } else { 0.0 }
                    }));
                }
            }
        }
        None
    }

    fn objective_contains(&self, obj: ModeObjective, feet: [f32; 3]) -> bool {
        let dx = feet[0] - obj.origin[0];
        let dy = feet[1] - obj.origin[1];
        dx * dx + dy * dy <= obj.radius * obj.radius && (feet[2] - obj.origin[2]).abs() <= 44.0
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

impl crate::nav::BakeTraces for SimWorld {
    fn bake_hull(&self, start: [f32; 3], end: [f32; 3]) -> HullTrace {
        hull(self.trace_world(start, end, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID))
    }

    fn bake_navigation(&self, start: [f32; 3], end: [f32; 3]) -> HullTrace {
        hull(self.trace_navigation(start, end, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID))
    }
}
impl crate::nav::BakeTraces for sim::CollisionScene {
    fn bake_hull(&self, start: [f32; 3], end: [f32; 3]) -> HullTrace {
        hull(self.trace_world(start, end, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID))
    }

    fn bake_navigation(&self, start: [f32; 3], end: [f32; 3]) -> HullTrace {
        hull(self.trace_navigation(start, end, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID))
    }
}

fn hull(hit: trace_iw4::Trace) -> HullTrace {
    HullTrace {
        normal: hit.normal,
        fraction: hit.fraction,
        endpos: hit.endpos,
        startsolid: hit.allsolid != 0 || hit.startsolid != 0,
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
            collider:
                ColliderId::EntityDObjBone { .. }
                | ColliderId::EntityLinkedBrush { .. }
                | ColliderId::EntityBody { .. },
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
