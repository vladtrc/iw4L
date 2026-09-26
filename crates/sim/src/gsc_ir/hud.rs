use super::entities::{EntityKind, HudAudience};
use super::iw4_natives::string;
use super::natives_math::{arg, float, int, optional};
use super::*;
use crate::frame::FrameWorld;
use bevy_ecs::prelude::World;
use hud_iw4::{
    HE_TYPE_MATERIAL, HE_TYPE_PLAYERNAME, HE_TYPE_TEXT, HE_TYPE_VALUE, HE_TYPE_WAYPOINT, HudElem,
    align_org, align_screen, bg_lerp_hud_colors, color_rgba, flags, hud_elem_lerp_font_scale,
    hud_elem_movement_frac, hud_elem_scale_frac, unpack_rgba,
};
use playerstate_iw4::ENTITYNUM_NONE;

const HE_TYPE_TIMER_DOWN: i32 = 5;
const HE_TYPE_TIMER_UP: i32 = 6;
const HE_TYPE_TIMER_STATIC: i32 = 7;
const HE_TYPE_TENTHS_TIMER_DOWN: i32 = 8;
const HE_TYPE_TENTHS_TIMER_UP: i32 = 9;
const HE_TYPE_TENTHS_TIMER_STATIC: i32 = 10;
const HE_TYPE_CLOCK_DOWN: i32 = 11;
const HE_TYPE_CLOCK_UP: i32 = 12;

const FONTS: &[&str] = &[
    "default",
    "bigfixed",
    "smallfixed",
    "objective",
    "big",
    "small",
    "hudbig",
    "hudsmall",
    "extrabig",
];
const ALIGN_X: &[&str] = &["left", "center", "right"];
const ALIGN_Y: &[&str] = &["top", "middle", "bottom"];
const HORZ_ALIGN: &[&str] = &[
    "subleft",
    "left",
    "center",
    "right",
    "fullscreen",
    "noscale",
    "alignto640",
    "center_safearea",
    "left_adjustable",
    "center_adjustable",
    "right_adjustable",
];
const VERT_ALIGN: &[&str] = &[
    "subtop",
    "top",
    "middle",
    "bottom",
    "fullscreen",
    "noscale",
    "alignto480",
    "center_safearea",
    "top_adjustable",
    "middle_adjustable",
    "bottom_adjustable",
];
const HORZ_ALIGN_T5: &[(&str, &str)] = &[
    ("user_left", "left_adjustable"),
    ("user_center", "center_adjustable"),
    ("user_right", "right_adjustable"),
];
const VERT_ALIGN_T5: &[(&str, &str)] = &[
    ("user_top", "top_adjustable"),
    ("user_center", "middle_adjustable"),
    ("user_bottom", "bottom_adjustable"),
];

const DEFAULTS: &[(&str, fn() -> Value)] = &[
    ("x", || Value::Float(0.0)),
    ("y", || Value::Float(0.0)),
    ("z", || Value::Float(0.0)),
    ("alpha", || Value::Float(1.0)),
    ("color", || Value::Vector([1.0; 3])),
    ("fontscale", || Value::Float(1.0)),
    ("font", || Value::string("default")),
    ("alignx", || Value::string("left")),
    ("aligny", || Value::string("top")),
    ("horzalign", || Value::string("subleft")),
    ("vertalign", || Value::string("subtop")),
    ("sort", || Value::Float(0.0)),
    ("archived", || Value::Int(1)),
    ("foreground", || Value::Int(0)),
    ("hidewhendead", || Value::Int(0)),
    ("hidewheninmenu", || Value::Int(0)),
    ("glowalpha", || Value::Float(0.0)),
    ("glowcolor", || Value::Vector([0.0; 3])),
];

fn now_ms(world: &World) -> i32 {
    let tick = world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |r| r.tick.0);
    i32::try_from(u64::from(tick) * u64::from(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX)
}

fn seconds_ms(seconds: f32) -> i32 {
    (seconds * 1000.0).round().clamp(0.0, i32::MAX as f32) as i32
}

fn team_number(team: &str) -> i32 {
    match team {
        "axis" => entity_iw4::TEAM_AXIS,
        "allies" => entity_iw4::TEAM_ALLIES,
        "spectator" => entity_iw4::TEAM_SPECTATOR,
        _ => 0,
    }
}

