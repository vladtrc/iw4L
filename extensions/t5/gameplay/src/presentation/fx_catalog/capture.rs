use asset_game::*;
use asset_iw4::size::fx_elem;
use asset_material::{AssetPointerIdentity, MaterialCatalog};
use fx_iw4::*;
pub fn capture(
    catalog: &mut FxCatalog,
    s: &ext_t5_fastfile::ZoneStream<'_>,
    geometry: ext_t5_fastfile::FxEffectDefGeometry,
    materials: &MaterialCatalog,
    material_name: &impl Fn(ext_t5_fastfile::Ptr) -> Option<String>,
) {
    use ext_t5_fastfile::size as sz_t5;
    let Some(name_ptr) = geometry.name else {
        catalog.capture_gaps += 1;
        return;
    };
    let Ok(name) = s.cstr(name_ptr) else {
        catalog.capture_gaps += 1;
        return;
    };
    if name.is_empty() {
        catalog.capture_gaps += 1;
        return;
    }
    let Ok(t5_header) = s.slice_at(geometry.header, 0, sz_t5::FX_EFFECT_DEF) else {
        catalog.capture_gaps += 1;
        return;
    };
    if t5_header.len() < sz_t5::FX_EFFECT_DEF {
        catalog.capture_gaps += 1;
        return;
    }
    let Some(view) = decode_effect_view(t5_header) else {
        catalog.capture_gaps += 1;
        return;
    };
    if view.looping_count != geometry.looping_count
        || view.one_shot_count != geometry.one_shot_count
        || view.emission_count != geometry.emission_count
    {
        catalog.capture_gaps += 1;
        return;
    }

    let mut elems = Vec::with_capacity(geometry.elem_def_count);
    if let Some(arr) = geometry.elem_defs {
        for i in 0..geometry.elem_def_count {
            let elem_ptr = arr.at(i * sz_t5::FX_ELEM_DEF);
            match capture_elem(s, elem_ptr, materials, material_name) {
                Some(elem) => elems.push(elem),
                None => {
                    catalog.capture_gaps += 1;
                    return;
                }
            }
        }
    } else if geometry.elem_def_count > 0 {
        catalog.capture_gaps += 1;
        return;
    }

    let namespace = crate::ID;
    catalog.insert_captured(OwnedFxEffectDef {
        namespace,
        name: name.to_owned(),
        view,
        header_raw: pack_fx_effect_header(&view),
        elems,
    });
}

fn decode_effect_view(bytes: &[u8]) -> Option<FxEffectDefView> {
    use ext_t5_fastfile::size as sz_t5;
    if bytes.len() < sz_t5::FX_EFFECT_DEF {
        return None;
    }
    Some(FxEffectDefView {
        flags: i32::from(bytes[sz_t5::FX_EFFECT_DEF_FLAGS_OFF]),
        msec_looping_life: i32::from_le_bytes(
            bytes[sz_t5::FX_EFFECT_DEF_MSEC_LOOPING_LIFE_OFF
                ..sz_t5::FX_EFFECT_DEF_MSEC_LOOPING_LIFE_OFF + 4]
                .try_into()
                .ok()?,
        ),
        looping_count: i32::from_le_bytes(
            bytes[sz_t5::FX_EFFECT_DEF_LOOPING_OFF..sz_t5::FX_EFFECT_DEF_LOOPING_OFF + 4]
                .try_into()
                .ok()?,
        ),
        one_shot_count: i32::from_le_bytes(
            bytes[sz_t5::FX_EFFECT_DEF_ONESHOT_OFF..sz_t5::FX_EFFECT_DEF_ONESHOT_OFF + 4]
                .try_into()
                .ok()?,
        ),
        emission_count: i32::from_le_bytes(
            bytes[sz_t5::FX_EFFECT_DEF_EMISSION_OFF..sz_t5::FX_EFFECT_DEF_EMISSION_OFF + 4]
                .try_into()
                .ok()?,
        ),
    })
}

