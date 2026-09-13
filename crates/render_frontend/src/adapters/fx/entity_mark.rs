use bevy::prelude::*;
use marks_iw4::{FxAllocMarkRequest, FxMarkStagingPoint, FxMarkStagingTri};
use render_fx::{EntityMarkAttachment, EntityMarkRequest, EntityMarks};

use crate::prepare::scene::world::{WorldScene, WorldScriptModelInstance};

pub(crate) fn queue(host: &fx::FxSystemHost, marks: &EntityMarks) {
    if !host.mark_receivers.fx_marks_ents {
        return;
    }
    let (
        Some(entity),
        Some(origin),
        Some(axis),
        Some(rotation),
        Some(radius),
        Some(material),
        Some(color),
    ) = (
        host.last_decal_mark_entity,
        host.last_decal_origin,
        host.last_decal_axis,
        host.last_decal_rotation,
        host.last_decal_size0,
        host.last_decal_mat0.as_ref(),
        host.last_decal_color,
    )
    else {
        return;
    };
    marks.lock().pending.push(EntityMarkRequest {
        entity,
        origin,
        axis: fx_iw4::fx_impact_mark_axis(axis, rotation),
        radius,
        material: material.clone(),
        color,
    });
}

pub(crate) fn generate(
    host: &mut fx::FxSystemHost,
    scene: &WorldScene,
    marks: &EntityMarks,
    owners: &Query<(&WorldScriptModelInstance, &Transform, &Visibility)>,
    xanims: Option<&assets::PreparedXAnims>,
    models: &assets::MapXModelSceneCatalog,
) {
    let mut state = marks.lock();
    let pending = std::mem::take(&mut state.pending);
    let before = state.unsupported_triangles;
    let before_receivers = state.unsupported_receivers;
    let mut mesh = host
        .last_mark_mesh
        .take()
        .expect("world mark generation precedes entity marks");
    for (owner, transform, visibility) in owners.iter() {
        let Some(entity) = owner.gentity_number else {
            continue;
        };
        if *visibility == Visibility::Hidden {
            continue;
        }
        if !pending.iter().any(|r| r.entity == entity)
            && !state.attached.iter().any(|a| a.entity == entity)
        {
            continue;
        }
        let requested = pending.iter().filter(|r| r.entity == entity).count() as u64;
        if owner.dobj_state.composition.models.is_empty() {
            state.unsupported_receivers += requested;
            continue;
        }
        let Some((specs, skels)) = crate::adapters::anim::script_model::collect_presented_models(
            models,
            &owner.dobj_state,
        ) else {
            state.unsupported_receivers += requested;
            continue;
        };
        let Ok(dobj) = assets::DObj::build(&specs) else {
            state.unsupported_receivers += requested;
            continue;
        };
        let Ok(request) = owner
            .dobj_state
            .resolve_request(|name| xanims?.0.clip(assets::AssetNamespace::Iw4, name))
        else {
            state.unsupported_receivers += requested;
            continue;
        };
        let Ok(pose) = assets::dobj::pose_dobj(&dobj, &request, transform.to_matrix()) else {
            state.unsupported_receivers += requested;
            continue;
        };
        let skin = dobj.skin_matrices(&pose);
        for request in pending.iter().filter(|r| r.entity == entity) {
            for (model_index, skel) in skels.iter().enumerate() {
                let Some(slot) = dobj.models.get(model_index) else {
                    state.unsupported_receivers += 1;
                    continue;
                };
                let Some(desc) = owner.dobj_state.composition.models.get(model_index) else {
                    state.unsupported_receivers += 1;
                    continue;
                };
                stage(
                    host,
                    scene,
                    &mut state,
                    request,
                    RigidReceiver {
                        owner,
                        skel,
                        skin: &skin,
                        model_index,
                        bone_base: slot.base,
                        model: desc.model.as_str(),
                        revision: owner.dobj_state.composition.revision,
                    },
                    models,
                );
            }
        }
        if !host.mark_receivers.fx_marks || !host.mark_receivers.fx_marks_ents {
            continue;
        }
        for attachment in state.attached.iter().filter(|a| {
            a.entity == entity
                && a.revision == owner.dobj_state.composition.revision
                && owner
                    .dobj_state
                    .composition
                    .models
                    .iter()
                    .any(|m| m.model == a.model)
        }) {
            if owner.dobj_state.hide_part_bits.get(attachment.bone) {
                continue;
            }
            let Some(matrix) = skin.get(attachment.bone).copied() else {
                continue;
            };
            let Some(mark) = host.marks.constructed(attachment.handle).copied() else {
                continue;
            };
            let result = host.marks.append_model_mark(
                &mut mesh,
                attachment.handle,
                matrix
                    .transform_point3(Vec3::from_array(mark.origin))
                    .to_array(),
                matrix
                    .transform_vector3(Vec3::from_array(mark.tex_coord_axis))
                    .to_array(),
                &|mut point| {
                    point.xyz = matrix
                        .transform_point3(Vec3::from_array(point.xyz))
                        .to_array();
                    point.normal = matrix
                        .transform_vector3(Vec3::from_array(point.normal))
                        .normalize()
                        .to_array();
                    point
                },
            );
            if result.is_err() {
                break;
            }
        }
    }
    if state.unsupported_triangles != before {
        diag::warn!(
            World,
            "entity marks: {} triangles lack a single rigid bone receiver",
            state.unsupported_triangles - before
        );
    }
    if state.unsupported_receivers != before_receivers {
        diag::warn!(
            World,
            "entity marks: {} requests lack composed rigid pose data",
            state.unsupported_receivers - before_receivers
        );
    }
    host.gfx_mark_surf_n = Some(mesh.budget.surf_n);
    host.gfx_mark_vert_n = Some(mesh.budget.vert_n);
    host.gfx_mark_index_n = Some(mesh.budget.index_n);
    host.gfx_mark_packed_n = Some(mesh.packed.len() as u32);
    host.last_mark_mesh = Some(mesh);
}

