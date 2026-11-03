use asset_iw4::size as sz;
use asset_material::MaterialCatalog;
use asset_model::*;
use bevy::prelude::{Quat, Vec3};
use xmodel_runtime::ModelPoseSrc;
const BONE_STRIDE: u16 = 64;
fn copy_packed_vertex_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    vertex: ext_t5_fastfile::Ptr,
) -> Option<[u8; sz::GFX_PACKED_VERTEX]> {
    let bytes = stream.slice_at(vertex, 0, sz::GFX_PACKED_VERTEX).ok()?;
    let mut packed = [0u8; sz::GFX_PACKED_VERTEX];
    packed.copy_from_slice(bytes);
    Some(packed)
}

pub fn capture_fpv_skel_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    strings: &ext_t5_fastfile::ScriptStrings,
    geometry: ext_t5_fastfile::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if super::policy::classify(&name) != Some(ModelKind::Fpv) {
        return None;
    }
    capture_model_skel_t5(stream, strings, geometry, name, materials)
}

pub fn capture_body_skel_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    strings: &ext_t5_fastfile::ScriptStrings,
    geometry: ext_t5_fastfile::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if super::policy::classify(&name) != Some(ModelKind::Soldier) {
        return None;
    }
    capture_model_skel_t5(stream, strings, geometry, name, materials)
}

pub fn capture_world_weapon_skel_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    strings: &ext_t5_fastfile::ScriptStrings,
    geometry: ext_t5_fastfile::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if super::policy::classify(&name) != Some(ModelKind::WorldWeapon) {
        return None;
    }
    capture_model_skel_t5(stream, strings, geometry, name, materials)
}

pub fn capture_xmodel_skel_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    strings: &ext_t5_fastfile::ScriptStrings,
    geometry: ext_t5_fastfile::XModelGeometry,
    materials: &MaterialCatalog,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    capture_model_skel_t5(stream, strings, geometry, name, Some(materials))
}

fn capture_model_skel_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    strings: &ext_t5_fastfile::ScriptStrings,
    geometry: ext_t5_fastfile::XModelGeometry,
    name: String,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    const XSURFACE: usize = 68;
    const DOBJ_ANIM_MAT: usize = 32;
    const GFX_PACKED_VERTEX: usize = 32;
    const XRIGID_VERT_LIST: usize = 12;

    let surfaces = geometry.surfaces?;
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
        let m = base_mat.at(i * DOBJ_ANIM_MAT);
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
    let pose = capture_pose_src_t5(stream, &geometry, &bone_name_strs, &bones, num_child);

    let mut positions = Vec::new();
    let mut indices = Vec::new();
    let mut surface_materials = Vec::with_capacity(geometry.surface_count);
    let mut surface_vertex_ranges = Vec::with_capacity(geometry.surface_count);
    let mut surface_index_ranges = Vec::with_capacity(geometry.surface_count);
    let mut vert_skin = Vec::new();
    let mut rigid_verts = 0usize;
    let mut blend_verts = 0usize;
    let mut packed_vertices = Vec::new();
    let mut surface_part_bits = Vec::with_capacity(geometry.surface_count);
    let mut surface_deformed = Vec::with_capacity(geometry.surface_count);
    let mut surface_vert_list_count = Vec::with_capacity(geometry.surface_count);

    for surface_index in 0..geometry.surface_count {
        let surface = surfaces.at(surface_index * XSURFACE);
        let vert_list_count = stream.u8_at(surface, 1).ok()? as usize;

        surface_deformed.push(Some(stream.u16_at(surface, 2).ok()? & 0x80 != 0));
        surface_vert_list_count.push(Some(vert_list_count as u32));
        let vertex_count = stream.u16_at(surface, 4).ok()? as usize;
        let tri_count = stream.u16_at(surface, 6).ok()? as usize;
        let vertices = match stream.ptr_at(surface, 32).ok()? {
            ext_t5_fastfile::ZonePtr::Offset(p) => stream.resolve_alias(p),
            _ => return None,
        };
        let triangles = match stream.ptr_at(surface, 12).ok()? {
            ext_t5_fastfile::ZonePtr::Offset(p) => stream.resolve_alias(p),
            _ => return None,
        };
        let mut part_bits = [0u32; 6];
        for i in 0..ext_t5_fastfile::size::XSURFACE_PART_BITS_WORDS {
            part_bits[i] = stream
                .u32_at(
                    surface,
                    ext_t5_fastfile::size::XSURFACE_PART_BITS_OFF + i * 4,
                )
                .ok()?;
        }

        let base = positions.len();
        let index_start = indices.len();
        let surface_skins = decode_surface_skin_t5(
            stream,
            surface,
            vertex_count,
            vert_list_count,
            geometry.num_bones,
            XRIGID_VERT_LIST,
        )?;

        for vertex_index in 0..vertex_count {
            let vertex = vertices.at(vertex_index * GFX_PACKED_VERTEX);
            packed_vertices.push(copy_packed_vertex_t5(stream, vertex)?);
            positions.push([
                stream.f32_at(vertex, 0).ok()?,
                stream.f32_at(vertex, 4).ok()?,
                stream.f32_at(vertex, 8).ok()?,
            ]);

            let skin = surface_skins[vertex_index];
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
        surface_part_bits.push(part_bits);
        surface_materials.push(geometry.material_handles.and_then(|handles| {
            materials?.material_index(asset_material::AssetPointerIdentity {
                block: handles.block,
                offset: handles.offset + (surface_index * 4) as u32,
            })
        }));
    }

    let bone_part_classification = match geometry.part_classification {
        Some(classes) => (0..geometry.num_bones)
            .map(|bone| stream.u8_at(classes, bone))
            .collect::<Result<Vec<_>, _>>()
            .ok()?,
        None => Vec::new(),
    };
    let (bone_collision, coll_surfs) = capture_collision_t5(stream, geometry).or_else(|| {
        diag::warn!(Zone, "T5 XModel {name}: malformed collision payload");
        None
    })?;
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
        contents: Some(geometry.contents),
        coll_lod: geometry.coll_lod,
        coll_surfs,
        movement_brushes: capture_movement_brushes_t5(stream, geometry)?,

        mount_tag: None,
        lod: Some(asset_model::ModelLodSelector {
            family: asset_core::FamilyId::new("t5"),
            lod_start: 0,
            num_lods: geometry.num_lods,
            lod_dist: geometry.lod_dist,
        }),
        lod_smc: None,
        lod_part_bits: None,
        lod_surf_span: geometry.lod_surf_span,
    })
}

