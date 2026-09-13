extern crate alloc;

use alloc::string::String;

pub const CG_CENTERTIME_DEFAULT_MS: i32 = 5000;

pub const CG_CENTERPRINT_FADE_TAIL_MS: i32 = 100;

pub const CENTERPRINT_STRIDE: usize = 0x408;

#[must_use]
pub fn cg_priority_center_print_accepts(time: i32, slot_priority: i32, new_priority: i32) -> bool {
    time == 0 || new_priority >= slot_priority
}

#[must_use]
pub fn cg_fade_color(now: i32, start: i32, duration: i32, fade_tail: i32) -> Option<f32> {
    if start == 0 {
        return None;
    }
    let elapsed = now - start;
    if elapsed >= duration {
        return None;
    }
    let remaining = duration - elapsed;
    if fade_tail < 1 || remaining >= fade_tail {
        Some(1.0)
    } else {
        Some(remaining as f32 / fade_tail as f32)
    }
}

#[must_use]
pub fn centerprint_replace_name(template: &str, name: &str) -> String {
    if !template.contains("&&") {
        return String::from(template);
    }
    replace_and_and_one(template, name)
}

fn replace_and_and_one(template: &str, value: &str) -> String {
    let bytes = template.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 2 < bytes.len() && bytes[i] == b'&' && bytes[i + 1] == b'&' && bytes[i + 2] == b'1' {
            out.push_str(value);
            i += 3;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}
