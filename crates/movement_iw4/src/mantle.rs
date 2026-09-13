use math_iw4::{angle_normalize_360, angle_subtract};
use playerstate_iw4::PlayerState;
use trace_iw4::Trace;

use crate::add_predictable_event;

pub const CONTENTS_MANTLE: u32 = 0x0100_0000;

pub const SURF_MANTLE_ON_OR_OVER: u32 = 0x0600_0000;

pub const SURF_MANTLE_OVER: u32 = 0x0400_0000;

pub const PMF_MANTLE: u32 = 0x4;

const EV_MANTLE: i32 = 0xae;

const EF_MANTLE: u32 = 0x1_0000;

pub const MANTLE_LEDGE_HEIGHTS: [f32; 3] = [60.0, 40.0, 20.0];

pub const MANTLE_FORWARD_DIST: f32 = 16.0;

pub const MANTLE_LEDGE_FLOOR_Z: f32 = 18.0;

pub const MANTLE_CLEARANCE_MAXS_Z: f32 = 50.0;

pub const MANTLE_HALF_WIDTH: f32 = 15.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MantleLedgeProbe {
    Front { contentmask: u32 },
    HeightLanding { height: f32 },
}

pub const MANTLE_PLAYER_RADIUS: f32 = 15.0;

pub const MANTLE_CHECK_RANGE_DEFAULT: f32 = 20.0;

pub const MANTLE_CHECK_RADIUS_DEFAULT: f32 = 0.1;

pub const MANTLE_FRONT_MAXS_Z: f32 = 70.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MantleFindLedgeContext {
    pub mantle_enable: bool,
    pub ledge_heights: [f32; 3],

    pub check_angle_deg: f32,

    pub check_range: f32,

    pub check_radius: f32,
}

