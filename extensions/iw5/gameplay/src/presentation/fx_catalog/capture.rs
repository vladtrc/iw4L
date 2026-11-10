use asset_game::*;
use asset_iw4::size::{self as sz, fx_elem};
use asset_material::{AssetPointerIdentity, MaterialCatalog};
use ext_iw5_fastfile::{Ptr, ZonePtr, ZoneStream};
use fx_iw4::*;
use std::collections::HashMap;
pub fn capture(
    catalog: &mut FxCatalog,
    s: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: ext_iw5_fastfile::FxEffectDefGeometry,
    materials: &MaterialCatalog,
) -> ext_iw5_fastfile::Result<()> {
    let Some(name) = geometry
        .name
        .and_then(|p| s.cstr(p).ok())
        .filter(|name| !name.is_empty())
    else {
        catalog.capture_gaps += 1;
        return Ok(());
    };
    let view = FxEffectDefView {
        flags: s.i32_at(geometry.header, s.layout(4, 8))?,
        msec_looping_life: s.i32_at(geometry.header, s.layout(12, 16))?,
        looping_count: geometry.looping_count,
        one_shot_count: geometry.one_shot_count,
        emission_count: geometry.emission_count,
    };
    let mut elems = Vec::with_capacity(geometry.elem_def_count);
    if let Some(arr) = geometry.elem_defs {
        for i in 0..geometry.elem_def_count {
            let ptr = arr.at(i * s.layout(ext_iw5_fastfile::size::FX_ELEM_DEF, 288));
            let Some(elem) = capture_elem(s, ptr, materials, &HashMap::new()) else {
                catalog.capture_gaps += 1;
                return Ok(());
            };
            elems.push(elem);
        }
    } else if geometry.elem_def_count > 0 {
        catalog.capture_gaps += 1;
        return Ok(());
    }
    let namespace = crate::ID;
    catalog.insert_captured(OwnedFxEffectDef {
        namespace,
        name: name.to_owned(),
        view,
        header_raw: pack_fx_effect_header(&view),
        elems,
    });
    Ok(())
}

fn capture_elem(
    s: &ZoneStream<'_>,
    p: Ptr,
    materials: &MaterialCatalog,
    xmodel_names: &HashMap<Ptr, Ptr>,
) -> Option<OwnedFxElemDef> {
    let raw = s
        .slice_at(p, 0, s.layout(ext_iw5_fastfile::size::FX_ELEM_DEF, 288))
        .ok()?
        .to_vec();
    let view = if s.pointer_bytes() == 8 {
        fx_iw4::elem_def_view_x64(&raw)?
    } else {
        elem_def_view(&raw)?
    };
    let vel_count = view.vel_interval_count as usize + 1;
    let vis_count = view.vis_state_interval_count as usize + 1;
    let vel_samples = copy_ptr_array(
        s,
        p,
        s.layout(0xb4, 184),
        vel_count,
        sz::FX_ELEM_VEL_STATE_SAMPLE,
    );
    let vel_graph_local = parse_vel_graph_channel(&vel_samples, vel_count, false);
    let vel_graph_world = parse_vel_graph_channel(&vel_samples, vel_count, true);
    let vis_samples = copy_ptr_array(
        s,
        p,
        s.layout(0xb8, 192),
        vis_count,
        sz::FX_ELEM_VIS_STATE_SAMPLE,
    );
    let visuals = capture_visuals(s, p, &view, materials, xmodel_names);
    let (effect_on_impact, effect_on_impact_hint) =
        capture_named_child(read_name_field(s, p, s.layout(216, 232)));
    let (effect_on_death, effect_on_death_hint) =
        capture_named_child(read_name_field(s, p, s.layout(220, 240)));
    let (effect_emitted, effect_emitted_hint) =
        capture_named_child(read_name_field(s, p, s.layout(224, 248)));
    let has_extended = !matches!(s.ptr_at(p, s.layout(0xf4, 272)).ok()?, ZonePtr::Null);
    let trail_def = if view.elem_type == elem_type::TRAIL {
        capture_trail_def(s, p)
    } else {
        None
    };
    let spark_fountain_def = if view.elem_type == elem_type::SPARK_FOUNTAIN {
        capture_spark_fountain_def(s, p)
    } else {
        None
    };
    Some(OwnedFxElemDef {
        view,
        raw,
        vel_samples,
        vel_graph_local,
        vel_graph_world,
        vis_samples,
        visuals,
        effect_on_impact,
        effect_on_impact_hint,
        effect_on_death,
        effect_on_death_hint,
        effect_emitted,
        effect_emitted_hint,
        has_extended,
        trail_def,
        spark_fountain_def,
        phys_pieces: Vec::new(),
    })
}

