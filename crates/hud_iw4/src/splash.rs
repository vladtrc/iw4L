extern crate alloc;

use alloc::string::String;

pub const SPLASH_SLOT_COUNT: usize = 5;

pub const SPLASH_TABLE_NAME: &str = "mp/splashTable.csv";

pub const SPLASH_COL_TEXT: i32 = 1;

pub const SPLASH_COL_DESCRIPTION: i32 = 2;

pub const SPLASH_COL_MATERIAL: i32 = 3;

pub const SPLASH_COL_DURATION: i32 = 4;

pub const SPLASH_COL_SOUND: i32 = 9;

pub const SPLASH_COL_MENU: i32 = 0xb;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SplashSlot {
    pub row: i32,

    pub start_ms: i32,

    pub duration_ms: i32,

    pub optional_number: i32,
}

impl SplashSlot {
    #[must_use]
    pub fn live(&self) -> bool {
        self.start_ms != 0
    }

    #[must_use]
    pub fn expired(&self, now_ms: i32) -> bool {
        self.live() && self.start_ms.saturating_add(self.duration_ms) < now_ms
    }
}

#[must_use]
pub fn splash_duration_ms(cell: &str) -> i32 {
    if cell.is_empty() {
        return 0;
    }
    let seconds: f32 = cell.parse().unwrap_or(0.0);
    (seconds * 1000.0) as i32
}

#[must_use]
pub fn cg_activate_splash(
    slot: i32,
    row: i32,
    duration_ms: i32,
    optional_number: i32,
    now_ms: i32,
) -> (usize, SplashSlot) {
    let index = if !(0..=4).contains(&slot) {
        0
    } else {
        slot as usize
    };
    (
        index,
        SplashSlot {
            row,
            start_ms: now_ms,
            duration_ms,
            optional_number,
        },
    )
}

#[must_use]
pub fn splash_replace_optional(template: &str, optional_number: i32) -> String {
    if !template.contains("&&") {
        return String::from(template);
    }
    let mut digits = [0u8; 16];
    let n = int_to_dec(optional_number, &mut digits);
    let num = core::str::from_utf8(&digits[..n]).unwrap_or("0");
    replace_and_and_one(template, num)
}

fn int_to_dec(n: i32, buf: &mut [u8; 16]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    let neg = n < 0;
    let mut x = n.unsigned_abs();
    let mut tmp = [0u8; 16];
    let mut i = tmp.len();
    while x > 0 {
        i -= 1;
        tmp[i] = b'0' + (x % 10) as u8;
        x /= 10;
    }
    if neg {
        i -= 1;
        tmp[i] = b'-';
    }
    let n = tmp.len() - i;
    buf[..n].copy_from_slice(&tmp[i..]);
    n
}

fn replace_and_and_one(template: &str, num: &str) -> String {
    let bytes = template.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 2 < bytes.len() && bytes[i] == b'&' && bytes[i + 1] == b'&' && bytes[i + 2] == b'1' {
            out.push_str(num);
            i += 3;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

#[must_use]
pub fn splash_has_icon(material: &str) -> bool {
    material.as_bytes().first().copied().unwrap_or(0) > 0
}
