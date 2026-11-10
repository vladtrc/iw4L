use crate::SimContent;
use playerstate_iw4::PlayerState;
#[derive(Clone, Copy)]
pub struct AnimationInputs<'a> {
    pub script: Option<&'a crate::PlayerAnimScript>,
    pub last_movetype: Option<u8>,
    pub strafing: u8,
    pub buttons: u32,
}

pub fn apply_animation_event(
    state: &mut PlayerState,
    content: &SimContent,
    animation: &AnimationInputs<'_>,
    seed: &mut u32,
    event: u8,
    next_weapon: Option<u32>,
    force: bool,
) {
    let Some(script) = animation.script else {
        return;
    };
    let (view, primary) = crate::pmove_anim_weapon_ids(state);
    let movetype = crate::player_anim_script::event_anim_movetype(state, animation.last_movetype);
    let mut conditions = crate::anim_conditions_from_pmove(
        state,
        content.weapon_combat_facts(view),
        content.weapon_combat_facts(primary),
        Some(movetype),
        animation.strafing,
        animation.buttons,
    );
    if let Some(next) = next_weapon {
        conditions.set_value(
            anim_iw4::ANIM_COND_RIOTSHIELDNEXT,
            u32::from(
                content
                    .weapon_combat_facts(next)
                    .is_some_and(|facts| facts.player_anim_type == 15),
            ),
        );
    }
    script.apply_event(state, event, &conditions, seed, force);
}

pub struct PlayerWeaponAnimation<'a> {
    pub content: &'a SimContent,
    pub inputs: AnimationInputs<'a>,
    pub seed: &'a mut u32,
}

impl weapon_iw4::WeaponAnimation for PlayerWeaponAnimation<'_> {
    fn apply(&mut self, state: &mut PlayerState, event: u8, next: Option<u32>, force: bool) {
        apply_animation_event(
            state,
            self.content,
            &self.inputs,
            self.seed,
            event,
            next,
            force,
        );
    }
}
