use super::AuthorityDObjState;

impl AuthorityDObjState {
    pub fn set_script_tree(&mut self, tree: Option<xmodel_runtime::XAnimTreeSnapshot>) {
        if self.play_anim.is_none() && self.semantic_state.tree == tree {
            return;
        }
        self.play_anim = None;
        self.pose_revision = self.pose_revision.wrapping_add(1);
        self.semantic_state.tree = tree;
        self.semantic_state.pose_revision = self.pose_revision;
        self.current_collision = None;
        self.materialized_pose_revision = None;
    }

    pub(super) fn composed(&self) -> Option<xmodel_runtime::DObj> {
        let capability = self.capability.as_ref()?;
        let mut models = vec![(&capability.pose, None)];
        models.extend(self.attached.iter().map(|(attached, tag)| {
            (
                &attached.pose,
                Some(xmodel_runtime::Attach {
                    parent_model: 0,
                    tag: tag.clone(),
                }),
            )
        }));
        xmodel_runtime::DObj::build(&models).ok()
    }
}
