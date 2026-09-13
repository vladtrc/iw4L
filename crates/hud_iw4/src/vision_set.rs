pub const VISION_SET_VARS_SIZE: usize = 0x70;

pub const VISION_DEF_FIELD_COUNT: usize = 21;

pub const VISION_SET_LERP_NONE: i32 = 0;

pub const VISION_SET_LERP_HOLD: i32 = 1;

pub const VISION_SET_LERP_TO_LINEAR: i32 = 2;

pub const VISION_SET_LERP_TO_SMOOTH: i32 = 3;

pub const VISION_SET_LERP_BACKFORTH_LINEAR: i32 = 4;

pub const VISION_SET_LERP_BACKFORTH_SMOOTH: i32 = 5;

pub const R_GLOW_ALLOWED_DEFAULT: bool = false;

pub const R_GLOW_ALLOWED_SCRIPT_FORCED_DEFAULT: bool = false;

pub const VISION_HOLD_BLEND_RATE: f32 = 0.001;

pub const VISION_HOLD_BLEND_RATE_NEG: f32 = -0.001;

pub const VISION_GLOW_FADE_INTENSITY_MAX: f32 = 1.5;

pub const VISION_GLOW_FADE_CUTOFF_MIN: f32 = 0.4;

const LERP_PI: f32 = 3.141592741012573;

const LERP_HALF: f32 = 0.5;

const LERP_HALF_F32: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisionDefField {
    pub name: &'static str,
    pub offset: u16,
    pub field_type: u8,
}

