use super::*;

/// Validated sync roots and ancestry from an authored ATR definition. Descendant
/// branch clock flags are inherited here, rather than mutating shared clips.
#[derive(Clone, Debug)]
pub struct XAnimSyncTree {
    definition: Arc<XAnimTreeDefinition>,
    groups: Vec<Option<XAnimNodeId>>,
    looping: Vec<bool>,
    children: Vec<Vec<usize>>,
}

impl XAnimSyncTree {
    pub fn new(
        definition: Arc<XAnimTreeDefinition>,
        flags: &[u16],
    ) -> Result<Self, XAnimTreeError> {
        let nodes = definition.nodes();
        if flags.len() != nodes.len() {
            return Err(XAnimTreeError::SyncFlagsLength);
        }
        let mut groups = vec![None; nodes.len()];
        let mut looping = vec![false; nodes.len()];
        let mut children = vec![Vec::new(); nodes.len()];
        for (node, entry) in nodes.iter().enumerate() {
            let inherited = entry.parent.and_then(|parent| groups[parent.0 as usize]);
            if let Some(parent) = entry.parent {
                children[parent.0 as usize].push(node);
            }
            let mode = flags[node] & 3;
            if mode == 3 {
                return Err(XAnimTreeError::ConflictingSyncMode { node });
            }
            if matches!(entry.kind, XAnimNodeKind::Leaf { .. }) {
                groups[node] = inherited;
            } else {
                if mode != 0 && inherited.is_some() {
                    return Err(XAnimTreeError::NestedSync { node });
                }
                groups[node] =
                    inherited.or_else(|| (mode != 0).then_some(XAnimNodeId(node as u16)));
                looping[node] = mode == 1;
            }
            if let Some(root) = groups[node] {
                if let XAnimNodeKind::Leaf { clip, .. } = &entry.kind {
                    if clip.looping != looping[root.0 as usize] {
                        return Err(XAnimTreeError::SyncLoopMismatch { node });
                    }
                }
            }
        }
        Ok(Self {
            definition,
            groups,
            looping,
            children,
        })
    }

    pub(super) fn remap_definition(
        &self,
        definition: Arc<XAnimTreeDefinition>,
        sources: &[usize],
    ) -> Result<Self, XAnimTreeError> {
        let flags: Vec<_> = sources
            .iter()
            .map(|&source| {
                if self.groups[source] == Some(XAnimNodeId(source as u16)) {
                    if self.looping[source] { 1 } else { 2 }
                } else {
                    0
                }
            })
            .collect();
        Self::new(definition, &flags)
    }

    pub(super) fn matches(&self, definition: &Arc<XAnimTreeDefinition>) -> bool {
        Arc::ptr_eq(&self.definition, definition)
    }

    pub fn group(&self, node: XAnimNodeId) -> Option<XAnimNodeId> {
        self.groups.get(node.0 as usize).copied().flatten()
    }

    pub fn looping(&self, node: XAnimNodeId) -> Option<bool> {
        self.group(node).map(|root| self.looping[root.0 as usize])
    }

    fn frequencies(&self, runtime: &XAnimTreeRuntime) -> Vec<f32> {
        let mut frequencies = vec![0.0; self.groups.len()];
        for node in (0..self.groups.len()).rev() {
            if let XAnimNodeKind::Leaf { clip, .. } = &self.definition.nodes[node].kind {
                frequencies[node] = clip.frequency();
            } else {
                let mut weight = 0.0;
                let mut frequency = 0.0;
                for &child in &self.children[node] {
                    let state = runtime.states[child];
                    if state.weight != 0.0 && frequencies[child] != 0.0 {
                        weight += state.weight;
                        frequency += frequencies[child] * state.rate * state.weight;
                    }
                }
                if weight != 0.0 {
                    frequencies[node] = frequency / weight;
                }
            }
        }
        frequencies
    }

    pub fn greatest_goal_descendant(
        &self,
        runtime: &XAnimTreeRuntime,
        node: XAnimNodeId,
    ) -> Option<XAnimNodeId> {
        if matches!(
            self.definition.nodes[node.0 as usize].kind,
            XAnimNodeKind::Leaf { .. }
        ) {
            return Some(node);
        }
        let mut best = None;
        let mut weight = 0.0;
        for &child in &self.children[node.0 as usize] {
            if runtime.states[child].goal_weight > weight {
                if let Some(leaf) =
                    self.greatest_goal_descendant(runtime, XAnimNodeId(child as u16))
                {
                    best = Some(leaf);
                    weight = runtime.states[child].goal_weight;
                }
            }
        }
        best
    }
}

impl XAnimTreeRuntime {
    /// Normalized clock speed; descendants of a sync root share its averaged
    /// frequency, while their individual rates contribute to that average.
    pub fn sync_clock_rate(&self, sync: &XAnimSyncTree, node: XAnimNodeId) -> f32 {
        let source = sync.group(node).unwrap_or(node);
        let mut rate = sync.frequencies(self)[source.0 as usize];
        let mut ancestor = Some(source);
        while let Some(at) = ancestor {
            let state = self.states[at.0 as usize];
            if state.weight == 0.0 {
                return 0.0;
            }
            rate *= state.rate;
            ancestor = self.definition.nodes[at.0 as usize].parent;
        }
        rate
    }