pub(super) fn new_hud_elem(world: &mut World, audience: HudAudience) -> Result<Value, String> {
    let (client_num, team) = match &audience {
        HudAudience::All => (ENTITYNUM_NONE, 0),
        HudAudience::Team(team) => (ENTITYNUM_NONE, team_number(team)),
        HudAudience::Client(client) => (*client as i32, 0),
    };
    let id = world
        .resource_mut::<Runtime>()
        .create_entity(EntityKind::HudElem, "hudelem")?;
    let Some(slot) = crate::hudelem::alloc_hud_elem(
        FrameWorld::from_world(world).hud_elem_slots_mut(),
        client_num,
        team,
    ) else {
        world.resource_mut::<Runtime>().delete_entity(id);
        return Err("G_HudElems: no free hud elems".into());
    };
    let mut runtime = world.resource_mut::<Runtime>();
    for (name, value) in DEFAULTS {
        runtime.set_object_field(id, name, value());
    }
    runtime.entities.get_mut(&id).unwrap().audience = audience;
    runtime.hud_slots.insert(id, slot);
    Ok(Value::Object(id))
}

pub(super) fn destroy(world: &mut World, id: u64) {
    let slot = world.resource_mut::<Runtime>().hud_slots.remove(&id);
    if let Some(slot) = slot {
        crate::hudelem::free_hud_elem(FrameWorld::from_world(world).hud_elem_slots_mut(), slot);
    }
    world.resource_mut::<Runtime>().delete_entity(id);
}

fn slot_of(world: &World, receiver: &Value) -> Result<(u64, usize), String> {
    let runtime = world.resource::<Runtime>();
    match runtime.entity(receiver) {
        Some((id, e)) if e.kind == EntityKind::HudElem => runtime
            .hud_slots
            .get(&id)
            .map(|slot| (id, *slot))
            .ok_or_else(|| "hud element has been freed".into()),
        _ => Err("not a hud element".into()),
    }
}

fn edit<R>(
    world: &mut World,
    slot: usize,
    change: impl FnOnce(&mut crate::hudelem::GameHudElemSlot) -> R,
) -> Option<R> {
    FrameWorld::from_world(world)
        .hud_elem_slots_mut()
        .get_mut(slot)
        .map(change)
}

fn number(name: &str, value: &Value) -> Result<f32, String> {
    match value {
        Value::Int(n) => Ok(*n as f32),
        Value::Float(f) => Ok(*f),
        other => Err(format!(
            "hud element field {name} takes a number, not {other:?}"
        )),
    }
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn with_rgb(rgba: u32, rgb: [f32; 3]) -> u32 {
    let a = unpack_rgba(rgba)[3];
    color_rgba(channel(rgb[0]), channel(rgb[1]), channel(rgb[2]), a)
}

fn with_alpha(rgba: u32, alpha: f32) -> u32 {
    let [r, g, b, _] = unpack_rgba(rgba);
    color_rgba(r, g, b, channel(alpha))
}

fn pack(rgba: [u8; 4]) -> u32 {
    color_rgba(rgba[0], rgba[1], rgba[2], rgba[3])
}

fn named(name: &str, value: &Value, table: &[&str]) -> Result<i32, String> {
    named_aliased(name, value, table, &[])
}

fn named_aliased(
    name: &str,
    value: &Value,
    table: &[&str],
    aliases: &[(&str, &str)],
) -> Result<i32, String> {
    let Value::String(text) = value else {
        return Err(format!("hud element field {name} takes a string"));
    };
    let text: &str = aliases
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(text))
        .map_or(text, |(_, entry)| entry);
    table
        .iter()
        .position(|entry| entry.eq_ignore_ascii_case(text))
        .map(|i| i as i32)
        .ok_or_else(|| {
            format!(
                "unknown {name} '{text}'; should be one of {}",
                table.join(", ")
            )
        })
}

