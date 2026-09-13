use bevy::prelude::Resource;
use gamemode_iw4::end_game::{EndGameTail, EndGameTailConfig, EndGameTailOutput};
use sim::{EventRecord, SimEvent};

#[derive(Resource, Debug, Default)]
pub struct PendingEndGameTail {
    tail: Option<EndGameTail>,

    pub spawning_intermission: u32,

    pub exit_level_called: u32,
}

impl PendingEndGameTail {
    pub fn start_from_journal(&mut self, journal: &[EventRecord]) {
        if self.tail.is_some() {
            return;
        }
        if !journal
            .iter()
            .any(|r| matches!(r.event, SimEvent::MatchEnded { .. }))
        {
            return;
        }
        self.tail = Some(EndGameTail::start(EndGameTailConfig::single_round()));
    }

    pub fn intermission(&self) -> bool {
        self.tail.as_ref().is_some_and(EndGameTail::intermission)
    }

    pub fn advance(
        &mut self,
        now_ms: i32,
        round_end_finished: bool,
        showing_final_killcam: bool,
    ) -> Option<EndGameTailOutput> {
        let tail = self.tail.as_mut()?;
        let out = tail.advance(now_ms, round_end_finished, showing_final_killcam);
        match out {
            Some(EndGameTailOutput::SpawningIntermission) => {
                self.spawning_intermission = self.spawning_intermission.saturating_add(1);
            }
            Some(EndGameTailOutput::ExitLevel) => {
                self.exit_level_called = self.exit_level_called.saturating_add(1);
            }
            None => {}
        }
        out
    }
}
