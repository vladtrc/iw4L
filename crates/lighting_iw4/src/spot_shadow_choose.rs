use crate::cull::{
    LightRegionHull, LightRegionHulls, cull_point_from_cone_expanded, light_region_culls_point,
};
use crate::scene_light::GFX_LIGHT_TYPE_SPOT;
use crate::spot_shadow::{
    SpotShadowSlotPlan, SpotShadowViewParms, spot_shadow_emit_walks_casters,
    spot_shadow_lookup_matrix, spot_shadow_near_bias, spot_shadow_slot_plan, spot_shadow_take_slot,
    spot_shadow_view_parms,
};

pub const SPOT_SHADOW_HISTORY_STRIDE: usize = 0x58;

pub const SPOT_SHADOW_HISTORY_ENTRY_STRIDE: usize = 0x0c;

pub const SPOT_SHADOW_VIEW_LIGHT_INDEX_BIAS: u32 = 0x3cec;

pub const SPOT_SHADOW_MSEC: f32 = 0.001;

pub const SPOT_SHADOW_TIME_WRAP: f32 = 4294967296.0;

pub const SPOT_SHADOW_FADE_DROP: f32 = 0.01;

pub const SPOT_SHADOW_SCORE_ONE: f32 = 1.0;

pub const SPOT_SHADOW_SM_LIGHT_CAP: usize = 4;

pub const SPOT_SHADOW_CANDIDATE_STACK: usize = 5;

pub const GFX_WORLD_LIGHT_REGION_OFF: usize = 0x1c4;

pub const SPOT_SHADOW_DIST_CULL_SAMPLE_SCALE: f32 = 1_048_576.0;

