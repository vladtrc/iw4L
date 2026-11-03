use bevy::prelude::*;
use frame::ScopeApp;

pub struct RenderAnimPlugin;

impl Plugin for RenderAnimPlugin {
    fn build(&self, app: &mut App) {
        app.scoped::<crate::anim::scene_submission::AnimDObjSceneSkels>(frame::MatchScope::Live)
            .scoped::<crate::anim::fpv_host::FpvPresentCursor>(frame::Timeline::Live)
            .scoped::<crate::anim::fpv_host::PendingFpvSpawn>(frame::Timeline::Live)
            .scoped::<crate::anim::fpv_host::PendingFpvNotetracks>(frame::Timeline::Live)
            .scoped::<crate::anim::fpv_host::FpvHeldSettled>(frame::Timeline::Live)
            .scoped::<crate::anim::fpv_host::FpvHeldLife>(frame::Timeline::Live)
            .scoped::<crate::anim::fpv_host::FpvBoltTargets>(frame::MatchScope::Live)
            .scoped::<crate::anim::fpv_host::FpvPoseProduct>(frame::MatchScope::Live)
            .scoped::<crate::draw::FpvDrawPlan>(frame::MatchScope::Live)
            .scoped::<crate::draw::ScriptModelDrawPlan>(frame::MatchScope::Live)
            .scoped::<crate::draw::MissileDrawPlan>(frame::MatchScope::Live)
            .scoped::<crate::draw::ItemDrawPlan>(frame::MatchScope::Live)
            .scoped::<crate::draw::DynEntDrawPlan>(frame::MatchScope::Live)
            .scoped_message::<crate::anim::scene_submission::AnimDObjSceneSubmission>(
                frame::MatchScope::Live,
            );
        crate::occupancy::fpv_present::register_fpv_present_systems(app);
        crate::occupancy::held_sync::register_held_sync_systems(app);
        crate::occupancy::remote_body::register_remote_body_systems(app);
        crate::occupancy::script_model::register_script_model_systems(app);
        crate::occupancy::missile::register_missile_systems(app);
        crate::occupancy::item::register_item_systems(app);
        crate::occupancy::dyn_ent::register_dyn_ent_systems(app);
    }
}