    /// Update goals once at frame start, before notetrack time subdivisions.
    pub fn begin_synced_update(
        &mut self,
        seconds: f32,
        sync: &XAnimSyncTree,
    ) -> Result<(), XAnimTreeError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(XAnimTreeError::NonFiniteState { node: 0 });
        }
        if !Arc::ptr_eq(&self.definition, &sync.definition) {
            return Err(XAnimTreeError::SyncDefinitionMismatch);
        }
        let can_reset: Vec<_> = self
            .states
            .iter()
            .map(|state| state.cycle_count == state.old_cycle_count)
            .collect();
        self.begin_update();
        let mut active = vec![false; self.states.len()];
        for node in 0..self.states.len() {
            let parent = self.definition.nodes[node].parent;
            let parent_active = parent.is_none_or(|parent| active[parent.0 as usize]);
            let state = &mut self.states[node];
            (state.weight, state.goal_time) = advance_goal_weight(
                state.weight,
                state.goal_weight,
                state.goal_time,
                seconds,
                parent_active,
            );
            active[node] = parent_active && state.weight != 0.0;
        }
        let mut has_time = vec![false; self.states.len()];
        for node in (0..self.states.len()).rev() {
            has_time[node] = active[node]
                && match &self.definition.nodes[node].kind {
                    XAnimNodeKind::Leaf { clip, .. } => clip.frequency() != 0.0,
                    _ => sync.children[node].iter().any(|&child| has_time[child]),
                };
        }
        for node in 0..self.states.len() {
            if has_time[node] {
                continue;
            }
            let id = XAnimNodeId(node as u16);
            let group = sync.group(id);
            let state = if group == Some(id) {
                if !can_reset[node] {
                    continue;
                }
                (0.0, 0)
            } else {
                group.map_or((0.0, 0), |root| {
                    let state = self.states[root.0 as usize];
                    (state.time, state.cycle_count)
                })
            };
            self.states[node].time = state.0;
            self.states[node].old_time = state.0;
            self.states[node].cycle_count = state.1;
            self.states[node].old_cycle_count = state.1;
        }
        Ok(())
    }

    /// Advance clocks after begin_synced_update, including partial frame steps.
    pub fn advance_synced(
        &mut self,
        seconds: f32,
        sync: &XAnimSyncTree,
    ) -> Result<(), XAnimTreeError> {
        if !Arc::ptr_eq(&self.definition, &sync.definition) {
            return Err(XAnimTreeError::SyncDefinitionMismatch);
        }
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(XAnimTreeError::NonFiniteState { node: 0 });
        }
        let mut active = vec![false; self.states.len()];
        let mut rates = vec![0.0; self.states.len()];
        for node in 0..self.states.len() {
            let parent = self.definition.nodes[node].parent;
            let state = &self.states[node];
            active[node] =
                state.weight != 0.0 && parent.is_none_or(|parent| active[parent.0 as usize]);
            rates[node] = if active[node] {
                state.rate * parent.map_or(1.0, |parent| rates[parent.0 as usize])
            } else {
                0.0
            };
        }
        let frequencies = sync.frequencies(self);
        for node in 0..self.states.len() {
            if !active[node] {
                continue;
            }
            let id = XAnimNodeId(node as u16);
            if let Some(root) = sync.group(id) {
                if root != id {
                    let root_state = self.states[root.0 as usize];
                    if frequencies[root.0 as usize] * rates[root.0 as usize] * seconds == 0.0 {
                        continue;
                    }
                    let state = &mut self.states[node];
                    if state.old_time != root_state.old_time
                        || state.old_cycle_count != root_state.old_cycle_count
                    {
                        state.old_time = root_state.old_time;
                        state.old_cycle_count = root_state.old_cycle_count;
                    }
                    state.time = root_state.time;
                    state.cycle_count = root_state.cycle_count;
                    continue;
                }
            } else if !matches!(self.definition.nodes[node].kind, XAnimNodeKind::Leaf { .. }) {
                continue;
            }
            let delta = frequencies[node] * rates[node] * seconds;
            // Avoid an unbounded subtraction loop for an unrepresentable clock
            // advance. Normal frames are several orders of magnitude smaller.
            if !delta.is_finite() {
                return Err(XAnimTreeError::NonFiniteState { node });
            }
            if delta > 65535.0 {
                return Err(XAnimTreeError::ClockAdvanceOutOfRange { node });
            }
            let looping = sync
                .looping(id)
                .unwrap_or_else(|| self.leaf_clip(id).unwrap().looping);
            let state = &mut self.states[node];
            let (mut time, cycle) = advance_leaf_time(
                state.time,
                state.cycle_count,
                1.0,
                frequencies[node] * rates[node],
                seconds,
                looping,
            );
            if !looping && time == 1.0 && state.old_time < anim_iw4::XANIM_NONLOOP_END_PARK {
                time = anim_iw4::XANIM_NONLOOP_END_PARK;
            }
            state.time = time;
            state.cycle_count = cycle;
        }
        Ok(())
    }
}
