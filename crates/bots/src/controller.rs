use playerstate_iw4::UserCmd;
use sim::{ClientId, ClientLifecycle};

use crate::intent::{BotIntent, MotorReport, MoveMode, PathOutcome};
use crate::memory::Memory;
use crate::motor::Motor;
use crate::nav::{self, NavGraph, PathError};
use crate::observation::{BotEvent, BotObservation, Contact, ModeObjective};
use crate::query::{SightSample, WalkSample, WorldQuery};
use crate::task::{SwitchReason, Task, TaskKind};

const THINK_PERIOD_TICKS: u32 = 2;
const MIN_HOLD_THINKS: u32 = 2;
const STEP_IN: f32 = 64.0;
const WAYPOINT_IN: f32 = 24.0;
const LAST_SEEN_FRESH_TICKS: u32 = 40;
const FIGHT_RING: u32 = 12;

#[derive(Clone, Debug)]
pub struct HostController {
    aggression: f32,
    self_preserve: f32,
    memory: Memory,
    task: Task,
    intent: BotIntent,
    motor: Motor,
    last_think_tick: Option<u32>,
    reaction_until: u32,
    last_target: Option<ClientId>,
    committed: Vec<[f32; 3]>,
    roam: Option<[f32; 3]>,
    roam_gen: u32,
    held_ticks: u32,
    prev_seen: Vec<ClientId>,
    was_touching: bool,
    last_cmd: UserCmd,
    report: MotorReport,
}

impl HostController {
    pub fn new(seed: u64) -> Self {
        let aggression = ((seed & 0xff) as f32) / 255.0;
        let self_preserve = (((seed >> 8) & 0xff) as f32) / 255.0;
        Self {
            aggression,
            self_preserve,
            memory: Memory::default(),
            task: Task::default(),
            intent: BotIntent::default(),
            motor: Motor::new(seed),
            last_think_tick: None,
            reaction_until: 0,
            last_target: None,
            committed: Vec::new(),
            roam: None,
            roam_gen: 0,
            held_ticks: 0,
            prev_seen: Vec::new(),
            was_touching: false,
            last_cmd: UserCmd::default(),
            report: MotorReport::Executing,
        }
    }

    pub fn task(&self) -> Task {
        self.task
    }

    pub fn intent(&self) -> BotIntent {
        self.intent
    }

    pub fn report(&self) -> MotorReport {
        self.report
    }

    pub fn last_cmd(&self) -> UserCmd {
        self.last_cmd
    }

    pub fn drive(
        &mut self,
        obs: &BotObservation,
        world: &mut impl WorldQuery,
        dt_ms: i32,
    ) -> UserCmd {
        self.drive_nav(obs, world, None, &mut 512, dt_ms)
    }

    pub fn drive_nav(
        &mut self,
        obs: &BotObservation,
        world: &mut impl WorldQuery,
        nav: Option<&NavGraph>,
        astar_budget: &mut u32,
        dt_ms: i32,
    ) -> UserCmd {
        if obs.self_state.lifecycle != ClientLifecycle::Alive {
            self.task = Task {
                kind: TaskKind::Hunt,
                reason: SwitchReason::Died,
            };
            self.intent = BotIntent::default();
            self.committed.clear();
            self.last_cmd = UserCmd {
                server_time: obs.time_ms,
                weapon: obs.self_state.weapon,
                weapon_mapped: obs.self_state.weapon,
                ..UserCmd::default()
            };
            return self.last_cmd;
        }
        if self.should_think(obs.tick, obs.self_state.id.0) || self.urgent_think(obs) {
            self.think(obs, world, nav, astar_budget);
            self.last_think_tick = Some(obs.tick);
        }
        let cmd = self.motor.drive(obs, &self.intent, dt_ms);
        self.report = self.motor.report(obs, &self.intent, &cmd);
        self.last_cmd = cmd;
        cmd
    }

    fn should_think(&self, tick: u32, id: u32) -> bool {
        match self.last_think_tick {
            None => true,
            Some(last) if last == tick => false,
            Some(_) => (tick.wrapping_add(id)).is_multiple_of(THINK_PERIOD_TICKS),
        }
    }

