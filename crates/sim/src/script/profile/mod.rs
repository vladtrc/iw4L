pub mod catalog;
mod iw4_campaign_catalog;
pub mod iw4_catalog;
pub mod iw4_startup;

pub(crate) fn iw4_builtins() -> &'static [catalog::Builtin] {
    iw4_catalog::IW4
}
