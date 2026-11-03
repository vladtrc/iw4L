#[derive(Clone, Copy, Debug)]
pub struct Span {
    begin: fn(),
    end: fn(),
}

impl Span {
    #[doc(hidden)]
    pub const fn new(begin: fn(), end: fn()) -> Self {
        Self { begin, end }
    }
    #[inline]
    pub fn begin(self) {
        (self.begin)();
    }
    #[inline]
    pub fn end(self) {
        (self.end)();
    }
    #[inline]
    pub fn enter(self) -> SpanGuard {
        self.begin();
        SpanGuard(self)
    }
}

#[must_use]
#[derive(Debug)]
pub struct SpanGuard(Span);
impl Drop for SpanGuard {
    #[inline]
    fn drop(&mut self) {
        self.0.end();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Counter(fn(f64));
impl Counter {
    #[doc(hidden)]
    pub const fn new(emit: fn(f64)) -> Self {
        Self(emit)
    }
    #[inline]
    pub fn emit(self, value: f64) {
        (self.0)(value);
    }
    #[inline]
    pub fn emit_at(self, value: f64, _frame: u64) {
        self.emit(value);
    }
}

#[cfg(all(unix, feature = "native"))]
#[doc(hidden)]
pub mod native {
    pub use perfetto_sdk::track_event::TrackEventTrack as Track;
    pub use perfetto_sdk::track_event::{EventContext as RawContext, TrackEventCounter};
    use perfetto_sdk::track_event::{EventContext, TrackEventDebugArg, TrackEventTrack};
    pub use perfetto_sdk::{
        track_event_begin, track_event_counter, track_event_end, track_event_instant,
    };
    pub use std::sync::OnceLock;

    perfetto_sdk::track_event_categories! {
        pub mod perfetto_te_ns {
            ("iw4l.frame", "IW4L frame events", ["iw4l"]),
            ("iw4l.fx", "IW4L FX events", ["iw4l"]),
            ("iw4l.render", "IW4L render events", ["iw4l"]),
            ("iw4l.sim", "IW4L simulation events", ["iw4l"]),
        }
    }

    pub fn register_categories() {
        let _ = perfetto_te_ns::register();
    }

    pub fn span_track(name: &'static str) -> TrackEventTrack {
        // The SDK incorporates the name into the track UUID.
        TrackEventTrack::register_named_track(name, 0, TrackEventTrack::process_track_uuid())
            .expect("register span track")
    }
    pub fn counter_track(name: &'static str) -> TrackEventTrack {
        TrackEventTrack::register_counter_track(name, TrackEventTrack::process_track_uuid())
            .expect("register counter track")
    }

    pub struct Context<'a>(&'a mut EventContext);
    impl<'a> Context<'a> {
        pub fn new(ctx: &'a mut EventContext) -> Self {
            Self(ctx)
        }
        pub fn arg(&mut self, key: &'static str, value: impl Argument) {
            value.record(self.0, key);
        }
    }
    pub trait Argument {
        fn record(self, ctx: &mut EventContext, key: &'static str);
    }
    impl Argument for &str {
        fn record(self, ctx: &mut EventContext, key: &'static str) {
            if self.contains('\0') {
                let value = self.replace('\0', "\\0");
                ctx.add_debug_arg(key, TrackEventDebugArg::String(&value));
            } else {
                ctx.add_debug_arg(key, TrackEventDebugArg::String(self));
            }
        }
    }
    impl Argument for &String {
        fn record(self, ctx: &mut EventContext, key: &'static str) {
            self.as_str().record(ctx, key);
        }
    }
    macro_rules! number {
        ($ty:ty, $variant:ident, $convert:expr) => {
            impl Argument for $ty {
                fn record(self, ctx: &mut EventContext, key: &'static str) {
                    ctx.add_debug_arg(key, TrackEventDebugArg::$variant(($convert)(self)));
                }
            }
        };
    }
    number!(u64, Uint64, |value| value);
    number!(u32, Uint64, u64::from);
    number!(i64, Int64, |value| value);
    number!(i32, Int64, i64::from);
    number!(f64, Double, |value| value);
    number!(f32, Double, f64::from);
    number!(bool, Bool, |value| value);
    impl<T: Argument> Argument for Option<T> {
        fn record(self, ctx: &mut EventContext, key: &'static str) {
            if let Some(value) = self {
                value.record(ctx, key);
            }
        }
    }
}

#[cfg(all(unix, feature = "native"))]
#[macro_export]
#[doc(hidden)]
macro_rules! span_handle {
    ($name:literal, $category:literal) => {{
        $crate::Span::new(
            || {
                use $crate::native::perfetto_te_ns;
                $crate::native::track_event_begin!(
                    $category,
                    $name,
                    |ctx: &mut $crate::native::RawContext| {
                        static TRACK: $crate::native::OnceLock<$crate::native::Track> =
                            $crate::native::OnceLock::new();
                        ctx.set_track(
                            TRACK.get_or_init(|| {
                                $crate::native::span_track(concat!("span.", $name))
                            }),
                        );
                    }
                );
            },
            || {
                use $crate::native::perfetto_te_ns;
                $crate::native::track_event_end!(
                    $category,
                    |ctx: &mut $crate::native::RawContext| {
                        static TRACK: $crate::native::OnceLock<$crate::native::Track> =
                            $crate::native::OnceLock::new();
                        ctx.set_track(
                            TRACK.get_or_init(|| {
                                $crate::native::span_track(concat!("span.", $name))
                            }),
                        );
                    }
                );
            },
        )
    }};
}
#[cfg(not(all(unix, feature = "native")))]
#[macro_export]
#[doc(hidden)]
macro_rules! span_handle {
    ($($args:tt)*) => {
        $crate::Span::new(|| $crate::noop!(), || $crate::noop!())
    };
}

#[macro_export]
macro_rules! span {
    ($name:literal, $category:literal) => {
        $crate::span_handle!($name, $category).enter()
    };
    ($name:literal, $category:literal, begin) => {
        $crate::span_handle!($name, $category).begin()
    };
    ($name:literal, $category:literal, end) => {
        $crate::span_handle!($name, $category).end()
    };
    ($name:literal, $category:literal, handle) => {
        $crate::span_handle!($name, $category)
    };
}

#[cfg(all(unix, feature = "native"))]
#[macro_export]
#[doc(hidden)]
macro_rules! counter_handle {
    ($name:literal, $category:literal) => {{
        $crate::Counter::new(|value| {
            use $crate::native::perfetto_te_ns;
            $crate::native::track_event_counter!(
                $category,
                |ctx: &mut $crate::native::RawContext| {
                    static TRACK: $crate::native::OnceLock<$crate::native::Track> =
                        $crate::native::OnceLock::new();
                    ctx.set_track(TRACK.get_or_init(|| $crate::native::counter_track($name)));
                    ctx.set_counter($crate::native::TrackEventCounter::Double(value));
                }
            );
        })
    }};
}
#[cfg(not(all(unix, feature = "native")))]
#[macro_export]
#[doc(hidden)]
macro_rules! counter_handle {
    ($($args:tt)*) => {
        $crate::Counter::new(|_| $crate::noop!())
    };
}

#[macro_export]
macro_rules! counter {
    ($name:literal, $category:literal, handle) => {
        $crate::counter_handle!($name, $category)
    };
    ($name:literal, $category:literal, $value:expr) => {
        $crate::counter_handle!($name, $category).emit($value as f64)
    };
}

#[cfg(all(unix, feature = "native"))]
#[macro_export]
macro_rules! instant {
    ($name:literal, $category:literal, |$ctx:ident| $body:block) => {{
        use $crate::native::perfetto_te_ns;
        $crate::native::track_event_instant!($category, $name, |raw: &mut $crate::native::RawContext| {
            let $ctx = &mut $crate::native::Context::new(raw);
            let _ = &$ctx;
            $body
        });
    }};
    ($name:literal, $category:literal $(, $key:literal => $value:expr)* $(,)?) => {
        $crate::instant!($name, $category, |ctx| { $(ctx.arg($key, $value);)* })
    };
}
#[cfg(not(all(unix, feature = "native")))]
#[macro_export]
macro_rules! instant {
    ($name:literal, $category:literal, |$ctx:ident| $body:block) => {{
        // Keep disabled event bodies checked without evaluating trace inputs.
        if false {
            struct Context;
            impl Context {
                fn arg<T>(&mut self, _key: &'static str, _value: T) {}
            }
            let $ctx = &mut Context;
            let _ = &$ctx;
            $body
        }
    }};
    ($name:literal, $category:literal $(, $key:literal => $value:expr)* $(,)?) => {
        $crate::instant!($name, $category, |ctx| { $(ctx.arg($key, $value);)* })
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! noop {
    ($($args:tt)*) => {{}};
}
