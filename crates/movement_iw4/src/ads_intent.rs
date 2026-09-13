use crate::add_predictable_event;
use playerstate_iw4::{PlayerState, UserCmd};

pub const PMF_ADS_INTENT: u32 = 0x10;

const PMF_ADS_PRONE_LATCH: u32 = 0x200;

const PMF_PRONE: u32 = 0x1;

const BUTTON_SPRINT: u32 = 0x2;

pub const BUTTON_ADS: u32 = 0x800;

const BUTTON_SCOPE_HOLD: u32 = 0x2000;

const EV_RESET_ADS: i32 = 0x11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdsIntentContext {
    pub ads_allowed: bool,

    pub weapon_def_scope: bool,

    pub sprint_hold_ads: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdsIntentResult {
    pub ads_anim_enabled: bool,

    pub exit_ads_event: bool,
}

fn prone_stance_settled(ps: &PlayerState) -> bool {
    (ps.pm_flags & 0x800) == 0 && (ps.pm_flags & 0x400_000) != 0 && ps.pm_time == 0
}

fn scoped_weapon_raised(ps: &PlayerState, weapon_def_scope: bool) -> bool {
    weapon_def_scope && ps.f_weapon_pos_frac > 0.0
}

pub fn pm_update_ads_intent(
    ps: &mut PlayerState,
    cmd: &UserCmd,
    old_buttons: u32,
    context: AdsIntentContext,
) -> AdsIntentResult {
    ps.pm_flags &= !PMF_ADS_INTENT;

    let mut ads_allowed = context.ads_allowed;
    let mut exit_ads_event = false;

    if (cmd.buttons & BUTTON_SPRINT) != 0
        && (!context.weapon_def_scope || (cmd.buttons & BUTTON_SCOPE_HOLD) == 0)
        && (!context.sprint_hold_ads || ps.f_weapon_pos_frac == 0.0)
    {
        add_predictable_event(ps, EV_RESET_ADS, 0);
        exit_ads_event = true;
        ps.pm_flags &= !PMF_ADS_INTENT;
        ads_allowed = false;
    }

    if (cmd.buttons & BUTTON_ADS) != 0 && ads_allowed {
        if (ps.pm_flags & PMF_PRONE) != 0 && !scoped_weapon_raised(ps, context.weapon_def_scope) {
            let moving = cmd.forwardmove != 0 || cmd.rightmove != 0;
            if (old_buttons & BUTTON_ADS) != 0 && moving && !prone_stance_settled(ps) {
            } else {
                ps.pm_flags |= PMF_ADS_INTENT;
                if !prone_stance_settled(ps) {
                    ps.pm_flags |= PMF_ADS_PRONE_LATCH;
                }
            }
        } else {
            ps.pm_flags |= PMF_ADS_INTENT;
        }
    }

    AdsIntentResult {
        ads_anim_enabled: (ps.pm_flags & PMF_ADS_INTENT) != 0,
        exit_ads_event,
    }
}
