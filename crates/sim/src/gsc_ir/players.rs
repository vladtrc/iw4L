use super::runtime::{raise, run_now};
use super::*;
use crate::frame::FrameWorld;
use crate::world::ClientId;
use bevy_ecs::prelude::World;

const CONNECT: &str = "maps/mp/gametypes/_callbacksetup::codecallback_playerconnect";
const DISCONNECT: &str = "maps/mp/gametypes/_callbacksetup::codecallback_playerdisconnect";
const DAMAGE: &str = "maps/mp/gametypes/_callbacksetup::codecallback_playerdamage";
pub(super) const KILLED: &str = "maps/mp/gametypes/_callbacksetup::codecallback_playerkilled";
pub(super) const LAST_STAND: &str =
    "maps/mp/gametypes/_callbacksetup::codecallback_playerlaststand";

pub(super) fn now_ms(world: &World) -> i64 {
    i64::from(world.resource::<crate::step::StepRequest>().tick.0) * i64::from(crate::MATCH_TICK_MS)
}

pub(super) fn player_object(world: &World, client: u32) -> Value {
    world
        .resource::<Runtime>()
        .players
        .get(&client)
        .map_or(Value::Undefined, |slot| Value::Object(slot.object))
}

pub(crate) fn player_damage(world: &mut World, tick: crate::Tick, hit: &crate::script_player::Hit) {
    let victim = player_object(world, hit.victim.0);
    if victim == Value::Undefined {
        return;
    }
    let attacker = match hit.attacker.map(|a| player_object(world, a.0)) {
        Some(attacker) if attacker != Value::Undefined => attacker,
        _ => world_entity(world),
    };
    let weapon = crate::script_player::weapon_name(&FrameWorld::from_world(world), hit.weapon);
    let hitloc = weapon_iw4::HITLOC_NAMES
        .get(usize::from(hit.hitloc))
        .copied()
        .unwrap_or("none");
    let inflictor = hit
        .inflictor
        .and_then(|id| projectile_entity(world, id, hit))
        .unwrap_or_else(|| attacker.clone());
    let args = vec![
        inflictor,
        attacker,
        Value::Int(hit.amount),
        Value::Int(hit.flags),
        Value::string(hit.means),
        Value::String(weapon.into()),
        Value::Vector(hit.point),
        Value::Vector(hit.dir),
        Value::string(hitloc),
        Value::Int(0),
    ];
    world.resource_mut::<Runtime>().current_hit = Some(hit.clone());
    let now = i64::from(tick.0) * i64::from(crate::MATCH_TICK_MS);
    let result = run_now(world, DAMAGE, victim, args, now);
    world.resource_mut::<Runtime>().current_hit = None;
    if result.is_ok() {
        // Deaths from this hit are settled before the caller continues.
        settle_deaths(world);
    }
}

fn world_entity(world: &World) -> Value {
    world
        .resource::<Runtime>()
        .engine
        .world
        .map_or(Value::Undefined, Value::Object)
}

fn projectile_entity(
    world: &mut World,
    id: crate::ProjectileId,
    hit: &crate::script_player::Hit,
) -> Option<Value> {
    let mut runtime = world.resource_mut::<Runtime>();
    let object = runtime
        .entities
        .iter()
        .find(|(_, e)| matches!(e.kind, entities::EntityKind::Missile(of) if of == id))
        .map(|(object, _)| *object)?;
    if hit.flags & crate::script_player::IDFLAGS_RADIUS != 0 {
        runtime.set_object_field(object, "origin", Value::Vector(hit.point));
    }
    Some(Value::Object(object))
}

pub(crate) fn flashbang(
    world: &mut World,
    target: ClientId,
    origin: [f32; 3],
    amount_distance: f32,
    amount_angle: f32,
    attacker: ClientId,
) {
    let victim = player_object(world, target.0);
    if victim == Value::Undefined {
        return;
    }
    let attacker = player_object(world, attacker.0);
    raise(
        world,
        victim,
        "flashbang",
        vec![
            Value::Vector(origin),
            Value::Float(amount_distance.into()),
            Value::Float(amount_angle.into()),
            attacker,
        ],
    );
}

