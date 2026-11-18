use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DObjReuseKey {
    pub e_type: i32,
    pub model: i32,
}

pub fn reuse_matches(cached: DObjReuseKey, current: DObjReuseKey) -> bool {
    cached.e_type == current.e_type && cached.model == current.model
}

pub fn model_token(parts: &[&str]) -> i32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
    }
    (hasher.finish() & 0x7fff_ffff) as i32
}
