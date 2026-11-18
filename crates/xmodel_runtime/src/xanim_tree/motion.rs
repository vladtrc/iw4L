use super::*;
use crate::{RootMotionDelta, RootMotionPose};

#[derive(Default)]
struct Accumulator {
    rotation: [f32; 4],
    translation: [f32; 3],
    position_weight: f32,
}

impl XAnimTreeRuntime {
    /// Blended frame motion in the current root frame, including authored yaw.
    pub fn relative_motion(
        &self,
        node: XAnimNodeId,
        goal_weights: bool,
    ) -> Option<RootMotionDelta> {
        let node = usize::from(node.0);
        self.states.get(node)?;
        self.relative_motion_node(node, goal_weights)
    }

    fn relative_motion_node(&self, node: usize, goal_weights: bool) -> Option<RootMotionDelta> {
        let weight = |node: usize| {
            if goal_weights {
                self.states[node].goal_weight
            } else {
                self.states[node].weight
            }
        };
        if weight(node) == 0.0 {
            return None;
        }
        match &self.definition.nodes[node].kind {
            XAnimNodeKind::Leaf { clip, .. } => {
                let state = self.states[node];
                clip.delta_enabled.then(|| {
                    let mut delta = clip.relative_delta(state.old_time, state.time);
                    // XAnimCalcDeltaTree requests translation relative to the current rotation.
                    let [z, w] = delta.rotation;
                    let norm = z * z + w * w;
                    if norm != 0. {
                        let zz = z * z * (2. / norm);
                        let zw = z * w * (2. / norm);
                        let [x, y, _] = delta.translation;
                        delta.translation[0] = (1. - zz) * x + zw * y;
                        delta.translation[1] = y - (zw * x + zz * y);
                    }
                    delta
                })
            }
            XAnimNodeKind::Additive => None,
            XAnimNodeKind::Blend => {
                let mut translation = [0.; 3];
                let mut rotation = [0.; 2];
                let mut total = 0.;
                for child in self.children_of(node) {
                    if let Some(delta) = self.relative_motion_node(child, goal_weights) {
                        let weight = weight(child);
                        for (sum, value) in translation.iter_mut().zip(delta.translation) {
                            *sum += weight * value;
                        }
                        for (sum, value) in rotation.iter_mut().zip(delta.rotation) {
                            *sum += weight * value;
                        }
                        total += weight;
                    }
                }
                (total > 0.).then(|| {
                    let length = rotation[0].hypot(rotation[1]);
                    RootMotionDelta {
                        translation: translation.map(|v| v / total),
                        rotation: if length > 0. {
                            rotation.map(|v| v / length)
                        } else {
                            [0., 1.]
                        },
                    }
                })
            }
        }
    }

    /// Frame translation from the active root-motion leaves, normalized at each blend.
    pub fn relative_translation(&self, node: XAnimNodeId) -> Option<[f32; 3]> {
        self.states.get(usize::from(node.0))?;
        self.relative_translation_node(usize::from(node.0))
    }

    fn relative_translation_node(&self, node: usize) -> Option<[f32; 3]> {
        if self.states[node].weight == 0.0 {
            return None;
        }
        match &self.definition.nodes[node].kind {
            XAnimNodeKind::Leaf { clip, .. } => {
                if !clip.has_delta() {
                    return None;
                }
                let state = self.states[node];
                let start = clip.abs_delta_trans(state.old_time);
                let end = clip.abs_delta_trans(state.time);
                let first = clip.abs_delta_trans(0.0);
                let last = clip.abs_delta_trans(1.0);
                let cycles = f32::from(state.cycle_count.wrapping_sub(state.old_cycle_count));
                Some(std::array::from_fn(|axis| {
                    end[axis] - start[axis] + cycles * (last[axis] - first[axis])
                }))
            }
            XAnimNodeKind::Additive => None,
            XAnimNodeKind::Blend => {
                let mut sum = [0.0; 3];
                let mut total = 0.0;
                for child in self.children_of(node) {
                    if let Some(delta) = self.relative_translation_node(child) {
                        let weight = self.states[child].weight;
                        for axis in 0..3 {
                            sum[axis] += delta[axis] * weight;
                        }
                        total += weight;
                    }
                }
                (total > 0.0).then(|| sum.map(|v| v / total))
            }
        }
    }

    pub fn absolute_delta(&self, node: XAnimNodeId) -> Option<RootMotionPose> {
        self.states.get(node.0 as usize)?;
        let mut sum = self.absolute_delta_node(node.0 as usize, 1.0, false);
        if sum.rotation == [0.0; 4] {
            sum.rotation = anim_iw4::QUAT_IDENTITY;
        }
        Some(RootMotionPose {
            translation: sum.translation,
            rotation: sum.rotation,
        })
    }

    fn absolute_delta_node(&self, node: usize, weight: f32, normalize: bool) -> Accumulator {
        if let XAnimNodeKind::Leaf { clip, .. } = &self.definition.nodes[node].kind {
            return clip.absolute_delta(self.states[node].time).map_or_else(
                Accumulator::default,
                |pose| Accumulator {
                    rotation: pose.rotation.map(|v| v * weight),
                    translation: pose.translation.map(|v| v * weight),
                    position_weight: weight,
                },
            );
        }
        let mut children = self
            .children_of(node)
            .filter(|&child| self.states[child].goal_weight != 0.0);
        let Some(first) = children.next() else {
            return Accumulator::default();
        };
        let Some(second) = children.next() else {
            return self.absolute_delta_node(first, weight, normalize);
        };
        let mut sum = Accumulator::default();
        for child in [first, second].into_iter().chain(children) {
            let value = self.absolute_delta_node(child, self.states[child].goal_weight, true);
            for axis in 0..4 {
                sum.rotation[axis] += value.rotation[axis];
            }
            for axis in 0..3 {
                sum.translation[axis] += value.translation[axis];
            }
            sum.position_weight += value.position_weight;
        }
        if normalize {
            let length = sum.rotation.iter().map(|v| v * v).sum::<f32>().sqrt();
            if length != 0.0 {
                sum.rotation = sum.rotation.map(|v| v * (weight / length));
            }
        }
        if sum.position_weight != 0.0 {
            let scale = if normalize { weight } else { 1.0 } / sum.position_weight;
            sum.translation = sum.translation.map(|v| v * scale);
            if normalize {
                sum.position_weight = weight;
            }
        }
        sum
    }
}
