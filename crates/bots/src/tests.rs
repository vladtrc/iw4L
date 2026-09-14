use playerstate_iw4::PlayerState;
use sim::{
    AuthorityDObjCollision, AuthorityDObjCollisionBone, AuthorityDObjState, AuthorityModelOwner,
    BulletTraceQuery, CONTENTS_SOLID, ClientAction, ClientId, ClientLifecycle, ClientSnapshotMeta,
    ClipStaticModel, EntityCollisionCapabilities, MASK_SHOT, MATCH_TICK_MS, MatchBootstrap,
    MatchPhase, ScriptModelId, SimBrush, SimClipBsp, SimClipCmodels, SimClipMesh,
    SimContentBuilder, SimStaticModel, SimWorld, Snapshot, StepReason, Tick, TickInput, XModelColl,
    XModelCollSurf, XModelCollTri,
};

use crate::MAX_HOST_BOTS;
use crate::controller::HostController;
use crate::intent::{BotIntent, MotorReport, MoveMode, PathOutcome};
use crate::motor::Motor;
use crate::observation::{
    BotObservation, Contact, KnowledgeSource, ModeObjective, SelfState, WeaponClass,
};
use crate::query::{Budgeted, ObstacleKind, SightSample, TraceBudget, WalkSample, WorldQuery};
use crate::roster::BotRoster;
use crate::sensor::observe;
use crate::support::{Traversal, V1_TELLS};
use crate::task::{SwitchReason, TaskKind};

struct ScriptedWorld {
    sight: SightSample,
    shot: SightSample,
    walk: WalkSample,
}

impl WorldQuery for ScriptedWorld {
    fn sight_ray(&mut self, _start: [f32; 3], _end: [f32; 3], _ignore: ClientId) -> SightSample {
        self.sight
    }

    fn shot_ray(&mut self, _start: [f32; 3], _end: [f32; 3], _ignore: ClientId) -> SightSample {
        self.shot
    }

    fn hull_trace(&mut self, start: [f32; 3], end: [f32; 3]) -> crate::query::HullTrace {
        if self.walk == WalkSample::Clear {
            crate::query::HullTrace {
                fraction: 1.0,
                endpos: end,
                startsolid: false,
            }
        } else {
            crate::query::HullTrace {
                fraction: 0.0,
                endpos: start,
                startsolid: false,
            }
        }
    }

    fn walk_hull(&mut self, _start: [f32; 3], _end: [f32; 3]) -> WalkSample {
        self.walk
    }
}

fn alive_meta(team: i32, clip: i32, stock: i32) -> ClientSnapshotMeta {
    ClientSnapshotMeta {
        lifecycle: ClientLifecycle::Alive,
        client_state_team: team,
        ammo_clip: clip,
        ammo_stock: stock,
        ..ClientSnapshotMeta::default()
    }
}

fn player(origin: [f32; 3], yaw: f32) -> PlayerState {
    let mut ps = PlayerState::ZERO;
    ps.origin = origin;
    ps.viewangles = [0.0, yaw, 0.0];
    ps.view_height_current = 60.0;
    ps.health = 100;
    ps.weapon = 1;
    ps
}

fn snapshot_two(
    bot_at: [f32; 3],
    bot_yaw: f32,
    visible_at: [f32; 3],
    hidden_at: [f32; 3],
) -> Snapshot {
    let mut snap = Snapshot::unpublished(Tick(8));
    snap.players = vec![
        (ClientId(1), player(bot_at, bot_yaw)),
        (ClientId(2), player(visible_at, 180.0)),
        (ClientId(3), player(hidden_at, 0.0)),
    ];
    snap.meta.clients = vec![
        (ClientId(1), alive_meta(0, 30, 90)),
        (ClientId(2), alive_meta(0, 30, 90)),
        (ClientId(3), alive_meta(0, 30, 90)),
    ];
    snap
}

fn aabb(mins: [f32; 3], maxs: [f32; 3]) -> SimBrush {
    SimBrush {
        planes: vec![
            [1.0, 0.0, 0.0, maxs[0]],
            [-1.0, 0.0, 0.0, -mins[0]],
            [0.0, 1.0, 0.0, maxs[1]],
            [0.0, -1.0, 0.0, -mins[1]],
            [0.0, 0.0, 1.0, maxs[2]],
            [0.0, 0.0, -1.0, -mins[2]],
        ],
        contents: 1,
        plane_surface_flags: vec![0; 6],
        glass_encoded: 0,
    }
}

fn self_obs(
    origin: [f32; 3],
    seen: Vec<Contact>,
    objective: Option<ModeObjective>,
) -> BotObservation {
    BotObservation {
        tick: 8,
        time_ms: 400,
        self_state: SelfState {
            id: ClientId(1),
            lifecycle: ClientLifecycle::Alive,
            origin,
            viewangles: [0.0, 0.0, 0.0],
            view_height: 60.0,
            stance: crate::observation::Stance::Stand,
            health: 100,
            weapon: 1,
            weapon_class: crate::observation::WeaponClass::Assault,
            ammo_clip: 30,
            ammo_stock: 90,
            team: 0,
        },
        seen,
        events: Vec::new(),
        objective,
        objectives: objective.into_iter().collect(),
    }
}

#[test]
fn hidden_enemy_is_absent_from_observation() {
    let snap = snapshot_two([0.0, 0.0, 0.0], 0.0, [200.0, 0.0, 0.0], [0.0, 800.0, 0.0]);
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let obs = observe(&snap, ClientId(1), &mut world).unwrap();
    assert!(obs.seen.iter().any(|c| c.id == ClientId(2)));
    assert!(obs.seen.iter().all(|c| c.id != ClientId(3)));
}

#[test]
fn hidden_positions_do_not_change_commands() {
    let a = snapshot_two([0.0, 0.0, 0.0], 0.0, [200.0, 0.0, 0.0], [0.0, 800.0, 0.0]);
    let b = snapshot_two([0.0, 0.0, 0.0], 0.0, [200.0, 0.0, 0.0], [0.0, -1200.0, 0.0]);
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let oa = observe(&a, ClientId(1), &mut world).unwrap();
    let ob = observe(&b, ClientId(1), &mut world).unwrap();
    assert_eq!(oa.seen, ob.seen);
    let mut ca = HostController::new(1);
    let mut cb = HostController::new(1);
    let cmd_a = ca.drive(&oa, &mut world, 50);
    let cmd_b = cb.drive(&ob, &mut world, 50);
    assert_eq!(cmd_a.forwardmove, cmd_b.forwardmove);
    assert_eq!(cmd_a.rightmove, cmd_b.rightmove);
    assert_eq!(cmd_a.buttons, cmd_b.buttons);
    assert_eq!(cmd_a.angles, cmd_b.angles);
}

#[test]
fn wounded_fight_breaks_contact() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let seen = Contact {
        id: ClientId(2),
        origin: [200.0, 0.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 8,
        confidence: 1.0,
    };
    let mut obs = self_obs([0.0, 0.0, 0.0], vec![seen], None);
    obs.self_state.health = 25;
    let mut bot = HostController::new(0xff00);
    let cmd = bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Fight);
    assert_eq!(bot.task().reason, SwitchReason::SawEnemy);
    assert!(
        cmd.forwardmove < 0,
        "wounded fight walked into the enemy, fwd={}",
        cmd.forwardmove
    );
}

#[test]
fn recover_walks_away_and_reloads() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let seen = Contact {
        id: ClientId(2),
        origin: [200.0, 0.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 8,
        confidence: 1.0,
    };
    let mut obs = self_obs([0.0, 0.0, 0.0], vec![seen], None);
    let mut bot = HostController::new(0xff00);
    bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Fight);
    obs.tick = 9;
    obs.seen.clear();
    obs.self_state.health = 25;
    obs.self_state.ammo_clip = 0;
    let cmd = bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Recover);
    assert_eq!(bot.task().reason, SwitchReason::LowHealth);
    assert_ne!(cmd.buttons & playerstate_iw4::buttons::RELOAD, 0);
    assert_ne!(cmd.buttons & playerstate_iw4::buttons::CROUCH, 0);
    assert!(
        cmd.forwardmove < 0,
        "recover walked toward last-seen, fwd={}",
        cmd.forwardmove
    );
}