pub(super) fn owe(world: &mut World, client: u32, callback: &'static str, args: Vec<Value>) {
    world
        .resource_mut::<Runtime>()
        .deaths
        .push_back((client, callback, args));
}

pub(super) fn suicide(world: &mut World, tick: crate::Tick, client: u32) {
    let id = crate::ClientId(client);
    let mut frame = FrameWorld::from_world(world);
    if !frame
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == crate::ClientLifecycle::Alive)
    {
        return;
    }
    crate::script_player::kill(&mut frame, tick, id, Some(id), None);
    let me = player_object(world, client);
    owe(
        world,
        client,
        KILLED,
        vec![
            me.clone(),
            me,
            Value::Int(100_000),
            Value::string("MOD_SUICIDE"),
            Value::string("none"),
            Value::Vector([0.0; 3]),
            Value::string("none"),
            Value::Int(0),
            Value::Int(0),
        ],
    );
}

pub(crate) fn force_death(world: &mut World, tick: crate::Tick, client: u32) {
    suicide(world, tick, client);
    settle_deaths(world);
}

pub(crate) fn settle_deaths(world: &mut World) {
    let now = now_ms(world);
    loop {
        let Some((client, callback, args)) = world.resource_mut::<Runtime>().deaths.pop_front()
        else {
            return;
        };
        let victim = player_object(world, client);
        if victim == Value::Undefined {
            continue;
        }
        if run_now(world, callback, victim, args, now).is_err() {
            return;
        }
    }
}

pub(crate) const TEAM_MENU: &str = "team_marinesopfor";
const CLASS_MENU: &str = "changeclass";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MenuAnswer {
    menu: Arc<str>,
    response: Arc<str>,
    data: Vec<(String, Value)>,
}

pub(crate) fn answer_menu(world: &mut World, client: u32, menu: &str, response: &str) {
    push_answer(
        world,
        client,
        MenuAnswer {
            menu: menu.into(),
            response: response.into(),
            data: Vec::new(),
        },
    );
}

pub(crate) fn answer_join(world: &mut World, client: u32) {
    // A spectator may pick a class; the team is autoassigned only once.
    if world.resource_mut::<Runtime>().joined.insert(client) {
        answer_menu(world, client, TEAM_MENU, "autoassign");
    }
}

pub(crate) fn note_team_answer(world: &mut World, client: u32, menu: &str) {
    if menu == TEAM_MENU {
        world.resource_mut::<Runtime>().joined.insert(client);
    }
}

fn push_answer(world: &mut World, client: u32, answer: MenuAnswer) {
    world
        .resource_mut::<Runtime>()
        .menu_answers
        .entry(client)
        .or_default()
        .push_back(answer);
}

const T5_DEFAULT_CLASSES: [&str; 5] = ["smg_mp", "cqb_mp", "assault_mp", "lmg_mp", "sniper_mp"];

pub(crate) fn choose_default_class(world: &mut World, client: u32, index: u8) {
    let realm = world
        .resource::<Runtime>()
        .program
        .as_ref()
        .map(|p| p.realm());
    let response = match realm {
        Some(super::Realm::T5) => {
            T5_DEFAULT_CLASSES[index as usize % T5_DEFAULT_CLASSES.len()].to_owned()
        }
        _ => format!("class{index}"),
    };
    answer_menu(world, client, CLASS_MENU, &response);
}

const T5_PRESET_CLASSES: [&str; 5] = ["assault_mp", "smg_mp", "cqb_mp", "sniper_mp", "assault_mp"];