fn capture_trail_def(s: &ZoneStream<'_>, elem: Ptr) -> Option<OwnedFxTrailDef> {
    let ZonePtr::Offset(trail) = s.ptr_at(elem, s.layout(0xf4, 272)).ok()? else {
        return None;
    };
    let trail = s.resolve_alias(trail);
    let header = s.slice_at(trail, 0, s.layout(sz::FX_TRAIL_DEF, 48)).ok()?;
    if header.len() < s.layout(sz::FX_TRAIL_DEF, 48) {
        return None;
    }
    let scroll_time_msec = i32::from_le_bytes(header[0x00..0x04].try_into().ok()?);
    let repeat_dist = i32::from_le_bytes(header[0x04..0x08].try_into().ok()?);
    let inv_split_dist = f32::from_le_bytes(header[0x08..0x0c].try_into().ok()?);
    let inv_split_arc_dist = f32::from_le_bytes(header[0x0c..0x10].try_into().ok()?);
    let inv_split_time = f32::from_le_bytes(header[0x10..0x14].try_into().ok()?);
    let vert_count = i32::from_le_bytes(header[0x14..0x18].try_into().ok()?).max(0) as usize;
    let ind_count = s.i32_at(trail, s.layout(0x1c, 32)).ok()?.max(0) as usize;

    let vert_bytes = copy_ptr_array(s, trail, 0x18, vert_count, sz::FX_TRAIL_VERTEX);
    let mut verts = Vec::with_capacity(vert_count);
    for chunk in vert_bytes.as_chunks::<{ sz::FX_TRAIL_VERTEX }>().0.iter() {
        let pos = [
            f32::from_le_bytes(chunk[0x00..0x04].try_into().ok()?),
            f32::from_le_bytes(chunk[0x04..0x08].try_into().ok()?),
        ];
        let normal = [
            f32::from_le_bytes(chunk[0x08..0x0c].try_into().ok()?),
            f32::from_le_bytes(chunk[0x0c..0x10].try_into().ok()?),
        ];
        let tex_coord = f32::from_le_bytes(chunk[0x10..0x14].try_into().ok()?);
        verts.push(FxTrailVertex {
            pos,
            normal,
            tex_coord,
        });
    }

    let ind_bytes = copy_ptr_array(s, trail, s.layout(0x20, 40), ind_count, 2);
    let mut inds = Vec::with_capacity(ind_count);
    for chunk in ind_bytes.as_chunks::<2>().0.iter() {
        inds.push(u16::from_le_bytes(*chunk));
    }

    Some(OwnedFxTrailDef {
        scroll_time_msec,
        repeat_dist,
        inv_split_dist,
        inv_split_arc_dist,
        inv_split_time,
        verts,
        inds,
    })
}

