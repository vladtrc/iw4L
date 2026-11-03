use crate::ClientId;
use crate::bullet_collision::{
    BulletTraceQuery, ColliderId, MASK_BULLET_WORLD, PlayerCollisionPose, TraceOutcome,
    bullet_trace_with_entity_models,
};
use crate::world::{SimBrush, SimClipBsp, SimClipCmodels, SimClipMesh};

pub use asset_core::atlas::{PortalClearance, PortalId, PortalTransform, WorldId, WorldLayout};

/// How far from the frame of a door a crossing must stay.
const DOOR_MARGIN: f32 = 2.0;

/// One extra map of the atlas, in its native coordinates.
#[derive(Debug, Default)]
pub struct AtlasWorld {
    pub zone: String,
    pub brushes: Vec<SimBrush>,
    pub bsp: SimClipBsp,
    pub mesh: SimClipMesh,
    pub cmodels: SimClipCmodels,
}

pub use asset_core::atlas::PortalDoor as AtlasDoor;

/// Two doors that are one: whatever crosses one comes out of the other.
/// `flip` sends motion along `a.normal` out against `b.normal`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasPortal {
    pub a: AtlasDoor,
    pub b: AtlasDoor,
    pub flip: bool,
}

impl AtlasPortal {
    /// The yaw in radians that turns `a`'s side onto `b`'s.
    pub fn yaw(&self) -> f32 {
        let target = if self.flip {
            [-self.b.normal[0], -self.b.normal[1]]
        } else {
            [self.b.normal[0], self.b.normal[1]]
        };
        target[1].atan2(target[0]) - self.a.normal[1].atan2(self.a.normal[0])
    }

    /// The doors seen from one side: `(entered, exit, yaw)`.
    pub fn oriented(&self, from_a: bool) -> (AtlasDoor, AtlasDoor, f32) {
        if from_a {
            (self.a, self.b, self.yaw())
        } else {
            (self.b, self.a, -self.yaw())
        }
    }
}

/// The atlas beyond the match's own map: extra worlds and the portals.
#[derive(Clone, Debug, Default)]
pub struct Atlas {
    /// Worlds 1.. of the atlas; world 0 is the match's map.
    pub worlds: Vec<std::sync::Arc<AtlasWorld>>,
    pub portals: Vec<AtlasPortal>,
    /// Authored wall thickness [a, b], present for interactive merged maps.
    pub portal_thickness: Vec<[f32; 2]>,
    pub portal_clearance: Vec<[PortalClearance; 2]>,
}

/// The clip one world of the atlas traces against.
#[derive(Clone, Copy)]
pub struct AtlasClip<'a> {
    pub brushes: &'a [SimBrush],
    pub bsp: &'a SimClipBsp,
    pub mesh: &'a SimClipMesh,
    pub cmodels: &'a SimClipCmodels,
}

/// The offset of world `world` in the shared coordinates.
pub fn world_offset(world: usize) -> [f32; 3] {
    WorldLayout.origin(WorldId(world))
}

/// The world a shared-coordinate position is in.
pub fn world_at(point: [f32; 3]) -> usize {
    WorldLayout.world_at(point).0
}

/// `pose` moved from shared coordinates into its world's own.
pub(crate) fn shifted(pose: &PlayerCollisionPose, offset: [f32; 3]) -> PlayerCollisionPose {
    let mut pose = pose.clone();
    pose.origin = sub(pose.origin, offset);
    for bone in &mut pose.bones {
        bone.center = sub(bone.center, offset);
    }
    pose
}

pub(crate) fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Turns `v` about z by `yaw` radians.
pub fn rotate_yaw(v: [f32; 3], yaw: f32) -> [f32; 3] {
    let (s, c) = yaw.sin_cos();
    [c * v[0] - s * v[1], s * v[0] + c * v[1], v[2]]
}

/// Carries a shared-coordinate point through a door onto the exit door.
pub fn carry_point(entered: &AtlasDoor, exit: &AtlasDoor, yaw: f32, point: [f32; 3]) -> [f32; 3] {
    PortalTransform {
        from: entered.world,
        to: exit.world,
        yaw,
        entry: entered.base,
        exit: exit.base,
    }
    .point(point)
}

impl Atlas {
    /// A trace in the world `start` is in, against that world's clip and the
    /// players standing in it.
    pub fn trace(
        &self,
        base: AtlasClip<'_>,
        players: &[PlayerCollisionPose],
        ignore: Option<ClientId>,
        start: [f32; 3],
        end: [f32; 3],
    ) -> (usize, TraceOutcome) {
        self.trace_masked(base, players, ignore, start, end, MASK_BULLET_WORLD)
    }