struct RigidReceiver<'a> {
    owner: &'a WorldScriptModelInstance,
    skel: &'a assets::ModelSkel,
    skin: &'a [Mat4],
    model_index: usize,
    bone_base: usize,
    model: &'a str,
    revision: u32,
}

fn stage(
    host: &mut fx::FxSystemHost,
    scene: &WorldScene,
    state: &mut render_fx::EntityMarkStore,
    request: &EntityMarkRequest,
    receiver: RigidReceiver<'_>,
    models: &assets::MapXModelSceneCatalog,
) {
    let RigidReceiver {
        owner,
        skel,
        skin,
        model_index,
        bone_base,
        model,
        revision,
    } = receiver;
    let Some(submodel) = u8::try_from(model_index).ok() else {
        state.unsupported_receivers += 1;
        return;
    };
    let model_key = assets::MapXModelAssetKey(model.to_string());
    let mark_bits = super::world_mark::runtime_material_by_name(scene, &request.material)
        .and_then(|m| m.surface_type_bits);
    let planes =
        marks_iw4::fx_mark_fragment_clip_planes(request.origin, request.axis, request.radius);
    let mut by_bone =
        std::collections::BTreeMap::<usize, (Vec<FxMarkStagingTri>, Vec<FxMarkStagingPoint>)>::new(
        );
    let mut fragment = [marks_iw4::FxWorldMarkPoint::ZERO; marks_iw4::R_MARK_CHOP_MAX_POINTS];
    for surface in skel.surfaces_for_lod(0) {
        if skel.surface_deformed.get(surface) != Some(&Some(false)) {
            continue;
        }
        let material = models.surface_material(&model_key, surface);
        let (flags, bits) = super::world_mark::receiver_allow_inputs(scene, surface, material);
        if !marks_iw4::fx_mark_include_in_world_clip(marks_iw4::fx_mark_allow(
            flags, bits, mark_bits,
        )) {
            continue;
        }
        let Some(&(start, count)) = skel.surface_index_ranges.get(surface) else {
            continue;
        };
        let Some(indices) = skel.indices.get(start..start + count) else {
            continue;
        };
        for triangle in indices.chunks_exact(3) {
            let ids = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            let Some(weights) = skel.vert_skin.get(ids[0]) else {
                continue;
            };
            let bone = weights.bones[0] as usize;
            if bone > u8::MAX as usize {
                state.unsupported_triangles += 1;
                continue;
            }
            let global = bone_base + bone;
            if !ids.iter().all(|&i| {
                skel.vert_skin.get(i).is_some_and(|w| {
                    w.bones[0] as usize == bone && w.weights == [1.0, 0.0, 0.0, 0.0]
                })
            }) {
                state.unsupported_triangles += 1;
                continue;
            }
            if owner.dobj_state.hide_part_bits.get(global) {
                continue;
            }
            let Some(matrix) = skin.get(global).copied() else {
                continue;
            };

            if !matrix
                .to_scale_rotation_translation()
                .0
                .abs_diff_eq(Vec3::ONE, 0.0001)
            {
                state.unsupported_triangles += 1;
                continue;
            }
            let [Some(p0), Some(p1), Some(p2)] = ids.map(|i| skel.positions.get(i).copied()) else {
                continue;
            };
            let world =
                [p0, p1, p2].map(|p| matrix.transform_point3(Vec3::from_array(p)).to_array());
            if marks_iw4::fx_mark_is_triangle_rejected(
                request.axis[0],
                world[0],
                world[1],
                world[2],
            ) {
                continue;
            }
            let n = marks_iw4::fx_mark_chop_world_triangle_points(
                &planes,
                world[0],
                world[1],
                world[2],
                &mut fragment,
            ) as usize;
            if n < 3 {
                continue;
            }
            let [Some(n0), Some(n1), Some(n2)] = ids.map(|i| skel.normals.get(i).copied()) else {
                continue;
            };
            let inverse = matrix.inverse();
            for p in &mut fragment[..n] {
                p.xyz = inverse.transform_point3(Vec3::from_array(p.xyz)).to_array();
            }
            let (tris, points) = by_bone.entry(global).or_default();
            let used_t = tris.len() as u32;
            let used_p = points.len() as u32;
            tris.resize(
                marks_iw4::R_MARK_FRAGMENTS_MAX_TRIS as usize,
                FxMarkStagingTri::ZERO,
            );
            points.resize(
                marks_iw4::R_MARK_FRAGMENTS_MAX_POINTS as usize,
                FxMarkStagingPoint::ZERO,
            );

            let Some(context) = dobj_mark_context(request.entity, bone, submodel) else {
                state.unsupported_triangles += 1;
                continue;
            };
            match marks_iw4::fx_mark_emit_brush_fragment(
                used_t,
                used_p,
                marks_iw4::R_MARK_FRAGMENTS_MAX_TRIS,
                marks_iw4::R_MARK_FRAGMENTS_MAX_POINTS,
                &fragment[..n],
                [0.0; 2],
                [0.0; 2],
                [0.0; 2],
                n0,
                n1,
                n2,
                context,
                tris,
                points,
            ) {
                Ok((t, p)) => {
                    tris.truncate(t as usize);
                    points.truncate(p as usize);
                }
                Err(_) => {
                    tris.truncate(used_t as usize);
                    points.truncate(used_p as usize);
                }
            }
        }
    }
    for (bone, (tris, points)) in by_bone {
        if tris.is_empty() {
            continue;
        }
        let inverse = skin[bone].inverse();
        let context = tris[0].context;
        let req = FxAllocMarkRequest {
            any_marks: true,
            tri_count: tris.len() as u32,
            point_count: points.len() as u32,
            origin: inverse
                .transform_point3(Vec3::from_array(request.origin))
                .to_array(),
            radius: request.radius,
            tex_coord_axis: inverse
                .transform_vector3(Vec3::from_array(request.axis[1]))
                .to_array(),
            native_color: request.color,
            material: 0,
            frame_count: host.frame_stamp,
            first_tri_context: u32::from_le_bytes(
                context[..4].try_into().expect("four context bytes"),
            ),
        };
        if let Some(handle) = host.marks.alloc_mark_from_go_callback(req, &tris, &points) {
            host.marks
                .set_material_name(handle, Some(&request.material));
            host.note_models_go_fire();
            state.attached.push(EntityMarkAttachment {
                entity: request.entity,
                model: model.to_string(),
                revision,
                bone,
                handle,
            });
        }
    }
}

fn dobj_mark_context(entity: u16, local_bone: usize, submodel: u8) -> Option<[u8; 7]> {
    let bone = u8::try_from(local_bone).ok()?;
    Some([3, bone, entity as u8, (entity >> 8) as u8, submodel, 0, 0])
}