fn capture_movement_brushes_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    geometry: ext_t5_fastfile::XModelGeometry,
) -> Option<Vec<xmodel_runtime::ModelMovementBrush>> {
    use ext_t5_fastfile::{Ptr, ZonePtr, size};
    let pointer = |p: Ptr, offset| match stream.ptr_at(p, offset).ok()? {
        ZonePtr::Offset(p) => Some(stream.resolve_alias(p)),
        _ => None,
    };
    if geometry.collision_map_count == 0 {
        return Some(Vec::new());
    }
    let maps = geometry.collision_maps?;
    if stream.ptr_at(maps, 0).ok()? == ZonePtr::Null {
        return Some(Vec::new());
    }
    let list = pointer(maps, 0)?;
    let count = stream.u32_at(list, 0).ok()? as usize;
    if count == 0 {
        return Some(Vec::new());
    }
    let geoms = pointer(list, 4)?;
    let mut brushes = Vec::new();
    for i in 0..count {
        let geom = geoms.at(i * size::PHYS_GEOM_INFO);
        if stream.ptr_at(geom, 0).ok()? == ZonePtr::Null {
            continue;
        }
        let brush = pointer(geom, 0)?;
        let mut planes = Vec::new();
        let mut plane_surface_flags = Vec::new();
        for side in 0..2 {
            for axis in 0..3 {
                let mut plane = [0.0; 4];
                plane[axis] = if side == 0 { -1.0 } else { 1.0 };
                plane[3] = stream.f32_at(brush, side * 16 + axis * 4).ok()? * plane[axis];
                planes.push(plane);
                plane_surface_flags.push(stream.u32_at(brush, 60 + (side * 3 + axis) * 4).ok()?);
            }
        }
        let side_count = stream
            .u32_at(brush, size::BRUSH_WRAPPER_NUMSIDES_OFF)
            .ok()? as usize;
        if side_count != 0 {
            let sides = pointer(brush, size::BRUSH_WRAPPER_SIDES_OFF)?;
            for j in 0..side_count {
                let side = sides.at(j * size::CBRUSH_SIDE);
                let plane = pointer(side, 0)?;
                planes.push([
                    stream.f32_at(plane, 0).ok()?,
                    stream.f32_at(plane, 4).ok()?,
                    stream.f32_at(plane, 8).ok()?,
                    stream.f32_at(plane, 12).ok()?,
                ]);
                plane_surface_flags.push(stream.u32_at(side, 8).ok()?);
            }
        }
        brushes.push(xmodel_runtime::ModelMovementBrush {
            planes,
            contents: stream.u32_at(brush, 12).ok()?,
            plane_surface_flags,
        });
    }
    Some(brushes)
}