pub(super) fn string_index(world: &mut World, value: &Value) -> Result<i32, String> {
    let text = match value {
        Value::LocalizedString(key) => key.to_string(),
        Value::String(text) if text.is_empty() => String::new(),
        Value::String(text) => format!("{}{text}", crate::HUD_STRING_PLAIN),
        Value::Int(_) | Value::Float(_) => format!(
            "{}{}",
            crate::HUD_STRING_PLAIN,
            super::runtime::to_text(value).unwrap_or_default()
        ),
        other => return Err(format!("{other:?} is not a string")),
    };
    FrameWorld::from_world(world)
        .hud_string_index(&text)
        .ok_or_else(|| "exceeded maximum number of localized strings".into())
}

fn set_flag(elem: &mut HudElem, flag: i32, value: &Value, name: &str) -> Result<(), String> {
    if number(name, value)? != 0.0 {
        elem.flags |= flag;
    } else {
        elem.flags &= !flag;
    }
    Ok(())
}

pub(super) fn store_field(
    world: &mut World,
    id: u64,
    name: &str,
    value: &Value,
) -> Result<(), String> {
    let Some(&slot) = world.resource::<Runtime>().hud_slots.get(&id) else {
        return Ok(());
    };
    let label = if name == "label" {
        Some(string_index(world, value)?)
    } else {
        None
    };
    let change = |s: &mut crate::hudelem::GameHudElemSlot| -> Result<(), String> {
        let elem = &mut s.elem;
        match name {
            "x" => elem.x = number(name, value)?,
            "y" => elem.y = number(name, value)?,
            "z" => elem.z = number(name, value)?,
            "fontscale" => elem.font_scale = number(name, value)?,
            "sort" => elem.sort = number(name, value)?,
            "alpha" => elem.color_rgba = with_alpha(elem.color_rgba, number(name, value)?),
            "glowalpha" => {
                elem.glow_color_rgba = with_alpha(elem.glow_color_rgba, number(name, value)?);
            }
            "color" | "glowcolor" => {
                let Value::Vector(rgb) = value else {
                    return Err(format!("hud element field {name} takes a vector"));
                };
                if name == "color" {
                    elem.color_rgba = with_rgb(elem.color_rgba, *rgb);
                } else {
                    elem.glow_color_rgba = with_rgb(elem.glow_color_rgba, *rgb);
                }
            }
            "font" => elem.font = named(name, value, FONTS)?,
            "alignx" => {
                let horz = named(name, value, ALIGN_X)?;
                elem.align_org = align_org(horz, elem.align_org & 3);
            }
            "aligny" => {
                let vert = named(name, value, ALIGN_Y)?;
                elem.align_org = align_org((elem.align_org >> 2) & 3, vert);
            }
            "horzalign" => {
                let horz = named_aliased(name, value, HORZ_ALIGN, HORZ_ALIGN_T5)?;
                elem.align_screen = align_screen(horz, elem.align_screen & 15);
            }
            "vertalign" => {
                let vert = named_aliased(name, value, VERT_ALIGN, VERT_ALIGN_T5)?;
                elem.align_screen = align_screen((elem.align_screen >> 4) & 15, vert);
            }
            "label" => elem.label = label.unwrap_or(0),
            "archived" => s.archived = i32::from(number(name, value)? != 0.0),
            "foreground" => set_flag(elem, flags::FOREGROUND, value, name)?,
            "hidewhendead" => set_flag(elem, flags::HIDEWHENDEAD, value, name)?,
            "hidewheninmenu" => set_flag(elem, flags::HIDEWHENINMENU, value, name)?,
            "splatter" => set_flag(elem, flags::SPLATTER, value, name)?,
            "lowresbackground" => set_flag(elem, flags::LOWRESBACKGROUND, value, name)?,
            _ => {}
        }
        Ok(())
    };
    edit(world, slot, change).unwrap_or(Ok(()))
}

fn timer(world: &mut World, receiver: &Value, args: &[Value], kind: i32) -> Result<Value, String> {
    let (_, slot) = slot_of(world, receiver)?;
    let ms = seconds_ms(float(args, 0)?);
    let now = now_ms(world);
    edit(world, slot, |s| {
        s.elem.elem_type = kind;
        s.elem.time = match kind {
            HE_TYPE_TIMER_DOWN | HE_TYPE_TENTHS_TIMER_DOWN => now.saturating_add(ms),
            HE_TYPE_TIMER_UP | HE_TYPE_TENTHS_TIMER_UP => now.saturating_sub(ms),
            _ => ms,
        };
    });
    Ok(Value::Undefined)
}

