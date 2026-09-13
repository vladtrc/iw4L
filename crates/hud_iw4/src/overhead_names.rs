pub const CROSSHAIR_SCAN_DISTANCE: f32 = 8192.0;

pub const OVERHEAD_TRACE_MASK: u32 = 0x0280_1001;

pub const ENEMY_NAME_FADE_MS: i32 = 250;

pub const FRIENDLY_NAME_FADE_IN_DEFAULT_MS: i32 = 0;

pub const FRIENDLY_NAME_FADE_OUT_DEFAULT_MS: i32 = 1500;

pub const FLASHBANG_NAME_FADE_IN_DEFAULT_MS: i32 = 1000;

pub const FLASHBANG_NAME_FADE_OUT_DEFAULT_MS: i32 = 50;

pub const OVERHEAD_HEAD_LIFT: f32 = 10.0;

pub const OVERHEAD_ORIGIN_FALLBACK_Z: f32 = 82.0;

pub const OVERHEAD_MAX_DISTANCE_DEFAULT: f32 = 10_000.0;

pub const OVERHEAD_NEAR_DISTANCE_DEFAULT: f32 = 64.0;

pub const OVERHEAD_FAR_DISTANCE_DEFAULT: f32 = 512.0;

pub const OVERHEAD_FAR_SCALE_DEFAULT: f32 = 0.7;

pub const OVERHEAD_NAME_SIZE_DEFAULT: f32 = 0.65;

pub const OVERHEAD_ICON_SIZE_DEFAULT: f32 = 1.1;

pub const OVERHEAD_RANK_SIZE_DEFAULT: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelativeTeamColorKey {
    MyTeam,
    EnemyTeam,
    MyParty,
    Spectator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartyRelation {
    SameParty,
    DifferentParty,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelativeTeamColorChoice {
    Key(RelativeTeamColorKey),
    PartyUnknown,
}

#[inline]
pub fn cg_relative_team_color_key(
    local_client: i32,
    local_team: i32,
    target_client: i32,
    target_team: i32,
    party: PartyRelation,
) -> RelativeTeamColorChoice {
    if target_team == 3 {
        return RelativeTeamColorChoice::Key(RelativeTeamColorKey::Spectator);
    }
    if target_client != local_client && (local_team == 0 || local_team != target_team) {
        return RelativeTeamColorChoice::Key(RelativeTeamColorKey::EnemyTeam);
    }
    if target_client == local_client {
        return RelativeTeamColorChoice::Key(RelativeTeamColorKey::MyTeam);
    }
    match party {
        PartyRelation::SameParty => RelativeTeamColorChoice::Key(RelativeTeamColorKey::MyParty),
        PartyRelation::DifferentParty => RelativeTeamColorChoice::Key(RelativeTeamColorKey::MyTeam),
        PartyRelation::Unknown => RelativeTeamColorChoice::PartyUnknown,
    }
}

#[inline]
pub fn cg_overhead_fade_alpha(
    now_ms: i32,
    start_ms: i32,
    last_seen_ms: i32,
    fade_in_ms: i32,
    fade_out_ms: i32,
) -> f32 {
    let since_last = now_ms.wrapping_sub(last_seen_ms);
    if fade_out_ms < since_last {
        return 0.0;
    }
    let visible_ms = last_seen_ms.wrapping_sub(start_ms);
    if visible_ms < fade_in_ms {
        return visible_ms as f32 / fade_in_ms as f32;
    }
    if since_last < fade_out_ms {
        return (fade_out_ms - since_last) as f32 / fade_out_ms as f32;
    }
    1.0
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverheadHeadResult {
    Exact([f32; 3]),
    NoDObjOrHead,
}

#[inline]
pub fn cg_overhead_anchor(head: OverheadHeadResult, centity_origin: [f32; 3]) -> [f32; 3] {
    match head {
        OverheadHeadResult::Exact(mut head) => {
            head[2] += OVERHEAD_HEAD_LIFT;
            head
        }
        OverheadHeadResult::NoDObjOrHead => [
            centity_origin[0],
            centity_origin[1],
            centity_origin[2] + OVERHEAD_ORIGIN_FALLBACK_Z,
        ],
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverheadView {
    pub origin: [f32; 3],

    pub axis: [[f32; 3]; 3],
    pub tan_half_fov_x: f32,
    pub tan_half_fov_y: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

#[inline]
pub fn cg_world_pos_to_overhead_pixel(view: OverheadView, world: [f32; 3]) -> Option<[f32; 2]> {
    let delta = [
        world[0] - view.origin[0],
        world[1] - view.origin[1],
        world[2] - view.origin[2],
    ];
    let forward = dot(delta, view.axis[0]);
    if forward < 0.0 {
        return None;
    }
    let right = dot(delta, view.axis[1]);
    let up = dot(delta, view.axis[2]);
    Some([
        (1.0 - right / view.tan_half_fov_x / forward) * view.viewport_width * 0.5,
        (1.0 - up / view.tan_half_fov_y / forward) * view.viewport_height * 0.5,
    ])
}

#[inline]
pub fn cg_overhead_distance_scale(
    view_origin: [f32; 3],
    anchor: [f32; 3],
    near_distance: f32,
    far_distance: f32,
    far_scale: f32,
) -> f32 {
    let delta = [
        anchor[0] - view_origin[0],
        anchor[1] - view_origin[1],
        anchor[2] - view_origin[2],
    ];
    let distance_sq = dot(delta, delta);
    if distance_sq < near_distance * near_distance {
        return 1.0;
    }
    if distance_sq <= far_distance * far_distance {
        let t = (libm::sqrtf(distance_sq) - near_distance) / (far_distance - near_distance);
        return (1.0 - t) + t * far_scale;
    }
    far_scale
}

#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
