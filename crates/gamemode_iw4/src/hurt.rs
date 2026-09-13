use crate::suicide::is_really_alive;

pub const HURT_INITIAL_WAIT_MAX: f32 = 1.0;

pub fn hurt_should_suicide(touching: bool, is_alive: bool, faux_dead: bool) -> bool {
    touching && is_really_alive(is_alive, faux_dead)
}