#[test]
fn wall_blocks_shot_mask_sight() {
    let mut build = SimContentBuilder::default();
    build.set_clip_brushes(vec![aabb([96.0, -128.0, 0.0], [104.0, 128.0, 128.0])]);
    let mut world = SimWorld::new();
    world.install_content(build.finish());
    let hit = world.sight_ray([0.0, 0.0, 40.0], [200.0, 0.0, 40.0], ClientId(1));
    match hit {
        SightSample::Blocked {
            obstacle: ObstacleKind::World,
            ..
        } => {}
        other => panic!("expected world block, got {other:?}"),
    }
    let open = world.sight_ray([0.0, 0.0, 40.0], [40.0, 0.0, 40.0], ClientId(1));
    assert_eq!(open, SightSample::Clear);
}

#[test]
fn blocked_step_does_not_beeline() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Blocked,
    };
    let obs = self_obs(
        [0.0, 0.0, 0.0],
        vec![],
        Some(ModeObjective::at([400.0, 0.0, 0.0])),
    );
    let mut bot = HostController::new(7);
    let cmd = bot.drive(&obs, &mut world, 50);
    assert_eq!(cmd.forwardmove, 0);
    assert_eq!(cmd.rightmove, 0);
    assert!(matches!(
        bot.intent().path,
        PathOutcome::Blocked | PathOutcome::Unreachable
    ));
    assert_ne!(bot.intent().move_mode, MoveMode::Walk);
}

#[test]
fn same_tick_does_not_rethink() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let seen = Contact {
        id: ClientId(2),
        origin: [200.0, 0.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 8,
        confidence: 1.0,
    };
    let obs = self_obs([0.0, 0.0, 0.0], vec![seen], None);
    let mut bot = HostController::new(3);
    let first = bot.drive(&obs, &mut world, 50);
    let path = bot.intent().path;
    let second = bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.intent().path, path);
    assert_eq!(first.weapon, second.weapon);
}

#[test]
fn visible_enemy_beats_objective() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let seen = Contact {
        id: ClientId(2),
        origin: [180.0, 0.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 8,
        confidence: 1.0,
    };
    let obs = self_obs(
        [0.0, 0.0, 0.0],
        vec![seen],
        Some(ModeObjective::at([50.0, 0.0, 0.0])),
    );
    let mut bot = HostController::new(4);
    bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Fight);
}

#[test]
fn unknown_sight_does_not_invent_a_shot() {
    let mut world = ScriptedWorld {
        sight: SightSample::Unknown,
        shot: SightSample::Unknown,
        walk: WalkSample::Clear,
    };
    let seen = Contact {
        id: ClientId(2),
        origin: [120.0, 0.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 8,
        confidence: 1.0,
    };
    let obs = self_obs([0.0, 0.0, 0.0], vec![seen], None);
    let mut bot = HostController::new(5);
    let cmd = bot.drive(&obs, &mut world, 50);
    assert_eq!(cmd.buttons & playerstate_iw4::buttons::ATTACK, 0);
}

#[test]
fn last_seen_stays_put_while_hidden() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let seen = Contact {
        id: ClientId(2),
        origin: [200.0, 0.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 8,
        confidence: 1.0,
    };
    let mut bot = HostController::new(9);
    bot.drive(&self_obs([0.0, 0.0, 0.0], vec![seen], None), &mut world, 50);
    let mut hidden = self_obs([0.0, 0.0, 0.0], vec![], None);
    hidden.tick = 9;
    hidden.time_ms = 450;
    bot.drive(&hidden, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Investigate);
}

#[test]
fn budget_exhaust_is_not_a_miss() {
    let mut inner = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let mut budget = TraceBudget::new(0);
    let mut world = Budgeted {
        world: &mut inner,
        budget: &mut budget,
    };
    assert_eq!(
        world.sight_ray([0.0; 3], [1.0, 0.0, 0.0], ClientId(1)),
        SightSample::Unknown
    );
    assert_eq!(
        world.walk_hull([0.0; 3], [64.0, 0.0, 0.0]),
        WalkSample::BudgetExhausted
    );
    assert_eq!(
        world.shot_ray([0.0; 3], [1.0, 0.0, 0.0], ClientId(1)),
        SightSample::Unknown
    );
}

#[test]
fn roster_stops_at_twenty() {
    let mut roster = crate::BotRoster::default();
    let first = roster.add_bots(20);
    assert_eq!(first.len(), MAX_HOST_BOTS as usize);
    assert!(roster.add_bots(3).is_empty());
}

fn floor_world(extra: Vec<SimBrush>) -> SimWorld {
    let mut brushes = vec![aabb([-512.0, -512.0, -16.0], [512.0, 512.0, 0.0])];
    brushes.extend(extra);
    let mut build = SimContentBuilder::default();
    build.set_clip_brushes(brushes);
    let mut world = SimWorld::new();
    world.install_content(build.finish());
    world
}

#[test]
fn glass_stops_shots_but_not_sight() {
    let mut pane = aabb([96.0, -64.0, 0.0], [104.0, 64.0, 80.0]);
    pane.contents = 0x10;
    pane.glass_encoded = 1;
    let mut world = floor_world(vec![pane]);
    let sight = world.sight_ray([0.0, 0.0, 40.0], [200.0, 0.0, 40.0], ClientId(1));
    assert_eq!(sight, SightSample::Clear);
    match world.shot_ray([0.0, 0.0, 40.0], [200.0, 0.0, 40.0], ClientId(1)) {
        SightSample::Blocked {
            obstacle: ObstacleKind::Glass,
            ..
        } => {}
        other => panic!("expected glass shot block, got {other:?}"),
    }
}

#[test]
fn island_goal_is_unreachable() {
    let wall = aabb([96.0, -512.0, 0.0], [104.0, 512.0, 96.0]);
    let mut world = floor_world(vec![wall]);
    let bounds = crate::nav::brush_bounds(world.clip_brushes()).unwrap();
    let graph = crate::nav::bake(&mut world, bounds, 1);
    assert!(
        graph.component.iter().copied().max().unwrap_or(0) >= 1,
        "wall must leave more than one component"
    );
    let mut budget = 2048u32;
    let err = crate::nav::find_path(&graph, [-200.0, 0.0, 0.0], [200.0, 0.0, 0.0], &mut budget);
    assert_eq!(err, Err(crate::nav::PathError::Unreachable));
    let obs = self_obs(
        [-200.0, 0.0, 0.0],
        vec![],
        Some(ModeObjective::at([200.0, 0.0, 0.0])),
    );
    let mut bot = HostController::new(2);
    let mut astar = 2048u32;
    let cmd = bot.drive_nav(&obs, &mut world, Some(&graph), &mut astar, 50);
    assert_eq!(bot.task().kind, TaskKind::Hunt);
    assert_eq!(bot.task().reason, SwitchReason::PathFailed);
    if let Some(goal) = bot.intent().move_goal {
        assert!(goal[0] < 96.0, "hunt roam picked the island {goal:?}");
    }
    assert_eq!(cmd.buttons & playerstate_iw4::buttons::ATTACK, 0);
}

#[test]
fn open_floor_has_a_walk_path() {
    let mut world = floor_world(vec![]);
    let bounds = crate::nav::brush_bounds(world.clip_brushes()).unwrap();
    let graph = crate::nav::bake(&mut world, bounds, 1);
    assert!(!graph.is_empty());
    let mut budget = 2048u32;
    let path = crate::nav::find_path(&graph, [-200.0, 0.0, 0.0], [200.0, 0.0, 0.0], &mut budget)
        .expect("path");
    assert!(path.len() >= 2);
    let obs = self_obs(
        [-200.0, 0.0, 0.0],
        vec![],
        Some(ModeObjective::at([200.0, 0.0, 0.0])),
    );
    let mut bot = HostController::new(3);
    let mut astar = 2048u32;
    let cmd = bot.drive_nav(&obs, &mut world, Some(&graph), &mut astar, 50);
    assert_ne!(cmd.forwardmove, 0);
    assert_eq!(bot.intent().path, PathOutcome::Clear);
}

#[test]
fn budget_exhaust_is_not_unreachable() {
    let mut world = floor_world(vec![]);
    let bounds = crate::nav::brush_bounds(world.clip_brushes()).unwrap();
    let graph = crate::nav::bake(&mut world, bounds, 1);
    let obs = self_obs(
        [-200.0, 0.0, 0.0],
        vec![],
        Some(ModeObjective::at([200.0, 0.0, 0.0])),
    );
    let mut bot = HostController::new(3);
    let mut astar = 0u32;
    let cmd = bot.drive_nav(&obs, &mut world, Some(&graph), &mut astar, 50);
    assert_eq!(bot.intent().path, PathOutcome::BudgetExhausted);
    assert_eq!(cmd.forwardmove, 0);
}

#[test]
fn budget_exhaust_keeps_a_committed_path() {
    let mut world = floor_world(vec![]);
    let bounds = crate::nav::brush_bounds(world.clip_brushes()).unwrap();
    let graph = crate::nav::bake(&mut world, bounds, 1);
    let mut obs = self_obs(
        [-200.0, 0.0, 0.0],
        vec![],
        Some(ModeObjective::at([200.0, 0.0, 0.0])),
    );
    let mut bot = HostController::new(3);
    let mut astar = 2048u32;
    bot.drive_nav(&obs, &mut world, Some(&graph), &mut astar, 50);
    assert_eq!(bot.intent().path, PathOutcome::Clear);
    obs.tick = 9;
    obs.time_ms = 450;
    let mut astar = 0u32;
    let cmd = bot.drive_nav(&obs, &mut world, Some(&graph), &mut astar, 50);
    assert_eq!(bot.intent().path, PathOutcome::Clear);
    assert_ne!(cmd.forwardmove, 0);
}

#[test]
fn fight_with_ammo_does_not_reload() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let seen = Contact {
        id: ClientId(2),
        origin: [120.0, 0.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 8,
        confidence: 1.0,
    };
    let obs = self_obs([0.0, 0.0, 0.0], vec![seen], None);
    let mut bot = HostController::new(5);
    let cmd = bot.drive(&obs, &mut world, 50);
    assert_eq!(cmd.buttons & playerstate_iw4::buttons::RELOAD, 0);
}

#[test]
fn fire_waits_until_the_weapon_points_at_the_target() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let seen = Contact {
        id: ClientId(2),
        origin: [200.0, 0.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 8,
        confidence: 1.0,
    };
    let mut obs = self_obs([0.0, 0.0, 0.0], vec![seen], None);
    obs.self_state.viewangles = [0.0, 90.0, 0.0];
    let mut bot = HostController::new(5);
    bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Fight);
    obs.tick = 20;
    let cmd = bot.drive(&obs, &mut world, 50);
    assert_eq!(
        cmd.buttons & playerstate_iw4::buttons::ATTACK,
        0,
        "shot before the muzzle finished turning"
    );
    let mut fired = false;
    for tick in 21..80 {
        obs.tick = tick;
        let cmd = bot.drive(&obs, &mut world, 50);
        if cmd.buttons & playerstate_iw4::buttons::ATTACK != 0 {
            fired = true;
            break;
        }
    }
    assert!(fired, "aimed weapon never fired");
}

