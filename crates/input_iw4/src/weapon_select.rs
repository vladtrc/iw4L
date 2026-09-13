use playerstate_iw4::PlayerState;

pub fn weapon_cycle_allowed(
    ps: &PlayerState,
    time: i32,
    select_time: i32,
    select_time_override: i32,
    cycle_delay: i32,
) -> bool {
    ps.pm_flags & 0xc08 == 0
        && ps.weap_flags & 0x880 == 0
        && ps.other_flags & 2 == 0
        && ps.other_flags & 0x1000 != 0
        && ps.e_flags & 0x100c00 == 0
        && ps.pm_type <= 7
        && time.wrapping_sub(select_time) >= cycle_delay
        && select_time_override <= time
}

pub fn cycle_weapon(
    weapons: &[i32; 15],
    selected: u32,
    last_primary: u32,
    next: bool,
    mut inventory_type: impl FnMut(u32) -> i32,
) -> Option<u32> {
    let current_type = if selected == 0 {
        0
    } else {
        inventory_type(selected)
    };
    if current_type == 3 {
        panic!("alternate-mode parent selection and remembered alt mode");
    }
    if current_type != 0
        && current_type != 4
        && last_primary != 0
        && weapons.contains(&(last_primary as i32))
    {
        return Some(last_primary);
    }
    let wanted_type = if current_type == 4 { 4 } else { 0 };
    let start = if selected == 0 {
        0
    } else {
        weapons
            .iter()
            .position(|&w| w == selected as i32)
            .unwrap_or(0)
    };

    let count = if selected == 0 { 15 } else { 14 };
    for step in 0..count {
        let distance = step + usize::from(selected != 0);
        let slot = if next {
            (start + distance) % 15
        } else {
            (start + 15 - distance) % 15
        };
        let candidate = weapons[slot];
        if candidate > 0 && inventory_type(candidate as u32) == wanted_type {
            return Some(candidate as u32);
        }
    }
    None
}