pub(crate) fn choose_class(world: &mut World, client: u32, class: &crate::ClassDef) {
    let realm = world
        .resource::<Runtime>()
        .program
        .as_ref()
        .map(|p| p.realm());
    if realm == Some(super::Realm::T5) {
        let response = T5_PRESET_CLASSES[class.id.0 as usize % T5_PRESET_CLASSES.len()];
        answer_menu(world, client, CLASS_MENU, response);
        return;
    }
    let index = class.id.0 as usize % 10;
    let frame = FrameWorld::from_world(world);
    let name = |weapon: u32| -> String {
        if weapon == 0 {
            "none".into()
        } else {
            frame.weapon_script_name(weapon).to_owned()
        }
    };
    let setup = |weapon: u32| -> (String, [String; 2]) {
        let full = name(weapon);
        let stem = full.strip_suffix("_mp").unwrap_or(&full);
        let mut tokens = stem.split('_');
        let base = tokens.next().unwrap_or("none").to_owned();
        let mut attachments = [String::from("none"), String::from("none")];
        for (slot, token) in attachments.iter_mut().zip(tokens) {
            *slot = token.to_owned();
        }
        (base, attachments)
    };
    let prefix = format!("customclasses.{index}");
    let mut data = Vec::new();
    let mut put = |key: String, value: &str| data.push((key, Value::String(value.into())));
    for (setup_index, weapon) in [class.primary, class.secondary].into_iter().enumerate() {
        let (base, attachments) = setup(weapon);
        let key = format!("{prefix}.weaponsetups.{setup_index}");
        put(format!("{key}.weapon"), &base);
        put(format!("{key}.attachment.0"), &attachments[0]);
        put(format!("{key}.attachment.1"), &attachments[1]);
        put(format!("{key}.camo"), "none");
    }
    put(format!("{prefix}.perks.0"), &name(class.lethal));
    let mut perks = ["specialty_null"; 3];
    for id in class.perks {
        if let (Some(slot), Some(perk)) = (
            crate::match_state::perk_slot_from_class_catalog(id),
            crate::match_state::class_catalog_perk_name(id),
        ) {
            perks[slot] = perk;
        }
    }
    for (slot, perk) in perks.iter().enumerate() {
        put(format!("{prefix}.perks.{}", slot + 1), perk);
    }
    let deathstreak = if class.deathstreak.is_empty() {
        "specialty_null"
    } else {
        &class.deathstreak
    };
    put(format!("{prefix}.perks.4"), deathstreak);
    let tactical = name(class.tactical);
    put(
        format!("{prefix}.specialgrenade"),
        tactical.strip_suffix("_mp").unwrap_or(&tactical),
    );

    push_answer(
        world,
        client,
        MenuAnswer {
            menu: CLASS_MENU.into(),
            response: format!("custom{}", index + 1).into(),
            data,
        },
    );
}

fn menu_kind(menu: &str) -> Option<bool> {
    if menu == TEAM_MENU {
        Some(false)
    } else if menu.starts_with(CLASS_MENU) {
        Some(true)
    } else {
        None
    }
}

