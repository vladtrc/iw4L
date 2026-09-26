pub const G_PLAYER_COLLISION_EJECT_SPEED_DEFAULT: i32 = 25;

pub(crate) const STUCK_PM_TIME: i32 = 300;

pub(crate) const STUCK_PM_FLAGS: u32 = 0x80;

pub(crate) const OTHER_FLAGS_PLAYER: u32 = 0x1000;

const CRANDOM_SCALE: f32 = 1.0 / 32768.0;

const CRANDOM_BIAS: f32 = 1.0;

const EJECT_SPEED_EPS: f32 = 0.0001;

#[derive(Clone, Copy, Debug)]
pub struct StuckClient {
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub speed: i32,
    pub other_flags: u32,
    pub health: i32,
    pub pm_time: i32,
    pub pm_flags: u32,

    pub maxs_x: f32,

    pub bounds_mid: [f32; 3],
    pub bounds_half: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StuckEject {
    pub self_idx: usize,
    pub other_idx: usize,
}

pub fn stuck_in_client(
    self_idx: usize,
    clients: &mut [StuckClient],
    eject_speed: i32,
    holdrand: &mut u32,
) -> Option<StuckEject> {
    let self_row = clients.get(self_idx)?;
    if !self_eligible(self_row) {
        return None;
    }
    let n = clients.len();
    let mut other_idx = None;
    for i in 0..n {
        if i == self_idx {
            continue;
        }
        if other_eligible(self_idx, i, clients) {
            other_idx = Some(i);
            break;
        }
    }
    let other_idx = other_idx?;

    let self_origin = clients[self_idx].origin;
    let other_origin = clients[other_idx].origin;
    let mut dx = other_origin[0] - self_origin[0];
    let mut dy = other_origin[1] - self_origin[1];
    dx += crandom(holdrand);
    dy += crandom(holdrand);
    vec2_normalize(&mut dx, &mut dy);

    let other_xy = vec2_length(clients[other_idx].velocity);
    let self_xy = vec2_length(clients[self_idx].velocity);
    let mut other_speed = if other_xy > 0.0 {
        eject_speed as f32
    } else {
        0.0
    };
    let mut self_speed = if self_xy > 0.0 {
        eject_speed as f32
    } else {
        0.0
    };
    if other_speed < EJECT_SPEED_EPS && self_speed < EJECT_SPEED_EPS {
        other_speed = clients[other_idx].speed as f32;
        self_speed = clients[self_idx].speed as f32;
    }

    clients[other_idx].velocity[0] = other_speed * dx;
    clients[other_idx].velocity[1] = other_speed * dy;
    clients[other_idx].pm_time = STUCK_PM_TIME;
    clients[other_idx].pm_flags |= STUCK_PM_FLAGS;

    clients[self_idx].velocity[0] = -self_speed * dx;
    clients[self_idx].velocity[1] = -self_speed * dy;
    clients[self_idx].pm_time = STUCK_PM_TIME;
    clients[self_idx].pm_flags |= STUCK_PM_FLAGS;

    Some(StuckEject {
        self_idx,
        other_idx,
    })
}

fn self_eligible(row: &StuckClient) -> bool {
    (row.other_flags & OTHER_FLAGS_PLAYER) != 0 && row.health > 0
}

fn other_eligible(self_idx: usize, other_idx: usize, clients: &[StuckClient]) -> bool {
    let other = &clients[other_idx];
    let self_row = &clients[self_idx];
    if (other.other_flags & OTHER_FLAGS_PLAYER) == 0 {
        return false;
    }
    if other.health <= 0 {
        return false;
    }
    if !bounds_overlap(
        other.bounds_mid,
        other.bounds_half,
        self_row.bounds_mid,
        self_row.bounds_half,
    ) {
        return false;
    }
    let dx = other.origin[0] - self_row.origin[0];
    let dy = other.origin[1] - self_row.origin[1];
    let radius = other.maxs_x + self_row.maxs_x;
    dx * dx + dy * dy <= radius * radius
}

fn bounds_overlap(a_mid: [f32; 3], a_half: [f32; 3], b_mid: [f32; 3], b_half: [f32; 3]) -> bool {
    let dx = abs_f32(a_mid[0] - b_mid[0]);
    let dy = abs_f32(a_mid[1] - b_mid[1]);
    let dz = abs_f32(a_mid[2] - b_mid[2]);
    dx <= a_half[0] + b_half[0] && dy <= a_half[1] + b_half[1] && dz <= a_half[2] + b_half[2]
}

pub(crate) fn crandom(holdrand: &mut u32) -> f32 {
    *holdrand = holdrand.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    let unit = (*holdrand >> 17) as f32 * CRANDOM_SCALE;
    (unit + unit) - CRANDOM_BIAS
}

fn vec2_normalize(x: &mut f32, y: &mut f32) {
    let mut len = libm::sqrtf(*x * *x + *y * *y);
    if 0.0 <= -len {
        len = 1.0;
    }
    let inv = 1.0 / len;
    *x *= inv;
    *y *= inv;
}

fn vec2_length(velocity: [f32; 3]) -> f32 {
    libm::sqrtf(velocity[0] * velocity[0] + velocity[1] * velocity[1])
}

fn abs_f32(v: f32) -> f32 {
    if v < 0.0 { -v } else { v }
}