#[test]
fn sniper_holds_where_a_shotgun_closes() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let seen = Contact {
        id: ClientId(2),
        origin: [200.0, 0.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 8,
        confidence: 1.0,
    };
    let mut sniper = self_obs([0.0, 0.0, 0.0], vec![seen], None);
    sniper.self_state.weapon_class = WeaponClass::Sniper;
    let mut bot = HostController::new(1);
    bot.drive(&sniper, &mut world, 50);
    assert_eq!(bot.intent().move_mode, MoveMode::Hold);
    assert!(bot.intent().desired_range.unwrap() > 240.0);

    let mut shotgun = self_obs([0.0, 0.0, 0.0], vec![seen], None);
    shotgun.self_state.weapon_class = WeaponClass::Shotgun;
    let mut bot = HostController::new(1);
    let cmd = bot.drive(&shotgun, &mut world, 50);
    assert_eq!(bot.intent().move_mode, MoveMode::Walk);
    assert!(bot.intent().desired_range.unwrap() < 96.0);
    assert_ne!(cmd.forwardmove, 0);
}

#[test]
fn script_name_selects_weapon_class() {
    assert_eq!(
        WeaponClass::from_script_name("cheytac_mp"),
        WeaponClass::Sniper
    );
    assert_eq!(
        WeaponClass::from_script_name("iw4:weapon/striker_mp"),
        WeaponClass::Shotgun
    );
    assert_eq!(WeaponClass::from_script_name("usp_mp"), WeaponClass::Smg);
    assert_eq!(WeaponClass::from_script_name(""), WeaponClass::Assault);
}

#[test]
fn sensor_uses_the_held_weapon_class() {
    let mut build = SimContentBuilder::default();
    build.set_clip_brushes(vec![aabb([-512.0, -512.0, -16.0], [512.0, 512.0, 0.0])]);
    build.set_weapon_script_names(vec![String::new(), String::from("cheytac_mp")]);
    let mut world = SimWorld::new();
    world.install_content(build.finish());
    let snap = snapshot_two([0.0, 0.0, 0.0], 0.0, [200.0, 0.0, 0.0], [0.0, 800.0, 0.0]);
    let obs = observe(&snap, ClientId(1), &mut world).expect("obs");
    assert_eq!(obs.self_state.weapon_class, WeaponClass::Sniper);
    assert!(obs.seen.iter().any(|c| c.id == ClientId(2)));
    let mut bot = HostController::new(1);
    bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.intent().move_mode, MoveMode::Hold);
    assert!(bot.intent().desired_range.unwrap() > 240.0);
}

#[test]
fn empty_clip_reloads_out_of_contact() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let mut obs = self_obs([0.0, 0.0, 0.0], vec![], None);
    obs.self_state.ammo_clip = 0;
    obs.self_state.ammo_stock = 90;
    let mut bot = HostController::new(8);
    let cmd = bot.drive(&obs, &mut world, 50);
    assert_ne!(cmd.buttons & playerstate_iw4::buttons::RELOAD, 0);
}

#[test]
fn hunt_searches_without_an_objective() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let obs = self_obs([0.0, 0.0, 0.0], vec![], None);
    let mut bot = HostController::new(8);
    let cmd = bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Hunt);
    assert_eq!(bot.intent().move_mode, MoveMode::Walk);
    assert!(
        cmd.forwardmove != 0 || cmd.rightmove != 0,
        "hunt stood still, cmd fwd={} right={}",
        cmd.forwardmove,
        cmd.rightmove
    );
}

#[test]
fn spotted_contact_thinks_off_stagger() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let mut obs = self_obs([0.0, 0.0, 0.0], vec![], None);
    let mut bot = HostController::new(8);
    bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Hunt);
    obs.tick = 10;
    obs.seen = vec![Contact {
        id: ClientId(2),
        origin: [180.0, 0.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 10,
        confidence: 1.0,
    }];
    bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Fight);
    assert_eq!(bot.task().reason, SwitchReason::SawEnemy);
}

#[test]
fn min_hold_keeps_hunt_for_one_think() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let mut obs = self_obs([0.0, 0.0, 0.0], vec![], None);
    let mut bot = HostController::new(8);
    bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Hunt);
    obs.tick = 9;
    obs.objective = Some(ModeObjective::at([200.0, 0.0, 0.0]));
    obs.objectives = vec![obs.objective.unwrap()];
    bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::Hunt);
    obs.tick = 11;
    bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::TouchObj);
}

#[test]
fn mantle_intent_is_unsupported() {
    let obs = self_obs([0.0, 0.0, 0.0], vec![], None);
    let intent = BotIntent {
        move_mode: MoveMode::Mantle,
        move_goal: Some([40.0, 0.0, 18.0]),
        path: PathOutcome::Clear,
        ..BotIntent::default()
    };
    let mut motor = Motor::new(1);
    let cmd = motor.drive(&obs, &intent, 50);
    assert_eq!(cmd.forwardmove, 0);
    assert_eq!(motor.report(&obs, &intent, &cmd), MotorReport::Unsupported);
}

