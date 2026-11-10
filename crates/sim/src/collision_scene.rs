use crate::world_objects::WorldObjectState;
use crate::{EntityCollisionCapabilities, SimContent, SimContentBuilder, Snapshot};
use std::sync::Arc;

/// Collision inputs without simulation entities, a scheduler, or a script runtime.
#[derive(Clone, Debug)]
pub struct CollisionScene {
    pub(crate) content: Arc<SimContent>,
    pub(crate) capabilities: Vec<EntityCollisionCapabilities>,
    pub(crate) world_objects: WorldObjectState,
}

impl Default for CollisionScene {
    fn default() -> Self {
        Self {
            content: SimContentBuilder::bootstrap().finish(),
            capabilities: Vec::new(),
            world_objects: WorldObjectState::default(),
        }
    }
}

impl CollisionScene {
    pub fn portal_blocked_exits(&self) -> &[[bool; 2]] {
        self.world_objects.portal_blocked_exits()
    }

    pub fn adopt_snapshot(&mut self, snapshot: &Snapshot) {
        crate::entity_body::synchronize(&mut self.capabilities, &snapshot.meta.script_movers);
        crate::presence::follow_movers(
            &mut self.capabilities,
            &snapshot.meta.script_movers,
            crate::level_time_ms(snapshot.tick),
        );
        self.world_objects
            .adopt_snapshot(&snapshot.meta.world_objects);
        self.materialize_models();
    }

    pub(crate) fn materialize_models(&mut self) {
        for row in &mut self.capabilities {
            if let Some(dobj) = &mut row.dobj {
                dobj.materialize();
            }
        }
    }

    pub fn player_collision(&self) -> PlayerCollisionScene<'_> {
        PlayerCollisionScene {
            scene: self,
            linked: self
                .capabilities
                .iter()
                .flat_map(|c| c.solid_brushes().iter().cloned())
                .collect(),
            models: model_movement_brushes(&self.capabilities, |_| true),
            glass: self.world_objects.glass_damage_pairs(),
        }
    }

    pub fn has_world_clip(&self) -> bool {
        !self.content.clip_brushes().is_empty() || self.content.clip_mesh().tables.tri_count() >= 1
    }

    pub fn collision_complete(&self) -> bool {
        self.content.collision_complete()
    }

    pub fn trace_world(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> trace_iw4::Trace {
        self.trace(start, end, mins, maxs, mask, false)
    }

    pub fn trace_sight_distance(&self, mut start: [f32; 3], mut end: [f32; 3], mask: u32) -> f32 {
        let mut travelled = 0.0;
        for _ in 0..16 {
            let crossing = self
                .content
                .atlas()
                .and_then(|atlas| atlas.crossing(start, end));
            let leg_end = crossing.map_or(end, |(fraction, _, _)| {
                std::array::from_fn(|axis| start[axis] + (end[axis] - start[axis]) * fraction)
            });
            let length = (0..3)
                .map(|axis| (leg_end[axis] - start[axis]).powi(2))
                .sum::<f32>()
                .sqrt();
            let hit = self.trace_world(start, leg_end, [0.0; 3], [0.0; 3], mask);
            if hit.startsolid != 0 {
                return travelled;
            }
            if hit.fraction < 1.0 {
                return travelled + length * hit.fraction;
            }
            travelled += length;
            let Some((_, portal, from_a)) = crossing else {
                return travelled;
            };
            if mask & 1 != 0
                && self
                    .world_objects
                    .portal_blocked_exits()
                    .get(portal.index())
                    .is_some_and(|doors| doors[usize::from(from_a)])
            {
                return travelled;
            }
            let atlas = self.content.atlas().unwrap();
            let (entered, exit, yaw) = atlas.portals[portal.index()].oriented(from_a);
            start = crate::world_graph::carry_point(&entered, &exit, yaw, leg_end);
            end = crate::world_graph::carry_point(&entered, &exit, yaw, end);
        }
        travelled
    }

    pub fn trace_navigation(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> trace_iw4::Trace {
        self.trace(start, end, mins, maxs, mask, true)
    }

    fn trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
        ignore_glass: bool,
    ) -> trace_iw4::Trace {
        trace_scene(
            &self.content,
            &self.world_objects,
            &self.capabilities,
            movement_iw4::GroundTraceInput {
                start,
                end,
                mins,
                maxs,
                tracemask: mask,
            },
            ignore_glass,
            None,
        )
    }

    pub fn trace_static_world(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> trace_iw4::Trace {
        SceneCollision::of(&self.content, &self.world_objects).trace_static(
            movement_iw4::GroundTraceInput {
                start,
                end,
                mins,
                maxs,
                tracemask: mask,
            },
            false,
        )
    }
}

use crate::SimBrush;
use crate::world::{clip_move_to_bmodels, clip_move_to_model_brushes, clip_trace};

pub(crate) fn model_movement_brushes(
    capabilities: &[EntityCollisionCapabilities],
    keep: impl Fn(&EntityCollisionCapabilities) -> bool,
) -> Vec<SimBrush> {
    let mut brushes = Vec::new();
    for row in capabilities.iter().filter(|row| row.solid && keep(row)) {
        if let Some(body) = row.body {
            brushes.push(body.bounds.brush(body.origin));
            continue;
        }
        let Some(dobj) = &row.dobj else { continue };
        let Some(capability) = &dobj.capability else {
            continue;
        };
        if capability.movement_brushes.is_empty() {
            continue;
        }
        let plane_transform = dobj.world_from_model.inverse().transpose();
        for brush in &capability.movement_brushes {
            let planes = brush
                .planes
                .iter()
                .map(|plane| {
                    let plane =
                        plane_transform * glam::Vec4::new(plane[0], plane[1], plane[2], -plane[3]);
                    let length = plane.truncate().length();
                    [
                        plane.x / length,
                        plane.y / length,
                        plane.z / length,
                        -plane.w / length,
                    ]
                })
                .collect();
            brushes.push(SimBrush {
                planes,
                contents: brush.contents,
                plane_surface_flags: brush.plane_surface_flags.clone(),
                glass_encoded: 0,
            });
        }
    }
    brushes
}

#[derive(Clone, Copy)]
pub(crate) enum SceneCollision<'a> {
    Portal {
        content: &'a SimContent,
        objects: &'a WorldObjectState,
    },
    Shared {
        content: &'a SimContent,
        objects: &'a WorldObjectState,
        frames: &'a [asset_core::world_frame::WorldFrame],
    },
}

