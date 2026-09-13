use crate::Pml;
use crate::jump_clear_state;
use playerstate_iw4::PlayerState;

const PM_TIME_EXPIRY_CLEAR: u32 = 0x2180;

pub fn pm_drop_timers(ps: &mut PlayerState, pml: &Pml) {
    if ps.pm_time != 0 {
        if pml.msec < ps.pm_time {
            ps.pm_time -= pml.msec;
        } else {
            if (ps.pm_flags & 0x2000) != 0 {
                jump_clear_state(ps);
            }
            ps.pm_flags &= !PM_TIME_EXPIRY_CLEAR;
            ps.pm_time = 0;
        }
    }
    if ps.legs_timer > 0 {
        ps.legs_timer -= pml.msec;
        if ps.legs_timer < 0 {
            ps.legs_timer = 0;
        }
    }
    if ps.torso_timer > 0 {
        ps.torso_timer -= pml.msec;
        if ps.torso_timer < 0 {
            ps.torso_timer = 0;
        }
    }
}
