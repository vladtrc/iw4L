use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

use crate::script::host::natives;
use crate::script::{Namespace, Runtime, Value};

pub type Native = fn(&mut World, &Value, &[Value]) -> Result<Value, String>;

#[derive(Resource, Clone)]
pub struct NativeRegistry(BTreeMap<(Namespace, String), Native>);
impl Default for NativeRegistry {
    /// The engine services alone: no game's own natives.
    fn default() -> Self {
        Self::engine_services()
    }
}
impl NativeRegistry {
    /// Builtins every game's scripts are served the same way: the language
    /// helpers, dvars, math, entities, players, sound, physics, triggers.
    pub fn engine_services() -> Self {
        let mut registry = Self(BTreeMap::new());
        registry.register(Namespace::Function, "isdefined", |world, _, args| {
            if args.len() != 1 {
                return Err("isdefined expects one argument".into());
            }
            let defined = match &args[0] {
                Value::Undefined => false,
                Value::Object(id) => world.resource::<Runtime>().live(id),
                _ => true,
            };
            Ok(Value::Int(i32::from(defined)))
        });
        registry.register(Namespace::Function, "gettime", |world, _, args| {
            if !args.is_empty() {
                return Err("gettime expects no arguments".into());
            }
            let tick = world.resource::<crate::step::StepRequest>().tick;
            let ms = u64::from(tick.0) * u64::from(crate::MATCH_TICK_MS);
            i32::try_from(ms)
                .map(Value::Int)
                .map_err(|_| "gettime overflow".into())
        });
        registry.register(Namespace::Function, "spawnstruct", |world, _, args| {
            if !args.is_empty() {
                return Err("spawnstruct expects no arguments".into());
            }
            let mut runtime = world.resource_mut::<Runtime>();
            let id = runtime.next_object;
            runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
            runtime.objects.insert(id, BTreeMap::new());
            Ok(Value::Object(id))
        });
        natives::iw4::register(&mut registry);
        super::audio::register(&mut registry);
        natives::math::register(&mut registry);
        natives::shared::register(&mut registry);
        natives::engine::register(&mut registry);
        natives::player::register(&mut registry);
        super::weapons::register(&mut registry);
        super::physics::register(&mut registry);
        super::controls::register(&mut registry);
        super::client_effects::register(&mut registry);
        super::triggers::register(&mut registry);
        super::spectators::register(&mut registry);
        registry
    }

    /// Modern Warfare 2's game systems: ranks, the local profile,
    /// objectives, vehicles, missile guidance, turrets.
    pub fn with_mw2_systems(mut self) -> Self {
        natives::skill::register(&mut self);
        natives::local_profile::register(&mut self);
        super::objectives::register(&mut self);
        super::vehicles::register(&mut self);
        super::guidance::register(&mut self);
        super::turrets::register(&mut self);
        self
    }

    /// Black Ops' natives: its builtins, zombies, and AI actors.
    pub fn with_black_ops(mut self) -> Self {
        natives::t5::register(&mut self);
        natives::t5_zombie::register(&mut self);
        super::actors::register(&mut self);
        super::actors::register_damage(&mut self);
        super::actor_anims::register(&mut self);
        super::actor_nav::register(&mut self);
        self
    }
}
impl NativeRegistry {
    pub fn register(&mut self, namespace: Namespace, name: &str, native: Native) {
        self.0
            .insert((namespace, name.to_ascii_lowercase()), native);
    }
    /// Binds every builtin the program names that the host does not implement
    /// to a native that fails, so a partial host still runs the scripts and each
    /// missing call site is reported once. Returns the names it bound.
    pub fn bind_gaps(&mut self, program: &crate::script::Program) -> Vec<&'static str> {
        let mut gaps = Vec::new();
        for builtin in &program.natives {
            if self.get(builtin.namespace, builtin.name).is_none() {
                self.register(builtin.namespace, builtin.name, |_, _, _| {
                    Err("not implemented yet".into())
                });
                gaps.push(builtin.name);
            }
        }
        gaps.sort_unstable();
        gaps.dedup();
        gaps
    }
    pub(crate) fn get(&self, namespace: Namespace, name: &str) -> Option<Native> {
        self.0.get(&(namespace, name.to_owned())).copied()
    }
}

pub(crate) fn unavailable(
    world: &mut World,
    name: &'static str,
    reason: &str,
) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<Runtime>();
    let calls = runtime.unsupported.entry(name).or_default();
    *calls = calls.saturating_add(1);
    Err(format!("unavailable: {reason}"))
}
