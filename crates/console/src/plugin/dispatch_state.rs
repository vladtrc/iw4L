use bevy::prelude::*;

use crate::ConsoleCommand;

#[derive(Resource, Default)]
pub struct ConsoleCommandQueue(pub std::collections::VecDeque<ConsoleCommand>);

impl ConsoleCommandQueue {
    pub fn push_script(&mut self, script: &str) {
        self.0.extend(ConsoleCommand::parse_script(script));
    }
}

#[derive(Resource, Default)]
pub struct ConsoleDispatch {
    pub hold: Option<Hold>,
    pub quit_jumps: u64,
    pub fifo_jumps: u64,
}

impl ConsoleDispatch {
    pub fn wait(&mut self, until: WaitKind) {
        diag::info!(Console, "fifo hold: {until:?}");
        self.hold = Some(Hold {
            until,
            elapsed: 0.0,
        });
    }

    pub fn release(&mut self) {
        self.hold = None;
    }

    pub fn session_completed(&mut self) {
        if let Some(hold) = self.hold.as_mut() {
            hold.until = match hold.until {
                WaitKind::SessionWorld => WaitKind::World,
                WaitKind::SessionTorn => WaitKind::Torn,
                _ => return,
            };
        }
    }
}

#[derive(Debug)]
pub struct Hold {
    pub until: WaitKind,
    pub elapsed: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct WaitMovePose {
    pub client: sim::ClientId,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

pub(super) const WAIT_TIMEOUT_SECS: f32 = 120.0;

#[derive(Debug)]
pub enum WaitKind {
    Seconds(f32),
    Ticks(u32),
    Take(i32),
    World,
    Progression,
    Spawn,
    SpawnAdmission(Option<String>),
    Torn,
    Ambient,
    Weapon(u32),
    Move(WaitMovePose),
    Playing,
    FinalKillcam,
    Alive(sim::ClientId, sim::LifeSequence),
    SessionWorld,
    SessionTorn,
}

impl WaitKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Seconds(_) => "wait",
            Self::Ticks(_) => "wait ticks",
            Self::Take(_) => "wait take",
            Self::World | Self::SessionWorld => "wait world",
            Self::Progression => "wait progression",
            Self::Spawn => "wait spawn",
            Self::SpawnAdmission(_) => "spawn",
            Self::Torn | Self::SessionTorn => "wait torn",
            Self::Ambient => "wait ambient",
            Self::Weapon(_) => "wait weapon",
            Self::Move(_) => "wait move",
            Self::Playing => "wait playing",
            Self::FinalKillcam => "wait finalkillcam",
            Self::Alive(..) => "force_spawn",
        }
    }
}

pub(crate) fn parse_wait_args(args: &[String]) -> WaitKind {
    match args.first().map(String::as_str) {
        Some(s) if s.eq_ignore_ascii_case("take") => WaitKind::Take(
            args.get(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
                .clamp(0, 120_000),
        ),
        Some(s) if s.eq_ignore_ascii_case("finalkillcam") => WaitKind::FinalKillcam,
        Some(s) if s.eq_ignore_ascii_case("progression") => WaitKind::Progression,
        Some(s) if s.eq_ignore_ascii_case("world") => WaitKind::World,
        Some(s) if s.eq_ignore_ascii_case("spawn") => WaitKind::Spawn,
        Some(s) if s.eq_ignore_ascii_case("torn") => WaitKind::Torn,
        Some(s) if s.eq_ignore_ascii_case("ambient") => WaitKind::Ambient,
        Some(s) if s.eq_ignore_ascii_case("weapon") => {
            WaitKind::Weapon(args.get(1).and_then(|id| id.parse().ok()).unwrap_or(0))
        }
        Some(s) if s.ends_with(['t', 'T']) && s[..s.len() - 1].parse::<u32>().is_ok() => {
            WaitKind::Ticks(s[..s.len() - 1].parse().unwrap_or(0))
        }
        other => {
            let secs = other
                .and_then(|s| s.trim_end_matches(['s', 'S']).parse::<f32>().ok())
                .unwrap_or(1.0)
                .clamp(0.0, WAIT_TIMEOUT_SECS);
            WaitKind::Seconds(secs)
        }
    }
}
