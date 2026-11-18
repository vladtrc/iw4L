pub mod args;
mod frame_owner;
mod launch;
mod plugins;
pub mod run_manifest;
mod showcase;

pub use args::{AcceptanceLaunch, LaunchMode, parse_cli};
pub use launch::launch;
pub use plugins::assemble_listen_app;

/// Assemble the selected families and platform services once per process.
pub fn initialize_runtime() {
    static INITIALIZE: std::sync::Once = std::sync::Once::new();
    INITIALIZE.call_once(|| {
        game_family::install(&[
            &game_family::Iw4Family,
            #[cfg(feature = "iw5")]
            &ext_iw5::Family,
            #[cfg(feature = "t5")]
            &ext_t5::Family,
            #[cfg(feature = "t6")]
            &ext_t6::Family,
        ])
        .expect("install game families");
        frame::install_showcase_classes(showcase::SHOWCASE).expect("install showcase classes");
        asset_transport::install_games_provider(desktop::DesktopGamesProvider)
            .expect("install desktop games provider");
    });
}

pub const FONT_LICENSES: [(&str, &str); 2] = [
    ("OFL-Oxanium.txt", ui::FONT_LICENSE),
    ("OFL-FiraMono.txt", console::FONT_LICENSE),
];
