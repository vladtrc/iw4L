use super::iw4_natives::string;
use super::natives_math::{arg, float, int, optional, vector};
use super::*;
use crate::frame::FrameWorld;
use crate::script_player;
use crate::world::ClientId;
use bevy_ecs::prelude::World;
use natives::Namespace::Method;

pub(super) fn player(world: &World, receiver: &Value) -> Result<u32, String> {
    match receiver {
        Value::Object(id) => world
            .resource::<Runtime>()
            .player_client(*id)
            .ok_or_else(|| "receiver is not a player".into()),
        _ => Err("receiver is not a player".into()),
    }
}

fn slot<'w>(world: &'w mut World, client: u32) -> Result<&'w mut players::PlayerSlot, String> {
    world
        .resource_mut::<Runtime>()
        .into_inner()
        .players
        .get_mut(&client)
        .ok_or_else(|| "player has disconnected".into())
}

pub(super) fn present(
    world: &mut World,
    receiver: &Value,
    key: &'static str,
    args: &[Value],
) -> Result<Value, String> {
    let client = player(world, receiver)?;
    slot(world, client)?.presented.insert(key, args.to_vec());
    Ok(Value::Undefined)
}

fn text(value: &Value) -> Result<Arc<str>, String> {
    match value {
        Value::String(s) | Value::LocalizedString(s) => Ok(s.clone()),
        Value::Int(n) => Ok(n.to_string().into()),
        Value::Float(f) => Ok(runtime::to_text(&Value::Float(*f))
            .unwrap_or_default()
            .into()),
        other => Err(format!("{} is not a string", natives_math::kind(other))),
    }
}

fn client_dvar_value(value: &Value, params: &[Value]) -> Result<String, String> {
    let mut out = match value {
        Value::LocalizedString(key) => format!("@{key}"),
        other => text(other)?.to_string(),
    };
    for param in params {
        out.push(crate::HUD_PRINT_ARG_SEPARATOR);
        out.push_str(&text(param)?);
    }
    Ok(out)
}

fn publish_client_dvar(world: &mut World, client: u32, name: &str, value: String) {
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(ClientId(client)).is_none() {
        return;
    }
    let dvars = &mut frame.client_meta_mut(ClientId(client)).client_dvars;
    match dvars.iter_mut().find(|(key, _)| key == name) {
        Some(row) => row.1 = value,
        None => dvars.push((name.to_owned(), value)),
    }
}

fn send_menu_command(world: &mut World, client: u32, kind: crate::MenuCommandKind) {
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(ClientId(client)).is_none() {
        return;
    }
    frame
        .client_meta_mut(ClientId(client))
        .push_menu_command(kind);
}

fn data_path(keys: &[Value]) -> Result<String, String> {
    let mut path = String::new();
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            path.push('.');
        }
        match key {
            Value::String(s) => path.push_str(&s.to_ascii_lowercase()),
            Value::Int(n) => path.push_str(&n.to_string()),
            other => {
                return Err(format!(
                    "player data key must be a string or int, not {}",
                    natives_math::kind(other)
                ));
            }
        }
    }
    if path.is_empty() {
        return Err("player data needs a key".into());
    }
    Ok(path)
}

const DEFAULT_KILLSTREAKS: [&str; 3] = ["uav", "airdrop", "predator_missile"];

