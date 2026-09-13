use trace_iw4::Trace;

use crate::{CollisionBackend, GroundTraceInput};

pub const BG_CORRECT_SOLID_DELTAS: [[f32; 3]; 26] = [
    [0.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [0.0, -1.0, 1.0],
    [1.0, 0.0, 1.0],
    [0.0, 1.0, 1.0],
    [-1.0, 0.0, 0.0],
    [0.0, -1.0, 0.0],
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, -1.0, -1.0],
    [1.0, 0.0, -1.0],
    [0.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, 1.0],
    [-1.0, -1.0, 0.0],
    [1.0, -1.0, 0.0],
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
];

const SPECIAL_GROUND_PROBE_DEPTH: f32 = 0.0;

const CORRECT_SOLID_DOWN_STEP: f32 = 0.25;

#[derive(Clone, Debug, PartialEq)]
pub struct CorrectSolidOutcome {
    pub origin: [f32; 3],
    pub trace: Trace,
}

pub fn pm_correct_solid<C: CollisionBackend + ?Sized>(
    origin: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    tracemask: u32,
    collision: &C,
) -> Option<CorrectSolidOutcome> {
    let down_delta = SPECIAL_GROUND_PROBE_DEPTH + CORRECT_SOLID_DOWN_STEP;
    for delta in BG_CORRECT_SOLID_DELTAS {
        let point = [
            origin[0] + delta[0],
            origin[1] + delta[1],
            origin[2] + delta[2],
        ];
        let at_point = collision.trace(GroundTraceInput {
            start: point,
            end: point,
            mins,
            maxs,
            tracemask,
        });
        if at_point.startsolid != 0 {
            continue;
        }

        let end = [point[0], point[1], point[2] - down_delta];
        let down = collision.trace(GroundTraceInput {
            start: point,
            end,
            mins,
            maxs,
            tracemask,
        });
        if down.startsolid != 0 {
            continue;
        }

        let new_origin = [
            point[0],
            point[1],
            point[2] + (end[2] - point[2]) * down.fraction,
        ];
        return Some(CorrectSolidOutcome {
            origin: new_origin,
            trace: down,
        });
    }

    None
}
