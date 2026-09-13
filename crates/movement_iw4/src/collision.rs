use trace_iw4::Trace;

use crate::GroundTraceInput;
use crate::correct_solid::{CorrectSolidOutcome, pm_correct_solid};

pub trait CollisionBackend {
    fn trace(&self, input: GroundTraceInput) -> Trace;

    fn correct_solid(
        &self,
        origin: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        tracemask: u32,
    ) -> Option<CorrectSolidOutcome> {
        pm_correct_solid(origin, mins, maxs, tracemask, self)
    }

    fn touch_entity(&self, _entity: i32) {}
}
