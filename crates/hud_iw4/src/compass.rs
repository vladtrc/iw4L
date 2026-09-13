pub const COMPASS_MAX_RANGE_DEFAULT_MP: f32 = 2500.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompassMapBounds {
    pub upper_left: [f32; 2],

    pub world_size: [f32; 2],

    pub north: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompassMapUvWindow {
    pub center: [f32; 2],

    pub half_s: f32,

    pub half_t: f32,

    pub radius_st: f32,

    pub scale_final_s: f32,

    pub scale_final_t: f32,
}

impl CompassMapUvWindow {
    #[must_use]
    pub fn uv_rect(self) -> [f32; 4] {
        [
            self.center[0] - self.half_s,
            self.center[1] - self.half_t,
            self.center[0] + self.half_s,
            self.center[1] + self.half_t,
        ]
    }
}

#[must_use]
pub fn compass_map_bounds_from_corners(
    upper_left: [f32; 2],
    lower_right: [f32; 2],
    north_yaw_degrees: f32,
) -> Option<CompassMapBounds> {
    let rad = north_yaw_degrees * (core::f32::consts::PI / 180.0);
    let north = [libm::cosf(rad), libm::sinf(rad)];
    let diff = [
        lower_right[0] - upper_left[0],
        lower_right[1] - upper_left[1],
    ];
    let world_size = [
        diff[0] * north[1] - diff[1] * north[0],
        -diff[0] * north[0] - diff[1] * north[1],
    ];
    if world_size[0] <= 0.0 || world_size[1] <= 0.0 {
        return None;
    }
    Some(CompassMapBounds {
        upper_left,
        world_size,
        north,
    })
}

pub const REQUIRED_MAP_ASPECT_RATIO_DEFAULT: f32 = 1.0;

#[must_use]
pub fn setup_mini_map_frame(
    corner0: [f32; 2],
    corner1: [f32; 2],
    north_yaw_degrees: f32,
    required_aspect: f32,
) -> ([f32; 2], [f32; 2]) {
    let rad = north_yaw_degrees * (core::f32::consts::PI / 180.0);
    let north = [libm::cosf(rad), libm::sinf(rad)];
    let west = [-north[1], north[0]];
    let diff = [corner1[0] - corner0[0], corner1[1] - corner0[1]];
    let along_west = diff[0] * west[0] + diff[1] * west[1];
    let along_north = diff[0] * north[0] + diff[1] * north[1];
    let side = [north[0] * along_north, north[1] * along_north];

    let (mut northwest, mut southeast) = match (along_west > 0.0, along_north > 0.0) {
        (true, true) => (corner1, corner0),

        (true, false) => (sub(corner1, side), add(corner0, side)),

        (false, true) => (add(corner0, side), sub(corner1, side)),

        (false, false) => (corner0, corner1),
    };

    if required_aspect > 0.0 {
        let span = sub(northwest, southeast);
        let north_portion = span[0] * north[0] + span[1] * north[1];
        let west_portion = span[0] * west[0] + span[1] * west[1];
        if north_portion != 0.0 && west_portion != 0.0 {
            let map_aspect = west_portion / north_portion;
            let addvec = if map_aspect < required_aspect {
                let incr = required_aspect / map_aspect;
                scale(west, west_portion * (incr - 1.0) * 0.5)
            } else {
                let incr = map_aspect / required_aspect;
                scale(north, north_portion * (incr - 1.0) * 0.5)
            };
            northwest = add(northwest, addvec);
            southeast = sub(southeast, addvec);
        }
    }
    (northwest, southeast)
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn scale(v: [f32; 2], s: f32) -> [f32; 2] {
    [v[0] * s, v[1] * s]
}

#[must_use]
pub fn compass_map_bounds_from_minimap_corners(
    a: [f32; 2],
    b: [f32; 2],
    north_yaw_degrees: f32,
) -> Option<CompassMapBounds> {
    let (northwest, southeast) =
        setup_mini_map_frame(a, b, north_yaw_degrees, REQUIRED_MAP_ASPECT_RATIO_DEFAULT);
    compass_map_bounds_from_corners(northwest, southeast, north_yaw_degrees)
}

#[must_use]
pub fn compass_partial_map_uv(
    bounds: CompassMapBounds,
    view_xy: [f32; 2],
    compass_max_range: f32,
) -> CompassMapUvWindow {
    let east = [bounds.north[1], -bounds.north[0]];
    let south = [-bounds.north[0], -bounds.north[1]];
    let delta = [
        view_xy[0] - bounds.upper_left[0],
        view_xy[1] - bounds.upper_left[1],
    ];
    let delta_east = east[0] * delta[0] + east[1] * delta[1];
    let delta_south = south[0] * delta[0] + south[1] * delta[1];
    let tex_center = [
        delta_east / bounds.world_size[0],
        delta_south / bounds.world_size[1],
    ];

    let half_range = compass_max_range * 0.5;
    let (tex_radius, scale_s, scale_t) = if bounds.world_size[1] >= bounds.world_size[0] {
        (
            half_range / bounds.world_size[1],
            bounds.world_size[1] / bounds.world_size[0],
            1.0,
        )
    } else {
        (
            half_range / bounds.world_size[0],
            1.0,
            bounds.world_size[0] / bounds.world_size[1],
        )
    };

    CompassMapUvWindow {
        center: tex_center,
        half_s: tex_radius * scale_s,
        half_t: tex_radius * scale_t,
        radius_st: tex_radius,
        scale_final_s: scale_s,
        scale_final_t: scale_t,
    }
}

pub const COMPASS_SOUND_PING_FADE_TIME_DEFAULT: f32 = 2.0;

pub const COMPASS_PLAYER_WIDTH_DEFAULT: f32 = 18.75;
pub const COMPASS_PLAYER_HEIGHT_DEFAULT: f32 = 18.75;

pub const COMPASS_SIZE_DEFAULT: f32 = 1.0;

#[must_use]
pub fn cg_compass_player_size(
    player_width: f32,
    player_height: f32,
    compass_size: f32,
) -> [f32; 2] {
    [player_width * compass_size, player_height * compass_size]
}

pub const COMPASS_FRIENDLY_WIDTH_DEFAULT: f32 = 16.0;
pub const COMPASS_FRIENDLY_HEIGHT_DEFAULT: f32 = 16.0;

#[must_use]
pub fn cg_compass_friendly_size(
    friendly_width: f32,
    friendly_height: f32,
    compass_size: f32,
) -> [f32; 2] {
    [
        friendly_width * compass_size,
        friendly_height * compass_size,
    ]
}

pub const COMPASS_PLAYER_IMAGE: &str = "compassping_player";

pub const COMPASS_ENEMY_FIRING_PING_IMAGE: &str = "compassping_enemyfiring";

#[must_use]
pub fn cg_compass_up_yaw_vector(view_yaw_degrees: f32) -> [f32; 2] {
    let rad = view_yaw_degrees * (core::f32::consts::PI / 180.0);
    [libm::cosf(rad), libm::sinf(rad)]
}

#[must_use]
pub fn cg_world_pos_to_compass_partial(
    north: [f32; 2],
    player_xy: [f32; 2],
    world_xy: [f32; 2],
    map_rect_h: f32,
    compass_max_range: f32,
) -> [f32; 2] {
    let pix_per_inch = map_rect_h / compass_max_range;
    let dx = (world_xy[0] - player_xy[0]) * pix_per_inch;
    let dy = (world_xy[1] - player_xy[1]) * pix_per_inch;
    [
        north[1] * dx - north[0] * dy,
        -north[1] * dy - north[0] * dx,
    ]
}

#[must_use]
pub fn cg_compass_sound_ping_fade(
    cg_time_ms: i32,
    begin_fade_time_ms: i32,
    fade_seconds: f32,
) -> Option<f32> {
    if begin_fade_time_ms == 0 {
        return None;
    }
    let duration_ms = fade_seconds * 1000.0;
    if (begin_fade_time_ms as f32) + duration_ms <= cg_time_ms as f32 {
        return None;
    }
    if begin_fade_time_ms >= cg_time_ms {
        return Some(1.0);
    }
    Some(1.0 - (cg_time_ms - begin_fade_time_ms) as f32 / duration_ms)
}

#[must_use]
pub fn radar_contact_trail_visible(perks0: u32) -> bool {
    (perks0 & 1) == 0 && (perks0 & 0x0800_0000) == 0
}

pub const RADARJAM_DIST_MIN: f32 = 350.0;

pub const RADARJAM_DIST_MAX: f32 = 800.0;

pub const RADARJAM_DIST_NONE: f32 = -1.0;

#[must_use]
pub fn cg_radar_jam_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    libm::sqrtf(dx * dx + dy * dy + dz * dz)
}

#[must_use]
pub fn cg_radar_jam_nearest_distance(
    local_origin: [f32; 3],
    jammer_origins: impl IntoIterator<Item = [f32; 3]>,
) -> f32 {
    let mut best = RADARJAM_DIST_NONE;
    for origin in jammer_origins {
        let dist = cg_radar_jam_distance(local_origin, origin);
        if best < 0.0 || dist < best {
            best = dist;
        }
    }
    best
}

#[must_use]
pub fn cg_radar_jam_intensity(dist: f32, dist_min: f32, dist_max: f32, emp: bool) -> f32 {
    if emp {
        return 1.0;
    }
    if dist <= 0.0 || dist > dist_max {
        return 0.0;
    }
    let span = dist_max - dist_min;
    if span == 0.0 {
        return 0.0;
    }
    if dist_min < dist {
        1.0 - (dist - dist_min) / span
    } else {
        1.0
    }
}

#[must_use]
pub fn cg_compass_fade_alpha(fade_compass: f32, jam_intensity: f32) -> f32 {
    (1.0 - jam_intensity) * fade_compass
}
