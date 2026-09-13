#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PlayerAnimScriptGap {
    pub fire_inputs: u64,
}

impl PlayerAnimScriptGap {
    pub fn total(&self) -> u64 {
        self.fire_inputs
    }

    pub const REASON: &'static str = "mp/playeranim.script default items write compiled indices; PLAYERANIMTYPE/AKIMBO still skip; WEAPON_POSITION is pm_flags&0x10; fire/reload events write torso when a default or ADS item matches; the flinch sector is written by PM_UpdateDamageTimer and no longer counted";
}
