use playerstate_iw4::{PlayerState, UserCmd, buttons};

pub(crate) fn move_player(
    ps: &mut PlayerState,
    cmd: &UserCmd,
    dt: f32,
    forward: [f32; 3],
    right: [f32; 3],
) {
    let vertical = i32::from(cmd.buttons & buttons::JUMP != 0)
        - i32::from(cmd.buttons & (buttons::CROUCH | buttons::PRONE) != 0);
    let direction: [f32; 3] = core::array::from_fn(|i| {
        forward[i] * cmd.forwardmove as f32 / 127.0
            + right[i] * cmd.rightmove as f32 / 127.0
            + if i == 2 { vertical as f32 } else { 0.0 }
    });
    let length = libm::sqrtf(direction.iter().map(|v| v * v).sum::<f32>()).max(1.0);
    let speed = if cmd.buttons & buttons::SPRINT != 0 {
        900.0
    } else {
        300.0
    };
    for (i, component) in direction.into_iter().enumerate() {
        ps.velocity[i] = component / length * speed;
        ps.origin[i] += ps.velocity[i] * dt;
    }
    ps.ground_entity_num = playerstate_iw4::ENTITYNUM_NONE;
}