fn capture_collision_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    geometry: ext_t5_fastfile::XModelGeometry,
) -> Option<(
    Vec<Option<BoneCollision>>,
    Vec<xmodel_runtime::CollSurfCollision>,
)> {
    use ext_t5_fastfile::{Ptr, ZonePtr, size as t5sz};
    let bounds = |row: Ptr, off| {
        let mut midpoint = [0.0; 3];
        let mut half_size = [0.0; 3];
        for axis in 0..3 {
            let min = stream.f32_at(row, off + axis * 4).ok()?;
            let max = stream.f32_at(row, off + 12 + axis * 4).ok()?;
            if !min.is_finite() || !max.is_finite() || min > max {
                return None;
            }
            midpoint[axis] = (min + max) * 0.5;
            half_size[axis] = (max - min) * 0.5;
        }
        Some((midpoint, half_size))
    };
    let mut bones = Vec::new();
    if let Some(info) = geometry.bone_info {
        let classes = geometry.part_classification?;
        for bone in 0..geometry.num_bones {
            let row = info.at(bone * t5sz::XBONE_INFO);

            let radius_sq = stream.f32_at(row, 36).ok()?;
            if radius_sq == 0.0 {
                bones.push(None);
                continue;
            }
            if !radius_sq.is_finite() || radius_sq < 0.0 {
                return None;
            }
            let (midpoint, half_size) = bounds(row, 0)?;
            bones.push(Some(BoneCollision {
                midpoint,
                half_size,
                radius_sq,
                part_classification: stream.u8_at(classes, bone).ok()?,
            }));
        }
    }
    let mut surfs = Vec::with_capacity(geometry.num_coll_surfs);
    if geometry.num_coll_surfs != 0 {
        let arr = geometry.coll_surfs?;
        for i in 0..geometry.num_coll_surfs {
            let row = arr.at(i * t5sz::XMODEL_COLL_SURF);
            let (midpoint, half_size) = bounds(row, 8)?;
            let bone = u16::try_from(stream.i32_at(row, 32).ok()?).ok()?;
            if usize::from(bone) >= geometry.num_bones {
                return None;
            }
            let num = usize::try_from(stream.i32_at(row, 4).ok()?).ok()?;
            let mut tris = Vec::with_capacity(num);
            if num != 0 {
                let ZonePtr::Offset(arr) = stream.ptr_at(row, 0).ok()? else {
                    return None;
                };
                let arr = stream.resolve_alias(arr);
                for i in 0..num {
                    let tri = arr.at(i * t5sz::XMODEL_COLL_TRI);
                    let vec4 = |off| {
                        let mut v = [0.0; 4];
                        for (j, value) in v.iter_mut().enumerate() {
                            *value = stream.f32_at(tri, off + j * 4).ok()?;
                            if !value.is_finite() {
                                return None;
                            }
                        }
                        Some(v)
                    };
                    tris.push(xmodel_runtime::CollTri {
                        plane: vec4(0)?,
                        svec: vec4(16)?,
                        tvec: vec4(32)?,
                    });
                }
            }
            surfs.push(xmodel_runtime::CollSurfCollision {
                bone,
                contents: stream.u32_at(row, 36).ok()?,
                surf_flags: stream.u32_at(row, 40).ok()?,
                midpoint,
                half_size,
                tris,
            });
        }
    }
    Some((bones, surfs))
}

fn capture_pose_src_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    geometry: &ext_t5_fastfile::XModelGeometry,
    bone_names: &[String],
    bones: &[BoneBind],
    num_child: usize,
) -> Option<ModelPoseSrc> {
    use ext_t5_fastfile::size as t5sz;

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
                let o = i * t5sz::XMODEL_QUAT;
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
                let o = i * t5sz::XMODEL_TRANS_STRIDE;
                Some([
                    stream.f32_at(arr, o).ok()?,
                    stream.f32_at(arr, o + 4).ok()?,
                    stream.f32_at(arr, o + 8).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?
    };
    let base_mat: Vec<(Quat, Vec3)> = bones
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
        scale: 1.0,
        no_scale_part_bits: [0; 6],
        bone_names: bone_names.to_vec(),
        parent_list,
        quats,
        trans,
        base_mat,
        root_rest: None,
    })
}

fn decode_surface_skin_t5(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    surface: ext_t5_fastfile::Ptr,
    vertex_count: usize,
    vert_list_count: usize,
    num_bones: usize,
    rigid_stride: usize,
) -> Option<Vec<VertSkin>> {
    let mut skins = vec![VertSkin::default(); vertex_count];
    let bone_at = |raw: u16| -> Option<u8> {
        if raw % BONE_STRIDE != 0 {
            return None;
        }
        let bone = raw / BONE_STRIDE;
        ((bone as usize) < num_bones)
            .then(|| u8::try_from(bone).ok())
            .flatten()
    };

    if vert_list_count > 0 {
        if let Ok(ext_t5_fastfile::ZonePtr::Offset(list_ptr)) = stream.ptr_at(surface, 40) {
            let list = stream.resolve_alias(list_ptr);
            let mut vertex = 0usize;
            for i in 0..vert_list_count {
                let entry = list.at(i * rigid_stride);
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
            if vertex == vertex_count {
                return Some(skins);
            }
        }
    }

    let vi = surface.at(16);
    let counts = [
        stream.i16_at(vi, 0).ok()?.max(0) as usize,
        stream.i16_at(vi, 2).ok()?.max(0) as usize,
        stream.i16_at(vi, 4).ok()?.max(0) as usize,
        stream.i16_at(vi, 6).ok()?.max(0) as usize,
    ];
    let blend_ptr = match stream.ptr_at(vi, 8).ok()? {
        ext_t5_fastfile::ZonePtr::Offset(p) => stream.resolve_alias(p),
        ext_t5_fastfile::ZonePtr::Null if counts.iter().all(|&c| c == 0) => {
            for skin in &mut skins {
                *skin = VertSkin::rigid(0);
            }
            return Some(skins);
        }
        _ => return None,
    };

    let mut cursor = 0usize;
    let mut vertex = 0usize;
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
