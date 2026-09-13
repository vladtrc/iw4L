#[inline]
pub fn select_fire_sound_ptr(player_view: bool, fire_sound: u32, fire_sound_player: u32) -> u32 {
    if player_view && fire_sound_player != 0 {
        fire_sound_player
    } else {
        fire_sound
    }
}

#[inline]
pub fn select_fire_last_sound_ptr(
    last_shot: bool,
    player_view: bool,
    current: u32,
    fire_last: u32,
    fire_last_player: u32,
) -> u32 {
    if !last_shot {
        return current;
    }
    if player_view && fire_last_player != 0 {
        return fire_last_player;
    }
    if fire_last != 0 {
        return fire_last;
    }
    current
}

#[inline]
pub fn select_cg_fire_sound_ptr(
    last_shot: bool,
    player_view: bool,
    fire_sound: u32,
    fire_sound_player: u32,
    fire_last: u32,
    fire_last_player: u32,
) -> u32 {
    let current = select_fire_sound_ptr(player_view, fire_sound, fire_sound_player);
    select_fire_last_sound_ptr(last_shot, player_view, current, fire_last, fire_last_player)
}