fn deliver_answers(world: &mut World, client: u32) {
    // Answers queued ahead of the open menu's kind were for a menu the scripts skipped.
    let runtime = world.resource::<Runtime>();
    let Some(slot) = runtime.players.get(&client) else {
        return;
    };
    if !slot.begun {
        return;
    }
    let skipped = slot
        .menu
        .as_deref()
        .and_then(menu_kind)
        .and_then(|open| {
            runtime
                .menu_answers
                .get(&client)?
                .iter()
                .position(|answer| menu_kind(&answer.menu) == Some(open))
        })
        .unwrap_or(0);
    if skipped > 0 {
        let mut runtime = world.resource_mut::<Runtime>();
        if let Some(queue) = runtime.menu_answers.get_mut(&client) {
            queue.drain(..skipped);
        }
    }
    let runtime = world.resource::<Runtime>();
    let Some(slot) = runtime.players.get(&client) else {
        return;
    };
    let Some(next) = runtime.menu_answers.get(&client).and_then(|q| q.front()) else {
        return;
    };
    let in_game = matches!(&*slot.sessionstate, "playing" | "dead");
    if slot.menu.is_none() && !(in_game && &*next.menu == CLASS_MENU) {
        return;
    }
    let object = slot.object;
    let mut runtime = world.resource_mut::<Runtime>();
    let answer = runtime
        .menu_answers
        .get_mut(&client)
        .and_then(VecDeque::pop_front)
        .expect("checked above");
    let slot = runtime.players.get_mut(&client).expect("checked above");
    slot.menu = None;
    slot.data.extend(answer.data);
    raise(
        world,
        Value::Object(object),
        "menuresponse",
        vec![Value::String(answer.menu), Value::String(answer.response)],
    );
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlayerSlot {
    pub object: u64,
    pub begun: bool,
    pub sessionstate: Arc<str>,
    pub dvars: BTreeMap<Arc<str>, Arc<str>>,
    pub menu: Option<Arc<str>>,
    pub data: BTreeMap<String, Value>,
    pub commands: Vec<(Arc<str>, Arc<str>)>,
    pub presented: BTreeMap<&'static str, Vec<Value>>,
    pub perks: std::collections::BTreeSet<Arc<str>>,
    pub spectate: BTreeMap<Arc<str>, bool>,
    pub seat: crate::ScriptSeat,
    pub weapon: u32,
    pub switching: bool,
    pub has_radar: bool,
    pub radar_mode: crate::RadarMode,
    pub radar_blocked: bool,
}

impl PlayerSlot {
    fn new(object: u64) -> Self {
        Self {
            object,
            begun: false,
            sessionstate: "spectator".into(),
            dvars: BTreeMap::new(),
            menu: None,
            data: BTreeMap::new(),
            commands: Vec::new(),
            presented: BTreeMap::new(),
            perks: Default::default(),
            spectate: BTreeMap::new(),
            seat: crate::ScriptSeat::default(),
            weapon: 0,
            switching: false,
            has_radar: false,
            radar_mode: crate::RadarMode::Normal,
            radar_blocked: false,
        }
    }
}

pub(crate) fn script_seats(world: &World) -> Vec<(ClientId, crate::ScriptSeat)> {
    world
        .resource::<Runtime>()
        .players
        .iter()
        .filter(|(_, slot)| {
            &*slot.sessionstate == "spectator"
                && slot.seat.archive_ms > 0
                && slot.seat.spectator_client >= 0
        })
        .map(|(client, slot)| (ClientId(*client), slot.seat))
        .collect()
}

const SEAT_FIELDS: [&str; 6] = [
    "forcespectatorclient",
    "killcamentity",
    "killcamentitylookat",
    "archivetime",
    "psoffsettime",
    "killcamlength",
];

const RADAR_FIELDS: [&str; 3] = ["hasradar", "radarmode", "isradarblocked"];

pub(super) fn publish_radar(world: &mut World) {
    let rows: Vec<(u32, bool, crate::RadarMode, bool)> = world
        .resource::<Runtime>()
        .players
        .iter()
        .map(|(client, slot)| (*client, slot.has_radar, slot.radar_mode, slot.radar_blocked))
        .collect();
    for (client, has, mode, blocked) in rows {
        let team = match load_field(world, client, "sessionteam") {
            Some(Value::String(team)) => team.to_string(),
            _ => String::new(),
        };
        let engine = &world.resource::<Runtime>().engine;
        let team_on = engine.team_radar.get(&team).is_some_and(|on| *on != 0);
        let team_blocked = engine.team_radar_blocked.contains(&team);
        let radar = if (has || team_on) && !blocked && !team_blocked {
            mode
        } else {
            crate::RadarMode::Off
        };
        let mut frame = FrameWorld::from_world(world);
        if frame.client_meta(ClientId(client)).is_some() {
            frame.client_meta_mut(ClientId(client)).radar = radar;
        }
    }
}

fn load_seat_field(seat: &crate::ScriptSeat, name: &str) -> Value {
    match name {
        "forcespectatorclient" => Value::Int(seat.spectator_client),
        "killcamentity" => Value::Int(seat.kill_cam_entity),
        "killcamentitylookat" => Value::Int(seat.look_at_entity),
        "archivetime" => Value::Float(seat.archive_ms as f32 / 1000.0),
        "psoffsettime" => Value::Int(seat.ps_offset_ms),
        _ => Value::Float(seat.length_ms as f32 / 1000.0),
    }
}

fn store_seat_field(seat: &mut crate::ScriptSeat, name: &str, value: &Value) -> Result<(), String> {
    let number = match value {
        Value::Int(n) => *n as f32,
        Value::Float(f) => *f,
        Value::Undefined => -1.0,
        other => return Err(format!("player field {name} takes a number, not {other:?}")),
    };
    let ms = (number.max(0.0) * 1000.0).round() as i32;
    match name {
        "forcespectatorclient" => seat.spectator_client = number as i32,
        "killcamentity" => seat.kill_cam_entity = number as i32,
        "killcamentitylookat" => seat.look_at_entity = number as i32,
        "archivetime" => seat.archive_ms = ms,
        "psoffsettime" => seat.ps_offset_ms = number as i32,
        _ => seat.length_ms = ms,
    }
    Ok(())
}

pub(crate) fn sync_players(world: &mut World) {
    let request = world.resource::<crate::step::StepRequest>();
    if !request.reason.advances_authority_world() {
        return;
    }
    let now = i64::from(request.tick.0) * i64::from(crate::MATCH_TICK_MS);
    let runtime = world.resource::<Runtime>();
    if runtime.program.is_none() || runtime.fault.is_some() || !runtime.started {
        return;
    }
    let clients: Vec<(u32, bool)> = {
        let frame = FrameWorld::from_world(world);
        frame
            .client_ids_sorted()
            .into_iter()
            .map(|id| {
                let joined = frame
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle != crate::ClientLifecycle::Connecting);
                (id.0, joined)
            })
            .collect()
    };
    let slots: Vec<(u32, PlayerSlot)> = world
        .resource::<Runtime>()
        .players
        .iter()
        .map(|(client, slot)| (*client, slot.clone()))
        .collect();
    for (client, slot) in &slots {
        if clients.iter().any(|(c, _)| c == client) {
            continue;
        }
        if run_now(
            world,
            DISCONNECT,
            Value::Object(slot.object),
            Vec::new(),
            now,
        )
        .is_err()
        {
            return;
        }
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.players.remove(client);
        runtime.menu_answers.remove(client);
        runtime.joined.remove(client);
        runtime.delete_entity(slot.object);
        let owned: Vec<u64> = runtime
            .entities
            .iter()
            .filter(|(_, e)| e.audience == entities::HudAudience::Client(*client))
            .map(|(id, _)| *id)
            .collect();
        drop(runtime);
        for id in owned {
            super::hud::destroy(world, id);
        }
    }
    for (client, joined) in clients {
        let slot = world.resource::<Runtime>().players.get(&client).cloned();
        match slot {
            Some(slot) if !slot.begun && joined => {
                raise(world, Value::Object(slot.object), "begin", Vec::new());
                if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
                    slot.begun = true;
                }
            }
            Some(_) => {}
            None => {
                let mut runtime = world.resource_mut::<Runtime>();
                let object = match runtime.create_player(client) {
                    Ok(object) => object,
                    Err(message) => {
                        runtime.fault = Some(Fault::at(
                            &Location {
                                module: "<engine>".into(),
                                function: "connect".into(),
                                line: 0,
                                column: 0,
                            },
                            message,
                        ));
                        return;
                    }
                };
                runtime.players.insert(client, PlayerSlot::new(object));
                match natives_math::new_array(world, Vec::new()) {
                    Ok(pers) => world
                        .resource_mut::<Runtime>()
                        .set_object_field(object, "pers", pers),
                    Err(_) => return,
                }
                if run_now(world, CONNECT, Value::Object(object), Vec::new(), now).is_err() {
                    return;
                }
            }
        }
        deliver_answers(world, client);
    }
    settle_deaths(world);
}

