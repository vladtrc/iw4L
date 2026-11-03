use asset_game::*;
use asset_iw4::size::fx_elem;
use fx_iw4::*;
#[derive(Clone, Debug, Default)]
pub struct T6FxCapture {
    pub name: String,
    pub header: Vec<u8>,
    pub elems: Vec<T6FxElemCapture>,
}

#[derive(Clone, Debug, Default)]
pub struct T6FxElemCapture {
    pub raw: Vec<u8>,
    pub vel_samples: Vec<u8>,
    pub vis_samples: Vec<u8>,
    pub visuals: Vec<String>,
    pub effect_on_impact: String,
    pub effect_on_death: String,
    pub effect_emitted: String,
}

const T6_FX_EFFECT_DEF: usize = 76;
const T6_FX_EFFECT_DEF_FLAGS_OFF: usize = 4;
const T6_FX_EFFECT_DEF_LOOPING_OFF: usize = 8;
const T6_FX_EFFECT_DEF_ONESHOT_OFF: usize = 10;
const T6_FX_EFFECT_DEF_EMISSION_OFF: usize = 12;
const T6_FX_EFFECT_DEF_MSEC_LOOPING_LIFE_OFF: usize = 20;
const T6_FX_ELEM_LINE: u8 = 4;
const T6_FX_VIS_SIZE_OFF: usize = 12;

pub fn capture(catalog: &mut FxCatalog, fx: &T6FxCapture) {
    let namespace = crate::ID;
    let h = &fx.header;
    if fx.name.is_empty() || h.len() < T6_FX_EFFECT_DEF {
        catalog.capture_gaps += 1;
        return;
    }
    let i16_at = |at: usize| i32::from(i16::from_le_bytes([h[at], h[at + 1]]));
    let view = FxEffectDefView {
        flags: i16_at(T6_FX_EFFECT_DEF_FLAGS_OFF) & 0xffff,
        msec_looping_life: i32::from_le_bytes(
            h[T6_FX_EFFECT_DEF_MSEC_LOOPING_LIFE_OFF..T6_FX_EFFECT_DEF_MSEC_LOOPING_LIFE_OFF + 4]
                .try_into()
                .expect("header extent"),
        ),
        looping_count: i16_at(T6_FX_EFFECT_DEF_LOOPING_OFF),
        one_shot_count: i16_at(T6_FX_EFFECT_DEF_ONESHOT_OFF),
        emission_count: i16_at(T6_FX_EFFECT_DEF_EMISSION_OFF),
    };
    if view.total_elem_defs() as usize != fx.elems.len() {
        catalog.capture_gaps += 1;
        return;
    }
    let mut elems = Vec::with_capacity(fx.elems.len());
    for elem in &fx.elems {
        match capture_elem_t6(elem, namespace) {
            Some(elem) => elems.push(elem),
            None => {
                catalog.capture_gaps += 1;
                return;
            }
        }
    }
    catalog.insert_captured(OwnedFxEffectDef {
        namespace,
        name: fx.name.clone(),
        view,
        header_raw: pack_fx_effect_header(&view),
        elems,
    });
}