fn clock(world: &mut World, receiver: &Value, args: &[Value], kind: i32) -> Result<Value, String> {
    let (_, slot) = slot_of(world, receiver)?;
    let ms = seconds_ms(float(args, 0)?);
    let duration = seconds_ms(float(args, 1)?);
    let material = string(args, 2)?;
    let width = optional(args, 3, int)?.unwrap_or(0);
    let height = optional(args, 4, int)?.unwrap_or(0);
    let now = now_ms(world);
    let index = FrameWorld::from_world(world).hud_material_index(&material);
    edit(world, slot, |s| {
        s.elem.elem_type = kind;
        s.elem.time = if kind == HE_TYPE_CLOCK_DOWN {
            now.saturating_add(ms)
        } else {
            now.saturating_sub(ms)
        };
        s.elem.duration = duration;
        s.elem.material_index = i32::from(index);
        s.elem.width = width;
        s.elem.height = height;
    });
    Ok(Value::Undefined)
}

fn plain_text(world: &mut World, value: &Value) -> String {
    let player = match value {
        Value::Object(id) => world.resource::<Runtime>().player_client(*id),
        _ => None,
    };
    let name = player.and_then(|client| super::players::load_field(world, client, "name"));
    match name.as_ref().unwrap_or(value) {
        Value::String(text) => text.to_string(),
        Value::LocalizedString(key) => key.to_string(),
        other => super::runtime::to_text(other).unwrap_or_default(),
    }
}

fn print_text(world: &mut World, value: &Value) -> String {
    match value {
        Value::LocalizedString(key) => key.to_string(),
        other => format!("{}{}", crate::HUD_STRING_PLAIN, plain_text(world, other)),
    }
}