fn unset_player_data(path: &str) -> Value {
    // A missing field is that field's zero, not undefined.
    if let Some(slot) = path.strip_prefix("killstreaks.") {
        let streak = slot
            .parse::<usize>()
            .ok()
            .and_then(|i| DEFAULT_KILLSTREAKS.get(i));
        return Value::string(streak.copied().unwrap_or("none"));
    }
    let field = path
        .rsplit('.')
        .find(|key| !key.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or("");
    match field {
        "weapon" | "attachment" | "camo" | "perks" | "specialgrenade" | "killstreaks" => {
            Value::string("none")
        }
        "cardtitle" | "cardicon" | "cardnameplate" | "name" => Value::string(""),
        _ => Value::Int(0),
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "ishost", |world, receiver, _| {
        let client = player(world, receiver)?;
        Ok(Value::Int((client == 0).into()))
    });
    registry.register(Method, "getguid", |world, receiver, _| {
        let client = player(world, receiver)?;
        Ok(Value::String(
            format!("{:016x}", u64::from(client) + 1).into(),
        ))
    });
    registry.register(Method, "getxuid", |world, receiver, _| {
        let client = player(world, receiver)?;
        Ok(Value::String(format!("{:x}", u64::from(client) + 1).into()))
    });
    registry.register(Method, "isusingonlinedataoffline", |world, receiver, _| {
        player(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "getrestedtime", |world, receiver, _| {
        player(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "setclientdvar", |world, receiver, args| {
        let client = player(world, receiver)?;
        let name: Arc<str> = string(args, 0)?.to_ascii_lowercase().into();
        let value = text(arg(args, 1)?)?;
        let sent = client_dvar_value(arg(args, 1)?, &args[2..])?;
        slot(world, client)?.dvars.insert(name.clone(), value);
        publish_client_dvar(world, client, &name, sent);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setclientdvars", |world, receiver, args| {
        let client = player(world, receiver)?;
        if args.len() % 2 != 0 {
            return Err("setclientdvars takes name/value pairs".into());
        }
        let mut pairs: Vec<(Arc<str>, Arc<str>)> = Vec::new();
        let mut sent = Vec::new();
        for i in (0..args.len()).step_by(2) {
            let name: Arc<str> = string(args, i)?.to_ascii_lowercase().into();
            sent.push((name.clone(), client_dvar_value(&args[i + 1], &[])?));
            pairs.push((name, text(&args[i + 1])?));
        }
        slot(world, client)?.dvars.extend(pairs);
        for (name, value) in sent {
            publish_client_dvar(world, client, &name, value);
        }
        Ok(Value::Undefined)
    });
    for name in ["openmenu", "openpopupmenu", "openmenunomouse"] {
        registry.register(Method, name, |world, receiver, args| {
            let client = player(world, receiver)?;
            let menu: Arc<str> = string(args, 0)?.to_ascii_lowercase().into();
            if let Some(cs_index) = hud_iw4::script_menu_cs_index(&menu) {
                FrameWorld::from_world(world).push_player_card_open(ClientId(client), cs_index);
            } else {
                send_menu_command(
                    world,
                    client,
                    crate::MenuCommandKind::Open(menu.to_string()),
                );
            }
            slot(world, client)?.menu = Some(menu);
            Ok(Value::Int(1))
        });
    }
    registry.register(Method, "setcarddisplayslot", |world, receiver, args| {
        let client = player(world, receiver)?;
        let source = player(world, arg(args, 0)?)?;
        let card = int(args, 1)?;
        if !(0..hud_iw4::PLAYER_CARD_SCRIPT_SLOT_COUNT as i32).contains(&card) {
            return Err(format!("card display slot {card} is out of range"));
        }
        FrameWorld::from_world(world).push_player_card_slot(
            ClientId(client),
            ClientId(source),
            card,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "showhudsplash", |world, receiver, args| {
        let client = player(world, receiver)?;
        let splash = string(args, 0)?;
        let splash_slot = int(args, 1)?;
        if !(0..5).contains(&splash_slot) {
            return Err(format!("hud splash slot {splash_slot} is out of range"));
        }
        let optional_number = optional(args, 2, int)?.unwrap_or(0);
        FrameWorld::from_world(world).push_hud_splash(
            ClientId(client),
            splash,
            splash_slot,
            optional_number,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "closepopupmenu", |world, receiver, _| {
        let client = player(world, receiver)?;
        send_menu_command(world, client, crate::MenuCommandKind::ClosePopup);
        slot(world, client)?.menu = None;
        Ok(Value::Undefined)
    });
    for name in ["closemenu", "closeingamemenu"] {
        registry.register(Method, name, |world, receiver, _| {
            let client = player(world, receiver)?;
            send_menu_command(world, client, crate::MenuCommandKind::CloseInGame);
            slot(world, client)?.menu = None;
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "getplayerdata", |world, receiver, args| {
        let client = player(world, receiver)?;
        let path = data_path(args)?;
        let stored = slot(world, client)?.data.get(&path).cloned();
        Ok(stored.unwrap_or_else(|| unset_player_data(&path)))
    });
    registry.register(Method, "setplayerdata", |world, receiver, args| {
        let client = player(world, receiver)?;
        let (value, keys) = args
            .split_last()
            .ok_or("setplayerdata needs a key and a value")?;
        let path = data_path(keys)?;
        slot(world, client)?.data.insert(path, value.clone());
        Ok(Value::Undefined)
    });
    registry.register(Method, "notifyonplayercommand", |world, receiver, args| {
        let client = player(world, receiver)?;
        let notify: Arc<str> = string(args, 0)?.into();
        let command: Arc<str> = string(args, 1)?.to_ascii_lowercase().into();
        let slot = slot(world, client)?;
        if !slot
            .commands
            .iter()
            .any(|(c, n)| *c == command && *n == notify)
        {
            slot.commands.push((command, notify));
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setrank", |world, receiver, args| {
        let client = player(world, receiver)?;
        let rank = int(args, 0)?;
        let prestige = optional(args, 1, int)?.unwrap_or(0);
        let mut frame = FrameWorld::from_world(world);
        if frame.client_meta(ClientId(client)).is_some() {
            let meta = frame.client_meta_mut(ClientId(client));
            meta.rank = rank;
            meta.prestige = prestige;
        }
        Ok(Value::Undefined)
    });
    for name in ["updatescores", "updatedmscores"] {
        registry.register(Method, name, |world, receiver, _| {
            player(world, receiver)?;
            Ok(Value::Undefined)
        });
    }

    register_body(registry);
    register_inventory(registry);
    register_death(registry);

    macro_rules! presented {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                present(world, receiver, $name, args)
            });
        )*};
    }
    registry.register(Method, "playlocalsound", |world, receiver, args| {
        let client = player(world, receiver)?;
        let alias = string(args, 0)?;
        super::natives_engine::sound_to(
            world,
            crate::EventAudience::Client(ClientId(client)),
            &alias,
            entity_iw4::LOCAL_SOUND_ENTITY,
            [0.0; 3],
        );
        Ok(Value::Undefined)
    });
    presented!(
        "stoplocalsound",
        "setcardtitle",
        "setcardicon",
        "setcardnameplate",
        "setblurforplayer",
        "setdepthoffield",
        "visionsetnakedforplayer",
        "visionsetthermalforplayer",
        "visionsetmissilecamforplayer",
        "thermalvisionon",
        "thermalvisionoff",
        "thermalvisionfofoverlayon",
        "thermalvisionfofoverlayoff",
        "stoprumble",
        "setempjammed",
        "remotecamerasoundscapeoff",
        "setrearviewrenderenabled",
        "setweaponhudiconoverride",
        "pingplayer",
        "sayall",
        "sayteam",
        "setspectatedefaults",
        "kc_regweaponforfxremoval",
        "setviewmodel",
        "viewkick",
        "stunplayer",
        "playerhide",
        "startac130",
        "stopac130",
        "forceusehinton",
        "forceusehintoff",
        "predictstreampos",
        "player_recoilscaleon",
        "player_recoilscaleoff",
        "attachshieldmodel",
        "detachshieldmodel",
        "moveshieldmodel",
        "playerforcedeathanim",
        "beginlocationselection",
        "endlocationselection",
        "weaponlockstart",
        "weaponlockfinalize",
        "weaponlockfree",
        "weaponlocknoclearance",
        "weaponlocktargettooclose",
        "setspreadoverride",
        "resetspreadoverride",
        "clientclaimtrigger",
        "clientreleasetrigger",
    );
    macro_rules! answers {
        ($value:expr => $($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                player(world, receiver)?;
                Ok($value)
            });
        )*};
    }
    answers!(Value::Int(1) => "isitemunlocked");
    answers!(Value::Int(0) => "canplayerplacesentry", "isusingturret", "isfiringturret",
        "worldpointinreticle_circle");
    answers!(Value::Undefined => "getspectatingplayer");
    answers!(Value::Vector([0.0; 3]) => "getthirdpersoncrosshairoffset");
}

const PLAYER_ANIM_TREE: &str = "multiplayer";

fn tick(world: &World) -> crate::Tick {
    world.resource::<crate::step::StepRequest>().tick
}

fn maybe_player(world: &World, args: &[Value], index: usize) -> Option<ClientId> {
    match args.get(index) {
        Some(Value::Object(id)) => world.resource::<Runtime>().player_client(*id).map(ClientId),
        _ => None,
    }
}

fn entity_kind(world: &World, receiver: &Value) -> Option<(u64, entities::EntityKind)> {
    let Value::Object(id) = receiver else {
        return None;
    };
    world
        .resource::<Runtime>()
        .entities
        .get(id)
        .map(|e| (*id, e.kind.clone()))
}

pub(super) fn corpse_anim(world: &World, receiver: &Value) -> Result<Option<Arc<str>>, String> {
    match entity_kind(world, receiver) {
        Some((_, entities::EntityKind::Corpse { anim, .. })) => Ok(anim),
        _ => Err("receiver is not a corpse".into()),
    }
}

fn item_number(world: &World, receiver: &Value) -> Result<i32, String> {
    match entity_kind(world, receiver) {
        Some((_, entities::EntityKind::Item(number))) => Ok(number),
        _ => Err("receiver is not a weapon item".into()),
    }
}

fn anim_clip(
    world: &mut World,
    args: &[Value],
    index: usize,
) -> Result<Arc<xmodel_runtime::AnimClip>, String> {
    let name = match arg(args, index)? {
        Value::Animation { name, .. } => name.clone(),
        other => return Err(format!("{} is not an animation", natives_math::kind(other))),
    };
    FrameWorld::from_world(world)
        .player_anim_clip_named(&name)
        .ok_or_else(|| format!("animation '{name}' is not loaded in the simulation"))
}

pub(super) const SCAVENGER_ITEM_CLASS: &str = "scavenger_item";

pub(super) fn new_item_entity(
    world: &mut World,
    number: i32,
    classname: &str,
) -> Result<Value, String> {
    let origin = FrameWorld::from_world(world)
        .dropped_item_by_number(number)
        .map_or([0.0; 3], |i| i.origin);
    let mut runtime = world.resource_mut::<Runtime>();
    let id = runtime.create_entity(entities::EntityKind::Item(number), classname)?;
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    Ok(Value::Object(id))
}

fn register_death(registry: &mut NativeRegistry) {
    registry.register(Method, "finishplayerdamage", |world, receiver, args| {
        let client = player(world, receiver)?;
        let id = ClientId(client);
        let attacker = maybe_player(world, args, 1);
        let amount = int(args, 2)?;
        let dir = match args.get(7) {
            Some(Value::Vector(v)) => Some(*v),
            _ => None,
        };
        let commit = world
            .resource::<Runtime>()
            .current_hit
            .as_ref()
            .filter(|hit| hit.victim == id)
            .and_then(|hit| hit.commit);
        let tick = tick(world);
        let owed = || -> Vec<Value> {
            let at = |i: usize| args.get(i).cloned().unwrap_or(Value::Undefined);
            vec![
                at(0),
                at(1),
                at(2),
                at(4),
                at(5),
                at(7),
                at(8),
                at(9),
                Value::Int(0),
            ]
        };
        let finish = {
            let mut frame = FrameWorld::from_world(world);
            let finish = script_player::finish_damage(&mut frame, id, amount, dir);
            if finish == script_player::Finish::Killed {
                script_player::kill(&mut frame, tick, id, attacker, commit);
            }
            finish
        };
        match finish {
            script_player::Finish::Hurt => {}
            script_player::Finish::LastStand => {
                players::owe(world, client, players::LAST_STAND, owed())
            }
            script_player::Finish::Killed => players::owe(world, client, players::KILLED, owed()),
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "suicide", |world, receiver, _| {
        let client = player(world, receiver)?;
        let tick = tick(world);
        players::suicide(world, tick, client);
        Ok(Value::Undefined)
    });
    registry.register(Method, "laststandrevive", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        script_player::revive(&mut FrameWorld::from_world(world), id);
        Ok(Value::Undefined)
    });
    registry.register(
        natives::Namespace::Function,
        "obituary",
        |world, _, args| {
            let Some(victim) = maybe_player(world, args, 0) else {
                return Err("obituary victim is not a player".into());
            };
            let attacker = maybe_player(world, args, 1);
            let weapon = weapon_arg(world, args, 2)?;
            let means = string(args, 3)?;
            let tick = tick(world);
            script_player::obituary(
                &mut FrameWorld::from_world(world),
                tick,
                victim,
                attacker,
                weapon,
                &means,
            );
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "cloneplayer", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let tick = tick(world);
        let mut frame = FrameWorld::from_world(world);
        let Some(ps) = frame.player(id).copied() else {
            return Ok(Value::Undefined);
        };
        let anim = frame
            .player_anim_clip(ps.legs_anim)
            .map(|clip| Arc::<str>::from(clip.name.as_str()));
        let Some(slot) = script_player::clone_corpse(&mut frame, tick, id) else {
            return Ok(Value::Undefined);
        };
        let mut runtime = world.resource_mut::<Runtime>();
        let body =
            runtime.create_entity(entities::EntityKind::Corpse { slot, anim }, "player_corpse")?;
        runtime.set_object_field(body, "origin", Value::Vector(ps.origin));
        runtime.set_object_field(body, "angles", Value::Vector([0.0, ps.viewangles[1], 0.0]));
        Ok(Value::Object(body))
    });
    registry.register(Method, "getcorpseanim", |world, receiver, _| {
        let name = corpse_anim(world, receiver)?.ok_or("the corpse has no death animation")?;
        Ok(Value::Animation {
            tree: PLAYER_ANIM_TREE.into(),
            name,
        })
    });
    registry.register(Method, "isragdoll", |world, receiver, _| {
        corpse_anim(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "startragdoll", |world, receiver, _| {
        corpse_anim(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(
        natives::Namespace::Function,
        "getanimlength",
        |world, _, args| Ok(Value::Float(anim_clip(world, args, 0)?.duration())),
    );
    registry.register(
        natives::Namespace::Function,
        "animhasnotetrack",
        |world, _, args| {
            let clip = anim_clip(world, args, 0)?;
            let note = string(args, 1)?;
            Ok(Value::Int(
                clip.notifies
                    .iter()
                    .any(|n| n.name.eq_ignore_ascii_case(&note))
                    .into(),
            ))
        },
    );
    registry.register(
        natives::Namespace::Function,
        "getnotetracktimes",
        |world, _, args| {
            let clip = anim_clip(world, args, 0)?;
            let note = string(args, 1)?;
            let times: Vec<Value> = clip
                .notifies
                .iter()
                .filter(|n| n.name.eq_ignore_ascii_case(&note))
                .map(|n| Value::Float(n.time))
                .collect();
            natives_math::new_array(world, times)
        },
    );
    registry.register(Method, "dropitem", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        let tick = tick(world);
        match crate::item::drop_weapon(&mut FrameWorld::from_world(world), tick, id, weapon) {
            Some(number) => new_item_entity(world, number, "weapon_item"),
            None => Ok(Value::Undefined),
        }
    });
    for name in ["dropscavengerbag", "dropscavengeritem"] {
        registry.register(Method, name, |world, receiver, args| {
            let id = client_of(world, receiver)?;
            let weapon = weapon_arg(world, args, 0)?;
            let tick = tick(world);
            let mut frame = FrameWorld::from_world(world);
            match crate::item::drop_scavenger_item(&mut frame, tick, id, weapon) {
                Some(number) => new_item_entity(world, number, SCAVENGER_ITEM_CLASS),
                None => Ok(Value::Undefined),
            }
        });
    }
    registry.register(Method, "itemweaponsetammo", |world, receiver, args| {
        let number = item_number(world, receiver)?;
        let clip = int(args, 0)?;
        let stock = int(args, 1)?;
        let clip_l = optional(args, 2, int)?;
        let mut frame = FrameWorld::from_world(world);
        if let Some(item) = frame.dropped_item_mut_by_number(number) {
            item.clip_r = clip.max(0);
            item.stock = stock.max(0);
            if let Some(clip_l) = clip_l {
                item.clip_l = clip_l.max(0);
            }
        }
        Ok(Value::Undefined)
    });
}

const OFFHAND_CLASSES: [&str; 6] = ["none", "frag", "smoke", "flash", "throwingknife", "other"];

fn offhand_class(name: &str) -> i32 {
    OFFHAND_CLASSES
        .iter()
        .skip(1)
        .position(|class| *class == name)
        .map_or(0, |i| i as i32 + 1)
}

fn offhand_class_of(world: &mut World, name: &str) -> i32 {
    match offhand_class(name) {
        0 => {
            let frame = FrameWorld::from_world(world);
            frame
                .weapon_index_by_script_name(name)
                .and_then(|weapon| frame.offhand_loadout_row(weapon))
                .map_or(0, |facts| facts.offhand_class)
        }
        class => class,
    }
}

fn client_of(world: &World, receiver: &Value) -> Result<ClientId, String> {
    player(world, receiver).map(ClientId)
}

fn flag(args: &[Value], index: usize) -> Result<bool, String> {
    Ok(optional(args, index, int)?.unwrap_or(1) != 0)
}

fn weapon_arg(world: &mut World, args: &[Value], index: usize) -> Result<u32, String> {
    let name = string(args, index)?;
    script_player::weapon_named(&FrameWorld::from_world(world), &name)
}

fn weapon_value(world: &mut World, weapon: u32) -> Value {
    Value::String(script_player::weapon_name(&FrameWorld::from_world(world), weapon).into())
}

fn controls(
    world: &mut World,
    receiver: &Value,
    set: impl FnOnce(&mut crate::match_state::ScriptControls),
) -> Result<Value, String> {
    let id = client_of(world, receiver)?;
    let mut frame = FrameWorld::from_world(world);
    set(&mut frame.client_meta_mut(id).controls);
    Ok(Value::Undefined)
}

fn register_body(registry: &mut NativeRegistry) {
    registry.register(Method, "spawn", |world, receiver, args| {
        let client = player(world, receiver)?;
        let origin = vector(args, 0)?;
        let angles = vector(args, 1)?;
        let state = slot(world, client)?.sessionstate.clone();
        let tick = world.resource::<crate::step::StepRequest>().tick;
        script_player::spawn(
            &mut FrameWorld::from_world(world),
            tick,
            ClientId(client),
            origin,
            angles,
            &state,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "freezecontrols", |world, receiver, args| {
        let on = flag(args, 0)?;
        controls(world, receiver, |c| c.frozen = on)
    });
    registry.register(Method, "allowjump", |world, receiver, args| {
        let on = flag(args, 0)?;
        controls(world, receiver, |c| c.jump_disabled = !on)
    });
    macro_rules! control {
        ($($name:literal => $field:ident = $value:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                controls(world, receiver, |c| c.$field = $value)
            });
        )*};
    }
    control!(
        "disableweapons" => weapons_disabled = true,
        "enableweapons" => weapons_disabled = false,
        "disableoffhandweapons" => offhands_disabled = true,
        "enableoffhandweapons" => offhands_disabled = false,
        "disableweaponswitch" => switch_disabled = true,
        "enableweaponswitch" => switch_disabled = false,
        "disableusability" => usability_disabled = true,
        "enableusability" => usability_disabled = false,
    );
    registry.register(Method, "allowspectateteam", |world, receiver, args| {
        let client = player(world, receiver)?;
        let team: Arc<str> = string(args, 0)?.into();
        let on = flag(args, 1)?;
        slot(world, client)?.spectate.insert(team, on);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setmovespeedscale", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let scale = float(args, 0)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.move_speed_scale_multiplier = scale;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setnormalhealth", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let fraction = float(args, 0)?.clamp(0.0, 1.0);
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.health = ((ps.max_health as f32 * fraction) as i32).max(1);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getplayerangles", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        Ok(Value::Vector(
            FrameWorld::from_world(world)
                .player(id)
                .map_or([0.0; 3], |ps| ps.viewangles),
        ))
    });
    registry.register(Method, "setvelocity", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let velocity = vector(args, 0)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.velocity = velocity;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setplayerangles", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let angles = vector(args, 0)?;
        FrameWorld::from_world(world).set_viewangles(id, angles);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getstance", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        Ok(Value::string(script_player::stance(
            &FrameWorld::from_world(world),
            id,
        )))
    });
    registry.register(Method, "setstance", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let stance = string(args, 0)?;
        let mut frame = FrameWorld::from_world(world);
        if let Some(ps) = frame.player_mut(id) {
            use playerstate_iw4::eflags::{DUCK, PRONE};
            ps.e_flags &= !(DUCK | PRONE);
            ps.e_flags |= match stance.as_str() {
                "crouch" => DUCK,
                "prone" => PRONE,
                _ => 0,
            };
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "isonground", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        Ok(Value::Int(
            frame
                .player(id)
                .is_some_and(|ps| ps.ground_entity_num != playerstate_iw4::ENTITYNUM_NONE)
                .into(),
        ))
    });
    registry.register(Method, "isonladder", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        Ok(Value::Int(
            frame
                .player(id)
                .is_some_and(|ps| ps.pm_flags & movement_iw4::PMF_LADDER != 0)
                .into(),
        ))
    });
    registry.register(Method, "ismantling", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        Ok(Value::Int(
            frame
                .player(id)
                .is_some_and(|ps| ps.mantle_flags & playerstate_iw4::mantle_flags::ACTIVE != 0)
                .into(),
        ))
    });
    registry.register(Method, "playerads", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        Ok(Value::Float(
            FrameWorld::from_world(world)
                .player(id)
                .map_or(0.0, |ps| ps.f_weapon_pos_frac),
        ))
    });
    macro_rules! button {
        ($($name:literal => $mask:expr),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                let id = client_of(world, receiver)?;
                let held = script_player::buttons(&mut FrameWorld::from_world(world), id);
                Ok(Value::Int((held & ($mask) != 0).into()))
            });
        )*};
    }
    {
        use playerstate_iw4::buttons::*;
        button!(
            "attackbuttonpressed" => ATTACK,
            "usebuttonpressed" => USE | USE_RELOAD,
            "meleebuttonpressed" => MELEE_CHARGE,
            "fragbuttonpressed" => FRAG,
            "secondaryoffhandbuttonpressed" => SMOKE,
            "adsbuttonpressed" => ADS,
            "jumpbuttonpressed" => JUMP,
        );
    }
    registry.register(Method, "buttonpressed", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let key = string(args, 0)?.to_ascii_lowercase();
        let mask = match key.as_str() {
            "+attack" | "mouse1" => playerstate_iw4::buttons::ATTACK,
            "+activate" | "+usereload" => playerstate_iw4::buttons::USE,
            "+melee" => playerstate_iw4::buttons::MELEE_CHARGE,
            "+frag" => playerstate_iw4::buttons::FRAG,
            "+smoke" => playerstate_iw4::buttons::SMOKE,
            "+speed_throw" | "+toggleads_throw" | "mouse2" => playerstate_iw4::buttons::ADS,
            "+gostand" | "space" => playerstate_iw4::buttons::JUMP,
            _ => 0,
        };
        let held = script_player::buttons(&mut FrameWorld::from_world(world), id);
        Ok(Value::Int((held & mask != 0).into()))
    });
    registry.register(Method, "playerlinkto", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        super::natives_engine::entity_id(world, arg(args, 0)?)?;
        let mut frame = FrameWorld::from_world(world);
        frame.client_meta_mut(id).controls.linked = true;
        if let Some(ps) = frame.player_mut(id) {
            ps.velocity = [0.0; 3];
        }
        Ok(Value::Undefined)
    });
    for name in ["playerlinktodelta", "playerlinktoabsolute"] {
        registry.register(Method, name, |world, receiver, args| {
            let id = client_of(world, receiver)?;
            super::natives_engine::entity_id(world, arg(args, 0)?)?;
            FrameWorld::from_world(world)
                .client_meta_mut(id)
                .controls
                .linked = true;
            Ok(Value::Undefined)
        });
    }
    for name in ["playerlinkedoffsetenable", "playerlinkedoffsetdisable"] {
        registry.register(Method, name, |world, receiver, _| {
            client_of(world, receiver)?;
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "setactionslot", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let index = int(args, 0)?;
        let slot = usize::try_from(index - 1)
            .ok()
            .filter(|slot| *slot < 4)
            .ok_or_else(|| format!("action slot {index} is out of range 1..4"))?;
        let kind = string(args, 1)?.to_ascii_lowercase();
        let (kind, param) = match kind.as_str() {
            "" | "none" => (0, 0),
            "weapon" => (1, weapon_arg(world, args, 2)? as i32),
            "altmode" => (2, 0),
            "nightvision" => (3, 0),
            other => return Err(format!("invalid action slot type '{other}'")),
        };
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.action_slot_type[slot] = kind;
            ps.action_slot_param[slot] = param;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "shellshock", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let name = string(args, 0)?.to_ascii_lowercase();
        let seconds = float(args, 1)?;
        if seconds < 0.0 {
            return Err(format!("shellshock duration {seconds} is negative"));
        }
        let index = *world
            .resource::<Runtime>()
            .precached
            .get(&("shellshock", name.clone()))
            .ok_or_else(|| format!("shellshock '{name}' was not precached"))?;
        let mut frame = FrameWorld::from_world(world);
        let shock = frame
            .shock(&name)
            .cloned()
            .ok_or_else(|| format!("no shock file for shellshock '{name}'"))?;
        let now = crate::level_time_ms(frame.ecs().resource::<crate::step::StepRequest>().tick);
        let Some(ps) = frame.player_mut(id) else {
            return Ok(Value::Undefined);
        };
        ps.shellshock_index = index;
        ps.shellshock_time = now;
        ps.shellshock_duration = (seconds * 1000.0) as i32;
        ps.pm_flags |= playerstate_iw4::pm_flags::SHELLSHOCKED;
        frame.client_meta_mut(id).shellshock = Some(shock);
        Ok(Value::Undefined)
    });
    registry.register(Method, "stopshellshock", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.shellshock_duration = 0;
            ps.pm_flags &= !playerstate_iw4::pm_flags::SHELLSHOCKED;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "radarjamon", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.e_flags |= playerstate_iw4::eflags::RADAR_JAM;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "radarjamoff", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.e_flags &= !playerstate_iw4::eflags::RADAR_JAM;
        }
        Ok(Value::Undefined)
    });
}