    fn urgent_think(&self, obs: &BotObservation) -> bool {
        if self.last_think_tick == Some(obs.tick) {
            return false;
        }
        obs.events_since(&self.prev_seen)
            .iter()
            .any(|event| matches!(event, BotEvent::Spotted { .. }))
    }

    fn think(
        &mut self,
        obs: &BotObservation,
        world: &mut impl WorldQuery,
        nav: Option<&NavGraph>,
        astar_budget: &mut u32,
    ) {
        let started = std::time::Instant::now();
        let events = obs.events_since(&self.prev_seen);
        self.memory.ingest(obs);
        let objective = choose_objective(obs, nav, astar_budget);
        let next = self.commit_task(self.pick_task(obs, objective));
        let switched = next.kind != self.task.kind;
        if switched {
            self.committed.clear();
            if next.kind != TaskKind::Hunt {
                self.roam = None;
            }
        }
        self.task = next;
        self.intent = self.intent_for(obs, world, nav, astar_budget, objective);
        if (self.intent.path == PathOutcome::Unreachable
            || self.intent.path == PathOutcome::Blocked)
            && (self.task.kind == TaskKind::TouchObj || self.task.kind == TaskKind::Investigate)
        {
            self.task.reason = SwitchReason::PathFailed;
        }
        let touching_now = objective.is_some_and(|obj| obj.touching);
        if self.task.kind == TaskKind::TouchObj && self.was_touching && !touching_now {
            self.intent.path = PathOutcome::ProgressLost;
            self.task.reason = SwitchReason::ProgressLost;
        }
        self.was_touching = touching_now && self.task.kind == TaskKind::TouchObj;
        self.prev_seen = self
            .memory
            .currently_seen()
            .map(|contact| contact.id)
            .collect();
        if switched {
            let left = self
                .intent
                .move_goal
                .map(|goal| dist2(obs.self_state.origin, goal).sqrt())
                .unwrap_or(0.0);
            let know = self
                .memory
                .currently_seen()
                .next()
                .or_else(|| self.memory.last_seen().next())
                .map(|c| c.source);
            diag::info!(
                Sim,
                "bots: id={} tick={} task={:?} reason={:?} path={:?} know={know:?} origin=({:.0},{:.0},{:.0}) left={left:.0} events={events:?} think_us={} report={:?}",
                obs.self_state.id.0,
                obs.tick,
                self.task.kind,
                self.task.reason,
                self.intent.path,
                obs.self_state.origin[0],
                obs.self_state.origin[1],
                obs.self_state.origin[2],
                started.elapsed().as_micros(),
                self.report,
            );
        }
    }

    fn commit_task(&mut self, next: Task) -> Task {
        if next.kind == self.task.kind {
            self.held_ticks = self.held_ticks.saturating_add(1);
            return next;
        }
        let allow = hard_interrupt(next.reason)
            || self.held_ticks == 0
            || self.held_ticks >= MIN_HOLD_THINKS;
        if !allow {
            self.held_ticks = self.held_ticks.saturating_add(1);
            return self.task;
        }
        self.held_ticks = 0;
        next
    }

    fn pick_task(&self, obs: &BotObservation, objective: Option<ModeObjective>) -> Task {
        if self.wounded(obs) && self.memory.currently_seen().next().is_none() {
            return Task {
                kind: TaskKind::Recover,
                reason: SwitchReason::LowHealth,
            };
        }
        if self.memory.currently_seen().next().is_some() {
            return Task {
                kind: TaskKind::Fight,
                reason: SwitchReason::SawEnemy,
            };
        }
        if let Some(c) = self.memory.last_seen().next()
            && obs.tick.saturating_sub(c.seen_tick) <= LAST_SEEN_FRESH_TICKS
        {
            return Task {
                kind: TaskKind::Investigate,
                reason: SwitchReason::LostSight,
            };
        }
        if objective.is_some() {
            return Task {
                kind: TaskKind::TouchObj,
                reason: SwitchReason::ObjectiveReachable,
            };
        }
        if obs.objective.is_some() || !obs.objectives.is_empty() {
            return Task {
                kind: TaskKind::Hunt,
                reason: SwitchReason::PathFailed,
            };
        }
        Task {
            kind: TaskKind::Hunt,
            reason: SwitchReason::Spawned,
        }
    }

