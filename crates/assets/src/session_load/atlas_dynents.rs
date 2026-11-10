use crate::PreparedWorld;
use asset_core::world_frame::WorldFrame;
use asset_world::{DynEntDef, MapXModelAssetKey};
use bevy::math::{Mat4, Quat, Vec3};

pub(super) fn frame_pose(frame: WorldFrame) -> Mat4 {
    Mat4::from_cols(
        Vec3::from_array(frame.vector_to_scene([1.0, 0.0, 0.0])).extend(0.0),
        Vec3::from_array(frame.vector_to_scene([0.0, 1.0, 0.0])).extend(0.0),
        Vec3::from_array(frame.vector_to_scene([0.0, 0.0, 1.0])).extend(0.0),
        Vec3::from_array(frame.point_to_scene([0.0; 3])).extend(1.0),
    )
}

fn next_id(rows: &[DynEntDef]) -> usize {
    rows.iter()
        .map(|d| usize::from(d.index) + 1)
        .max()
        .unwrap_or(0)
}
fn rebased(value: u16, base: usize, what: &str) -> Result<u16, String> {
    u16::try_from(base + usize::from(value)).map_err(|_| format!("atlas {what} index overflow"))
}
fn script_id(world: usize, native: u32) -> Result<asset_world::ScriptModelId, String> {
    // Keep native source ordinals below the dynamic GSC presence range
    // (0x4000_0000), with a separate map domain in the high byte.
    if !(1..64).contains(&world) || native >= 1 << 24 {
        return Err("atlas script-model identity overflow".into());
    }
    Ok(asset_world::ScriptModelId::from_source_ordinal(
        (world as u32) << 24 | native,
    ))
}
fn quat_pose(origin: [f32; 3], quat: [f32; 4]) -> Mat4 {
    let q = Quat::from_array(quat);
    Mat4::from_rotation_translation(
        if q.length_squared() > 1e-12 {
            q.normalize()
        } else {
            Quat::IDENTITY
        },
        Vec3::from_array(origin),
    )
}

fn carried_brush(
    native: Mat4,
    local: asset_world::GfxBrushModelBounds,
    frame: WorldFrame,
) -> (Mat4, dpvs_iw4::Bounds) {
    let carry = frame_pose(frame);
    let xf = carry * native;
    let half = Vec3::from_array(local.half);
    let world_half = xf.x_axis.truncate().abs() * half.x
        + xf.y_axis.truncate().abs() * half.y
        + xf.z_axis.truncate().abs() * half.z;
    (
        xf * carry.inverse(),
        dpvs_iw4::Bounds::from_mid_half(
            xf.transform_point3(Vec3::from_array(local.mid)).to_array(),
            world_half.to_array(),
        ),
    )
}

// A prototype has no placement. Identity is a valid authored pose, and
// script claims take precedence over later dyn_ent claims, as on a plain map.
fn placed_brush_poses(
    model_n: usize,
    placements: impl IntoIterator<Item = (usize, Mat4)>,
) -> Vec<Option<Mat4>> {
    let mut poses = vec![None; model_n];
    for (model, pose) in placements {
        if model > 0
            && let Some(slot) = poses.get_mut(model)
            && slot.is_none()
        {
            *slot = Some(pose);
        }
    }
    poses
}

pub(super) struct Imported {
    pub poses: Vec<(usize, Mat4)>,
    pub bounds: Vec<(u16, dpvs_iw4::Bounds)>,
}

