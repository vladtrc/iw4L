use std::sync::Arc;

use crate::xanim_clip::AnimClip;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipSchedulerError {
    BadNode { node: usize, len: usize },
}

impl core::fmt::Display for ClipSchedulerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadNode { node, len } => {
                write!(
                    f,
                    "clip scheduler node {node} is out of range (has {len} nodes)"
                )
            }
        }
    }
}

impl std::error::Error for ClipSchedulerError {}

#[derive(Debug, Clone, Copy)]
pub struct ActiveAnim<'a> {
    pub node: usize,
    pub clip: &'a AnimClip,
    pub time: f32,
    pub rate: f32,
    pub weight: f32,
}

#[derive(Debug, Default)]
struct Node {
    clip: Option<Arc<AnimClip>>,
    time: f32,
    rate: f32,
    weight: f32,
    goal_weight: f32,
    goal_time_remaining: f32,
}

#[derive(Debug)]
pub struct ClipScheduler {
    nodes: Vec<Node>,
}

impl ClipScheduler {
    pub fn new(node_count: usize) -> Self {
        Self {
            nodes: (0..node_count)
                .map(|_| Node {
                    rate: 1.0,
                    ..Default::default()
                })
                .collect(),
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn set_clip(&mut self, node: usize, clip: Arc<AnimClip>) -> Result<(), ClipSchedulerError> {
        let node = self.node_mut(node)?;
        node.clip = Some(clip);
        node.time = 0.0;
        Ok(())
    }

    pub fn clear(&mut self, node: usize) -> Result<(), ClipSchedulerError> {
        *self.node_mut(node)? = Node {
            rate: 1.0,
            ..Default::default()
        };
        Ok(())
    }

    pub fn set_time(&mut self, node: usize, time: f32) -> Result<(), ClipSchedulerError> {
        self.node_mut(node)?.time = time.max(0.0);
        Ok(())
    }

    pub fn set_rate(&mut self, node: usize, rate: f32) -> Result<(), ClipSchedulerError> {
        self.node_mut(node)?.rate = rate;
        Ok(())
    }

    pub fn set_goal_weight(
        &mut self,
        node: usize,
        goal_weight: f32,
        goal_time: f32,
    ) -> Result<(), ClipSchedulerError> {
        let node = self.node_mut(node)?;
        node.goal_weight = goal_weight.max(0.0);
        node.goal_time_remaining = goal_time.max(0.0);
        if node.goal_time_remaining == 0.0 {
            node.weight = node.goal_weight;
        }
        Ok(())
    }

    pub fn advance(&mut self, dt: f32) -> Vec<String> {
        let dt = dt.max(0.0);
        let mut notifies = Vec::new();
        for node in &mut self.nodes {
            if node.weight > 0.0 {
                let old_time = node.time;
                advance_time(node, dt);
                if node.goal_weight > 0.0
                    && let Some(clip) = node.clip.as_deref()
                {
                    notifies.extend(clip.crossed_notifies(old_time, node.time));
                }
            }
            advance_goal(node, dt);
        }
        notifies
    }

    pub fn active(&self) -> impl Iterator<Item = ActiveAnim<'_>> {
        self.nodes.iter().enumerate().filter_map(|(node, state)| {
            (state.weight > 0.0).then(|| {
                state.clip.as_deref().map(|clip| ActiveAnim {
                    node,
                    clip,
                    time: state.time,
                    rate: state.rate,
                    weight: state.weight,
                })
            })?
        })
    }

    pub fn weight(&self, node: usize) -> Result<f32, ClipSchedulerError> {
        Ok(self.node(node)?.weight)
    }

    pub fn time(&self, node: usize) -> Result<f32, ClipSchedulerError> {
        Ok(self.node(node)?.time)
    }

    pub fn rate(&self, node: usize) -> Result<f32, ClipSchedulerError> {
        Ok(self.node(node)?.rate)
    }

    fn node(&self, node: usize) -> Result<&Node, ClipSchedulerError> {
        self.nodes.get(node).ok_or(ClipSchedulerError::BadNode {
            node,
            len: self.nodes.len(),
        })
    }

    fn node_mut(&mut self, node: usize) -> Result<&mut Node, ClipSchedulerError> {
        let len = self.nodes.len();
        self.nodes
            .get_mut(node)
            .ok_or(ClipSchedulerError::BadNode { node, len })
    }
}

fn advance_time(node: &mut Node, dt: f32) {
    let Some(clip) = &node.clip else { return };
    let duration = clip.duration();
    if duration <= f32::EPSILON {
        node.time = 0.0;
    } else if clip.looping {
        node.time = (node.time + dt * node.rate).rem_euclid(duration);
    } else {
        node.time = (node.time + dt * node.rate).clamp(0.0, duration);
    }
}

fn advance_goal(node: &mut Node, dt: f32) {
    if node.goal_time_remaining <= 0.0 {
        return;
    }
    let step = dt.min(node.goal_time_remaining);
    node.weight += (node.goal_weight - node.weight) * step / node.goal_time_remaining;
    node.goal_time_remaining -= step;
    if node.goal_time_remaining <= f32::EPSILON {
        node.weight = node.goal_weight;
        node.goal_time_remaining = 0.0;
    }
}
