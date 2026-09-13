use crate::quat::{Quat, Vec3, lerp_vec3, slerp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Dense,

    Sparse,
}

pub fn span(kind: FrameKind, frames: &[u16], count: usize, frame: f32) -> (usize, usize, f32) {
    if count <= 1 {
        return (0, 0, 0.0);
    }
    match kind {
        FrameKind::Dense => {
            let frame = frame.clamp(0.0, (count - 1) as f32);
            let first = libm::floorf(frame) as usize;
            (first, (first + 1).min(count - 1), frame - first as f32)
        }
        FrameKind::Sparse => {
            if frames.len() < count {
                return (0, 0, 0.0);
            }
            if frame <= frames[0] as f32 {
                return (0, 0, 0.0);
            }
            let last = count - 1;
            if frame >= frames[last] as f32 {
                return (last, last, 0.0);
            }
            let mut i = 0;
            while i < last {
                let start = frames[i] as f32;
                let end = frames[i + 1] as f32;
                if frame <= end {
                    let amount = if end > start {
                        (frame - start) / (end - start)
                    } else {
                        0.0
                    };
                    return (i, i + 1, amount);
                }
                i += 1;
            }
            (last, last, 0.0)
        }
    }
}

pub fn sample_quat(kind: FrameKind, frames: &[u16], values: &[Quat], frame: f32) -> Quat {
    if values.is_empty() {
        return crate::quat::QUAT_IDENTITY;
    }
    let (a, b, t) = span(kind, frames, values.len(), frame);
    slerp(values[a], values[b], t)
}

pub fn sample_vec3(kind: FrameKind, frames: &[u16], values: &[Vec3], frame: f32) -> Vec3 {
    if values.is_empty() {
        return crate::quat::VEC3_ZERO;
    }
    let (a, b, t) = span(kind, frames, values.len(), frame);
    lerp_vec3(values[a], values[b], t)
}

pub fn time_to_frame(time: f32, framerate: f32, numframes: u16, looping: bool) -> f32 {
    if framerate <= 0.0 {
        return 0.0;
    }
    let raw = time * framerate;
    if looping && numframes > 0 {
        let n = numframes as f32;
        raw - n * libm::floorf(raw / n)
    } else {
        raw.clamp(0.0, numframes as f32)
    }
}
