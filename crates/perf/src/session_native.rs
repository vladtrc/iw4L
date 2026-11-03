use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Mutex;
use std::time::Duration;

use perfetto_sdk_sys::{
    PerfettoTracingSessionDestroy, PerfettoTracingSessionFlushBlocking, PerfettoTracingSessionImpl,
    PerfettoTracingSessionInProcessCreate, PerfettoTracingSessionReadTraceBlocking,
    PerfettoTracingSessionSetup, PerfettoTracingSessionStartBlocking,
    PerfettoTracingSessionStopBlocking,
};

pub(super) struct RecordingSession(NonNull<PerfettoTracingSessionImpl>);

// Perfetto permits session calls and destruction from application threads.
// The session owner serializes access; blocking calls never run on its service thread.
unsafe impl Send for RecordingSession {}

impl RecordingSession {
    pub(super) fn start(config: &[u8]) -> Result<Self, String> {
        // The C constructor transfers ownership of a nullable opaque handle.
        let handle = NonNull::new(unsafe { PerfettoTracingSessionInProcessCreate() })
            .ok_or("create Perfetto tracing session")?;
        let session = Self(handle);
        // Setup parses the borrowed protobuf before returning and does not mutate it.
        unsafe {
            PerfettoTracingSessionSetup(
                session.0.as_ptr(),
                config.as_ptr().cast_mut().cast(),
                config.len(),
            );
            PerfettoTracingSessionStartBlocking(session.0.as_ptr());
        }
        Ok(session)
    }

    pub(super) fn capture(&mut self, timeout: Duration) -> Result<Vec<u8>, String> {
        let timeout_ms = u32::try_from(timeout.as_millis())
            .map_err(|_| "Perfetto flush timeout exceeds u32 milliseconds")?;
        // This live handle is exclusively borrowed; calls run on an application thread.
        if !unsafe { PerfettoTracingSessionFlushBlocking(self.0.as_ptr(), timeout_ms) } {
            return Err("Perfetto flush failed or timed out".into());
        }
        let sink = Mutex::new(ReadSink {
            bytes: Vec::new(),
            error: None,
        });
        // ReadTraceBlocking waits for the final callback, so the stack sink outlives
        // every callback. The mutex synchronizes access from the service thread.
        unsafe {
            PerfettoTracingSessionStopBlocking(self.0.as_ptr());
            PerfettoTracingSessionReadTraceBlocking(
                self.0.as_ptr(),
                Some(read_chunk),
                (&raw const sink).cast_mut().cast(),
            );
        }
        let sink = sink
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(error) = sink.error {
            return Err(error.into());
        }
        Ok(sink.bytes)
    }
}

impl Drop for RecordingSession {
    fn drop(&mut self) {
        // The constructor transferred this handle; no callback can outlive capture.
        unsafe { PerfettoTracingSessionDestroy(self.0.as_ptr()) };
    }
}

struct ReadSink {
    bytes: Vec<u8>,
    error: Option<&'static str>,
}

unsafe extern "C" fn read_chunk(
    _session: *mut PerfettoTracingSessionImpl,
    data: *const c_void,
    size: usize,
    _has_more: bool,
    user_arg: *mut c_void,
) {
    // capture passes this mutex and keeps it alive until ReadTraceBlocking returns.
    let sink = unsafe { &*user_arg.cast::<Mutex<ReadSink>>() };
    let mut sink = sink
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if size == 0 || sink.error.is_some() {
        return;
    }
    if data.is_null() || size > isize::MAX as usize {
        sink.error = Some("Perfetto returned an invalid trace chunk");
        return;
    }
    if sink.bytes.try_reserve(size).is_err() {
        sink.error = Some("cannot reserve memory for Perfetto trace");
        return;
    }
    // Perfetto lends size initialized bytes for this callback; the copy does not
    // retain its pointer. Empty chunks are handled without constructing a slice.
    let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size) };
    sink.bytes.extend_from_slice(bytes);
}