impl Default for MantleFindLedgeContext {
    fn default() -> Self {
        Self {
            mantle_enable: true,
            ledge_heights: MANTLE_LEDGE_HEIGHTS,
            check_angle_deg: 60.0,
            check_range: MANTLE_CHECK_RANGE_DEFAULT,
            check_radius: MANTLE_CHECK_RADIUS_DEFAULT,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MantleResults {
    pub dir: [f32; 3],
    pub start_pos: [f32; 3],
    pub ledge_pos: [f32; 3],

    pub end_pos: [f32; 3],
    pub flags: u32,
}

pub trait MantleCapsuleTrace {
    fn mantle_trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        contentmask: u32,
    ) -> Trace;
}

pub trait MantleLedgeBackend {
    fn front_probe(&mut self, contentmask: u32) -> Option<[f32; 3]>;
    fn height_landing(&mut self, height: f32, results: &mut MantleResults) -> bool;
}

const MANTLE_FLAG_EVENT7: u32 = 1 << 1;

const MANTLE_FLAG_EVENT6: u32 = 1 << 2;

pub const MANTLE_OVER_FORWARD: f32 = 31.0;

const RAD2DEG: f32 = 57.295_780;

const VECTOR_ANGLE_DEG2RAD: f32 = 0.01745329238474369_f32;

const MANTLE_NORMAL_MIN_LEN: f32 = 0.0001;

#[must_use]
pub fn mantle_front_probe_along(range: f32, radius: f32) -> (f32, f32) {
    let inner = MANTLE_PLAYER_RADIUS - radius;
    (-inner, range + inner)
}

#[must_use]
pub fn mantle_front_probe_cast(
    origin: [f32; 3],
    facing_xy: [f32; 2],
    range: f32,
    radius: f32,
) -> Option<MantleFrontProbeCast> {
    let face_len = libm::sqrtf(facing_xy[0] * facing_xy[0] + facing_xy[1] * facing_xy[1]);
    if face_len <= 0.0 {
        return None;
    }
    let fx = facing_xy[0] / face_len;
    let fy = facing_xy[1] / face_len;
    let (along0, along1) = mantle_front_probe_along(range, radius);
    Some(MantleFrontProbeCast {
        start: [origin[0] + fx * along0, origin[1] + fy * along0, origin[2]],
        end: [origin[0] + fx * along1, origin[1] + fy * along1, origin[2]],
        mins: [-radius, -radius, 0.0],
        maxs: [radius, radius, MANTLE_FRONT_MAXS_Z],
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MantleFrontProbeCast {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

#[must_use]
pub fn mantle_front_probe_accept(
    trace: &Trace,
    facing_xy: [f32; 2],
    check_angle_deg: f32,
) -> Option<[f32; 3]> {
    if trace.startsolid != 0 || trace.allsolid != 0 {
        return None;
    }
    if trace.fraction == 1.0 {
        return None;
    }
    if (trace.surface_flags & SURF_MANTLE_ON_OR_OVER) == 0 {
        return None;
    }
    let mut dir = [-trace.normal[0], -trace.normal[1], 0.0];
    let len = libm::sqrtf(dir[0] * dir[0] + dir[1] * dir[1]);
    if len < MANTLE_NORMAL_MIN_LEN {
        return None;
    }
    dir[0] /= len;
    dir[1] /= len;
    let face_len = libm::sqrtf(facing_xy[0] * facing_xy[0] + facing_xy[1] * facing_xy[1]);
    if face_len <= 0.0 {
        return None;
    }
    let fx = facing_xy[0] / face_len;
    let fy = facing_xy[1] / face_len;
    let dot = (fx * dir[0] + fy * dir[1]).clamp(-1.0, 1.0);
    let angle_deg = libm::acosf(dot) * RAD2DEG;
    if check_angle_deg >= angle_deg {
        Some(dir)
    } else {
        None
    }
}

#[must_use]
pub fn mantle_height_landing_probe(
    results: &mut MantleResults,
    height: f32,
    tracemask: u32,
    tracer: &mut impl MantleCapsuleTrace,
) -> bool {
    let mins = [-MANTLE_HALF_WIDTH, -MANTLE_HALF_WIDTH, 0.0];
    let maxs = [
        MANTLE_HALF_WIDTH,
        MANTLE_HALF_WIDTH,
        MANTLE_HALF_WIDTH * 2.0,
    ];
    let start = [
        results.start_pos[0],
        results.start_pos[1],
        results.start_pos[2] + height,
    ];
    let end = [
        start[0] + results.dir[0] * MANTLE_FORWARD_DIST,
        start[1] + results.dir[1] * MANTLE_FORWARD_DIST,
        start[2] + results.dir[2] * MANTLE_FORWARD_DIST,
    ];
    let forward = tracer.mantle_trace(start, end, mins, maxs, tracemask);
    if forward.startsolid != 0 || forward.fraction < 1.0 {
        return false;
    }

    let drop_start = end;
    let drop_end = [end[0], end[1], results.start_pos[2] + MANTLE_LEDGE_FLOOR_Z];
    let down = tracer.mantle_trace(drop_start, drop_end, mins, maxs, tracemask);
    if down.startsolid != 0 || down.fraction == 1.0 || down.walkable == 0 {
        return false;
    }
    let ledge_z = drop_start[2] + (drop_end[2] - drop_start[2]) * down.fraction;
    results.ledge_pos = [drop_end[0], drop_end[1], ledge_z];
    if results.ledge_pos[2] - results.start_pos[2] <= 0.0 {
        return false;
    }

    let clear_maxs = [
        MANTLE_HALF_WIDTH,
        MANTLE_HALF_WIDTH,
        MANTLE_CLEARANCE_MAXS_Z,
    ];
    let fit = tracer.mantle_trace(
        results.ledge_pos,
        results.ledge_pos,
        mins,
        clear_maxs,
        tracemask,
    );
    if fit.startsolid != 0 {
        return false;
    }
    results.flags |= playerstate_iw4::mantle_flags::ACTIVE;
    if (down.surface_flags & SURF_MANTLE_OVER) != 0 {
        results.flags |= playerstate_iw4::mantle_flags::OVER;
    }
    true
}

pub fn mantle_calc_path(
    results: &mut MantleResults,
    tracemask: u32,
    tracer: &mut impl MantleCapsuleTrace,
) {
    if (results.flags & playerstate_iw4::mantle_flags::OVER) == 0 {
        results.end_pos = results.ledge_pos;
        return;
    }
    let mins = [-MANTLE_HALF_WIDTH, -MANTLE_HALF_WIDTH, 0.0];
    let maxs = [
        MANTLE_HALF_WIDTH,
        MANTLE_HALF_WIDTH,
        MANTLE_CLEARANCE_MAXS_Z,
    ];
    let start = results.ledge_pos;
    let end = [
        start[0] + results.dir[0] * MANTLE_OVER_FORWARD,
        start[1] + results.dir[1] * MANTLE_OVER_FORWARD,
        start[2] + results.dir[2] * MANTLE_OVER_FORWARD,
    ];
    let forward = tracer.mantle_trace(start, end, mins, maxs, tracemask);
    if forward.startsolid != 0 || forward.fraction < 1.0 {
        results.flags &= !(playerstate_iw4::mantle_flags::OVER | MANTLE_FLAG_EVENT7);
        results.end_pos = results.ledge_pos;
        return;
    }
    let drop_start = end;
    let drop_end = [end[0], end[1], end[2] - MANTLE_LEDGE_FLOOR_Z];
    let down = tracer.mantle_trace(drop_start, drop_end, mins, maxs, tracemask);
    if down.startsolid != 0 || down.fraction < 1.0 {
        results.flags &= !(playerstate_iw4::mantle_flags::OVER | MANTLE_FLAG_EVENT7);
        results.end_pos = results.ledge_pos;
        return;
    }
    results.end_pos = [
        drop_end[0],
        drop_end[1],
        drop_start[2] + (drop_end[2] - drop_start[2]) * down.fraction,
    ];
}

pub fn mantle_calc_end_pos(
    results: &mut MantleResults,
    tracemask: u32,
    tracer: &mut impl MantleCapsuleTrace,
) {
    mantle_calc_path(results, tracemask, tracer);
}

pub fn mantle_start_clearance(
    ps: &PlayerState,
    results: &mut MantleResults,
    tracemask: u32,
    tracer: &mut impl MantleCapsuleTrace,
) {
    if (ps.e_flags & 4) != 0 {
        return;
    }
    let zero = [0.0_f32; 3];
    let ledge = tracer.mantle_trace(results.ledge_pos, results.ledge_pos, zero, zero, tracemask);
    if ledge.startsolid != 0 {
        results.flags |= MANTLE_FLAG_EVENT7;
    }
    let end = tracer.mantle_trace(results.end_pos, results.end_pos, zero, zero, tracemask);
    if end.startsolid == 0 {
        results.flags |= MANTLE_FLAG_EVENT6;
    }
}

pub fn vector_angle_multiply(v: &mut [f32; 3], yaw_deg: f32) {
    let yaw = yaw_deg * VECTOR_ANGLE_DEG2RAD;
    let c = libm::cosf(yaw);
    let s = libm::sinf(yaw);
    let x = v[0];
    let y = v[1];
    v[1] = c * y + x * s;
    v[0] = c * x - s * y;
}

pub trait MantleRootDelta {
    fn abs_delta(&self, fast_mantle: bool, anim_index: i32, frac: f32) -> [f32; 3];
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ZeroMantleRootDelta;

impl MantleRootDelta for ZeroMantleRootDelta {
    fn abs_delta(&self, _fast_mantle: bool, _anim_index: i32, _frac: f32) -> [f32; 3] {
        [0.0; 3]
    }
}

pub fn mantle_create_anims_end_delta(anim_index: i32) -> [f32; 3] {
    match anim_index {
        1..=7 => {
            let row = (anim_index as usize) - 1;
            [MANTLE_FORWARD_DIST, 0.0, MANTLE_TRANS_HEIGHTS[row]]
        }
        8..=10 => [MANTLE_OVER_FORWARD, 0.0, -MANTLE_LEDGE_FLOOR_Z],
        _ => [0.0; 3],
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CreateAnimsMantleRootDelta;

impl MantleRootDelta for CreateAnimsMantleRootDelta {
    fn abs_delta(&self, _fast_mantle: bool, anim_index: i32, frac: f32) -> [f32; 3] {
        let end = mantle_create_anims_end_delta(anim_index);
        let f = frac.clamp(0.0, 1.0);
        [end[0] * f, end[1] * f, end[2] * f]
    }
}

pub fn mantle_sample_root_track(
    ps: &PlayerState,
    time_ms: i32,
    lengths: &impl MantleXAnimLength,
    root: &impl MantleRootDelta,
) -> [f32; 3] {
    let up_len = mantle_up_length(ps, lengths).max(1);
    let over_len = mantle_over_length(ps, lengths);
    let fast = mantle_fast_tree(ps.mantle_flags);
    let up_anim = mantle_trans_up_anim(ps.mantle_trans_index);
    let mut delta = if time_ms > up_len && over_len > 0 {
        let up_full = root.abs_delta(fast, up_anim, 1.0);
        let frac = (time_ms - up_len) as f32 / over_len as f32;
        let over_anim = mantle_trans_over_anim(ps.mantle_trans_index);
        let over = root.abs_delta(fast, over_anim, frac);
        [
            up_full[0] + over[0],
            up_full[1] + over[1],
            up_full[2] + over[2],
        ]
    } else {
        let frac = time_ms as f32 / up_len as f32;
        root.abs_delta(fast, up_anim, frac)
    };
    vector_angle_multiply(&mut delta, ps.mantle_yaw);
    delta
}

#[must_use]
pub fn mantle_find_ledge(
    ps: &PlayerState,
    context: MantleFindLedgeContext,
    results: &mut MantleResults,
    backend: &mut impl MantleLedgeBackend,
) -> bool {
    if !context.mantle_enable {
        return false;
    }
    if ps.pm_type >= 8 || (ps.pm_flags & PMF_MANTLE) != 0 || (ps.e_flags & 0xc) != 0 {
        return false;
    }

    let Some(dir) = backend.front_probe(CONTENTS_MANTLE) else {
        return false;
    };
    *results = MantleResults {
        dir,
        start_pos: ps.origin,
        ledge_pos: [0.0; 3],
        end_pos: [0.0; 3],
        flags: 0,
    };

    for height in context.ledge_heights {
        if backend.height_landing(height, results) {
            return true;
        }
    }
    false
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MantleLedgeProbeLog {
    slots: [Option<MantleLedgeProbe>; 4],
    len: usize,
}

impl MantleLedgeProbeLog {
    pub const fn new() -> Self {
        Self {
            slots: [None; 4],
            len: 0,
        }
    }

    pub fn push(&mut self, probe: MantleLedgeProbe) {
        assert!(self.len < 4, "FindLedge probe log overflow");
        self.slots[self.len] = Some(probe);
        self.len += 1;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn get(&self, index: usize) -> Option<MantleLedgeProbe> {
        self.slots.get(index).copied().flatten()
    }
}

#[must_use]
pub fn mantle_find_ledge_recording(
    ps: &PlayerState,
    context: MantleFindLedgeContext,
    results: &mut MantleResults,
    backend: &mut impl MantleLedgeBackend,
    log: &mut MantleLedgeProbeLog,
) -> bool {
    struct Recording<'a, B: MantleLedgeBackend> {
        inner: &'a mut B,
        log: &'a mut MantleLedgeProbeLog,
    }

    impl<B: MantleLedgeBackend> MantleLedgeBackend for Recording<'_, B> {
        fn front_probe(&mut self, contentmask: u32) -> Option<[f32; 3]> {
            self.log.push(MantleLedgeProbe::Front { contentmask });
            self.inner.front_probe(contentmask)
        }

        fn height_landing(&mut self, height: f32, results: &mut MantleResults) -> bool {
            self.log.push(MantleLedgeProbe::HeightLanding { height });
            self.inner.height_landing(height, results)
        }
    }

    let mut wrapped = Recording {
        inner: backend,
        log,
    };
    mantle_find_ledge(ps, context, results, &mut wrapped)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MantleCheckContext {
    pub find: MantleFindLedgeContext,

    pub buttons: u32,

    pub forwardmove: i8,

    pub facing_xy: [f32; 2],

    pub tracemask: u32,
}

const BUTTON_JUMP: u32 = 0x400;

const MANTLE_FLAG_AUTOMANTLE: u32 = 0x20;

const PERK_AUTOMANTLE: u32 = 0x2;

#[must_use]
pub fn mantle_is_weapon_inactive(ps: &PlayerState, mantle_enable: bool) -> bool {
    if !mantle_enable {
        return false;
    }
    if (ps.pm_flags & PMF_MANTLE) == 0 {
        return false;
    }
    mantle_trans_over_anim(ps.mantle_trans_index) != 10
}

#[must_use]
pub fn mantle_start_allowed(
    ps: &PlayerState,
    buttons: u32,
    results: &MantleResults,
    forwardmove: i8,
) -> bool {
    if (buttons & BUTTON_JUMP) != 0 {
        return true;
    }
    if (results.flags & MANTLE_FLAG_AUTOMANTLE) != 0 {
        return true;
    }
    if (ps.perks[1] & PERK_AUTOMANTLE) == 0 {
        return false;
    }
    let along = ps.velocity[0] * results.dir[0]
        + ps.velocity[1] * results.dir[1]
        + ps.velocity[2] * results.dir[2];
    along > 0.0 || (along == 0.0 && forwardmove > 0)
}

pub fn mantle_enter(
    ps: &mut PlayerState,
    results: &MantleResults,
    lengths: &impl MantleXAnimLength,
    root: &impl MantleRootDelta,
) {
    ps.mantle_yaw = vectoyaw(results.dir);
    ps.mantle_timer = 0;
    ps.mantle_trans_index = mantle_find_transition(results.start_pos[2], results.ledge_pos[2]);
    ps.mantle_flags = results.flags & !0x20;

    if (ps.perks[0] & 0x8_0000) != 0 {
        ps.mantle_flags |= playerstate_iw4::mantle_flags::FAST_MANTLE;
    }
    let duration = mantle_duration(ps, lengths);
    let trans = mantle_sample_root_track(ps, duration, lengths, root);
    ps.origin = [
        results.end_pos[0] - trans[0],
        results.end_pos[1] - trans[1],
        results.end_pos[2] - trans[2],
    ];
    ps.pm_flags |= PMF_MANTLE;
    add_predictable_event(ps, EV_MANTLE, 0);
    ps.e_flags |= EF_MANTLE;
}

pub const MANTLE_VIEW_YAWCAP_DEFAULT: f32 = 60.0;

const MANTLE_TRANS_HEIGHTS: [f32; 7] = [57.0, 51.0, 45.0, 39.0, 33.0, 27.0, 21.0];

pub const MANTLE_XANIM_TREE_SIZE: usize = 11;

pub const MANTLE_XANIM_NAMES: [&str; MANTLE_XANIM_TREE_SIZE] = [
    "mp_mantle_root",
    "mp_mantle_up_57",
    "mp_mantle_up_51",
    "mp_mantle_up_45",
    "mp_mantle_up_39",
    "mp_mantle_up_33",
    "mp_mantle_up_27",
    "mp_mantle_up_21",
    "mp_mantle_over_high",
    "mp_mantle_over_mid",
    "mp_mantle_over_low",
];

pub const MANTLE_XANIM_NAMES_FR: [&str; MANTLE_XANIM_TREE_SIZE] = [
    "mp_mantle_root",
    "mp_mantle_up_57_fr",
    "mp_mantle_up_51_fr",
    "mp_mantle_up_45_fr",
    "mp_mantle_up_39_fr",
    "mp_mantle_up_33_fr",
    "mp_mantle_up_27_fr",
    "mp_mantle_up_21_fr",
    "mp_mantle_over_high_fr",
    "mp_mantle_over_mid_fr",
    "mp_mantle_over_low_fr",
];

const MANTLE_TRANS_UP_ANIM: [i32; 7] = [1, 2, 3, 4, 5, 6, 7];
const MANTLE_TRANS_OVER_ANIM: [i32; 7] = [8, 8, 9, 9, 9, 10, 10];

pub fn mantle_find_transition(cur_height: f32, goal_height: f32) -> i32 {
    let height = goal_height - cur_height;
    let mut best_index = 0_i32;
    let mut best_diff = libm::fabsf(MANTLE_TRANS_HEIGHTS[0] - height);
    for (i, &table_h) in MANTLE_TRANS_HEIGHTS.iter().enumerate().skip(1) {
        let diff = libm::fabsf(table_h - height);
        if best_diff > diff {
            best_index = i as i32;
            best_diff = diff;
        }
    }
    best_index
}

fn mantle_trans_row(trans_index: i32) -> usize {
    let i = trans_index as usize;
    if i < MANTLE_TRANS_UP_ANIM.len() { i } else { 0 }
}

pub fn mantle_trans_up_anim(trans_index: i32) -> i32 {
    MANTLE_TRANS_UP_ANIM[mantle_trans_row(trans_index)]
}

pub fn mantle_trans_over_anim(trans_index: i32) -> i32 {
    MANTLE_TRANS_OVER_ANIM[mantle_trans_row(trans_index)]
}

pub trait MantleXAnimLength {
    fn length_msec(&self, fast_mantle: bool, anim_index: i32) -> i32;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlatMantleAnimLength {
    pub up_msec: i32,
    pub over_msec: i32,
}

impl Default for FlatMantleAnimLength {
    fn default() -> Self {
        Self {
            up_msec: 400,
            over_msec: 200,
        }
    }
}

impl MantleXAnimLength for FlatMantleAnimLength {
    fn length_msec(&self, _fast_mantle: bool, anim_index: i32) -> i32 {
        if (1..=7).contains(&anim_index) {
            self.up_msec
        } else {
            self.over_msec
        }
    }
}

fn mantle_fast_tree(flags: u32) -> bool {
    (flags >> 6) & 1 != 0
}

pub fn mantle_up_length(ps: &PlayerState, lengths: &impl MantleXAnimLength) -> i32 {
    let anim = mantle_trans_up_anim(ps.mantle_trans_index);
    lengths.length_msec(mantle_fast_tree(ps.mantle_flags), anim)
}

pub fn mantle_over_length(ps: &PlayerState, lengths: &impl MantleXAnimLength) -> i32 {
    if (ps.mantle_flags & playerstate_iw4::mantle_flags::OVER) == 0 {
        return 0;
    }
    let anim = mantle_trans_over_anim(ps.mantle_trans_index);
    lengths.length_msec(mantle_fast_tree(ps.mantle_flags), anim)
}

pub fn mantle_duration(ps: &PlayerState, lengths: &impl MantleXAnimLength) -> i32 {
    mantle_up_length(ps, lengths).saturating_add(mantle_over_length(ps, lengths))
}

pub fn mantle_active_xanim(ps: &PlayerState, lengths: &impl MantleXAnimLength) -> i32 {
    let up_len = mantle_up_length(ps, lengths);
    if ps.mantle_timer <= up_len {
        mantle_trans_up_anim(ps.mantle_trans_index)
    } else {
        mantle_trans_over_anim(ps.mantle_trans_index)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MantleCapViewContext {
    pub mantle_enable: bool,

    pub view_yawcap: f32,
}

impl Default for MantleCapViewContext {
    fn default() -> Self {
        Self {
            mantle_enable: true,
            view_yawcap: MANTLE_VIEW_YAWCAP_DEFAULT,
        }
    }
}

pub fn mantle_clear_hint(ps: &mut PlayerState) {
    ps.mantle_flags &=
        !(playerstate_iw4::mantle_flags::ACTIVE | playerstate_iw4::mantle_flags::QUICK);
}

pub fn mantle_cap_view(ps: &mut PlayerState, context: MantleCapViewContext) {
    debug_assert!(
        (ps.pm_flags & PMF_MANTLE) != 0,
        "Mantle_CapView requires PMF_MANTLE"
    );
    if !context.mantle_enable {
        return;
    }
    let yawcap = context.view_yawcap;
    let neg_cap = -yawcap;
    let mut delta = angle_subtract(ps.mantle_yaw, ps.viewangles[1]);

    if !(delta < neg_cap || yawcap < delta) {
        return;
    }
    while neg_cap > delta {
        delta += yawcap;
    }
    while delta > yawcap {
        delta -= yawcap;
    }
    let value = if delta <= 0.0 { yawcap } else { neg_cap };
    ps.delta_angles[1] += delta;
    ps.viewangles[1] = angle_normalize_360(ps.mantle_yaw + value);
}

const EV_MANTLE_MOVE_BIT2: i32 = 7;
const EV_MANTLE_MOVE_BIT4: i32 = 6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MantleMoveContext {
    pub mantle_enable: bool,

    pub max_vertical_end_velocity: f32,
}

impl Default for MantleMoveContext {
    fn default() -> Self {
        Self {
            mantle_enable: true,

            max_vertical_end_velocity: 0.0,
        }
    }
}

pub fn mantle_move(
    ps: &mut PlayerState,
    msec: i32,
    context: MantleMoveContext,
    lengths: &impl MantleXAnimLength,
    root: &impl MantleRootDelta,
) {
    debug_assert!(
        (ps.pm_flags & PMF_MANTLE) != 0,
        "Mantle_Move requires PMF_MANTLE"
    );
    if !context.mantle_enable {
        return;
    }

    ps.mantle_flags &= !playerstate_iw4::mantle_flags::ACTIVE;

    if (ps.mantle_flags & MANTLE_FLAG_EVENT7) != 0 {
        add_predictable_event(ps, EV_MANTLE_MOVE_BIT2, 0);
    }

    let duration = mantle_duration(ps, lengths).max(0);
    let prev = ps.mantle_timer;
    let next = prev.saturating_add(msec);
    ps.mantle_timer = if next > duration { duration } else { next };

    let prev_delta = mantle_sample_root_track(ps, prev, lengths, root);
    let cur_delta = mantle_sample_root_track(ps, ps.mantle_timer, lengths, root);
    let step = [
        cur_delta[0] - prev_delta[0],
        cur_delta[1] - prev_delta[1],
        cur_delta[2] - prev_delta[2],
    ];
    ps.origin[0] += step[0];
    ps.origin[1] += step[1];
    ps.origin[2] += step[2];
    let dt = (ps.mantle_timer - prev) as f32;
    if dt > 0.0 {
        let inv_sec = 1.0 / (dt * 0.001);
        ps.velocity[0] = step[0] * inv_sec;
        ps.velocity[1] = step[1] * inv_sec;
        ps.velocity[2] = step[2] * inv_sec;
    }
    let _ = mantle_active_xanim(ps, lengths);

    if ps.mantle_timer < duration {
        return;
    }

    ps.pm_flags &= !PMF_MANTLE;
    if (ps.mantle_flags & MANTLE_FLAG_EVENT6) != 0 {
        add_predictable_event(ps, EV_MANTLE_MOVE_BIT4, 0);
        ps.e_flags &= !EF_MANTLE;
    }
    if context.max_vertical_end_velocity < ps.velocity[2] {
        ps.velocity[2] = context.max_vertical_end_velocity;
    }
    ps.mantle_flags |= playerstate_iw4::mantle_flags::QUICK;
}

fn vectoyaw(dir: [f32; 3]) -> f32 {
    if dir[0] == 0.0 && dir[1] == 0.0 {
        return 0.0;
    }
    libm::atan2f(dir[1], dir[0]) * RAD2DEG
}

pub fn mantle_check(
    ps: &mut PlayerState,
    context: MantleCheckContext,
    tracer: &mut impl MantleCapsuleTrace,
    lengths: &impl MantleXAnimLength,
    root: &impl MantleRootDelta,
) -> bool {
    if (ps.pm_flags & PMF_MANTLE) != 0 {
        return false;
    }
    let mut results = MantleResults::default();
    let facing = context.facing_xy;
    let angle = context.find.check_angle_deg;
    let range = context.find.check_range;
    let radius = context.find.check_radius;
    let tracemask = context.tracemask;

    struct TraceBackend<'a, T: MantleCapsuleTrace> {
        tracer: &'a mut T,
        facing: [f32; 2],
        angle: f32,
        range: f32,
        radius: f32,
        origin: [f32; 3],
        tracemask: u32,
    }

    impl<T: MantleCapsuleTrace> MantleLedgeBackend for TraceBackend<'_, T> {
        fn front_probe(&mut self, contentmask: u32) -> Option<[f32; 3]> {
            let cast = mantle_front_probe_cast(self.origin, self.facing, self.range, self.radius)?;
            let hit =
                self.tracer
                    .mantle_trace(cast.start, cast.end, cast.mins, cast.maxs, contentmask);
            mantle_front_probe_accept(&hit, self.facing, self.angle)
        }

        fn height_landing(&mut self, height: f32, results: &mut MantleResults) -> bool {
            mantle_height_landing_probe(results, height, self.tracemask, self.tracer)
        }
    }

    let mut backend = TraceBackend {
        tracer,
        facing,
        angle,
        range,
        radius,
        origin: ps.origin,
        tracemask,
    };
    if !mantle_find_ledge(ps, context.find, &mut results, &mut backend) {
        return false;
    }

    ps.mantle_flags |= playerstate_iw4::mantle_flags::ACTIVE;
    results.flags |= playerstate_iw4::mantle_flags::ACTIVE;

    if !mantle_start_allowed(ps, context.buttons, &results, context.forwardmove) {
        return false;
    }
    mantle_calc_path(&mut results, tracemask, tracer);
    mantle_start_clearance(ps, &mut results, tracemask, tracer);
    mantle_enter(ps, &results, lengths, root);
    true
}
