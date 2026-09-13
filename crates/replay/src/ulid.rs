use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const ENCODING: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

static LAST: Mutex<Option<(u64, [u8; 10])>> = Mutex::new(None);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UlidError {
    ClockBeforeEpoch,
    Random(String),
}

impl core::fmt::Display for UlidError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ClockBeforeEpoch => write!(f, "system clock is before UNIX epoch"),
            Self::Random(e) => write!(f, "CSPRNG failed: {e}"),
        }
    }
}

impl std::error::Error for UlidError {}

pub fn new_ulid() -> Result<String, UlidError> {
    let ms = unix_ms()?;
    let mut random = [0u8; 10];
    getrandom::fill(&mut random).map_err(|e| UlidError::Random(e.to_string()))?;
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let (ms, random) = next_parts(ms, random, last.as_ref().copied());
    *last = Some((ms, random));
    Ok(encode(ms, random))
}

fn unix_ms() -> Result<u64, UlidError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .map_err(|_| UlidError::ClockBeforeEpoch)
}

fn next_parts(ms: u64, random: [u8; 10], last: Option<(u64, [u8; 10])>) -> (u64, [u8; 10]) {
    let Some((last_ms, last_rand)) = last else {
        return (ms.min((1u64 << 48) - 1), random);
    };
    if ms > last_ms {
        return (ms.min((1u64 << 48) - 1), random);
    }

    match increment_be(last_rand) {
        Some(next) => (last_ms, next),
        None => {
            let bumped = last_ms.saturating_add(1).min((1u64 << 48) - 1);
            (bumped, [0u8; 10])
        }
    }
}

fn increment_be(mut bytes: [u8; 10]) -> Option<[u8; 10]> {
    for i in (0..10).rev() {
        let (n, overflow) = bytes[i].overflowing_add(1);
        bytes[i] = n;
        if !overflow {
            return Some(bytes);
        }
    }
    None
}

pub fn encode(ms: u64, random: [u8; 10]) -> String {
    let ms = ms & ((1u64 << 48) - 1);
    let mut out = [0u8; 26];
    let mut time = ms;
    for i in (0..10).rev() {
        out[i] = ENCODING[(time % 32) as usize];
        time /= 32;
    }
    let mut acc = 0u128;
    for b in random {
        acc = (acc << 8) | u128::from(b);
    }
    for i in 0..16 {
        let shift = (15 - i) * 5;
        out[10 + i] = ENCODING[((acc >> shift) & 0x1f) as usize];
    }
    String::from_utf8(out.to_vec()).expect("Crockford alphabet is ASCII")
}
