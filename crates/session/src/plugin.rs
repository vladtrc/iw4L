use bevy::prelude::*;

pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        crate::scope_control::register(app);
        crate::lifecycle::register_lifecycle(app);
        crate::campaign_reload::register(app);
        crate::match_load::register(app);
        crate::match_apply::register_match_apply_systems(app);
        crate::admission::register_admission(app);
        crate::local_arm::register_local_arm_systems(app);
        crate::life_front::register_life_front(app);
        crate::view_subject::register_view_subject(app);
    }
}
