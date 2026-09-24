use bevy::prelude::*;

pub mod anim;
pub mod fx;
pub(crate) mod motion_tracker;

pub struct RenderAdaptersPlugin;

impl Plugin for RenderAdaptersPlugin {
    fn build(&self, app: &mut App) {
        crate::adapters::fx::system::register_combat_fx_systems(app);
        motion_tracker::register(app);
        crate::adapters::anim::dyn_ent::register_dyn_ent_frontend(app);
        crate::adapters::anim::dyn_ent_brush::register_dyn_ent_brush_systems(app);
    }
}
