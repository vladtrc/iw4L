use std::sync::Arc;

use crate::{AnimClip, PartBits};
use anim_iw4::{xanim_advance_goal_weight, xanim_advance_leaf_time};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XAnimNodeId(pub u16);

#[derive(Clone, Debug)]
pub enum XAnimNodeKind {
    Blend,
    Additive,
    Leaf {
        clip: Arc<AnimClip>,
        parts: Option<PartBits>,
    },
}

#[derive(Clone, Debug)]
pub struct XAnimNodeDefinition {
    pub parent: Option<XAnimNodeId>,
    pub kind: XAnimNodeKind,
}

#[derive(Clone, Debug)]
pub struct XAnimTreeDefinition {
    nodes: Vec<XAnimNodeDefinition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum XAnimTreeError {
    Empty,
    TooManyNodes { count: usize },
    ParentOutOfRange { node: usize, parent: usize },
    ParentNotBeforeChild { node: usize, parent: usize },
    LeafHasChildren { node: usize },
    NonFiniteState { node: usize },
}

impl core::fmt::Display for XAnimTreeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for XAnimTreeError {}

impl XAnimTreeDefinition {
    pub fn new(nodes: Vec<XAnimNodeDefinition>) -> Result<Self, XAnimTreeError> {
        if nodes.is_empty() {
            return Err(XAnimTreeError::Empty);
        }
        if nodes.len() > u16::MAX as usize {
            return Err(XAnimTreeError::TooManyNodes { count: nodes.len() });
        }
        let mut has_child = vec![false; nodes.len()];
        for (node, definition) in nodes.iter().enumerate() {
            if let Some(XAnimNodeId(parent)) = definition.parent {
                let parent = parent as usize;
                if parent >= nodes.len() {
                    return Err(XAnimTreeError::ParentOutOfRange { node, parent });
                }
                if parent >= node {
                    return Err(XAnimTreeError::ParentNotBeforeChild { node, parent });
                }
                has_child[parent] = true;
            }
        }
        for (node, definition) in nodes.iter().enumerate() {
            if matches!(definition.kind, XAnimNodeKind::Leaf { .. }) && has_child[node] {
                return Err(XAnimTreeError::LeafHasChildren { node });
            }
        }
        Ok(Self { nodes })
    }

    pub fn one_leaf(clip: Arc<AnimClip>, parts: Option<PartBits>) -> Arc<Self> {
        Arc::new(
            Self::new(vec![XAnimNodeDefinition {
                parent: None,
                kind: XAnimNodeKind::Leaf { clip, parts },
            }])
            .expect("one leaf is a valid XAnim definition"),
        )
    }

