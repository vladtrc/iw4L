use crate::{
    AnimClip, XAnimNodeDefinition, XAnimNodeId, XAnimNodeKind, XAnimNodeState, XAnimTreeDefinition,
    XAnimTreeRuntime,
};
use std::sync::Arc;

/// An authored full-body action on the same clock as the actor's idle pose.
#[derive(Clone, Debug)]
pub struct PerformanceAction {
    pub clip: Arc<AnimClip>,
    pub start_ms: i32,
    pub duration_ms: i32,
    pub blend_ms: i32,
}

#[derive(Clone, Debug)]
pub struct NativePerformance {
    pub idle: Arc<AnimClip>,
    pub actions: Vec<PerformanceAction>,
}

impl NativePerformance {
    pub fn active_clip(&self, time_ms: i32) -> &Arc<AnimClip> {
        self.actions
            .iter()
            .find(|a| time_ms >= a.start_ms && time_ms < a.start_ms.saturating_add(a.duration_ms))
            .map_or(&self.idle, |a| &a.clip)
    }

    pub fn runtime_at_ms(&self, time_ms: i32) -> XAnimTreeRuntime {
        let Some(action) = self
            .actions
            .iter()
            .find(|a| time_ms >= a.start_ms && time_ms < a.start_ms.saturating_add(a.duration_ms))
        else {
            return XAnimTreeRuntime::looping_leaf_at_ms(Arc::clone(&self.idle), time_ms);
        };
        let elapsed = time_ms - action.start_ms;
        let remaining = action.duration_ms - elapsed;
        let weight = if action.blend_ms > 0 {
            (elapsed.min(remaining) as f32 / action.blend_ms as f32).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let definition = Arc::new(
            XAnimTreeDefinition::new(vec![
                XAnimNodeDefinition {
                    parent: None,
                    kind: XAnimNodeKind::Blend,
                },
                XAnimNodeDefinition {
                    parent: Some(XAnimNodeId(0)),
                    kind: XAnimNodeKind::Leaf {
                        clip: Arc::clone(&self.idle),
                        parts: None,
                    },
                },
                XAnimNodeDefinition {
                    parent: Some(XAnimNodeId(0)),
                    kind: XAnimNodeKind::Leaf {
                        clip: Arc::clone(&action.clip),
                        parts: None,
                    },
                },
            ])
            .expect("performance blend tree"),
        );
        let mut runtime = XAnimTreeRuntime::new(definition);
        for (node, clip, clock, w) in [
            (0, &self.idle, 0, 1.0),
            (1, &self.idle, time_ms, 1.0 - weight),
            (2, &action.clip, elapsed, weight),
        ] {
            let phase = if clip.duration() > 0.0 {
                (clock as f32 * 0.001 / clip.duration()).rem_euclid(1.0)
            } else {
                0.0
            };
            runtime
                .set_state(
                    XAnimNodeId(node),
                    XAnimNodeState {
                        time: phase,
                        old_time: phase,
                        weight: w,
                        goal_weight: w,
                        ..Default::default()
                    },
                )
                .expect("performance node");
        }
        runtime
    }
}
