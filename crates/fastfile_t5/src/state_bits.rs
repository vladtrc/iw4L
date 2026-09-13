pub const fn alpha_test(word0: u32) -> Option<(u32, u8)> {
    if word0 & 0x800 != 0 {
        return None;
    }
    match (word0 >> 12) & 3 {
        1 => Some((5, 0)),
        2 => Some((7, 255)),
        _ => Some((7, 128)),
    }
}

pub const fn smodel_camera_emits(game_flags: u8, camera_region: u8) -> bool {
    camera_region != 3 && game_flags & 1 == 0
}