    pub fn nodes(&self) -> &[XAnimNodeDefinition] {
        &self.nodes
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XAnimNodeState {
    pub time: f32,
    pub old_time: f32,
    pub cycle_count: i16,
    pub old_cycle_count: i16,
    pub goal_time: f32,
    pub goal_weight: f32,
    pub weight: f32,
    pub rate: f32,
}

impl Default for XAnimNodeState {
    fn default() -> Self {
        Self {
            time: 0.0,
            old_time: 0.0,
            cycle_count: 0,
            old_cycle_count: 0,
            goal_time: 0.0,
            goal_weight: 0.0,
            weight: 0.0,
            rate: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct XAnimTreeRuntime {
    definition: Arc<XAnimTreeDefinition>,
    states: Vec<XAnimNodeState>,
}

#[derive(Debug)]
pub(crate) struct ActiveXAnimLeaf<'a> {
    pub clip: &'a AnimClip,
    pub time: f32,
    pub weight: f32,
    pub parts: Option<&'a PartBits>,
}

#[derive(Debug)]
pub(crate) struct ActiveAdditiveLayer<'a> {
    pub weight: f32,
    pub leaves: Vec<ActiveXAnimLeaf<'a>>,
    pub inner: Vec<ActiveAdditiveLayer<'a>>,
}

impl XAnimTreeRuntime {
    pub fn new(definition: Arc<XAnimTreeDefinition>) -> Self {
        let mut states = vec![XAnimNodeState::default(); definition.nodes.len()];
        for (node, definition) in definition.nodes.iter().enumerate() {
            if definition.parent.is_none() {
                states[node].weight = 1.0;
                states[node].goal_weight = 1.0;
            }
        }
        Self { definition, states }
    }

    pub fn definition(&self) -> &Arc<XAnimTreeDefinition> {
        &self.definition
    }

    pub fn states(&self) -> &[XAnimNodeState] {
        &self.states
    }

    pub fn set_state(
        &mut self,
        node: XAnimNodeId,
        state: XAnimNodeState,
    ) -> Result<(), XAnimTreeError> {
        let index = node.0 as usize;
        if !state.time.is_finite()
            || !state.old_time.is_finite()
            || !state.goal_time.is_finite()
            || !state.goal_weight.is_finite()
            || !state.weight.is_finite()
            || !state.rate.is_finite()
        {
            return Err(XAnimTreeError::NonFiniteState { node: index });
        }
        let Some(slot) = self.states.get_mut(index) else {
            return Err(XAnimTreeError::ParentOutOfRange {
                node: index,
                parent: index,
            });
        };
        *slot = state;
        Ok(())
    }

    pub fn set_complete_goal_weight(
        &mut self,
        node: XAnimNodeId,
        time: f32,
        weight: f32,
    ) -> Result<(), XAnimTreeError> {
        self.set_complete_goal_weight_in(node, time, weight, 0.0)
    }

    pub fn set_complete_goal_weight_in(
        &mut self,
        node: XAnimNodeId,
        time: f32,
        weight: f32,
        goal_time: f32,
    ) -> Result<(), XAnimTreeError> {
        let mut current = Some(node);
        let mut first = true;
        while let Some(id) = current {
            let index = id.0 as usize;
            let parent = self
                .definition
                .nodes()
                .get(index)
                .ok_or(XAnimTreeError::ParentOutOfRange {
                    node: index,
                    parent: index,
                })?
                .parent;
            let mut state = *self
                .states
                .get(index)
                .ok_or(XAnimTreeError::ParentOutOfRange {
                    node: index,
                    parent: index,
                })?;
            if first {
                state.time = time;
                state.old_time = time;
                state.goal_weight = weight;
                state.goal_time = goal_time;
                if goal_time <= 0.0 {
                    state.weight = weight;
                }
                first = false;
            } else if state.goal_weight == 0.0 {
                state.weight = 1.0;
                state.goal_weight = 1.0;
                state.goal_time = 0.0;
            }
            self.set_state(id, state)?;
            current = parent;
        }
        Ok(())
    }

    pub fn set_goal_weight(
        &mut self,
        node: XAnimNodeId,
        goal_weight: f32,
        goal_time: f32,
    ) -> Result<(), XAnimTreeError> {
        let index = node.0 as usize;
        let mut state = *self
            .states
            .get(index)
            .ok_or(XAnimTreeError::ParentOutOfRange {
                node: index,
                parent: index,
            })?;
        state.goal_weight = goal_weight;
        state.goal_time = goal_time;
        if goal_time <= 0.0 {
            state.weight = goal_weight;
        }
        self.set_state(node, state)
    }

    pub fn set_rate(&mut self, node: XAnimNodeId, rate: f32) -> Result<(), XAnimTreeError> {
        if !rate.is_finite() {
            return Err(XAnimTreeError::NonFiniteState {
                node: node.0 as usize,
            });
        }
        let index = node.0 as usize;
        let mut state = *self
            .states
            .get(index)
            .ok_or(XAnimTreeError::ParentOutOfRange {
                node: index,
                parent: index,
            })?;
        state.rate = rate;
        self.set_state(node, state)
    }

    pub fn rebind(&mut self, definition: Arc<XAnimTreeDefinition>) -> Result<(), XAnimTreeError> {
        if definition.nodes().len() != self.states.len() {
            return Err(XAnimTreeError::ParentOutOfRange {
                node: self.states.len(),
                parent: definition.nodes().len(),
            });
        }
        self.definition = definition;
        Ok(())
    }

    pub fn leaf_clip(&self, node: XAnimNodeId) -> Option<Arc<AnimClip>> {
        match self
            .definition
            .nodes()
            .get(node.0 as usize)
            .map(|n| &n.kind)
        {
            Some(XAnimNodeKind::Leaf { clip, .. }) => Some(Arc::clone(clip)),
            _ => None,
        }
    }

    pub fn one_leaf(clip: Arc<AnimClip>, time: f32, weight: f32) -> Self {
        let definition = XAnimTreeDefinition::one_leaf(clip, None);
        let mut runtime = Self::new(definition);
        runtime.states[0].time = time;
        runtime.states[0].old_time = time;
        runtime.states[0].weight = weight;
        runtime.states[0].goal_weight = weight;
        runtime
    }

    pub fn update(&mut self, dtime_seconds: f32) -> Result<(), XAnimTreeError> {
        if !dtime_seconds.is_finite() || dtime_seconds < 0.0 {
            return Err(XAnimTreeError::NonFiniteState { node: 0 });
        }
        let n = self.states.len();
        for node in 0..n {
            let parent_has_weight = self.definition.nodes[node]
                .parent
                .is_none_or(|parent| self.states[parent.0 as usize].weight != 0.0);
            let state = &mut self.states[node];
            let (weight, goal_time) = xanim_advance_goal_weight(
                state.weight,
                state.goal_weight,
                state.goal_time,
                dtime_seconds,
                parent_has_weight,
            );
            state.weight = weight;
            state.goal_time = goal_time;
            state.old_time = state.time;
            state.old_cycle_count = state.cycle_count;
        }
        for node in 0..n {
            if self.states[node].weight == 0.0 {
                continue;
            }
            let XAnimNodeKind::Leaf { clip, .. } = &self.definition.nodes[node].kind else {
                continue;
            };
            let state = &mut self.states[node];
            let (time, cycle) = xanim_advance_leaf_time(
                state.old_time,
                state.cycle_count,
                state.rate,
                clip.frequency(),
                dtime_seconds,
                clip.looping,
            );
            state.time = time;
            state.cycle_count = cycle;
        }
        Ok(())
    }

    #[must_use]
    pub fn calc_delta_translation(&self) -> Option<[f32; 3]> {
        let mut found: Option<[f32; 3]> = None;
        for (node, state) in self.states.iter().enumerate() {
            if state.weight == 0.0 {
                continue;
            }
            let XAnimNodeKind::Leaf { clip, .. } = &self.definition.nodes[node].kind else {
                continue;
            };
            if found.is_some() {
                return None;
            }
            let now = clip.abs_delta_trans(state.time);
            let then = clip.abs_delta_trans(state.old_time);
            let full = clip.abs_delta_trans(1.0);
            let cycles = f32::from(state.cycle_count - state.old_cycle_count);
            found = Some([
                now[0] - then[0] + cycles * full[0],
                now[1] - then[1] + cycles * full[1],
                now[2] - then[2] + cycles * full[2],
            ]);
        }
        found
    }

    fn sample_seconds(clip: &AnimClip, normalized: f32) -> f32 {
        clip.duration() * normalized
    }

    fn children_of(&self, parent: usize) -> impl Iterator<Item = usize> + '_ {
        self.definition
            .nodes
            .iter()
            .enumerate()
            .filter(move |(_, def)| def.parent.is_some_and(|p| p.0 as usize == parent))
            .map(|(i, _)| i)
    }

    fn collect_node<'a>(
        &'a self,
        node: usize,
        incoming: f32,
        leaves: &mut Vec<ActiveXAnimLeaf<'a>>,
        layers: &mut Vec<ActiveAdditiveLayer<'a>>,
    ) {
        let w = incoming * self.states[node].weight;
        if w <= 0.0 {
            return;
        }
        match &self.definition.nodes[node].kind {
            XAnimNodeKind::Leaf { clip, parts } => leaves.push(ActiveXAnimLeaf {
                clip,
                time: Self::sample_seconds(clip, self.states[node].time),
                weight: w,
                parts: parts.as_ref(),
            }),
            XAnimNodeKind::Blend => {
                for child in self.children_of(node) {
                    self.collect_node(child, w, leaves, layers);
                }
            }
            XAnimNodeKind::Additive => {
                let mut add_leaves = Vec::new();
                let mut inner = Vec::new();
                for child in self.children_of(node) {
                    self.collect_node(child, 1.0, &mut add_leaves, &mut inner);
                }
                if !add_leaves.is_empty() || !inner.is_empty() {
                    layers.push(ActiveAdditiveLayer {
                        weight: w,
                        leaves: add_leaves,
                        inner,
                    });
                }
            }
        }
    }

    pub(crate) fn active_pose(
        &self,
    ) -> Result<(Vec<ActiveXAnimLeaf<'_>>, Vec<ActiveAdditiveLayer<'_>>), XAnimTreeError> {
        let mut leaves = Vec::new();
        let mut layers = Vec::new();
        for (node, definition) in self.definition.nodes.iter().enumerate() {
            if definition.parent.is_none() {
                self.collect_node(node, 1.0, &mut leaves, &mut layers);
            }
        }
        Ok((leaves, layers))
    }
}
