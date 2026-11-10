use asset_game::*;
use asset_iw4::size::weap_anim;
pub mod attachments;
use attachments::Attachment;
mod capture_iw5;
pub use capture_iw5::NativeWeaponCapture;
pub mod configuration;
mod iw5_configuration;
mod iw5_parameters;

pub mod captured;
use captured::*;

mod body;
use body::*;

pub mod preparation;
