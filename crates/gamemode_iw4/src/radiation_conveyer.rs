pub const TRIGGER_TARGETNAME: &str = "coveyer_trig";

pub const SPEED: f32 = 45.0;

pub fn is_conveyer_trigger(targetname: &str) -> bool {
    targetname == TRIGGER_TARGETNAME
}

pub fn force_vector(struct_angles: [f32; 3]) -> [f32; 3] {
    let (forward, _, _) = math_iw4::angle_vectors(struct_angles);
    [forward[0] * SPEED, forward[1] * SPEED, forward[2] * SPEED]
}

pub fn should_push(on_ground: bool) -> bool {
    on_ground
}