    pub fn trace_masked(
        &self,
        base: AtlasClip<'_>,
        players: &[PlayerCollisionPose],
        ignore: Option<ClientId>,
        start: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> (usize, TraceOutcome) {
        let world = world_at(start);
        let offset = world_offset(world);
        let local: Vec<PlayerCollisionPose>;
        let (clip, players) = match world {
            0 => (base, players),
            n => match self.worlds.get(n - 1) {
                Some(w) => (
                    AtlasClip {
                        brushes: &w.brushes,
                        bsp: &w.bsp,
                        mesh: &w.mesh,
                        cmodels: &w.cmodels,
                    },
                    {
                        local = players
                            .iter()
                            .filter(|pose| world_at(pose.origin) == n)
                            .map(|pose| shifted(pose, offset))
                            .collect();
                        &local[..]
                    },
                ),
                None => return (world, TraceOutcome::Miss { end }),
            },
        };
        let outcome = bullet_trace_with_entity_models(
            clip,
            players,
            &[],
            &BulletTraceQuery {
                start: sub(start, offset),
                end: sub(end, offset),
                mask,
                ignore,
                ignore_hit: None,
                ignore_model: None,
            },
            &|_| true,
        );
        let outcome = match outcome {
            TraceOutcome::Miss { end } => TraceOutcome::Miss {
                end: add(end, offset),
            },
            TraceOutcome::Hit {
                fraction,
                end,
                normal,
                collider,
            } => TraceOutcome::Hit {
                fraction,
                end: add(end, offset),
                normal,
                collider,
            },
            TraceOutcome::StartSolid { collider, end } => TraceOutcome::StartSolid {
                collider,
                end: add(end, offset),
            },
            invalid @ TraceOutcome::Invalid { .. } => invalid,
        };
        (world, outcome)
    }

    /// Validate the carried hull and the remaining movement before committing
    /// any world, velocity or view-angle change. Shared by authority/prediction.

    pub(crate) fn carry_player_checked_with_availability(
        &self,
        ps: &mut playerstate_iw4::PlayerState,
        from: [f32; 3],
        to: [f32; 3],
        bounds: movement_iw4::MoveBounds,
        blocked: &[[bool; 2]],
        trace: impl Fn(movement_iw4::GroundTraceInput) -> trace_iw4::Trace,
    ) -> Option<(PortalId, bool, WorldId)> {
        let mut start_center = from;
        let mut end_center = to;
        start_center[2] += ps.view_height_current * 0.5;
        end_center[2] += ps.view_height_current * 0.5;
        let (fraction, portal, from_a) = self.crossing(start_center, end_center)?;
        if blocked
            .get(portal.index())
            .is_some_and(|doors| doors[usize::from(from_a)])
        {
            ps.origin = from;
            ps.velocity = [0.; 3];
            diag::info!(
                Sim,
                "atlas: blocked route portal={portal:?} from_a={from_a} origin={from:?}"
            );
            return None;
        }
        let mut candidate = *ps;
        let result = self.carry_player(&mut candidate, from, to)?;
        let (entered, exit, yaw) = self.portals[result.0.index()].oriented(result.1);
        let entry = lerp(from, to, fraction);
        let start = carry_point(&entered, &exit, yaw, entry);
        let input = |start, end| movement_iw4::GroundTraceInput {
            start,
            end,
            mins: bounds.mins,
            maxs: bounds.maxs,
            tracemask: bounds.tracemask,
        };
        let mut last_hit = trace_iw4::Trace::default();
        let mut safe = false;
        // Door bases describe geometry, not the capsule's ground contact. The
        // destination floor may be a fraction higher; use the walking step
        // envelope rather than rejecting a valid doorway or pushing into solid.
        for lift in [0.0, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 12.0, 16.0, 18.0] {
            let raised_start = add(start, [0.0, 0.0, lift]);
            let raised_end = add(candidate.origin, [0.0, 0.0, lift]);
            let hit = trace(input(raised_start, raised_end));
            last_hit = hit;
            if hit.startsolid != 0 || hit.allsolid != 0 || hit.fraction <= 0.0 {
                continue;
            }
            let reached = lerp(raised_start, raised_end, hit.fraction);
            let down = trace(input(reached, sub(reached, [0.0, 0.0, lift])));
            if down.startsolid != 0
                || down.allsolid != 0
                || (down.fraction < 1.0 && down.normal[2] < 0.7)
            {
                continue;
            }
            let landed = lerp(reached, sub(reached, [0.0, 0.0, lift]), down.fraction);
            // A swept contact can end on the wrong side of a collision skin
            // after rounding. Never commit a destination that already overlaps.
            let stationary = trace(input(landed, landed));
            if stationary.startsolid != 0 || stationary.allsolid != 0 {
                last_hit = stationary;
                continue;
            }
            candidate.origin = landed;
            if hit.fraction < 1.0 {
                candidate.velocity = [0.0; 3];
            }
            safe = true;
            break;
        }
        if !safe {
            ps.origin = from;
            ps.velocity = [0.0; 3];
            diag::info!(
                Sim,
                "atlas: blocked exit portal={} from_a={} world={} start={start:?} end={:?} fraction={} startsolid={} allsolid={}",
                result.0,
                result.1,
                result.2,
                candidate.origin,
                last_hit.fraction,
                last_hit.startsolid,
                last_hit.allsolid
            );
            return None;
        }
        *ps = candidate;
        Some(result)
    }

    /// Apply exactly the same portal movement on authority and prediction.
    pub(crate) fn carry_player(
        &self,
        ps: &mut playerstate_iw4::PlayerState,
        from: [f32; 3],
        to: [f32; 3],
    ) -> Option<(PortalId, bool, WorldId)> {
        let center = |mut p: [f32; 3]| {
            p[2] += ps.view_height_current * 0.5;
            p
        };
        let (_, portal, from_a) = self.crossing(center(from), center(to))?;
        let (entered, exit, yaw) = self.portals[portal.index()].oriented(from_a);
        ps.origin = carry_point(&entered, &exit, yaw, ps.origin);
        ps.velocity = rotate_yaw(ps.velocity, yaw);
        ps.viewangles[1] += yaw.to_degrees();
        ps.delta_angles[1] += yaw.to_degrees();
        ps.e_flags ^= playerstate_iw4::eflags::TELEPORT;
        Some((portal, from_a, exit.world))
    }

    /// A closed destination is an opaque physical panel at the entry plane.
    /// Keep this separate from geometric crossing: callers must stop at it,
    /// rather than silently treating a closed doorway as ordinary empty space.
    pub fn unavailable_exit_hit(
        &self,
        blocked: &[[bool; 2]],
        start: [f32; 3],
        end: [f32; 3],
    ) -> Option<TraceOutcome> {
        let (fraction, portal, from_a) = self.crossing(start, end)?;
        if !blocked
            .get(portal.index())
            .is_some_and(|doors| doors[usize::from(from_a)])
        {
            return None;
        }
        let entered = self.portals[portal.index()].oriented(from_a).0;
        let base = add(entered.base, world_offset(entered.world.index()));
        let normal = if dot(sub(start, base), entered.normal) >= 0. {
            entered.normal
        } else {
            entered.normal.map(|v| -v)
        };
        Some(TraceOutcome::Hit {
            fraction,
            end: lerp(start, end, fraction),
            normal,
            collider: ColliderId::World {
                surface_flags: 0,
                contents: 1,
                glass_encoded: 0,
            },
        })
    }

    pub fn clip_unavailable_exit(
        &self,
        blocked: &[[bool; 2]],
        start: [f32; 3],
        end: [f32; 3],
        outcome: TraceOutcome,
    ) -> TraceOutcome {
        let Some(
            barrier @ TraceOutcome::Hit {
                fraction: limit, ..
            },
        ) = self.unavailable_exit_hit(blocked, start, end)
        else {
            return outcome;
        };
        match outcome {
            TraceOutcome::Miss { .. } => barrier,
            TraceOutcome::Hit { fraction, .. } if limit < fraction => barrier,
            _ => outcome,
        }
    }

    /// The first door the segment passes through inside its opening:
    /// `(fraction, portal, entered through a)`.
    pub fn crossing(&self, start: [f32; 3], end: [f32; 3]) -> Option<(f32, PortalId, bool)> {
        let world = world_at(start);
        let offset = world_offset(world);
        let (p0, p1) = (sub(start, offset), sub(end, offset));
        let mut best: Option<(f32, usize, bool)> = None;
        for (index, portal) in self.portals.iter().enumerate() {
            for (door, from_a) in [(&portal.a, true), (&portal.b, false)] {
                if door.world.index() != world {
                    continue;
                }
                let d0 = dot(sub(p0, door.base), door.normal);
                let d1 = dot(sub(p1, door.base), door.normal);
                let exit = if from_a { &portal.b } else { &portal.a };
                let plane_epsilon = (2.0 * f32::EPSILON * start[0].abs()).max(1e-3);
                // An incoming wall-door step must not escape the route when it
                // starts exactly on the plane. Carried motion leaves its one
                // accessible destination front, so it cannot re-enter this way.
                let plane_entry =
                    !door.two_sided && d0.abs() < plane_epsilon && d1 < -plane_epsilon;
                if !asset_core::atlas::portal_side_allowed(
                    door.two_sided,
                    exit.two_sided,
                    portal.flip,
                    plane_entry || d0 > 0.0,
                ) {
                    continue;
                }
                // Carry a step that ends on the plane as well. Otherwise the
                // next step starts in the precision guard and can enter a wall.
                if !plane_entry && (d0.abs() < plane_epsilon || d0 * d1 > 0.0) {
                    continue;
                }
                let fraction = if plane_entry { 0.0 } else { d0 / (d0 - d1) };
                let q = sub(lerp(p0, p1, fraction), door.base);
                let tangent = [-door.normal[1], door.normal[0], 0.0];
                let across = dot(q, tangent).abs();
                if across > door.width * 0.5 - DOOR_MARGIN
                    || q[2] < DOOR_MARGIN
                    || q[2] > door.height - DOOR_MARGIN
                {
                    continue;
                }
                if best.is_none_or(|(f, _, _)| fraction < f) {
                    best = Some((fraction, index, from_a));
                }
            }
        }
        best.map(|(fraction, index, from_a)| (fraction, PortalId(index), from_a))
    }
}
