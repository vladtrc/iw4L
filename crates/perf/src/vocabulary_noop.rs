pub use crate::vocabulary_types::Span;

impl Span {
    pub fn begin(self) {
        crate::stats::begin(self);
    }

    pub fn end(self) {
        crate::stats::end(self);
    }

    pub fn enter(self) -> SpanGuard {
        SpanGuard
    }
}

#[must_use = "the span closes when this guard drops"]
pub struct SpanGuard;

pub use crate::vocabulary_types::Counter;

impl Counter {
    pub fn emit(self, _value: f64) {}
}
