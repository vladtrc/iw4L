use crate::bullet_collision::AuthorityDObjState;
use crate::frame::FrameWorld;

pub(crate) fn bind(
    frame: &FrameWorld,
    weapon: u32,
    variant: u8,
) -> Result<AuthorityDObjState, String> {
    let content = frame.content();
    let weapons = content.weapons();
    let index = weapon as usize;
    let (base, _) = weapons
        .weapon_world_models
        .get(index)
        .ok_or("entity has no model")?;
    if variant != 0
        && !weapons
            .weapon_camouflage_slots
            .get(index)
            .is_some_and(|slots| slots.contains(&variant))
    {
        return Err("entity has no model".into());
    }
    let model = if variant == 0 {
        base.as_str()
    } else {
        weapons
            .weapon_world_model_variants
            .get(index)
            .and_then(|rows| rows.iter().find(|(slot, _)| *slot == variant))
            .map_or(base.as_str(), |(_, name)| name.as_str())
    };
    if model.is_empty() {
        return Err("entity has no model".into());
    }
    let capability = frame
        .model_capability(&model.to_ascii_lowercase())
        .flatten()
        .ok_or("entity has no model")?;
    let mut dobj = AuthorityDObjState::at_pose(model, Some(capability), [0.; 3], [0.; 3]);
    if let Some(rows) = weapons.weapon_world_model_attachments.get(index) {
        let attachments: Vec<_> = rows
            .iter()
            .map(|(name, tag)| {
                (
                    name.as_str(),
                    tag.as_str(),
                    frame.model_capability(&name.to_ascii_lowercase()).flatten(),
                )
            })
            .collect();
        dobj.set_attachments(&attachments);
    }
    if !dobj.has_part_model() {
        return Err("entity has no model".into());
    }
    Ok(dobj)
}

pub(crate) fn initial_mask(
    frame: &FrameWorld,
    weapon: u32,
    variant: u8,
) -> [u32; anim_iw4::PartBits::WORDS] {
    let mut hidden = xmodel_runtime::HidePartBits::default();
    let Ok(dobj) = bind(frame, weapon, variant) else {
        return *hidden.words();
    };
    let Ok(graph) = dobj.part_graph() else {
        return *hidden.words();
    };
    let content = frame.content();
    let Some((_, tags)) = content.weapons().weapon_world_models.get(weapon as usize) else {
        return *hidden.words();
    };
    let mut roots = std::collections::BTreeSet::new();
    for tag in tags {
        if let Some(bone) = graph.matching_bones(tag, None).next() {
            roots.insert(bone);
        } else {
            diag::error!(
                Sim,
                "item model {} has no hidden part {tag}",
                dobj.current_model
            );
        }
    }
    for bone in 0..graph.bones.len() {
        let mut ancestor = Some(bone);
        while let Some(index) = ancestor {
            if roots.contains(&index) {
                hidden.set(bone);
                break;
            }
            ancestor = graph.bones[index].parent;
        }
    }
    *hidden.words()
}