    fn intent_for(
        &mut self,
        obs: &BotObservation,
        world: &mut impl WorldQuery,
        nav: Option<&NavGraph>,
        astar_budget: &mut u32,
        objective: Option<ModeObjective>,
    ) -> BotIntent {
        match self.task.kind {
            TaskKind::Fight => self.fight(obs, world, nav, astar_budget),
            TaskKind::Investigate => self.seek(obs, world, nav, astar_budget, SeekKind::LastSeen),
            TaskKind::TouchObj => self.seek(
                obs,
                world,
                nav,
                astar_budget,
                SeekKind::Objective(objective),
            ),
            TaskKind::Recover => self.seek(obs, world, nav, astar_budget, SeekKind::BackOff),
            TaskKind::Hunt => self.hunt(obs, world, nav, astar_budget),
        }
    }

    fn hunt(
        &mut self,
        obs: &BotObservation,
        world: &mut impl WorldQuery,
        nav: Option<&NavGraph>,
        astar_budget: &mut u32,
    ) -> BotIntent {
        let Some(goal) = self.ensure_roam(obs, nav) else {
            return BotIntent {
                reload: empty_clip(obs),
                ..BotIntent::default()
            };
        };
        let (move_mode, path, move_goal) =
            self.route(obs.self_state.origin, goal, world, nav, astar_budget);
        BotIntent {
            look_at: Some(goal),
            move_goal,
            desired_range: None,
            move_mode,
            fire: false,
            use_button: false,
            reload: empty_clip(obs),
            crouch: false,
            path,
        }
    }

    fn ensure_roam(&mut self, obs: &BotObservation, nav: Option<&NavGraph>) -> Option<[f32; 3]> {
        if let Some(goal) = self.roam
            && dist2(obs.self_state.origin, goal) > 64.0 * 64.0
        {
            return Some(goal);
        }
        if self.roam.is_some() {
            self.committed.clear();
        }
        self.roam_gen = self.roam_gen.saturating_add(1);
        let salt = obs.self_state.id.0 ^ self.roam_gen;
        let next = if let Some(graph) = nav.filter(|graph| !graph.is_empty()) {
            nav::roam_node(graph, obs.self_state.origin, salt)
        } else {
            let ang = (salt as f32) * 0.37;
            Some([
                obs.self_state.origin[0] + 160.0 * ang.cos(),
                obs.self_state.origin[1] + 160.0 * ang.sin(),
                obs.self_state.origin[2],
            ])
        };
        self.roam = next;
        next
    }

    fn fight(
        &mut self,
        obs: &BotObservation,
        world: &mut impl WorldQuery,
        nav: Option<&NavGraph>,
        astar_budget: &mut u32,
    ) -> BotIntent {
        let Some(target) = self.pick_fight_target(obs) else {
            return BotIntent::default();
        };
        if self.last_target != Some(target.id) {
            self.last_target = Some(target.id);
            let reaction = (6.0 - self.aggression * 4.0).max(2.0) as u32;
            self.reaction_until = obs.tick.saturating_add(reaction);
        }
        let aim_at = [target.origin[0], target.origin[1], target.origin[2] + 48.0];
        if self.wounded(obs) {
            return self.fight_break(obs, world, nav, astar_budget, target);
        }
        let fire = obs.tick >= self.reaction_until
            && obs.self_state.ammo_clip != 0
            && shot_allowed(
                world.shot_ray(obs.eye(), aim_at, obs.self_state.id),
                target.id,
            );
        let range = dist2(obs.self_state.origin, target.origin).sqrt();
        let hold = self.hold_range(obs);
        let (move_mode, path, move_goal) = if range <= hold {
            self.fight_hold(obs, world, nav, astar_budget, target.origin, target.id)
        } else {
            self.route(
                obs.self_state.origin,
                target.origin,
                world,
                nav,
                astar_budget,
            )
        };
        BotIntent {
            look_at: Some(aim_at),
            move_goal,
            desired_range: Some(hold),
            move_mode,
            fire,
            use_button: false,
            reload: empty_clip(obs) && !fire,
            crouch: false,
            path,
        }
    }