fn capture_elem(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    p: ext_t5_fastfile::Ptr,
    materials: &MaterialCatalog,
    material_name: &impl Fn(ext_t5_fastfile::Ptr) -> Option<String>,
) -> Option<OwnedFxElemDef> {
    use ext_t5_fastfile::size as sz_t5;
    let raw = s.slice_at(p, 0, sz_t5::FX_ELEM_DEF).ok()?.to_vec();
    if raw.len() < sz_t5::FX_ELEM_DEF {
        return None;
    }
    let (view, atlas) = decode_elem_view(&raw)?;
    let vel_count = view.vel_interval_count as usize + 1;
    let vis_count = view.vis_state_interval_count as usize + 1;
    let vel_samples = copy_ptr_array(
        s,
        p,
        sz_t5::FX_ELEM_VEL_SAMPLES_OFF,
        vel_count,
        sz_t5::FX_ELEM_VEL_STATE_SAMPLE,
    );
    let vel_graph_local = parse_vel_graph_channel(&vel_samples, vel_count, false);
    let vel_graph_world = parse_vel_graph_channel(&vel_samples, vel_count, true);
    let vis_samples = copy_ptr_array(
        s,
        p,
        sz_t5::FX_ELEM_VIS_SAMPLES_OFF,
        vis_count,
        sz_t5::FX_ELEM_VIS_STATE_SAMPLE,
    );
    let visuals = capture_visuals(s, p, &view, materials, material_name);
    let (effect_on_impact, effect_on_impact_hint) =
        capture_named_child(read_name(s, p, sz_t5::FX_ELEM_EFFECT_ON_IMPACT_OFF));
    let (effect_on_death, effect_on_death_hint) =
        capture_named_child(read_name(s, p, sz_t5::FX_ELEM_EFFECT_ON_DEATH_OFF));
    let (effect_emitted, effect_emitted_hint) =
        capture_named_child(read_name(s, p, sz_t5::FX_ELEM_EFFECT_EMITTED_OFF));
    let has_extended = !matches!(
        s.ptr_at(p, sz_t5::FX_ELEM_TRAIL_DEF_OFF).ok()?,
        ext_t5_fastfile::ZonePtr::Null
    );
    Some(OwnedFxElemDef {
        view,
        raw: pack_fx_elem_raw(&view, atlas),
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
        trail_def: None,
        spark_fountain_def: None,
        phys_pieces: Vec::new(),
    })
}