impl<'a> SceneCollision<'a> {
    pub(crate) fn of(content: &'a SimContent, objects: &'a WorldObjectState) -> Self {
        match content.atlas().map(|atlas| &atlas.space) {
            Some(asset_core::world_frame::SceneSpace::Shared(frames)) => Self::Shared {
                content,
                objects,
                frames,
            },
            _ => Self::Portal { content, objects },
        }
    }

    pub(crate) fn is_shared(self) -> bool {
        matches!(self, Self::Shared { .. })
    }

    fn skips_dynamic(self, start: [f32; 3]) -> bool {
        matches!(self, Self::Portal { .. })
            && asset_core::world_frame::SimPoint::from_slab(start)
                .world()
                .index()
                != 0
    }

    fn trace_static(
        self,
        input: movement_iw4::GroundTraceInput,
        ignore_glass: bool,
    ) -> trace_iw4::Trace {
        match self {
            Self::Shared {
                content,
                objects,
                frames,
            } => trace_shared_geometry(content, objects, frames, input, ignore_glass),
            Self::Portal { content, objects } => {
                let world = asset_core::world_frame::SimPoint::from_slab(input.start)
                    .world()
                    .index();
                let clip = content
                    .world_clip(world)
                    .unwrap_or_else(|| content.root_clip());
                let offset = crate::world_graph::world_offset(world);
                let local = |p: [f32; 3]| std::array::from_fn(|i| p[i] - offset[i]);
                let mut hit = clip_trace(
                    clip.brushes,
                    clip.bsp,
                    clip.mesh,
                    &clipmap_iw4::TraceExtents {
                        start: local(input.start),
                        end: local(input.end),
                        mins: input.mins,
                        maxs: input.maxs,
                        mask: input.tracemask,
                    },
                    &|piece| !ignore_glass && (world != 0 || objects.glass_is_solid(piece as u32)),
                );
                for (i, axis) in offset.iter().enumerate() {
                    hit.endpos[i] += *axis;
                }
                hit
            }
        }
    }
}

pub(crate) fn trace_shared_geometry(
    content: &SimContent,
    world_objects: &WorldObjectState,
    frames: &[asset_core::world_frame::WorldFrame],
    input: movement_iw4::GroundTraceInput,
    ignore_glass: bool,
) -> trace_iw4::Trace {
    let atlas = content.atlas().expect("shared geometry requires atlas");
    assert_eq!(frames.len(), atlas.worlds.len());
    let mut best = trace_iw4::Trace {
        fraction: 1.0,
        endpos: input.end,
        ..Default::default()
    };
    for (world, &frame) in frames.iter().enumerate() {
        let clip = content.world_clip(world).expect("placement has a world");
        let (mins, maxs) = frame.box_to_local(input.mins, input.maxs);
        let mut hit = clip_trace(
            clip.brushes,
            clip.bsp,
            clip.mesh,
            &clipmap_iw4::TraceExtents {
                start: frame.point_to_local(input.start),
                end: frame.point_to_local(input.end),
                mins,
                maxs,
                mask: input.tracemask,
            },
            &|piece| !ignore_glass && (world != 0 || world_objects.glass_is_solid(piece as u32)),
        );
        hit.endpos = frame.point_to_scene(hit.endpos);
        hit.normal = frame.vector_to_scene(hit.normal);
        // Solid flags describe the union even when a different world supplied
        // the nearest surface. Keep stable world order for equal fractions.
        let startsolid = best.startsolid | hit.startsolid;
        let allsolid = best.allsolid | hit.allsolid;
        if hit.fraction < best.fraction || (hit.allsolid != 0 && best.allsolid == 0) {
            best = hit;
        }
        best.startsolid = startsolid;
        best.allsolid = allsolid;
    }
    best
}

