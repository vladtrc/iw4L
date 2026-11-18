use crate::DObj;
use anim_iw4::PartBits;
use glam::Mat4;

/// Transfer an authored rig's complete pose by model slot and bone name. The
/// reference may have intermediate control bones absent from the destination.
/// Rest-space offsets preserve the destination's mesh, proportions and bone IDs.
#[derive(Clone, Debug)]
pub(crate) struct PoseRetarget {
    pub(crate) source: DObj,
    source_bones: Vec<Option<usize>>,
    offsets: Vec<Mat4>,
    parent_locals: Vec<Mat4>,
}

impl PoseRetarget {
    pub(crate) fn new(source: DObj, target: &DObj) -> Self {
        let source_rest = source.compose(&source.bind_locals(), Mat4::IDENTITY);
        let target_rest = target.compose(&target.bind_locals(), Mat4::IDENTITY);
        let source_bones: Vec<_> = target
            .bones
            .iter()
            .map(|bone| {
                let slot = &source.models[bone.model];
                (slot.base..slot.base + slot.bone_count)
                    .find(|&index| source.bones[index].name == bone.name)
            })
            .collect();
        let offsets = source_bones
            .iter()
            .enumerate()
            .map(|(index, source)| {
                source.map_or(Mat4::IDENTITY, |source| {
                    source_rest[source].inverse() * target_rest[index]
                })
            })
            .collect();
        let parent_locals = target
            .bones
            .iter()
            .enumerate()
            .map(|(index, bone)| {
                bone.parent.map_or(target_rest[index], |parent| {
                    target_rest[parent].inverse() * target_rest[index]
                })
            })
            .collect();
        Self {
            source,
            source_bones,
            offsets,
            parent_locals,
        }
    }

    pub(crate) fn source_parts(&self, parts: &PartBits) -> PartBits {
        let mut source = PartBits::default();
        for (target, bone) in self.source_bones.iter().enumerate() {
            if parts.get(target) {
                let mut bone = *bone;
                while let Some(index) = bone {
                    source.set(index);
                    bone = self.source.bones[index].parent;
                }
            }
        }
        source
    }

    pub(crate) fn apply(&self, target: &DObj, source: &[Mat4], placement: Mat4) -> Vec<Mat4> {
        let mut posed = Vec::with_capacity(target.bones.len());
        for (index, bone) in target.bones.iter().enumerate() {
            let matrix = match self.source_bones[index] {
                Some(source_bone) => placement * source[source_bone] * self.offsets[index],
                None => {
                    bone.parent.map_or(placement, |parent| posed[parent])
                        * self.parent_locals[index]
                }
            };
            posed.push(matrix);
        }
        posed
    }
}
