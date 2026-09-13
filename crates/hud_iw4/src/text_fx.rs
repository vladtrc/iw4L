pub const TEXT_RENDERFLAG_DROP_SHADOW: u32 = 4;

pub const TEXT_RENDERFLAG_BIG_SHADOW: u32 = 8;

pub const TEXT_RENDERFLAG_FX_DECODE: u32 = 0x40;

pub const TEXT_RENDERFLAG_PADDING: u32 = 0x80;

pub const FX_DECODE_RENDERFLAGS: u32 = TEXT_RENDERFLAG_FX_DECODE | TEXT_RENDERFLAG_PADDING;

pub const DECODE_CHARACTERS_MATERIAL: &str = "decode_characters";

pub const DECODE_CHARACTERS_GLOW_MATERIAL: &str = "decode_characters_glow";

pub const FX_RANDOM_CHARS: &[u8; 62] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz1234567890";

pub const FX_EXTRA_CHAR_LETTER: u32 = 0x4f;

pub const FX_EXTRA_CHAR_ATLAS_STEP: f32 = 0.0625;

pub const FX_TYPING_LETTER_ALPHA: u8 = 0xc0;

pub const FX_DECAY_TICKS_PER_SECOND: f64 = 30.0;

pub const FX_DECAY_LETTER_FADE_MS: i32 = 60;

pub const SNAP_FLOAT_TO_INT_BIAS: f64 = 9.313_225_746_154_785e-10;

pub fn rand_with_seed(seed: &mut i32) -> i32 {
    let v = seed.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
    *seed = v;
    (v.wrapping_add((v >> 31) & 0xffff) >> 16) & 0x7fff
}

#[must_use]
pub fn r_font_get_random_letter(seed: i32) -> u32 {
    let mut seed = seed;
    let i = rand_with_seed(&mut seed).rem_euclid(FX_RANDOM_CHARS.len() as i32) as usize;
    u32::from(FX_RANDOM_CHARS[i])
}

