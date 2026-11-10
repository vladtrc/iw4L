use std::sync::Arc;

use crate::{
    AnimClip, DObjPoseRequest, HidePartBits, XAnimNodeDefinition, XAnimNodeId, XAnimNodeKind,
    XAnimNodeState, XAnimTreeDefinition, XAnimTreeError, XAnimTreeRuntime,
};
use anim_iw4::PartBits;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DObjModelDescriptor {
    pub model: String,
    pub parent_model: Option<u16>,
    pub attach_tag: Option<String>,
    pub ignore_collision: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DObjCompositionDescriptor {
    pub revision: u32,
    pub models: Vec<DObjModelDescriptor>,
}

impl DObjCompositionDescriptor {
    pub fn single(revision: u32, model: String) -> Self {
        Self {
            revision,
            models: vec![DObjModelDescriptor {
                model,
                parent_model: None,
                attach_tag: None,
                ignore_collision: false,
            }],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XAnimSemanticNodeKind {
    Blend,
    Additive,
    Leaf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XAnimSemanticNode {
    pub parent: Option<XAnimNodeId>,
    pub kind: XAnimSemanticNodeKind,
    pub clip: Option<String>,
    pub parts: Option<PartBits>,
    pub state: XAnimNodeState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XAnimTreeSnapshot {
    pub definition_revision: u32,
    pub state_revision: u32,
    pub nodes: Vec<XAnimSemanticNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DObjSemanticState {
    pub composition: DObjCompositionDescriptor,
    pub pose_revision: u32,
    pub tag_rotations: Vec<crate::TagRotation>,
    pub tree: Option<XAnimTreeSnapshot>,
    pub requested_parts: Option<PartBits>,
    pub hide_part_bits: HidePartBits,
}

impl DObjSemanticState {
    pub fn bind_pose(model: String, composition_revision: u32, pose_revision: u32) -> Self {
        Self {
            composition: DObjCompositionDescriptor::single(composition_revision, model),
            pose_revision,
            tag_rotations: Vec::new(),
            tree: None,
            requested_parts: None,
            hide_part_bits: HidePartBits::default(),
        }
    }

    pub fn one_leaf(
        model: String,
        clip: String,
        composition_revision: u32,
        pose_revision: u32,
        time: f32,
    ) -> Self {
        Self {
            composition: DObjCompositionDescriptor::single(composition_revision, model),
            pose_revision,
            tag_rotations: Vec::new(),
            tree: Some(XAnimTreeSnapshot {
                definition_revision: 1,
                state_revision: pose_revision,
                nodes: vec![XAnimSemanticNode {
                    parent: None,
                    kind: XAnimSemanticNodeKind::Leaf,
                    clip: Some(clip),
                    parts: None,
                    state: XAnimNodeState {
                        time,
                        old_time: time,
                        weight: 1.0,
                        goal_weight: 1.0,
                        rate: 1.0,
                        ..XAnimNodeState::default()
                    },
                }],
            }),
            requested_parts: None,
            hide_part_bits: HidePartBits::default(),
        }
    }

    pub fn resolve_request(
        &self,
        mut resolve_clip: impl FnMut(&str) -> Option<Arc<AnimClip>>,
    ) -> Result<DObjPoseRequest, SemanticResolveError> {
        let tree = self
            .tree
            .as_ref()
            .map(|tree| tree.resolve(&mut resolve_clip))
            .transpose()?;
        Ok(DObjPoseRequest {
            tree,
            tag_rotations: self.tag_rotations.clone(),
            requested_parts: self.requested_parts,
            hide_part_bits: self.hide_part_bits,
        })
    }
}

impl XAnimTreeSnapshot {
    pub fn active(
        runtime: &XAnimTreeRuntime,
        definition_revision: u32,
        state_revision: u32,
    ) -> Self {
        let mut mapping = vec![None; runtime.definition().nodes().len()];
        let mut nodes = Vec::new();
        for (index, definition) in runtime.definition().nodes().iter().enumerate() {
            let state = runtime.states()[index];
            if state.weight == 0.0
                || definition
                    .parent
                    .is_some_and(|parent| mapping[parent.0 as usize].is_none())
            {
                continue;
            }
            let (kind, clip, parts) = match &definition.kind {
                XAnimNodeKind::Blend => (XAnimSemanticNodeKind::Blend, None, None),
                XAnimNodeKind::Additive => (XAnimSemanticNodeKind::Additive, None, None),
                XAnimNodeKind::Leaf { clip, parts } => {
                    (XAnimSemanticNodeKind::Leaf, Some(clip.name.clone()), *parts)
                }
            };
            mapping[index] = Some(XAnimNodeId(nodes.len() as u16));
            nodes.push(XAnimSemanticNode {
                parent: definition
                    .parent
                    .and_then(|parent| mapping[parent.0 as usize]),
                kind,
                clip,
                parts,
                state,
            });
        }
        if nodes.is_empty() {
            nodes.push(XAnimSemanticNode {
                parent: None,
                kind: XAnimSemanticNodeKind::Blend,
                clip: None,
                parts: None,
                state: XAnimNodeState {
                    weight: 1.0,
                    goal_weight: 1.0,
                    ..Default::default()
                },
            });
        }
        Self {
            definition_revision,
            state_revision,
            nodes,
        }
    }

    pub fn resolve(
        &self,
        mut resolve_clip: impl FnMut(&str) -> Option<Arc<AnimClip>>,
    ) -> Result<XAnimTreeRuntime, SemanticResolveError> {
        let mut definitions = Vec::with_capacity(self.nodes.len());
        for (node, semantic) in self.nodes.iter().enumerate() {
            let kind = match semantic.kind {
                XAnimSemanticNodeKind::Blend => XAnimNodeKind::Blend,
                XAnimSemanticNodeKind::Additive => XAnimNodeKind::Additive,
                XAnimSemanticNodeKind::Leaf => {
                    let name = semantic
                        .clip
                        .as_deref()
                        .ok_or(SemanticResolveError::LeafClipMissing { node })?;
                    let clip = resolve_clip(name).ok_or_else(|| {
                        SemanticResolveError::ClipUnavailable {
                            node,
                            clip: name.to_owned(),
                        }
                    })?;
                    XAnimNodeKind::Leaf {
                        clip,
                        parts: semantic.parts,
                    }
                }
            };
            definitions.push(XAnimNodeDefinition {
                parent: semantic.parent,
                kind,
            });
        }
        let definition = Arc::new(XAnimTreeDefinition::new(definitions)?);
        let mut runtime = XAnimTreeRuntime::new(definition);
        for (node, semantic) in self.nodes.iter().enumerate() {
            runtime.set_state(XAnimNodeId(node as u16), semantic.state)?;
        }
        Ok(runtime)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticResolveError {
    LeafClipMissing { node: usize },
    ClipUnavailable { node: usize, clip: String },
    Tree(XAnimTreeError),
}

impl From<XAnimTreeError> for SemanticResolveError {
    fn from(value: XAnimTreeError) -> Self {
        Self::Tree(value)
    }
}