#[test]
fn drop_intent_walks_off() {
    let obs = self_obs([0.0, 0.0, 80.0], vec![], None);
    let intent = BotIntent {
        move_mode: MoveMode::Drop,
        move_goal: Some([48.0, 0.0, 0.0]),
        path: PathOutcome::Clear,
        ..BotIntent::default()
    };
    let mut motor = Motor::new(1);
    let cmd = motor.drive(&obs, &intent, 50);
    assert_ne!(cmd.forwardmove, 0);
    assert_eq!(motor.report(&obs, &intent, &cmd), MotorReport::Executing);
}

#[test]
fn leaving_a_flag_is_progress_lost_not_blocked() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let mut obs = self_obs(
        [0.0, 0.0, 0.0],
        vec![],
        Some(ModeObjective {
            origin: [200.0, 0.0, 0.0],
            touching: true,
            use_button: false,
            radius: crate::DEFAULT_OBJECTIVE_RADIUS,
        }),
    );
    let mut bot = HostController::new(4);
    bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::TouchObj);
    obs.tick = 9;
    obs.objective = Some(ModeObjective::at([200.0, 0.0, 0.0]));
    obs.objectives = vec![obs.objective.unwrap()];
    let cmd = bot.drive(&obs, &mut world, 50);
    assert_eq!(bot.task().kind, TaskKind::TouchObj);
    assert_eq!(bot.task().reason, SwitchReason::ProgressLost);
    assert_eq!(bot.intent().path, PathOutcome::ProgressLost);
    assert_ne!(cmd.forwardmove, 0);
    assert_eq!(bot.report(), MotorReport::Executing);
    assert_eq!(bot.last_cmd().forwardmove, cmd.forwardmove);
}

#[test]
fn drop_edge_is_one_way_from_a_ledge() {
    let ledge = aabb([0.0, -48.0, 64.0], [48.0, 48.0, 80.0]);
    let mut world = floor_world(vec![ledge]);
    let bounds = crate::nav::brush_bounds(world.clip_brushes()).unwrap();
    let graph = crate::nav::bake(&mut world, bounds, 1);
    assert!(graph.drops > 0, "ledge bake produced no drop edges");
    let mut high = None;
    let mut low = None;
    for (i, pos) in graph.nodes.iter().enumerate() {
        if pos[2] > 50.0 && pos[0] >= 0.0 && pos[0] <= 48.0 {
            high = Some((i, *pos));
        }
        if pos[2] < 16.0 && pos[0] > 48.0 && pos[0] < 144.0 && pos[1].abs() < 48.0 {
            low = Some((i, *pos));
        }
    }
    let (hi, high_pos) = high.expect("stand node on the ledge");
    let (lo, low_pos) = low.expect("stand node on the floor off the ledge");
    assert_ne!(graph.component[hi], graph.component[lo]);
    let mut budget = 2048u32;
    crate::nav::find_path(&graph, high_pos, low_pos, &mut budget).expect("drop path");
    let mut budget = 2048u32;
    let back = crate::nav::find_path(&graph, low_pos, high_pos, &mut budget);
    assert_eq!(back, Err(crate::nav::PathError::Unreachable));
    let seek = self_obs(high_pos, vec![], Some(ModeObjective::at(low_pos)));
    let mut bot = HostController::new(5);
    let mut astar = 2048u32;
    let cmd = bot.drive_nav(&seek, &mut world, Some(&graph), &mut astar, 50);
    assert_eq!(bot.intent().move_mode, MoveMode::Drop);
    assert_eq!(bot.intent().path, PathOutcome::Clear);
    assert_ne!(cmd.forwardmove, 0);
}

#[test]
fn drop_lands_as_arrived() {
    let obs = self_obs([48.0, 0.0, 0.0], vec![], None);
    let intent = BotIntent {
        move_mode: MoveMode::Drop,
        move_goal: Some([48.0, 0.0, 0.0]),
        path: PathOutcome::Clear,
        ..BotIntent::default()
    };
    let mut motor = Motor::new(1);
    let cmd = motor.drive(&obs, &intent, 50);
    assert_eq!(motor.report(&obs, &intent, &cmd), MotorReport::Arrived);
}

#[test]
fn flag_outside_use_volume_is_not_a_path() {
    let graph = crate::nav::NavGraph {
        digest: 1,
        schema: crate::NAV_SCHEMA,
        hull: crate::NAV_HULL,
        nodes: vec![[0.0, 0.0, 0.0], [48.0, 0.0, 0.0]],
        component: vec![0, 0],
        adj: vec![vec![(1, 48.0)], vec![(0, 48.0)]],
        drops: 0,
    };
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let obs = self_obs(
        [0.0, 0.0, 0.0],
        vec![],
        Some(ModeObjective {
            origin: [240.0, 0.0, 0.0],
            touching: false,
            use_button: false,
            radius: 96.0,
        }),
    );
    let mut bot = HostController::new(6);
    let mut astar = 2048u32;
    let cmd = bot.drive_nav(&obs, &mut world, Some(&graph), &mut astar, 50);
    assert_eq!(bot.task().kind, TaskKind::Hunt);
    assert_eq!(bot.task().reason, SwitchReason::PathFailed);
    assert_eq!(cmd.buttons & playerstate_iw4::buttons::ATTACK, 0);
    if let Some(goal) = bot.intent().move_goal {
        assert!(
            goal[0] < 96.0,
            "walked toward a node outside the use volume {goal:?}"
        );
    }
}

#[test]
fn flag_on_another_floor_is_not_a_path() {
    let graph = crate::nav::NavGraph {
        digest: 1,
        schema: crate::NAV_SCHEMA,
        hull: crate::NAV_HULL,
        nodes: vec![[0.0, 0.0, 0.0], [48.0, 0.0, 0.0]],
        component: vec![0, 0],
        adj: vec![vec![(1, 48.0)], vec![(0, 48.0)]],
        drops: 0,
    };
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let obs = self_obs(
        [0.0, 0.0, 0.0],
        vec![],
        Some(ModeObjective {
            origin: [0.0, 0.0, 80.0],
            touching: false,
            use_button: false,
            radius: 96.0,
        }),
    );
    let mut bot = HostController::new(6);
    let mut astar = 2048u32;
    bot.drive_nav(&obs, &mut world, Some(&graph), &mut astar, 50);
    assert_eq!(bot.task().kind, TaskKind::Hunt);
    assert_eq!(bot.task().reason, SwitchReason::PathFailed);
}

#[test]
fn v1_matrix_is_walk_on_boneyard_and_terminal_is_diagnostic() {
    assert!(
        V1_TELLS
            .iter()
            .any(|t| t.map == "mp_boneyard" && t.mode == "ffa" && t.combat && !t.diagnostic)
    );
    assert!(V1_TELLS.iter().any(|t| {
        t.map == "mp_boneyard" && t.mode == "domination" && t.objective && !t.diagnostic
    }));
    let terminal = V1_TELLS
        .iter()
        .find(|t| t.map == "mp_terminal")
        .expect("terminal row");
    assert!(terminal.diagnostic);
    assert!(!terminal.combat);
    assert!(V1_TELLS.iter().all(|t| t.traversal == Traversal::Walk));
}

#[test]
fn extra_frames_on_one_tick_do_not_rethink() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let obs = self_obs([0.0, 0.0, 0.0], vec![], None);
    let mut bot = HostController::new(11);
    let first = bot.drive(&obs, &mut world, 50);
    for _ in 0..12 {
        let again = bot.drive(&obs, &mut world, 16);
        assert_eq!(again.buttons, first.buttons);
        assert_eq!(again.forwardmove, first.forwardmove);
    }
}