#[must_use]
pub fn seh_print_strlen(text: &str) -> i32 {
    let mut n = 0i32;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' {
            match chars.peek().copied() {
                Some(next) if next != '^' && next.is_ascii_digit() => {
                    chars.next();
                }
                _ => n += 1,
            }
            continue;
        }
        if c != '\n' && c != '\r' {
            n += 1;
        }
    }
    n
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPulseFx {
    pub birth_time: i32,

    pub letter_time: i32,

    pub decay_start_time: i32,

    pub decay_duration: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PulseFxVars {
    pub draw_rand_char_at_end: bool,

    pub rand_seed: i32,

    pub max_length: i32,

    pub decaying: bool,

    pub decay_time_elapsed: i32,
}

#[must_use]
pub fn setup_pulse_fx_vars(
    printed_len: i32,
    max_length: i32,
    scene_time: i32,
    fx: TextPulseFx,
) -> Option<PulseFxVars> {
    if scene_time <= 0 {
        return None;
    }
    let elapsed = scene_time - fx.birth_time;
    if elapsed > fx.decay_start_time.saturating_add(fx.decay_duration) {
        return None;
    }
    let str_length = printed_len.min(max_length);
    let mut vars = PulseFxVars {
        draw_rand_char_at_end: false,
        rand_seed: 1,
        max_length,
        decaying: false,
        decay_time_elapsed: 0,
    };

    if fx.letter_time != 0 && elapsed < fx.letter_time.saturating_mul(str_length) {
        let born = elapsed / fx.letter_time;
        let mut remainder = elapsed % fx.letter_time;
        let quarter = fx.letter_time / 4;
        if quarter != 0 {
            remainder /= quarter;
        }
        let mut seed = remainder
            .wrapping_add(str_length)
            .wrapping_add(born)
            .wrapping_add(fx.birth_time);
        rand_with_seed(&mut seed);
        rand_with_seed(&mut seed);
        vars.draw_rand_char_at_end = true;
        vars.rand_seed = seed;
        vars.max_length = born.saturating_add(1);
    } else if elapsed > fx.decay_start_time {
        let mut seed = str_length.wrapping_add(fx.birth_time);
        rand_with_seed(&mut seed);
        rand_with_seed(&mut seed);
        vars.decaying = true;
        vars.rand_seed = seed;
        vars.decay_time_elapsed = elapsed - fx.decay_start_time;
    }
    Some(vars)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecayingLetter {
    pub skip_drawing: bool,

    pub alpha: u8,

    pub letter: u32,

    pub draw_extra_fx_char: bool,
}

#[must_use]
pub fn fx_decay_tick_count(fx_decay_duration: i32) -> Option<i32> {
    let ratio = (f64::from(fx_decay_duration) / 1000.0) as f32;
    let ticks = (f64::from(ratio) * FX_DECAY_TICKS_PER_SECOND) as i32;
    (ticks != 0).then_some(ticks)
}

#[must_use]
pub fn get_decaying_letter_info(
    letter: u32,
    seed: &mut i32,
    decay_time_elapsed: i32,
    fx_birth_time: i32,
    fx_decay_duration: i32,
    alpha: u8,
) -> Option<DecayingLetter> {
    let tick_count = fx_decay_tick_count(fx_decay_duration)?;
    let tick = rand_with_seed(seed) % tick_count;
    let time_limit = tick.saturating_mul(fx_decay_duration / tick_count);
    let mut out = DecayingLetter {
        skip_drawing: false,
        alpha: 255,
        letter,
        draw_extra_fx_char: false,
    };
    if decay_time_elapsed >= time_limit {
        out.skip_drawing = true;
        return Some(out);
    }
    if decay_time_elapsed + FX_DECAY_LETTER_FADE_MS < time_limit {
        return Some(out);
    }
    let mut scramble = decay_time_elapsed
        .wrapping_add(letter as i32)
        .wrapping_add(fx_birth_time);
    if rand_with_seed(&mut scramble) % 2 != 0 {
        out.draw_extra_fx_char = true;
        out.letter = FX_EXTRA_CHAR_LETTER;
    } else {
        out.letter = r_font_get_random_letter(scramble);
    }
    let gone = (decay_time_elapsed + FX_DECAY_LETTER_FADE_MS - time_limit) as f32
        / FX_DECAY_LETTER_FADE_MS as f32;
    let fade = (1.0 - gone) * (f32::from(alpha) / 255.0);
    let snapped = libm::round(f64::from(fade) * 255.0 + SNAP_FLOAT_TO_INT_BIAS) as i32;
    out.alpha = snapped.clamp(0, 255) as u8;
    Some(out)
}

#[must_use]
pub fn modulate_byte_colors(a: u8, b: u8) -> u8 {
    ((f32::from(a) / 255.0) * (f32::from(b) / 255.0) * 255.0) as u8
}

#[must_use]
pub fn decode_fx_char_st(char_index: i32) -> (f32, f32) {
    let column = char_index.rem_euclid(16) as f32;
    let s0 = column * FX_EXTRA_CHAR_ATLAS_STEP;
    (s0, s0 + FX_EXTRA_CHAR_ATLAS_STEP)
}

#[must_use]
pub fn text_drop_shadow_offset(render_flags: u32) -> Option<f32> {
    if render_flags & TEXT_RENDERFLAG_DROP_SHADOW == 0 {
        return None;
    }
    Some(if render_flags & TEXT_RENDERFLAG_BIG_SHADOW == 0 {
        1.0
    } else {
        2.0
    })
}

pub const HUDELEM_SOUND_SLOTS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextPulseSound {
    Type,

    Delete,
}

impl TextPulseSound {
    #[must_use]
    pub const fn alias(self) -> &'static str {
        match self {
            TextPulseSound::Type => "ui_pulse_text_type",
            TextPulseSound::Delete => "ui_pulse_text_delete",
        }
    }
}

#[must_use]
pub fn cl_play_text_fx_pulse_sounds(
    current_time: i32,
    str_length: i32,
    fx_birth_time: i32,
    fx_letter_time: i32,
    fx_decay_start_time: i32,
    last_played_time: &mut i32,
) -> Option<TextPulseSound> {
    if fx_letter_time == 0 {
        return None;
    }
    let time_elapsed = current_time.wrapping_sub(fx_birth_time);
    if time_elapsed < 0 {
        return None;
    }
    let last_sound_time = last_played_time.wrapping_sub(fx_birth_time);
    let typing_duration = fx_letter_time.saturating_mul(str_length);
    let decay_start_time = fx_decay_start_time.max(typing_duration);
    let sound = if time_elapsed > decay_start_time {
        (last_sound_time < decay_start_time).then_some(TextPulseSound::Delete)
    } else if time_elapsed < typing_duration
        && last_sound_time < fx_letter_time * (time_elapsed / fx_letter_time)
    {
        Some(TextPulseSound::Type)
    } else {
        None
    };
    if sound.is_some() {
        *last_played_time = current_time;
    }
    sound
}
