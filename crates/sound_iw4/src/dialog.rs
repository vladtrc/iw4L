use killcam_iw4::log::Log;
use killcam_iw4::task::{EndOn, Event, Millis, Scheduler, TaskId, Wake, Woke};

use crate::level::{Fleet, Level, Player, is_excluded};
use crate::notify::NotifyKind;
use crate::output::{Alias, ClientId, Output, PersTeam, Team};
use crate::recipes;

pub const LEADER_DIALOG_WAIT_MS: Millis = 3000;

pub const NULL: &str = "null";

const PC_WAIT: u16 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogRequest {
    pub dialog: &'static str,

    pub group: Option<&'static str>,

    pub group_override: bool,
}

impl DialogRequest {
    pub const fn plain(dialog: &'static str) -> Self {
        Self {
            dialog,
            group: None,
            group_override: false,
        }
    }

    pub const fn grouped(dialog: &'static str, group: &'static str) -> Self {
        Self {
            dialog,
            group: Some(group),
            group_override: false,
        }
    }

    pub const fn overriding(dialog: &'static str, group: &'static str) -> Self {
        Self {
            dialog,
            group: Some(group),
            group_override: true,
        }
    }
}

pub mod group {
    pub const STATUS: &str = "status";

    pub const INTROBOOST: &str = "introboost";
}

#[derive(Clone, Copy, Debug)]
pub struct LeaderDialogQueue<const Q: usize, const G: usize> {
    client: ClientId,

    active: Option<Alias>,

    group: Option<&'static str>,

    groups: [Option<(&'static str, &'static str)>; G],

    queue: [Option<&'static str>; Q],
    queue_len: usize,
    dropped: usize,

    chain_team: Option<Team>,
    sched: Scheduler<NotifyKind, 2>,
    task: Option<TaskId>,
}

impl<const Q: usize, const G: usize> LeaderDialogQueue<Q, G> {
    pub const fn new(client: ClientId) -> Self {
        Self {
            client,
            active: None,
            group: None,
            groups: [None; G],
            queue: [None; Q],
            queue_len: 0,
            dropped: 0,
            chain_team: None,
            sched: Scheduler::new(),
            task: None,
        }
    }

    pub const fn client(&self) -> ClientId {
        self.client
    }

    pub const fn active(&self) -> Option<Alias> {
        self.active
    }

    pub const fn playing_group(&self) -> Option<&'static str> {
        self.group
    }

    pub const fn queued(&self) -> usize {
        self.queue_len
    }

    pub const fn dropped(&self) -> usize {
        self.dropped
    }

    pub fn leader_dialog_on_player(
        &mut self,
        now_ms: Millis,
        splitscreen: bool,
        player: &Player,
        req: DialogRequest,
    ) -> Log<Output, 2> {
        debug_assert!(player.client == self.client, "queue belongs to this client");
        let mut out = Log::new();

        if splitscreen {
            return out;
        }

        let Some(pers_team) = player.pers_team else {
            return out;
        };
        let Some(team) = pers_team.playing() else {
            return out;
        };

        let mut dialog = req.dialog;
        if let Some(group) = req.group {
            if self.group == Some(group) {
                if req.group_override {
                    if let Some(active) = self.active {
                        out.push(Output::StopLocalSound {
                            client: self.client,
                            alias: active,
                        });
                    }
                    self.play(now_ms, team, dialog, &mut out);
                }
                return out;
            }

            let had_group_dialog = self.group_slot(group).is_some();
            self.remember_group(group, dialog);
            dialog = group;
            if had_group_dialog {
                return out;
            }
        }

        if self.active.is_none() {
            self.play(now_ms, team, dialog, &mut out);
        } else {
            self.enqueue(dialog);
        }
        out
    }

    pub fn advance(&mut self, now_ms: Millis) -> Log<Output, 1> {
        let mut out = Log::new();
        let events = self.sched.advance(now_ms, &[]);
        for event in events.iter() {
            let Event::Woke {
                id,
                cause: Woke::Deadline,
                ..
            } = event
            else {
                continue;
            };
            if self.task != Some(id) {
                continue;
            }
            self.sched.finish(id);
            self.task = None;

            self.active = None;
            self.group = None;
            if let Some(next) = self.dequeue() {
                let team = self
                    .chain_team
                    .expect("a chain that is draining started with a team");
                self.play(now_ms, team, next, &mut out);
            } else {
                self.chain_team = None;
            }
        }
        out
    }

    pub fn disconnect(&mut self, now_ms: Millis) {
        self.sched.advance(now_ms, &[NotifyKind::Disconnect]);
        self.task = None;
    }

    fn play<const N: usize>(
        &mut self,
        now_ms: Millis,
        team: Team,
        dialog: &'static str,
        out: &mut Log<Output, N>,
    ) {
        self.sched
            .advance(now_ms, &[NotifyKind::PlayLeaderDialogOnPlayer]);
        self.task = None;

        let mut dialog = dialog;
        if let Some(line) = self.take_group_dialog(dialog) {
            self.group = Some(dialog);
            dialog = line;
        }

        let line = recipes::dialog_line(dialog).unwrap_or(dialog);

        if line.contains(NULL) {
            return;
        }

        let alias = Alias::Voice { team, line };
        self.active = Some(alias);
        self.chain_team = Some(team);
        out.push(Output::PlayLocalSound {
            client: self.client,
            alias,
        });

        let endon = EndOn::none()
            .on(NotifyKind::Disconnect)
            .on(NotifyKind::PlayLeaderDialogOnPlayer);
        self.task = Some(
            self.sched
                .spawn(
                    PC_WAIT,
                    Wake::Deadline(now_ms + LEADER_DIALOG_WAIT_MS),
                    endon,
                )
                .expect("the previous dialog thread was ended, so a slot is free"),
        );
    }

