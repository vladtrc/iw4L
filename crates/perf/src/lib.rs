pub mod frames;
mod instrumentation;
pub mod run;
#[cfg(all(unix, feature = "native"))]
mod session;
#[cfg(not(all(unix, feature = "native")))]
#[path = "session_noop.rs"]
mod session;
#[cfg(all(unix, feature = "native"))]
mod session_native;
mod switch;

#[cfg(all(unix, feature = "native"))]
pub use instrumentation::native;
pub use instrumentation::{Counter, Span, SpanGuard};
pub use session::{RunMetadata, enabled, flush, start};
pub use switch::on as switch;

#[inline]
pub fn recording() -> bool {
    enabled()
}
