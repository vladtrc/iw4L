use crate::log::Log;
use crate::notify::NotifyKind;
use crate::task::{EndOn, Millis, Scheduler, TaskId, Wake};

pub const FINAL_KILLCAM_MAXTIME_SECONDS: f32 = 10_000.0;

pub const FINAL_KILLCAM_SETTLE_MS: Millis = 100;

pub const FINAL_KILLCAM_POLL_MS: Millis = 50;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FinalKillcamConfig {
    pub death_time_ms: Millis,

    pub death_time_offset: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FinalKillcamStart {
    pub predelay: f32,

    pub time_until_respawn: f32,

    pub maxtime: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FinalKillcamOutput {
    ShowingFinalKillcam,

    Start(FinalKillcamStart),

    Done,
}

pub type FinalKillcamLog = Log<FinalKillcamOutput, 4>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pc {
    WaitRoundEnd,

    Settle,

    PollPlayers,
    Done,
}

#[derive(Clone, Copy, Debug)]
pub struct FinalKillcamSequence {
    sched: Scheduler<NotifyKind, 2>,
    main: TaskId,
    cfg: FinalKillcamConfig,
    pc: Pc,
}

impl FinalKillcamSequence {
    pub fn start(cfg: FinalKillcamConfig) -> (Self, FinalKillcamLog) {
        let mut sched: Scheduler<NotifyKind, 2> = Scheduler::new();
        let main = sched
            .spawn(0, Wake::Notify(NotifyKind::RoundEndFinished), EndOn::none())
            .expect("final killcam main");
        let seq = Self {
            sched,
            main,
            cfg,
            pc: Pc::WaitRoundEnd,
        };
        let mut log = FinalKillcamLog::new();
        log.push(FinalKillcamOutput::ShowingFinalKillcam);
        (seq, log)
    }

    pub const fn is_done(&self) -> bool {
        matches!(self.pc, Pc::Done)
    }

    pub const fn showing(&self) -> bool {
        !matches!(self.pc, Pc::Done)
    }

    pub fn advance(
        &mut self,
        now_ms: Millis,
        level_notifies: &[NotifyKind],
        any_players_in_killcam: bool,
    ) -> FinalKillcamLog {
        let mut out = FinalKillcamLog::new();
        if self.pc == Pc::Done {
            return out;
        }
        let events = self.sched.advance(now_ms, level_notifies);
        for event in events.iter() {
            if event.id() != self.main {
                continue;
            }
            match self.pc {
                Pc::WaitRoundEnd => {
                    let post_death_delay = (now_ms - self.cfg.death_time_ms) as f32 / 1000.0;
                    out.push(FinalKillcamOutput::Start(FinalKillcamStart {
                        predelay: post_death_delay + self.cfg.death_time_offset,
                        time_until_respawn: 0.0,
                        maxtime: FINAL_KILLCAM_MAXTIME_SECONDS,
                    }));
                    self.pc = Pc::Settle;
                    self.sched
                        .resume(
                            self.main,
                            1,
                            Wake::Deadline(now_ms + FINAL_KILLCAM_SETTLE_MS),
                        )
                        .expect("final killcam settle");
                }
                Pc::Settle | Pc::PollPlayers => {
                    if any_players_in_killcam {
                        self.pc = Pc::PollPlayers;
                        self.sched
                            .resume(self.main, 2, Wake::Deadline(now_ms + FINAL_KILLCAM_POLL_MS))
                            .expect("final killcam poll");
                    } else {
                        out.push(FinalKillcamOutput::Done);
                        self.pc = Pc::Done;
                        self.sched.finish(self.main);
                    }
                }
                Pc::Done => {}
            }
        }
        out
    }
}