fn capture_spark_fountain_def(s: &ZoneStream<'_>, elem: Ptr) -> Option<OwnedFxSparkFountainDef> {
    let ZonePtr::Offset(def) = s.ptr_at(elem, s.layout(0xf4, 272)).ok()? else {
        return None;
    };
    let def = s.resolve_alias(def);
    let header = s.slice_at(def, 0, FX_SPARK_FOUNTAIN_DEF_SIZE).ok()?;
    if header.len() < FX_SPARK_FOUNTAIN_DEF_SIZE {
        return None;
    }
    let f32_at = |off: usize| -> Option<f32> {
        Some(f32::from_le_bytes(header[off..off + 4].try_into().ok()?))
    };
    let spark_count = i32::from_le_bytes(
        header[FX_SPARK_FOUNTAIN_DEF_OFF_SPARK_COUNT..FX_SPARK_FOUNTAIN_DEF_OFF_SPARK_COUNT + 4]
            .try_into()
            .ok()?,
    );
    Some(OwnedFxSparkFountainDef {
        gravity: f32_at(0x00)?,
        bounce_frac: f32_at(0x04)?,
        bounce_rand: f32_at(0x08)?,
        spark_spacing: f32_at(0x0c)?,
        spark_length: f32_at(0x10)?,
        spark_count,
        loop_time: f32_at(0x18)?,
        vel_min: f32_at(0x1c)?,
        vel_max: f32_at(0x20)?,
        vel_cone_frac: f32_at(0x24)?,
        rest_speed: f32_at(0x28)?,
        boost_time: f32_at(0x2c)?,
        boost_factor: f32_at(0x30)?,
    })
}

fn capture_visuals(
    s: &ZoneStream<'_>,
    p: Ptr,
    view: &FxElemDefView,
    materials: &MaterialCatalog,
    xmodel_names: &HashMap<Ptr, Ptr>,
) -> Vec<OwnedFxVisual> {
    let t = view.elem_type;
    let visual_count = view.visual_count as usize;
    let vis = p.at(s.layout(0xbc, 200));

    if matches!(t, elem_type::OMNI_LIGHT | elem_type::SPOT_LIGHT) || visual_count == 0 {
        return vec![OwnedFxVisual::None];
    }
    if t == elem_type::DECAL {
        let ptr = s.ptr_at(vis, 0);
        let Ok(ZonePtr::Offset(arr)) = ptr else {
            return (0..visual_count.max(1))
                .map(|_| OwnedFxVisual::UnresolvedMaterial)
                .collect();
        };
        let arr = s.resolve_alias(arr);
        return (0..visual_count.max(1))
            .map(|i| {
                let m = arr.at(i * s.layout(sz::FX_ELEM_MARK_VISUALS, 16));
                mark_pair(
                    resolve_material_visual(s, materials, m.at(0)),
                    resolve_material_visual(s, materials, m.at(s.layout(4, 8))),
                )
            })
            .collect();
    }
    if fx_elem::is_sprite(t) {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ZonePtr::Offset(arr)) = ptr else {
                let kind = match ptr {
                    Ok(ZonePtr::Following) => "following",
                    Ok(ZonePtr::Insert) => "insert",
                    Ok(ZonePtr::Null) => "null",
                    Ok(ZonePtr::Offset(_)) | Err(_) => "read_err",
                };
                return (0..visual_count)
                    .map(|_| OwnedFxVisual::Material {
                        material: FxElemMaterial::unresolved_presence(true, false),
                        hint: Some(kind.to_string()),
                        material_namespace: materials.capture_ns(),
                        authored: AuthoredRef {
                            slot: true,
                            alias: false,
                        },
                    })
                    .collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| {
                    resolve_material_visual(
                        s,
                        materials,
                        arr.at(i * s.layout(sz::FX_ELEM_VISUALS, 8)),
                    )
                })
                .collect();
        }
        return vec![resolve_material_visual(s, materials, vis)];
    }
    if t == elem_type::MODEL {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ZonePtr::Offset(arr)) = ptr else {
                return (0..visual_count).map(|_| OwnedFxVisual::None).collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| {
                    let slot = arr.at(i * s.layout(sz::FX_ELEM_VISUALS, 8));
                    model_visual(read_xmodel_name(s, slot, xmodel_names))
                })
                .collect();
        }
        return vec![model_visual(read_xmodel_name(s, vis, xmodel_names))];
    }
    if t == elem_type::RUNNER {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ZonePtr::Offset(arr)) = ptr else {
                return (0..visual_count).map(|_| OwnedFxVisual::None).collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| {
                    runner_visual(read_name_field(
                        s,
                        arr.at(i * s.layout(sz::FX_ELEM_VISUALS, 8)),
                        0,
                    ))
                })
                .collect();
        }
        return vec![runner_visual(read_name_field(s, vis, 0))];
    }
    if t == elem_type::SOUND {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ZonePtr::Offset(arr)) = ptr else {
                return (0..visual_count).map(|_| OwnedFxVisual::None).collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| {
                    sound_visual(read_name_field(
                        s,
                        arr.at(i * s.layout(sz::FX_ELEM_VISUALS, 8)),
                        0,
                    ))
                })
                .collect();
        }
        return vec![sound_visual(read_name_field(s, vis, 0))];
    }
    vec![OwnedFxVisual::None]
}