/// Import local owners before their native scene captures are consumed. The
/// caller appends world geometry with the same brush-model base afterwards.
pub(super) fn absorb(
    host: &mut PreparedWorld,
    extra: &mut PreparedWorld,
    world: usize,
    frame: WorldFrame,
    materials: &[usize],
    brush_base: usize,
) -> Result<Imported, String> {
    let carry = frame_pose(frame);
    let rotation = Quat::from_mat4(&carry);
    let model_base = next_id(&host.dyn_ents.models);
    let brush_id_base = next_id(&host.dyn_ents.brushes);
    let model_n = extra.draw.as_ref().map_or(0, |d| d.brush_models.len());
    let poses = placed_brush_poses(
        model_n,
        extra
            .script_brush_models
            .iter()
            .map(|brush| {
                (
                    brush.cmodel_handle as usize,
                    asset_world::brush_model_pose(brush.origin, brush.angles),
                )
            })
            .chain(extra.dyn_ents.brushes.iter().map(|def| {
                (
                    usize::from(def.brush_model),
                    quat_pose(def.origin, def.quat),
                )
            })),
    );
    diag::info!(
        World,
        "atlas brush owners world={world} placed={} unplaced={}",
        poses.iter().flatten().count(),
        poses.iter().skip(1).filter(|p| p.is_none()).count()
    );
    let mut carried_poses = Vec::new();
    let mut carried_bounds = Vec::new();
    for (i, p) in poses
        .into_iter()
        .enumerate()
        .skip(1)
        .filter_map(|(i, pose)| Some((i, pose?)))
    {
        let model = rebased(
            i.try_into().map_err(|_| "native brush index overflow")?,
            brush_base,
            "brush model",
        )?;
        let local = extra
            .draw
            .as_ref()
            .and_then(|d| d.brush_model_bounds.get(i))
            .ok_or("atlas brush model has no bounds")?;
        let (pose, bounds) = carried_brush(p, *local, frame);
        carried_poses.push((usize::from(model), pose));
        carried_bounds.push((model, bounds));
    }

    let mut names: std::collections::BTreeSet<String> = extra
        .dyn_ents
        .models
        .iter()
        .filter_map(|d| d.xmodel.clone())
        .collect();
    for instance in &extra.script_model_instances {
        names.insert(instance.current_model.0.clone());
        names.extend(
            instance
                .dobj_state
                .composition
                .models
                .iter()
                .map(|model| model.model.clone()),
        );
    }
    extra.map_xmodel_scene_assets.retain_names(&names);
    extra
        .map_xmodel_scene_assets
        .remap_walk_materials(materials);
    host.map_xmodel_scene_assets
        .absorb_atlas(std::mem::take(&mut extra.map_xmodel_scene_assets), world)?;
    for instance in &mut extra.script_model_instances {
        let native = instance.id.source_ordinal();
        instance.id = script_id(world, native)?;
        instance.current_model = MapXModelAssetKey::atlas(world, &instance.current_model.0);
        for model in &mut instance.dobj_state.composition.models {
            model.model = MapXModelAssetKey::atlas(world, &model.model).0;
        }
        instance.transform.translation = carry.transform_point3(instance.transform.translation);
        instance.transform.rotation = rotation * instance.transform.rotation;
        instance.lighting_origin = frame.point_to_scene(instance.lighting_origin);
        diag::info!(
            World,
            "atlas script model world={world} native_id={native} id={} model={:?} origin={:?}",
            instance.id.source_ordinal(),
            instance.current_model.0,
            instance.transform.translation
        );
    }
    host.script_model_instances
        .append(&mut extra.script_model_instances);
    for def in &mut extra.dyn_ents.models {
        let native = def.index;
        def.index = rebased(native, model_base, "dyn_ent model")?;
        def.origin = frame.point_to_scene(def.origin);
        let native_rotation = Quat::from_array(def.quat);
        def.quat = (rotation
            * if native_rotation.length_squared() > 1e-12 {
                native_rotation.normalize()
            } else {
                Quat::IDENTITY
            })
        .to_array();
        if let Some(name) = def.xmodel.as_mut() {
            *name = MapXModelAssetKey::atlas(world, name).0;
        }
        diag::info!(
            World,
            "atlas dynent model world={world} native_id={native} id={} model={:?} origin={:?}",
            def.index,
            def.xmodel,
            def.origin
        );
    }
    for def in &mut extra.dyn_ents.brushes {
        let native = def.index;
        def.index = rebased(native, brush_id_base, "dyn_ent brush")?;
        if def.brush_model != 0 {
            def.brush_model = rebased(def.brush_model, brush_base, "brush model")?;
        }
        // physics_brush_model is a local clip owner, retained in its world's clip.
        def.origin = frame.point_to_scene(def.origin);
        let native_rotation = Quat::from_array(def.quat);
        def.quat = (rotation
            * if native_rotation.length_squared() > 1e-12 {
                native_rotation.normalize()
            } else {
                Quat::IDENTITY
            })
        .to_array();
        diag::info!(
            World,
            "atlas dynent brush world={world} native_id={native} id={} brush_model={} origin={:?}",
            def.index,
            def.brush_model,
            def.origin
        );
    }
    host.dyn_ents.models.append(&mut extra.dyn_ents.models);
    host.dyn_ents.brushes.append(&mut extra.dyn_ents.brushes);
    host.dyn_ents.truncated += extra.dyn_ents.truncated;
    host.dyn_ents.missing_xmodel += extra.dyn_ents.missing_xmodel;
    host.dyn_ents.missing_preset += extra.dyn_ents.missing_preset;
    Ok(Imported {
        poses: carried_poses,
        bounds: carried_bounds,
    })
}
