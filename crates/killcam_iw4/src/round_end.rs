use crate::log::Log;
use crate::notify::NotifyKind;
use crate::task::{EndOn, Millis, Scheduler, TaskId, Wake};

pub const POST_ROUND_TIME_MS: Millis = 5_000;

pub const ROUND_END_DELAY_MS: Millis = 4_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundEndWaitConfig {
    pub delay_ms: Millis,

    pub match_bonus: bool,
}

impl RoundEndWaitConfig {
    pub const fn ffa_match_end() -> Self {
        Self {
            delay_ms: POST_ROUND_TIME_MS,
            match_bonus: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundEndWaitOutput {
    GiveMatchBonus,

    RoundEndFinished,
}

pub type RoundEndWaitLog = Log<RoundEndWaitOutput, 2>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pc {
    FirstWait,

    SecondWait,
    Done,
}

#[derive(Clone, Copy, Debug)]
pub struct RoundEndWaitSequence {
    sched: Scheduler<NotifyKind, 2>,
    main: TaskId,
    cfg: RoundEndWaitConfig,
    pc: Pc,
}

impl RoundEndWaitSequence {
    pub fn start(now_ms: Millis, cfg: RoundEndWaitConfig) -> Self {
        let mut sched: Scheduler<NotifyKind, 2> = Scheduler::new();
        let first_ms = if cfg.match_bonus {
            cfg.delay_ms / 2
        } else {
            cfg.delay_ms
        };
        let main = sched
            .spawn(0, Wake::Deadline(now_ms + first_ms), EndOn::none())
            .expect("round end wait main");
        Self {
            sched,
            main,
            cfg,
            pc: Pc::FirstWait,
        }
    }

    pub const fn is_done(&self) -> bool {
        matches!(self.pc, Pc::Done)
    }

    pub fn advance(&mut self, now_ms: Millis) -> RoundEndWaitLog {
        let mut out = RoundEndWaitLog::new();
        if self.pc == Pc::Done {
            return out;
        }
        let events = self.sched.advance(now_ms, &[]);
        for event in events.iter() {
            if event.id() != self.main {
                continue;
            }
            match self.pc {
                Pc::FirstWait => {
                    if self.cfg.match_bonus {
                        out.push(RoundEndWaitOutput::GiveMatchBonus);
                        let half = self.cfg.delay_ms / 2;
                        self.pc = Pc::SecondWait;
                        self.sched
                            .resume(self.main, 0, Wake::Deadline(now_ms + half))
                            .expect("second half");
                    } else {
                        out.push(RoundEndWaitOutput::RoundEndFinished);
                        self.pc = Pc::Done;
                        self.sched.finish(self.main);
                    }
                }
                Pc::SecondWait => {
                    out.push(RoundEndWaitOutput::RoundEndFinished);
                    self.pc = Pc::Done;
                    self.sched.finish(self.main);
                }
                Pc::Done => {}
            }
        }
        out
    }
}