    fn pick_fight_target(&self, obs: &BotObservation) -> Option<Contact> {
        let hold = self.hold_range(obs);
        self.memory
            .currently_seen()
            .max_by(|a, b| {
                fight_score(obs, a, hold, self.last_target).total_cmp(&fight_score(
                    obs,
                    b,
                    hold,
                    self.last_target,
                ))
            })
            .copied()
    }

    fn wounded(&self, obs: &BotObservation) -> bool {
        obs.self_state.health > 0
            && (obs.self_state.health as f32) <= 20.0 + 20.0 * self.self_preserve
    }

    fn hold_range(&self, obs: &BotObservation) -> f32 {
        (obs.self_state.weapon_class.fight_hold() - 40.0 * self.aggression).max(48.0)
    }

    fn fight_break(
        &mut self,
        obs: &BotObservation,
        world: &mut impl WorldQuery,
        nav: Option<&NavGraph>,
        astar_budget: &mut u32,
        target: Contact,
    ) -> BotIntent {
        let aim_at = [target.origin[0], target.origin[1], target.origin[2] + 48.0];
        let away = away_from(obs.self_state.origin, target.origin, 192.0);
        let fire = self.aggression > self.self_preserve
            && obs.tick >= self.reaction_until
            && obs.self_state.ammo_clip != 0
            && shot_allowed(
                world.shot_ray(obs.eye(), aim_at, obs.self_state.id),
                target.id,
            );
        let (move_mode, path, move_goal) =
            self.route(obs.self_state.origin, away, world, nav, astar_budget);
        BotIntent {
            look_at: Some(aim_at),
            move_goal,
            desired_range: None,
            move_mode,
            fire,
            use_button: false,
            reload: empty_clip(obs) && !fire,
            crouch: false,
            path,
        }
    }

    fn fight_hold(
        &mut self,
        obs: &BotObservation,
        world: &mut impl WorldQuery,
        nav: Option<&NavGraph>,
        astar_budget: &mut u32,
        target: [f32; 3],
        target_id: ClientId,
    ) -> (MoveMode, PathOutcome, Option<[f32; 3]>) {
        let here = obs.self_state.origin;
        let aim_at = [target[0], target[1], target[2] + 48.0];
        if shot_allowed(
            world.shot_ray(obs.eye(), aim_at, obs.self_state.id),
            target_id,
        ) {
            return (MoveMode::Hold, PathOutcome::Clear, Some(here));
        }
        let hold = self.hold_range(obs);
        let mut best: Option<(f32, [f32; 3])> = None;
        for i in 0..FIGHT_RING {
            let ang = (i as f32) * (core::f32::consts::TAU / FIGHT_RING as f32);
            let cand = [
                target[0] + hold * ang.cos(),
                target[1] + hold * ang.sin(),
                here[2],
            ];
            let eye = [
                cand[0],
                cand[1],
                here[2]
                    + if obs.self_state.view_height > 1.0 {
                        obs.self_state.view_height
                    } else {
                        60.0
                    },
            ];
            if !shot_allowed(world.shot_ray(eye, aim_at, obs.self_state.id), target_id) {
                continue;
            }
            let threat_eye = [target[0], target[1], target[2] + 60.0];
            let chest = [cand[0], cand[1], cand[2] + 48.0];
            let exposed = return_fire_clear(
                world.shot_ray(threat_eye, chest, target_id),
                obs.self_state.id,
            );
            let score = dist2(here, cand) + if exposed { 6400.0 } else { 0.0 };
            if best.is_some_and(|(best_score, _)| score >= best_score) {
                continue;
            }
            if matches!(world.walk_hull(here, cand), WalkSample::Clear) {
                best = Some((score, cand));
            }
        }
        let Some((_, pos)) = best else {
            return (MoveMode::Hold, PathOutcome::Clear, Some(here));
        };
        if dist2(here, pos) <= WAYPOINT_IN * WAYPOINT_IN {
            return (MoveMode::Hold, PathOutcome::Clear, Some(here));
        }
        self.route(here, pos, world, nav, astar_budget)
    }

