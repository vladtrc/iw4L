use playerstate_iw4::PlayerState;

use crate::Pml;

#[allow(clippy::assign_op_pattern)]
pub fn pm_predict_integrate(ps: &mut PlayerState, pml: &Pml) {
    integrate_origin(&mut ps.origin, &ps.velocity, pml.frametime);
}

#[allow(clippy::assign_op_pattern)]
fn integrate_origin(origin: &mut [f32; 3], velocity: &[f32; 3], frametime: f32) {
    origin[0] = origin[0] + frametime * velocity[0];
    origin[1] = velocity[1] * frametime + origin[1];
    origin[2] = frametime * velocity[2] + origin[2];
}
