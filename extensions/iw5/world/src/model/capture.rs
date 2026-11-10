use asset_iw4::size as sz;
use asset_material::MaterialCatalog;
use asset_model::*;
use bevy::prelude::{Quat, Vec3};
use xmodel_runtime::ModelPoseSrc;
const BONE_STRIDE: u16 = 64;
fn copy_packed_vertex_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    vertex: ext_iw5_fastfile::Ptr,
) -> Option<[u8; sz::GFX_PACKED_VERTEX]> {
    let bytes = stream.slice_at(vertex, 0, sz::GFX_PACKED_VERTEX).ok()?;
    let mut packed = [0u8; sz::GFX_PACKED_VERTEX];
    packed.copy_from_slice(bytes);
    Some(packed)
}

pub fn capture_fpv_skel_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    geometry: ext_iw5_fastfile::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<FpvSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if super::policy::classify(&name) != Some(ModelKind::Fpv) {
        return None;
    }
    capture_model_skel_iw5(stream, strings, geometry, name, materials, None)
}

pub fn capture_world_weapon_skel_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    geometry: ext_iw5_fastfile::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if super::policy::classify(&name) != Some(ModelKind::WorldWeapon) {
        return None;
    }
    capture_model_skel_iw5(stream, strings, geometry, name, materials, None)
}

pub fn capture_xmodel_skel_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    geometry: ext_iw5_fastfile::XModelGeometry,
    materials: &MaterialCatalog,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    capture_model_skel_iw5(stream, strings, geometry, name, Some(materials), None)
}

pub fn capture_xmodel_skel_iw5_with_shared(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    geometry: ext_iw5_fastfile::XModelGeometry,
    materials: &MaterialCatalog,
    shared: &SharedXModelSurfaces,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    capture_model_skel_iw5(
        stream,
        strings,
        geometry,
        name,
        Some(materials),
        Some(shared),
    )
}

pub fn capture_body_skel_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    geometry: ext_iw5_fastfile::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if super::policy::classify(&name) != Some(ModelKind::Soldier) {
        return None;
    }
    capture_model_skel_iw5(stream, strings, geometry, name, materials, None)
}

