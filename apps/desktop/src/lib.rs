mod asset_transport;
pub use asset_transport::discover::{DesktopGamesProvider, load_dotenv};
pub use asset_transport::steam::{MW2_SHORTCUT, SteamCandidate, SteamProbe, link_steam_games};
