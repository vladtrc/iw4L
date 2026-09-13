#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmmoCounterClipKind {
    None,

    Magazine,

    ShortMagazine,

    Shotgun,

    Rocket,

    Beltfed,

    AltWeapon,
}

pub const CLIP_PIP_EMPTY_RGB: f32 = 0.3;

pub const CLIP_PIP_EMPTY_ALPHA: f32 = 0.4;

#[must_use]
pub fn ammo_counter_clip_kind(ordinal: i32) -> Option<AmmoCounterClipKind> {
    Some(match ordinal {
        0 => AmmoCounterClipKind::None,
        1 => AmmoCounterClipKind::Magazine,
        2 => AmmoCounterClipKind::ShortMagazine,
        3 => AmmoCounterClipKind::Shotgun,
        4 => AmmoCounterClipKind::Rocket,
        5 => AmmoCounterClipKind::Beltfed,
        6 => AmmoCounterClipKind::AltWeapon,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipPipMetrics {
    pub width: f32,
    pub height: f32,

    pub step_x: f32,

    pub wrap: i32,
    pub step_y: f32,

    pub image: &'static str,
}

pub const CLIP_PIP_ALIGN_RIGHT: i32 = 10;

#[must_use]
pub fn clip_pip_grid_xy(
    metrics: ClipPipMetrics,
    base: [f32; 2],
    index: i32,
    align: i32,
) -> [f32; 2] {
    let wrap = metrics.wrap.max(1);
    let y0 = base[1] - metrics.height * 0.5;
    let (x0, step) = if align == CLIP_PIP_ALIGN_RIGHT {
        (base[0] - metrics.width, -metrics.step_x)
    } else {
        (base[0], metrics.step_x)
    };
    if index < 0 {
        return [x0, y0];
    }
    let mut x = x0;
    let mut y = y0;
    for i in 0..=index {
        if i != 0 && i % wrap == 0 {
            x = x0;
            y += metrics.step_y;
        }
        if i == index {
            return [x, y];
        }
        x += step;
    }
    [x, y]
}

#[must_use]
pub fn clip_pip_belt_xy(
    metrics: ClipPipMetrics,
    base: [f32; 2],
    index: i32,
    clip_size: i32,
    align: i32,
) -> [f32; 2] {
    let wrap = metrics.wrap.max(1);
    let mut x = base[0];
    let mut y = metrics.height * 0.5 * (clip_size / wrap) as f32 + base[1];
    let mut step = if align == CLIP_PIP_ALIGN_RIGHT {
        -metrics.step_x
    } else {
        metrics.step_x
    };
    if clip_size <= 0 || index < 0 {
        return [x, y];
    }
    let last = index.min(clip_size - 1);
    for i in 0..=last {
        if i % wrap == 0 {
            step *= -1.0;
            y += metrics.step_y;
            x += step;
        }
        if i == last {
            return [x, y];
        }
        x += step;
    }
    [x, y]
}

#[must_use]
pub fn clip_pip_metrics(kind: AmmoCounterClipKind) -> Option<ClipPipMetrics> {
    Some(match kind {
        AmmoCounterClipKind::None | AmmoCounterClipKind::AltWeapon => return None,

        AmmoCounterClipKind::Magazine => ClipPipMetrics {
            width: 4.0,
            height: 20.0,
            step_x: 4.0,
            wrap: 50,
            step_y: -20.0,
            image: "ammo_counter_bullet_mp",
        },

        AmmoCounterClipKind::ShortMagazine => ClipPipMetrics {
            width: 9.0,
            height: 20.0,
            step_x: 9.0,
            wrap: 16,
            step_y: -20.0,
            image: "ammo_counter_riflebullet_mp",
        },

        AmmoCounterClipKind::Shotgun => ClipPipMetrics {
            width: 11.0,
            height: 20.0,
            step_x: 11.0,
            wrap: 14,
            step_y: -20.0,
            image: "ammo_counter_shotgunshell_mp",
        },

        AmmoCounterClipKind::Rocket => ClipPipMetrics {
            width: 11.0,
            height: 20.0,
            step_x: 11.0,
            wrap: 14,
            step_y: -20.0,
            image: "ammo_counter_rocket_mp",
        },

        AmmoCounterClipKind::Beltfed => ClipPipMetrics {
            width: 7.0,
            height: 4.0,
            step_x: 8.0,
            wrap: 20,
            step_y: -16.0,
            image: "ammo_counter_beltbullet_mp",
        },
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LowAmmoWarningKind {
    Reload,

    LowAmmo,

    NoAmmo,
}

impl LowAmmoWarningKind {
    #[must_use]
    pub fn loc_key(self) -> &'static str {
        match self {
            Self::Reload => "PLATFORM_RELOAD",
            Self::LowAmmo => "PLATFORM_LOW_AMMO_NO_RELOAD",
            Self::NoAmmo => "WEAPON_NO_AMMO",
        }
    }
}

pub const LOW_AMMO_WARNING_EFLAGS_SILENCE: u32 = 0x100c00;

pub const LOW_AMMO_WARNING_PULSE_HZ: f32 = 1.7;

pub const LOW_AMMO_WARNING_PULSE_MIN: f32 = 0.0;
pub const LOW_AMMO_WARNING_PULSE_MAX: f32 = 1.5;

#[must_use]
pub fn cg_low_ammo_warning_outer_gate(pm_type: i32, e_flags: u32, weapon: u32) -> bool {
    pm_type < 8 && pm_type != 5 && (e_flags & LOW_AMMO_WARNING_EFLAGS_SILENCE) == 0 && weapon != 0
}

#[must_use]
pub fn weaponstate_skips_low_ammo_warning(weaponstate: i32) -> bool {
    matches!(weaponstate, 8 | 9 | 10 | 11 | 12)
}

#[must_use]
pub fn cg_check_player_for_low_clip(
    clip: i32,
    clip_size: i32,
    threshold: f32,
    clip_only: bool,
) -> bool {
    !clip_only && clip >= 0 && clip_size > 0 && (clip_size as f32) * threshold >= clip as f32
}

#[must_use]
pub fn cg_low_ammo_warning_kind(
    stock: i32,
    clip_total: i32,
    clip_size: i32,
) -> Option<LowAmmoWarningKind> {
    if stock < 1 {
        Some(if clip_total == 0 {
            LowAmmoWarningKind::NoAmmo
        } else {
            LowAmmoWarningKind::LowAmmo
        })
    } else if clip_size == 1 {
        None
    } else {
        Some(LowAmmoWarningKind::Reload)
    }
}

#[must_use]
pub fn cg_low_ammo_warning_pulse_frac(cg_time_ms: i32) -> f32 {
    let t = cg_time_ms as f32 * 0.001;
    let amp = (LOW_AMMO_WARNING_PULSE_MAX - LOW_AMMO_WARNING_PULSE_MIN) * 0.5;
    let bias = LOW_AMMO_WARNING_PULSE_MIN + amp;
    let wave = libm::sinf(core::f32::consts::TAU * LOW_AMMO_WARNING_PULSE_HZ * t);
    (wave * amp + bias).clamp(LOW_AMMO_WARNING_PULSE_MIN, LOW_AMMO_WARNING_PULSE_MAX)
}

#[must_use]
pub fn cg_low_ammo_warning_color_pair(kind: LowAmmoWarningKind) -> ([f32; 4], [f32; 4]) {
    match kind {
        LowAmmoWarningKind::Reload => ([0.9, 0.9, 0.9, 0.8], [1.0, 1.0, 1.0, 1.0]),
        LowAmmoWarningKind::NoAmmo => ([0.8, 0.0, 0.0, 0.8], [1.0, 0.0, 0.0, 1.0]),
        LowAmmoWarningKind::LowAmmo => ([0.7, 0.7, 0.0, 0.8], [1.0, 1.0, 0.0, 1.0]),
    }
}

#[must_use]
pub fn vec4_lerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct LowAmmoWarningQuery {
    pub pm_type: i32,
    pub e_flags: u32,
    pub weapon: u32,
    pub ammo_counter_clip: i32,
    pub weaponstate: [i32; 2],

    pub hands: usize,
    pub clip: [i32; 2],
    pub clip_size: i32,
    pub stock: i32,
    pub threshold: f32,

    pub clip_only: bool,
}

#[must_use]
pub fn cg_draw_player_weapon_low_ammo_warning(
    q: LowAmmoWarningQuery,
) -> Option<LowAmmoWarningKind> {
    if !cg_low_ammo_warning_outer_gate(q.pm_type, q.e_flags, q.weapon) {
        return None;
    }
    if q.ammo_counter_clip == 0 {
        return None;
    }
    let hands = q.hands.clamp(1, 2);
    let mut live = false;
    for i in 0..hands {
        if weaponstate_skips_low_ammo_warning(q.weaponstate[i]) {
            continue;
        }
        if cg_check_player_for_low_clip(q.clip[i], q.clip_size, q.threshold, q.clip_only) {
            live = true;
            break;
        }
    }
    if !live {
        return None;
    }
    let clip_total = q.clip[0] + if hands > 1 { q.clip[1] } else { 0 };
    cg_low_ammo_warning_kind(q.stock, clip_total, q.clip_size)
}
