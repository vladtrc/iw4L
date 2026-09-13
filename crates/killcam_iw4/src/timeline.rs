use crate::camtime::SERVER_FRAME_MS;
use crate::cancel::{CancelTick, DoubleTap};
use crate::log::Log;
use crate::notify::NotifyKind;
use crate::task::{EndOn, Millis, Scheduler, TaskId, Wake};

pub const ONE_SECOND_MS: Millis = 1000;

pub const QUARTER_SECOND_MS: Millis = 250;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeathConfig {
    pub is_faux_death: bool,

    pub final_kill: bool,

    pub do_killcam: bool,

    pub nuke_detonated: bool,

    pub level_killcam: bool,

    pub showing_final_killcam: bool,

    pub victim_has_copycat: bool,

    pub game_state_playing: bool,

    pub is_using_remote: bool,

    pub lives_left: bool,

    pub time_until_spawn: f32,

    pub death_time_offset: f32,
}

impl Default for DeathConfig {
    fn default() -> Self {
        Self {
            is_faux_death: false,
            final_kill: false,
            do_killcam: true,
            nuke_detonated: false,
            level_killcam: true,
            showing_final_killcam: false,
            victim_has_copycat: false,
            game_state_playing: true,
            is_using_remote: false,
            lives_left: true,
            time_until_spawn: 5.0,
            death_time_offset: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StartKillcam {
    pub predelay: f32,

    pub time_until_respawn: f32,

    pub will_respawn_immediately: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoKillcam {
    CancelledByPlayer,

    NotForThisDeath,

    DisabledByTweakable,

    NotPlaying,

    UsingRemote,

    FinalKillcamOwnsIt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeathOutput {
    ThreadFinalKillcam,

    ThreadDeathCopyCatButton,

    ArmCancelOnUse,

    PredictAboutToSpawn,

    DeathDelayFinished,

    PostDeathDelay(f32),

    CancelKillcamPressed,

    StartKillcam(StartKillcam),

    NoKillcam(NoKillcam),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pc {
    Enter,
    AfterFinalKillcamWait,
    CopycatBranch,
    FirstQuarter,
    ArmPhaseA,
    PredictSpawn,
    NotifyDeathDelayFinished,
    Measure,
    Done,
}

#[derive(Clone, Copy, Debug)]
pub struct DeathSequence {
    cfg: DeathConfig,

    death_time: Millis,
    sched: Scheduler<NotifyKind, 4>,
    main: TaskId,
    phase_a: Option<TaskId>,
    pc: Pc,
    showing_final_killcam: bool,
    cancel: DoubleTap,
    cancel_killcam: bool,
    post_death_delay: Option<f32>,
}

pub type DeathLog = Log<DeathOutput, 6>;

impl DeathSequence {
    pub fn start(now_ms: Millis, cfg: DeathConfig) -> (Self, DeathLog) {
        let mut sched: Scheduler<NotifyKind, 4> = Scheduler::new();

        let main = sched
            .spawn(0, Wake::Parked, EndOn::none())
            .expect("a fresh scheduler has room for the death callback");
        let mut me = Self {
            cfg,
            death_time: now_ms,
            sched,
            main,
            phase_a: None,
            pc: Pc::Enter,
            showing_final_killcam: cfg.showing_final_killcam,
            cancel: DoubleTap::new(),
            cancel_killcam: false,
            post_death_delay: None,
        };
        let mut out = DeathLog::new();
        me.run(now_ms, &mut out);
        (me, out)
    }

    pub fn advance(
        &mut self,
        now_ms: Millis,
        use_button_pressed: bool,
        incoming: &[NotifyKind],
    ) -> DeathLog {
        let mut out = DeathLog::new();
        let events = self.sched.advance(now_ms, incoming);
        for event in events.iter() {
            if event.id() == self.main {
                self.run(now_ms, &mut out);
            } else if Some(event.id()) == self.phase_a {
                if !self.sched.is_alive(event.id()) {
                    continue;
                }
                self.tick_phase_a(now_ms, use_button_pressed, &mut out);
            }
        }
        out
    }

    pub const fn cancel_killcam(&self) -> bool {
        self.cancel_killcam
    }

    pub const fn post_death_delay(&self) -> Option<f32> {
        self.post_death_delay
    }

    pub const fn showing_final_killcam(&self) -> bool {
        self.showing_final_killcam
    }

    pub const fn is_done(&self) -> bool {
        matches!(self.pc, Pc::Done)
    }

    fn wait(&mut self, now_ms: Millis, ms: Millis, next: Pc) {
        self.pc = next;
        self.sched
            .resume(self.main, 0, Wake::Deadline(now_ms + ms))
            .expect("the death callback cannot be cancelled");
    }

    fn run(&mut self, now_ms: Millis, out: &mut DeathLog) {
        loop {
            match self.pc {
                Pc::Enter => {
                    if self.cfg.final_kill && self.cfg.do_killcam && !self.cfg.nuke_detonated {
                        out.push(DeathOutput::ThreadFinalKillcam);

                        self.showing_final_killcam = true;
                        if !self.cfg.is_faux_death {
                            self.wait(now_ms, ONE_SECOND_MS, Pc::AfterFinalKillcamWait);
                            return;
                        }
                    }
                    self.pc = Pc::AfterFinalKillcamWait;
                }
                Pc::AfterFinalKillcamWait => {
                    if self.cfg.is_faux_death {
                        self.pc = Pc::Measure;
                        continue;
                    }
                    self.pc = Pc::CopycatBranch;
                }
                Pc::CopycatBranch => {
                    self.pc = Pc::FirstQuarter;
                    if !self.showing_final_killcam
                        && !self.cfg.level_killcam
                        && self.cfg.do_killcam
                        && self.cfg.victim_has_copycat
                    {
                        out.push(DeathOutput::ThreadDeathCopyCatButton);
                        self.wait(now_ms, ONE_SECOND_MS, Pc::FirstQuarter);
                        return;
                    }
                }
                Pc::FirstQuarter => {
                    self.wait(now_ms, QUARTER_SECOND_MS, Pc::ArmPhaseA);
                    return;
                }
                Pc::ArmPhaseA => {
                    out.push(DeathOutput::ArmCancelOnUse);
                    self.arm_phase_a(now_ms);
                    self.wait(now_ms, QUARTER_SECOND_MS, Pc::PredictSpawn);
                    return;
                }
                Pc::PredictSpawn => {
                    out.push(DeathOutput::PredictAboutToSpawn);
                    self.wait(now_ms, ONE_SECOND_MS, Pc::NotifyDeathDelayFinished);
                    return;
                }
                Pc::NotifyDeathDelayFinished => {
                    out.push(DeathOutput::DeathDelayFinished);

                    self.sched
                        .advance(now_ms, &[NotifyKind::DeathDelayFinished]);
                    self.phase_a = None;
                    self.pc = Pc::Measure;
                }
                Pc::Measure => {
                    let measured = (now_ms - self.death_time) as f32 / 1000.0;
                    self.post_death_delay = Some(measured);
                    out.push(DeathOutput::PostDeathDelay(measured));
                    match self.killcam_decision(measured) {
                        Ok(start) => out.push(DeathOutput::StartKillcam(start)),
                        Err(reason) => out.push(DeathOutput::NoKillcam(reason)),
                    }
                    self.pc = Pc::Done;
                    self.sched.finish(self.main);
                    return;
                }
                Pc::Done => return,
            }
        }
    }

    fn killcam_decision(&self, post_death_delay: f32) -> Result<StartKillcam, NoKillcam> {
        if self.cancel_killcam {
            return Err(NoKillcam::CancelledByPlayer);
        }
        if !self.cfg.do_killcam {
            return Err(NoKillcam::NotForThisDeath);
        }
        if !self.cfg.level_killcam {
            return Err(NoKillcam::DisabledByTweakable);
        }
        if !self.cfg.game_state_playing {
            return Err(NoKillcam::NotPlaying);
        }
        if self.cfg.is_using_remote {
            return Err(NoKillcam::UsingRemote);
        }
        if self.showing_final_killcam {
            return Err(NoKillcam::FinalKillcamOwnsIt);
        }
        let time_until_spawn = self.cfg.time_until_spawn;
        let will_respawn_immediately = self.cfg.lives_left && time_until_spawn <= 0.0;
        Ok(StartKillcam {
            predelay: post_death_delay + self.cfg.death_time_offset,
            time_until_respawn: if self.cfg.lives_left {
                time_until_spawn
            } else {
                -1.0
            },
            will_respawn_immediately,
        })
    }

    fn arm_phase_a(&mut self, now_ms: Millis) {
        let id = self
            .sched
            .spawn(
                0,
                Wake::Deadline(now_ms + SERVER_FRAME_MS),
                EndOn::none()
                    .on(NotifyKind::DeathDelayFinished)
                    .on(NotifyKind::Disconnect)
                    .on(NotifyKind::GameEnded),
            )
            .expect("the scheduler has room for the cancel thread");
        self.phase_a = Some(id);
    }

    fn tick_phase_a(&mut self, now_ms: Millis, pressed: bool, out: &mut DeathLog) {
        let Some(id) = self.phase_a else { return };
        if self.cancel.tick(pressed) == CancelTick::Fired {
            self.cancel_killcam = true;
            out.push(DeathOutput::CancelKillcamPressed);
            self.sched.finish(id);
            self.phase_a = None;
            return;
        }
        let _ = self
            .sched
            .resume(id, 0, Wake::Deadline(now_ms + SERVER_FRAME_MS));
    }
}
