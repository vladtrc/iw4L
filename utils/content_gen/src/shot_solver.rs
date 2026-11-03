//! `iw4l solve-shot <route.json>`: finds a grenade-launcher shot whose
//! projectile_flight flight passes through the atlas doors of a route and ends in a
//! victim standing in the route's last world.
//!
//! The route names three worlds, the doors between them and how long the
//! flight stays:
//!
//! ```json
//! { "worlds": ["iw4:mp_terminal", "t5:mp_firingrange", "t6:mp_nuketown_2020"],
//!   "enter": "iw4:mp_terminal#3",
//!   "bridge": ["t5:mp_firingrange#4", "t6:mp_nuketown_2020#9"],
//!   "min_world_ms": [0, 6000, 3000],
//!   "min_flight_ms": 20000,
//!   "name": "projectile_tour" }
//! ```
//!
//! The flight goes world 0 → `enter` → any door X of world 1 → `bridge` →
//! world 2, where the victim stands. X and the side it opens to are the
//! solver's choice; a bridge of `<zone>#*` in world 1 leaves that door to the
//! solver too, among the doors of the landing's size. The best flight covers the most ground: the floor-plan
//! cells its path passes through, in every world. The search runs leg by
//! leg: every leg keeps the best candidates and the next one continues each of them through every door
//! choice. A candidate is only accepted after the whole flight is flown again
//! on the final atlas.
//!
//! Output: the merged map `<artifacts>/maps/<name>.json` (`assets::MergedMap`:
//! the worlds, the portals, a final killcam long enough for the flight
//! (`scr_killcam_time`, and `sv_archive_mb` so the archive holds it), the
//! bounce rules it was solved with (`projectile_parallel` /
//! `projectile_perpendicular`), and under `shot` the shooter pose, the victim and
//! the expected launch), and the flown path in
//! `<artifacts>/atlas/projectile_flight/<name>-path.json`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use asset_transport::GamesRoot;
use movement_iw4::{CollisionBackend, GroundTraceInput};
use serde_json::{Value, json};
use sim::bg::player_collision::PlayerCollision;
use sim::bullet_collision::{MASK_PLAYER_SOLID, PLAYER_MAXS, PLAYER_MINS};
use sim::projectile_flight::{
    ProjectileFlightBody, ProjectileFlightEvent, ProjectileFlightRules, projectile_step,
};
use sim::world_graph::{
    Atlas, AtlasClip, AtlasDoor, AtlasPortal, AtlasWorld, carry_point, rotate_yaw, world_at,
    world_offset,
};
use sim::{EquipmentRuntimeFacts, GrenadeLaunchKind, TraceOutcome};

/// How long the final killcam runs before the shot.
const KILLCAM_LEAD_MS: i32 = 4_000;

/// The bounce a merged map flies with, livelier than core's default so a
/// flight crosses more of each world: off a wall it heads for the door out
/// on the high arc, so it flies over the world instead of rattling in a room
/// (a route's `lift` / `home` override those). The recipe sets it through
/// the `projectile_*` dvars.
const RULES: ProjectileFlightRules = ProjectileFlightRules {
    parallel: 1.0,
    perpendicular: 0.98,
    lift: 0.0,
    home: 2.0,
    home_target: None,
    home_align: 0.0,
    gravity_scale: 1.0,
    speed_scale: 1.0,
};

/// Killcam archive MiB a second of a merged map's frames, with headroom:
/// three worlds run ~1.6 MiB/s.
const ARCHIVE_MB_PER_S: i32 = 2;

const TICK: i32 = sim::MATCH_TICK_MS as i32;
/// `gl_m4_mp`: projectileSpeed and projectileSpeedUp.
/// The launch point above the shooter's feet (the standing eye).
const EYE: f32 = 60.0;
/// How much longer than the route's shortest flight a flight may last.
const FLIGHT_SLACK_MS: i32 = 15_000;
/// The longest a shot may take to reach the first door.
const LAUNCH_MS: i32 = 8_000;
/// The longest a candidate may stay in world 1 beyond the route's minimum
/// before it is dropped.
const LEG_MS: i32 = 15_000;
/// Leg 2 steers the nearest misses within `STEER_MISS` units of an exit:
/// each shot is aimed again on a grid of ±`STEER_STEPS` steps of
/// `STEER_DEG` degrees.
const STEER_SHOTS: usize = 400;
const STEER_MISS: f32 = 1500.0;
const STEER_STEPS: usize = 10;
const STEER_DEG: f32 = 0.1;
/// How many candidates each leg hands on.
const KEEP: usize = 1500;
/// The most bounces a second a flight may take in any world unless the
/// route sets `max_bounces_per_s`: past that it rattles in a room instead
/// of flying through the map.
const MAX_BOUNCES_PER_S: f32 = 0.5;
/// How far past its doors a world's playable box reaches, sideways and
/// below; above, the sky. The victim stands in it.
const BOUNDS_MARGIN: f32 = 512.0;
const BOUNDS_SKY: f32 = 8192.0;
/// How much farther sideways the projectile may fly: the doors are in the
/// buildings, the open ground lies past them.
const BOUNDS_FLY: f32 = 4096.0;
/// The aim grid of leg 1, in degrees: coarse first, then the kept shots'
/// neighbours on the fine step.
const AIM_COARSE: i32 = 1;
/// A projectile at 24..52 above the floor hits a standing player, hips to
/// chest: the hit volumes are bones, and a low one rising off the floor
/// passes between the legs.
const VICTIM_HEIGHT: (f32, f32) = (24.0, 52.0);

/// A door in a route: the zone, its index in the zone's catalog, the door.
#[derive(Clone)]
struct RouteDoor {
    source: Value,
    id: String,
    door: AtlasDoor,
    walkable: bool,
}

impl RouteDoor {
    fn fits(&self, other: &RouteDoor) -> bool {
        self.walkable
            && other.walkable
            && (self.door.width - other.door.width).abs() <= assets::MAX_DOOR_WIDTH_TRIM as f32
            && (self.door.height - other.door.height).abs() <= assets::MAX_DOOR_HEIGHT_TRIM as f32
    }
}

#[derive(Clone)]
struct Route {
    source: Value,
    /// The merged map's name: `map <name>` plays it.
    name: String,
    weapon: String,
    launch_feet: Option<[f32; 3]>,
    launch_velocity: [f32; 3],
    launch_eye_height: f32,
    max_portal_angle_deg: f32,
    aim_yaw_span_deg: i32,
    aim_pitch_span_deg: i32,
    aim_fine_steps: i32,
    keep_per_spot: usize,
    zones: Vec<String>,
    enter: RouteDoor,
    /// The doors of world 1 the flight may leave by: the bridge's first door,
    /// or every door of the landing's size for `#*`.
    exits: Vec<RouteDoor>,
    /// The bridge's door in world 2.
    landing: RouteDoor,
    landings: Vec<RouteDoor>,
    /// The doors of world 1: the choices for X.
    middle: Vec<RouteDoor>,
    /// The least time the flight spends in each world.
    min_world_ms: [i32; 3],
    /// The least time from the shot to the hit.
    min_flight_ms: i32,
    max_flight_ms: i32,
    min_portal_entry_clear_ms: i32,
    min_portal_exit_clear_ms: i32,
    /// Each world's playable box in shared coordinates: its doors' extent.
    bounds: [([f32; 3], [f32; 3]); 3],
    max_bounces_per_s: f32,
    max_bounces_per_world: [usize; 3],
    rules: ProjectileFlightRules,
}

