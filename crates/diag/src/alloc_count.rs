use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
};

use serde::Serialize;

const FALLBACK_SLOT: usize = 0;
const MAX_THREADS: usize = 256;

#[repr(C, align(64))]
struct ThreadCounters {
    alloc_count: AtomicU64,
    alloc_bytes: AtomicU64,
    dealloc_count: AtomicU64,
    dealloc_bytes: AtomicU64,
}

impl ThreadCounters {
    const fn new() -> Self {
        Self {
            alloc_count: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            dealloc_count: AtomicU64::new(0),
            dealloc_bytes: AtomicU64::new(0),
        }
    }
}

static SLOTS: [ThreadCounters; MAX_THREADS] = [const { ThreadCounters::new() }; MAX_THREADS];

static NEXT_SLOT: AtomicUsize = AtomicUsize::new(1);

static SATURATED: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static SLOT: Cell<usize> = const { Cell::new(usize::MAX) };
}

fn slot() -> &'static ThreadCounters {
    let idx = SLOT
        .try_with(|cell| {
            let mut idx = cell.get();
            if idx == usize::MAX {
                idx = NEXT_SLOT.fetch_add(1, Ordering::Relaxed);
                if idx >= MAX_THREADS {
                    SATURATED.fetch_add(1, Ordering::Relaxed);
                    idx = MAX_THREADS - 1;
                    NEXT_SLOT.store(MAX_THREADS, Ordering::Relaxed);
                }
                cell.set(idx);
            }
            idx
        })
        .unwrap_or(FALLBACK_SLOT);
    &SLOTS[idx]
}

fn record_alloc(size: u64) {
    if !counting_enabled() {
        return;
    }
    let s = slot();
    s.alloc_count.fetch_add(1, Ordering::Relaxed);
    s.alloc_bytes.fetch_add(size, Ordering::Relaxed);
}

fn record_dealloc(size: u64) {
    if !counting_enabled() {
        return;
    }
    let s = slot();
    s.dealloc_count.fetch_add(1, Ordering::Relaxed);
    s.dealloc_bytes.fetch_add(size, Ordering::Relaxed);
}

fn counting_enabled() -> bool {
    static STATE: AtomicU8 = AtomicU8::new(2);
    match STATE.load(Ordering::Relaxed) {
        0 => false,
        1 => true,
        _ => {
            let on = env_key_present(b"IW4L_COUNTING_ALLOC\0");
            STATE.store(u8::from(on), Ordering::Relaxed);
            on
        }
    }
}

fn env_key_present(key_nul: &[u8]) -> bool {
    debug_assert_eq!(key_nul.last().copied(), Some(0));
    !unsafe { getenv(key_nul.as_ptr().cast()) }.is_null()
}

unsafe extern "C" {
    fn getenv(name: *const core::ffi::c_char) -> *mut core::ffi::c_char;
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    fn malloc_trim(pad: usize) -> core::ffi::c_int;
}

pub fn release_freed_heap() -> std::time::Duration {
    let at = std::time::Instant::now();
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe { malloc_trim(0) };
    }
    at.elapsed()
}

pub struct ProcessCountingAllocator;

unsafe impl GlobalAlloc for ProcessCountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_alloc(layout.size() as u64);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_alloc(layout.size() as u64);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record_dealloc(layout.size() as u64);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_alloc(new_size as u64);
        record_dealloc(layout.size() as u64);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub struct ProcessAllocationStats {
    pub process_allocations: u64,
    pub process_allocation_bytes: u64,
    pub process_deallocations: u64,
    pub process_deallocation_bytes: u64,

    pub process_allocation_saturated: u64,
}

pub fn process_allocations() -> ProcessAllocationStats {
    let n = NEXT_SLOT.load(Ordering::Relaxed).clamp(1, MAX_THREADS);
    let mut stats = ProcessAllocationStats::default();
    for slot in &SLOTS[..n] {
        stats.process_allocations += slot.alloc_count.load(Ordering::Relaxed);
        stats.process_allocation_bytes += slot.alloc_bytes.load(Ordering::Relaxed);
        stats.process_deallocations += slot.dealloc_count.load(Ordering::Relaxed);
        stats.process_deallocation_bytes += slot.dealloc_bytes.load(Ordering::Relaxed);
    }
    stats.process_allocation_saturated = SATURATED.load(Ordering::Relaxed);
    stats
}

pub fn process_live_heap_bytes() -> Option<u64> {
    if !counting_enabled() {
        return None;
    }
    let stats = process_allocations();
    Some(
        stats
            .process_allocation_bytes
            .saturating_sub(stats.process_deallocation_bytes),
    )
}

pub fn process_allocation_slots_used() -> usize {
    NEXT_SLOT.load(Ordering::Relaxed).clamp(1, MAX_THREADS)
}

pub fn process_allocation_saturated() -> u64 {
    SATURATED.load(Ordering::Relaxed)
}
