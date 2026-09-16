pub use crate::vocabulary_types::Span;

impl Span {
    pub fn begin(self) {
        crate::stats::begin(self);
    }

    pub fn end(self) {
        crate::stats::end(self);
    }

    pub fn enter(self) -> SpanGuard {
        self.begin();
        SpanGuard { span: self }
    }
}

#[must_use = "the span closes when this guard drops"]
pub struct SpanGuard {
    span: Span,
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        self.span.end();
    }
}

pub use crate::vocabulary_types::Counter;

impl Counter {
    pub fn emit(self, value: f64) {
        crate::stats::count(self, value);
    }

    pub fn emit_at(self, value: f64, frame: u64) {
        crate::stats::count_at(self, value, frame);
    }
}