    fn seek(
        &mut self,
        obs: &BotObservation,
        world: &mut impl WorldQuery,
        nav: Option<&NavGraph>,
        astar_budget: &mut u32,
        kind: SeekKind,
    ) -> BotIntent {
        let (goal, use_button, look) = match kind {
            SeekKind::LastSeen => {
                let Some(last) = self.memory.last_seen().next().copied() else {
                    return BotIntent::default();
                };
                (last.origin, false, last.origin)
            }
            SeekKind::Objective(obj) => {
                let Some(obj) = obj else {
                    return BotIntent::default();
                };
                (obj.origin, obj.touching && obj.use_button, obj.origin)
            }
            SeekKind::BackOff => {
                let threat = self
                    .memory
                    .last_seen()
                    .next()
                    .or_else(|| self.memory.currently_seen().next())
                    .map(|c| c.origin)
                    .unwrap_or([
                        obs.self_state.origin[0] + 64.0,
                        obs.self_state.origin[1],
                        obs.self_state.origin[2],
                    ]);
                let away = away_from(obs.self_state.origin, threat, 192.0);
                (away, false, threat)
            }
        };
        let (move_mode, path, move_goal) =
            self.route(obs.self_state.origin, goal, world, nav, astar_budget);
        BotIntent {
            look_at: Some(look),
            move_goal,
            desired_range: None,
            move_mode,
            fire: false,
            use_button,
            reload: empty_clip(obs) && !matches!(kind, SeekKind::Objective(_)),
            crouch: matches!(kind, SeekKind::BackOff),
            path,
        }
    }

    fn route(
        &mut self,
        from: [f32; 3],
        to: [f32; 3],
        world: &mut impl WorldQuery,
        nav: Option<&NavGraph>,
        astar_budget: &mut u32,
    ) -> (MoveMode, PathOutcome, Option<[f32; 3]>) {
        if let Some(graph) = nav.filter(|g| !g.is_empty()) {
            return self.route_nav(from, to, world, graph, astar_budget);
        }
        walk_step(from, to, world)
    }