fn print(
    world: &mut World,
    recipient: Option<u32>,
    bold: bool,
    args: &[Value],
) -> Result<Value, String> {
    let (template, arg) = match arg(args, 0)? {
        Value::LocalizedString(key) => (
            key.to_string(),
            args[1..]
                .iter()
                .map(|value| print_text(world, value))
                .collect::<Vec<_>>()
                .join(&crate::HUD_PRINT_ARG_SEPARATOR.to_string()),
        ),
        _ => {
            let mut text = String::from(crate::HUD_STRING_PLAIN);
            for value in args {
                match value {
                    Value::LocalizedString(key) => text.push_str(key),
                    other => text.push_str(&plain_text(world, other)),
                }
            }
            (text, String::new())
        }
    };
    FrameWorld::from_world(world).push_print(crate::PendingPrint {
        recipient: recipient.map(crate::ClientId),
        bold,
        template,
        arg,
    });
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Function, "iprintln", |world, _, args| {
        print(world, None, false, args)
    });
    registry.register(Function, "iprintlnbold", |world, _, args| {
        print(world, None, true, args)
    });
    registry.register(Method, "iprintln", |world, receiver, args| {
        let client = super::natives_player::player(world, receiver)?;
        print(world, Some(client), false, args)
    });
    registry.register(Method, "iprintlnbold", |world, receiver, args| {
        let client = super::natives_player::player(world, receiver)?;
        print(world, Some(client), true, args)
    });
    registry.register(Function, "newhudelem", |world, _, _| {
        new_hud_elem(world, HudAudience::All)
    });
    registry.register(Function, "newteamhudelem", |world, _, args| {
        let team = string(args, 0)?;
        if !matches!(
            team.as_str(),
            "allies" | "axis" | "free" | "spectator" | "none" | "neutral"
        ) {
            return Err(format!("'{team}' is an illegal team string"));
        }
        new_hud_elem(world, HudAudience::Team(team.into()))
    });
    registry.register(Function, "newclienthudelem", |world, _, args| {
        let client = super::natives_player::player(world, arg(args, 0)?)
            .map_err(|_| "newClientHudElem needs a player".to_string())?;
        new_hud_elem(world, HudAudience::Client(client))
    });
    registry.register(Function, "newscorehudelem", |world, _, args| {
        let client = super::natives_player::player(world, arg(args, 0)?)
            .map_err(|_| "NewScoreHudElem needs a player".to_string())?;
        new_hud_elem(world, HudAudience::Client(client))
    });
    registry.register(Function, "newdebughudelem", |world, _, _| {
        new_hud_elem(world, HudAudience::All)
    });
    registry.register(Method, "setcod7decodefx", |world, receiver, args| {
        slot_of(world, receiver)?;
        for index in 0..3 {
            int(args, index)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "destroy", |world, receiver, _| {
        let (id, _) = slot_of(world, receiver)?;
        destroy(world, id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "reset", |world, receiver, _| {
        let (id, slot) = slot_of(world, receiver)?;
        edit(world, slot, |s| {
            s.elem = crate::hudelem::default_hud_elem();
            s.archived = 1;
        });
        let mut runtime = world.resource_mut::<Runtime>();
        for (name, value) in DEFAULTS {
            runtime.set_object_field(id, name, value());
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "settext", |world, receiver, args| {
        let (_, slot) = slot_of(world, receiver)?;
        let text = string_index(world, arg(args, 0)?)?;
        edit(world, slot, |s| {
            s.elem.elem_type = HE_TYPE_TEXT;
            s.elem.text = text;
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "setvalue", |world, receiver, args| {
        let (_, slot) = slot_of(world, receiver)?;
        let value = float(args, 0)?;
        edit(world, slot, |s| {
            s.elem.elem_type = HE_TYPE_VALUE;
            s.elem.value = value;
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "setshader", |world, receiver, args| {
        let (_, slot) = slot_of(world, receiver)?;
        let material = string(args, 0)?;
        let width = optional(args, 1, int)?.unwrap_or(0);
        let height = optional(args, 2, int)?.unwrap_or(0);
        let index = FrameWorld::from_world(world).hud_material_index(&material);
        edit(world, slot, |s| {
            s.elem.elem_type = HE_TYPE_MATERIAL;
            s.elem.material_index = i32::from(index);
            s.elem.width = width;
            s.elem.height = height;
        });
        Ok(Value::Undefined)
    });
    macro_rules! timers {
        ($($name:literal => $kind:expr),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                timer(world, receiver, args, $kind)
            });
        )*};
    }
    timers! {
        "settimer" => HE_TYPE_TIMER_DOWN,
        "settimerup" => HE_TYPE_TIMER_UP,
        "settimerstatic" => HE_TYPE_TIMER_STATIC,
        "settenthstimer" => HE_TYPE_TENTHS_TIMER_DOWN,
        "settenthstimerup" => HE_TYPE_TENTHS_TIMER_UP,
        "settenthstimerstatic" => HE_TYPE_TENTHS_TIMER_STATIC,
    }
    registry.register(Method, "setclock", |world, receiver, args| {
        clock(world, receiver, args, HE_TYPE_CLOCK_DOWN)
    });
    registry.register(Method, "setclockup", |world, receiver, args| {
        clock(world, receiver, args, HE_TYPE_CLOCK_UP)
    });
    registry.register(Method, "setplayernamestring", |world, receiver, args| {
        let (_, slot) = slot_of(world, receiver)?;
        let client = super::natives_player::player(world, arg(args, 0)?)?;
        edit(world, slot, |s| {
            s.elem.elem_type = HE_TYPE_PLAYERNAME;
            s.elem.value = client as f32;
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "setwaypoint", |world, receiver, args| {
        let (_, slot) = slot_of(world, receiver)?;
        let flag = |at: usize| optional(args, at, int).map(|v| v.unwrap_or(0) != 0);
        let mut bits = 0;
        let flags: &[(usize, i32)] = match args.get(1) {
            // On T5 the second argument is a material name, not a flag.
            Some(Value::String(_)) => &[(0, hud_iw4::WAYPOINT_CONSTANT_SIZE)],
            _ => &[
                (0, hud_iw4::WAYPOINT_CONSTANT_SIZE),
                (1, hud_iw4::WAYPOINT_PULSE_OFFSCREEN),
                (2, hud_iw4::WAYPOINT_HIDE_OFFSCREEN),
            ],
        };
        for &(at, bit) in flags {
            if flag(at)? {
                bits |= bit;
            }
        }
        edit(world, slot, |s| {
            s.elem.elem_type = HE_TYPE_WAYPOINT;
            s.elem.value = bits as f32;
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "settargetent", |world, receiver, args| {
        let (_, slot) = slot_of(world, receiver)?;
        let number = match world.resource::<Runtime>().entity(arg(args, 0)?) {
            Some((_, e)) if e.kind != EntityKind::HudElem && e.number >= 0 => e.number,
            _ => return Err("setTargetEnt needs an entity".into()),
        };
        edit(world, slot, |s| s.elem.target_ent_num = number);
        Ok(Value::Undefined)
    });
    registry.register(Method, "cleartargetent", |world, receiver, _| {
        let (_, slot) = slot_of(world, receiver)?;
        edit(world, slot, |s| s.elem.target_ent_num = ENTITYNUM_NONE);
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearalltextafterhudelem", |world, receiver, _| {
        slot_of(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "fadeovertime", |world, receiver, args| {
        let (_, slot) = slot_of(world, receiver)?;
        let ms = seconds_ms(float(args, 0)?);
        let now = now_ms(world);
        edit(world, slot, |s| {
            s.elem.from_color_rgba = pack(bg_lerp_hud_colors(&s.elem, now));
            s.elem.fade_start_time = now;
            s.elem.fade_time = ms;
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "moveovertime", |world, receiver, args| {
        let (_, slot) = slot_of(world, receiver)?;
        let ms = seconds_ms(float(args, 0)?);
        let now = now_ms(world);
        edit(world, slot, |s| {
            let e = &mut s.elem;
            let lerp = hud_elem_movement_frac(e, now);
            let same_align =
                e.from_align_org == e.align_org && e.from_align_screen == e.align_screen;
            if lerp < 1.0 && same_align {
                e.from_x += (e.x - e.from_x) * lerp;
                e.from_y += (e.y - e.from_y) * lerp;
            } else {
                e.from_x = e.x;
                e.from_y = e.y;
            }
            e.from_align_org = e.align_org;
            e.from_align_screen = e.align_screen;
            e.move_start_time = now;
            e.move_time = ms;
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "scaleovertime", |world, receiver, args| {
        let (_, slot) = slot_of(world, receiver)?;
        let ms = seconds_ms(float(args, 0)?);
        let width = int(args, 1)?;
        let height = int(args, 2)?;
        let now = now_ms(world);
        edit(world, slot, |s| {
            let e = &mut s.elem;
            let (from_w, from_h) = match hud_elem_scale_frac(e, now) {
                Some(lerp) => (
                    e.from_width + ((e.width - e.from_width) as f32 * lerp) as i32,
                    e.from_height + ((e.height - e.from_height) as f32 * lerp) as i32,
                ),
                None => (e.width, e.height),
            };
            e.from_width = from_w;
            e.from_height = from_h;
            if hud_elem_movement_frac(e, now) >= 1.0 {
                e.from_align_screen = e.align_screen;
            }
            e.scale_start_time = now;
            e.scale_time = ms;
            e.width = width;
            e.height = height;
        });
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "changefontscaleovertime",
        |world, receiver, args| {
            let (_, slot) = slot_of(world, receiver)?;
            let ms = seconds_ms(float(args, 0)?);
            let now = now_ms(world);
            edit(world, slot, |s| {
                s.elem.from_font_scale = hud_elem_lerp_font_scale(&s.elem, now);
                s.elem.font_scale_start_time = now;
                s.elem.font_scale_time = ms;
            });
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "setpulsefx", |world, receiver, args| {
        let (_, slot) = slot_of(world, receiver)?;
        let speed = int(args, 0)?;
        let decay_start = int(args, 1)?;
        let decay_duration = int(args, 2)?;
        let now = now_ms(world);
        let mut frame = FrameWorld::from_world(world);
        let mut ids = frame.hud_elem_sound_ids();
        if let Some(s) = frame.hud_elem_slots_mut().get_mut(slot) {
            crate::hudelem::set_pulse_fx(
                &mut s.elem,
                now,
                speed,
                decay_start,
                decay_duration,
                &mut ids,
            );
        }
        frame.set_hud_elem_sound_ids(ids);
        Ok(Value::Undefined)
    });
}
