use super::*;

impl XAnimTreeRuntime {
    pub fn restart_with_blend(
        &mut self,
        node: XAnimNodeId,
        blend: f32,
        sync: &XAnimSyncTree,
    ) -> Result<Option<(XAnimSyncTree, XAnimNodeId)>, XAnimTreeError> {
        if !sync.matches(&self.definition) {
            return Err(XAnimTreeError::SyncDefinitionMismatch);
        }
        if !blend.is_finite() || blend < 0.0 {
            return Err(XAnimTreeError::NonFiniteState {
                node: node.0 as usize,
            });
        }
        if node.0 as usize >= self.states.len() {
            return Err(XAnimTreeError::ParentOutOfRange {
                node: node.0 as usize,
                parent: node.0 as usize,
            });
        }
        let root = if let Some(root) = sync.group(node) {
            root
        } else {
            if !matches!(
                self.definition.nodes[node.0 as usize].kind,
                XAnimNodeKind::Leaf { .. }
            ) {
                return Ok(None);
            }
            node
        };
        let state = self.states[root.0 as usize];
        if (state.time == 0.0 && state.cycle_count == 0)
            || state.cycle_count != state.old_cycle_count
        {
            return Ok(None);
        }
        let mut inside = vec![false; self.states.len()];
        for index in 0..inside.len() {
            inside[index] = index == root.0 as usize
                || self.definition.nodes[index]
                    .parent
                    .is_some_and(|parent| inside[parent.0 as usize]);
        }
        if blend == 0.0 || state.weight == 0.0 {
            for (index, &inside) in inside.iter().enumerate() {
                if inside {
                    reset_time(&mut self.states[index]);
                }
            }
            return Ok(Some((sync.clone(), root)));
        }
        let mut definitions = self.definition.nodes.clone();
        let mut states = self.states.clone();
        let mut sources: Vec<_> = (0..states.len()).collect();
        let mut copied = vec![None; states.len()];
        for index in 0..inside.len() {
            if !inside[index] || self.states[index].weight == 0.0 {
                continue;
            }
            let parent = self.definition.nodes[index].parent;
            if index != root.0 as usize
                && parent.is_some_and(|parent| copied[parent.0 as usize].is_none())
            {
                continue;
            }
            let mut definition = self.definition.nodes[index].clone();
            definition.parent = parent.map(|parent| copied[parent.0 as usize].unwrap_or(parent));
            let id = XAnimNodeId(u16::try_from(definitions.len()).map_err(|_| {
                XAnimTreeError::TooManyNodes {
                    count: definitions.len() + 1,
                }
            })?);
            copied[index] = Some(id);
            definitions.push(definition);
            let mut old = self.states[index];
            old.goal_weight = 0.0;
            old.goal_time = blend;
            states.push(old);
            sources.push(index);
        }
        let definition = Arc::new(XAnimTreeDefinition::new(definitions)?);
        let rebuilt = sync.remap_definition(definition.clone(), &sources)?;
        for (index, &inside) in inside.iter().enumerate() {
            if inside {
                reset_time(&mut states[index]);
                states[index].weight = 0.0;
            }
        }
        self.definition = definition;
        self.states = states;
        Ok(Some((rebuilt, root)))
    }

    pub fn compact_restarts(
        &mut self,
        script_nodes: usize,
        sync: &XAnimSyncTree,
    ) -> Result<Option<XAnimSyncTree>, XAnimTreeError> {
        if !sync.matches(&self.definition) {
            return Err(XAnimTreeError::SyncDefinitionMismatch);
        }
        if script_nodes >= self.states.len() {
            return Ok(None);
        }
        let mut mapping = vec![None; self.states.len()];
        let mut definitions = Vec::new();
        let mut states = Vec::new();
        let mut sources = Vec::new();
        for index in 0..self.states.len() {
            let node = &self.definition.nodes[index];
            if index >= script_nodes
                && (self.states[index].weight == 0.0
                    || node
                        .parent
                        .is_some_and(|parent| mapping[parent.0 as usize].is_none()))
            {
                continue;
            }
            mapping[index] = Some(XAnimNodeId(definitions.len() as u16));
            let mut definition = node.clone();
            definition.parent = node.parent.and_then(|parent| mapping[parent.0 as usize]);
            definitions.push(definition);
            states.push(self.states[index]);
            sources.push(index);
        }
        if states.len() == self.states.len() {
            return Ok(None);
        }
        let definition = Arc::new(XAnimTreeDefinition::new(definitions)?);
        let rebuilt = sync.remap_definition(definition.clone(), &sources)?;
        self.definition = definition;
        self.states = states;
        Ok(Some(rebuilt))
    }
}

fn reset_time(state: &mut XAnimNodeState) {
    state.time = 0.0;
    state.old_time = 0.0;
    state.cycle_count = 0;
    state.old_cycle_count = 0;
}
