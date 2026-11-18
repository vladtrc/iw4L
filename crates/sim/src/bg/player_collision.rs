use crate::bullet_collision::{LinkedBrushCollisionBrush, PLAYER_MAXS, PLAYER_MINS};
use crate::world::{
    SimBrush, SimClipBsp, SimClipMesh, clip_move_to_bmodels, clip_move_to_model_brushes, clip_trace,
};
use movement_iw4::{CollisionBackend, GroundTraceInput};
use trace_iw4::Trace;

pub struct PlayerBodyClip {
    pub entnum: u16,
    pub origin: [f32; 3],
}

#[derive(Clone, Copy)]
pub struct SharedMovementCollision<'a> {
    pub content: &'a crate::SimContent,
    pub objects: &'a crate::world_objects::WorldObjectState,
    pub world: usize,
    pub linked: &'a [LinkedBrushCollisionBrush],
    pub models: &'a [SimBrush],
}

use crate::penetration::{frame_origin64, frame_vector64};

impl SharedMovementCollision<'_> {
    pub fn penetrations(
        self,
        input: GroundTraceInput,
        out: &mut movement_iw4::recovery::ContactBuffer,
    ) -> movement_iw4::recovery::Coverage {
        use movement_iw4::penetration::{Capsule, Contact};
        use movement_iw4::recovery::{ContactBuffer, Coverage};
        let atlas = self
            .content
            .atlas()
            .expect("shared movement requires atlas");
        let asset_core::world_frame::SceneSpace::Shared(frames) = &atlas.space else {
            return Coverage::Unsupported;
        };
        let actor = frames[self.world];
        let offset = crate::world_graph::world_offset(self.world);
        let native = GroundTraceInput {
            start: std::array::from_fn(|i| input.start[i] - offset[i]),
            end: std::array::from_fn(|i| input.end[i] - offset[i]),
            ..input
        };
        let Some(cap) = crate::penetration::capsule(native, false) else {
            return Coverage::Unsupported;
        };
        // Keep capsule arithmetic in f64; only the BSP broadphase uses f32.
        let scene_a = frame_vector64(actor, cap.a, false) + frame_origin64(actor);
        let scene_b = frame_vector64(actor, cap.b, false) + frame_origin64(actor);
        let placed = crate::collision_scene::NativePlacement::of(
            frames,
            asset_core::atlas::WorldId(self.world),
        )
        .expect("shared movement world has a placement")
        .input_to_scene(input);
        for (world, &frame) in frames.iter().enumerate() {
            let glass = |piece| world != 0 || self.objects.glass_is_solid(piece as u32);
            let clip = self
                .content
                .world_clip(world)
                .expect("placement has a world");
            let root = world == 0;
            let scene = crate::penetration::RecoveryScene {
                brushes: clip.brushes,
                bsp: clip.bsp,
                mesh: clip.mesh,
                cmodels: if root { &clip.cmodels.models } else { &[] },
                linked: if root { self.linked } else { &[] },
                models: if root { self.models } else { &[] },
                glass_is_solid: &glass,
            };
            let local_cap = Capsule {
                a: frame_vector64(frame, scene_a - frame_origin64(frame), true),
                b: frame_vector64(frame, scene_b - frame_origin64(frame), true),
                radius: cap.radius,
            };
            let (mins, maxs) = frame.box_to_local(placed.mins, placed.maxs);
            let start = frame.point_to_local(placed.start);
            let mut contacts = ContactBuffer::new();
            let status = crate::penetration::contacts_with_capsule(
                &scene,
                GroundTraceInput {
                    start,
                    end: start,
                    mins,
                    maxs,
                    ..input
                },
                local_cap,
                &mut contacts,
            );
            if status != Coverage::Complete {
                return status;
            }
            for contact in contacts.as_slice() {
                let normal =
                    frame_vector64(actor, frame_vector64(frame, contact.normal, false), true);
                if !out.push(Contact {
                    normal,
                    depth: contact.depth,
                }) {
                    return Coverage::Overflow;
                }
            }
        }
        Coverage::Complete
    }

    pub fn trace(self, input: GroundTraceInput) -> Trace {
        let atlas = self
            .content
            .atlas()
            .expect("shared movement requires atlas");
        let asset_core::world_frame::SceneSpace::Shared(frames) = &atlas.space else {
            panic!("shared movement requires placements")
        };
        let placement = crate::collision_scene::NativePlacement::of(
            frames,
            asset_core::atlas::WorldId(self.world),
        )
        .expect("shared movement world has a placement");
        let input = placement.input_to_scene(input);
        let hit = crate::collision_scene::trace_shared_geometry(
            self.content,
            self.objects,
            frames,
            input,
            false,
        );
        let hit = clip_move_to_bmodels(
            hit,
            &self.content.clip_cmodels().models,
            &self.content.clip_bsp().leafbrushes,
            self.content.clip_brushes(),
            self.linked,
            input,
        );
        placement.hit_to_native(clip_move_to_model_brushes(hit, self.models, input))
    }
}

