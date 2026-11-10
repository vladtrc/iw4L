use crate::{AuthorityModelOwner, EntityCollisionCapabilities, ScriptModelId};
use movement_iw4::GroundTraceInput;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityBodyBounds {
    pub midpoint: [f32; 3],
    pub half_size: [f32; 3],
    pub contents: u32,
}
impl EntityBodyBounds {
    pub fn valid(self) -> bool {
        self.midpoint
            .into_iter()
            .chain(self.half_size)
            .all(f32::is_finite)
            && self.half_size.iter().all(|&v| v >= 0.0)
    }
    pub(crate) fn abs_bounds(self, origin: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        (
            std::array::from_fn(|i| origin[i] + self.midpoint[i] - self.half_size[i]),
            std::array::from_fn(|i| origin[i] + self.midpoint[i] + self.half_size[i]),
        )
    }
    pub(crate) fn brush(self, origin: [f32; 3]) -> crate::world::SimBrush {
        let (mins, maxs) = self.abs_bounds(origin);
        crate::world::SimBrush {
            planes: vec![
                [1., 0., 0., maxs[0]],
                [-1., 0., 0., -mins[0]],
                [0., 1., 0., maxs[1]],
                [0., -1., 0., -mins[1]],
                [0., 0., 1., maxs[2]],
                [0., 0., -1., -mins[2]],
            ],
            contents: self.contents,
            plane_surface_flags: vec![0; 6],
            glass_encoded: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EntityBodyPose {
    pub bounds: EntityBodyBounds,
    pub origin: [f32; 3],
    pub number: u16,
}

impl EntityBodyPose {
    pub(crate) fn trace(self, input: GroundTraceInput) -> trace_iw4::Trace {
        let (mins, maxs) = self.bounds.abs_bounds([0.; 3]);
        clipmap_iw4::transformed_temp_capsule_trace(
            &clipmap_iw4::TraceExtents {
                start: input.start,
                end: input.end,
                mins: input.mins,
                maxs: input.maxs,
                mask: input.tracemask,
            },
            self.origin,
            mins,
            maxs,
            self.bounds.contents,
        )
    }
}

pub(crate) fn trace_bodies(
    mut hit: trace_iw4::Trace,
    rows: &[EntityCollisionCapabilities],
    input: GroundTraceInput,
    exclude: Option<AuthorityModelOwner>,
) -> trace_iw4::Trace {
    for row in rows
        .iter()
        .filter(|row| row.solid && Some(row.owner) != exclude)
    {
        let Some(body) = row.body else {
            continue;
        };
        let mut other = body.trace(input);
        let startsolid = hit.startsolid | other.startsolid;
        let allsolid = hit.allsolid | other.allsolid;
        if other.fraction < hit.fraction {
            other.hit_type = trace_iw4::HITTYPE_ENTITY;
            other.hit_id = body.number;
            hit = other;
        }
        hit.startsolid = startsolid;
        hit.allsolid = allsolid;
    }
    hit
}

pub(crate) fn synchronize(
    rows: &mut Vec<EntityCollisionCapabilities>,
    movers: &[crate::ScriptMoverGentity],
) {
    for mover in movers.iter().filter(|m| m.body.is_some()) {
        let owner = AuthorityModelOwner::ScriptModel(mover.id);
        if !rows.iter().any(|row| row.owner == owner) {
            rows.push(EntityCollisionCapabilities::current_tick(
                owner,
                None,
                vec![],
            ));
        }
    }
    rows.sort_by_key(|row| row.owner);
    rows.retain(|row| {
        !(row.body.is_some()
            && row.dobj.is_none()
            && row.linked_brushes.is_empty()
            && !movers
                .iter()
                .any(|m| Some(m.id) == row.owner.script_model()))
    });
    for row in rows.iter_mut() {
        let mover = movers
            .iter()
            .find(|m| Some(m.id) == row.owner.script_model());
        if row.body.is_some() && mover.is_none() {
            row.solid = false;
        }
        row.body = mover.and_then(|m| {
            m.body.map(|bounds| EntityBodyPose {
                bounds,
                origin: m.state.tr_base,
                number: m.state.number as u16,
            })
        });
    }
}

pub(crate) fn owner_pose(
    id: ScriptModelId,
    bounds: EntityBodyBounds,
    origin: [f32; 3],
    number: u16,
) -> EntityCollisionCapabilities {
    let mut row = EntityCollisionCapabilities::current_tick(
        AuthorityModelOwner::ScriptModel(id),
        None,
        vec![],
    );
    row.body = Some(EntityBodyPose {
        bounds,
        origin,
        number,
    });
    row.followed_pose = Some((origin, [0.; 3]));
    row
}