fn decode_elem_view(bytes: &[u8]) -> Option<(FxElemDefView, [u8; 8])> {
    use ext_t5_fastfile::size as sz_t5;
    if bytes.len() < sz_t5::FX_ELEM_DEF {
        return None;
    }
    let f32_at = |off: usize| -> Option<f32> {
        Some(f32::from_le_bytes(bytes[off..off + 4].try_into().ok()?))
    };
    let i32_at = |off: usize| -> Option<i32> {
        Some(i32::from_le_bytes(bytes[off..off + 4].try_into().ok()?))
    };
    let range2 = |off: usize| -> Option<[f32; 2]> { Some([f32_at(off)?, f32_at(off + 4)?]) };
    let mut spawn_origin = [[0.0f32; 2]; 3];
    for i in 0..3 {
        spawn_origin[i] = range2(0x38 + i * 8)?;
    }
    let mut spawn_angles = [[0.0f32; 2]; 3];
    for i in 0..3 {
        spawn_angles[i] = range2(0x60 + i * 8)?;
    }
    let mut angular_velocity = [[0.0f32; 2]; 3];
    for i in 0..3 {
        angular_velocity[i] = range2(0x78 + i * 8)?;
    }
    let atlas: [u8; 8] = bytes[sz_t5::FX_ELEM_ATLAS_OFF..sz_t5::FX_ELEM_ATLAS_OFF + 8]
        .try_into()
        .ok()?;
    let coll = sz_t5::FX_ELEM_COLL_MINS_OFF;
    let emit = sz_t5::FX_ELEM_EMIT_DIST_OFF;
    Some((
        FxElemDefView {
            flags: i32_at(0)?,
            spawn_a: i32_at(0x04)?,
            spawn_b: i32_at(0x08)?,
            spawn_range_base: f32_at(0x0c)?,
            spawn_range_amplitude: f32_at(0x10)?,
            fade_in_range: [0.0, 0.0],
            fade_out_range: [0.0, 0.0],
            spawn_frustum_cull_radius: f32_at(0x24)?,
            spawn_delay_msec_base: i32_at(0x28)?,
            spawn_delay_msec_amplitude: i32_at(0x2c)?,
            life_span_msec_base: i32_at(0x30)?,
            life_span_msec_amplitude: i32_at(0x34)?,
            spawn_origin,
            spawn_offset_radius_base: f32_at(0x50)?,
            spawn_offset_radius_amplitude: f32_at(0x54)?,
            spawn_offset_height_base: f32_at(0x58)?,
            spawn_offset_height_amplitude: f32_at(0x5c)?,
            spawn_angles,
            angular_velocity,
            initial_rotation: range2(0x90)?,
            gravity_base: f32_at(0x9c)?,
            gravity_amplitude: f32_at(0xa0)?,
            reflection_factor: range2(0xa4)?,
            coll_mins: [f32_at(coll)?, f32_at(coll + 4)?, f32_at(coll + 8)?],
            coll_maxs: [f32_at(coll + 12)?, f32_at(coll + 16)?, f32_at(coll + 20)?],
            elem_type: sz_t5::leftover_iw4_elem_type(bytes[sz_t5::FX_ELEM_TYPE_OFF]),
            visual_count: bytes[sz_t5::FX_ELEM_TYPE_OFF + 1],
            vel_interval_count: bytes[sz_t5::FX_ELEM_TYPE_OFF + 2],
            vis_state_interval_count: bytes[sz_t5::FX_ELEM_TYPE_OFF + 3],
            lighting_frac: bytes[sz_t5::FX_ELEM_LIGHTING_FRAC_OFF],
            use_item_clip: 0,
            sort_order: bytes[sz_t5::FX_ELEM_SORT_ORDER_OFF],
            emit_dist: range2(emit)?,
            emit_dist_variance: range2(emit + 8)?,
        },
        atlas,
    ))
}

fn copy_ptr_array(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    parent: ext_t5_fastfile::Ptr,
    field: usize,
    count: usize,
    stride: usize,
) -> Vec<u8> {
    if count == 0 {
        return Vec::new();
    }
    let Ok(ext_t5_fastfile::ZonePtr::Offset(arr)) = s.ptr_at(parent, field) else {
        return Vec::new();
    };
    let arr = s.resolve_alias(arr);
    let nbytes = count.saturating_mul(stride);
    s.slice_at(arr, 0, nbytes)
        .map(|b| b.to_vec())
        .unwrap_or(Vec::new())
}

fn read_name(s: &ext_t5_fastfile::ZoneStream<'_>, p: ext_t5_fastfile::Ptr, field: usize) -> String {
    match s.ptr_at(p, field) {
        Ok(ext_t5_fastfile::ZonePtr::Offset(name)) => {
            s.cstr(s.resolve_alias(name)).ok().unwrap_or("").to_owned()
        }
        _ => String::new(),
    }
}

