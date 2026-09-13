pub fn is_really_alive(is_alive: bool, faux_dead: bool) -> bool {
    is_alive && !faux_dead
}

pub const SUICIDE_INTERNAL_DAMAGE: i32 = 10000;

pub const SUICIDE_MOD: &str = "MOD_SUICIDE";

pub const SUICIDE_WEAPON: &str = "frag_grenade_mp";

pub const SUICIDE_HITLOC: &str = "none";

pub const SUICIDE_INTERNAL_P9: i32 = 0;

pub const SUICIDE_INTERNAL_P10: i32 = 1116;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuicideAction {
    PlayerKilledInternal,

    BuiltinSuicide,

    None,
}

pub fn suicide_action(using_remote: bool, faux_dead: bool) -> SuicideAction {
    if using_remote && !faux_dead {
        SuicideAction::PlayerKilledInternal
    } else if !using_remote && !faux_dead {
        SuicideAction::BuiltinSuicide
    } else {
        SuicideAction::None
    }
}
