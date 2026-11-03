use std::sync::Arc;

use crate::{ClientId, Snapshot, TickInput};

#[derive(Clone, Debug, Default)]
pub struct TickEffects {
    pub prints: Vec<crate::PendingPrint>,
    pub local_sounds: Vec<crate::PendingLocalSound>,
    pub player_cards: Vec<crate::PendingPlayerCardEvent>,
    pub script_audio: Vec<crate::ScriptAudioCommand>,
    pub kicks: Vec<(ClientId, String)>,
}

#[derive(Clone, Debug)]
pub struct TickResult {
    pub input: TickInput,
    pub snapshot: Snapshot,
    pub effects: TickEffects,
    pub weapon_script_names: Arc<[String]>,
    pub pending_final_kill: Option<(ClientId, ClientId)>,
    pub script_seats: Vec<(ClientId, crate::ScriptSeat)>,
    pub script_exit_level: bool,
}
