use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static INDEX: AtomicU64 = AtomicU64::new(0);
static OPEN: AtomicBool = AtomicBool::new(false);
static SCREENSHOT: AtomicBool = AtomicBool::new(false);

pub fn open_index() -> u64 {
    INDEX.load(Ordering::Relaxed)
}

pub fn advance() {
    close();
    INDEX.fetch_add(1, Ordering::Relaxed);
    crate::span!("wall", "iw4l.frame", begin);
    OPEN.store(true, Ordering::Relaxed);
}

pub(crate) fn close() {
    if OPEN.swap(false, Ordering::Relaxed) {
        crate::span!("wall", "iw4l.frame", end);
    }
}

pub fn screenshot(active: bool) {
    if SCREENSHOT.swap(active, Ordering::Relaxed) != active {
        crate::instant!("screenshot_state", "iw4l.frame", "active" => active as u64);
    }
}

pub fn set_replay_tick(_tick: u64) {
    crate::instant!("replay_tick", "iw4l.frame", "tick" => _tick);
}