fn resolve_material_visual(
    s: &ZoneStream<'_>,
    materials: &MaterialCatalog,
    slot: Ptr,
) -> OwnedFxVisual {
    let alias = match s.ptr_at(slot, 0) {
        Ok(ZonePtr::Offset(target)) => Some(s.resolve_alias(target)),
        _ => None,
    };
    let index = materials
        .material_index(identity(slot))
        .or_else(|| alias.and_then(|target| materials.material_index(identity(target))));
    let name = index
        .and_then(|i| materials.materials.get(i.get()).map(|m| m.name.to_string()))
        .filter(|n| !n.is_empty());

    OwnedFxVisual::Material {
        material: FxElemMaterial::unresolved_presence(true, alias.is_some()),
        hint: name,
        material_namespace: materials.capture_ns(),
        authored: AuthoredRef {
            slot: true,
            alias: alias.is_some(),
        },
    }
}

fn read_xmodel_name(
    s: &ZoneStream<'_>,
    slot: Ptr,
    xmodel_names: &HashMap<Ptr, Ptr>,
) -> Option<String> {
    if let Some(name) = xmodel_names.get(&slot).copied() {
        return s.cstr(name).ok().map(str::to_owned);
    }
    let ZonePtr::Offset(body) = s.ptr_at(slot, 0).ok()? else {
        return None;
    };
    let body = s.resolve_alias(body);
    if let Some(name) = xmodel_names.get(&body).copied() {
        return s.cstr(name).ok().map(str::to_owned);
    }
    match s.ptr_at(body, 0).ok()? {
        ZonePtr::Offset(name) => s.cstr(s.resolve_alias(name)).ok().map(str::to_owned),
        _ => s.cstr(body).ok().map(str::to_owned),
    }
}

fn read_name_field(s: &ZoneStream<'_>, p: Ptr, field: usize) -> String {
    match s.ptr_at(p, field) {
        Ok(ZonePtr::Offset(name)) => s.cstr(s.resolve_alias(name)).ok().unwrap_or("").to_owned(),
        _ => String::new(),
    }
}

fn copy_ptr_array(
    s: &ZoneStream<'_>,
    parent: Ptr,
    field: usize,
    count: usize,
    stride: usize,
) -> Vec<u8> {
    if count == 0 {
        return Vec::new();
    }
    let Ok(ZonePtr::Offset(arr)) = s.ptr_at(parent, field) else {
        return Vec::new();
    };
    let arr = s.resolve_alias(arr);
    let nbytes = count.saturating_mul(stride);
    s.slice_at(arr, 0, nbytes)
        .map(|b| b.to_vec())
        .unwrap_or(Vec::new())
}

fn identity(p: Ptr) -> AssetPointerIdentity {
    AssetPointerIdentity {
        block: p.block,
        offset: p.offset,
    }
}
