use crate::{PortalView, RetainedDrawKind};
use glam::{DMat4, DVec3, DVec4, Mat4};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalEntityAperture {
    /// Four positive half-spaces of the doorway's rectangular prism.
    pub planes: [[f64; 4]; 4],
    pub is_clone: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PortalEntityInstance {
    pub world_from_local: Mat4,
    /// Positive half-space, expressed in this instance's physical world.
    pub slice_plane: Option<[f64; 4]>,
    pub aperture: Option<PortalEntityAperture>,
}

impl PortalEntityInstance {
    pub fn apply_to(self, kind: &mut RetainedDrawKind) {
        if let RetainedDrawKind::XModel {
            world_from_local,
            portal_slice,
            portal_aperture,
            ..
        } = kind
        {
            *world_from_local = self.world_from_local;
            *portal_slice = self.slice_plane;
            *portal_aperture = self.aperture;
        }
    }
}

impl PortalView {
    pub fn set_entity_bounds(&mut self, bounds: impl IntoIterator<Item = (u32, dpvs_iw4::Bounds)>) {
        let mut owners = std::collections::BTreeMap::<u32, dpvs_iw4::Bounds>::new();
        for (entity, bounds) in bounds {
            owners
                .entry(entity)
                .and_modify(|current| {
                    let lo = std::array::from_fn(|i| current.mins()[i].min(bounds.mins()[i]));
                    let hi = std::array::from_fn(|i| current.maxs()[i].max(bounds.maxs()[i]));
                    *current = dpvs_iw4::Bounds::from_mins_maxs(lo, hi);
                })
                .or_insert(bounds);
        }
        self.entity_bounds = owners.into_iter().collect::<Vec<_>>().into();
    }
    /// Presentation copies share the native skin/geometry and never create a
    /// second simulation entity. The owner's current posed bounds decide when
    /// the whole object, including attachments, straddles an authored opening.
    pub fn entity_instances(&self, kind: &RetainedDrawKind) -> Vec<PortalEntityInstance> {
        let RetainedDrawKind::XModel {
            world_from_local,
            scene_entnum,
            object_id,
            ..
        } = *kind
        else {
            return Vec::new();
        };
        let original = PortalEntityInstance {
            world_from_local,
            slice_plane: None,
            aperture: None,
        };
        if self.shared {
            return vec![original];
        }
        if object_id == 1 || object_id == u16::MAX {
            return vec![original];
        }
        let Some(bounds) = scene_entnum
            .and_then(|id| {
                self.entity_bounds
                    .binary_search_by_key(&id, |&(id, _)| id)
                    .ok()
            })
            .map(|index| self.entity_bounds[index].1)
        else {
            return vec![original];
        };
        let origin = world_from_local.w_axis.truncate().as_dvec3();
        let center = DVec3::from_array(bounds.mid().map(f64::from));
        let half = DVec3::from_array(bounds.half().map(f64::from));
        if !origin.is_finite()
            || !center.is_finite()
            || !half.is_finite()
            || half.min_element() < 0.
        {
            return vec![original];
        }
        let world = usize::from(
            self.point_tag(origin.as_vec3().to_array())
                .saturating_sub(1),
        );
        let mut crossing = None;
        for (pair_id, pair) in self.pairs.iter().enumerate() {
            for (side, (entered, exit, yaw)) in
                [(pair.a, pair.b, pair.yaw()), (pair.b, pair.a, -pair.yaw())]
                    .into_iter()
                    .enumerate()
            {
                if entered.world.index() != world
                    || self
                        .blocked_exits
                        .get(pair_id)
                        .is_some_and(|doors| doors[1 - side])
                {
                    continue;
                }
                let base = DVec3::from_array(entered.base.map(f64::from))
                    + DVec3::from_array(
                        asset_core::atlas::WorldLayout
                            .origin(entered.world)
                            .map(f64::from),
                    );
                let normal = DVec3::from_array(entered.normal.map(f64::from)).normalize();
                let tangent = DVec3::new(-normal.y, normal.x, 0.);
                let local = center - base;
                let d = normal.dot(local);
                if d.abs() > normal.abs().dot(half)
                    || tangent.dot(local).abs()
                        > f64::from(entered.width) * 0.5 + tangent.abs().dot(half)
                    || local.z + half.z < 0.
                    || local.z - half.z > f64::from(entered.height)
                {
                    continue;
                }
                let distance = normal.dot(origin - base);
                // A nearby plane can belong to a different doorway beyond a
                // jamb. Rank the actual opening, not its infinite plane: broad
                // posed bounds may overlap several doors in the same room.
                let origin_local = origin - base;
                let across =
                    (tangent.dot(origin_local).abs() - f64::from(entered.width) * 0.5).max(0.);
                let above = (-origin_local.z)
                    .max(origin_local.z - f64::from(entered.height))
                    .max(0.);
                let opening_distance = distance * distance + across * across + above * above;
                if crossing.as_ref().is_some_and(
                    |(best, _, _, _): &(f64, DMat4, DVec4, [[f64; 4]; 4])| {
                        *best <= opening_distance
                    },
                ) {
                    continue;
                }
                let side = if distance < 0. { -1. } else { 1. };
                let n = normal * side;
                let plane = n.extend(-n.dot(base));
                let exit_base = DVec3::from_array(exit.base.map(f64::from))
                    + DVec3::from_array(
                        asset_core::atlas::WorldLayout
                            .origin(exit.world)
                            .map(f64::from),
                    );
                let carry = DMat4::from_translation(exit_base)
                    * DMat4::from_rotation_z(yaw)
                    * DMat4::from_translation(-base);
                let width = f64::from(entered.width) * 0.5;
                let bottom = base.z;
                let rect = [
                    tangent.extend(width - tangent.dot(base)),
                    (-tangent).extend(width + tangent.dot(base)),
                    DVec4::new(0., 0., 1., -bottom),
                    DVec4::new(0., 0., -1., bottom + f64::from(entered.height)),
                ]
                .map(|p| p.to_array());
                crossing = Some((opening_distance, carry, plane, rect));
            }
        }
        let Some((_, carry, plane, rect)) = crossing else {
            return vec![original];
        };
        // Tiny overlap prevents a rounding gap at the shared physical plane.
        let source = plane + DVec4::W * 0.01;
        let destination = -(carry.inverse().transpose() * plane) + DVec4::W * 0.01;
        vec![
            PortalEntityInstance {
                slice_plane: Some(source.to_array()),
                aperture: Some(PortalEntityAperture {
                    planes: rect,
                    is_clone: false,
                }),
                ..original
            },
            PortalEntityInstance {
                world_from_local: (carry * world_from_local.as_dmat4()).as_mat4(),
                slice_plane: Some(destination.to_array()),
                aperture: Some(PortalEntityAperture {
                    planes: rect
                        .map(|p| (carry.inverse().transpose() * DVec4::from_array(p)).to_array()),
                    is_clone: true,
                }),
            },
        ]
    }
}