fn capture_model_skel_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    geometry: ext_iw5_fastfile::XModelGeometry,
    name: String,
    materials: Option<&MaterialCatalog>,
    shared: Option<&SharedXModelSurfaces>,
) -> Option<ModelSkel> {
    use ext_iw5_fastfile::size as iw5sz;

    let bone_names = geometry.bone_names?;
    let base_mat = geometry.base_mat?;

    let mut bones = Vec::with_capacity(geometry.num_bones);
    let mut bone_name_strs = Vec::with_capacity(geometry.num_bones);
    let mut tag_view = None;
    let mut tag_weapon = None;
    for i in 0..geometry.num_bones {
        let id = stream.u16_at(bone_names, i * 2).ok()?;
        let bone_name = strings.get(stream, id).unwrap_or("").to_owned();
        if bone_name == "tag_view" && tag_view.is_none() {
            tag_view = Some(i);
        }
        if bone_name == "tag_weapon" && tag_weapon.is_none() {
            tag_weapon = Some(i);
        }
        bone_name_strs.push(bone_name);
        let m = base_mat.at(i * iw5sz::DOBJ_ANIM_MAT);
        bones.push(BoneBind {
            quat: [
                stream.f32_at(m, 0).ok()?,
                stream.f32_at(m, 4).ok()?,
                stream.f32_at(m, 8).ok()?,
                stream.f32_at(m, 12).ok()?,
            ],
            trans: [
                stream.f32_at(m, 16).ok()?,
                stream.f32_at(m, 20).ok()?,
                stream.f32_at(m, 24).ok()?,
            ],
        });
    }

    let num_child = geometry.num_bones.saturating_sub(geometry.num_root_bones);
    let pose = capture_pose_src_iw5(stream, &geometry, &bone_name_strs, &bones, num_child);
    let bone_part_classification = match geometry.part_classification {
        Some(classes) => (0..geometry.num_bones)
            .map(|bone| stream.u8_at(classes, bone))
            .collect::<Result<Vec<_>, _>>()
            .ok()?,
        None => Vec::new(),
    };
    let bone_collision = capture_bone_collision_iw5(stream, geometry);
    let material_at = |surface_index: usize| {
        geometry.material_handles.and_then(|handles| {
            materials?.material_index(asset_material::AssetPointerIdentity {
                block: handles.block,
                offset: handles.offset + (surface_index * stream.pointer_bytes()) as u32,
            })
        })
    };

    let Some(surfaces) = geometry.surfaces else {
        let surfaces_name = stream
            .cstr(geometry.surfaces_name?)
            .ok()?
            .trim_start_matches(',');
        let (source, _) = shared?.0.get(surfaces_name)?;
        if source.surface_vertex_ranges.len() != geometry.surface_count
            || source.vert_skin.iter().any(|skin| {
                skin.bones.iter().enumerate().any(|(index, bone)| {
                    skin.weight_is_nonzero(index) && usize::from(*bone) >= geometry.num_bones
                })
            })
        {
            return None;
        }
        return Some(ModelSkel {
            name,
            bones,
            bone_names: bone_name_strs,
            tag_view,
            tag_weapon,
            pose,
            bone_collision,
            bone_part_classification,
            surface_materials: (0..geometry.surface_count).map(material_at).collect(),
            radius: geometry.radius,
            ..(**source).clone()
        });
    };

    let mut positions = Vec::new();
    let mut indices = Vec::new();
    let mut packed_vertices = Vec::new();
    let mut surface_materials = Vec::with_capacity(geometry.surface_count);
    let mut surface_vertex_ranges = Vec::with_capacity(geometry.surface_count);
    let mut surface_index_ranges = Vec::with_capacity(geometry.surface_count);
    let mut surface_part_bits = Vec::with_capacity(geometry.surface_count);
    let mut surface_deformed = Vec::with_capacity(geometry.surface_count);
    let mut surface_vert_list_count = Vec::with_capacity(geometry.surface_count);
    let mut vert_skin = Vec::new();
    let mut rigid_verts = 0usize;
    let mut blend_verts = 0usize;

    for surface_index in 0..geometry.surface_count {
        let surface = surfaces.at(surface_index * stream.layout(iw5sz::XSURFACE, 88));
        let vertex_count = stream.u16_at(surface, 2).ok()? as usize;
        let tri_count = stream.u16_at(surface, 4).ok()? as usize;
        let vert_list_count = stream
            .u32_at(
                surface,
                stream.layout(iw5sz::XSURFACE_VERT_LIST_COUNT_OFF, 48),
            )
            .ok()? as usize;
        let flags = stream.u8_at(surface, iw5sz::XSURFACE_FLAGS_OFF).ok()?;
        surface_deformed.push(Some(flags & iw5sz::XSURFACE_FLAG_DEFORMED != 0));
        surface_vert_list_count.push(Some(u32::try_from(vert_list_count).ok()?));

        let part_bits_off = stream.layout(44, 64);
        let mut part_bits = [0u32; 6];
        for (i, word) in part_bits.iter_mut().enumerate() {
            *word = stream.u32_at(surface, part_bits_off + i * 4).ok()?;
        }
        surface_part_bits.push(part_bits);
        let vertices = match stream
            .ptr_at(surface, stream.layout(iw5sz::XSURFACE_VERTS0_OFF, 40))
            .ok()?
        {
            ext_iw5_fastfile::ZonePtr::Offset(p) => stream.resolve_alias(p),
            _ => return None,
        };
        let triangles = match stream
            .ptr_at(surface, iw5sz::XSURFACE_TRI_INDICES_OFF)
            .ok()?
        {
            ext_iw5_fastfile::ZonePtr::Offset(p) => stream.resolve_alias(p),
            _ => return None,
        };

        let base = positions.len();
        let index_start = indices.len();
        let surface_skins = decode_surface_skin_iw5(
            stream,
            surface,
            vertex_count,
            vert_list_count,
            geometry.num_bones,
        )?;

        for (vertex_index, surface_skins_entry) in surface_skins[..vertex_count].iter().enumerate()
        {
            let vertex = vertices.at(vertex_index * iw5sz::GFX_PACKED_VERTEX);
            positions.push([
                stream.f32_at(vertex, 0).ok()?,
                stream.f32_at(vertex, 4).ok()?,
                stream.f32_at(vertex, 8).ok()?,
            ]);
            packed_vertices.push(copy_packed_vertex_iw5(stream, vertex)?);

            let skin = *surface_skins_entry;
            if skin.is_rigid() {
                rigid_verts += 1;
            } else {
                blend_verts += 1;
            }
            vert_skin.push(skin);
        }
        for index_offset in 0..tri_count * 3 {
            let index = stream.u16_at(triangles, index_offset * 2).ok()?;
            if usize::from(index) >= vertex_count {
                return None;
            }
            indices.push(base as u32 + u32::from(index));
        }
        surface_vertex_ranges.push((base, vertex_count));
        surface_index_ranges.push((index_start, indices.len() - index_start));
        surface_materials.push(material_at(surface_index));
    }

    Some(ModelSkel {
        name,
        bones,
        bone_collision,
        bone_part_classification,
        bone_names: bone_name_strs,
        tag_view,
        tag_weapon,
        pose,
        positions: positions.into(),
        indices: indices.into(),
        surface_materials,
        surface_vertex_ranges,
        surface_index_ranges,
        surface_part_bits,
        surface_deformed,
        surface_vert_list_count,
        vert_skin: vert_skin.into(),
        rigid_verts,
        blend_verts,
        packed_vertices: packed_vertices.into(),
        radius: geometry.radius,
        bounds: None,
        contents: None,
        coll_lod: 0,
        coll_surfs: Vec::new(),
        movement_brushes: Vec::new(),

        mount_tag: None,

        lod: None,
        lod_smc: None,
        lod_part_bits: None,
        lod_surf_span: [(0, 0); 4],
    })
}

