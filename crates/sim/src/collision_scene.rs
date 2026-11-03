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
        crate::world::clip_trace(
            self.content.clip_brushes(),
            self.content.clip_bsp(),
            self.content.clip_mesh(),
            &clipmap_iw4::TraceExtents {
                start,
                end,
                mins,
                maxs,
                mask,
            },
            &|piece| self.world_objects.glass_is_solid(piece as u32),
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

pub(crate) fn trace_scene(
    content: &SimContent,
    world_objects: &WorldObjectState,
    capabilities: &[EntityCollisionCapabilities],
    input: movement_iw4::GroundTraceInput,
    ignore_glass: bool,
    exclude: Option<crate::AuthorityModelOwner>,
) -> trace_iw4::Trace {
    let clip_brushes = content.clip_brushes();
    let clip_bsp = content.clip_bsp();
    let clip_mesh = content.clip_mesh();
    let movement_iw4::GroundTraceInput {
        start,
        end,
        mins,
        maxs,
        tracemask: mask,
    } = input;
    if let Some(atlas) = content.atlas()
        && let Some(index) = crate::world_graph::world_at(start).checked_sub(1)
        && let Some(world) = atlas.worlds.get(index)
    {
        let offset = crate::world_graph::world_offset(index + 1);
        let local = |p: [f32; 3]| std::array::from_fn(|i| p[i] - offset[i]);
        let mut hit = clip_trace(
            &world.brushes,
            &world.bsp,
            &world.mesh,
            &clipmap_iw4::TraceExtents {
                start: local(start),
                end: local(end),
                mins,
                maxs,
                mask,
            },
            &|_| !ignore_glass,
        );
        for i in 0..3 {
            hit.endpos[i] += offset[i];
        }
        return hit;
    }
    let world_hit = clip_trace(
        clip_brushes,
        clip_bsp,
        clip_mesh,
        &clipmap_iw4::TraceExtents {
            start,
            end,
            mins,
            maxs,
            mask,
        },
        &|piece| !ignore_glass && world_objects.glass_is_solid(piece as u32),
    );
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
        movement_iw4::GroundTraceInput {
            start,
            end,
            mins,
            maxs,
            tracemask: mask,
        },
    );
    clip_move_to_model_brushes(
        hit,
        &model_movement_brushes(capabilities, |row| Some(row.owner) != exclude),
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
        let content = &self.scene.content;
        if let Some(atlas) = content.atlas()
            && let Some(index) = crate::world_graph::world_at(origin).checked_sub(1)
            && let Some(world) = atlas.worlds.get(index)
        {
            return crate::bg::player_collision::PlayerCollision {
                brushes: &world.brushes,
                bsp: &world.bsp,
                mesh: &world.mesh,
                glass_damage: &[],
                bodies,
                self_entnum,
                cmodels: &world.cmodels.models,
                linked_brushes: &[],
                model_brushes: &[],
                offset: crate::world_graph::world_offset(index + 1),
            };
        }
        crate::bg::player_collision::PlayerCollision {
            brushes: content.clip_brushes(),
            bsp: content.clip_bsp(),
            mesh: content.clip_mesh(),
            glass_damage: &self.glass,
            bodies,
            self_entnum,
            cmodels: &content.clip_cmodels().models,
            linked_brushes: &self.linked,
            model_brushes: &self.models,
            offset: [0.0; 3],
        }
    }
}