pub(crate) fn describe_players(world: &mut World) -> Vec<String> {
    let clients: Vec<(u32, PlayerSlot)> = world
        .resource::<Runtime>()
        .players
        .iter()
        .map(|(client, slot)| (*client, slot.clone()))
        .collect();
    clients
        .into_iter()
        .map(|(client, slot)| {
            let team = load_field(world, client, "sessionteam");
            let health = load_field(world, client, "health");
            let frame = FrameWorld::from_world(world);
            let weapon = frame.player(ClientId(client)).map_or(0, |ps| ps.weapon);
            let weapon = crate::script_player::weapon_name(&frame, weapon);
            format!(
                "client {client} {} team={:?} health={:?} weapon={weapon} menu={:?} perks={}",
                slot.sessionstate,
                team,
                health,
                slot.menu,
                slot.perks.len()
            )
        })
        .collect()
}

fn team_name(team: i32) -> &'static str {
    match team {
        entity_iw4::TEAM_AXIS => "axis",
        entity_iw4::TEAM_ALLIES => "allies",
        entity_iw4::TEAM_SPECTATOR => "spectator",
        _ => "none",
    }
}

fn client_name(name: &[u8]) -> String {
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    String::from_utf8_lossy(&name[..end]).into_owned()
}

pub(super) fn load_field(world: &mut World, client: u32, name: &str) -> Option<Value> {
    let id = ClientId(client);
    if name == "sessionstate" {
        let runtime = world.resource::<Runtime>();
        return runtime
            .players
            .get(&client)
            .map(|slot| Value::String(slot.sessionstate.clone()));
    }
    if SEAT_FIELDS.contains(&name) {
        let runtime = world.resource::<Runtime>();
        return runtime
            .players
            .get(&client)
            .map(|slot| load_seat_field(&slot.seat, name));
    }
    if RADAR_FIELDS.contains(&name) {
        let slot = world.resource::<Runtime>().players.get(&client)?;
        return Some(match name {
            "hasradar" => Value::Int(slot.has_radar.into()),
            "isradarblocked" => Value::Int(slot.radar_blocked.into()),
            _ => Value::string(match slot.radar_mode {
                crate::RadarMode::Fast => "fast_radar",
                crate::RadarMode::Constant => "constant_radar",
                _ => "normal_radar",
            }),
        });
    }
    let frame = FrameWorld::from_world(world);
    let ps = frame.player(id);
    let meta = frame.client_meta(id);
    Some(match name {
        "origin" => Value::Vector(ps.map_or([0.0; 3], |ps| ps.origin)),
        "angles" => Value::Vector(ps.map_or([0.0; 3], |ps| ps.viewangles)),
        "health" => Value::Int(ps.map_or(0, |ps| ps.health)),
        "maxhealth" => Value::Int(ps.map_or(0, |ps| ps.max_health)),
        "name" => Value::String(client_name(&meta?.name).into()),
        "score" => Value::Int(meta?.score),
        "kills" => Value::Int(meta?.kills),
        "deaths" => Value::Int(meta?.deaths),
        "sessionteam" => Value::string(team_name(meta?.client_state_team)),
        _ => return None,
    })
}

