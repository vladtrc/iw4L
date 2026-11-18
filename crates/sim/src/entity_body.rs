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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EntityBodyPose {
    pub bounds: EntityBodyBounds,
    pub origin: [f32; 3],
    pub number: u16,
}

impl EntityBodyPose {
    pub(crate) fn clip(
        self,
        content: &crate::SimContent,
        hit: trace_iw4::Trace,
        input: GroundTraceInput,
    ) -> trace_iw4::Trace {
        EntityBodyCollision::new(content, &[]).clip_pose(
            hit,
            self,
            input,
            BodyQuerySpace::Simulation,
        )
    }

    fn trace_native(self, input: GroundTraceInput) -> trace_iw4::Trace {
        let (mins, maxs) = self.bounds.abs_bounds([0.; 3]);
        clipmap_iw4::transformed_temp_capsule_trace(
            &clipmap_iw4::TraceExtents {
                shape: trace_iw4::TraceShape::Capsule,
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

#[derive(Clone, Copy)]
pub(crate) enum BodyQuerySpace {
    Scene,
    Simulation,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EntityBodyClip {
    pub(crate) owner: AuthorityModelOwner,
    pub(crate) body: EntityBodyPose,
}

pub(crate) fn collision_bodies(rows: &[EntityCollisionCapabilities]) -> Vec<EntityBodyClip> {
    rows.iter()
        .filter(|row| row.solid)
        .filter_map(|row| {
            Some(EntityBodyClip {
                owner: row.owner,
                body: row.body?,
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
pub struct EntityBodyCollision<'a> {
    atlas: Option<&'a crate::world_graph::Atlas>,
    rows: &'a [EntityBodyClip],
    exclude: &'a [AuthorityModelOwner],
}

impl<'a> EntityBodyCollision<'a> {
    pub(crate) fn new(content: &'a crate::SimContent, rows: &'a [EntityBodyClip]) -> Self {
        Self::from_atlas(content.atlas().map(std::sync::Arc::as_ref), rows)
    }

    pub(crate) fn from_atlas(
        atlas: Option<&'a crate::world_graph::Atlas>,
        rows: &'a [EntityBodyClip],
    ) -> Self {
        Self {
            atlas,
            rows,
            exclude: &[],
        }
    }

    pub(crate) fn excluding(mut self, exclude: &'a [AuthorityModelOwner]) -> Self {
        self.exclude = exclude;
        self
    }

    fn bodies(self, mask: u32, self_entnum: u16) -> impl Iterator<Item = EntityBodyPose> + 'a {
        self.rows
            .iter()
            .filter(move |row| !self.exclude.contains(&row.owner))
            .map(|row| row.body)
            .filter(move |body| body.number != self_entnum && body.bounds.contents & mask != 0)
    }

    pub(crate) fn capsule(
        self,
        body: EntityBodyPose,
        input: GroundTraceInput,
        space: BodyQuerySpace,
    ) -> Result<Option<movement_iw4::penetration::Capsule>, ()> {
        use crate::penetration::{frame_origin64, frame_vector64};
        use asset_core::world_frame::{SceneSpace, SimPoint};
        use movement_iw4::penetration::{Capsule, Vec3};
        let (mins, maxs) = body.bounds.abs_bounds([0.; 3]);
        let cap = crate::penetration::capsule(
            GroundTraceInput {
                start: body.origin,
                end: body.origin,
                mins,
                maxs,
                tracemask: input.tracemask,
            },
            true,
        )
        .ok_or(())?;
        let Some(atlas) = self.atlas else {
            return Ok(Some(cap));
        };
        let source_world = SimPoint::from_slab(body.origin).world().index();
        let SceneSpace::Shared(frames) = &atlas.space else {
            let target_world = SimPoint::from_slab(input.start).world().index();
            return Ok((source_world == target_world).then_some(cap));
        };
        let source = *frames.get(source_world).ok_or(())?;
        let source_offset: Vec3 = crate::world_graph::world_offset(source_world)
            .map(f64::from)
            .into();
        let target = match space {
            BodyQuerySpace::Scene => None,
            BodyQuerySpace::Simulation => {
                let target_world = SimPoint::from_slab(input.start).world().index();
                let target = *frames.get(target_world).ok_or(())?;
                let offset: Vec3 = crate::world_graph::world_offset(target_world)
                    .map(f64::from)
                    .into();
                Some((target, offset))
            }
        };
        let point = |v| {
            let scene = frame_vector64(source, v - source_offset, false) + frame_origin64(source);
            target.map_or(scene, |(frame, offset)| {
                frame_vector64(frame, scene - frame_origin64(frame), true) + offset
            })
        };
        Ok(Some(Capsule {
            a: point(cap.a),
            b: point(cap.b),
            radius: cap.radius,
        }))
    }

    pub(crate) fn trace(
        self,
        mut hit: trace_iw4::Trace,
        input: GroundTraceInput,
        space: BodyQuerySpace,
        self_entnum: u16,
    ) -> trace_iw4::Trace {
        if crate::penetration::capsule(input, true).is_none() {
            return trace_iw4::Trace {
                fraction: 0.0,
                startsolid: 1,
                endpos: input.start,
                ..hit
            };
        }
        for body in self.bodies(input.tracemask, self_entnum) {
            hit = self.clip_pose(hit, body, input, space);
        }
        hit
    }

    pub(crate) fn trace_shaped(
        self,
        mut hit: trace_iw4::Trace,
        input: GroundTraceInput,
        space: BodyQuerySpace,
        self_entnum: u16,
        shape: trace_iw4::TraceShape,
    ) -> trace_iw4::Trace {
        let trace_iw4::TraceShape::Box { half_size, axes } = shape else {
            return self.trace(hit, input, space, self_entnum);
        };
        for body in self.bodies(input.tracemask, self_entnum) {
            hit = self.clip_box_pose(hit, body, input, space, half_size, axes);
        }
        hit
    }

    fn clip_box_pose(
        self,
        mut hit: trace_iw4::Trace,
        body: EntityBodyPose,
        input: GroundTraceInput,
        space: BodyQuerySpace,
        half: [f32; 3],
        axes: [[f32; 3]; 3],
    ) -> trace_iw4::Trace {
        let blocked = |hit| trace_iw4::Trace {
            fraction: 0.,
            startsolid: 1,
            endpos: input.start,
            ..hit
        };
        let fixed = match self.capsule(body, input, space) {
            Ok(Some(fixed)) => fixed,
            Ok(None) => return hit,
            Err(()) => return blocked(hit),
        };
        let center = std::array::from_fn::<_, 3, _>(|i| {
            f64::from(input.start[i]) + (f64::from(input.mins[i]) + f64::from(input.maxs[i])) * 0.5
        });
        let project =
            |v: [f64; 3]| axes.map(|axis| (0..3).map(|i| f64::from(axis[i]) * v[i]).sum());
        let relative = |v: [f64; 3]| project(std::array::from_fn(|i| v[i] - center[i]));
        let delta = project(std::array::from_fn(|i| {
            f64::from(input.end[i]) - f64::from(input.start[i])
        }));
        let sweep = match clipmap_iw4::sweep_box_capsule(
            [0.; 3],
            delta,
            half.map(f64::from),
            relative(fixed.a.into()),
            relative(fixed.b.into()),
            fixed.radius,
        ) {
            Ok(Some(sweep)) => sweep,
            Ok(None) => return hit,
            Err(_) => return blocked(hit),
        };
        let startsolid = hit.startsolid | u8::from(sweep.startsolid);
        let allsolid = hit.allsolid | u8::from(sweep.allsolid);
        if sweep.fraction < f64::from(hit.fraction) {
            hit.fraction = sweep.fraction as f32;
            hit.normal = std::array::from_fn(|i| {
                (0..3)
                    .map(|j| f64::from(axes[j][i]) * sweep.normal[j])
                    .sum::<f64>() as f32
            });
            hit.endpos = std::array::from_fn(|i| {
                (f64::from(input.start[i])
                    + (f64::from(input.end[i]) - f64::from(input.start[i])) * sweep.fraction)
                    as f32
            });
            hit.contents = body.bounds.contents;
            hit.hit_type = trace_iw4::HITTYPE_ENTITY;
            hit.hit_id = body.number;
            hit.walkable = u8::from(!sweep.startsolid && hit.normal[2] >= 0.7);
        }
        hit.startsolid = startsolid;
        hit.allsolid = allsolid;
        hit
    }

    pub(crate) fn clip_pose(
        self,
        mut hit: trace_iw4::Trace,
        body: EntityBodyPose,
        input: GroundTraceInput,
        space: BodyQuerySpace,
    ) -> trace_iw4::Trace {
        use movement_iw4::penetration::{Vec3, sweep_capsules};
        if body.bounds.contents & input.tracemask == 0 {
            return hit;
        }
        let blocked = |hit| trace_iw4::Trace {
            fraction: 0.0,
            startsolid: 1,
            endpos: input.start,
            ..hit
        };
        let Some(moving) = crate::penetration::capsule(input, true) else {
            return blocked(hit);
        };
        let fixed = match self.capsule(body, input, space) {
            Ok(Some(cap)) => cap,
            Ok(None) => return hit,
            Err(()) => return blocked(hit),
        };
        let native = !matches!(
            self.atlas.map(|atlas| &atlas.space),
            Some(asset_core::world_frame::SceneSpace::Shared(_))
        );
        let mut other = if native {
            body.trace_native(input)
        } else {
            let delta: Vec3 = std::array::from_fn::<_, 3, _>(|i| {
                f64::from(input.end[i]) - f64::from(input.start[i])
            })
            .into();
            let sweep = match sweep_capsules(moving, delta, fixed) {
                Ok(Some(sweep)) => sweep,
                Ok(None) => return hit,
                Err(_) => return blocked(hit),
            };
            let normal: [f64; 3] = sweep.normal.into();
            trace_iw4::Trace {
                fraction: sweep.fraction as f32,
                normal: normal.map(|v| v as f32),
                startsolid: u8::from(sweep.startsolid),
                allsolid: u8::from(sweep.allsolid),
                walkable: u8::from(!sweep.startsolid && normal[2] >= 0.7),
                contents: body.bounds.contents,
                endpos: std::array::from_fn(|i| {
                    (f64::from(input.start[i])
                        + (f64::from(input.end[i]) - f64::from(input.start[i])) * sweep.fraction)
                        as f32
                }),
                ..Default::default()
            }
        };
        let startsolid = hit.startsolid | other.startsolid;
        let allsolid = hit.allsolid | other.allsolid;
        if other.fraction < hit.fraction {
            other.hit_type = trace_iw4::HITTYPE_ENTITY;
            other.hit_id = body.number;
            hit = other;
        }
        hit.startsolid = startsolid;
        hit.allsolid = allsolid;
        hit
    }

    pub(crate) fn penetrations(
        self,
        input: GroundTraceInput,
        self_entnum: u16,
        out: &mut movement_iw4::recovery::ContactBuffer,
    ) -> movement_iw4::recovery::Coverage {
        use movement_iw4::{penetration::capsule_capsule, recovery::Coverage};
        let Some(moving) = crate::penetration::capsule(input, true) else {
            return Coverage::Unsupported;
        };
        for body in self.bodies(input.tracemask, self_entnum) {
            let fixed = match self.capsule(body, input, BodyQuerySpace::Simulation) {
                Ok(Some(cap)) => cap,
                Ok(None) => continue,
                Err(()) => return Coverage::Unsupported,
            };
            if let Err(status) = crate::penetration::append(capsule_capsule(moving, fixed), out) {
                return status;
            }
        }
        Coverage::Complete
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