    fn group_slot(&self, group: &str) -> Option<usize> {
        self.groups
            .iter()
            .position(|slot| slot.is_some_and(|(g, _)| g == group))
    }

    fn remember_group(&mut self, group: &'static str, dialog: &'static str) {
        if let Some(i) = self.group_slot(group) {
            self.groups[i] = Some((group, dialog));
            return;
        }
        match self.groups.iter().position(Option::is_none) {
            Some(i) => self.groups[i] = Some((group, dialog)),
            None => self.dropped += 1,
        }
    }

    fn take_group_dialog(&mut self, group: &str) -> Option<&'static str> {
        let i = self.group_slot(group)?;
        let (_, dialog) = self.groups[i].take()?;
        Some(dialog)
    }

    fn enqueue(&mut self, dialog: &'static str) {
        if self.queue_len == Q {
            self.dropped += 1;
            return;
        }
        self.queue[self.queue_len] = Some(dialog);
        self.queue_len += 1;
    }

    fn dequeue(&mut self) -> Option<&'static str> {
        if self.queue_len == 0 {
            return None;
        }
        let head = self.queue[0];
        for i in 1..self.queue_len {
            self.queue[i - 1] = self.queue[i];
        }
        self.queue_len -= 1;
        self.queue[self.queue_len] = None;
        head
    }
}

pub type PlayerDialog = LeaderDialogQueue<8, 4>;

fn ask<const N: usize>(
    now_ms: Millis,
    splitscreen: bool,
    fleet: &mut Fleet,
    i: usize,
    req: DialogRequest,
    out: &mut Log<Output, N>,
) {
    let player = fleet.players[i];
    let emitted = fleet.queues[i].leader_dialog_on_player(now_ms, splitscreen, &player, req);
    for o in emitted.iter() {
        out.push(o);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Broadcast<'a> {
    pub dialog: &'static str,

    pub team: Option<Team>,
    pub group: Option<&'static str>,
    pub exclude: &'a [ClientId],
}

impl Broadcast<'static> {
    pub const fn everyone(dialog: &'static str) -> Self {
        Self {
            dialog,
            team: None,
            group: None,
            exclude: &[],
        }
    }

    pub const fn to_team(dialog: &'static str, team: Team) -> Self {
        Self {
            dialog,
            team: Some(team),
            group: None,
            exclude: &[],
        }
    }
}

pub fn leader_dialog<const N: usize>(
    now_ms: Millis,
    level: &Level,
    fleet: &mut Fleet,
    call: Broadcast,
    out: &mut Log<Output, N>,
) {
    fleet.check();

    if level.splitscreen {
        return;
    }

    if call.dialog == NULL {
        return;
    }
    for i in 0..fleet.players.len() {
        let Some(pers_team) = fleet.players[i].pers_team else {
            continue;
        };
        if is_excluded(fleet.players[i].client, call.exclude) {
            continue;
        }
        let wanted = match call.team {
            Some(t) => pers_team == to_pers(t),
            None => matches!(pers_team, PersTeam::Allies | PersTeam::Axis),
        };
        if !wanted {
            continue;
        }
        let req = DialogRequest {
            dialog: call.dialog,
            group: call.group,
            group_override: false,
        };
        ask(now_ms, level.splitscreen, fleet, i, req, out);
    }
}

pub fn leader_dialog_on_players<const N: usize>(
    now_ms: Millis,
    level: &Level,
    fleet: &mut Fleet,
    targets: &[ClientId],
    dialog: &'static str,
    out: &mut Log<Output, N>,
) {
    fleet.check();
    for client in targets {
        let i = fleet
            .index_of(*client)
            .expect("target is a connected player");
        ask(
            now_ms,
            level.splitscreen,
            fleet,
            i,
            DialogRequest::plain(dialog),
            out,
        );
    }
}

pub fn leader_dialog_on_one<const N: usize>(
    now_ms: Millis,
    level: &Level,
    fleet: &mut Fleet,
    client: ClientId,
    dialog: &'static str,
    out: &mut Log<Output, N>,
) {
    leader_dialog_on_one_req(
        now_ms,
        level,
        fleet,
        client,
        DialogRequest::plain(dialog),
        out,
    );
}

pub fn leader_dialog_on_one_grouped<const N: usize>(
    now_ms: Millis,
    level: &Level,
    fleet: &mut Fleet,
    client: ClientId,
    dialog: &'static str,
    group: &'static str,
    out: &mut Log<Output, N>,
) {
    leader_dialog_on_one_req(
        now_ms,
        level,
        fleet,
        client,
        DialogRequest::grouped(dialog, group),
        out,
    );
}

fn leader_dialog_on_one_req<const N: usize>(
    now_ms: Millis,
    level: &Level,
    fleet: &mut Fleet,
    client: ClientId,
    req: DialogRequest,
    out: &mut Log<Output, N>,
) {
    fleet.check();
    let i = fleet
        .index_of(client)
        .expect("target is a connected player");
    ask(now_ms, level.splitscreen, fleet, i, req, out);
}

pub fn advance_all<const N: usize>(
    now_ms: Millis,
    fleet: &mut Fleet<'_>,
    out: &mut Log<Output, N>,
) {
    for queue in fleet.queues.iter_mut() {
        for o in queue.advance(now_ms).iter() {
            out.push(o);
        }
    }
}

const fn to_pers(team: Team) -> PersTeam {
    match team {
        Team::Allies => PersTeam::Allies,
        Team::Axis => PersTeam::Axis,
    }
}