#[test]
fn aim_does_not_snap_across_the_fov_in_one_frame() {
    let mut world = ScriptedWorld {
        sight: SightSample::Clear,
        shot: SightSample::Clear,
        walk: WalkSample::Clear,
    };
    let seen = Contact {
        id: ClientId(2),
        origin: [200.0, 180.0, 0.0],
        source: KnowledgeSource::CurrentlySeen,
        seen_tick: 8,
        confidence: 1.0,
    };
    let mut obs = self_obs([0.0, 0.0, 0.0], vec![seen], None);
    obs.self_state.viewangles = [0.0, 0.0, 0.0];
    let mut bot = HostController::new(5);
    let cmd = bot.drive(&obs, &mut world, 16);
    let mut yaw = cmd.angles[1] as f32 / movement_iw4::ANGLE2SHORT;
    if yaw > 180.0 {
        yaw -= 360.0;
    }
    assert!(
        yaw.abs() > 0.5 && yaw.abs() < 16.0,
        "aim snapped or did not turn: yaw={yaw}"
    );
    obs.tick = 9;
    let again = bot.drive(&obs, &mut world, 16);
    let mut yaw2 = again.angles[1] as f32 / movement_iw4::ANGLE2SHORT;
    if yaw2 > 180.0 {
        yaw2 -= 360.0;
    }
    assert!(
        yaw2.abs() > yaw.abs() + 0.4,
        "aim did not accelerate: yaw={yaw} then {yaw2}"
    );
}

const BOT: ClientId = ClientId(1);
const ENEMY: ClientId = ClientId(2);
const BOT_B: ClientId = ClientId(3);

struct StepScene {
    world: SimWorld,
    graph: crate::nav::NavGraph,
    bot: HostController,
    tick: u32,
}

fn playing_world(extra: Vec<SimBrush>) -> SimWorld {
    playing_world_kind(extra, gamemode_iw4::GameModeKind::FreeForAll)
}

fn playing_world_kind(extra: Vec<SimBrush>, kind: gamemode_iw4::GameModeKind) -> SimWorld {
    let mut world = floor_world(extra);
    world
        .bootstrap(MatchBootstrap {
            allow_debug_actions: true,
            time_limit_ms: 600_000,
            kind,
            ..MatchBootstrap::default()
        })
        .expect("bootstrap");
    world
}

fn begin_scene(extra: Vec<SimBrush>, origin: [f32; 3]) -> StepScene {
    let mut world = playing_world(extra);
    let bounds = crate::nav::brush_bounds(world.clip_brushes()).unwrap();
    let digest = world.content_digest();
    let graph = crate::nav::bake(&mut world, bounds, digest);
    world.debug_place_alive_player(BOT, origin);
    let _ = sim::step(
        &mut world,
        Tick(1),
        &TickInput {
            cmds: Vec::new(),
            actions: vec![(
                BOT,
                ClientAction::SetMatchPhase {
                    request_id: 1,
                    phase: MatchPhase::Playing,
                },
            )],
        },
        MATCH_TICK_MS as i32,
        StepReason::AuthorityFrame,
    );
    StepScene {
        world,
        graph,
        bot: HostController::new(3),
        tick: 1,
    }
}

impl StepScene {
    fn origin(&self) -> [f32; 3] {
        self.world.player(BOT).expect("bot").origin
    }

    fn drive(
        &mut self,
        dt_ms: i32,
    ) -> (playerstate_iw4::UserCmd, crate::observation::BotObservation) {
        let snap = self.world.snapshot(Tick(self.tick));
        let obs = observe(&snap, BOT, &mut self.world).expect("obs");
        let mut astar = 2048u32;
        let mut cmd =
            self.bot
                .drive_nav(&obs, &mut self.world, Some(&self.graph), &mut astar, dt_ms);
        self.tick += 1;
        cmd.server_time = (self.tick as i32) * MATCH_TICK_MS as i32;
        let _ = sim::step(
            &mut self.world,
            Tick(self.tick),
            &TickInput::from_cmds(vec![(BOT, cmd)]),
            dt_ms,
            StepReason::AuthorityFrame,
        );
        (cmd, obs)
    }
}

fn enemy_flag(origin: [f32; 3]) -> sim::ObjectiveView {
    sim::ObjectiveView {
        id: 1,
        model_source: 0,
        label: String::from("A"),
        origin,
        owner: gamemode_iw4::Team::Axis,
        progress: 0.0,
        capturing: gamemode_iw4::Team::Free,
        contested: false,
        users: Vec::new(),
    }
}

#[test]
fn pmove_walks_the_open_floor() {
    let mut scene = begin_scene(vec![], [-200.0, 0.0, 0.0]);
    scene
        .world
        .objectives
        .flags
        .push(enemy_flag([200.0, 0.0, 0.0]));
    let start = scene.origin();
    for _ in 0..120 {
        scene.drive(MATCH_TICK_MS as i32);
        if dist_xy(scene.origin(), [200.0, 0.0, 0.0]) < 48.0 {
            return;
        }
    }
    panic!(
        "bot did not reach the goal via pmove: start={start:?} end={:?}",
        scene.origin()
    );
}

#[test]
fn pmove_drops_off_a_ledge() {
    let ledge = aabb([0.0, -48.0, 64.0], [48.0, 48.0, 80.0]);
    let mut scene = begin_scene(vec![ledge], [16.0, 0.0, 82.0]);
    scene
        .world
        .objectives
        .flags
        .push(enemy_flag([160.0, 0.0, 0.0]));
    let start = scene.origin();
    assert!(
        start[2] > 50.0,
        "bot spawned on the floor instead of the ledge: {start:?}"
    );
    let mut saw_drop = false;
    for _ in 0..160 {
        scene.drive(MATCH_TICK_MS as i32);
        if scene.bot.intent().move_mode == MoveMode::Drop {
            saw_drop = true;
        }
        let now = scene.origin();
        if now[2] < 24.0 && now[0] > 48.0 {
            assert!(
                saw_drop || now[2] < start[2] - 40.0,
                "fell without a Drop intent, start={start:?} end={now:?}"
            );
            return;
        }
    }
    panic!(
        "bot did not drop off the ledge via pmove: start={start:?} end={:?} task={:?} mode={:?} path={:?}",
        scene.origin(),
        scene.bot.task(),
        scene.bot.intent().move_mode,
        scene.bot.intent().path
    );
}

#[test]
fn pmove_hunt_walks_without_a_flag() {
    let mut scene = begin_scene(vec![], [-200.0, 0.0, 0.0]);
    let start = scene.origin();
    for _ in 0..80 {
        scene.drive(MATCH_TICK_MS as i32);
        if dist_xy(scene.origin(), start) > 80.0 {
            assert_eq!(scene.bot.task().kind, TaskKind::Hunt);
            return;
        }
    }
    panic!(
        "hunt did not leave spawn, end={:?} task={:?} path={:?}",
        scene.origin(),
        scene.bot.task(),
        scene.bot.intent().path
    );
}

#[test]
fn pmove_holds_on_an_island() {
    let wall = aabb([96.0, -512.0, 0.0], [104.0, 512.0, 96.0]);
    let mut scene = begin_scene(vec![wall], [-200.0, 0.0, 0.0]);
    scene
        .world
        .objectives
        .flags
        .push(enemy_flag([200.0, 0.0, 0.0]));
    for _ in 0..80 {
        scene.drive(MATCH_TICK_MS as i32);
        let x = scene.origin()[0];
        assert!(
            x < 90.0,
            "beeline into the wall: origin={:?}",
            scene.origin()
        );
    }
    assert_eq!(scene.bot.task().reason, SwitchReason::PathFailed);
}

#[test]
fn pmove_picks_the_reachable_flag_not_the_island() {
    let wall = aabb([96.0, -512.0, 0.0], [104.0, 512.0, 96.0]);
    let mut scene = begin_scene(vec![wall], [-200.0, 0.0, 0.0]);
    let mut island = enemy_flag([200.0, 0.0, 0.0]);
    island.id = 1;
    let mut reachable = enemy_flag([-200.0, 400.0, 0.0]);
    reachable.id = 2;
    scene.world.objectives.flags.push(island);
    scene.world.objectives.flags.push(reachable);
    for _ in 0..160 {
        scene.drive(MATCH_TICK_MS as i32);
        let now = scene.origin();
        assert!(now[0] < 90.0, "walked onto the island: {now:?}");
        if dist_xy(now, [-200.0, 400.0, 0.0]) < 64.0 {
            return;
        }
    }
    panic!(
        "did not take the reachable flag, end={:?} task={:?}",
        scene.origin(),
        scene.bot.task()
    );
}