fn capture_visuals(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    p: ext_t5_fastfile::Ptr,
    view: &FxElemDefView,
    materials: &MaterialCatalog,
    material_name: &impl Fn(ext_t5_fastfile::Ptr) -> Option<String>,
) -> Vec<OwnedFxVisual> {
    use ext_t5_fastfile::size as sz_t5;
    let t = view.elem_type;
    let visual_count = view.visual_count as usize;
    let vis = p.at(sz_t5::FX_ELEM_VISUALS_OFF);

    if matches!(t, elem_type::OMNI_LIGHT | elem_type::SPOT_LIGHT) || visual_count == 0 {
        return vec![OwnedFxVisual::None];
    }
    if t == elem_type::DECAL {
        let ptr = s.ptr_at(vis, 0);
        let Ok(ext_t5_fastfile::ZonePtr::Offset(arr)) = ptr else {
            return (0..visual_count.max(1))
                .map(|_| OwnedFxVisual::UnresolvedMaterial)
                .collect();
        };
        let arr = s.resolve_alias(arr);
        return (0..visual_count.max(1))
            .map(|i| {
                let m = arr.at(i * sz_t5::FX_ELEM_MARK_VISUALS);
                mark_pair(
                    resolve_material_visual(s, materials, material_name, m.at(0)),
                    resolve_material_visual(s, materials, material_name, m.at(4)),
                )
            })
            .collect();
    }
    if fx_elem::is_sprite(t) {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ext_t5_fastfile::ZonePtr::Offset(arr)) = ptr else {
                let kind = match ptr {
                    Ok(ext_t5_fastfile::ZonePtr::Following) => "following",
                    Ok(ext_t5_fastfile::ZonePtr::Insert) => "insert",
                    Ok(ext_t5_fastfile::ZonePtr::Null) => "null",
                    Ok(ext_t5_fastfile::ZonePtr::Offset(_)) | Err(_) => "read_err",
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
                        material_name,
                        arr.at(i * sz_t5::FX_ELEM_VISUALS),
                    )
                })
                .collect();
        }
        return vec![resolve_material_visual(s, materials, material_name, vis)];
    }
    if t == elem_type::MODEL {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ext_t5_fastfile::ZonePtr::Offset(arr)) = ptr else {
                return (0..visual_count).map(|_| OwnedFxVisual::None).collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| model_visual(Some(read_name(s, arr.at(i * sz_t5::FX_ELEM_VISUALS), 0))))
                .collect();
        }
        return vec![model_visual(Some(read_name(s, vis, 0)))];
    }
    if t == elem_type::RUNNER {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ext_t5_fastfile::ZonePtr::Offset(arr)) = ptr else {
                return (0..visual_count).map(|_| OwnedFxVisual::None).collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| runner_visual(read_name(s, arr.at(i * sz_t5::FX_ELEM_VISUALS), 0)))
                .collect();
        }
        return vec![runner_visual(read_name(s, vis, 0))];
    }
    if t == elem_type::SOUND {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ext_t5_fastfile::ZonePtr::Offset(arr)) = ptr else {
                return (0..visual_count).map(|_| OwnedFxVisual::None).collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| sound_visual(read_name(s, arr.at(i * sz_t5::FX_ELEM_VISUALS), 0)))
                .collect();
        }
        return vec![sound_visual(read_name(s, vis, 0))];
    }
    vec![OwnedFxVisual::None]
}

fn resolve_material_visual(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    materials: &MaterialCatalog,
    material_name: &impl Fn(ext_t5_fastfile::Ptr) -> Option<String>,
    slot: ext_t5_fastfile::Ptr,
) -> OwnedFxVisual {
    let slot_id = identity(slot);
    let alias = match s.ptr_at(slot, 0) {
        Ok(ext_t5_fastfile::ZonePtr::Offset(target)) => Some(identity(s.resolve_alias(target))),
        _ => None,
    };
    let index = materials
        .material_index(slot_id)
        .or_else(|| alias.and_then(|target| materials.material_index(target)));
    let name = material_name(slot).or_else(|| {
        index
            .and_then(|i| materials.materials.get(i.get()).map(|m| m.name.to_string()))
            .filter(|n| !n.is_empty())
    });
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

fn identity(p: ext_t5_fastfile::Ptr) -> AssetPointerIdentity {
    AssetPointerIdentity {
        block: p.block,
        offset: p.offset,
    }
}
