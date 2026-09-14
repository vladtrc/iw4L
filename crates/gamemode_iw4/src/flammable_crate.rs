pub const TARGETNAME: &str = "flammable_crate";

pub const HEALTH: i32 = 200;

pub const BURN: i32 = 100;

pub const BURN_DRAIN: i32 = 10;

pub const BURN_DRAIN_INTERVAL_MS: u32 = 1000;

pub const DESTROYED_STATE: u8 = 1;

pub const EXPLODE_RANGE: u32 = 250;

pub const EXPLODE_DAMAGE: (u32, u32) = (1, 250);

pub const EXPLODE_ORIGIN_Z: f32 = 30.0;

pub const HUSK: &str = "global_flammable_crate_jap_piece01_d";

pub fn is_flammable_crate(targetname: &str, script_noteworthy: &str) -> bool {
    targetname.eq_ignore_ascii_case(TARGETNAME)
        || script_noteworthy.eq_ignore_ascii_case(TARGETNAME)
}

pub fn mapent_flag(value: &str) -> bool {
    !value.is_empty() && value != "0"
}

pub fn damage_applies(requires_player: bool, attacker_is_player: bool) -> bool {
    !requires_player || attacker_is_player
}

pub fn health_after(health: i32, amount: i32) -> i32 {
    health.saturating_sub(amount.max(0))
}

pub fn should_ignite(health: i32) -> bool {
    health > 0 && health <= BURN
}

pub fn should_explode(health: i32) -> bool {
    health <= 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn think_ignites_at_burn_threshold() {
        assert_eq!(health_after(HEALTH, 50), 150);
        assert!(!should_ignite(150));
        assert!(should_ignite(health_after(HEALTH, 100)));
        assert!(!should_explode(100));
    }

    #[test]
    fn a_killing_blow_explodes_without_a_burn_loop() {
        assert!(should_explode(health_after(HEALTH, HEALTH)));
        assert!(!should_ignite(health_after(HEALTH, HEALTH)));
    }
}