pub(super) fn entity_field(world: &mut World, id: u64, name: &str) -> Value {
    if let Some(client) = world.resource::<Runtime>().player_client(id)
        && let Some(value) = load_field(world, client, name)
    {
        return value;
    }
    world.resource_mut::<Runtime>().object_field(id, name)
}

pub(super) fn alive_on_team(world: &mut World, team: &str) -> i32 {
    let clients: Vec<u32> = world
        .resource::<Runtime>()
        .players
        .iter()
        .filter(|(_, slot)| &*slot.sessionstate == "playing")
        .map(|(client, _)| *client)
        .collect();
    let mut alive = 0;
    for client in clients {
        let on_team = load_field(world, client, "sessionteam") == Some(Value::string(team));
        let health = load_field(world, client, "health");
        if on_team && matches!(health, Some(Value::Int(n)) if n > 0) {
            alive += 1;
        }
    }
    alive
}

pub(super) fn store_field(
    world: &mut World,
    client: u32,
    name: &str,
    value: &Value,
) -> Result<bool, String> {
    let id = ClientId(client);
    let int = |value: &Value| match value {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(*f as i32),
        other => Err(format!("player field {name} takes a number, not {other:?}")),
    };
    if SEAT_FIELDS.contains(&name) {
        if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
            store_seat_field(&mut slot.seat, name, value)?;
        }
        return Ok(true);
    }
    if RADAR_FIELDS.contains(&name) {
        let mode = match (name, value) {
            ("radarmode", Value::String(mode)) => Some(match &**mode {
                "normal_radar" => crate::RadarMode::Normal,
                "fast_radar" => crate::RadarMode::Fast,
                "constant_radar" => crate::RadarMode::Constant,
                other => return Err(format!("invalid radarmode '{other}'")),
            }),
            ("radarmode", other) => return Err(format!("radarmode takes a string, not {other:?}")),
            _ => None,
        };
        let on = match mode {
            Some(_) => false,
            None => int(value)? != 0,
        };
        if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
            match (name, mode) {
                (_, Some(mode)) => slot.radar_mode = mode,
                ("hasradar", _) => slot.has_radar = on,
                _ => slot.radar_blocked = on,
            }
        }
        return Ok(true);
    }
    match name {
        "sessionstate" => {
            let Value::String(state) = value else {
                return Err("sessionstate takes a string".into());
            };
            if !matches!(&**state, "playing" | "dead" | "spectator" | "intermission") {
                return Err(format!("invalid sessionstate '{state}'"));
            }
            if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
                slot.sessionstate = state.clone();
            }
        }
        "origin" | "angles" => {
            let Value::Vector(v) = value else {
                return Err(format!("player field {name} takes a vector"));
            };
            let mut frame = FrameWorld::from_world(world);
            if name == "origin" {
                frame.set_origin(id, *v);
            } else {
                frame.set_viewangles(id, *v);
            }
        }
        "health" | "maxhealth" => {
            let n = int(value)?;
            let mut frame = FrameWorld::from_world(world);
            if let Some(ps) = frame.player_mut(id) {
                if name == "health" {
                    ps.health = n;
                } else {
                    ps.max_health = n;
                }
            }
        }
        "score" | "kills" | "deaths" => {
            let n = int(value)?;
            let mut frame = FrameWorld::from_world(world);
            if frame.client_meta(id).is_some() {
                let meta = frame.client_meta_mut(id);
                match name {
                    "score" => meta.score = n,
                    "kills" => meta.kills = n,
                    _ => meta.deaths = n,
                }
            }
        }
        "sessionteam" => {
            let Value::String(team) = value else {
                return Err("sessionteam takes a string".into());
            };
            let team = match &**team {
                "axis" => entity_iw4::TEAM_AXIS,
                "allies" => entity_iw4::TEAM_ALLIES,
                "spectator" => entity_iw4::TEAM_SPECTATOR,
                "none" => entity_iw4::TEAM_FREE,
                other => return Err(format!("invalid sessionteam '{other}'")),
            };
            let mut frame = FrameWorld::from_world(world);
            if frame.client_meta(id).is_some() {
                let meta = frame.client_meta_mut(id);
                meta.client_state_team = team;
                if matches!(
                    meta.lifecycle,
                    crate::ClientLifecycle::Spectating | crate::ClientLifecycle::ChoosingClass
                ) {
                    meta.lifecycle = crate::script_player::spectator_lifecycle(team);
                }
            }
        }
        "name" => return Err("player field name is read-only".into()),
        _ => return Ok(false),
    }
    Ok(true)
}