#[test]
fn pmove_sees_then_loses_an_enemy() {
    let wall = aabb([96.0, -128.0, 0.0], [104.0, 128.0, 96.0]);
    let mut scene = begin_scene(vec![wall], [-80.0, 0.0, 0.0]);
    scene
        .world
        .debug_place_alive_player(ENEMY, [200.0, 0.0, 0.0]);
    for _ in 0..8 {
        let (_, obs) = scene.drive(MATCH_TICK_MS as i32);
        assert!(
            obs.seen.iter().all(|c| c.id != ENEMY),
            "telepathy through the wall"
        );
        assert_ne!(scene.bot.task().kind, TaskKind::Fight);
    }
    scene.world.set_origin(ENEMY, [40.0, 0.0, 0.0]);
    scene.world.set_viewangles(BOT, [0.0, 0.0, 0.0]);
    let mut saw = false;
    for _ in 0..12 {
        scene.drive(MATCH_TICK_MS as i32);
        if scene.bot.task().kind == TaskKind::Fight {
            saw = true;
            break;
        }
    }
    assert!(saw, "visible enemy never became Fight");
    scene.world.set_origin(ENEMY, [200.0, 0.0, 0.0]);
    for _ in 0..8 {
        scene.drive(MATCH_TICK_MS as i32);
        if scene.bot.task().kind == TaskKind::Investigate {
            return;
        }
    }
    panic!(
        "lost LOS did not become Investigate, task={:?}",
        scene.bot.task().kind
    );
}

#[test]
fn linked_model_blocks_sight() {
    let mut world = floor_world(vec![]);
    let mut dobj = AuthorityDObjState::new_dirty(String::from("box"), None, glam::Mat4::IDENTITY);
    dobj.current_collision = Some(AuthorityDObjCollision {
        bones: vec![AuthorityDObjCollisionBone {
            bone: 0,
            part_classification: 0,
            center: [100.0, 0.0, 40.0],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            half_size: [8.0, 40.0, 40.0],
        }],
        coll: None,
    });
    world.install_entity_collision_capabilities(vec![EntityCollisionCapabilities::current_tick(
        AuthorityModelOwner::ScriptModel(ScriptModelId::from_wire(1)),
        Some(dobj),
        Vec::new(),
    )]);
    match world.sight_ray([0.0, 0.0, 40.0], [200.0, 0.0, 40.0], BOT) {
        SightSample::Blocked {
            obstacle: ObstacleKind::StaticOrLinked,
            ..
        } => {}
        other => panic!("expected linked block, got {other:?}"),
    }
    let open = world.sight_ray([0.0, 0.0, 40.0], [40.0, 0.0, 40.0], BOT);
    assert_eq!(open, SightSample::Clear);
}

#[test]
fn short_msec_still_walks_on_authority_ticks() {
    let mut scene = begin_scene(vec![], [-200.0, 0.0, 0.0]);
    scene
        .world
        .objectives
        .flags
        .push(enemy_flag([200.0, 0.0, 0.0]));
    let start = scene.origin()[0];
    for _ in 0..40 {
        scene.drive(16);
    }
    assert!(
        scene.origin()[0] > start + 40.0,
        "16ms steps on authority ticks did not advance: {:?}",
        scene.origin()
    );
}

fn dist_xy(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

#[test]
fn pmove_fight_aims_shoots_then_reloads_out_of_contact() {
    let mut scene = begin_scene(vec![], [0.0, 0.0, 0.0]);
    scene.bot = HostController::new(0xff);
    scene.world.debug_set_held_ammo(BOT, 30, 90);
    scene
        .world
        .debug_place_alive_player(ENEMY, [80.0, 0.0, 0.0]);
    scene.world.set_viewangles(BOT, [0.0, 0.0, 0.0]);
    let mut fired = false;
    for _ in 0..24 {
        let (cmd, _) = scene.drive(MATCH_TICK_MS as i32);
        if scene.bot.task().kind == TaskKind::Fight
            && cmd.buttons & playerstate_iw4::buttons::ATTACK != 0
        {
            fired = true;
            break;
        }
    }
    assert!(
        fired,
        "fight never pressed attack, task={:?}",
        scene.bot.task()
    );
    scene.world.set_origin(ENEMY, [0.0, 2000.0, 0.0]);
    let mut lost = false;
    for _ in 0..12 {
        scene.drive(MATCH_TICK_MS as i32);
        if scene.bot.task().kind == TaskKind::Investigate {
            lost = true;
            break;
        }
    }
    assert!(lost, "lost LOS did not investigate");
    scene.world.debug_set_held_ammo(BOT, 0, 90);
    let mut reloaded = false;
    for _ in 0..6 {
        let (cmd, _) = scene.drive(MATCH_TICK_MS as i32);
        if cmd.buttons & playerstate_iw4::buttons::RELOAD != 0 {
            reloaded = true;
            break;
        }
    }
    assert!(reloaded, "empty clip out of contact did not reload");
}

#[test]
fn pmove_objective_does_not_reload_until_the_touch_is_done() {
    let mut scene = begin_scene(vec![], [-200.0, 0.0, 0.0]);
    scene
        .world
        .objectives
        .flags
        .push(enemy_flag([200.0, 0.0, 0.0]));
    scene.world.debug_set_held_ammo(BOT, 0, 90);
    let (cmd, _) = scene.drive(MATCH_TICK_MS as i32);
    assert_eq!(scene.bot.task().kind, TaskKind::TouchObj);
    assert_eq!(
        cmd.buttons & playerstate_iw4::buttons::RELOAD,
        0,
        "objective must not dump the magazine"
    );
    scene.world.objectives.flags.clear();
    let mut reloaded = false;
    for _ in 0..6 {
        let (cmd, _) = scene.drive(MATCH_TICK_MS as i32);
        if cmd.buttons & playerstate_iw4::buttons::RELOAD != 0 {
            reloaded = true;
            break;
        }
    }
    assert!(
        reloaded,
        "after the objective dropped, empty clip did not reload"
    );
}

fn floor_with_smodels(models: Vec<SimStaticModel>) -> SimWorld {
    let mesh = SimClipMesh {
        static_models: models,
        ..SimClipMesh::default()
    };
    let mut build = SimContentBuilder::default();
    build.set_clip_map(
        vec![aabb([-512.0, -512.0, -16.0], [512.0, 512.0, 0.0])],
        SimClipBsp::default(),
        mesh,
        SimClipCmodels::default(),
    );
    let mut world = SimWorld::new();
    world.install_content(build.finish());
    world
}

fn slab_smodel(origin: [f32; 3], tris: Vec<XModelCollTri>) -> SimStaticModel {
    let half = [8.0, 48.0, 48.0];
    SimStaticModel {
        index: 0,
        name: String::from("slab"),
        model: ClipStaticModel {
            origin,
            inv_scaled_axis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            bounds_mid: origin,
            bounds_half: half,
            coll: XModelColl {
                coll_lod: 0,
                contents: CONTENTS_SOLID,
                surfs: vec![XModelCollSurf {
                    tris,
                    midpoint: [0.0, 0.0, 0.0],
                    half_size: half,
                    bone_idx: 0,
                    contents: CONTENTS_SOLID,
                    surf_flags: 0,
                }],
            },
        },
    }
}

fn slab_tris() -> Vec<XModelCollTri> {
    vec![
        XModelCollTri {
            plane: [-1.0, 0.0, 0.0, 0.0],
            svec: [0.0, 1.0 / 80.0, 0.0, -0.5],
            tvec: [0.0, 0.0, 1.0 / 80.0, -0.5],
        },
        XModelCollTri {
            plane: [-1.0, 0.0, 0.0, 0.0],
            svec: [0.0, -1.0 / 80.0, 0.0, -0.5],
            tvec: [0.0, 0.0, -1.0 / 80.0, -0.5],
        },
    ]
}

fn install_linked_box(world: &mut SimWorld) {
    let mut dobj = AuthorityDObjState::new_dirty(String::from("box"), None, glam::Mat4::IDENTITY);
    dobj.current_collision = Some(AuthorityDObjCollision {
        bones: vec![AuthorityDObjCollisionBone {
            bone: 0,
            part_classification: 0,
            center: [100.0, 0.0, 40.0],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            half_size: [8.0, 40.0, 40.0],
        }],
        coll: None,
    });
    world.install_entity_collision_capabilities(vec![EntityCollisionCapabilities::current_tick(
        AuthorityModelOwner::ScriptModel(ScriptModelId::from_wire(1)),
        Some(dobj),
        Vec::new(),
    )]);
}

fn bind_dom_flags(world: &mut SimWorld) {
    let ents = [
        gamemode_iw4::DomFlagMapEnt {
            classname: "trigger_radius",
            targetname: "flag_primary",
            origin: [200.0, 0.0, 0.0],
            angles: [0.0; 3],
            script_label: "_a",
            gameobject: "",
            radius: Some(96.0),
            height: Some(160.0),
        },
        gamemode_iw4::DomFlagMapEnt {
            classname: "trigger_radius",
            targetname: "flag_secondary",
            origin: [-200.0, 400.0, 0.0],
            angles: [0.0; 3],
            script_label: "_b",
            gameobject: "",
            radius: Some(96.0),
            height: Some(160.0),
        },
    ];
    let ids = world.install_dom_flags(&ents).expect("dom flags");
    world.objectives.flags = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let mut flag = enemy_flag(ents[i].origin);
            flag.id = *id;
            flag.owner = gamemode_iw4::Team::Free;
            flag.label = if i == 0 {
                String::from("A")
            } else {
                String::from("B")
            };
            flag
        })
        .collect();
}