fn capture_bone_collision_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: ext_iw5_fastfile::XModelGeometry,
) -> Vec<Option<BoneCollision>> {
    let (Some(info), Some(classes)) = (geometry.bone_info, geometry.part_classification) else {
        return Vec::new();
    };
    (0..geometry.num_bones)
        .map(|bone| {
            let row = info.at(bone * ext_iw5_fastfile::size::XBONE_INFO);
            let radius_sq = stream.f32_at(row, 24).ok()?;
            let midpoint = [
                stream.f32_at(row, 0).ok()?,
                stream.f32_at(row, 4).ok()?,
                stream.f32_at(row, 8).ok()?,
            ];
            let half_size = [
                stream.f32_at(row, 12).ok()?,
                stream.f32_at(row, 16).ok()?,
                stream.f32_at(row, 20).ok()?,
            ];
            let finite = radius_sq.is_finite()
                && radius_sq > 0.0
                && midpoint.iter().all(|value| value.is_finite())
                && half_size
                    .iter()
                    .all(|value| value.is_finite() && *value >= 0.0);
            let part_classification = stream.u8_at(classes, bone).ok()?;
            finite.then_some(BoneCollision {
                midpoint,
                half_size,
                radius_sq,
                part_classification,
            })
        })
        .collect()
}

