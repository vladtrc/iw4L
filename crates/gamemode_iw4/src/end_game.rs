pub const LOBBY_DATA_WAIT_MS: i32 = 1_000;

pub const ONLY_ROUND_EXIT_WAIT_MS: i32 = 3_000;

pub const MULTI_ROUND_EXIT_WAIT_MS: i32 = 6_000;

pub const POST_GAME_NOTIFY_BASE_MS: i32 = 4_000;

pub const POST_GAME_NOTIFY_CAP_MS: i32 = 10_000;

pub const FINAL_KILLCAM_POLL_MS: i32 = 50;

pub const fn exit_wait_ms(only_round: bool, post_game_notifies: i32) -> i32 {
    if post_game_notifies <= 0 {
        if only_round {
            ONLY_ROUND_EXIT_WAIT_MS
        } else {
            MULTI_ROUND_EXIT_WAIT_MS
        }
    } else {
        let want = POST_GAME_NOTIFY_BASE_MS + post_game_notifies * 1_000;
        if want < POST_GAME_NOTIFY_CAP_MS {
            want
        } else {
            POST_GAME_NOTIFY_CAP_MS
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndGameTailConfig {
    pub only_round: bool,

    pub post_game_notifies: i32,
}

impl EndGameTailConfig {
    pub const fn single_round() -> Self {
        Self {
            only_round: true,
            post_game_notifies: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndGameTailOutput {
    SpawningIntermission,

    ExitLevel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pc {
    DisplayGameEnd,

    WaitFinalKillcam { next_poll_ms: i32 },

    LobbyData { until_ms: i32 },

    ExitWait { until_ms: i32 },
    Done,
}

#[derive(Clone, Copy, Debug)]
pub struct EndGameTail {
    cfg: EndGameTailConfig,
    pc: Pc,
}

impl EndGameTail {
    pub const fn start(cfg: EndGameTailConfig) -> Self {
        Self {
            cfg,
            pc: Pc::DisplayGameEnd,
        }
    }

    pub const fn is_done(&self) -> bool {
        matches!(self.pc, Pc::Done)
    }

    pub const fn intermission(&self) -> bool {
        !matches!(self.pc, Pc::DisplayGameEnd | Pc::WaitFinalKillcam { .. })
    }

    pub fn advance(
        &mut self,
        now_ms: i32,
        round_end_finished: bool,
        showing_final_killcam: bool,
    ) -> Option<EndGameTailOutput> {
        match self.pc {
            Pc::DisplayGameEnd => {
                if !round_end_finished {
                    return None;
                }
                self.pc = Pc::WaitFinalKillcam {
                    next_poll_ms: now_ms,
                };
                self.advance(now_ms, false, showing_final_killcam)
            }
            Pc::WaitFinalKillcam { next_poll_ms } => {
                if now_ms < next_poll_ms {
                    return None;
                }
                if showing_final_killcam {
                    self.pc = Pc::WaitFinalKillcam {
                        next_poll_ms: now_ms + FINAL_KILLCAM_POLL_MS,
                    };
                    return None;
                }
                self.pc = Pc::LobbyData {
                    until_ms: now_ms + LOBBY_DATA_WAIT_MS,
                };
                Some(EndGameTailOutput::SpawningIntermission)
            }
            Pc::LobbyData { until_ms } => {
                if now_ms < until_ms {
                    return None;
                }
                self.pc = Pc::ExitWait {
                    until_ms: now_ms
                        + exit_wait_ms(self.cfg.only_round, self.cfg.post_game_notifies),
                };
                None
            }
            Pc::ExitWait { until_ms } => {
                if now_ms < until_ms {
                    return None;
                }
                self.pc = Pc::Done;
                Some(EndGameTailOutput::ExitLevel)
            }
            Pc::Done => None,
        }
    }
}
