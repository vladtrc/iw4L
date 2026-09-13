pub const CALLBACK_START_GAME_TYPE: &str = "maps/mp/gametypes/_gamelogic::Callback_StartGameType";

pub const CALLBACK_PLAYER_CONNECT: &str = "maps/mp/gametypes/_playerlogic::Callback_PlayerConnect";

pub const CALLBACK_PLAYER_DISCONNECT: &str =
    "maps/mp/gametypes/_playerlogic::Callback_PlayerDisconnect";

pub const CALLBACK_PLAYER_DAMAGE: &str = "maps/mp/gametypes/_damage::Callback_PlayerDamage";

pub const CALLBACK_PLAYER_KILLED: &str = "maps/mp/gametypes/_damage::Callback_PlayerKilled";

pub const CALLBACK_CODE_END_GAME: &str = "maps/mp/gametypes/_gamelogic::Callback_CodeEndGame";

pub const CALLBACK_PLAYER_LAST_STAND: &str = "maps/mp/gametypes/_damage::Callback_PlayerLastStand";

pub const CALLBACK_PLAYER_MIGRATED: &str =
    "maps/mp/gametypes/_playerlogic::Callback_PlayerMigrated";

pub const CALLBACK_HOST_MIGRATION: &str =
    "maps/mp/gametypes/_hostmigration::Callback_HostMigration";

pub const DEFAULT_CALLBACKS: &[&str] = &[
    CALLBACK_START_GAME_TYPE,
    CALLBACK_PLAYER_CONNECT,
    CALLBACK_PLAYER_DISCONNECT,
    CALLBACK_PLAYER_DAMAGE,
    CALLBACK_PLAYER_KILLED,
    CALLBACK_CODE_END_GAME,
    CALLBACK_PLAYER_LAST_STAND,
    CALLBACK_PLAYER_MIGRATED,
    CALLBACK_HOST_MIGRATION,
];