fn capture_elem_t6(
    elem: &T6FxElemCapture,
    namespace: asset_core::FamilyId,
) -> Option<OwnedFxElemDef> {
    let (mut view, atlas) = decode_elem_view(&elem.raw)?;
    let vel_count = view.vel_interval_count as usize + 1;
    let mut vel_samples = elem.vel_samples.clone();
    if elem.raw[FX_ELEM_TYPE_OFF] == T6_FX_ELEM_LINE {
        vel_samples = vec![0; vel_count * FX_ELEM_VEL_STATE_SAMPLE];
        for sample in vel_samples.chunks_mut(FX_ELEM_VEL_STATE_SAMPLE) {
            sample[0..4].copy_from_slice(&(-1.0e-3f32).to_le_bytes());
        }
        view.flags = (view.flags & !fx_iw4::FX_ELEM_VEL_WORLD) | fx_iw4::FX_ELEM_VEL_LOCAL;
    }
    let vel_graph_local = parse_vel_graph_channel(&vel_samples, vel_count, false);
    let vel_graph_world = parse_vel_graph_channel(&vel_samples, vel_count, true);
    let t = view.elem_type;
    let authored = AuthoredRef {
        slot: true,
        alias: false,
    };
    let visuals =
        if matches!(t, elem_type::OMNI_LIGHT | elem_type::SPOT_LIGHT) || elem.visuals.is_empty() {
            vec![OwnedFxVisual::None]
        } else if fx_elem::is_sprite(t) {
            elem.visuals
                .iter()
                .map(|name| OwnedFxVisual::Material {
                    material: authored.unresolved(),
                    hint: Some(name.clone()).filter(|name| !name.is_empty()),
                    material_namespace: namespace,
                    authored,
                })
                .collect()
        } else if t == elem_type::DECAL {
            elem.visuals
                .chunks_exact(2)
                .map(|pair| {
                    let material = |name: &String| OwnedFxVisual::Material {
                        material: authored.unresolved(),
                        hint: Some(name.clone()).filter(|name| !name.is_empty()),
                        material_namespace: namespace,
                        authored,
                    };
                    mark_pair(material(&pair[0]), material(&pair[1]))
                })
                .collect()
        } else if t == elem_type::MODEL {
            elem.visuals
                .iter()
                .map(|name| model_visual(Some(name.clone())))
                .collect()
        } else if t == elem_type::RUNNER {
            elem.visuals
                .iter()
                .map(|name| runner_visual(name.clone()))
                .collect()
        } else if t == elem_type::SOUND {
            elem.visuals
                .iter()
                .map(|name| sound_visual(name.clone()))
                .collect()
        } else {
            vec![OwnedFxVisual::None]
        };
    let (effect_on_impact, effect_on_impact_hint) =
        capture_named_child(elem.effect_on_impact.clone());
    let (effect_on_death, effect_on_death_hint) = capture_named_child(elem.effect_on_death.clone());
    let (effect_emitted, effect_emitted_hint) = capture_named_child(elem.effect_emitted.clone());
    let line = elem.raw[FX_ELEM_TYPE_OFF] == T6_FX_ELEM_LINE;
    let mut vis_samples = elem.vis_samples.clone();
    for sample in vis_samples.chunks_mut(FX_ELEM_VIS_STATE_SAMPLE) {
        for state in [0, FX_ELEM_VIS_STATE_SAMPLE / 2] {
            if let Some(rgba) = sample.get_mut(state..state + 4) {
                rgba.swap(0, 2);
            }
            for size in [state + T6_FX_VIS_SIZE_OFF, state + T6_FX_VIS_SIZE_OFF + 4] {
                if line && let Some(bytes) = sample.get_mut(size..size + 4) {
                    let half = f32::from_le_bytes(bytes.try_into().expect("size extent")) * 0.5;
                    bytes.copy_from_slice(&half.to_le_bytes());
                }
            }
        }
    }
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
        has_extended: false,
        trail_def: None,
        spark_fountain_def: None,
        phys_pieces: Vec::new(),
    })
}

fn decode_elem_view(bytes: &[u8]) -> Option<(FxElemDefView, [u8; 8])> {
    if bytes.len() < FX_ELEM_DEF {
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
    let atlas: [u8; 8] = bytes[FX_ELEM_ATLAS_OFF..FX_ELEM_ATLAS_OFF + 8]
        .try_into()
        .ok()?;
    let coll = FX_ELEM_COLL_MINS_OFF;
    let emit = FX_ELEM_EMIT_DIST_OFF;
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
            elem_type: normalize_elem_type(bytes[FX_ELEM_TYPE_OFF]),
            visual_count: bytes[FX_ELEM_TYPE_OFF + 1],
            vel_interval_count: bytes[FX_ELEM_TYPE_OFF + 2],
            vis_state_interval_count: bytes[FX_ELEM_TYPE_OFF + 3],
            lighting_frac: bytes[FX_ELEM_LIGHTING_FRAC_OFF],
            use_item_clip: 0,
            sort_order: bytes[FX_ELEM_SORT_ORDER_OFF],
            emit_dist: range2(emit)?,
            emit_dist_variance: range2(emit + 8)?,
        },
        atlas,
    ))
}

const FX_ELEM_ATLAS_OFF: usize = 0xac;
const FX_ELEM_COLL_MINS_OFF: usize = 0xc8;
const FX_ELEM_DEF: usize = 292;
const FX_ELEM_EMIT_DIST_OFF: usize = 0xec;
const FX_ELEM_LIGHTING_FRAC_OFF: usize = 261;
const FX_ELEM_SORT_ORDER_OFF: usize = 260;
const FX_ELEM_TYPE_OFF: usize = 184;
const FX_ELEM_VEL_STATE_SAMPLE: usize = 96;
const FX_ELEM_VIS_STATE_SAMPLE: usize = 48;
fn normalize_elem_type(native: u8) -> u8 {
    match native {
        0 => 0,
        1 => 1,
        2 => 1,
        3 => 2,
        4 => 2,
        5 => 3,
        6 => 4,
        other => other,
    }
}