pub fn solve(
    games: &GamesRoot,
    artifacts: &std::path::Path,
    route: &std::path::Path,
    replace_victim: bool,
) -> Result<(), String> {
    let text = std::fs::read_to_string(route).map_err(|e| format!("{}: {e}", route.display()))?;
    let config: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", route.display()))?;
    let mut route = read_route(artifacts, &config)?;

    announce("loading worlds");
    // The worlds load side by side.
    let worlds = par_map(&route.zones, |zone| {
        vec![(|| {
            let (prepared, _) = crate::doors::load_map(games, zone)?;
            let equipment = if zone == &route.zones[0] {
                let id = prepared.weapons.resolve_index(&route.weapon).map_err(|e| format!("weapon: {e:?}"))?
                    .ok_or_else(|| format!("weapon {} not loaded", route.weapon))?;
                let facts = prepared.weapons.equipment_facts_of(id).ok_or("weapon has no equipment facts")?;
                announce(&format!("weapon {} id={id} speed={} up={} forward={} relative_up={} class={} offhand={} impact={} stickiness={} stick_to_players={}",
                    route.weapon, facts.projectile_speed, facts.projectile_speed_up, facts.projectile_speed_forward,
                    facts.projectile_speed_relative_up, facts.weap_class, facts.offhand_class, facts.impact_damage, facts.stickiness, facts.stick_to_players));
                Some(facts)
            } else { None };
            let clip = prepared.clip.as_ref().ok_or_else(|| format!("{zone}: no clip"))?;
            let mut search_clip = clip.clone();
            if zone == &route.zones[0] && !route.enter.source["replace"].is_null() {
                let mut endpoint = route.enter.source.clone();
                endpoint["world"] = json!(0);
                let scene = assets::MergedMap {
                    name: route.name.clone(), path: Default::default(), worlds: route.zones.clone(),
                    recipe: json!({"portals":[{"a":endpoint,"b":endpoint}]}),
                };
                let replacements = scene.door_replacements()?.into_iter().take(1).collect::<Vec<_>>();
                let mut opened = Arc::unwrap_or_clone(search_clip);
                asset_world::PortalDoorReplacement::apply_all_collision(&replacements, &mut opened)?;
                search_clip = Arc::new(opened);
            }
            let (brushes, bsp, mesh, cmodels) = session::sim_clip_map(&search_clip);
            announce(&format!("{zone}: {} brushes", brushes.len()));
            Ok::<_, String>((Arc::new(AtlasWorld {
                zone: zone.clone(), brushes, bsp, mesh, cmodels,
            }), equipment, clip.clone()))
        })()]
    })
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let equipment = worlds[0].1.ok_or("base weapon facts missing")?;
    let native_clips = worlds.iter().map(|world| world.2.clone()).collect();
    let worlds: Vec<_> = worlds.into_iter().map(|(world, _, _)| world).collect();
    let mut solver = Solver {
        equipment,
        native_clips,
        max_portal_angle_deg: route.max_portal_angle_deg,
        min_portal_entry_clear_ms: route.min_portal_entry_clear_ms,
        min_portal_exit_clear_ms: route.min_portal_exit_clear_ms,
        base: worlds[0].clone(),
        others: worlds[1..].to_vec(),
        bounds: route.bounds,
        max_bounces_per_s: route.max_bounces_per_s,
        max_bounces_per_world: route.max_bounces_per_world,
        rules: route.rules,
    };

    if replace_victim {
        return replace_saved_victim(artifacts, &config, &mut route, &mut solver);
    }
    let shots = solver.leg_launch(&route);
    announce(&format!(
        "leg 1 ({}): {} shots cross {}",
        route.zones[0],
        shots.len(),
        route.enter.id
    ));
    let shots = keep_best(shots, route.keep_per_spot);

    let mut seconds = Vec::new();
    for x in (0..route.middle.len()).filter(|&x| route.enter.fits(&route.middle[x])) {
        // Any exit but X itself; one at a time, as a homing bounce heads
        // for the one there is.
        for e in (0..route.exits.len()).filter(|&e| route.exits[e].id != route.middle[x].id) {
            for flip in [false, true] {
                seconds.push((x, flip, e));
            }
        }
    }
    let through = |shot: &Flight, c: usize| {
        let (x, flip, exit) = seconds[c];
        let portals = vec![
            portal(&route.enter, &route.middle[x], flip),
            portal(&route.exits[exit], &route.landing, false),
        ];
        let shot = solver
            .launch_on(&route, shot.launch.feet, shot.launch.angles, portals[0])
            .0
            .ok_or(f32::MAX)?;
        solver
            .leg_through(&shot, portals, 0, route.min_world_ms[1])
            .map(|(mut flight, _)| {
                flight.choice = Choice {
                    x: Some((x, flip)),
                    exit,
                    exit_flip: false,
                    landing: 0,
                };
                flight
            })
    };
    let tried = par_map(&iproduct(shots.len(), seconds.len()), |&(s, c)| {
        vec![(s, c, through(&shots[s], c))]
    });
    let mut misses: Vec<(f32, usize, usize)> = Vec::new();
    let mut crossed = Vec::new();
    for (s, c, result) in tried {
        match result {
            Ok(flight) => crossed.push(flight),
            Err(miss) => misses.push((miss, s, c)),
        }
    }
    // The near misses steer: the shot is aimed again on a fine grid around
    // them, the flight past the door changing smoothly with the aim while
    // it seldom bounces.
    misses.sort_by(|a, b| a.0.total_cmp(&b.0));
    let near: Vec<_> = misses
        .iter()
        .take(if route.rules.home > 0.0 {
            0
        } else {
            STEER_SHOTS
        })
        .filter(|m| m.0 < STEER_MISS)
        .collect();
    let aims: Vec<_> = near
        .iter()
        .flat_map(|&&(_, s, c)| {
            let (feet, [pitch, yaw]) = (shots[s].launch.feet, shots[s].launch.angles);
            iproduct(STEER_STEPS * 2 + 1, STEER_STEPS * 2 + 1)
                .into_iter()
                .map(move |(i, j)| {
                    let step = |n: usize| (n as f32 - STEER_STEPS as f32) * STEER_DEG;
                    (
                        feet,
                        [short_angle(pitch + step(j)), short_angle(yaw + step(i))],
                        c,
                    )
                })
        })
        .collect();
    announce(&format!(
        "leg 2: {} flights through, steering {} near misses ({:.0} units nearest) with {} aims",
        crossed.len(),
        near.len(),
        misses.first().map_or(f32::MAX, |m| m.0),
        aims.len()
    ));
    crossed.extend(par_map(&aims, |&(feet, angles, c)| {
        solver
            .shoot(&route, feet, angles)
            .and_then(|shot| through(&shot, c).ok())
            .into_iter()
            .collect()
    }));
    let exits: Vec<&str> = route.exits.iter().map(|door| door.id.as_str()).collect();
    announce(&format!(
        "leg 2 ({}): {} flights reach {}",
        route.zones[1],
        crossed.len(),
        exits.join(" / ")
    ));
    let crossed = keep_best(crossed, route.keep_per_spot);

    let mut targets = vec![route.rules.home_target];
    if config["search_home_target"].as_bool() == Some(true) {
        targets.clear();
        let (lo, hi) = route.bounds[2];
        for x in (lo[0] as i32..hi[0] as i32).step_by(256) {
            for y in (lo[1] as i32..hi[1] as i32).step_by(256) {
                let point = [x as f32, y as f32, hi[2] - BOUNDS_SKY + 128.0];
                if let Some((floor, normal)) = solver.trace(point, sub(point, [0.0, 0.0, 1024.0])) {
                    if normal[2] > 0.9 && solver.stands(add(floor, [0.0, 0.0, 1.0])) {
                        targets.push(Some(add(floor, [0.0, 0.0, 40.0])));
                    }
                }
            }
        }
    }
    announce(&format!("leg 3: trying {} homing targets", targets.len()));
    let mut finished = par_map(&crossed, |flight| {
        route
            .landings
            .iter()
            .enumerate()
            .flat_map(|(landing, door)| {
                if !door.fits(&route.exits[flight.choice.exit]) {
                    return Vec::new();
                }
                let mut last_route = route.clone();
                last_route.landing = door.clone();
                [false, true]
                    .into_iter()
                    .flat_map(|exit_flip| {
                        let mut entered = flight.clone();
                        entered.choice.exit_flip = exit_flip;
                        entered.choice.landing = landing;
                        let choice = entered.choice;
                        let atlas = route_atlas(&last_route, &solver, &entered);
                        let Some(shot) = solver
                            .launch_on(
                                &last_route,
                                entered.launch.feet,
                                entered.launch.angles,
                                atlas.portals[0],
                            )
                            .0
                        else {
                            return Vec::new();
                        };
                        let Ok((mut entered, _)) =
                            solver.leg_through(&shot, atlas.portals, 0, last_route.min_world_ms[1])
                        else {
                            return Vec::new();
                        };
                        entered.choice = choice;
                        targets
                            .iter()
                            .filter_map(|&target| {
                                let mut last_solver = solver.clone();
                                last_solver.rules.home_target = target;
                                last_solver
                                    .leg_last(&last_route, &entered)
                                    .map(|mut result| {
                                        result.home_target = target;
                                        result
                                    })
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    });
    announce(&format!(
        "leg 3 ({}): {} flights end in a victim",
        route.zones[2],
        finished.len()
    ));
    finished.sort_by(|a, b| b.score().total_cmp(&a.score()));

    announce("replaying the best on the final atlas");
    for flight in &finished {
        route.landing = route.landings[flight.choice.landing].clone();
        route.rules.home_target = flight.home_target;
        solver.rules = route.rules;
        let atlas = route_atlas(&route, &solver, flight);
        let Some(check) = solver.replay(&atlas, flight) else {
            announce("a candidate diverged on the final atlas; next");
            continue;
        };
        write_outputs(artifacts, &route, &atlas, flight, &check, &solver)?;
        return Ok(());
    }
    Err("no flight passes the whole route".into())
}

/// Replays a saved shot with its chosen doors and rules, searching only for
/// a standing victim on its final leg. It never searches shooter poses or aims.
fn replace_saved_victim(
    artifacts: &std::path::Path,
    config: &Value,
    route: &mut Route,
    solver: &mut Solver,
) -> Result<(), String> {
    let path = artifacts.join("maps").join(format!("{}.json", route.name));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let saved: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if saved["worlds"] != json!(route.zones) {
        return Err("saved shot's worlds differ from the route".into());
    }
    let mut fixed = config.clone();
    let doors = &saved["shot"]["doors"];
    fixed["enter"] = doors[0].clone();
    fixed["bridge"] = json!([doors[2], doors[3]]);
    fixed["search_landings"] = json!(false);
    *route = read_route(artifacts, &fixed)?;
    route.rules = route
        .rules
        .with_dvars(|key| saved["dvars"][key].as_str()?.parse().ok());
    solver.rules = route.rules;
    let vector = |v: &Value| -> Result<[f32; 3], String> {
        Ok([
            v[0].as_f64().ok_or("saved shot: invalid vector")? as f32,
            v[1].as_f64().ok_or("saved shot: invalid vector")? as f32,
            v[2].as_f64().ok_or("saved shot: invalid vector")? as f32,
        ])
    };
    let shooter = &saved["shot"]["shooter"];
    let feet = vector(&shooter["feet"])?;
    let angles = [
        shooter["pitch"].as_f64().ok_or("saved shot: pitch")? as f32,
        shooter["yaw"].as_f64().ok_or("saved shot: yaw")? as f32,
    ];
    let shot = solver
        .shoot(route, feet, angles)
        .ok_or("saved shot no longer reaches its first door")?;
    if dist(
        shot.launch.body.pos.tr_base,
        vector(&saved["shot"]["launch"]["base"])?,
    ) > 0.01
        || dist(
            shot.launch.body.pos.tr_delta,
            vector(&saved["shot"]["launch"]["delta"])?,
        ) > 0.01
    {
        return Err("saved shot's launch physics changed".into());
    }
    let x = route
        .middle
        .iter()
        .position(|d| Some(d.id.as_str()) == doors[1].as_str())
        .ok_or("saved entry door missing")?;
    let flip = |i: usize| saved["portals"][i]["flip"].as_bool().unwrap_or(false);
    let portals = vec![
        portal(&route.enter, &route.middle[x], flip(0)),
        portal(&route.exits[0], &route.landing, flip(1)),
    ];
    let (mut through, _) = solver
        .leg_through(&shot, portals, 0, route.min_world_ms[1])
        .map_err(|_| "saved shot no longer reaches its bridge")?;
    through.choice = Choice {
        x: Some((x, flip(0))),
        exit: 0,
        exit_flip: flip(1),
        landing: 0,
    };
    let mut flight = solver
        .leg_last(route, &through)
        .ok_or("saved path has no standing victim satisfying the route")?;
    flight.home_target = route.rules.home_target;
    let atlas = route_atlas(route, solver, &flight);
    let check = solver
        .replay(&atlas, &flight)
        .ok_or("saved path diverged on full replay")?;
    write_outputs(artifacts, route, &atlas, &flight, &check, solver)
}

/// Each line carries the seconds since the solve started.
fn announce(text: &str) {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let at = START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f32();
    diag::announce_stdout(&format!("solve-shot: [{at:.1}s] {text}"));
}

fn read_route(artifacts: &std::path::Path, config: &Value) -> Result<Route, String> {
    let zones: Vec<String> = config["worlds"]
        .as_array()
        .ok_or("route: no worlds")?
        .iter()
        .filter_map(|zone| zone.as_str().map(str::to_owned))
        .collect();
    if zones.len() != 3 {
        return Err("route: the worlds are three zones".into());
    }
    let catalog_dir = match config["catalog"].as_str() {
        None | Some("doors") => "doors",
        Some("projectile-openings") => "projectile-openings",
        Some(value) => return Err(format!("route: unknown catalog {value}")),
    };
    let catalogs = zones
        .iter()
        .map(|zone| {
            let path = artifacts
                .join("content")
                .join(catalog_dir)
                .join(format!("{}.json", zone.replace(':', "_")));
            let text = std::fs::read_to_string(&path).map_err(|e| {
                format!(
                    "{}: {e} (run `iw4l-content inspect-doors` first)",
                    path.display()
                )
            })?;
            let value: Value =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            let rows = value["doors"].as_array().cloned().unwrap_or(Vec::new());
            if rows
                .iter()
                .any(|row| row["geometry_version"] != 4 || !row["two_sided"].is_boolean())
            {
                return Err(format!(
                    "{}: regenerate native-sidedness door catalog version 4",
                    path.display()
                ));
            }
            Ok(rows)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let find = |id: &str| -> Result<RouteDoor, String> {
        let (world, row) = catalogs
            .iter()
            .enumerate()
            .find_map(|(world, doors)| {
                doors
                    .iter()
                    .find(|row| row["id"].as_str() == Some(id))
                    .map(|row| (world, row))
            })
            .ok_or_else(|| format!("route: no door {id} in the catalogs"))?;
        Ok(catalog_door(world, row))
    };
    let named = |key: &str| -> Result<RouteDoor, String> {
        find(
            config[key]
                .as_str()
                .ok_or_else(|| format!("route: no {key}"))?,
        )
    };
    let bridge = config["bridge"].as_array().ok_or("route: no bridge")?;
    let leave = bridge
        .first()
        .and_then(Value::as_str)
        .ok_or("route: bridge")?;
    let landing = find(
        bridge
            .get(1)
            .and_then(Value::as_str)
            .ok_or("route: bridge")?,
    )?;
    let enter = named("enter")?;
    let middle: Vec<RouteDoor> = catalogs[1].iter().map(|row| catalog_door(1, row)).collect();
    let exits: Vec<RouteDoor> = match leave.strip_suffix("#*") {
        Some(_) => catalogs[1]
            .iter()
            .map(|row| catalog_door(1, row))
            .filter(|door| door.fits(&landing))
            .collect(),
        None => vec![find(leave)?],
    };
    if exits.is_empty() || !exits.iter().all(|door| door.fits(&landing)) {
        return Err(format!(
            "route: no door {leave} is the size of {}",
            landing.id
        ));
    }
    let ms = |value: &Value| value.as_i64().unwrap_or(0) as i32;
    let bounds = std::array::from_fn(|w| {
        let offset = world_offset(w);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for row in &catalogs[w] {
            let base = add(catalog_door(w, row).door.base, offset);
            for i in 0..3 {
                lo[i] = lo[i].min(base[i]);
                hi[i] = hi[i].max(base[i]);
            }
        }
        (
            sub(lo, [BOUNDS_MARGIN; 3]),
            add(hi, [BOUNDS_MARGIN, BOUNDS_MARGIN, BOUNDS_SKY]),
        )
    });
    let launch_vector = |key: &str| -> Result<[f32; 3], String> {
        let values = config["launch"][key]
            .as_array()
            .filter(|v| v.len() == 3)
            .ok_or_else(|| format!("route: launch.{key} must contain three numbers"))?;
        let mut result = [0.0; 3];
        for (out, value) in result.iter_mut().zip(values) {
            *out = value
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or("route: nonfinite launch vector")? as f32;
            if !out.is_finite() {
                return Err("route: launch vector exceeds f32 range".into());
            }
        }
        Ok(result)
    };
    let launch_feet = if config.get("launch").is_some() {
        Some(launch_vector("feet")?)
    } else {
        None
    };
    let launch_velocity = if launch_feet.is_some() {
        launch_vector("owner_velocity")?
    } else {
        [0.0; 3]
    };
    let launch_eye_height = if launch_feet.is_some() {
        config["launch"]["eye_height"]
            .as_f64()
            .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
            .ok_or("route: launch.eye_height must be 0..100")? as f32
    } else {
        EYE
    };
    Ok(Route {
        launch_feet,
        launch_velocity,
        launch_eye_height,
        source: config.clone(),
        name: config["name"]
            .as_str()
            .unwrap_or("projectile_flight")
            .to_ascii_lowercase(),
        weapon: config["weapon"]
            .as_str()
            .unwrap_or("iw4:weapon/gl_m4_mp")
            .to_owned(),
        max_portal_angle_deg: config["max_portal_angle_deg"]
            .as_f64()
            .unwrap_or(90.0)
            .clamp(0.0, 90.0) as f32,
        aim_fine_steps: config["aim_fine_steps"].as_i64().unwrap_or(1).clamp(1, 16) as i32,
        keep_per_spot: config["keep_per_spot"]
            .as_u64()
            .unwrap_or(40)
            .clamp(1, KEEP as u64) as usize,
        aim_pitch_span_deg: config["aim_pitch_span_deg"]
            .as_i64()
            .unwrap_or(18)
            .clamp(1, 80) as i32,
        aim_yaw_span_deg: config["aim_yaw_span_deg"]
            .as_i64()
            .unwrap_or(24)
            .clamp(1, 180) as i32,
        zones,
        enter,
        exits,
        landings: if config["search_landings"].as_bool() == Some(true) {
            catalogs[2]
                .iter()
                .map(|row| catalog_door(2, row))
                .filter(|d| d.fits(&landing))
                .collect()
        } else {
            vec![landing.clone()]
        },
        landing,
        middle,
        min_world_ms: std::array::from_fn(|w| ms(&config["min_world_ms"][w])),
        min_flight_ms: ms(&config["min_flight_ms"]),
        max_flight_ms: if let Some(value) = config.get("max_flight_ms") {
            let value = value
                .as_i64()
                .filter(|&v| v >= i64::from(ms(&config["min_flight_ms"])) && v <= 60_000)
                .ok_or("route: max_flight_ms must be between min_flight_ms and 60000")?;
            value as i32
        } else {
            ms(&config["min_flight_ms"]).saturating_add(FLIGHT_SLACK_MS)
        },
        min_portal_entry_clear_ms: ms(&config["min_portal_entry_clear_ms"]).max(0),
        min_portal_exit_clear_ms: ms(&config["min_portal_exit_clear_ms"]).max(0),
        bounds,
        max_bounces_per_world: if let Some(values) = config.get("max_bounces_per_world") {
            let values = values
                .as_array()
                .filter(|v| v.len() == 3)
                .ok_or("route: max_bounces_per_world needs three integers")?;
            let mut caps = [0; 3];
            for (cap, value) in caps.iter_mut().zip(values) {
                *cap = usize::try_from(
                    value
                        .as_u64()
                        .ok_or("route: negative or noninteger bounce cap")?,
                )
                .map_err(|_| "route: bounce cap overflow")?;
            }
            caps
        } else {
            [usize::MAX; 3]
        },
        max_bounces_per_s: config["max_bounces_per_s"]
            .as_f64()
            .map_or(MAX_BOUNCES_PER_S, |v| v as f32),
        rules: ProjectileFlightRules {
            speed_scale: config["speed_scale"]
                .as_f64()
                .unwrap_or(1.0)
                .clamp(0.05, 4.0) as f32,
            lift: config["lift"].as_f64().map_or(RULES.lift, |v| v as f32),
            home: config["home"].as_f64().map_or(RULES.home, |v| v as f32),
            home_align: config["home_align"].as_f64().unwrap_or(0.0) as f32,
            gravity_scale: config["gravity_scale"]
                .as_f64()
                .unwrap_or(1.0)
                .clamp(0.0, 4.0) as f32,
            home_target: config["home_target"]
                .as_array()
                .filter(|a| a.len() == 3)
                .and_then(|a| {
                    Some(add(
                        [
                            a[0].as_f64()? as f32,
                            a[1].as_f64()? as f32,
                            a[2].as_f64()? as f32,
                        ],
                        world_offset(2),
                    ))
                }),
            ..RULES
        },
    })
}

/// The catalog rounds normals; the yaw is the door's facing.
fn catalog_door(world: usize, row: &Value) -> RouteDoor {
    let number = |key: &str| row[key].as_f64().unwrap_or(0.0) as f32;
    let base = std::array::from_fn(|i| row["base"][i].as_f64().unwrap_or(0.0) as f32);
    RouteDoor {
        source: row.clone(),
        id: row["id"].as_str().unwrap_or("").to_owned(),
        door: AtlasDoor {
            two_sided: row["two_sided"]
                .as_bool()
                .expect("validated native door sidedness"),
            world: sim::world_graph::WorldId(world),
            base,
            normal: std::array::from_fn(|i| row["normal"][i].as_f64().unwrap_or(0.0) as f32),
            width: number("width"),
            height: number("height"),
        },
        walkable: row["kind"].as_str() == Some("door")
            || row["clearance"].as_str() == Some("flight"),
    }
}

fn portal(a: &RouteDoor, b: &RouteDoor, flip: bool) -> AtlasPortal {
    let mut a = a.door;
    let mut b = b.door;
    let width = a.width.min(b.width);
    let height = a.height.min(b.height);
    a.width = width;
    b.width = width;
    a.height = height;
    b.height = height;
    AtlasPortal { a, b, flip }
}

fn route_atlas(route: &Route, solver: &Solver, flight: &Flight) -> Atlas {
    let (x, flip_x) = flight.choice.x.expect("x chosen");
    Atlas {
        portal_thickness: Vec::new(),
        portal_clearance: Vec::new(),
        worlds: solver.others.clone(),
        portals: vec![
            portal(&route.enter, &route.middle[x], flip_x),
            portal(
                &route.exits[flight.choice.exit],
                &route.landings[flight.choice.landing],
                flight.choice.exit_flip,
            ),
        ],
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Choice {
    x: Option<(usize, bool)>,
    /// The index in `Route::exits` of the door the flight leaves world 1 by.
    exit: usize,
    exit_flip: bool,
    landing: usize,
}

/// Where the shot comes from.
#[derive(Clone, Copy, Debug)]
struct Launch {
    feet: [f32; 3],
    /// Pitch (down positive) and yaw, degrees, on the 16-bit angle grid.
    angles: [f32; 2],
    body: ProjectileFlightBody,
}

/// A flight up to the door that ends its latest leg.
#[derive(Clone, Debug)]
struct Flight {
    launch: Launch,
    choice: Choice,
    /// The door the latest leg ended in: the portal event, unapplied.
    door: Option<Door>,
    /// Tick-end positions, shared coordinates.
    path: Vec<[f32; 3]>,
    /// Bounce points and the path index of the tick they happened in.
    bounces: Vec<(usize, [f32; 3])>,
    last_bounce_ms: Option<i32>,
    victim: Option<[f32; 3]>,
    home_target: Option<[f32; 3]>,
    /// Exact portal entry times, avoiding duplicated samples at seams.
    entered_at: [Option<i32>; 3],
    /// The tick time the flight ends at.
    time: i32,
}

#[derive(Clone, Copy, Debug)]
struct Door {
    at: [f32; 3],
    time: i32,
    velocity: [f32; 3],
    tick_end: i32,
}

impl Flight {
    fn bounces_in(&self, world: usize) -> usize {
        self.bounces
            .iter()
            .filter(|(_, p)| world_at(*p) == world)
            .count()
    }

    /// Whether the flight bounces faster than `per_s` in any world it has
    /// flown a second in.
    fn rattles(&self, per_s: f32, caps: [usize; 3]) -> bool {
        (0..3).any(|w| {
            let seconds = self.ms_in(w) as f32 / 1000.0;
            self.bounces_in(w) > caps[w]
                || (seconds > 0.0 && self.bounces_in(w) as f32 > per_s * seconds + 1e-4)
        })
    }

    /// Time in `world`, using exact portal events and the final hit tick.
    fn ms_in(&self, world: usize) -> i32 {
        let Some(start) = self.entered_at[world] else {
            return 0;
        };
        let end = self
            .entered_at
            .get(world + 1)
            .copied()
            .flatten()
            .unwrap_or(self.time);
        (end - start).max(0)
    }

    /// How much of the maps the flight shows, flying rather than
    /// rattling: the floor-plan cells it passes through, each world's on a
    /// square root so no world is left as a room, less every bounce.
    fn score(&self) -> f32 {
        let shown: f32 = (0..3)
            .map(|w| {
                let path: Vec<_> = self
                    .path
                    .iter()
                    .copied()
                    .filter(|p| world_at(*p) == w)
                    .collect();
                (coverage(&path) as f32).sqrt()
            })
            .sum();
        shown - BOUNCE_COST * self.bounces.len() as f32
    }
}

/// What a bounce takes off `Flight::score`: a flight should fly, not rattle.
const BOUNCE_COST: f32 = 0.08;

/// The side of a floor-plan cell of `coverage`.
const CELL: f32 = 128.0;

/// The distinct floor-plan cells `path` passes through.
fn coverage(path: &[[f32; 3]]) -> usize {
    path.iter()
        .map(|p| ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32))
        .collect::<std::collections::HashSet<_>>()
        .len()
}

/// Keeps the candidates that cover the most ground and keep the most speed
/// at the door, at most a few per shooter spot.
fn keep_best(mut flights: Vec<Flight>, per_spot_limit: usize) -> Vec<Flight> {
    let key = |flight: &Flight| {
        let speed = flight
            .door
            .map_or(0.0, |door| dist(door.velocity, [0.0; 3]));
        flight.score() + speed / 300.0
    };
    flights.sort_by(|a, b| key(b).total_cmp(&key(a)));
    let mut per_spot = std::collections::HashMap::<[i32; 3], usize>::new();
    flights
        .into_iter()
        .filter(|flight| {
            let spot = flight.launch.feet.map(|v| v as i32);
            let count = per_spot.entry(spot).or_default();
            *count += 1;
            *count <= per_spot_limit
        })
        .take(KEEP)
        .collect()
}

#[derive(Debug)]
enum Stop {
    Oblique,
    Door(usize, bool, Door),
    Deadline,
    Stalled,
    /// Out of its world's playable box.
    Out,
}

#[derive(Clone)]
struct Solver {
    native_clips: Vec<Arc<asset_world::ClipCollision>>,
    equipment: EquipmentRuntimeFacts,
    max_portal_angle_deg: f32,
    min_portal_entry_clear_ms: i32,
    min_portal_exit_clear_ms: i32,
    base: Arc<AtlasWorld>,
    others: Vec<Arc<AtlasWorld>>,
    bounds: [([f32; 3], [f32; 3]); 3],
    max_bounces_per_s: f32,
    max_bounces_per_world: [usize; 3],
    rules: ProjectileFlightRules,
}

impl Solver {
    fn clip(&self) -> AtlasClip<'_> {
        AtlasClip {
            brushes: &self.base.brushes,
            bsp: &self.base.bsp,
            mesh: &self.base.mesh,
            cmodels: &self.base.cmodels,
        }
    }

    /// Whether a player stands at `feet`: the floor the projectile bounces
    /// on may be clip only bullets hit, and a victim there falls through.
    fn stands(&self, feet: [f32; 3]) -> bool {
        let n = world_at(feet);
        let Some(world) = (if n == 0 {
            Some(&self.base)
        } else {
            self.others.get(n - 1)
        }) else {
            return false;
        };
        let collision = PlayerCollision {
            brushes: &world.brushes,
            bsp: &world.bsp,
            mesh: &world.mesh,
            glass_damage: &[],
            bodies: &[],
            self_entnum: 0,
            cmodels: &world.cmodels.models,
            linked_brushes: &[],
            model_brushes: &[],
            offset: world_offset(n),
        };
        let trace = |start: [f32; 3], end: [f32; 3]| {
            collision.trace(GroundTraceInput {
                start,
                end,
                mins: PLAYER_MINS,
                maxs: PLAYER_MAXS,
                tracemask: MASK_PLAYER_SOLID,
            })
        };
        let up = add(feet, [0.0, 0.0, 2.0]);
        let free = trace(up, up);
        let ground = trace(up, sub(feet, [0.0, 0.0, 16.0]));
        free.startsolid == 0
            && ground.startsolid == 0
            && ground.fraction < 1.0
            && ground.normal[2] >= 0.7
    }

    fn atlas(&self, portals: Vec<AtlasPortal>) -> Atlas {
        Atlas {
            portal_thickness: Vec::new(),
            portal_clearance: Vec::new(),
            worlds: self.others.clone(),
            portals,
        }
    }

    fn trace(&self, start: [f32; 3], end: [f32; 3]) -> Option<([f32; 3], [f32; 3])> {
        let atlas = Atlas {
            portal_thickness: Vec::new(),
            portal_clearance: Vec::new(),
            worlds: self.others.clone(),
            portals: Vec::new(),
        };
        match atlas.trace(self.clip(), &[], None, start, end).1 {
            TraceOutcome::Hit { end, normal, .. } => Some((end, normal)),
            TraceOutcome::StartSolid { end, .. } => Some((end, [0.0, 0.0, 0.0])),
            _ => None,
        }
    }

    /// Flies the body tick by tick from `prev` (the first tick ends at
    /// `first_end`) until a door, the deadline or a stall.
    fn fly(
        &self,
        atlas: &Atlas,
        body: &mut ProjectileFlightBody,
        prev: i32,
        first_end: i32,
        deadline: i32,
        flight: &mut Flight,
    ) -> Stop {
        let rules = self.rules;
        let (mut prev, mut time) = (prev, first_end);
        let mut slow = 0;
        loop {
            if time > deadline {
                return Stop::Deadline;
            }
            let before = body.origin;
            let events = projectile_step(
                sim::projectile_flight::ProjectileFlightScene {
                    atlas,
                    base: self.clip(),
                    players: &[],
                    blocked: &[],
                },
                None,
                &rules,
                body,
                prev,
                time,
            );
            for event in events {
                match event {
                    ProjectileFlightEvent::Bounce { origin, .. } => {
                        let world = world_at(origin).min(2);
                        if self.min_portal_exit_clear_ms > 0
                            && world > 0
                            && flight.entered_at[world].is_some_and(|entered| {
                                prev - entered < self.min_portal_exit_clear_ms
                            })
                        {
                            return Stop::Stalled;
                        }
                        flight.bounces.push((flight.path.len(), origin));
                        flight.last_bounce_ms = Some(time);
                    }
                    ProjectileFlightEvent::Portal {
                        portal,
                        from_a,
                        at,
                        time: at_time,
                        velocity,
                    } => {
                        let (entered, exit, _) = atlas.portals[portal.index()].oriented(from_a);
                        if self.min_portal_entry_clear_ms > 0
                            && at_time - flight.last_bounce_ms.unwrap_or(0)
                                < self.min_portal_entry_clear_ms
                        {
                            return Stop::Stalled;
                        }
                        if portal_entry_angle(velocity, entered.normal)
                            > self.max_portal_angle_deg + 1e-4
                        {
                            return Stop::Oblique;
                        }
                        if exit.world.index() < 3 {
                            flight.entered_at[exit.world.index()] = Some(at_time);
                        }
                        flight.path.push(at);
                        flight.time = time;
                        return Stop::Door(
                            portal.index(),
                            from_a,
                            Door {
                                at,
                                time: at_time,
                                velocity,
                                tick_end: time,
                            },
                        );
                    }
                    ProjectileFlightEvent::Player { .. } => {}
                }
            }
            let world = world_at(body.origin).min(2);
            if flight.bounces_in(world) as f32
                > (self.max_bounces_per_s * (deadline as f32 / 1000.0 + 0.1)).ceil()
            {
                return Stop::Stalled;
            }
            let (lo, hi) = self.bounds[world];
            let reach = [BOUNDS_FLY, BOUNDS_FLY, 0.0];
            let (lo, hi) = (sub(lo, reach), add(hi, reach));
            if (0..3).any(|i| body.origin[i] < lo[i] || body.origin[i] > hi[i]) {
                return Stop::Out;
            }
            flight.path.push(body.origin);
            flight.time = time;
            slow = if dist(before, body.origin) < 6.0 {
                slow + 1
            } else {
                0
            };
            if slow >= 4 {
                return Stop::Stalled;
            }
            prev = time;
            time += TICK;
        }
    }

    /// The body just past `portal`, as `projectile_step` leaves it, and the
    /// tick it continues in.
    fn through(
        &self,
        atlas: &Atlas,
        portal: usize,
        from_a: bool,
        door: &Door,
        pos: ProjectileFlightBody,
    ) -> ProjectileFlightBody {
        let (entered, exit, yaw) = atlas.portals[portal].oriented(from_a);
        let carried = carry_point(&entered, &exit, yaw, door.at);
        let mut body = pos;
        body.origin = carried;
        body.pos.tr_base = carried;
        body.pos.tr_time = door.time;
        body.pos.tr_delta = rotate_yaw(door.velocity, yaw);
        body
    }

    /// Leg 1: shots from spots around the entry door that cross it first.
    fn leg_launch(&self, route: &Route) -> Vec<Flight> {
        let door = route.enter.door;
        let mut spots = route
            .launch_feet
            .map_or_else(|| self.shooter_spots(&door), |feet| vec![feet]);
        if let Some(every) = std::env::var("IW4L_SOLVE_SPOT_EVERY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            spots = spots.into_iter().step_by(every.max(1)).collect();
        }
        announce(&format!("{} shooter spots", spots.len()));
        let aims: Vec<_> = spots
            .iter()
            .flat_map(|&feet| {
                let eye = add(feet, [0.0, 0.0, route.launch_eye_height]);
                let target = add(door.base, [0.0, 0.0, door.height * 0.5]);
                let d = sub(target, eye);
                let yaw0 = d[1].atan2(d[0]).to_degrees();
                let pitch0 = -(d[2].atan2((d[0] * d[0] + d[1] * d[1]).sqrt())).to_degrees();
                let mut aims = Vec::new();
                for i in
                    (-route.aim_yaw_span_deg..=route.aim_yaw_span_deg).step_by(AIM_COARSE as usize)
                {
                    for j in (-route.aim_pitch_span_deg..=route.aim_pitch_span_deg)
                        .step_by(AIM_COARSE as usize)
                    {
                        let angles = [short_angle(pitch0 + j as f32), short_angle(yaw0 + i as f32)];
                        if angles[0].abs() < 85.0 {
                            aims.push((feet, angles));
                        }
                    }
                }
                aims
            })
            .collect();
        announce(&format!("{} aims", aims.len()));
        let attempts = par_map(&aims, |&(feet, angles)| {
            let (flight, miss) = self.launch(route, feet, angles);
            vec![(flight, miss, feet, angles)]
        });
        let mut near: Vec<_> = attempts
            .iter()
            .filter(|(flight, miss, _, _)| flight.is_none() && miss.is_finite())
            .map(|(_, miss, feet, angles)| (*miss, *feet, *angles))
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(64);
        let coarse = keep_best(
            attempts
                .into_iter()
                .filter_map(|(flight, _, _, _)| flight)
                .collect(),
            route.keep_per_spot,
        );
        let seeds: Vec<_> = coarse
            .iter()
            .map(|flight| (flight.launch.feet, flight.launch.angles))
            .chain(near.into_iter().map(|(_, feet, angles)| (feet, angles)))
            .collect();
        let shoot = |&(feet, angles): &([f32; 3], [f32; 2])| -> Vec<Flight> {
            self.shoot(route, feet, angles).into_iter().collect()
        };
        // The fine step around the coarse shots that were kept.
        let fine: Vec<_> = seeds
            .into_iter()
            .flat_map(|(feet, [pitch, yaw])| {
                let steps = route.aim_fine_steps;
                (-steps..=steps)
                    .flat_map(move |i| (-steps..=steps).map(move |j| (i, j)))
                    .filter(|&ij| ij != (0, 0))
                    .map(move |(i, j)| {
                        (
                            feet,
                            [
                                short_angle(pitch + j as f32 / steps as f32),
                                short_angle(yaw + i as f32 / steps as f32),
                            ],
                        )
                    })
            })
            .filter(|(_, angles)| angles[0].abs() < 85.0)
            .collect();
        announce(&format!(
            "{} kept, {} fine aims around them",
            coarse.len(),
            fine.len()
        ));
        let mut flights = coarse;
        flights.extend(par_map(&fine, shoot));
        flights
    }

    /// The shot from `feet` at `angles`, when it crosses the entry door first
    /// without rattling.
    fn shoot(&self, route: &Route, feet: [f32; 3], angles: [f32; 2]) -> Option<Flight> {
        self.launch(route, feet, angles).0
    }

    fn launch(&self, route: &Route, feet: [f32; 3], angles: [f32; 2]) -> (Option<Flight>, f32) {
        let mut entry = portal(&route.enter, &route.exits[0], false);
        entry.a = route.enter.door;
        entry.b.width = entry.a.width;
        entry.b.height = entry.a.height;
        entry.b.two_sided = true;
        self.launch_on(route, feet, angles, entry)
    }

    fn launch_on(
        &self,
        route: &Route,
        feet: [f32; 3],
        angles: [f32; 2],
        entry: AtlasPortal,
    ) -> (Option<Flight>, f32) {
        let atlas = self.atlas(vec![entry]);
        let facts = self.equipment;
        {
            let eye = add(feet, [0.0, 0.0, route.launch_eye_height]);
            let mut motion = sim::ProjectileLaunch {
                tick: sim::Tick(0),
                origin: eye,
                angles: [angles[0], angles[1], 0.0],
                owner_velocity: route.launch_velocity,
                combat_seed: 0,
            }
            .grenade_motion(
                &facts,
                if facts.is_offhand() {
                    GrenadeLaunchKind::Thrown {
                        remaining_fuse_ms: None,
                    }
                } else {
                    GrenadeLaunchKind::Launcher
                },
            );
            motion.pos.tr_delta = motion
                .pos
                .tr_delta
                .map(|speed| speed * self.rules.speed_scale);
            let body = ProjectileFlightBody {
                origin: eye,
                pos: motion.pos,
                travel: 0.0,
            };
            let mut flight = Flight {
                launch: Launch { feet, angles, body },
                choice: Choice::default(),
                door: None,
                path: vec![eye],
                bounces: Vec::new(),
                last_bounce_ms: None,
                victim: None,
                home_target: None,
                entered_at: [Some(0), None, None],
                time: 0,
            };
            let mut moving = body;
            let stop = self.fly(&atlas, &mut moving, 0, TICK, LAUNCH_MS, &mut flight);
            if std::env::var_os("IW4L_SOLVE_DEBUG").is_some() {
                eprintln!(
                    "launch: {stop:?} angles={angles:?} t={} bounces={} end={:?}",
                    flight.time,
                    flight.bounces.len(),
                    moving.origin
                );
            }
            let target = add(
                route.enter.door.base,
                [0.0, 0.0, route.enter.door.height * 0.5],
            );
            let mut miss = f32::INFINITY;
            for (i, &point) in flight.path.iter().enumerate() {
                if flight
                    .bounces
                    .iter()
                    .filter(|(index, _)| *index <= i)
                    .count()
                    > self.max_bounces_per_world[0]
                {
                    break;
                }
                if i as i32 * TICK >= route.min_world_ms[0] {
                    miss = miss.min(dist(point, target));
                }
            }
            let accepted = match stop {
                Stop::Door(0, true, door) => {
                    flight.door = Some(door);
                    (!flight.rattles(self.max_bounces_per_s, self.max_bounces_per_world)
                        && flight.ms_in(0) >= route.min_world_ms[0])
                        .then_some(flight)
                }
                _ => None,
            };
            (accepted, miss)
        }
    }

    /// Leg 2: through `portals[enter]` (from its a side), then on until any
    /// other portal is crossed from its a side, no sooner than `min_ms` after
    /// the first door; with the portal it left by. A flight that does not
    /// gives how near it passed an exit late enough without rattling, to
    /// steer the shot by.
    fn leg_through(
        &self,
        from: &Flight,
        portals: Vec<AtlasPortal>,
        enter: usize,
        min_ms: i32,
    ) -> Result<(Flight, usize), f32> {
        let atlas = self.atlas(portals.clone());
        let door = from.door.ok_or(f32::MAX)?;
        let approach = door.velocity.map(|v| v * 0.01);
        if !atlas
            .crossing(sub(door.at, approach), add(door.at, approach))
            .is_some_and(|(_, id, from_a)| id.index() == enter && from_a)
        {
            return Err(f32::MAX);
        }
        let mut flight = from.clone();
        let mut body = self.through(&atlas, enter, true, &door, from.launch.body);
        flight.path.push(body.origin);
        let deadline = door.tick_end + min_ms + LEG_MS;
        let stop = self.fly(
            &atlas,
            &mut body,
            door.time,
            door.tick_end,
            deadline,
            &mut flight,
        );
        if std::env::var_os("IW4L_SOLVE_DEBUG").is_some() {
            let what = match &stop {
                Stop::Door(portal, from_a, _) => format!("door {portal} from_a={from_a}"),
                Stop::Deadline => "deadline".into(),
                Stop::Oblique => "oblique".into(),
                Stop::Stalled => "stalled".into(),
                Stop::Out => "out".into(),
            };
            eprintln!(
                "leg: {what} after {} ticks, {} bounces, start {:?} end {:?}",
                flight.path.len() - from.path.len(),
                flight.bounces.len() - from.bounces.len(),
                flight.path[from.path.len()],
                flight.path.last()
            );
        }
        if let Stop::Door(portal, true, next) = stop
            && portal != enter
            && next.time - door.time >= min_ms
        {
            flight.door = Some(next);
            if !flight.rattles(self.max_bounces_per_s, self.max_bounces_per_world) {
                return Ok((flight, portal));
            }
        }
        // The exits' middles, in shared coordinates.
        let exits: Vec<[f32; 3]> = (0..portals.len())
            .filter(|&p| p != enter)
            .map(|p| {
                let a = portals[p].a;
                add(
                    add(a.base, world_offset(a.world.index())),
                    [0.0, 0.0, a.height * 0.5],
                )
            })
            .collect();
        let first = from.path.len() + (min_ms / TICK).max(0) as usize;
        let mut miss = f32::MAX;
        let mut so_far = from.clone();
        for k in from.path.len()..flight.path.len() {
            so_far.path.push(flight.path[k]);
            so_far
                .bounces
                .extend(flight.bounces.iter().filter(|b| b.0 == k));
            if so_far.rattles(self.max_bounces_per_s, self.max_bounces_per_world) {
                break;
            }
            if k >= first {
                miss = exits
                    .iter()
                    .fold(miss, |m, &e| m.min(dist(flight.path[k], e)));
            }
        }
        Err(miss)
    }

    /// Leg 3: out of the bridge into world 2, until a spot a standing victim
    /// would take the projectile in the chest, late enough for the route;
    /// of those spots, the first that the most ground has been covered by.
    fn leg_last(&self, route: &Route, from: &Flight) -> Option<Flight> {
        let mut flight = from.clone();
        let atlas = route_atlas(route, self, from);
        let door = from.door?;
        let mut body = self.through(&atlas, 1, true, &door, from.launch.body);
        let last_start = flight.path.len();
        flight.path.push(body.origin);
        let deadline = route.max_flight_ms;
        // Wherever it stops, the victim is placed on the way there.
        let stop = self.fly(
            &atlas,
            &mut body,
            door.time,
            door.tick_end,
            deadline,
            &mut flight,
        );
        if std::env::var_os("IW4L_SOLVE_DEBUG").is_some() {
            let last = &flight.path[last_start..];
            eprintln!(
                "last: {:?} t={} start_t={} b={} {} ticks, start {:?} end {:?}",
                stop,
                flight.time,
                door.time,
                flight.bounces_in(2),
                last.len(),
                last.first(),
                last.last()
            );
        }
        let time_at = |k: usize| door.tick_end + (k - last_start - 1) as i32 * TICK;
        let exit = add(route.landing.door.base, world_offset(2));
        // The player's box: the first point of the last leg inside it is the
        // hit, wherever the victim was placed.
        let inside = |q: &[f32; 3], floor: [f32; 3]| {
            let flat = ((q[0] - floor[0]).powi(2) + (q[1] - floor[1]).powi(2)).sqrt();
            flat <= 16.0 && q[2] >= floor[2] - 2.0 && q[2] <= floor[2] + 72.0
        };
        let enough = |k: usize| {
            time_at(k) >= route.min_flight_ms
                && time_at(k) <= route.max_flight_ms
                && time_at(k) - door.time >= route.min_world_ms[2]
        };
        let mut cells = std::collections::HashSet::new();
        let covered: Vec<usize> = flight.path[last_start..]
            .iter()
            .map(|p| {
                cells.insert(((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32));
                cells.len()
            })
            .collect();
        let spots = (last_start + 2..flight.path.len())
            .filter(|&i| enough(i))
            .flat_map(|i| {
                let flight = &flight;
                let delta = sub(flight.path[i], flight.path[i - 1]);
                let length = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt();
                let direction = if length > 0.001 {
                    [delta[0] / length, delta[1] / length, 0.0]
                } else {
                    [0.0; 3]
                };
                [0.0, 8.0, 16.0].into_iter().filter_map(move |advance| {
                    let query = add(flight.path[i], direction.map(|v| v * advance));
                    let floor = self.victim_floor(query)?;
                    let hit = (last_start + 1..=i).find(|&k| inside(&flight.path[k], floor))?;
                    // A bounce inside the last tick would change the segment the
                    // victim is struck on.
                    let bounces = flight
                        .bounces
                        .iter()
                        .filter(|b| (last_start..hit).contains(&b.0))
                        .count();
                    let seconds = (time_at(hit) - door.time) as f32 / 1000.0;
                    let fine = enough(hit)
                        && dist(flight.path[hit], exit) >= 48.0
                        && !flight.bounces.iter().any(|b| b.0 == hit)
                        && bounces as f32 <= self.max_bounces_per_s * seconds + 1e-4
                        && bounces <= self.max_bounces_per_world[2];
                    fine.then_some((hit, floor))
                })
            });
        // The same trade as `Flight::score`, within the last world.
        let worth = |hit: usize| {
            let bounces = flight
                .bounces
                .iter()
                .filter(|b| (last_start..hit).contains(&b.0))
                .count();
            (covered[hit - last_start] as f32).sqrt() - BOUNCE_COST * bounces as f32
        };
        let (i, floor) = spots
            .rev()
            .max_by(|a, b| worth(a.0).total_cmp(&worth(b.0)))?;
        flight.path.truncate(i + 1);
        flight.bounces.retain(|&(tick, _)| tick < i);
        flight.time = time_at(i);
        flight.victim = Some(floor);
        Some(flight)
    }

    /// The floor under a projectile point when a player standing there would
    /// take it in the chest and has room to stand.
    fn victim_floor(&self, p: [f32; 3]) -> Option<[f32; 3]> {
        let (floor, normal) = self.trace(p, sub(p, [0.0, 0.0, VICTIM_HEIGHT.1 + 4.0]))?;
        let height = p[2] - floor[2];
        if normal[2] < 0.9 || height < VICTIM_HEIGHT.0 || height > VICTIM_HEIGHT.1 {
            return None;
        }
        let feet = add(floor, [0.0, 0.0, 1.0]);
        let (lo, hi) = self.bounds[world_at(feet).min(2)];
        if (0..3).any(|i| feet[i] < lo[i] || feet[i] > hi[i]) {
            return None;
        }
        if self.trace(feet, add(feet, [0.0, 0.0, 72.0])).is_some() {
            return None;
        }
        for z in [8.0, 40.0, 68.0] {
            let center = add(feet, [0.0, 0.0, z]);
            for d in [
                [18.0, 0.0, 0.0],
                [-18.0, 0.0, 0.0],
                [0.0, 18.0, 0.0],
                [0.0, -18.0, 0.0],
            ] {
                if self.trace(center, add(center, d)).is_some() {
                    return None;
                }
            }
        }
        self.stands(feet).then_some(floor)
    }

    /// Standing spots on floors around the door, on both sides.
    fn shooter_spots(&self, door: &AtlasDoor) -> Vec<[f32; 3]> {
        let mut spots = Vec::new();
        for i in -14..=14 {
            for j in -14..=14 {
                let column = add(door.base, [i as f32 * 40.0, j as f32 * 40.0, 0.0]);
                let flat = ((i * i + j * j) as f32).sqrt() * 40.0;
                if !(96.0..=600.0).contains(&flat) {
                    continue;
                }
                for top in [100.0, -40.0] {
                    let start = add(column, [0.0, 0.0, top]);
                    let Some((floor, normal)) = self.trace(start, sub(start, [0.0, 0.0, 400.0]))
                    else {
                        continue;
                    };
                    if normal[2] < 0.9 || floor[2] < door.base[2] - 200.0 {
                        continue;
                    }
                    let feet = add(floor, [0.0, 0.0, 0.125]);
                    if self.standing_room(feet) && !spots.iter().any(|s| dist(*s, feet) < 1.0) {
                        spots.push(feet);
                    }
                }
            }
        }
        spots
    }

    fn standing_room(&self, feet: [f32; 3]) -> bool {
        let up = add(feet, [0.0, 0.0, 1.0]);
        if self.trace(up, add(up, [0.0, 0.0, 72.0])).is_some() {
            return false;
        }
        [8.0, 40.0, 68.0].iter().all(|&z| {
            let center = add(feet, [0.0, 0.0, z]);
            [
                [20.0, 0.0, 0.0],
                [-20.0, 0.0, 0.0],
                [0.0, 20.0, 0.0],
                [0.0, -20.0, 0.0],
            ]
            .iter()
            .all(|d| self.trace(center, add(center, *d)).is_none())
        })
    }

    /// The whole flight again on the final atlas, as the game flies it: the
    /// door order and the end point must match the legs.
    fn replay(&self, atlas: &Atlas, flight: &Flight) -> Option<Replay> {
        let rules = self.rules;
        let mut body = flight.launch.body;
        let mut doors = Vec::new();
        let mut entries = Vec::new();
        let mut bounces = 0;
        let mut bounce_times = Vec::new();
        let mut prev = 0;
        let mut time = TICK;
        let victim = flight.victim?;
        let end = *flight.path.last()?;
        let mut path = vec![body.origin];
        while time <= flight.time {
            for event in projectile_step(
                sim::projectile_flight::ProjectileFlightScene {
                    atlas,
                    base: self.clip(),
                    players: &[],
                    blocked: &[],
                },
                None,
                &rules,
                &mut body,
                prev,
                time,
            ) {
                match event {
                    ProjectileFlightEvent::Portal {
                        portal,
                        from_a,
                        time,
                        at,
                        velocity,
                    } => {
                        let (entered, _, _) = atlas.portals[portal.index()].oriented(from_a);
                        let angle = portal_entry_angle(velocity, entered.normal);
                        if angle > self.max_portal_angle_deg + 1e-4 {
                            return None;
                        }
                        entries.push(json!({"portal":portal.index(),"from_a":from_a,"time_ms":time,"at":at,"velocity":velocity,"normal":entered.normal,"angle_deg":angle}));
                        doors.push((portal.index(), from_a));
                    }
                    ProjectileFlightEvent::Bounce { .. } => {
                        bounces += 1;
                        bounce_times.push((prev, time));
                    }
                    ProjectileFlightEvent::Player { .. } => {}
                }
            }
            path.push(body.origin);
            prev = time;
            time += TICK;
        }
        let wanted = [(0, true), (1, true)];
        if doors != wanted || dist(body.origin, end) > 1.0 {
            announce(&format!(
                "replay: doors {doors:?}, end {:?} vs {end:?}",
                body.origin
            ));
            return None;
        }
        for entry in &mut entries {
            let at = entry["time_ms"].as_i64()? as i32;
            let before = at
                - bounce_times
                    .iter()
                    .filter(|(start, _)| *start < at)
                    .map(|(_, end)| (*end).min(at))
                    .max()
                    .unwrap_or(0);
            let after = (bounce_times
                .iter()
                .find(|(_, end)| *end > at)
                .map_or(flight.time, |(start, _)| *start)
                - at)
                .max(0);
            if before < self.min_portal_entry_clear_ms || after < self.min_portal_exit_clear_ms {
                return None;
            }
            entry["clear_before_ms"] = json!(before);
            entry["clear_after_ms"] = json!(after);
        }
        Some(Replay {
            entries,
            bounces,
            path,
            victim,
        })
    }
}

struct Replay {
    entries: Vec<Value>,
    bounces: usize,
    path: Vec<[f32; 3]>,
    victim: [f32; 3],
}

fn write_outputs(
    artifacts: &std::path::Path,
    route: &Route,
    atlas: &Atlas,
    flight: &Flight,
    check: &Replay,
    solver: &Solver,
) -> Result<(), String> {
    let door_json = |door: &AtlasDoor| -> Result<Value, String> {
        let native = std::iter::once(&route.enter)
            .chain(&route.middle)
            .chain(&route.exits)
            .chain(&route.landings)
            .find(|d| d.door.world == door.world && d.door.base == door.base)
            .ok_or("solved door has no native catalog geometry")?;
        let mut endpoint =
            assets::fit_portal_endpoint(&native.source, door.width as f64, door.height as f64);
        endpoint["world"] = json!(door.world.index());
        endpoint["normal"] = json!(door.normal);
        Ok(endpoint)
    };
    let pairs = atlas
        .portals
        .iter()
        .map(|p| {
            Ok(json!({
                "a": door_json(&p.a)?, "b": door_json(&p.b)?, "flip": p.flip,
                "shared_aperture": {"width": p.a.width, "height": p.a.height}
            }))
        })
        .collect::<Result<Vec<Value>, String>>()?;
    let (x, _) = flight.choice.x.expect("x");
    let launch = &flight.launch;
    let victim = check.victim;
    // Side-on to the last leg of the flight: the hit volumes are the bones,
    // and a grenade along the facing slips between the legs.
    let victim_yaw = match check.path.as_slice() {
        [.., from, to] => (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees() + 90.0,
        _ => 0.0,
    };
    let shot = json!({
        "weapon": route.weapon,
        "max_portal_angle_deg": route.max_portal_angle_deg,
        "portal_entries": check.entries,
        "doors": [route.enter.id, route.middle[x].id, route.exits[flight.choice.exit].id, route.landings[flight.choice.landing].id],
        "shooter": { "feet": launch.feet, "pitch": launch.angles[0], "yaw": launch.angles[1], "owner_velocity": route.launch_velocity, "eye_height": route.launch_eye_height },
        "requires_input_program": route.launch_feet.is_some(),
        "launch": { "base": launch.body.pos.tr_base, "delta": launch.body.pos.tr_delta },
        "victim": { "feet": victim, "yaw": victim_yaw },
        "flight_ms": flight.time,
        "bounces": check.bounces,
        "bounces_per_world": (0..3).map(|w| flight.bounces_in(w)).collect::<Vec<_>>(),
        "ms_per_world": (0..3).map(|w| flight.ms_in(w)).collect::<Vec<_>>(),
        "cmds": if route.launch_feet.is_some() { format!("bot tp all {} {} {} {} 0", victim[0], victim[1], victim[2] + 1.0, victim_yaw) } else { format!(
            "move {} {} {} {} {}; bot tp all {} {} {} {} 0",
            launch.feet[0], launch.feet[1], launch.feet[2], launch.angles[1], launch.angles[0],
            victim[0], victim[1], victim[2] + 1.0, victim_yaw,
        ) },
    });
    // The final killcam replays the whole flight and the walk before it.
    let killcam_s = (flight.time + KILLCAM_LEAD_MS + 999) / 1000;
    let mut map = json!({
        "schema_version":1,
        "worlds": route.zones,
        "dvars": {
            "scr_killcam_time": killcam_s.to_string(),
            "projectile_parallel": route.rules.parallel.to_string(),
            "projectile_perpendicular": route.rules.perpendicular.to_string(),
            "projectile_lift": route.rules.lift.to_string(),
            "projectile_home": route.rules.home.to_string(),
            "projectile_home_align": route.rules.home_align.to_string(),
            "projectile_gravity_scale": route.rules.gravity_scale.to_string(),
            "projectile_speed_scale": route.rules.speed_scale.to_string(),
            // The archive must hold the whole killcam; core keeps 32 MiB.
            (net::ARCHIVE_MB_DVAR): (ARCHIVE_MB_PER_S * (killcam_s + KILLCAM_LEAD_MS / 1000)).max(32).to_string(),
        },
        "shot": shot,
        "aperture_version":4,
        "generator": {"algorithm":"shot-search", "version":1, "route":route.source, "tick_ms":TICK},
        "portals": pairs,
    });
    if let Some(target) = route.rules.home_target {
        for (key, value) in [
            "projectile_home_x",
            "projectile_home_y",
            "projectile_home_z",
        ]
        .into_iter()
        .zip(target)
        {
            map["dvars"][key] = json!(value.to_string());
        }
    }
    let scene = assets::MergedMap {
        name: route.name.clone(),
        path: artifacts.join("maps").join(format!("{}.json", route.name)),
        worlds: route.zones.clone(),
        recipe: map.clone(),
    };
    assets::validate_scene_version(&scene.recipe)?;
    let worlds = solver
        .native_clips
        .iter()
        .enumerate()
        .map(|(index, clip)| {
            let clip = session::prepare_scene_collision(clip.clone(), &scene, index)?;
            let (brushes, bsp, mesh, cmodels) = session::sim_clip_map(&clip);
            Ok(Arc::new(AtlasWorld {
                zone: route.zones[index].clone(),
                brushes,
                bsp,
                mesh,
                cmodels,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut prepared_solver = solver.clone();
    prepared_solver.base = worlds[0].clone();
    prepared_solver.others = worlds[1..].to_vec();
    let mut prepared_atlas = atlas.clone();
    prepared_atlas.worlds = prepared_solver.others.clone();
    let prepared_check = prepared_solver
        .replay(&prepared_atlas, flight)
        .ok_or("shot diverges after applying the runtime scene collision")?;
    if prepared_check.path != check.path {
        return Err("shot trajectory differs after applying the runtime scene collision".into());
    }
    let path = json!({ "path": check.path });
    for (path, value) in [
        (
            artifacts.join("maps").join(format!("{}.json", route.name)),
            map,
        ),
        (
            artifacts
                .join("content")
                .join("shots")
                .join(&route.name)
                .join("trajectory.json"),
            path,
        ),
    ] {
        let dir = path.parent().expect("dir");
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let text = serde_json::to_string_pretty(&value).expect("json");
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        announce(&format!("wrote {}", path.display()));
    }
    announce(&format!(
        "flight {} ms {:?}, {} bounces {:?}, doors {} → {} → {}",
        flight.time,
        (0..3).map(|w| flight.ms_in(w)).collect::<Vec<_>>(),
        check.bounces,
        (0..3).map(|w| flight.bounces_in(w)).collect::<Vec<_>>(),
        route.enter.id,
        route.middle[x].id,
        route.exits[flight.choice.exit].id,
    ));
    Ok(())
}

/// Rounds to the 16-bit angle grid usercmds carry.
fn short_angle(degrees: f32) -> f32 {
    let step = 360.0 / 65536.0;
    (degrees / step).round() * step
}

fn iproduct(a: usize, b: usize) -> Vec<(usize, usize)> {
    (0..a).flat_map(|i| (0..b).map(move |j| (i, j))).collect()
}

/// Maps every item on all cores; the results keep the items' order.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> Vec<R> + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut rows: Vec<(usize, Vec<R>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut rows = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(i) else {
                            break rows;
                        };
                        rows.push((i, f(item)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("solver thread"))
            .collect()
    });
    rows.sort_by_key(|(i, _)| *i);
    rows.into_iter().flat_map(|(_, row)| row).collect()
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// Full 3D deviation from a doorway's normal; either face may be entered.
fn portal_entry_angle(velocity: [f32; 3], normal: [f32; 3]) -> f32 {
    let length = |v: [f32; 3]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let denominator = length(velocity) * length(normal);
    if denominator < 1e-6 {
        return 90.0;
    }
    let cosine = velocity
        .iter()
        .zip(normal)
        .map(|(v, n)| v * n)
        .sum::<f32>()
        .abs()
        / denominator;
    cosine.clamp(0.0, 1.0).acos().to_degrees()
}
