use super::super::super::animations::Animator;
use super::*;
use std::sync::Arc;
use xmodel_runtime::{RootMotionPose, XAnimNodeId, XAnimNodeKind};

const SEATING_RESPONSE_SECONDS: f32 = 0.08;

#[derive(Clone, Debug)]
struct Row {
    node: XAnimNodeId,
    samples: Vec<XAnimNodeId>,
}

#[derive(Clone, Debug)]
pub(crate) struct AimGrid {
    tree: String,
    root: XAnimNodeId,
    rows: Vec<Row>,
}

#[derive(Clone, Debug)]
pub(crate) struct Placement {
    height: f32,
    position_error: [f32; 3],
    angle_error: [f32; 3],
}

struct Weights {
    rows: Vec<f32>,
    samples: Vec<Vec<f32>>,
}

impl AimGrid {
    fn new(animator: &Animator, animation: &Value) -> Result<Self, String> {
        let root = animator.node(animation)?;
        let nodes = animator.data.definition.nodes();
        let children = |parent| {
            nodes
                .iter()
                .enumerate()
                .filter_map(move |(index, node)| {
                    (node.parent == Some(parent)).then_some(XAnimNodeId(index as u16))
                })
                .collect::<Vec<_>>()
        };
        if !matches!(nodes[root.0 as usize].kind, XAnimNodeKind::Blend) {
            return Err("turret aim animation must be a blend grid".into());
        }
        let mut rows = Vec::new();
        for node in children(root) {
            if !matches!(nodes[node.0 as usize].kind, XAnimNodeKind::Blend) {
                return Err("turret aim grid rows must be blend nodes".into());
            }
            let samples = children(node);
            if samples.is_empty() {
                return Err("turret aim grid row has no samples".into());
            }
            for sample in &samples {
                let XAnimNodeKind::Leaf { clip, .. } = &nodes[sample.0 as usize].kind else {
                    return Err("turret aim grid sample must be a leaf animation".into());
                };
                let pose = clip
                    .absolute_delta(0.)
                    .ok_or("turret aim sample has no authored root pose")?;
                if !pose
                    .translation
                    .into_iter()
                    .chain(pose.rotation)
                    .all(f32::is_finite)
                {
                    return Err("turret aim sample root pose is nonfinite".into());
                }
            }
            rows.push(Row { node, samples });
        }
        if rows.is_empty() {
            return Err("turret aim animation has no grid rows".into());
        }
        Ok(Self {
            tree: animator.name.clone(),
            root,
            rows,
        })
    }

    fn weights(
        &self,
        animator: &Animator,
        yaw: f32,
        column_degrees: f32,
        height: f32,
    ) -> Result<Weights, String> {
        if !yaw.is_finite() || !height.is_finite() {
            return Err("turret aim grid requires finite yaw and mounting height".into());
        }
        let mut samples = Vec::with_capacity(self.rows.len());
        let mut heights = Vec::with_capacity(self.rows.len());
        for (index, row) in self.rows.iter().enumerate() {
            let mut weights = vec![0.; row.samples.len()];
            if weights.len() == 1 {
                weights[0] = 1.;
            } else {
                if !column_degrees.is_finite() || column_degrees <= 0. {
                    return Err(
                        "multi-column turret aim grid requires positive authored angular spacing"
                            .into(),
                    );
                }
                let last = (weights.len() - 1) as f32;
                let coordinate = (last * 0.5 + yaw / column_degrees).clamp(0., last);
                let left = coordinate.floor() as usize;
                let right = (left + 1).min(weights.len() - 1);
                let fraction = coordinate - left as f32;
                weights[left] = 1. - fraction;
                weights[right] += fraction;
            }
            let mut row_height = 0.;
            for (&node, &weight) in row.samples.iter().zip(&weights) {
                let pose = animator
                    .runtime
                    .absolute_delta(node)
                    .ok_or("turret aim sample is absent from the active tree")?;
                row_height += pose.translation[2] * weight;
            }
            if !row_height.is_finite() {
                return Err("turret aim grid height is nonfinite".into());
            }
            heights.push((index, row_height));
            samples.push(weights);
        }
        heights.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut rows = vec![0.; self.rows.len()];
        let upper = heights.partition_point(|(_, value)| *value < height);
        if upper == 0 {
            rows[heights[0].0] = 1.;
        } else if upper == heights.len() {
            rows[heights[upper - 1].0] = 1.;
        } else {
            let (below, low) = heights[upper - 1];
            let (above, high) = heights[upper];
            let fraction = (height - low) / (high - low);
            rows[below] = 1. - fraction;
            rows[above] = fraction;
        }
        Ok(Weights { rows, samples })
    }