#[derive(Clone, Copy)]
pub struct PlayerCollision<'a> {
    pub entity_bodies: Option<crate::EntityBodyCollision<'a>>,
    pub shared: Option<SharedMovementCollision<'a>>,
    pub brushes: &'a [SimBrush],
    pub bsp: &'a SimClipBsp,
    pub mesh: &'a SimClipMesh,
    pub glass_damage: &'a [(u32, u16)],
    pub bodies: &'a [PlayerBodyClip],
    pub self_entnum: u16,
    pub cmodels: &'a [clipmap_iw4::ClipCmodel],
    pub linked_brushes: &'a [LinkedBrushCollisionBrush],
    pub model_brushes: &'a [SimBrush],
    /// Where the clip's world sits in the shared coordinates: an atlas world
    /// beyond the match's map traces its own clip, shifted to its slab.
    pub offset: [f32; 3],
}

#[derive(Clone, Copy, Default)]
pub struct RootDynamics<'a> {
    pub entity_bodies: Option<crate::EntityBodyCollision<'a>>,
    pub glass_damage: &'a [(u32, u16)],
    pub linked: &'a [LinkedBrushCollisionBrush],
    pub models: &'a [SimBrush],
}

impl<'a> PlayerCollision<'a> {
    pub(crate) fn world(
        clip: crate::world_graph::AtlasClip<'a>,
        world: usize,
        dynamics: RootDynamics<'a>,
        bodies: &'a [PlayerBodyClip],
        self_entnum: u16,
    ) -> Self {
        Self {
            shared: None,
            entity_bodies: dynamics.entity_bodies,
            brushes: clip.brushes,
            bsp: clip.bsp,
            mesh: clip.mesh,
            glass_damage: dynamics.glass_damage,
            bodies,
            self_entnum,
            cmodels: &clip.cmodels.models,
            linked_brushes: dynamics.linked,
            model_brushes: dynamics.models,
            offset: crate::world_graph::world_offset(world),
        }
    }

    pub(crate) fn root(
        content: &'a crate::SimContent,
        dynamics: RootDynamics<'a>,
        bodies: &'a [PlayerBodyClip],
        self_entnum: u16,
    ) -> Self {
        let mut backend = Self::world(content.root_clip(), 0, dynamics, bodies, self_entnum);
        backend
            .entity_bodies
            .get_or_insert_with(|| crate::EntityBodyCollision::new(content, &[]));
        backend
    }

    pub(crate) fn at(
        content: &'a crate::SimContent,
        origin: [f32; 3],
        dynamics: RootDynamics<'a>,
        bodies: &'a [PlayerBodyClip],
        self_entnum: u16,
    ) -> Self {
        let world = asset_core::world_frame::SimPoint::from_slab(origin)
            .world()
            .index();
        let mut backend = match content.world_clip(world).filter(|_| world != 0) {
            Some(clip) => Self::world(
                clip,
                world,
                RootDynamics {
                    entity_bodies: dynamics.entity_bodies,
                    ..Default::default()
                },
                bodies,
                self_entnum,
            ),
            None => Self::root(content, dynamics, bodies, self_entnum),
        };
        backend
            .entity_bodies
            .get_or_insert_with(|| crate::EntityBodyCollision::new(content, &[]));
        backend
    }

    pub(crate) fn for_scene(
        content: &'a crate::SimContent,
        objects: &'a crate::world_objects::WorldObjectState,
        origin: [f32; 3],
        dynamics: RootDynamics<'a>,
        bodies: &'a [PlayerBodyClip],
        self_entnum: u16,
    ) -> Self {
        let mut backend = Self::at(content, origin, dynamics, bodies, self_entnum);
        if crate::collision_scene::SceneCollision::of(content, objects).is_shared() {
            backend.shared = Some(SharedMovementCollision {
                content,
                objects,
                world: backend_world(origin),
                linked: dynamics.linked,
                models: dynamics.models,
            });
        }
        backend
    }
}

fn backend_world(origin: [f32; 3]) -> usize {
    asset_core::world_frame::SimPoint::from_slab(origin)
        .world()
        .index()
}

impl PlayerBodyClip {
    fn pose(&self) -> crate::entity_body::EntityBodyPose {
        crate::entity_body::EntityBodyPose {
            bounds: crate::EntityBodyBounds {
                midpoint: std::array::from_fn(|i| (PLAYER_MINS[i] + PLAYER_MAXS[i]) * 0.5),
                half_size: std::array::from_fn(|i| (PLAYER_MAXS[i] - PLAYER_MINS[i]) * 0.5),
                contents: crate::world::CONTENTS_BODY,
            },
            origin: self.origin,
            number: self.entnum,
        }
    }
}

