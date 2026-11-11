use playerstate_iw4::buttons;

use crate::script::Namespace::{Function, Method};
use crate::script::host::args::{byte_string, float, optional, string};
use crate::script::host::natives::player::{link_to, player};
use crate::script::host::players::LinkView;
use crate::script::{Arc, NativeRegistry, RoundScript, Value};

pub(super) fn command_buttons(command: &str) -> u32 {
    match command {
        "+speed" | "toggleads" => buttons::ADS,
        "+reload" => buttons::RELOAD,
        "+sprint" => buttons::SPRINT,
        "+holdbreath" => buttons::BREATH,
        "+breath_sprint" => buttons::BREATH | buttons::SPRINT,
        "+melee_breath" => buttons::MELEE_CHARGE | buttons::BREATH,
        "+moveup" => buttons::JUMP,
        "+movedown" | "gocrouch" | "togglecrouch" => buttons::CROUCH,
        "+prone" | "goprone" | "toggleprone" | "lowerstance" => buttons::PRONE,
        "+stance" => buttons::CROUCH | buttons::PRONE | buttons::STANCE_HELD,
        "+throw" => buttons::THROW,
        _ => 0,
    }
}

pub(crate) fn blend_link(
    world: &mut bevy_ecs::prelude::World,
    client: u32,
    origin: [f32; 3],
    view: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    let now = crate::script::host::players::now_ms(world);
    let blends = &mut world.resource_mut::<RoundScript>().campaign.link_blends;
    let Some(&(from, from_view, start, ms)) = blends.get(&client) else {
        return (origin, view);
    };
    if now >= start + ms {
        blends.remove(&client);
        return (origin, view);
    }
    let t = ((now - start) as f32 / ms as f32).clamp(0.0, 1.0);
    let t = t * t * (3.0 - 2.0 * t);
    let turn = |a: f32, b: f32| ((b - a + 540.0) % 360.0) - 180.0;
    (
        std::array::from_fn(|k| from[k] + (origin[k] - from[k]) * t),
        std::array::from_fn(|k| from_view[k] + turn(from_view[k], view[k]) * t),
    )
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "playerlinktoblend", |world, receiver, args| {
        let client = player(world, receiver)?;
        let ms = optional(args, 2, float)?.map_or(0, |s| (s.max(0.0) * 1000.0) as i64);
        let start = crate::frame::FrameWorld::from_world(world)
            .player(crate::world::ClientId(client))
            .map(|ps| (ps.origin, ps.viewangles));
        let result = link_to(
            world,
            receiver,
            &args[..args.len().min(2)],
            LinkView::Absolute,
        )?;
        let now = crate::script::host::players::now_ms(world);
        let blends = &mut world.resource_mut::<RoundScript>().campaign.link_blends;
        match start {
            Some((origin, view)) if ms > 0 => {
                blends.insert(client, (origin, view, now, ms));
            }
            _ => {
                blends.remove(&client);
            }
        }
        Ok(result)
    });
    registry.register(Method, "linktoblendtotag", |world, receiver, args| {
        use crate::script::host::natives::engine::entity_id;
        let id = entity_id(world, receiver)?;
        let parent = entity_id(world, crate::script::host::args::arg(args, 0)?)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        if runtime.player_client(id).is_some() {
            return Err("linktoblendtotag does not take a player".into());
        }
        if parent == id {
            return Err("cannot link an entity to itself".into());
        }
        let tag = optional(args, 1, string)?
            .filter(|tag| !tag.is_empty() && !tag.eq_ignore_ascii_case("tag_origin"))
            .map(Arc::<str>::from);
        let tag_offset = tag.is_none().then_some([0.0; 3]);
        if let Some(entity) = runtime.entities.get_mut(&id) {
            entity.linked_to = Some(crate::script::host::entities::Link {
                parent,
                origin: [0.0; 3],
                angles: [0.0; 3],
                tag,
                tag_offset,
            });
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "lerpviewangleclamp", |world, receiver, args| {
        let client = player(world, receiver)?;
        let arc = |index| -> Result<f32, String> { Ok(float(args, index)?.clamp(0.0, 180.0)) };
        let clamp = [-arc(5)?, arc(6)?, -arc(3)?, arc(4)?];
        if let Some(link) = world
            .resource_mut::<RoundScript>()
            .players
            .get_mut(&client)
            .and_then(|slot| slot.link.as_mut())
        {
            link.clamp = Some(clamp);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getnormalizedmovement", |world, receiver, _| {
        let client = player(world, receiver)?;
        let input = &world.resource::<crate::step::StepRequest>().input;
        let (forward, right) = input
            .cmds
            .iter()
            .rev()
            .find(|command| command.client.0 == client)
            .map_or((0.0, 0.0), |command| {
                (
                    f32::from(command.command.forwardmove) / 127.0,
                    f32::from(command.command.rightmove) / 127.0,
                )
            });
        Ok(Value::Vector([forward, right, 0.0]))
    });
    registry.register(Function, "notifyoncommand", |world, _, args| {
        let notify: Arc<str> = byte_string(args, 0)?.symbol_key();
        let command: Arc<str> = string(args, 1)?.to_ascii_lowercase().into();
        let mut runtime = world.resource_mut::<RoundScript>();
        for slot in runtime.players.values_mut() {
            if !slot
                .commands
                .iter()
                .any(|(c, n)| *c == command && *n == notify)
            {
                slot.commands.push((command.clone(), notify.clone()));
            }
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "getkeybinding", |world, _, args| {
        let cmd = string(args, 0)?;
        let key = key_for_command(&cmd);
        let vals = match key {
            Some(k) => vec![Value::string(k)],
            None => Vec::new(),
        };
        let array = super::super::arrays::new_array(world, vals)?;
        if let Value::Array(id) = array {
            let count = if key.is_some() { 1 } else { 0 };
            world
                .resource_mut::<RoundScript>()
                .arrays
                .get_mut(&id)
                .unwrap()
                .insert(
                    crate::script::ArrayKey::String("count".into()),
                    Value::Int(count),
                );
        }
        Ok(array)
    });
    registry.register(Function, "getcommandfromkey", |_, _, args| {
        let key = string(args, 0)?;
        let cmd = command_for_key(&key).unwrap_or("");
        Ok(Value::string(cmd))
    });
    registry.register(Method, "setfriendlychain", |world, receiver, _args| {
        let _ = super::super::natives::engine::entity_id(world, receiver)?;
        diag::warn!(Sim, "Friendly chains are disabled");
        Ok(Value::Undefined)
    });
    registry.register(Method, "enableaimassist", |world, receiver, _| {
        let _ = super::super::natives::player::player(world, receiver)?;
        super::super::natives::iw4::set_dvar(world, "aim_assist_enabled", "1");
        Ok(Value::Undefined)
    });
    registry.register(Method, "disableaimassist", |world, receiver, _| {
        let _ = super::super::natives::player::player(world, receiver)?;
        super::super::natives::iw4::set_dvar(world, "aim_assist_enabled", "0");
        Ok(Value::Undefined)
    });
    registry.register(Method, "setwatersheeting", |world, receiver, args| {
        let _ = super::super::natives::player::player(world, receiver)?;
        let enabled = super::super::args::int(args, 0)? != 0;
        let duration =
            super::super::args::optional(args, 1, super::super::args::float)?.unwrap_or(3.0);
        super::super::natives::iw4::set_dvar(
            world,
            "r_watersheetingfx_enable",
            if enabled { "1" } else { "0" },
        );
        super::super::natives::iw4::set_dvar(
            world,
            "r_watersheetingfx_fadeduration",
            &duration.to_string(),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "painvisionon", |world, receiver, _| {
        let _ = super::super::natives::player::player(world, receiver)?;
        let id = super::super::natives::engine::entity_id(world, receiver)?;
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "player_has_red_flashing_overlay",
            Value::Int(1),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "painvisionoff", |world, receiver, _| {
        let _ = super::super::natives::player::player(world, receiver)?;
        let id = super::super::natives::engine::entity_id(world, receiver)?;
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "player_has_red_flashing_overlay",
            Value::Int(0),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "setlookatentity", |world, receiver, args| {
        let id = super::super::natives::engine::entity_id(world, receiver)?;
        let target = args.get(0).cloned().unwrap_or(Value::Undefined);
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "lookat_entity", target);
        Ok(Value::Undefined)
    });
}

fn key_for_command(cmd: &str) -> Option<&'static str> {
    match cmd.to_ascii_lowercase().as_str() {
        "+activate" | "+usereload" | "+reload" => Some("BUTTON_X"),
        "+speed" | "toggleads" => Some("BUTTON_LSHLDR"),
        "+attack" => Some("BUTTON_RSHLDR"),
        "+frag" => Some("BUTTON_RTRIG"),
        "+smoke" => Some("BUTTON_LTRIG"),
        "+melee" | "+melee_breath" => Some("BUTTON_RSTICK"),
        "+sprint" | "+breath_sprint" | "+holdbreath" => Some("BUTTON_LSTICK"),
        "+gostand" | "+moveup" => Some("BUTTON_A"),
        "+stance" | "+movedown" | "togglecrouch" | "gocrouch" => Some("BUTTON_B"),
        "weapnext" => Some("BUTTON_Y"),
        "pause" => Some("BUTTON_START"),
        _ => None,
    }
}

fn command_for_key(key: &str) -> Option<&'static str> {
    match key.to_ascii_uppercase().as_str() {
        "BUTTON_START" => Some("pause"),
        "BUTTON_X" | "F" | "R" => Some("+usereload"),
        "BUTTON_LSHLDR" | "MOUSE2" => Some("+speed"),
        "BUTTON_RSHLDR" | "MOUSE1" => Some("+attack"),
        "BUTTON_RTRIG" | "G" => Some("+frag"),
        "BUTTON_LTRIG" | "4" => Some("+smoke"),
        "BUTTON_RSTICK" | "V" => Some("+melee_breath"),
        "BUTTON_LSTICK" | "SHIFT" => Some("+breath_sprint"),
        "BUTTON_A" | "SPACE" => Some("+gostand"),
        "BUTTON_B" | "C" => Some("+stance"),
        "BUTTON_Y" | "1" => Some("weapnext"),
        _ => None,
    }
}
