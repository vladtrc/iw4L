use crate::bullet_collision::{LinkedBrushCollisionBrush, PLAYER_MAXS, PLAYER_MINS};
use crate::world::{
    SimBrush, SimClipBsp, SimClipMesh, clip_move_to_bmodels, clip_move_to_model_brushes, clip_trace,
};
use movement_iw4::{CollisionBackend, GroundTraceInput};
use trace_iw4::{HITTYPE_ENTITY, Trace};

pub struct PlayerBodyClip {
    pub entnum: u16,
    pub origin: [f32; 3],
}

#[derive(Clone, Copy)]
pub struct PlayerCollision<'a> {
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

impl PlayerCollision<'_> {
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
        if input.tracemask & crate::world::CONTENTS_BODY == 0 {
            return status;
        }
        let Some(moving) = crate::penetration::capsule(input, true) else {
            return Coverage::Unsupported;
        };
        for body in self
            .bodies
            .iter()
            .filter(|body| body.entnum != self.self_entnum)
        {
            let body_input = GroundTraceInput {
                start: body.origin,
                end: body.origin,
                mins: PLAYER_MINS,
                maxs: PLAYER_MAXS,
                tracemask: input.tracemask,
            };
            let Some(fixed) = crate::penetration::capsule(body_input, true) else {
                return Coverage::Unsupported;
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

    fn trace(&self, input: GroundTraceInput) -> Trace {
        let shared = input;
        let input = self.local(input);
        let world_hit = clip_trace(
            self.brushes,
            self.bsp,
            self.mesh,
            &clipmap_iw4::TraceExtents {
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
        clip_move_to_players(with_models, self.bodies, self.self_entnum, shared)
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

fn clip_move_to_players(
    mut hit: Trace,
    bodies: &[PlayerBodyClip],
    self_entnum: u16,
    input: GroundTraceInput,
) -> Trace {
    if hit.fraction == 0.0 {
        return hit;
    }
    for body in bodies {
        if body.entnum == self_entnum {
            continue;
        }
        let other = clipmap_iw4::transformed_temp_capsule_trace(
            &clipmap_iw4::TraceExtents {
                start: input.start,
                end: input.end,
                mins: input.mins,
                maxs: input.maxs,
                mask: input.tracemask,
            },
            body.origin,
            PLAYER_MINS,
            PLAYER_MAXS,
            crate::world::CONTENTS_BODY,
        );
        if other.fraction >= hit.fraction && other.startsolid == 0 && other.allsolid == 0 {
            continue;
        }
        let startsolid = hit.startsolid | other.startsolid;
        let allsolid = hit.allsolid | other.allsolid;
        if other.fraction < hit.fraction {
            hit = other;
            hit.hit_type = HITTYPE_ENTITY;
            hit.hit_id = body.entnum;
        }
        hit.startsolid = startsolid;
        hit.allsolid = allsolid;
    }
    hit
}