    fn apply(&self, animator: &mut Animator, weights: Weights) -> Result<RootMotionPose, String> {
        let blend_seconds = crate::MATCH_TICK_MS as f32 * 0.001;
        animator.goal(self.root, 1., blend_seconds, 1., true)?;
        for ((row, weight), samples) in self.rows.iter().zip(weights.rows).zip(weights.samples) {
            animator.goal(row.node, weight, blend_seconds, 1., false)?;
            for (&sample, weight) in row.samples.iter().zip(samples) {
                animator.goal(sample, weight, blend_seconds, 1., false)?;
            }
        }
        animator
            .runtime
            .absolute_delta(self.root)
            .ok_or("turret aim grid root is absent from the active tree".into())
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "setturretanim",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let runtime = world.resource::<RoundScript>();
            let animator = runtime
                .animators
                .get(&id)
                .ok_or("turret actor has no animation tree")?;
            let grid = AimGrid::new(animator, arg(args, 0)?)?;
            let mut runtime = world.resource_mut::<RoundScript>();
            let actor = runtime.actors.get_mut(&id).unwrap();
            actor.turret.grid = Some(Arc::new(grid));
            actor.turret.placement = None;
            Ok(Value::Undefined)
        },
    );
}

pub(crate) fn advance(world: &mut World) -> Result<(), String> {
    let _span = perf::span_handle!("actor_turret_pose", "iw4l.sim").enter();
    let users: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .keys()
        .filter_map(|&id| super::active(world.resource::<RoundScript>(), id).map(|gun| (id, gun)))
        .collect();
    for (id, gun) in users {
        let (grid, link, placement, column_degrees) = {
            let runtime = world.resource::<RoundScript>();
            let control = &runtime.actors[&id].turret;
            (
                control.grid.clone(),
                runtime.entities[&id].linked_to.clone(),
                control.placement.clone(),
                runtime.engine.turrets[&gun].aim_column_degrees,
            )
        };
        let Some(grid) = grid else {
            super::stop(world, id)?;
            diag::warn!(
                Sim,
                "turret pose refused: actor={id} turret={gun} has no configured aim grid"
            );
            continue;
        };
        let Some(link) = link.filter(|link| link.parent == gun) else {
            super::stop(world, id)?;
            continue;
        };
        let Some((mount, mount_axis)) =
            super::super::super::presence::tag_world(world, gun, "tag_weapon")
        else {
            super::stop(world, id)?;
            diag::warn!(
                Sim,
                "turret pose refused: actor={id} turret={gun} has no tag_weapon"
            );
            continue;
        };
        let (base, base_axis) = super::super::super::players::link_parent_pose(world, gun, None)?;
        let delta = Vec3::from_array(mount) - Vec3::from_array(base);
        let local_mount = base_axis.map(|axis| delta.dot(Vec3::from_array(axis)));
        let local_axis = super::super::super::players::links::relative(mount_axis, base_axis);
        let yaw = math_iw4::angle_subtract(math_iw4::axis_to_angles(local_axis)[1], 0.);
        let height = placement
            .as_ref()
            .map_or(link.origin[2], |pose| pose.height);
        let pose = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let animator = runtime
                .animators
                .get_mut(&id)
                .ok_or("turret actor has no animation tree")?;
            if animator.name != grid.tree {
                return Err("turret aim grid belongs to a different animation tree".into());
            }
            let weights = grid.weights(animator, yaw, column_degrees, height - local_mount[2])?;
            grid.apply(animator, weights)?
        };
        // The gun can pitch independently of the standing gunner's body.
        let (desired_origin, desired_angles) =
            pose.world_pose(local_mount, math_iw4::angles_to_axis([0., yaw, 0.]));
        let first = placement.is_none();
        let mut placement = placement.unwrap_or_else(|| Placement {
            height,
            position_error: std::array::from_fn(|i| link.origin[i] - desired_origin[i]),
            angle_error: std::array::from_fn(|i| {
                math_iw4::angle_subtract(link.angles[i], desired_angles[i])
            }),
        });
        if !first {
            let residual =
                (-(crate::MATCH_TICK_MS as f32 * 0.001) / SEATING_RESPONSE_SECONDS).exp();
            let fade = |value: f32| {
                let next = value * residual;
                if next.abs() < f32::MIN_POSITIVE {
                    0.
                } else {
                    next
                }
            };
            placement.position_error = placement.position_error.map(fade);
            placement.angle_error = placement.angle_error.map(fade);
        }
        let local_origin = std::array::from_fn(|i| desired_origin[i] + placement.position_error[i]);
        let local_angles = std::array::from_fn(|i| desired_angles[i] + placement.angle_error[i]);
        let origin = math_iw4::matrix_transform_vector43(local_origin, base_axis, base);
        let angles = math_iw4::axis_to_angles(math_iw4::matrix_multiply(
            math_iw4::angles_to_axis(local_angles),
            base_axis,
        ));
        let number = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let actor = runtime.actors.get_mut(&id).unwrap();
            actor.turret.placement = Some(placement);
            actor.motion.set_angles(angles);
            actor.orientation.set_angles(angles);
            let entity = runtime.entities.get_mut(&id).unwrap();
            let number = entity.number;
            let link = entity.linked_to.as_mut().unwrap();
            link.origin = local_origin;
            link.angles = local_angles;
            runtime.set_object_field(id, "origin", Value::Vector(origin));
            runtime.set_object_field(id, "angles", Value::Vector(angles));
            number
        };
        if world.resource::<RoundScript>().actors[&id].physics_initialized {
            let mut frame = crate::frame::FrameWorld::from_world(world);
            let mover = frame
                .script_mover_mut_by_number(number)
                .ok_or("turret actor presence is missing")?;
            entity_iw4::set_origin(&mut mover.state, origin);
            entity_iw4::set_angle(&mut mover.state, angles);
        }
    }
    Ok(())
}
