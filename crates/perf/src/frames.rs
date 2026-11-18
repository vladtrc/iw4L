use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static INDEX: AtomicU64 = AtomicU64::new(0);
static OPEN: AtomicBool = AtomicBool::new(false);
static SCREENSHOT: AtomicBool = AtomicBool::new(false);

pub fn open_index() -> u64 {
    INDEX.load(Ordering::Relaxed)
}

pub fn advance() {
    let boundary = crate::timing::advance_frame(open_index());
    close_at(boundary);
    let frame = INDEX.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    #[cfg(all(unix, feature = "native"))]
    if let Some(timestamp) = boundary.trace {
        use crate::native::perfetto_te_ns;
        crate::native::track_event_begin!(
            "iw4l.frame",
            "wall",
            |ctx: &mut crate::native::RawContext| {
                ctx.set_track(wall_track());
                ctx.set_timestamp(timestamp);
                ctx.add_debug_arg(
                    "frame_index",
                    perfetto_sdk::track_event::TrackEventDebugArg::Uint64(frame),
                );
            }
        );
        OPEN.store(true, Ordering::Relaxed);
    }
    #[cfg(not(all(unix, feature = "native")))]
    let _ = frame;
}

#[cfg(all(unix, feature = "native"))]
pub(crate) fn close() {
    close_at(crate::timing::FrameBoundary::now());
}

fn close_at(_boundary: crate::timing::FrameBoundary) {
    if OPEN.swap(false, Ordering::Relaxed) {
        #[cfg(all(unix, feature = "native"))]
        if let Some(timestamp) = _boundary.trace {
            use crate::native::perfetto_te_ns;
            crate::native::track_event_end!("iw4l.frame", |ctx: &mut crate::native::RawContext| {
                ctx.set_track(wall_track());
                ctx.set_timestamp(timestamp);
            });
        }
    }
}

#[cfg(all(unix, feature = "native"))]
fn wall_track() -> &'static crate::native::Track {
    static TRACK: crate::native::OnceLock<crate::native::Track> = crate::native::OnceLock::new();
    TRACK.get_or_init(|| crate::native::span_track("span.wall"))
}

pub fn screenshot(active: bool) {
    if SCREENSHOT.swap(active, Ordering::Relaxed) != active {
        crate::instant!("screenshot_state", "iw4l.frame", "active" => active as u64);
    }
}

pub fn set_replay_tick(_tick: u64) {
    crate::instant!("replay_tick", "iw4l.frame", "tick" => _tick);
}