fn register_inventory(registry: &mut NativeRegistry) {
    registry.register(
        natives::Namespace::Function,
        "getweaponmodel",
        |world, _, args| {
            let weapon = weapon_arg(world, args, 0)?;
            let frame = FrameWorld::from_world(world);
            let model = frame
                .weapon_world_model(weapon)
                .map_or("", |(model, _)| model);
            Ok(Value::String(model.into()))
        },
    );
    registry.register(
        natives::Namespace::Function,
        "getweaponhidetags",
        |world, _, args| {
            let weapon = weapon_arg(world, args, 0)?;
            let tags: Vec<Value> = FrameWorld::from_world(world)
                .weapon_world_model(weapon)
                .map(|(_, tags)| {
                    tags.iter()
                        .map(|t| Value::String(t.as_str().into()))
                        .collect()
                })
                .unwrap_or_default();
            natives_math::new_array(world, tags)
        },
    );
    registry.register(Method, "giveweapon", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        let akimbo = optional(args, 2, int)?.unwrap_or(0) != 0;
        script_player::give_weapon(&mut FrameWorld::from_world(world), id, weapon, akimbo)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "takeweapon", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        script_player::take_weapon(&mut FrameWorld::from_world(world), id, weapon);
        Ok(Value::Undefined)
    });
    registry.register(Method, "takeallweapons", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        script_player::take_all_weapons(&mut FrameWorld::from_world(world), id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "hasweapon", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        Ok(Value::Int(
            script_player::has_weapon(&FrameWorld::from_world(world), id, weapon).into(),
        ))
    });
    registry.register(Method, "setspawnweapon", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        script_player::set_spawn_weapon(&mut FrameWorld::from_world(world), id, weapon)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "switchtoweapon", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        script_player::switch_to_weapon(&mut FrameWorld::from_world(world), id, weapon);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getcurrentweapon", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let weapon = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.weapon);
        Ok(weapon_value(world, weapon))
    });
    registry.register(Method, "getcurrentprimaryweapon", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let weapon = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.weapon_primary);
        Ok(weapon_value(world, weapon))
    });
    macro_rules! weapon_list {
        ($($name:literal => $list:ident),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                let id = client_of(world, receiver)?;
                let list = script_player::WeaponList::$list;
                let weapons = script_player::weapons(&FrameWorld::from_world(world), id, list);
                let names = weapons.into_iter().map(|w| weapon_value(world, w)).collect();
                natives_math::new_array(world, names)
            });
        )*};
    }
    weapon_list!(
        "getweaponslistall" => All,
        "getweaponslistprimaries" => Primaries,
        "getweaponslistoffhands" => Offhands,
        "getweaponslistitems" => Items,
        "getweaponslistexclusives" => Exclusives,
    );
    registry.register(Method, "getweaponammoclip", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        Ok(Value::Int(script_player::ammo_clip(
            &FrameWorld::from_world(world),
            id,
            weapon,
        )))
    });
    registry.register(Method, "getammocount", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        let frame = FrameWorld::from_world(world);
        Ok(Value::Int(
            script_player::ammo_clip(&frame, id, weapon)
                + script_player::ammo_stock(&frame, id, weapon),
        ))
    });
    registry.register(Method, "ischangingweapon", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let state = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.weaponstate_primary);
        Ok(Value::Int(super::weapons::is_changing_weapon(state).into()))
    });
    registry.register(Method, "getweaponammostock", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        Ok(Value::Int(script_player::ammo_stock(
            &FrameWorld::from_world(world),
            id,
            weapon,
        )))
    });
    registry.register(Method, "setweaponammoclip", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        let count = int(args, 1)?;
        script_player::set_ammo_clip(&mut FrameWorld::from_world(world), id, weapon, count);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setweaponammostock", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        let count = int(args, 1)?;
        script_player::set_ammo_stock(&mut FrameWorld::from_world(world), id, weapon, count);
        Ok(Value::Undefined)
    });
    registry.register(Method, "givemaxammo", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        script_player::give_max_ammo(&mut FrameWorld::from_world(world), id, weapon);
        Ok(Value::Undefined)
    });
    registry.register(Method, "givestartammo", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        script_player::give_start_ammo(&mut FrameWorld::from_world(world), id, weapon);
        Ok(Value::Undefined)
    });
    registry.register(Method, "anyammoforweaponmodes", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = weapon_arg(world, args, 0)?;
        let frame = FrameWorld::from_world(world);
        let any = script_player::ammo_clip(&frame, id, weapon) > 0
            || script_player::ammo_stock(&frame, id, weapon) > 0;
        Ok(Value::Int(any.into()))
    });
    registry.register(Method, "setperk", |world, receiver, args| {
        let client = player(world, receiver)?;
        let name: Arc<str> = string(args, 0)?.into();
        script_player::set_perk(
            &mut FrameWorld::from_world(world),
            ClientId(client),
            &name,
            true,
        );
        slot(world, client)?.perks.insert(name);
        Ok(Value::Undefined)
    });
    registry.register(Method, "unsetperk", |world, receiver, args| {
        let client = player(world, receiver)?;
        let name: Arc<str> = string(args, 0)?.into();
        script_player::set_perk(
            &mut FrameWorld::from_world(world),
            ClientId(client),
            &name,
            false,
        );
        slot(world, client)?.perks.remove(&name);
        Ok(Value::Undefined)
    });
    registry.register(Method, "hasperk", |world, receiver, args| {
        let client = player(world, receiver)?;
        let name = string(args, 0)?;
        Ok(Value::Int(
            slot(world, client)?.perks.contains(name.as_str()).into(),
        ))
    });
    registry.register(Method, "clearperks", |world, receiver, _| {
        let client = player(world, receiver)?;
        script_player::clear_perks(&mut FrameWorld::from_world(world), ClientId(client));
        slot(world, client)?.perks.clear();
        Ok(Value::Undefined)
    });
    macro_rules! offhand_class {
        ($($name:literal => $field:ident),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                let id = client_of(world, receiver)?;
                let class = offhand_class_of(world, &string(args, 0)?);
                if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
                    ps.$field = class;
                }
                let name = OFFHAND_CLASSES.get(class as usize).copied().unwrap_or("none");
                Ok(Value::string(name))
            });
        )*};
    }
    offhand_class!(
        "setoffhandprimaryclass" => offhand_primary,
        "setoffhandsecondaryclass" => offhand_secondary,
        "switchtooffhand" => offhand_primary,
    );
    registry.register(Method, "getoffhandsecondaryclass", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let class = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.offhand_secondary);
        Ok(Value::string(match class {
            2 => "smoke",
            3 => "flash",
            _ => "none",
        }))
    });
}