fn capture_pose_src_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: &ext_iw5_fastfile::XModelGeometry,
    bone_names: &[String],
    bones: &[BoneBind],
    num_child: usize,
) -> Option<ModelPoseSrc> {
    use ext_iw5_fastfile::size as iw5sz;

    let parent_list = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.parent_list?;
        (0..num_child)
            .map(|i| stream.u8_at(arr, i).ok())
            .collect::<Option<Vec<_>>>()?
    };
    let quats = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.quats?;
        (0..num_child)
            .map(|i| {
                let o = i * iw5sz::XMODEL_QUAT;
                Some([
                    stream.i16_at(arr, o).ok()?,
                    stream.i16_at(arr, o + 2).ok()?,
                    stream.i16_at(arr, o + 4).ok()?,
                    stream.i16_at(arr, o + 6).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?
    };
    let trans = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.trans?;
        (0..num_child)
            .map(|i| {
                let o = i * 12;
                Some([
                    stream.f32_at(arr, o).ok()?,
                    stream.f32_at(arr, o + 4).ok()?,
                    stream.f32_at(arr, o + 8).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?
    };
    let base_mat = bones
        .iter()
        .map(|b| {
            (
                Quat::from_xyzw(b.quat[0], b.quat[1], b.quat[2], b.quat[3]),
                Vec3::from_array(b.trans),
            )
        })
        .collect();
    Some(ModelPoseSrc {
        name: geometry
            .name
            .and_then(|p| stream.cstr(p).ok())
            .unwrap_or("")
            .to_owned(),
        num_bones: geometry.num_bones,
        num_root_bones: geometry.num_root_bones,
        scale: geometry.scale,
        no_scale_part_bits: geometry.no_scale_part_bits,
        bone_names: bone_names.to_vec(),
        parent_list,
        quats,
        trans,
        base_mat,
        root_rest: None,
    })
}

fn decode_surface_skin_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    surface: ext_iw5_fastfile::Ptr,
    vertex_count: usize,
    vert_list_count: usize,
    num_bones: usize,
) -> Option<Vec<VertSkin>> {
    use ext_iw5_fastfile::size as iw5sz;

    let mut skins = vec![VertSkin::default(); vertex_count];
    let bone_at = |raw: u16| -> Option<u8> {
        if !raw.is_multiple_of(BONE_STRIDE) {
            return None;
        }
        let bone = raw / BONE_STRIDE;
        ((bone as usize) < num_bones)
            .then(|| u8::try_from(bone).ok())
            .flatten()
    };

    let mut rigid_assigned = 0usize;
    if vert_list_count > 0
        && let Ok(ext_iw5_fastfile::ZonePtr::Offset(list_ptr)) =
            stream.ptr_at(surface, stream.layout(iw5sz::XSURFACE_VERT_LIST_OFF, 56))
    {
        let list = stream.resolve_alias(list_ptr);
        let mut vertex = 0usize;
        for i in 0..vert_list_count {
            let entry = list.at(i * stream.layout(iw5sz::XRIGID_VERT_LIST, 16));
            let bone_offset = stream.u16_at(entry, 0).ok()?;
            let run = stream.u16_at(entry, 2).ok()? as usize;
            let bone = bone_at(bone_offset)?;
            for _ in 0..run {
                if vertex >= vertex_count {
                    break;
                }
                skins[vertex] = VertSkin::rigid(bone);
                vertex += 1;
            }
        }
        rigid_assigned = vertex;
        if vertex == vertex_count {
            return Some(skins);
        }
    }

    let vi = surface.at(stream.layout(iw5sz::XSURFACE_VERT_INFO_OFF, 24));
    let counts = [
        stream.i16_at(vi, 0).ok()?.max(0) as usize,
        stream.i16_at(vi, 2).ok()?.max(0) as usize,
        stream.i16_at(vi, 4).ok()?.max(0) as usize,
        stream.i16_at(vi, 6).ok()?.max(0) as usize,
    ];
    let blend_ptr = match stream.ptr_at(vi, 8).ok()? {
        ext_iw5_fastfile::ZonePtr::Offset(p) => stream.resolve_alias(p),
        ext_iw5_fastfile::ZonePtr::Null if counts.iter().all(|&c| c == 0) => {
            if rigid_assigned > 0 {
                return Some(skins);
            }
            if num_bones > 1 {
                diag::warn!(
                    Zone,
                    "iw5 xsurface skin: {vertex_count} verts, vertListCount={vert_list_count}, \
                     no blend buckets, {num_bones} bones — cannot resolve ownership (typed gap)"
                );
            }
            for skin in &mut skins {
                *skin = VertSkin::rigid(0);
            }
            return Some(skins);
        }
        _ => return None,
    };

    let mut cursor = 0usize;
    let mut vertex = rigid_assigned;
    let take = |cursor: &mut usize, n: usize| -> Option<()> {
        *cursor = cursor.checked_add(n)?;
        Some(())
    };
    for (bucket, count) in counts.iter().enumerate() {
        let influences = bucket + 1;
        for _ in 0..*count {
            let words = 1 + (influences - 1) * 2;
            let start = cursor;
            take(&mut cursor, words)?;
            let mut skin = VertSkin::default();
            let b0 = stream.u16_at(blend_ptr, start * 2).ok()?;
            skin.bones[0] = bone_at(b0)?;
            let mut remaining = 1.0f32;
            for extra in 1..influences {
                let off = start + 1 + (extra - 1) * 2;
                let b = stream.u16_at(blend_ptr, off * 2).ok()?;
                let raw = stream.u16_at(blend_ptr, off * 2 + 2).ok()?;
                let w = dpvs_iw4::skin_blend_weight(raw);
                skin.bones[extra] = bone_at(b)?;
                skin.set_weight(extra, w);
                skin.set_extra_weight(extra, raw);
                remaining -= w;
            }
            skin.set_weight(0, remaining);
            if vertex >= vertex_count {
                return None;
            }
            skins[vertex] = skin;
            vertex += 1;
        }
    }
    if vertex != vertex_count {
        return None;
    }
    Some(skins)
}

pub fn retain_surfaces(
    shared: &mut SharedXModelSurfaces,
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: ext_iw5_fastfile::XModelGeometry,
    skel: std::sync::Arc<ModelSkel>,
) {
    if geometry.surfaces.is_some()
        && let Some(name) = geometry.surfaces_name.and_then(|p| stream.cstr(p).ok())
        && !name.starts_with(',')
    {
        shared.0.insert(name.to_owned(), (skel, 0));
    }
}
