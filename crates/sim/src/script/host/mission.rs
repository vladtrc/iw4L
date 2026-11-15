use super::args::string;
use super::restart::Detached;
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;
use std::sync::Arc;

const FAIL_RESTART_MS: i32 = 3_000;
const FAIL_BLUR: f32 = 32.0;
const FAIL_TIME_SCALE: f32 = 0.1;
const FAIL_LEVEL_MS: i64 = FAIL_RESTART_MS as i64 * 11 / 20;

const RECENT_LOAD_MS: i64 = 1_000;

#[derive(Default)]
struct Saves {
    level: String,
    next_id: i32,
    pending: Option<(i32, crate::Tick, crate::SimWorld)>,
    committed: Option<(crate::Tick, crate::SimWorld)>,
    restore: bool,
}

static SAVES: std::sync::Mutex<Option<Saves>> = std::sync::Mutex::new(None);

fn saves() -> std::sync::MutexGuard<'static, Option<Saves>> {
    SAVES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

static CARRIED: std::sync::Mutex<Option<(String, Vec<(Arc<str>, Detached)>)>> =
    std::sync::Mutex::new(None);

fn carried() -> std::sync::MutexGuard<'static, Option<(String, Vec<(Arc<str>, Detached)>)>> {
    CARRIED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

static GAME_SKILL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

pub fn set_game_skill(skill: u8) {
    GAME_SKILL.store(skill.min(3), std::sync::atomic::Ordering::Relaxed);
}