fn begin_dom_scene(origin: [f32; 3]) -> StepScene {
    let mut world = playing_world_kind(vec![], gamemode_iw4::GameModeKind::Domination);
    bind_dom_flags(&mut world);
    let bounds = crate::nav::brush_bounds(world.clip_brushes()).unwrap();
    let digest = world.content_digest();
    let graph = crate::nav::bake(&mut world, bounds, digest);
    world.debug_place_alive_player(BOT, origin);
    world.debug_set_team(BOT, entity_iw4::TEAM_ALLIES);
    let _ = sim::step(
        &mut world,
        Tick(1),
        &TickInput {
            cmds: Vec::new(),
            actions: vec![(
                BOT,
                ClientAction::SetMatchPhase {
                    request_id: 1,
                    phase: MatchPhase::Playing,
                },
            )],
        },
        MATCH_TICK_MS as i32,
        StepReason::AuthorityFrame,
    );
    StepScene {
        world,
        graph,
        bot: HostController::new(3),
        tick: 1,
    }
}

#[test]
fn static_model_mesh_blocks_sight() {
    let origin = [100.0, 0.0, 40.0];
    let mut world = floor_with_smodels(vec![slab_smodel(origin, slab_tris())]);
    match world.sight_ray([0.0, 0.0, 40.0], [200.0, 0.0, 40.0], BOT) {
        SightSample::Blocked {
            obstacle: ObstacleKind::World,
            ..
        } => {}
        other => panic!("expected static-model mesh block, got {other:?}"),
    }
    let mut empty = floor_with_smodels(vec![slab_smodel(origin, Vec::new())]);
    assert_eq!(
        empty.sight_ray([0.0, 0.0, 40.0], [200.0, 0.0, 40.0], BOT),
        SightSample::Clear,
        "AABB-only smodel must not stand in for coll tris"
    );
}

#[test]
fn bullet_trace_uses_the_same_linked_geom_as_the_sensor() {
    let mut world = floor_world(vec![]);
    install_linked_box(&mut world);
    match world.sight_ray([0.0, 0.0, 40.0], [200.0, 0.0, 40.0], BOT) {
        SightSample::Blocked {
            obstacle: ObstacleKind::StaticOrLinked,
            ..
        } => {}
        other => panic!("expected linked block, got {other:?}"),
    }
    match world.bullet_trace(
        BulletTraceQuery {
            start: [0.0, 0.0, 40.0],
            end: [200.0, 0.0, 40.0],
            mask: MASK_SHOT,
            ignore: Some(BOT),
            ignore_hit: None,
        },
        None,
    ) {
        sim::TraceOutcome::Hit {
            collider: sim::ColliderId::EntityDObjBone { .. },
            ..
        } => {}
        other => panic!("combat bullet_trace missed the linked geom: {other:?}"),
    }
}

#[test]
fn shared_trace_quota_does_not_invent_a_shot() {
    let mut scene = begin_scene(vec![], [0.0, 0.0, 0.0]);
    scene
        .world
        .debug_place_alive_player(ENEMY, [80.0, 0.0, 0.0]);
    scene
        .world
        .debug_place_alive_player(BOT_B, [0.0, 80.0, 0.0]);
    scene.world.debug_set_held_ammo(BOT, 30, 90);
    scene.world.debug_set_held_ammo(BOT_B, 30, 90);
    let snap = scene.world.snapshot(Tick(scene.tick));
    let mut budget = TraceBudget::new(0);
    let mut queried = Budgeted {
        world: &mut scene.world,
        budget: &mut budget,
    };
    let obs_a = observe(&snap, BOT, &mut queried).unwrap();
    let obs_b = observe(&snap, BOT_B, &mut queried).unwrap();
    assert!(obs_a.seen.is_empty(), "quota 0 invented a contact");
    assert!(obs_b.seen.is_empty(), "quota 0 invented a contact");
    let mut a = HostController::new(5);
    let mut b = HostController::new(7);
    let cmd_a = a.drive(&obs_a, &mut queried, 50);
    let cmd_b = b.drive(&obs_b, &mut queried, 50);
    assert_eq!(cmd_a.buttons & playerstate_iw4::buttons::ATTACK, 0);
    assert_eq!(cmd_b.buttons & playerstate_iw4::buttons::ATTACK, 0);
}

#[test]
fn pmove_two_bots_walk_on_one_authority_tick() {
    let mut world = playing_world(vec![]);
    let bounds = crate::nav::brush_bounds(world.clip_brushes()).unwrap();
    let digest = world.content_digest();
    let graph = crate::nav::bake(&mut world, bounds, digest);
    world.debug_place_alive_player(BOT, [-200.0, -80.0, 0.0]);
    world.debug_place_alive_player(BOT_B, [-200.0, 80.0, 0.0]);
    world.objectives.flags.push(enemy_flag([200.0, 0.0, 0.0]));
    let _ = sim::step(
        &mut world,
        Tick(1),
        &TickInput {
            cmds: Vec::new(),
            actions: vec![(
                BOT,
                ClientAction::SetMatchPhase {
                    request_id: 1,
                    phase: MatchPhase::Playing,
                },
            )],
        },
        MATCH_TICK_MS as i32,
        StepReason::AuthorityFrame,
    );
    let mut a = HostController::new(3);
    let mut b = HostController::new(4);
    let mut tick = 1u32;
    for _ in 0..160 {
        let snap = world.snapshot(Tick(tick));
        let mut astar = 2048u32;
        let obs_a = observe(&snap, BOT, &mut world).expect("obs a");
        let mut cmd_a = a.drive_nav(&obs_a, &mut world, Some(&graph), &mut astar, 50);
        let obs_b = observe(&snap, BOT_B, &mut world).expect("obs b");
        let mut cmd_b = b.drive_nav(&obs_b, &mut world, Some(&graph), &mut astar, 50);
        tick += 1;
        cmd_a.server_time = (tick as i32) * MATCH_TICK_MS as i32;
        cmd_b.server_time = (tick as i32) * MATCH_TICK_MS as i32;
        let _ = sim::step(
            &mut world,
            Tick(tick),
            &TickInput::from_cmds(vec![(BOT, cmd_a), (BOT_B, cmd_b)]),
            MATCH_TICK_MS as i32,
            StepReason::AuthorityFrame,
        );
        let oa = world.player(BOT).expect("a").origin;
        let ob = world.player(BOT_B).expect("b").origin;
        if dist_xy(oa, [200.0, 0.0, 0.0]) < 80.0 && dist_xy(ob, [200.0, 0.0, 0.0]) < 80.0 {
            return;
        }
    }
    panic!(
        "two bots did not both walk in: a={:?} b={:?}",
        world.player(BOT).map(|p| p.origin),
        world.player(BOT_B).map(|p| p.origin)
    );
}