#[derive(Clone, Copy)]
pub(crate) struct NativePlacement {
    world: asset_core::atlas::WorldId,
    frame: asset_core::world_frame::WorldFrame,
}

impl NativePlacement {
    pub(crate) fn of(
        frames: &[asset_core::world_frame::WorldFrame],
        world: asset_core::atlas::WorldId,
    ) -> Option<Self> {
        Some(Self {
            world,
            frame: *frames.get(world.index())?,
        })
    }

    fn to_scene(self, point: [f32; 3]) -> [f32; 3] {
        self.frame.point_to_scene(
            asset_core::world_frame::SimPoint::new(self.world, point)
                .to_local()
                .xyz(),
        )
    }

    pub(crate) fn input_to_scene(
        self,
        input: movement_iw4::GroundTraceInput,
    ) -> movement_iw4::GroundTraceInput {
        let mid = std::array::from_fn(|i| (input.mins[i] + input.maxs[i]) * 0.5);
        let half = std::array::from_fn(|i| (input.maxs[i] - input.mins[i]) * 0.5);
        let (_, half) = self.frame.bounds_to_scene([0.0; 3], half);
        let mid = self.frame.vector_to_scene(mid);
        movement_iw4::GroundTraceInput {
            start: self.to_scene(input.start),
            end: self.to_scene(input.end),
            mins: std::array::from_fn(|i| mid[i] - half[i]),
            maxs: std::array::from_fn(|i| mid[i] + half[i]),
            ..input
        }
    }

    pub(crate) fn hit_to_native(self, mut hit: trace_iw4::Trace) -> trace_iw4::Trace {
        hit.normal = self.frame.vector_to_local(hit.normal);
        hit.endpos = asset_core::world_frame::LocalPoint::new(
            self.world,
            self.frame.point_to_local(hit.endpos),
        )
        .to_sim()
        .xyz();
        hit
    }
}

pub(crate) fn trace_native_scene(
    content: &SimContent,
    world_objects: &WorldObjectState,
    capabilities: &[EntityCollisionCapabilities],
    input: movement_iw4::GroundTraceInput,
    ignore_glass: bool,
    exclude: Option<crate::AuthorityModelOwner>,
) -> trace_iw4::Trace {
    let scene = |input| {
        trace_scene(
            content,
            world_objects,
            capabilities,
            input,
            ignore_glass,
            exclude,
        )
    };
    let placement = match SceneCollision::of(content, world_objects) {
        SceneCollision::Shared { frames, .. } => NativePlacement::of(
            frames,
            asset_core::world_frame::SimPoint::from_slab(input.start).world(),
        ),
        SceneCollision::Portal { .. } => None,
    };
    match placement {
        Some(placement) => placement.hit_to_native(scene(placement.input_to_scene(input))),
        None => scene(input),
    }
}

pub(crate) fn trace_scene(
    content: &SimContent,
    world_objects: &WorldObjectState,
    capabilities: &[EntityCollisionCapabilities],
    input: movement_iw4::GroundTraceInput,
    ignore_glass: bool,
    exclude: Option<crate::AuthorityModelOwner>,
) -> trace_iw4::Trace {
    let space = SceneCollision::of(content, world_objects);
    let world_hit = space.trace_static(input, ignore_glass);
    if space.skips_dynamic(input.start) {
        return world_hit;
    }
    let clip_brushes = content.clip_brushes();
    let clip_bsp = content.clip_bsp();
    let linked = capabilities
        .iter()
        .filter(|c| Some(c.owner) != exclude)
        .flat_map(|c| c.solid_brushes().iter());
    let hit = clip_move_to_bmodels(
        world_hit,
        &content.clip_cmodels().models,
        &clip_bsp.leafbrushes,
        clip_brushes,
        linked,
        input,
    );
    let hit = crate::entity_body::trace_bodies(hit, capabilities, input, exclude);
    clip_move_to_model_brushes(
        hit,
        &model_movement_brushes(capabilities, |row| {
            row.body.is_none() && Some(row.owner) != exclude
        }),
        input,
    )
}

pub struct PlayerCollisionScene<'a> {
    scene: &'a CollisionScene,
    linked: Vec<crate::bullet_collision::LinkedBrushCollisionBrush>,
    models: Vec<SimBrush>,
    glass: Vec<(u32, u16)>,
}

impl PlayerCollisionScene<'_> {
    pub fn backend<'a>(
        &'a self,
        bodies: &'a [crate::bg::player_collision::PlayerBodyClip],
        self_entnum: u16,
        origin: [f32; 3],
    ) -> crate::bg::player_collision::PlayerCollision<'a> {
        let dynamics = crate::bg::player_collision::RootDynamics {
            glass_damage: &self.glass,
            linked: &self.linked,
            models: &self.models,
        };
        crate::bg::player_collision::PlayerCollision::for_scene(
            &self.scene.content,
            &self.scene.world_objects,
            origin,
            dynamics,
            bodies,
            self_entnum,
        )
    }
}
