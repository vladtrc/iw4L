#![no_std]
#![forbid(unsafe_code)]

pub mod battlechatter;
pub mod dialog;
pub mod level;
pub mod music;
pub mod notify;
pub mod output;
pub mod recipes;
pub mod suspense;

pub use battlechatter::{
    BATTLECHATTER_INFIX, BATTLECHATTER_STEMS, DEATH_VOICE_MAX_EXCLUSIVE, DEATH_VOICE_MIN, STEM_C4,
    STEM_CLAYMORE, STEM_FLASH, STEM_FRAG, STEM_KILLFIRM, STEM_RELOAD, STEM_SMOKE, STEM_STUN,
    death_voice_nationality,
};
pub use dialog::{
    Broadcast, DialogRequest, LEADER_DIALOG_WAIT_MS, LeaderDialogQueue, NULL, PlayerDialog,
    advance_all, group, leader_dialog, leader_dialog_on_one, leader_dialog_on_one_grouped,
    leader_dialog_on_players,
};
pub use level::{Fleet, Level, Player, TeamScores, is_excluded, play_sound_on_players};
pub use music::{MusicController, MusicStep, play_ffa_game_win, play_spawn_music};
pub use notify::{MatchEndingReason, Notify, NotifyKind, RoundSwitch, Winner};
pub use output::{
    Alias, ClientId, ClientMask, Origin, Output, PersTeam, Team, VOICE_INFIX,
    music_bus_interaction, team_voice_prefix,
};
pub use recipes::{
    DialogSet, INIT, Recipe, dialog_line, music_alias, suspense_len, suspense_track,
    team_music_alias,
};
pub use suspense::{SuspenseMusic, SuspenseStep, final_killcam_music};
