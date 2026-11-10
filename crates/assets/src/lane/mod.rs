pub(crate) mod helpers;
mod iw4;
mod region;
mod sink;
pub(crate) use helpers::build_static_model_draw;
pub const REFERENCE_ZONE_LANE: &dyn crate::ZoneLane = &iw4::Iw4Lane;