pub const SPOT_SHADOW_SCORE_LUMA: [f32; 3] = [0.2989, 0.587, 0.114];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotShadowUsedForce {
    None,

    IncludeBelowSun,

    ExcludeBelowSun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotShadowCandidateSkip {
    IndexZero,
    SunPrimary,
    SpotDisabled,
    CannotUseShadowMap,
    BelowMinScore,

    DistCullFar,

    DistCullCone,

    DistCullRegion,

    DistCullRegionUnread,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowableLight {
    pub can_use_shadow_map: bool,
    pub color: [f32; 3],
    pub dir: [f32; 3],
    pub origin: [f32; 3],
    pub radius: f32,

    pub light_type: u8,

    pub cos_half_fov_expanded: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowHistoryEntry {
    pub light_index: u8,
    pub fading_out: bool,
    pub score: f32,
    pub fade: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpotShadowHistory {
    pub was_used: [u32; 8],
    pub entries: [SpotShadowHistoryEntry; SPOT_SHADOW_SM_LIGHT_CAP],
    pub entry_count: u32,
    pub last_update_time: u32,
}

impl Default for SpotShadowHistory {
    fn default() -> Self {
        Self {
            was_used: [0; 8],
            entries: [SpotShadowHistoryEntry {
                light_index: 0,
                fading_out: false,
                score: 0.0,
                fade: 0.0,
            }; SPOT_SHADOW_SM_LIGHT_CAP],
            entry_count: 0,
            last_update_time: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowChooseDvars {
    pub spot_enable: bool,
    pub spot_limit: u32,
    pub max_lights: u32,
    pub min_score: f32,
    pub fade_time: f32,
    pub eye_project_dist: f32,
    pub spot_project_frac: f32,
    pub quality_spot_shadow: bool,
    pub spot_dist_cull: bool,

    pub sun_sample_size_near: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowEmit {
    pub light_index: u8,
    pub fade: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpotShadowChoose {
    pub fast: bool,
    pub emit: [Option<SpotShadowEmit>; SPOT_SHADOW_SM_LIGHT_CAP],
}

pub fn used_bit(words: &[u32; 8], index: u32) -> bool {
    let w = (index >> 5) as usize;
    w < 8 && (words[w] & (1 << (index & 31))) != 0
}

pub fn set_used_bit(words: &mut [u32; 8], index: u32) {
    let w = (index >> 5) as usize;
    if w < 8 {
        words[w] |= 1 << (index & 31);
    }
}

pub fn clear_used_bit(words: &mut [u32; 8], index: u32) {
    let w = (index >> 5) as usize;
    if w < 8 {
        words[w] &= !(1 << (index & 31));
    }
}

#[must_use]
pub fn generate_primary_light_copy_bytes(primary_count: u32) -> usize {
    (primary_count as usize) << 6
}

pub const SM_ENABLE_DEFAULT: bool = true;

pub const SM_SUN_ENABLE_DEFAULT: bool = true;

#[must_use]
pub fn generate_runs_choose(sm_enable: Option<bool>, view_e24: Option<bool>) -> bool {
    sm_enable.unwrap_or(SM_ENABLE_DEFAULT) && view_e24 != Some(false)
}

#[must_use]
pub fn generate_clears_used_bits_through_sun(sm_sun_enable: bool, sun_primary: u32) -> bool {
    !sm_sun_enable && sun_primary != 0
}

pub fn clear_used_bits_1_through_sun(used: &mut [u32; 8], sun_primary: u32) {
    let mut i = 1u32;
    while i <= sun_primary {
        clear_used_bit(used, i);
        i += 1;
    }
}

pub fn spot_shadow_fade_delta(time_delta: i32, fade_time: f32) -> Option<f32> {
    if fade_time == 0.0 {
        return None;
    }
    let mut t = time_delta as f32;
    if time_delta < 0 {
        t += SPOT_SHADOW_TIME_WRAP;
    }
    Some(t * (SPOT_SHADOW_MSEC / fade_time))
}

pub fn spot_shadow_score(
    view_origin: [f32; 3],
    view_forward: [f32; 3],
    light: SpotShadowableLight,
    eye_project_dist: f32,
    spot_project_frac: f32,
    luma: [f32; 3],
) -> f32 {
    let eye = [
        view_origin[0] + eye_project_dist * view_forward[0],
        view_origin[1] + eye_project_dist * view_forward[1],
        view_origin[2] + eye_project_dist * view_forward[2],
    ];
    let scale = -light.radius * spot_project_frac;
    let dx = (light.origin[0] - eye[0]) + scale * light.dir[0];
    let dy = (light.origin[1] - eye[1]) + scale * light.dir[1];
    let dz = (light.origin[2] - eye[2]) + scale * light.dir[2];
    let dist = libm::sqrtf(dx * dx + dy * dy + dz * dz);
    let intensity = light.color[0] * luma[0] + light.color[1] * luma[1] + light.color[2] * luma[2];
    (intensity * light.radius) / (dist + SPOT_SHADOW_SCORE_ONE)
}

pub fn spot_shadow_sun_in_front(dir: [f32; 3], camera_forward: [f32; 3]) -> bool {
    dir[0] * camera_forward[0] + dir[1] * camera_forward[1] + dir[2] * camera_forward[2] > 0.0
}

fn force_mask(sun_primary_index: u32) -> u32 {
    let shift = sun_primary_index.wrapping_add(1) & 31;
    if shift == 0 { 0 } else { (1u32 << shift) - 1 }
}

fn apply_force(word: u32, force: SpotShadowUsedForce, sun_primary_index: u32) -> u32 {
    let mask = force_mask(sun_primary_index);
    if mask == 0 {
        return word;
    }
    match force {
        SpotShadowUsedForce::None => word,
        SpotShadowUsedForce::IncludeBelowSun => word | mask,
        SpotShadowUsedForce::ExcludeBelowSun => word & !mask,
    }
}

fn consume_bsr(bits: &mut u32) -> Option<u32> {
    if *bits == 0 {
        return None;
    }
    let bit = 31 - bits.leading_zeros();
    *bits &= !(1u32 << bit);
    Some(bit)
}

fn cap_pair(max_lights: u32, spot_limit: u32) -> usize {
    max_lights
        .min(spot_limit)
        .min(SPOT_SHADOW_SM_LIGHT_CAP as u32) as usize
}

pub fn spot_shadow_fade_out_history(
    history: &mut SpotShadowHistory,
    used: &[u32; 8],
    fade_delta: f32,
) {
    let mut i = 0usize;
    while i < history.entry_count as usize {
        let idx = history.entries[i].light_index as u32;
        if !used_bit(used, idx) {
            history.entry_count -= 1;
            let last = history.entry_count as usize;
            history.entries[i] = history.entries[last];
            continue;
        }
        if !history.entries[i].fading_out {
            history.entries[i].fading_out = true;
            i += 1;
        } else {
            let next = history.entries[i].fade - fade_delta;
            if next < SPOT_SHADOW_FADE_DROP {
                history.entry_count -= 1;
                let last = history.entry_count as usize;
                history.entries[i] = history.entries[last];
            } else {
                history.entries[i].fade = next;
                i += 1;
            }
        }
    }
}

fn bubble_score(history: &mut SpotShadowHistory, mut slot: usize) {
    while slot != 0 {
        let prev = slot - 1;
        if history.entries[slot].score <= history.entries[prev].score {
            return;
        }
        history.entries.swap(slot, prev);
        slot = prev;
    }
}

pub fn spot_shadow_history_add(
    history: &mut SpotShadowHistory,
    light_index: u8,
    score: f32,
    fade_delta: f32,
    max_lights: u32,
    spot_limit: u32,
) {
    let cap = cap_pair(max_lights, spot_limit);
    let n = history.entry_count as usize;
    for i in 0..n {
        if history.entries[i].light_index == light_index {
            history.entries[i].score = score;
            history.entries[i].fading_out = false;
            let fade = history.entries[i].fade + fade_delta;
            history.entries[i].fade = if fade < 1.0 { fade } else { 1.0 };
            bubble_score(history, i);
            return;
        }
    }
    if n >= cap {
        return;
    }
    let was = used_bit(&history.was_used, light_index as u32);
    history.entries[n] = SpotShadowHistoryEntry {
        light_index,
        fading_out: false,
        score,
        fade: if was { fade_delta } else { 1.0 },
    };
    history.entry_count = (n as u32) + 1;
    bubble_score(history, n);
}

#[must_use]
pub fn spot_shadow_dist_cull_extra(sun_sample_size_near: f32) -> f32 {
    sun_sample_size_near * SPOT_SHADOW_DIST_CULL_SAMPLE_SCALE
}

pub fn spot_shadow_dist_cull(
    view_origin: [f32; 3],
    light: SpotShadowableLight,
    extra: f32,
    region: Option<&[LightRegionHull<'_>]>,
) -> Result<(), SpotShadowCandidateSkip> {
    let dx = view_origin[0] - light.origin[0];
    let dy = view_origin[1] - light.origin[1];
    let dz = view_origin[2] - light.origin[2];
    let d2 = dx * dx + dy * dy + dz * dz;
    let r = extra + light.radius;
    if d2 >= r * r {
        return Err(SpotShadowCandidateSkip::DistCullFar);
    }
    if light.light_type == GFX_LIGHT_TYPE_SPOT
        && light.cos_half_fov_expanded > 0.0
        && cull_point_from_cone_expanded(
            light.origin,
            light.dir,
            light.cos_half_fov_expanded,
            view_origin,
            extra,
        )
    {
        return Err(SpotShadowCandidateSkip::DistCullCone);
    }
    let Some(hulls) = region else {
        return Err(SpotShadowCandidateSkip::DistCullRegionUnread);
    };
    if light_region_culls_point(hulls, light.origin, view_origin, extra) {
        return Err(SpotShadowCandidateSkip::DistCullRegion);
    }
    Ok(())
}

pub fn spot_shadow_add_candidate(
    light_index: u32,
    sun_primary_index: u32,
    light: SpotShadowableLight,
    view_origin: [f32; 3],
    view_forward: [f32; 3],
    dvars: SpotShadowChooseDvars,
    luma: [f32; 3],
    primary_light_count: u32,
    region: Option<&[LightRegionHull<'_>]>,
    candidates: &mut [(u32, f32); SPOT_SHADOW_CANDIDATE_STACK],
    candidate_n: usize,
    has_shadow_map: &mut [u32; 8],
) -> Result<usize, SpotShadowCandidateSkip> {
    if light_index == 0 {
        return Err(SpotShadowCandidateSkip::IndexZero);
    }
    if light_index <= sun_primary_index {
        if spot_shadow_sun_in_front(light.dir, view_forward) {
            set_used_bit(has_shadow_map, light_index);
        }
        return Err(SpotShadowCandidateSkip::SunPrimary);
    }
    if !light.can_use_shadow_map {
        return Err(SpotShadowCandidateSkip::CannotUseShadowMap);
    }
    if !dvars.spot_enable {
        return Err(SpotShadowCandidateSkip::SpotDisabled);
    }
    let score = spot_shadow_score(
        view_origin,
        view_forward,
        light,
        dvars.eye_project_dist,
        dvars.spot_project_frac,
        luma,
    );
    if score < dvars.min_score {
        return Err(SpotShadowCandidateSkip::BelowMinScore);
    }
    if dvars.spot_dist_cull && light_index < primary_light_count {
        spot_shadow_dist_cull(
            view_origin,
            light,
            spot_shadow_dist_cull_extra(dvars.sun_sample_size_near),
            region,
        )?;
    }
    let mut insert = candidate_n;
    while insert != 0 && score > candidates[insert - 1].1 {
        if insert < SPOT_SHADOW_CANDIDATE_STACK {
            candidates[insert] = candidates[insert - 1];
        }
        insert -= 1;
    }
    if insert < SPOT_SHADOW_CANDIDATE_STACK {
        candidates[insert] = (light_index, score);
    }
    let mut n = candidate_n + 1;
    let max_l = dvars.max_lights.max(1);
    if max_l <= n as u32 {
        n = max_l as usize;
    }
    if dvars.spot_limit <= n as u32 {
        n = dvars.spot_limit as usize;
    }
    Ok(n.min(SPOT_SHADOW_CANDIDATE_STACK))
}

fn any_sun_has_map(has_shadow_map: &[u32; 8], sun_primary_index: u32) -> bool {
    (1..=sun_primary_index).any(|i| used_bit(has_shadow_map, i))
}

pub fn spot_shadow_emit_from_history(
    history: &SpotShadowHistory,
    lights: &[SpotShadowableLight],
    shadowable_light_count: u32,
    has_shadow_map: &mut [u32; 8],
) -> [Option<SpotShadowEmit>; SPOT_SHADOW_SM_LIGHT_CAP] {
    let mut out = [None; SPOT_SHADOW_SM_LIGHT_CAP];
    let mut n = 0usize;
    let count = history.entry_count.min(SPOT_SHADOW_SM_LIGHT_CAP as u32) as usize;
    for e in history.entries.iter().take(count) {
        let idx = e.light_index as u32;
        if idx >= shadowable_light_count {
            continue;
        }
        let Some(light) = lights.get(idx as usize) else {
            continue;
        };
        if !light.can_use_shadow_map {
            continue;
        }
        set_used_bit(has_shadow_map, idx);
        if n < SPOT_SHADOW_SM_LIGHT_CAP {
            out[n] = Some(SpotShadowEmit {
                light_index: e.light_index,
                fade: e.fade,
            });
            n += 1;
        }
    }
    out
}

pub fn spot_shadow_choose(
    history: &mut SpotShadowHistory,
    used: [u32; 8],
    lights: &[SpotShadowableLight],
    view_origin: [f32; 3],
    view_forward: [f32; 3],
    scene_time: u32,
    sun_primary_index: u32,
    shadowable_light_count: u32,
    primary_light_count: u32,
    force: SpotShadowUsedForce,
    mut fast: bool,
    dvars: SpotShadowChooseDvars,
    luma: [f32; 3],
    dist_cull_regions: Option<&[LightRegionHulls<'_>]>,
) -> Option<SpotShadowChoose> {
    let mut has_shadow_map = [0u32; 8];
    let time_delta = scene_time as i32 - history.last_update_time as i32;
    if time_delta == 0 {
        if sun_primary_index != 0 {
            for i in 1..=sun_primary_index {
                let allow = match force {
                    SpotShadowUsedForce::None => used_bit(&used, i),
                    SpotShadowUsedForce::IncludeBelowSun => true,
                    SpotShadowUsedForce::ExcludeBelowSun => false,
                };
                let Some(light) = lights.get(i as usize) else {
                    continue;
                };
                if allow && spot_shadow_sun_in_front(light.dir, view_forward) {
                    set_used_bit(&mut has_shadow_map, i);
                }
            }
        }
    } else {
        let fade_delta = spot_shadow_fade_delta(time_delta, dvars.fade_time)?;
        history.last_update_time = scene_time;
        spot_shadow_fade_out_history(history, &used, fade_delta);
        let words = (shadowable_light_count.saturating_add(31)) >> 5;
        let mut candidates = [(0u32, 0.0f32); SPOT_SHADOW_CANDIDATE_STACK];
        let mut candidate_n = 0usize;
        for w in 0..words.min(8) {
            let mut bits = apply_force(used[w as usize], force, sun_primary_index);
            while let Some(bit) = consume_bsr(&mut bits) {
                let index = bit + 32 * w;
                let Some(light) = lights.get(index as usize).copied() else {
                    continue;
                };
                let region = dist_cull_regions.and_then(|all| all.get(index as usize).copied());
                match spot_shadow_add_candidate(
                    index,
                    sun_primary_index,
                    light,
                    view_origin,
                    view_forward,
                    dvars,
                    luma,
                    primary_light_count,
                    region,
                    &mut candidates,
                    candidate_n,
                    &mut has_shadow_map,
                ) {
                    Ok(n) => candidate_n = n,
                    Err(SpotShadowCandidateSkip::DistCullRegionUnread) => return None,
                    Err(_) => {}
                }
            }
        }
        for c in candidates.iter().take(candidate_n) {
            if c.0 == 0 || c.0 > u8::MAX as u32 {
                continue;
            }
            spot_shadow_history_add(
                history,
                c.0 as u8,
                c.1,
                fade_delta,
                dvars.max_lights,
                dvars.spot_limit,
            );
        }
        history.was_used = used;
    }
    if dvars.quality_spot_shadow && !fast && !any_sun_has_map(&has_shadow_map, sun_primary_index) {
        fast = true;
    }
    let emit =
        spot_shadow_emit_from_history(history, lights, shadowable_light_count, &mut has_shadow_map);
    Some(SpotShadowChoose { fast, emit })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowEmittedSlot {
    pub slot_index: u32,
    pub light_index: u8,
    pub fade: f32,
    pub plan: SpotShadowSlotPlan,
    pub walks_casters: bool,
    pub view_parms: Option<SpotShadowViewParms>,
    pub lookup: Option<[f32; 16]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpotShadowFrontend {
    pub fast: bool,
    pub shadowable_count: u32,
    pub slots: [Option<SpotShadowEmittedSlot>; SPOT_SHADOW_SM_LIGHT_CAP],
}

pub fn spot_shadow_emit_frontend(
    choose: &SpotShadowChoose,
    lights: &[SpotShadowableLight],
    world_plus_0x20: u32,
    near_bias_dvar: f32,
    rand_zero: bool,
) -> SpotShadowFrontend {
    let mut shadowable_count = 0u32;
    let mut slots = [None; SPOT_SHADOW_SM_LIGHT_CAP];
    for (i, emit) in choose.emit.iter().enumerate() {
        let Some(emit) = *emit else {
            continue;
        };
        let Some(slot_index) = spot_shadow_take_slot(&mut shadowable_count) else {
            break;
        };
        let plan = spot_shadow_slot_plan(choose.fast, slot_index, rand_zero);
        let light_index = emit.light_index as u32;
        let walks_casters = spot_shadow_emit_walks_casters(light_index, world_plus_0x20);
        let (view_parms, lookup) = lights
            .get(light_index as usize)
            .map_or((None, None), |light| {
                let near = spot_shadow_near_bias(light_index, world_plus_0x20, near_bias_dvar);
                let parms = spot_shadow_view_parms(
                    light.origin,
                    light.dir,
                    light.cos_half_fov_expanded,
                    light.radius,
                    near,
                );
                let lookup = parms.and_then(|p| spot_shadow_lookup_matrix(&p, plan));
                (parms, lookup)
            });
        slots[i] = Some(SpotShadowEmittedSlot {
            slot_index,
            light_index: emit.light_index,
            fade: emit.fade,
            plan,
            walks_casters,
            view_parms,
            lookup,
        });
    }
    SpotShadowFrontend {
        fast: choose.fast,
        shadowable_count,
        slots,
    }
}