impl PlayerCollision<'_> {
    fn body_collision(&self) -> crate::EntityBodyCollision<'_> {
        self.entity_bodies.unwrap_or_else(|| {
            self.shared.map_or_else(
                || crate::EntityBodyCollision::from_atlas(None, &[]),
                |shared| crate::EntityBodyCollision::new(shared.content, &[]),
            )
        })
    }

    fn trace_players(&self, mut hit: Trace, input: GroundTraceInput) -> Trace {
        let collision = self.body_collision();
        for body in self
            .bodies
            .iter()
            .filter(|body| body.entnum != self.self_entnum)
        {
            hit = collision.clip_pose(
                hit,
                body.pose(),
                input,
                crate::entity_body::BodyQuerySpace::Simulation,
            );
        }
        hit
    }

    fn player_penetrations(
        &self,
        input: GroundTraceInput,
        contacts: &mut movement_iw4::recovery::ContactBuffer,
    ) -> movement_iw4::recovery::Coverage {
        use movement_iw4::recovery::Coverage;
        if input.tracemask & crate::world::CONTENTS_BODY == 0 {
            return Coverage::Complete;
        }
        let Some(moving) = crate::penetration::capsule(input, true) else {
            return Coverage::Unsupported;
        };
        let collision = self.body_collision();
        for body in self
            .bodies
            .iter()
            .filter(|body| body.entnum != self.self_entnum)
        {
            let fixed = match collision.capsule(
                body.pose(),
                input,
                crate::entity_body::BodyQuerySpace::Simulation,
            ) {
                Ok(Some(cap)) => cap,
                Ok(None) => continue,
                Err(()) => return Coverage::Unsupported,
            };
            if let Err(status) = crate::penetration::append(
                movement_iw4::penetration::capsule_capsule(moving, fixed),
                contacts,
            ) {
                return status;
            }
        }
        Coverage::Complete
    }

    fn trace_entity_bodies(&self, hit: Trace, input: GroundTraceInput) -> Trace {
        self.entity_bodies.map_or(hit, |bodies| {
            bodies.trace(
                hit,
                input,
                crate::entity_body::BodyQuerySpace::Simulation,
                self.self_entnum,
            )
        })
    }

    fn local(&self, input: GroundTraceInput) -> GroundTraceInput {
        let shift = |p: [f32; 3]| std::array::from_fn(|i| p[i] - self.offset[i]);
        GroundTraceInput {
            start: shift(input.start),
            end: shift(input.end),
            ..input
        }
    }
}

impl CollisionBackend for PlayerCollision<'_> {
    fn penetrations(
        &self,
        input: GroundTraceInput,
        contacts: &mut movement_iw4::recovery::ContactBuffer,
    ) -> movement_iw4::recovery::Coverage {
        use movement_iw4::recovery::Coverage;
        if let Some(bodies) = self.entity_bodies {
            let status = bodies.penetrations(input, self.self_entnum, contacts);
            if status != Coverage::Complete {
                return status;
            }
        }
        if let Some(shared) = self.shared {
            let status = shared.penetrations(input, contacts);
            if status != Coverage::Complete {
                return status;
            }
            return self.player_penetrations(input, contacts);
        }
        let glass = |piece| {
            let damage = self
                .glass_damage
                .iter()
                .find(|(id, _)| *id == u32::from(piece))
                .map_or(0, |(_, d)| *d);
            crate::world_objects::glass_piece_is_solid(damage)
        };
        let scene = crate::penetration::RecoveryScene {
            brushes: self.brushes,
            bsp: self.bsp,
            mesh: self.mesh,
            cmodels: self.cmodels,
            linked: self.linked_brushes,
            models: self.model_brushes,
            glass_is_solid: &glass,
        };
        let status = crate::penetration::contacts(&scene, self.local(input), contacts);
        if status != Coverage::Complete {
            return status;
        }
        self.player_penetrations(input, contacts)
    }

    fn trace(&self, input: GroundTraceInput) -> Trace {
        if let Some(shared) = self.shared {
            let hit = shared.trace(input);
            let hit = self.trace_players(hit, input);
            return self.trace_entity_bodies(hit, input);
        }
        let shared = input;
        let input = self.local(input);
        let world_hit = clip_trace(
            self.brushes,
            self.bsp,
            self.mesh,
            &clipmap_iw4::TraceExtents {
                shape: trace_iw4::TraceShape::Capsule,
                start: input.start,
                end: input.end,
                mins: input.mins,
                maxs: input.maxs,
                mask: input.tracemask,
            },
            &|piece| {
                let damage = self
                    .glass_damage
                    .iter()
                    .find(|(id, _)| *id == u32::from(piece))
                    .map_or(0, |(_, d)| *d);
                crate::world_objects::glass_piece_is_solid(damage)
            },
        );
        let with_bmodels = clip_move_to_bmodels(
            world_hit,
            self.cmodels,
            &self.bsp.leafbrushes,
            self.brushes,
            self.linked_brushes,
            input,
        );
        let mut with_models = clip_move_to_model_brushes(with_bmodels, self.model_brushes, input);
        for (axis, offset) in with_models.endpos.iter_mut().zip(self.offset) {
            *axis += offset;
        }
        self.trace_entity_bodies(self.trace_players(with_models, shared), shared)
    }
}

impl movement_iw4::MantleCapsuleTrace for PlayerCollision<'_> {
    fn trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        contentmask: u32,
    ) -> Trace {
        CollisionBackend::trace(
            self,
            GroundTraceInput {
                start,
                end,
                mins,
                maxs,
                tracemask: contentmask,
            },
        )
    }
}