pub fn game_skill() -> u8 {
    GAME_SKILL.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn restore_game(world: &mut World) {
    let level = current_level(world);
    let mut guard = carried();
    match guard.as_ref() {
        Some((to, _)) if to.eq_ignore_ascii_case(&level) => {}
        _ => {
            *guard = None;
            return;
        }
    }
    let (_, fields) = guard.as_ref().unwrap();
    let fields = fields.clone();
    drop(guard);
    let mut runtime = world.resource_mut::<RoundScript>();
    for (name, value) in fields {
        match super::restart::attach(&mut runtime, value) {
            Ok(value) => runtime.set_object_field(1, &name, value),
            Err(message) => diag::warn!(Sim, "campaign: game.{name} not carried over: {message}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SaveRequest {
    id: i32,
    commit: bool,
}

pub(crate) fn take_save(world: &mut crate::SimWorld, tick: crate::Tick) {
    let Some(request) = world
        .ecs_mut()
        .resource_mut::<RoundScript>()
        .campaign
        .save
        .take()
    else {
        return;
    };
    let level = current_level(world.ecs());
    let copy = world.clone();
    let mut guard = saves();
    let slot = guard.get_or_insert_with(Saves::default);
    if slot.level != level {
        *slot = Saves {
            level,
            next_id: slot.next_id,
            ..Saves::default()
        };
    }
    if request.commit {
        slot.pending = None;
        slot.committed = Some((tick, copy));
        diag::info!(Sim, "campaign: checkpoint saved at tick {}", tick.0);
    } else {
        slot.pending = Some((request.id, tick, copy));
    }
}

pub(crate) fn restore_checkpoint(level: &str, fresh: &crate::SimWorld) -> Option<crate::SimWorld> {
    let mut guard = saves();
    let slot = guard.as_mut()?;
    if !slot.restore || !slot.level.eq_ignore_ascii_case(level) {
        *guard = None;
        return None;
    }
    slot.restore = false;
    slot.pending = None;
    let (tick, saved) = slot.committed.as_mut()?;
    saved.replace_content(fresh.content());
    let mut world = saved.clone();
    let runtime = &mut world.ecs_mut().resource_mut::<RoundScript>().campaign;
    runtime.resume_tick = Some(*tick);
    runtime.loaded_ms = i64::from(tick.0) * i64::from(crate::MATCH_TICK_MS);
    diag::info!(
        Sim,
        "campaign: resuming '{level}' from the checkpoint at tick {}",
        tick.0
    );
    Some(world)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LevelChange {
    pub(crate) due_ms: i64,
    pub(crate) level: String,
    pub(crate) sent: bool,
    quote: Option<u64>,
}

fn current_level(world: &World) -> String {
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("mapname")
        .cloned()
        .unwrap_or_default()
}

fn request(world: &mut World, level: String, delay_ms: i64) {
    let due_ms = super::players::now_ms(world) + delay_ms;
    let restart = level == current_level(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    if runtime.campaign.level_change.is_none() {
        diag::info!(Sim, "campaign: load level '{level}' in {delay_ms} ms");
        if let Some(slot) = saves().as_mut() {
            slot.restore = restart && slot.committed.is_some();
        }
        runtime.campaign.level_change = Some(LevelChange {
            due_ms,
            level,
            sent: false,
            quote: None,
        });
        drop(runtime);
        if restart {
            fail_screen(world);
        }
    }
}

fn fail_screen(world: &mut World) {
    crate::frame::FrameWorld::from_world(world).push_script_audio(
        crate::ScriptAudioCommand::SoundFade {
            volume: 0.0,
            fade_ms: FAIL_RESTART_MS,
        },
    );
    for client in FrameWorld::from_world(world).client_ids_sorted() {
        super::client_effects::blur(world, client.0, FAIL_BLUR, FAIL_LEVEL_MS as i32);
    }
    let start_ms = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    world.resource_mut::<RoundScript>().engine.slow_motion = Some(crate::ScriptSlowMotion {
        from: 1.0,
        to: FAIL_TIME_SCALE,
        start_ms,
        duration_ms: FAIL_RESTART_MS,
    });
    let Some(title) = text_elem(world, -40.0, 2.0, [1.0, 0.25, 0.2]) else {
        return;
    };
    set_text(world, title, &Value::string("MISSION FAILED"));
    let quote = text_elem(world, 0.0, 1.4, [1.0, 1.0, 1.0]);
    if let Some(change) = world
        .resource_mut::<RoundScript>()
        .campaign
        .level_change
        .as_mut()
    {
        change.quote = quote;
    }
    fill_quote(world);
}

fn text_elem(world: &mut World, y: f32, scale: f32, color: [f32; 3]) -> Option<u64> {
    let Ok(Value::Object(id)) = super::hud::new_hud_elem(world, super::entities::HudAudience::All)
    else {
        return None;
    };
    let fields = [
        ("horzalign", Value::string("center")),
        ("vertalign", Value::string("middle")),
        ("alignx", Value::string("center")),
        ("aligny", Value::string("middle")),
        ("font", Value::string("objective")),
        ("fontscale", Value::Float(scale)),
        ("y", Value::Float(y)),
        ("color", Value::Vector(color)),
        ("foreground", Value::Int(1)),
    ];
    for (name, value) in fields {
        let _ = super::hud::store_field(world, id, name, &value);
    }
    Some(id)
}

fn set_text(world: &mut World, id: u64, value: &Value) {
    let Ok(text) = super::hud::string_index(world, value) else {
        return;
    };
    let Some(&slot) = world.resource::<RoundScript>().hud_slots.get(&id) else {
        return;
    };
    if let Some(s) = FrameWorld::from_world(world)
        .hud_elem_slots_mut()
        .get_mut(slot)
    {
        s.elem.elem_type = hud_iw4::HE_TYPE_TEXT;
        s.elem.text = text;
    }
}

fn fill_quote(world: &mut World) {
    let Some(id) = world
        .resource::<RoundScript>()
        .campaign
        .level_change
        .as_ref()
        .and_then(|change| change.quote)
    else {
        return;
    };
    let quote = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("ui_deadquote")
        .map(|q| q.trim_start_matches('@').to_owned())
        .unwrap_or_default();
    if quote.is_empty() {
        return;
    }
    set_text(world, id, &Value::LocalizedString(quote.into()));
    if let Some(change) = world
        .resource_mut::<RoundScript>()
        .campaign
        .level_change
        .as_mut()
    {
        change.quote = None;
    }
}

pub(crate) fn advance(world: &mut World) {
    let clients: Vec<u32> = world
        .resource::<RoundScript>()
        .players
        .keys()
        .copied()
        .collect();
    let dead = {
        let frame = FrameWorld::from_world(world);
        frame.bootstrap_ref().campaign
            && clients.into_iter().any(|c| {
                frame
                    .client_meta(crate::ClientId(c))
                    .is_some_and(|m| m.lifecycle == crate::ClientLifecycle::Dead)
            })
    };
    if dead {
        let level = current_level(world);
        request(world, level, FAIL_LEVEL_MS);
    }
    fill_quote(world);
}

pub(crate) fn take_due(world: &mut World) -> Option<String> {
    let now = super::players::now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    let change = runtime.campaign.level_change.as_mut()?;
    if change.sent || change.due_ms > now {
        return None;
    }
    change.sent = true;
    let level = change.level.clone();
    drop(runtime);
    if !level.is_empty() && !level.eq_ignore_ascii_case(&current_level(world)) {
        let game = super::restart::detach_game(world.resource::<RoundScript>());
        *carried() = Some((level.clone(), game));
    }
    Some(level)
}

fn save(world: &mut World, commit: bool) -> Value {
    let id = {
        let mut guard = saves();
        let slot = guard.get_or_insert_with(Saves::default);
        slot.next_id += 1;
        slot.next_id
    };
    world.resource_mut::<RoundScript>().campaign.save = Some(SaveRequest { id, commit });
    Value::Int(id)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;
    registry.register(Namespace::Method, "getplayersetting", |_, _, args| {
        Ok(match string(args, 0)?.to_ascii_lowercase().as_str() {
            "gameskill" => Value::Int(game_skill().into()),
            _ => Value::Undefined,
        })
    });
    registry.register(Function, "savegame", |world, _, _| Ok(save(world, true)));
    registry.register(Function, "savegamenocommit", |world, _, _| {
        Ok(save(world, false))
    });
    registry.register(Function, "commitsave", |_, _, args| {
        let id = super::args::int(args, 0)?;
        if let Some(slot) = saves().as_mut()
            && let Some((pending, tick, world)) = slot.pending.take()
        {
            if pending == id {
                diag::info!(Sim, "campaign: checkpoint saved at tick {}", tick.0);
                slot.committed = Some((tick, world));
            } else {
                slot.pending = Some((pending, tick, world));
            }
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "commitwouldbevalid", |_, _, args| {
        let id = super::args::int(args, 0)?;
        let valid = saves()
            .as_ref()
            .and_then(|slot| slot.pending.as_ref())
            .is_some_and(|(pending, _, _)| *pending == id);
        Ok(Value::Int(valid.into()))
    });
    registry.register(Function, "issaverecentlyloaded", |world, _, _| {
        let loaded = world.resource::<RoundScript>().campaign.loaded_ms;
        let now = super::players::now_ms(world);
        Ok(Value::Int(
            (loaded > 0 && now - loaded < RECENT_LOAD_MS).into(),
        ))
    });
    registry.register(Function, "missionfailed", |world, _, _| {
        let level = current_level(world);
        request(world, level, FAIL_LEVEL_MS);
        Ok(Value::Undefined)
    });
    for name in ["missionsuccess", "changelevel"] {
        registry.register(Function, name, |world, _, args| {
            let level = string(args, 0)?;
            request(world, level, 0);
            Ok(Value::Undefined)
        });
    }
}