    fn route_nav(
        &mut self,
        from: [f32; 3],
        to: [f32; 3],
        world: &mut impl WorldQuery,
        graph: &NavGraph,
        astar_budget: &mut u32,
    ) -> (MoveMode, PathOutcome, Option<[f32; 3]>) {
        let need = self
            .committed
            .last()
            .is_none_or(|g| dist2(*g, to) > WAYPOINT_IN * WAYPOINT_IN);
        if need {
            match nav::find_path(graph, from, to, astar_budget) {
                Ok(path) => self.committed = path,
                Err(PathError::BudgetExhausted { .. }) => {
                    if self.committed.is_empty() {
                        return (MoveMode::Hold, PathOutcome::BudgetExhausted, Some(to));
                    }
                }
                Err(PathError::Unreachable | PathError::EmptyGraph) => {
                    self.committed.clear();
                    return (MoveMode::Hold, PathOutcome::Unreachable, Some(to));
                }
            }
        }
        while self
            .committed
            .first()
            .is_some_and(|p| dist2(from, *p) <= WAYPOINT_IN * WAYPOINT_IN)
        {
            self.committed.remove(0);
        }
        let Some(&next) = self.committed.first() else {
            if dist_xy(from, to) > WAYPOINT_IN {
                return walk_step(from, to, world);
            }
            return (MoveMode::Hold, PathOutcome::Clear, Some(to));
        };
        match world.walk_hull(from, next) {
            WalkSample::Clear => {
                let mode = if from[2] - next[2] > nav::STEP_Z_IN {
                    MoveMode::Drop
                } else {
                    MoveMode::Walk
                };
                (mode, PathOutcome::Clear, Some(next))
            }
            WalkSample::Blocked if from[2] - next[2] > nav::STEP_Z_IN => {
                (MoveMode::Drop, PathOutcome::Clear, Some(next))
            }
            WalkSample::Blocked => (MoveMode::Hold, PathOutcome::Blocked, Some(next)),
            WalkSample::BudgetExhausted => {
                (MoveMode::Hold, PathOutcome::BudgetExhausted, Some(next))
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SeekKind {
    LastSeen,
    Objective(Option<ModeObjective>),
    BackOff,
}

fn choose_objective(
    obs: &BotObservation,
    nav: Option<&NavGraph>,
    astar_budget: &mut u32,
) -> Option<ModeObjective> {
    let mut candidates = obs.objectives.clone();
    if candidates.is_empty()
        && let Some(obj) = obs.objective
    {
        candidates.push(obj);
    }
    candidates.sort_by(|a, b| {
        dist2(obs.self_state.origin, a.origin).total_cmp(&dist2(obs.self_state.origin, b.origin))
    });
    let Some(graph) = nav.filter(|graph| !graph.is_empty()) else {
        return candidates.first().copied();
    };
    let mut exhausted: Option<ModeObjective> = None;
    for obj in candidates {
        match nav::find_path(graph, obs.self_state.origin, obj.origin, astar_budget) {
            Ok(path) => {
                if path_ends_in_use_volume(&path, obj) {
                    return Some(obj);
                }
            }
            Err(PathError::BudgetExhausted { .. }) => {
                exhausted = Some(obj);
            }
            Err(PathError::Unreachable | PathError::EmptyGraph) => {}
        }
    }
    exhausted
}

fn shot_allowed(sample: SightSample, target: ClientId) -> bool {
    match sample {
        SightSample::Clear => true,
        SightSample::HitPlayer { client, .. } => client == target,
        SightSample::Unknown | SightSample::Blocked { .. } => false,
    }
}

fn return_fire_clear(sample: SightSample, bot: ClientId) -> bool {
    match sample {
        SightSample::Clear => true,
        SightSample::HitPlayer { client, .. } => client == bot,
        SightSample::Unknown | SightSample::Blocked { .. } => false,
    }
}

fn hard_interrupt(reason: SwitchReason) -> bool {
    matches!(
        reason,
        SwitchReason::SawEnemy
            | SwitchReason::LostSight
            | SwitchReason::LowHealth
            | SwitchReason::PathFailed
            | SwitchReason::Died
    )
}

fn fight_score(obs: &BotObservation, contact: &Contact, hold: f32, last: Option<ClientId>) -> f32 {
    let range = dist2(obs.self_state.origin, contact.origin).sqrt();
    let dist = 1.0 - (range - hold).abs() / (hold + 80.0);
    let stick = if last == Some(contact.id) { 0.4 } else { 0.0 };
    dist + stick
}

fn empty_clip(obs: &BotObservation) -> bool {
    obs.self_state.ammo_clip == 0 && obs.self_state.ammo_stock > 0
}

fn away_from(here: [f32; 3], threat: [f32; 3], dist: f32) -> [f32; 3] {
    let dx = here[0] - threat[0];
    let dy = here[1] - threat[1];
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    [
        here[0] + dx / len * dist,
        here[1] + dy / len * dist,
        here[2],
    ]
}

fn path_ends_in_use_volume(path: &[[f32; 3]], obj: ModeObjective) -> bool {
    let Some(&end) = path.last() else {
        return false;
    };
    if (end[2] - obj.origin[2]).abs() > nav::STEP_Z_IN * 2.0 + 8.0 {
        return false;
    }
    let radius = obj.radius.max(WAYPOINT_IN);
    dist_xy(end, obj.origin) <= radius
}

fn walk_step(
    from: [f32; 3],
    to: [f32; 3],
    world: &mut impl WorldQuery,
) -> (MoveMode, PathOutcome, Option<[f32; 3]>) {
    let d = [to[0] - from[0], to[1] - from[1]];
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if len < 8.0 {
        return (MoveMode::Hold, PathOutcome::Clear, Some(to));
    }
    let scale = (STEP_IN / len).min(1.0);
    let step = [from[0] + d[0] * scale, from[1] + d[1] * scale, from[2]];
    match world.walk_hull(from, step) {
        WalkSample::Clear => (MoveMode::Walk, PathOutcome::Clear, Some(to)),
        WalkSample::Blocked if len > STEP_IN => {
            (MoveMode::Hold, PathOutcome::Unreachable, Some(to))
        }
        WalkSample::Blocked => (MoveMode::Hold, PathOutcome::Blocked, Some(to)),
        WalkSample::BudgetExhausted => (MoveMode::Hold, PathOutcome::BudgetExhausted, Some(to)),
    }
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

fn dist_xy(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}
