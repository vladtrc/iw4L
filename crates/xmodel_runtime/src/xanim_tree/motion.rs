use super::*;
use crate::RootMotionPose;

#[derive(Default)]
struct Accumulator {
    rotation: [f32; 4],
    translation: [f32; 3],
    position_weight: f32,
}

impl XAnimTreeRuntime {
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