pub const VISION_DEF_FIELDS: [VisionDefField; VISION_DEF_FIELD_COUNT] = [
    VisionDefField {
        name: "r_glow",
        offset: 0x00,
        field_type: 0,
    },
    VisionDefField {
        name: "r_glowBloomCutoff",
        offset: 0x04,
        field_type: 1,
    },
    VisionDefField {
        name: "r_glowBloomDesaturation",
        offset: 0x08,
        field_type: 1,
    },
    VisionDefField {
        name: "r_glowBloomIntensity0",
        offset: 0x0c,
        field_type: 1,
    },
    VisionDefField {
        name: "r_glowBloomIntensity1",
        offset: 0x10,
        field_type: 1,
    },
    VisionDefField {
        name: "r_glowRadius0",
        offset: 0x14,
        field_type: 1,
    },
    VisionDefField {
        name: "r_glowRadius1",
        offset: 0x18,
        field_type: 1,
    },
    VisionDefField {
        name: "r_glowSkyBleedIntensity0",
        offset: 0x1c,
        field_type: 1,
    },
    VisionDefField {
        name: "r_glowSkyBleedIntensity1",
        offset: 0x20,
        field_type: 1,
    },
    VisionDefField {
        name: "r_filmEnable",
        offset: 0x24,
        field_type: 0,
    },
    VisionDefField {
        name: "r_filmBrightness",
        offset: 0x28,
        field_type: 1,
    },
    VisionDefField {
        name: "r_filmContrast",
        offset: 0x2c,
        field_type: 1,
    },
    VisionDefField {
        name: "r_filmDesaturation",
        offset: 0x30,
        field_type: 1,
    },
    VisionDefField {
        name: "r_filmDesaturationDark",
        offset: 0x34,
        field_type: 1,
    },
    VisionDefField {
        name: "r_filmInvert",
        offset: 0x38,
        field_type: 0,
    },
    VisionDefField {
        name: "r_filmLightTint",
        offset: 0x3c,
        field_type: 2,
    },
    VisionDefField {
        name: "r_filmMediumTint",
        offset: 0x48,
        field_type: 2,
    },
    VisionDefField {
        name: "r_filmDarkTint",
        offset: 0x54,
        field_type: 2,
    },
    VisionDefField {
        name: "r_primaryLightUseTweaks",
        offset: 0x60,
        field_type: 0,
    },
    VisionDefField {
        name: "r_primaryLightTweakDiffuseStrength",
        offset: 0x64,
        field_type: 1,
    },
    VisionDefField {
        name: "r_primaryLightTweakSpecularStrength",
        offset: 0x68,
        field_type: 1,
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisionSetVars {
    pub r_glow: bool,
    pub r_glow_bloom_cutoff: f32,
    pub r_glow_bloom_desaturation: f32,
    pub r_glow_bloom_intensity0: f32,
    pub r_glow_bloom_intensity1: f32,
    pub r_glow_radius0: f32,
    pub r_glow_radius1: f32,
    pub r_glow_sky_bleed_intensity0: f32,
    pub r_glow_sky_bleed_intensity1: f32,
    pub r_film_enable: bool,
    pub r_film_brightness: f32,
    pub r_film_contrast: f32,
    pub r_film_desaturation: f32,
    pub r_film_desaturation_dark: f32,
    pub r_film_invert: bool,
    pub r_film_light_tint: [f32; 3],
    pub r_film_medium_tint: [f32; 3],
    pub r_film_dark_tint: [f32; 3],
    pub r_primary_light_use_tweaks: bool,
    pub r_primary_light_tweak_diffuse: f32,
    pub r_primary_light_tweak_specular: f32,

    pub blend: f32,
}

impl Default for VisionSetVars {
    fn default() -> Self {
        Self {
            r_glow: false,
            r_glow_bloom_cutoff: 0.0,
            r_glow_bloom_desaturation: 0.0,
            r_glow_bloom_intensity0: 0.0,
            r_glow_bloom_intensity1: 0.0,
            r_glow_radius0: 0.0,
            r_glow_radius1: 0.0,
            r_glow_sky_bleed_intensity0: 0.0,
            r_glow_sky_bleed_intensity1: 0.0,
            r_film_enable: false,
            r_film_brightness: 0.0,
            r_film_contrast: 0.0,
            r_film_desaturation: 0.0,
            r_film_desaturation_dark: 0.0,
            r_film_invert: false,
            r_film_light_tint: [0.0; 3],
            r_film_medium_tint: [0.0; 3],
            r_film_dark_tint: [0.0; 3],
            r_primary_light_use_tweaks: false,
            r_primary_light_tweak_diffuse: 1.0,
            r_primary_light_tweak_specular: 1.0,
            blend: 1.0,
        }
    }
}

impl VisionSetVars {
    #[must_use]
    pub fn presented_glow_intensity0(self) -> f32 {
        self.blend * self.r_glow_bloom_intensity0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisionSetLerpData {
    pub time_start: i32,
    pub time_duration: i32,
    pub style: i32,
    pub blend: f32,
    pub last_time: i32,
}

impl Default for VisionSetLerpData {
    fn default() -> Self {
        Self {
            time_start: 0,
            time_duration: 0,
            style: VISION_SET_LERP_NONE,
            blend: 1.0,
            last_time: 0,
        }
    }
}

#[must_use]
fn com_clamp(val: f32, min: f32, max: f32) -> f32 {
    let mut out = val;
    if (out - max) > 0.0 {
        out = max;
    }
    if (min - val) > 0.0 { min } else { out }
}

#[must_use]
pub fn cg_vision_lerp_float(from: f32, to: f32, mut fraction: f32, style: i32) -> f32 {
    if style == VISION_SET_LERP_TO_LINEAR {
        return from + fraction * (to - from);
    }
    if style == VISION_SET_LERP_TO_SMOOTH {
        fraction = libm::sinf(fraction * LERP_PI * LERP_HALF);
        return from + fraction * (to - from);
    }
    if style == VISION_SET_LERP_BACKFORTH_SMOOTH {
        fraction = libm::sinf(fraction * LERP_PI * LERP_HALF);
    }
    if fraction < LERP_HALF_F32 {
        let d = (to - from) * fraction;
        d + d + from
    } else {
        let d = (fraction - LERP_HALF) * (from - to);
        d + d + to
    }
}

#[must_use]
pub fn cg_vision_lerp_bool(from: bool, to: bool, fraction: f32, style: i32) -> bool {
    if style < VISION_SET_LERP_BACKFORTH_LINEAR || style > VISION_SET_LERP_BACKFORTH_SMOOTH {
        to
    } else if (LERP_HALF_F32 < fraction) == (LERP_HALF_F32 == fraction) {
        to
    } else {
        from
    }
}

#[must_use]
pub fn cg_vision_lerp_vec3(from: [f32; 3], to: [f32; 3], fraction: f32, style: i32) -> [f32; 3] {
    [
        cg_vision_lerp_float(from[0], to[0], fraction, style),
        cg_vision_lerp_float(from[1], to[1], fraction, style),
        cg_vision_lerp_float(from[2], to[2], fraction, style),
    ]
}

#[must_use]
pub fn cg_vision_lerp_vars(
    from: VisionSetVars,
    to: VisionSetVars,
    fraction: f32,
    style: i32,
) -> VisionSetVars {
    VisionSetVars {
        r_glow: cg_vision_lerp_bool(from.r_glow, to.r_glow, fraction, style),
        r_glow_bloom_cutoff: cg_vision_lerp_float(
            from.r_glow_bloom_cutoff,
            to.r_glow_bloom_cutoff,
            fraction,
            style,
        ),
        r_glow_bloom_desaturation: cg_vision_lerp_float(
            from.r_glow_bloom_desaturation,
            to.r_glow_bloom_desaturation,
            fraction,
            style,
        ),
        r_glow_bloom_intensity0: cg_vision_lerp_float(
            from.r_glow_bloom_intensity0,
            to.r_glow_bloom_intensity0,
            fraction,
            style,
        ),
        r_glow_bloom_intensity1: cg_vision_lerp_float(
            from.r_glow_bloom_intensity1,
            to.r_glow_bloom_intensity1,
            fraction,
            style,
        ),
        r_glow_radius0: cg_vision_lerp_float(
            from.r_glow_radius0,
            to.r_glow_radius0,
            fraction,
            style,
        ),
        r_glow_radius1: cg_vision_lerp_float(
            from.r_glow_radius1,
            to.r_glow_radius1,
            fraction,
            style,
        ),
        r_glow_sky_bleed_intensity0: cg_vision_lerp_float(
            from.r_glow_sky_bleed_intensity0,
            to.r_glow_sky_bleed_intensity0,
            fraction,
            style,
        ),
        r_glow_sky_bleed_intensity1: cg_vision_lerp_float(
            from.r_glow_sky_bleed_intensity1,
            to.r_glow_sky_bleed_intensity1,
            fraction,
            style,
        ),
        r_film_enable: cg_vision_lerp_bool(from.r_film_enable, to.r_film_enable, fraction, style),
        r_film_brightness: cg_vision_lerp_float(
            from.r_film_brightness,
            to.r_film_brightness,
            fraction,
            style,
        ),
        r_film_contrast: cg_vision_lerp_float(
            from.r_film_contrast,
            to.r_film_contrast,
            fraction,
            style,
        ),
        r_film_desaturation: cg_vision_lerp_float(
            from.r_film_desaturation,
            to.r_film_desaturation,
            fraction,
            style,
        ),
        r_film_desaturation_dark: cg_vision_lerp_float(
            from.r_film_desaturation_dark,
            to.r_film_desaturation_dark,
            fraction,
            style,
        ),
        r_film_invert: cg_vision_lerp_bool(from.r_film_invert, to.r_film_invert, fraction, style),
        r_film_light_tint: cg_vision_lerp_vec3(
            from.r_film_light_tint,
            to.r_film_light_tint,
            fraction,
            style,
        ),
        r_film_medium_tint: cg_vision_lerp_vec3(
            from.r_film_medium_tint,
            to.r_film_medium_tint,
            fraction,
            style,
        ),
        r_film_dark_tint: cg_vision_lerp_vec3(
            from.r_film_dark_tint,
            to.r_film_dark_tint,
            fraction,
            style,
        ),
        r_primary_light_use_tweaks: cg_vision_lerp_bool(
            from.r_primary_light_use_tweaks,
            to.r_primary_light_use_tweaks,
            fraction,
            style,
        ),
        r_primary_light_tweak_diffuse: cg_vision_lerp_float(
            from.r_primary_light_tweak_diffuse,
            to.r_primary_light_tweak_diffuse,
            fraction,
            style,
        ),
        r_primary_light_tweak_specular: cg_vision_lerp_float(
            from.r_primary_light_tweak_specular,
            to.r_primary_light_tweak_specular,
            fraction,
            style,
        ),
        blend: from.blend,
    }
}

#[must_use]
pub fn cg_vision_hold_blend_rate(
    allowed: bool,
    script_forced: bool,
    intensity0: f32,
    cutoff: f32,
) -> f32 {
    if !allowed
        && !script_forced
        && intensity0 <= VISION_GLOW_FADE_INTENSITY_MAX
        && VISION_GLOW_FADE_CUTOFF_MIN <= cutoff
    {
        VISION_HOLD_BLEND_RATE_NEG
    } else {
        VISION_HOLD_BLEND_RATE
    }
}

fn apply_hold_blend(
    lerp: VisionSetLerpData,
    now: i32,
    rate: f32,
    mut result: VisionSetVars,
) -> (VisionSetVars, VisionSetLerpData) {
    let blend = com_clamp((now - lerp.last_time) as f32 * rate + lerp.blend, 0.0, 1.0);
    result.blend = blend;
    (
        result,
        VisionSetLerpData {
            last_time: now,
            blend,
            ..lerp
        },
    )
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn cg_vision_sets_update(
    now: i32,
    from: VisionSetVars,
    to: VisionSetVars,
    lerp: VisionSetLerpData,
    result: VisionSetVars,
    allowed: bool,
    script_forced: bool,
) -> (VisionSetVars, VisionSetLerpData) {
    if lerp.style == VISION_SET_LERP_NONE {
        return (result, lerp);
    }
    if lerp.style == VISION_SET_LERP_HOLD {
        let rate = cg_vision_hold_blend_rate(
            allowed,
            script_forced,
            result.r_glow_bloom_intensity0,
            result.r_glow_bloom_cutoff,
        );
        return apply_hold_blend(lerp, now, rate, result);
    }
    if lerp.time_start.wrapping_add(lerp.time_duration) < now {
        let snapped = if 1 < (lerp.style as u32).wrapping_sub(2) {
            from
        } else {
            to
        };
        let mut lerp = lerp;
        lerp.style = VISION_SET_LERP_HOLD;
        let mut snapped = snapped;
        snapped.blend = lerp.blend;
        return (snapped, lerp);
    }
    let duration = lerp.time_duration as f32;
    let frac = if duration == 0.0 {
        1.0
    } else {
        com_clamp((now - lerp.time_start) as f32 / duration, 0.0, 1.0)
    };
    let result = cg_vision_lerp_vars(from, to, frac, lerp.style);
    apply_hold_blend(lerp, now, VISION_HOLD_BLEND_RATE, result)
}

#[must_use]
pub fn cg_vision_set_start(
    now: i32,
    duration_ms: i32,
    requested_style: i32,
    current_style: i32,
    current: VisionSetVars,
    to: VisionSetVars,
) -> (VisionSetVars, VisionSetVars, VisionSetLerpData) {
    if duration_ms > 0 && current_style != VISION_SET_LERP_NONE {
        (
            current,
            to,
            VisionSetLerpData {
                time_start: now,
                time_duration: duration_ms,
                style: requested_style,
                blend: current.blend,
                last_time: now,
            },
        )
    } else {
        let mut to = to;
        to.blend = 1.0;
        (
            current,
            to,
            VisionSetLerpData {
                time_start: now,
                time_duration: 0,
                style: VISION_SET_LERP_HOLD,
                blend: 1.0,
                last_time: now,
            },
        )
    }
}
