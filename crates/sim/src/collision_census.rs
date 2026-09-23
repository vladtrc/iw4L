use crate::bullet_collision::PlayerCollisionPose;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelCollisionCensus {
    pub key: String,

    pub present: bool,

    pub bones: u32,
    pub bone_boxes: u32,

    pub coll_lod: i16,
    pub coll_surfs: u32,
    pub coll_tris: u32,
    pub contents: Option<u32>,
}

impl ModelCollisionCensus {
    pub fn of(key: &str, capability: Option<&xmodel_runtime::RetainedModelCapability>) -> Self {
        let Some(cap) = capability else {
            return Self {
                key: key.to_owned(),
                ..Self::default()
            };
        };
        Self {
            key: key.to_owned(),
            present: true,
            bones: cap.pose.num_bones as u32,
            bone_boxes: cap.bone_collision.iter().flatten().count() as u32,
            coll_lod: cap.coll_lod,
            coll_surfs: cap.coll_surfs.len() as u32,
            coll_tris: cap.coll_surfs.iter().map(|s| s.tris.len() as u32).sum(),
            contents: cap.contents,
        }
    }

    pub fn clip(&self) -> &'static str {
        if !self.present {
            "absent"
        } else if self.coll_tris > 0 {
            "colltris"
        } else if self.bone_boxes > 0 {
            "boxes"
        } else {
            "none"
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldClipCensus {
    pub brushes: u32,
    pub bsp_nodes: u32,
    pub bsp_leaves: u32,
    pub leafbrushes: u32,
    pub mesh_tris: u32,
    pub cmodels: u32,
    pub static_models: u32,
    pub static_models_with_tris: u32,
    pub pen_table_loaded: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerClipCensus {
    pub poses: u32,
    pub with_bones: u32,
    pub aabb_only: u32,
    pub min_bones: u32,
    pub max_bones: u32,
    pub with_head_bone: u32,
    pub materialize_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityClipCensus {
    pub rows: u32,

    pub colltris: u32,
    pub boxes_only: u32,
    pub brush_only: u32,

    pub not_bullet_solid: u32,
    pub no_collision_authored: u32,

    pub no_dobj: u32,
    pub no_capability: u32,
    pub materialize_failed: u32,

    pub linked_brushes: u32,

    pub no_clip_sample: Vec<String>,
}

const NO_CLIP_SAMPLES: usize = 12;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollisionCensus {
    pub world: WorldClipCensus,
    pub players: PlayerClipCensus,
    pub entities: EntityClipCensus,
    pub kits: Vec<ModelCollisionCensus>,
}

pub(crate) fn player_clip_census(
    poses: &[PlayerCollisionPose],
    materialize_error: Option<String>,
) -> PlayerClipCensus {
    let mut out = PlayerClipCensus {
        min_bones: u32::MAX,
        materialize_error,
        ..PlayerClipCensus::default()
    };
    for pose in poses {
        let bones = pose.bones.len() as u32;
        out.poses += 1;
        if bones > 0 {
            out.with_bones += 1;
        } else {
            out.aabb_only += 1;
        }
        out.min_bones = out.min_bones.min(bones);
        out.max_bones = out.max_bones.max(bones);
        if pose
            .bones
            .iter()
            .any(|bone| hud_iw4::obituary_is_headshot(bone.part_classification))
        {
            out.with_head_bone += 1;
        }
    }
    if out.poses == 0 {
        out.min_bones = 0;
    }
    out
}

pub(crate) fn entity_clip_census(
    owners: &[crate::bullet_collision::EntityCollisionCapabilities],
    mask: u32,
) -> EntityClipCensus {
    use crate::bullet_collision::dobj_contents_match_mask;

    let mut out = EntityClipCensus::default();
    for owner in owners {
        out.rows += 1;
        out.linked_brushes += owner.linked_brushes.len() as u32;
        let contents = owner
            .dobj
            .as_ref()
            .and_then(|state| state.capability.as_ref())
            .and_then(|capability| capability.contents);
        let collision = owner
            .dobj
            .as_ref()
            .and_then(|state| state.current_collision.as_ref());
        match collision {
            Some(collision)
                if crate::bullet_collision::coll_tris_clip_available(
                    collision.coll.as_ref(),
                    mask,
                ) =>
            {
                out.colltris += 1;
                continue;
            }
            Some(collision) if !collision.bones.is_empty() => {
                out.boxes_only += 1;
                continue;
            }
            _ => {}
        }
        let Some(state) = owner.dobj.as_ref() else {
            if owner.linked_brushes.is_empty() {
                out.no_dobj += 1;
            } else {
                out.brush_only += 1;
            }
            continue;
        };
        if !dobj_contents_match_mask(contents, mask) {
            out.not_bullet_solid += 1;
            continue;
        }
        if state.capability.is_none() {
            out.no_capability += 1;
        } else if state.materialize_error.is_some() {
            out.materialize_failed += 1;
        } else if !owner.linked_brushes.is_empty() {
            out.brush_only += 1;
            continue;
        } else {
            out.no_collision_authored += 1;
        }
        if out.no_clip_sample.len() < NO_CLIP_SAMPLES
            && !out.no_clip_sample.contains(&state.current_model)
        {
            out.no_clip_sample.push(state.current_model.clone());
        }
    }
    out
}