#[test]
fn pmove_dom_flag_claims_without_reloading() {
    let mut scene = begin_dom_scene([200.0, 0.0, 0.0]);
    scene.world.debug_set_held_ammo(BOT, 0, 90);
    let mut captured = false;
    for _ in 0..240 {
        let (cmd, _) = scene.drive(MATCH_TICK_MS as i32);
        assert_eq!(
            cmd.buttons & playerstate_iw4::buttons::RELOAD,
            0,
            "capture must not dump the magazine"
        );
        let flag = scene
            .world
            .objectives
            .flags
            .iter()
            .find(|f| f.label == "A")
            .expect("flag A");
        if flag.owner == gamemode_iw4::Team::Allies {
            captured = true;
            break;
        }
    }
    assert!(
        captured,
        "Allies standing in a DOM radius never finished the capture, flags={:?}",
        scene.world.objectives.flags
    );
    scene.world.set_origin(BOT, [0.0, 0.0, 0.0]);
    scene.world.set_viewangles(BOT, [0.0, 0.0, 0.0]);
    scene
        .world
        .debug_place_alive_player(ENEMY, [80.0, 0.0, 0.0]);
    scene.world.debug_set_held_ammo(BOT, 30, 90);
    let mut fought = false;
    for _ in 0..16 {
        scene.drive(MATCH_TICK_MS as i32);
        if scene.bot.task().kind == TaskKind::Fight {
            fought = true;
            break;
        }
    }
    assert!(
        fought,
        "visible enemy did not interrupt the flag, task={:?}",
        scene.bot.task()
    );
}

const HUMAN: ClientId = ClientId(0);

fn enter_playing(world: &mut SimWorld) {
    let _ = sim::step(
        world,
        Tick(1),
        &TickInput {
            cmds: Vec::new(),
            actions: vec![(
                BOT,
                ClientAction::SetMatchPhase {
                    request_id: 1,
                    phase: MatchPhase::Playing,
                },
            )],
        },
        MATCH_TICK_MS as i32,
        StepReason::AuthorityFrame,
    );
}

#[test]
fn pmove_fight_sidesteps_a_pillar_to_shoot() {
    let pillar = aabb([72.0, -20.0, 0.0], [88.0, 20.0, 58.0]);
    let mut scene = begin_scene(vec![pillar], [0.0, 0.0, 0.0]);
    scene.bot = HostController::new(3);
    scene.world.debug_set_held_ammo(BOT, 30, 90);
    scene
        .world
        .debug_place_alive_player(ENEMY, [150.0, 0.0, 0.0]);
    scene.world.set_viewangles(BOT, [0.0, 0.0, 0.0]);
    let mut fired_wide = false;
    for _ in 0..80 {
        let (cmd, _) = scene.drive(MATCH_TICK_MS as i32);
        if scene.bot.task().kind == TaskKind::Fight
            && cmd.buttons & playerstate_iw4::buttons::ATTACK != 0
            && scene.origin()[1].abs() > 24.0
        {
            fired_wide = true;
            break;
        }
    }
    assert!(
        fired_wide,
        "did not sidestep the pillar to shoot, origin={:?} task={:?}",
        scene.origin(),
        scene.bot.task()
    );
}

#[test]
fn pmove_human_and_several_bots_fight() {
    let mut world = playing_world(vec![]);
    let bounds = crate::nav::brush_bounds(world.clip_brushes()).unwrap();
    let digest = world.content_digest();
    let graph = crate::nav::bake(&mut world, bounds, digest);
    world.debug_place_alive_player(HUMAN, [120.0, 0.0, 0.0]);
    world.set_viewangles(HUMAN, [0.0, 180.0, 0.0]);
    let bots = [ClientId(1), ClientId(2), ClientId(3)];
    for (i, id) in bots.into_iter().enumerate() {
        world.debug_place_alive_player(id, [0.0, (i as f32 - 1.0) * 48.0, 0.0]);
        world.set_viewangles(id, [0.0, 0.0, 0.0]);
        world.debug_set_held_ammo(id, 30, 90);
    }
    enter_playing(&mut world);
    let mut brains: Vec<HostController> = bots
        .iter()
        .map(|id| HostController::new(u64::from(id.0) | 0x80))
        .collect();
    let mut tick = 1u32;
    let mut fired = false;
    for _ in 0..48 {
        let snap = world.snapshot(Tick(tick));
        let mut astar = 2048u32;
        let mut cmds = vec![(
            HUMAN,
            playerstate_iw4::UserCmd {
                server_time: ((tick + 1) as i32) * MATCH_TICK_MS as i32,
                ..playerstate_iw4::UserCmd::default()
            },
        )];
        for (id, brain) in bots.iter().zip(brains.iter_mut()) {
            let obs = observe(&snap, *id, &mut world).expect("obs");
            let mut cmd = brain.drive_nav(&obs, &mut world, Some(&graph), &mut astar, 50);
            cmd.server_time = ((tick + 1) as i32) * MATCH_TICK_MS as i32;
            if brain.task().kind == TaskKind::Fight
                && cmd.buttons & playerstate_iw4::buttons::ATTACK != 0
            {
                fired = true;
            }
            cmds.push((*id, cmd));
        }
        tick += 1;
        let _ = sim::step(
            &mut world,
            Tick(tick),
            &TickInput::from_cmds(cmds),
            MATCH_TICK_MS as i32,
            StepReason::AuthorityFrame,
        );
        if fired {
            return;
        }
    }
    panic!("human + bots never pressed attack");
}

#[test]
fn pmove_max_roster_walks_on_shared_budget() {
    let mut world = playing_world_kind(vec![], gamemode_iw4::GameModeKind::Domination);
    bind_dom_flags(&mut world);
    let bounds = crate::nav::brush_bounds(world.clip_brushes()).unwrap();
    let digest = world.content_digest();
    let graph = crate::nav::bake(&mut world, bounds, digest);
    let mut roster = BotRoster::default();
    let ids = roster.add_bots(MAX_HOST_BOTS);
    assert_eq!(ids.len(), MAX_HOST_BOTS as usize);
    for (i, id) in ids.iter().enumerate() {
        let col = (i / 10) as f32;
        let row = (i % 10) as f32;
        world.debug_place_alive_player(*id, [-200.0 - col * 40.0, (row - 4.5) * 40.0, 0.0]);
        world.debug_set_team(*id, entity_iw4::TEAM_ALLIES);
        world.set_viewangles(*id, [0.0, 0.0, 0.0]);
    }
    enter_playing(&mut world);
    let mut tick = 1u32;
    let mut progressed = 0u32;
    for _ in 0..80 {
        let snap = world.snapshot(Tick(tick));
        let mut traces = TraceBudget::new(96);
        let mut astar = 2048u32;
        let mut cmds = Vec::new();
        for slot in &mut roster.bots {
            let mut queried = Budgeted {
                world: &mut world,
                budget: &mut traces,
            };
            let Some(obs) = observe(&snap, slot.id, &mut queried) else {
                continue;
            };
            let mut cmd = slot.brain.drive_nav(
                &obs,
                &mut queried,
                Some(&graph),
                &mut astar,
                MATCH_TICK_MS as i32,
            );
            cmd.server_time = ((tick + 1) as i32) * MATCH_TICK_MS as i32;
            cmds.push((slot.id, cmd));
        }
        tick += 1;
        let _ = sim::step(
            &mut world,
            Tick(tick),
            &TickInput::from_cmds(cmds),
            MATCH_TICK_MS as i32,
            StepReason::AuthorityFrame,
        );
        progressed = ids
            .iter()
            .filter(|id| world.player(**id).is_some_and(|ps| ps.origin[0] > -160.0))
            .count() as u32;
        if progressed >= 8 {
            return;
        }
    }
    panic!("max roster did not walk on the shared budget, progressed={progressed}");
}
