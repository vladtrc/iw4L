pub const TRIGGER_RADIUS: &str = "trigger_radius";

pub const TRIGGER_USE_TOUCH: &str = "trigger_use_touch";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateUseTriggerKind {
    Use,
    Proximity,
}

pub const fn create_use_trigger_kind(classname: &str) -> CreateUseTriggerKind {
    if classname_contains_use(classname) {
        CreateUseTriggerKind::Use
    } else {
        CreateUseTriggerKind::Proximity
    }
}

const fn classname_contains_use(classname: &str) -> bool {
    let bytes = classname.as_bytes();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if bytes[i] == b'u' && bytes[i + 1] == b's' && bytes[i + 2] == b'e' {
            return true;
        }
        i += 1;
    }
    false
}

pub fn set_use_time_ms(time_seconds: f32) -> i32 {
    (time_seconds * 1000.0) as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerRadiusError {
    MissingRadius,
    MissingHeight,
}

pub fn trigger_radius_box(
    radius: Option<f32>,
    height: Option<f32>,
) -> Result<([f32; 3], [f32; 3]), TriggerRadiusError> {
    let radius = radius.ok_or(TriggerRadiusError::MissingRadius)?;
    let height = height.ok_or(TriggerRadiusError::MissingHeight)?;
    Ok(([0.0, 0.0, height * 0.5], [radius, radius, height * 0.5]))
}

pub fn world_aabb_from_link_bounds(mid: [f32; 3], half: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    (
        [mid[0] - half[0], mid[1] - half[1], mid[2] - half[2]],
        [mid[0] + half[0], mid[1] + half[1], mid[2] + half[2]],
    )
}

pub fn cylinder_contact(
    mid: [f32; 3],
    half: [f32; 3],
    origin: [f32; 3],
    radius: f32,
    height: f32,
) -> bool {
    let dx = origin[0] - mid[0];
    let dy = origin[1] - mid[1];
    let r = radius + half[0];
    (origin[2] + height * 0.5 - mid[2]).abs() < height * 0.5 + half[2] && dx * dx + dy * dy < r * r
}

pub fn capsule_trigger_hull_contact(
    mid: [f32; 3],
    half: [f32; 3],
    hull_mid: [f32; 3],
    hull_half: [f32; 3],
    slabs: &[([f32; 3], f32, f32)],
) -> bool {
    (0..3).all(|i| (mid[i] - hull_mid[i]).abs() < half[i] + hull_half[i])
        && slabs.iter().all(|(dir, center, extent)| {
            let distance = mid[0] * dir[0] + mid[1] * dir[1] + mid[2] * dir[2];
            let support = dir[2].abs() * (half[2] - half[0]) + half[0];
            (distance - center).abs() < extent + support
        })
}
