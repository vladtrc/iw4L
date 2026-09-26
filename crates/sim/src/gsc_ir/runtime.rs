use super::*;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::World;

pub type Native = fn(&mut World, &Value, &[Value]) -> Result<Value, String>;

#[derive(Resource, Clone)]
pub struct NativeRegistry(BTreeMap<(Namespace, String), Native>);
impl Default for NativeRegistry {
    fn default() -> Self {
        let mut registry = Self(BTreeMap::new());
        registry.register(Namespace::Function, "isdefined", |world, _, args| {
            if args.len() != 1 {
                return Err("isdefined expects one argument".into());
            }
            let defined = match &args[0] {
                Value::Undefined => false,
                Value::Entity(entity) => crate::frame::FrameWorld::from_world(world)
                    .entity_kernel()
                    .resolve(*entity)
                    .is_ok(),
                Value::Object(id) => world.resource::<Runtime>().objects.contains_key(id),
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
        super::iw4_natives::register(&mut registry);
        super::natives_math::register(&mut registry);
        super::natives_engine::register(&mut registry);
        super::natives_player::register(&mut registry);
        super::objectives::register(&mut registry);
        super::weapons::register(&mut registry);
        super::vehicles::register(&mut registry);
        super::physics::register(&mut registry);
        super::natives_t5::register(&mut registry);
        registry
    }
}
impl NativeRegistry {
    pub fn register(&mut self, namespace: Namespace, name: &str, native: Native) {
        self.0
            .insert((namespace, name.to_ascii_lowercase()), native);
    }
    pub(super) fn get(&self, namespace: Namespace, name: &str) -> Option<Native> {
        self.0.get(&(namespace, name.to_owned())).copied()
    }
}

impl Runtime {
    pub(super) fn symbol(&mut self, name: &str) -> u32 {
        let program = self.program.as_ref().unwrap();
        if let Some(&id) = program.symbol_ids.get(name) {
            return id;
        }
        let next = (program.symbols.len() + self.dynamic_symbols.len()) as u32;
        *self.dynamic_symbols.entry(name.into()).or_insert(next)
    }
}

pub(crate) fn copy_state(source: &World, target: &mut World) {
    target.insert_resource(source.resource::<Runtime>().clone());
    target.insert_resource(source.resource::<NativeRegistry>().clone());
    for entity in source.iter_entities() {
        if let Some(thread) = entity.get::<Thread>() {
            target.spawn(thread.clone());
        }
    }
}

pub(crate) fn reset(world: &mut World) {
    let ids: Vec<_> = world
        .query_filtered::<Entity, bevy_ecs::query::With<Thread>>()
        .iter(world)
        .collect();
    for id in ids {
        world.despawn(id);
    }
    world.insert_resource(Runtime::default());
}

pub(crate) fn install(
    world: &mut World,
    program: Program,
    natives: NativeRegistry,
    level: LevelData,
) -> Result<(), Fault> {
    let location = Location {
        module: "<runtime>".into(),
        function: "install".into(),
        line: 0,
        column: 0,
    };
    if world.resource::<Runtime>().program.is_some() {
        return Err(Fault::at(
            &location,
            "program already installed; reset the match before replacing scripts",
        ));
    }
    let mut unbound: Vec<_> = program
        .natives
        .iter()
        .filter(|b| natives.get(b.namespace, b.name).is_none())
        .map(|b| b.name)
        .collect();
    unbound.sort_unstable();
    if !unbound.is_empty() {
        return Err(Fault::at(
            &location,
            format!("{} unbound natives: {}", unbound.len(), unbound.join(" ")),
        ));
    }
    let bound = program
        .natives
        .iter()
        .map(|b| natives.get(b.namespace, b.name).unwrap())
        .collect();
    let program = Arc::new(program);
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.program = Some(program.clone());
    runtime.natives = bound;
    runtime.objects.insert(0, BTreeMap::new());
    runtime.objects.insert(1, BTreeMap::new());
    runtime.objects.insert(2, BTreeMap::new());
    runtime.next_object = 3;
    runtime.next_entity_number = playerstate_iw4::GENTITY_SPAWN_BASE;
    runtime.tables = Arc::new(level.tables);
    runtime.rng = u32::from_le_bytes(program.fingerprint()[..4].try_into().unwrap()) | 1;
    runtime.loading = true;
    world.insert_resource(natives);
    if let Some(&function) = program.names.get(STRUCT_INIT) {
        let mut thread = new_thread(world, &program, function, Value::Object(0), Vec::new())
            .map_err(|m| Fault::at(&location, m))?;
        world.resource_mut::<Runtime>().budget = INSTRUCTION_BUDGET;
        thread.state = ThreadState::Runnable;
        execute(world, &program, &mut thread, 0);
        let mut runtime = world.resource_mut::<Runtime>();
        if let Some(fault) = runtime.fault.clone() {
            return Err(fault);
        }
        if thread.state != ThreadState::Complete {
            return Err(Fault::at(&location, format!("{STRUCT_INIT} must not wait")));
        }
        runtime.started = false;
    }
    world
        .resource_mut::<Runtime>()
        .spawn_map_entities(&level.entities)
        .map_err(|m| Fault::at(&location, m))?;
    Ok(())
}

/// Runs during install, before the map's `script_struct` blocks are appended to `level.struct`.
pub const STRUCT_INIT: &str = "codescripts/struct::initstructs";
/// Level startup runs in one frame and needs over a million instructions on the larger maps.
const INSTRUCTION_BUDGET: usize = 16 * 1_000_000;

pub(crate) fn take_signals(world: &mut World) -> Vec<Arc<str>> {
    std::mem::take(&mut world.resource_mut::<Runtime>().signals)
}

pub(super) fn raise(world: &mut World, receiver: Value, name: &str, args: Vec<Value>) {
    world
        .resource_mut::<Runtime>()
        .pending_notifies
        .push((receiver, name.into(), args));
}

fn deliver_pending(world: &mut World, thread: &mut Thread, now: i64) -> Result<(), String> {
    loop {
        let pending = std::mem::take(&mut world.resource_mut::<Runtime>().pending_notifies);
        if pending.is_empty() {
            return Ok(());
        }
        for (receiver, name, args) in pending {
            notify(world, thread, &receiver, &name, &args, now)?;
        }
    }
}

fn deliver_external(world: &mut World, now: i64) {
    let mut carrier = Thread {
        serial: u64::MAX,
        frames: Vec::new(),
        stack: Vec::new(),
        state: ThreadState::Complete,
    };
    if let Err(message) = deliver_pending(world, &mut carrier, now) {
        world.resource_mut::<Runtime>().fault = Some(Fault::at(
            &Location {
                module: "<engine>".into(),
                function: "notify".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
    }
}

fn frame(
    program: &Program,
    function: usize,
    args: Vec<Value>,
    stack_base: usize,
    receiver: Value,
) -> Frame {
    let definition = &program.functions[function];
    let mut locals = vec![Value::Undefined; definition.slots];
    for (slot, value) in locals.iter_mut().zip(args).take(definition.parameters) {
        *slot = value;
    }
    Frame {
        function,
        pc: 0,
        stack_base,
        receiver,
        locals,
    }
}

fn entry(world: &World, name: &str) -> Result<(Arc<Program>, usize, Location), Fault> {
    let location = Location {
        module: "<runtime>".into(),
        function: name.into(),
        line: 0,
        column: 0,
    };
    let runtime = world.resource::<Runtime>();
    if let Some(fault) = &runtime.fault {
        return Err(fault.clone());
    }
    let program = runtime
        .program
        .as_ref()
        .ok_or_else(|| Fault::at(&location, "no loaded GSC program"))?
        .clone();
    let function = *program
        .names
        .get(&name.replace('\\', "/").to_ascii_lowercase())
        .ok_or_else(|| Fault::at(&location, "unknown script entry point"))?;
    Ok((program, function, location))
}

pub(crate) fn start(
    world: &mut World,
    name: &str,
    receiver: Value,
    args: Vec<Value>,
) -> Result<u64, Fault> {
    let (program, function, location) = entry(world, name)?;
    spawn_thread(world, &program, function, receiver, args).map_err(|m| Fault::at(&location, m))
}

pub(super) fn run_now(
    world: &mut World,
    name: &str,
    receiver: Value,
    args: Vec<Value>,
    now: i64,
) -> Result<(), Fault> {
    let (program, function, location) = entry(world, name)?;
    let mut thread = new_thread(world, &program, function, receiver, args)
        .map_err(|m| Fault::at(&location, m))?;
    thread.state = ThreadState::Runnable;
    execute(world, &program, &mut thread, now);
    if thread.state == ThreadState::Complete {
        retire(&mut world.resource_mut::<Runtime>(), thread.serial);
    } else {
        world.spawn(thread);
    }
    match world.resource::<Runtime>().fault.clone() {
        Some(fault) => Err(fault),
        None => Ok(()),
    }
}

fn spawn_thread(
    world: &mut World,
    program: &Program,
    function: usize,
    receiver: Value,
    args: Vec<Value>,
) -> Result<u64, String> {
    let thread = new_thread(world, program, function, receiver, args)?;
    let serial = thread.serial;
    world.resource_mut::<Runtime>().spawned.push(serial);
    world.spawn(thread);
    Ok(serial)
}

fn new_thread(
    world: &mut World,
    program: &Program,
    function: usize,
    receiver: Value,
    args: Vec<Value>,
) -> Result<Thread, String> {
    let args = args
        .into_iter()
        .map(|value| copy_value(world, value))
        .collect::<Result<Vec<_>, _>>()?;
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.started = true;
    let serial = runtime.next_serial;
    runtime.next_serial = serial.checked_add(1).ok_or("thread identifier exhausted")?;
    Ok(Thread {
        serial,
        frames: vec![frame(program, function, args, 0, receiver)],
        stack: Vec::new(),
        state: ThreadState::Queued,
    })
}

/// Frames live at once across a thread and the threads suspended beneath it.
const MAX_FRAMES: usize = 31;

fn frame_room(world: &World, thread: &Thread) -> Result<(), String> {
    if world.resource::<Runtime>().suspended_frames + thread.frames.len() >= MAX_FRAMES {
        return Err("script stack overflow (too many embedded function calls)".into());
    }
    Ok(())
}

fn run_inline(
    world: &mut World,
    program: &Program,
    parent: &mut Thread,
    mut child: Thread,
    now: i64,
) -> Result<(), String> {
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.suspended.push(parent.serial);
    runtime.suspended_frames += parent.frames.len();
    child.state = ThreadState::Runnable;
    execute(world, program, &mut child, now);
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.suspended.pop();
    runtime.suspended_frames -= parent.frames.len();
    if child.state == ThreadState::Complete {
        retire(&mut runtime, child.serial);
    } else {
        world.spawn(child);
    }
    parent.stack.push(Value::Undefined);
    let mut runtime = world.resource_mut::<Runtime>();
    if runtime.fault.is_some() {
        return Err(String::new());
    }
    let depth = runtime
        .pending_unwinds
        .iter()
        .filter(|(serial, _)| *serial == parent.serial)
        .map(|(_, depth)| *depth)
        .min();
    runtime
        .pending_unwinds
        .retain(|(serial, _)| *serial != parent.serial);
    if let Some(depth) = depth.filter(|depth| *depth < parent.frames.len()) {
        unwind(world, parent, depth, now, true);
    }
    Ok(())
}

fn pop(thread: &mut Thread) -> Result<Value, String> {
    thread
        .stack
        .pop()
        .ok_or_else(|| "invalid IR: stack underflow".into())
}

pub(super) fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Undefined => "undefined",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::LocalizedString(_) => "localized string",
        Value::Vector(_) => "vector",
        Value::Entity(_) => "entity",
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::Function(_) => "function",
        Value::Builtin(_) => "builtin function",
        Value::Animation { .. } => "animation",
        Value::AnimationTree(_) => "animtree",
    }
}

fn mismatch(a: &Value, b: &Value) -> String {
    format!(
        "pair '{}' and '{}' has unmatching types",
        type_name(a),
        type_name(b)
    )
}

fn truth(value: &Value) -> Result<bool, String> {
    match value {
        Value::Int(n) => Ok(*n != 0),
        Value::Float(n) => Ok(*n != 0.0),
        _ => Err(format!("cast from {} to bool", type_name(value))),
    }
}

fn scalar(value: &Value) -> Option<f32> {
    match value {
        Value::Int(n) => Some(*n as f32),
        Value::Float(n) => Some(*n),
        _ => None,
    }
}

fn weaker(a: Value, b: Value) -> (Value, Value) {
    match (&a, &b) {
        (Value::Int(x), Value::Float(_)) => (Value::Float(*x as f32), b),
        (Value::Float(_), Value::Int(y)) => (a, Value::Float(*y as f32)),
        (Value::Vector(_), _) if scalar(&b).is_some() => {
            let y = scalar(&b).unwrap();
            (a, Value::Vector([y; 3]))
        }
        (_, Value::Vector(_)) if scalar(&a).is_some() => {
            let x = scalar(&a).unwrap();
            (Value::Vector([x; 3]), b)
        }
        _ => (a, b),
    }
}

fn format_g(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let trim = |text: &str| -> String {
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            text.to_owned()
        }
    };
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    if !(-4..6).contains(&exponent) {
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{}e{sign}{:03}", trim(mantissa), exponent.abs())
    } else {
        trim(&format!("{value:.*}", (5 - exponent) as usize))
    }
}

pub(super) fn to_text(value: &Value) -> Option<String> {
    match value {
        Value::Int(n) => Some(n.to_string()),
        Value::Float(n) => Some(format_g(f64::from(*n))),
        Value::Vector(v) => Some(format!(
            "({}, {}, {})",
            format_g(f64::from(v[0])),
            format_g(f64::from(v[1])),
            format_g(f64::from(v[2]))
        )),
        _ => None,
    }
}

fn equality(a: Value, b: Value) -> Result<bool, String> {
    let (a, b) = weaker(a, b);
    Ok(match (&a, &b) {
        (Value::Array(_), _) | (_, Value::Array(_)) => {
            return Err("cannot compare arrays".into());
        }
        (Value::Undefined, Value::Undefined) => true,
        (Value::Entity(_) | Value::Object(_), Value::Entity(_) | Value::Object(_)) => a == b,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::LocalizedString(x), Value::LocalizedString(y)) => x == y,
        (Value::Vector(x), Value::Vector(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => (x - y).abs() < 1e-6,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Function(x), Value::Function(y)) => x == y,
        (Value::Builtin(x), Value::Builtin(y)) => x == y,
        (Value::Animation { .. }, Value::Animation { .. })
        | (Value::AnimationTree(_), Value::AnimationTree(_)) => a == b,
        _ => return Err(mismatch(&a, &b)),
    })
}

pub(super) fn unary(op: Unary, value: Value) -> Result<Value, String> {
    match (op, &value) {
        (Unary::Not, _) => Ok(Value::Int(i32::from(!truth(&value)?))),
        (Unary::Complement, Value::Int(n)) => Ok(Value::Int(!n)),
        (Unary::Complement, _) => Err(format!("~ cannot be applied to {}", type_name(&value))),
    }
}

pub(super) fn binary(op: Binary, a: Value, b: Value) -> Result<Value, String> {
    let integers = |a: &Value, b: &Value| match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok((*x, *y)),
        _ => Err(format!(
            "{op:?} requires int operands, found {} and {}",
            type_name(a),
            type_name(b)
        )),
    };
    let result = match op {
        Binary::Or => integers(&a, &b).map(|(x, y)| Value::Int(x | y))?,
        Binary::Xor => integers(&a, &b).map(|(x, y)| Value::Int(x ^ y))?,
        Binary::And => integers(&a, &b).map(|(x, y)| Value::Int(x & y))?,
        Binary::Shl => {
            integers(&a, &b).map(|(x, y)| Value::Int(x.wrapping_shl((y & 31) as u32)))?
        }
        Binary::Shr => {
            integers(&a, &b).map(|(x, y)| Value::Int(x.wrapping_shr((y & 31) as u32)))?
        }
        Binary::Mod => {
            let (x, y) = integers(&a, &b)?;
            if y == 0 {
                return Err("divide by 0".into());
            }
            Value::Int(x.wrapping_rem(y))
        }
        Binary::Equal => Value::Int(i32::from(equality(a, b)?)),
        Binary::NotEqual => Value::Int(i32::from(!equality(a, b)?)),
        Binary::Case => match (&a, &b) {
            (Value::Int(_) | Value::String(_), _) => Value::Int(i32::from(a == b)),
            _ => return Err(format!("cannot switch on {}", type_name(&a))),
        },
        Binary::Add => {
            let (a, b) = match (&a, &b) {
                (Value::String(_), _) | (_, Value::String(_)) => {
                    let text = |v: &Value| match v {
                        Value::String(s) => Some(s.to_string()),
                        other => to_text(other),
                    };
                    match (text(&a), text(&b)) {
                        (Some(x), Some(y)) => {
                            if x.len() + y.len() > 0x2000 {
                                return Err("string too long".into());
                            }
                            return Ok(Value::String(format!("{x}{y}").into()));
                        }
                        _ => return Err(mismatch(&a, &b)),
                    }
                }
                (Value::Int(x), Value::Float(_)) => (Value::Float(*x as f32), b),
                (Value::Float(_), Value::Int(y)) => (a, Value::Float(*y as f32)),
                _ => (a, b),
            };
            match (&a, &b) {
                (Value::Int(x), Value::Int(y)) => Value::Int(x.wrapping_add(*y)),
                (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
                (Value::Vector(x), Value::Vector(y)) => {
                    Value::Vector([x[0] + y[0], x[1] + y[1], x[2] + y[2]])
                }
                _ => return Err(mismatch(&a, &b)),
            }
        }
        Binary::Sub | Binary::Mul | Binary::Div => {
            let (a, b) = weaker(a, b);
            match (&a, &b, op) {
                (Value::Int(x), Value::Int(y), Binary::Sub) => Value::Int(x.wrapping_sub(*y)),
                (Value::Int(x), Value::Int(y), Binary::Mul) => Value::Int(x.wrapping_mul(*y)),
                (Value::Int(_), Value::Int(0), _) | (_, Value::Float(0.0), Binary::Div) => {
                    return Err("divide by 0".into());
                }
                (Value::Int(x), Value::Int(y), _) => Value::Float(*x as f32 / *y as f32),
                (Value::Float(x), Value::Float(y), Binary::Sub) => Value::Float(x - y),
                (Value::Float(x), Value::Float(y), Binary::Mul) => Value::Float(x * y),
                (Value::Float(x), Value::Float(y), _) => Value::Float(x / y),
                (Value::Vector(x), Value::Vector(y), Binary::Sub) => {
                    Value::Vector([x[0] - y[0], x[1] - y[1], x[2] - y[2]])
                }
                (Value::Vector(x), Value::Vector(y), Binary::Mul) => {
                    Value::Vector([x[0] * y[0], x[1] * y[1], x[2] * y[2]])
                }
                (Value::Vector(_), Value::Vector(y), _) if y.contains(&0.0) => {
                    return Err("divide by 0".into());
                }
                (Value::Vector(x), Value::Vector(y), _) => {
                    Value::Vector([x[0] / y[0], x[1] / y[1], x[2] / y[2]])
                }
                _ => return Err(mismatch(&a, &b)),
            }
        }
        Binary::Less | Binary::Greater | Binary::LessEqual | Binary::GreaterEqual => {
            let (a, b) = weaker(a, b);
            let ordered = match (&a, &b, op) {
                (Value::Int(x), Value::Int(y), Binary::Less | Binary::GreaterEqual) => x < y,
                (Value::Int(x), Value::Int(y), _) => x > y,
                (Value::Float(x), Value::Float(y), Binary::Less | Binary::GreaterEqual) => x < y,
                (Value::Float(x), Value::Float(y), _) => x > y,
                _ => return Err(mismatch(&a, &b)),
            };
            let inverted = matches!(op, Binary::GreaterEqual | Binary::LessEqual);
            Value::Int(i32::from(ordered != inverted))
        }
    };
    match &result {
        Value::Float(n) if !n.is_finite() => Err("non-finite arithmetic result".into()),
        Value::Vector(v) if !v.iter().all(|n| n.is_finite()) => {
            Err("non-finite arithmetic result".into())
        }
        _ => Ok(result),
    }
}

fn object_key(runtime: &mut Runtime, key: ArrayKey) -> Result<u32, String> {
    match key {
        ArrayKey::String(key) => Ok(runtime.symbol(&key)),
        ArrayKey::Integer(_) => Err("object index must be a string".into()),
    }
}

fn instruction(
    world: &mut World,
    program: &Program,
    thread: &mut Thread,
    op: Op,
    now: i64,
) -> Result<(), String> {
    let spawn = matches!(op, Op::Spawn(..) | Op::Indirect(_, _, true));
    let method = matches!(
        op,
        Op::Call(_, _, true) | Op::Spawn(_, _, true) | Op::Indirect(_, true, _)
    );
    match op {
        Op::Constant(value) => thread.stack.push(value),
        Op::FunctionRef(_) => return Err("invalid IR: unlinked function reference".into()),
        Op::Global(global) => thread.stack.push(match global {
            Global::SelfRef => thread.frames.last().unwrap().receiver.clone(),
            Global::Level => Value::Object(0),
            Global::Game => Value::Object(1),
            Global::Anim => Value::Object(2),
        }),
        Op::Load(slot) => {
            let value = thread.frames.last().unwrap().locals[slot as usize].clone();
            thread.stack.push(value);
        }
        Op::Store(slot) => {
            let value = copy_value(world, pop(thread)?)?;
            thread.frames.last_mut().unwrap().locals[slot as usize] = value;
        }
        Op::Dup => {
            let value = thread
                .stack
                .last()
                .cloned()
                .ok_or("invalid IR: stack underflow")?;
            thread.stack.push(value);
        }
        Op::DupPair => {
            let base = thread
                .stack
                .len()
                .checked_sub(2)
                .ok_or("invalid IR: stack underflow")?;
            let pair = thread.stack[base..].to_vec();
            thread.stack.extend(pair);
        }
        Op::ArrayKeys => {
            let Value::Array(id) = pop(thread)? else {
                return Err("foreach requires an array".into());
            };
            let keys: Vec<_> = world
                .resource::<Runtime>()
                .arrays
                .get(&id)
                .ok_or("invalid array reference")?
                .keys()
                .map(|key| match key {
                    ArrayKey::Integer(i) => Value::Int(*i),
                    ArrayKey::String(s) => Value::String(s.clone()),
                })
                .collect();
            let value = allocate_array(world)?;
            let Value::Array(id) = value else {
                unreachable!()
            };
            let mut runtime = world.resource_mut::<Runtime>();
            let array = runtime.arrays.get_mut(&id).unwrap();
            for (i, key) in keys.into_iter().enumerate() {
                array.insert(ArrayKey::Integer(i as i32), key);
            }
            thread.stack.push(value);
        }
        Op::EnsureLocalArray(slot) => {
            if thread.frames.last().unwrap().locals[slot as usize] == Value::Undefined {
                let value = allocate_array(world)?;
                thread.frames.last_mut().unwrap().locals[slot as usize] = value;
            }
        }
        Op::EnsureFieldArray(field) => {
            let receiver = pop(thread)?;
            let Value::Object(id) = receiver else {
                return Err("array field requires an object".into());
            };
            let value = world
                .resource::<Runtime>()
                .objects
                .get(&id)
                .ok_or("invalid object reference")?
                .get(&field)
                .cloned()
                .unwrap_or(Value::Undefined);
            let value = if value == Value::Undefined {
                let value = allocate_array(world)?;
                world
                    .resource_mut::<Runtime>()
                    .objects
                    .get_mut(&id)
                    .unwrap()
                    .insert(field, value.clone());
                value
            } else {
                value
            };
            thread.stack.push(value);
        }
        Op::EnsureIndexArray => {
            let key = array_key(pop(thread)?)?;
            let receiver = pop(thread)?;
            let mut runtime = world.resource_mut::<Runtime>();
            let (value, field) = match &receiver {
                Value::Array(id) => (
                    runtime
                        .arrays
                        .get(id)
                        .ok_or("invalid array reference")?
                        .get(&key)
                        .cloned(),
                    None,
                ),
                Value::Object(id) => {
                    let field = object_key(&mut runtime, key.clone())?;
                    (
                        runtime
                            .objects
                            .get(id)
                            .ok_or("invalid object reference")?
                            .get(&field)
                            .cloned(),
                        Some(field),
                    )
                }
                _ => return Err("index assignment requires array or object".into()),
            };
            let value = match value {
                Some(value) if value != Value::Undefined => value,
                _ => {
                    let value = allocate_array(world)?;
                    let mut runtime = world.resource_mut::<Runtime>();
                    match receiver {
                        Value::Array(id) => {
                            runtime
                                .arrays
                                .get_mut(&id)
                                .unwrap()
                                .insert(key, value.clone());
                        }
                        Value::Object(id) => {
                            runtime
                                .objects
                                .get_mut(&id)
                                .unwrap()
                                .insert(field.unwrap(), value.clone());
                        }
                        _ => unreachable!(),
                    }
                    value
                }
            };
            thread.stack.push(value);
        }
        Op::Array => {
            thread.stack.push(allocate_array(world)?);
        }
        Op::LoadIndex => {
            let key = array_key(pop(thread)?)?;
            let receiver = pop(thread)?;
            let mut runtime = world.resource_mut::<Runtime>();
            let value = match receiver {
                Value::Array(id) => runtime
                    .arrays
                    .get(&id)
                    .ok_or("invalid array reference")?
                    .get(&key)
                    .cloned()
                    .unwrap_or(Value::Undefined),
                Value::Object(id) => {
                    let field = object_key(&mut runtime, key)?;
                    runtime
                        .objects
                        .get(&id)
                        .ok_or("invalid object reference")?
                        .get(&field)
                        .cloned()
                        .unwrap_or(Value::Undefined)
                }
                Value::Vector(v) => {
                    let ArrayKey::Integer(i) = key else {
                        return Err("vector index must be an integer".into());
                    };
                    Value::Float(*v.get(i as usize).ok_or("vector index out of range")?)
                }
                Value::String(s) => {
                    let ArrayKey::Integer(i) = key else {
                        return Err("string index must be an integer".into());
                    };
                    s.chars()
                        .nth(i as usize)
                        .map(|c| Value::String(c.to_string().into()))
                        .unwrap_or(Value::Undefined)
                }
                _ => return Err("value cannot be indexed".into()),
            };
            thread.stack.push(value);
        }
        Op::StoreIndex => {
            let value = copy_value(world, pop(thread)?)?;
            let key = array_key(pop(thread)?)?;
            let receiver = pop(thread)?;
            let mut runtime = world.resource_mut::<Runtime>();
            match receiver {
                Value::Array(id) => {
                    let values = runtime
                        .arrays
                        .get_mut(&id)
                        .ok_or("invalid array reference")?;
                    if value == Value::Undefined {
                        values.remove(&key);
                    } else {
                        values.insert(key, value);
                    }
                }
                Value::Object(id) => {
                    let field = object_key(&mut runtime, key)?;
                    let fields = runtime
                        .objects
                        .get_mut(&id)
                        .ok_or("invalid object reference")?;
                    if value == Value::Undefined {
                        fields.remove(&field);
                    } else {
                        fields.insert(field, value);
                    }
                }
                _ => return Err("index assignment requires array or object".into()),
            }
        }
        Op::Pop => {
            pop(thread)?;
        }
        Op::Unary(op) => {
            let value = pop(thread)?;
            thread.stack.push(unary(op, value)?);
        }
        Op::Binary(op) => {
            let b = pop(thread)?;
            let a = pop(thread)?;
            thread.stack.push(binary(op, a, b)?);
        }
        Op::Vector => {
            let mut v = [0.0; 3];
            for component in v.iter_mut().rev() {
                let value = pop(thread)?;
                *component = scalar(&value)
                    .ok_or_else(|| format!("vector component cannot be {}", type_name(&value)))?;
            }
            thread.stack.push(Value::Vector(v));
        }
        Op::Jump(pc) => thread.frames.last_mut().unwrap().pc = pc,
        Op::JumpFalse(pc) => {
            if !truth(&pop(thread)?)? {
                thread.frames.last_mut().unwrap().pc = pc;
            }
        }
        Op::Call(_, argc, _) | Op::Spawn(_, argc, _) | Op::Indirect(argc, _, _) => {
            let base = thread
                .stack
                .len()
                .checked_sub(argc)
                .ok_or("invalid IR: argument underflow")?;
            let args = thread
                .stack
                .split_off(base)
                .into_iter()
                .map(|value| copy_value(world, value))
                .collect::<Result<Vec<_>, _>>()?;
            let callee = match op {
                Op::Call(callee, _, _) | Op::Spawn(callee, _, _) => callee,
                Op::Indirect(..) => match pop(thread)? {
                    Value::Function(id) => Callee::Script(id),
                    Value::Builtin(id) => Callee::Native(id),
                    other => {
                        return Err(format!(
                            "indirect call requires a function reference, found {}",
                            type_name(&other)
                        ));
                    }
                },
                _ => unreachable!(),
            };
            let receiver = if method {
                pop(thread)?
            } else {
                thread.frames.last().unwrap().receiver.clone()
            };
            match callee {
                Callee::Script(function) if spawn => {
                    frame_room(world, thread)?;
                    let child = new_thread(world, program, function as usize, receiver, args)?;
                    run_inline(world, program, thread, child, now)?;
                }
                Callee::Script(function) => {
                    frame_room(world, thread)?;
                    thread.frames.push(frame(
                        program,
                        function as usize,
                        args,
                        thread.stack.len(),
                        receiver,
                    ));
                }
                Callee::Native(_) if spawn => {
                    return Err("threading native calls is unsupported".into());
                }
                Callee::Native(index) => {
                    let native = world.resource::<Runtime>().natives[index as usize];
                    let value = native(world, &receiver, &args)
                        .map_err(|m| format!("{}: {m}", program.natives[index as usize].name))?;
                    thread.stack.push(value);
                    deliver_pending(world, thread, now)?;
                }
                Callee::Unlinked(_) => return Err("invalid IR: unlinked call".into()),
            }
        }
        Op::FrameEnd => {
            thread.state = ThreadState::Queued;
            world
                .resource_mut::<Runtime>()
                .buckets
                .entry(now)
                .or_default()
                .push_back(thread.serial);
        }
        Op::Wait => {
            let value = pop(thread)?;
            let frames = match value {
                Value::Int(n) => n.wrapping_mul(20),
                Value::Float(n) if n < 0.0 => return Err("negative wait is not allowed".into()),
                Value::Float(n) => match (n * 20.0 + 0.5).floor() {
                    0.0 if n != 0.0 => 1,
                    f if f < 2_147_483_648.0 => f as i32,
                    _ => i32::MIN,
                },
                other => return Err(format!("type {} is not a float", type_name(&other))),
            };
            if frames as u32 >= 0xff_ffff {
                return Err(if frames < 0 {
                    "negative wait is not allowed".into()
                } else {
                    "wait is too long".into()
                });
            }
            let until = now + i64::from(frames) * i64::from(crate::MATCH_TICK_MS);
            thread.state = ThreadState::Queued;
            world
                .resource_mut::<Runtime>()
                .buckets
                .entry(until)
                .or_default()
                .push_front(thread.serial);
        }
        Op::Size => {
            let receiver = pop(thread)?;
            let size = match &receiver {
                Value::Array(id) => world
                    .resource::<Runtime>()
                    .arrays
                    .get(id)
                    .ok_or("invalid array reference")?
                    .len(),
                Value::Object(_) | Value::Entity(_) => 1,
                Value::String(s) => s.chars().count(),
                other => return Err(format!("size cannot be applied to {}", type_name(other))),
            };
            thread.stack.push(Value::Int(size as i32));
        }
        Op::LoadField(field) => {
            let receiver = pop(thread)?;
            if let Value::Entity(entity) = receiver {
                let frame = crate::frame::FrameWorld::from_world(world);
                frame
                    .entity_kernel()
                    .resolve(entity)
                    .map_err(|e| format!("invalid entity receiver: {e:?}"))?;
                let name = &program.symbols[field as usize];
                if &**name != "origin" {
                    return Err(format!("unsupported native entity field {name}"));
                }
                let mover = frame
                    .script_mover_by_number(entity.number())
                    .ok_or("origin receiver is not a script mover")?;
                thread.stack.push(Value::Vector(mover.state.tr_base));
                return Ok(());
            }
            let Value::Object(id) = receiver else {
                return Err(format!(
                    "field receiver must be an object or entity, found {}",
                    type_name(&receiver)
                ));
            };
            if let Some(client) = world.resource::<Runtime>().player_client(id)
                && let Some(value) =
                    super::players::load_field(world, client, &program.symbols[field as usize])
            {
                thread.stack.push(value);
                return Ok(());
            }
            let runtime = world.resource::<Runtime>();
            let fields = runtime
                .objects
                .get(&id)
                .ok_or("invalid script object reference")?;
            thread
                .stack
                .push(fields.get(&field).cloned().unwrap_or(Value::Undefined));
        }
        Op::StoreField(field) => {
            let value = copy_value(world, pop(thread)?)?;
            let receiver = pop(thread)?;
            let Value::Object(id) = receiver else {
                return Err("native entity fields are not bound".into());
            };
            if let Some(client) = world.resource::<Runtime>().player_client(id)
                && super::players::store_field(
                    world,
                    client,
                    &program.symbols[field as usize],
                    &value,
                )?
            {
                return Ok(());
            }
            super::hud::store_field(world, id, &program.symbols[field as usize], &value)?;
            let mut runtime = world.resource_mut::<Runtime>();
            let fields = runtime
                .objects
                .get_mut(&id)
                .ok_or("invalid script object reference")?;
            if value == Value::Undefined {
                fields.remove(&field);
            } else {
                fields.insert(field, value);
            }
        }
        Op::Notify(argc) => {
            let base = thread
                .stack
                .len()
                .checked_sub(argc)
                .ok_or("invalid IR: event argument underflow")?;
            let arguments = thread.stack.split_off(base);
            let name = event_name(pop(thread)?)?;
            let receiver = event_receiver(pop(thread)?)?;
            notify(world, thread, &receiver, &name, &arguments, now)?;
        }
        Op::Await(outputs) => {
            let name = event_name(pop(thread)?)?;
            let receiver = event_receiver(pop(thread)?)?;
            register(
                world,
                thread,
                receiver,
                name,
                WaiterKind::Waittill { outputs },
            );
        }
        Op::AwaitMatch(argc) => {
            let base = thread
                .stack
                .len()
                .checked_sub(argc)
                .ok_or("invalid IR: event argument underflow")?;
            let values = thread.stack.split_off(base);
            let name = event_name(pop(thread)?)?;
            let receiver = event_receiver(pop(thread)?)?;
            register(world, thread, receiver, name, WaiterKind::Match { values });
        }
        Op::Endon => {
            let name = event_name(pop(thread)?)?;
            let receiver = event_receiver(pop(thread)?)?;
            let frame = thread.frames.len() - 1;
            world.resource_mut::<Runtime>().waiters.push(Waiter {
                receiver,
                name,
                thread: thread.serial,
                kind: WaiterKind::Endon { frame },
            });
        }
        Op::Return => {
            let value = pop(thread)?;
            let depth = thread.frames.len() - 1;
            let serial = thread.serial;
            world.resource_mut::<Runtime>().waiters.retain(|w| {
                w.thread != serial
                    || !matches!(w.kind, WaiterKind::Endon { frame } if frame >= depth)
            });
            let frame = thread
                .frames
                .pop()
                .ok_or("invalid IR: return without frame")?;
            thread.stack.truncate(frame.stack_base);
            if thread.frames.is_empty() {
                thread.state = ThreadState::Complete;
            } else {
                thread.stack.push(value);
            }
        }
    }
    Ok(())
}

fn event_name(value: Value) -> Result<Arc<str>, String> {
    match value {
        Value::String(name) => Ok(name),
        other => Err(format!(
            "event name must be a string, found {}",
            type_name(&other)
        )),
    }
}
fn event_receiver(value: Value) -> Result<Value, String> {
    match value {
        Value::Object(_) | Value::Entity(_) => Ok(value),
        other => Err(format!("{} is not an object", type_name(&other))),
    }
}

fn register(
    world: &mut World,
    thread: &mut Thread,
    receiver: Value,
    name: Arc<str>,
    kind: WaiterKind,
) {
    thread.state = ThreadState::Awaiting;
    world.resource_mut::<Runtime>().waiters.push(Waiter {
        receiver,
        name,
        thread: thread.serial,
        kind,
    });
}

fn find_thread(world: &mut World, serial: u64) -> Option<Entity> {
    world
        .query::<(Entity, &Thread)>()
        .iter(world)
        .find(|(_, t)| t.serial == serial)
        .map(|(e, _)| e)
}

fn with_thread<R>(
    world: &mut World,
    current: &mut Thread,
    serial: u64,
    f: impl FnOnce(&mut World, &mut Thread, bool) -> R,
) -> Option<R> {
    if serial == current.serial {
        return Some(f(world, current, true));
    }
    let entity = find_thread(world, serial)?;
    let mut thread = world.entity_mut(entity).take::<Thread>().unwrap();
    let result = f(world, &mut thread, false);
    if thread.state == ThreadState::Complete {
        world.despawn(entity);
    } else {
        world.entity_mut(entity).insert(thread);
    }
    Some(result)
}

fn retire(runtime: &mut Runtime, serial: u64) {
    runtime.waiters.retain(|w| w.thread != serial);
    dequeue(runtime, serial);
}

fn dequeue(runtime: &mut Runtime, serial: u64) {
    for bucket in runtime.buckets.values_mut() {
        bucket.retain(|s| *s != serial);
    }
    runtime.spawned.retain(|s| *s != serial);
}

fn resume_now(world: &mut World, thread: &mut Thread, now: i64) {
    thread.state = ThreadState::Queued;
    world
        .resource_mut::<Runtime>()
        .buckets
        .entry(now)
        .or_default()
        .push_front(thread.serial);
}

fn unwind(world: &mut World, thread: &mut Thread, depth: usize, now: i64, running: bool) {
    let serial = thread.serial;
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.waiters.retain(|w| {
        w.thread != serial || matches!(w.kind, WaiterKind::Endon { frame } if frame < depth)
    });
    if !running {
        dequeue(&mut runtime, serial);
    }
    let base = thread.frames[depth].stack_base;
    thread.frames.truncate(depth);
    thread.stack.truncate(base);
    if depth == 0 {
        thread.state = ThreadState::Complete;
        return;
    }
    thread.stack.push(Value::Undefined);
    if running {
        thread.state = ThreadState::Runnable;
    } else {
        resume_now(world, thread, now);
    }
}

fn payload_matches(values: &[Value], arguments: &[Value]) -> bool {
    values.len() <= arguments.len()
        && values
            .iter()
            .zip(arguments)
            .all(|(v, a)| equality(v.clone(), a.clone()) == Ok(true))
}

fn notify(
    world: &mut World,
    current: &mut Thread,
    receiver: &Value,
    name: &Arc<str>,
    arguments: &[Value],
    now: i64,
) -> Result<(), String> {
    if *receiver == Value::Object(0) {
        world.resource_mut::<Runtime>().signals.push(name.clone());
    }
    loop {
        let runtime = world.resource::<Runtime>();
        let Some(index) = runtime.waiters.iter().position(|w| {
            &w.receiver == receiver
                && &w.name == name
                && match &w.kind {
                    WaiterKind::Match { values } => payload_matches(values, arguments),
                    _ => true,
                }
        }) else {
            return Ok(());
        };
        let waiter = world.resource_mut::<Runtime>().waiters.remove(index);
        let result = match waiter.kind {
            WaiterKind::Endon { frame } => {
                let mut runtime = world.resource_mut::<Runtime>();
                if runtime.suspended.contains(&waiter.thread) {
                    runtime.pending_unwinds.push((waiter.thread, frame));
                    runtime.waiters.retain(|w| {
                        w.thread != waiter.thread
                            || matches!(w.kind, WaiterKind::Endon { frame: f } if f < frame)
                    });
                    continue;
                }
                with_thread(world, current, waiter.thread, |world, thread, running| {
                    unwind(world, thread, frame, now, running);
                    Ok::<_, String>(())
                })
            }
            WaiterKind::Waittill { outputs } => {
                with_thread(world, current, waiter.thread, |world, thread, _| {
                    for (i, slot) in outputs.iter().enumerate() {
                        let value = match arguments.get(i) {
                            Some(value) => copy_value(world, value.clone())?,
                            None => Value::Undefined,
                        };
                        thread.frames.last_mut().unwrap().locals[*slot as usize] = value;
                    }
                    resume_now(world, thread, now);
                    Ok(())
                })
            }
            WaiterKind::Match { .. } => {
                with_thread(world, current, waiter.thread, |world, thread, _| {
                    resume_now(world, thread, now);
                    Ok(())
                })
            }
        };
        result.unwrap_or(Ok(()))?;
    }
}

fn kill(world: &mut World, entity: Entity, serial: u64) {
    world.despawn(entity);
    retire(&mut world.resource_mut::<Runtime>(), serial);
}

pub(crate) fn advance_scheduler(world: &mut World) {
    let request = world.resource::<crate::step::StepRequest>();
    if !request.reason.advances_authority_world() {
        return;
    }
    let tick = request.tick;
    let runtime = world.resource::<Runtime>();
    if runtime.fault.is_some() {
        return;
    }
    let Some(program) = runtime.program.clone() else {
        return;
    };
    if runtime
        .last_tick
        .is_some_and(|previous| previous.0 >= tick.0)
    {
        world.resource_mut::<Runtime>().fault = Some(Fault::at(
            &Location {
                module: "<scheduler>".into(),
                function: String::new(),
                line: 0,
                column: 0,
            },
            "authority script ticks must increase; restore the runtime before replaying",
        ));
        return;
    }
    world.resource_mut::<Runtime>().last_tick = Some(tick);
    let now = i64::from(tick.0) * i64::from(crate::MATCH_TICK_MS);
    super::natives_engine::advance_motions(world, now);
    deliver_external(world, now);
    let threads: Vec<_> = world
        .query::<(Entity, &Thread)>()
        .iter(world)
        .map(|(entity, thread)| (entity, thread.serial, entity_receivers(world, thread)))
        .collect();
    let dead: Vec<_> = threads
        .into_iter()
        .filter(|(_, _, receivers)| any_deleted(world, receivers))
        .map(|(entity, serial, _)| (entity, serial))
        .collect();
    for (entity, serial) in dead {
        kill(world, entity, serial);
    }
    {
        let mut runtime = world.resource_mut::<Runtime>();
        let due: Vec<_> = runtime.buckets.range(..=now).map(|(k, _)| *k).collect();
        let mut current = VecDeque::new();
        for key in due {
            current.extend(runtime.buckets.remove(&key).unwrap());
        }
        for serial in std::mem::take(&mut runtime.spawned).into_iter().rev() {
            current.push_front(serial);
        }
        runtime.buckets.insert(now, current);
    }
    world.resource_mut::<Runtime>().budget = INSTRUCTION_BUDGET;
    loop {
        let next = world
            .resource_mut::<Runtime>()
            .buckets
            .get_mut(&now)
            .and_then(VecDeque::pop_front);
        let Some(serial) = next else {
            break;
        };
        let Some(entity) = find_thread(world, serial) else {
            continue;
        };
        let receivers = entity_receivers(world, world.get::<Thread>(entity).unwrap());
        if any_deleted(world, &receivers) {
            kill(world, entity, serial);
            continue;
        }
        let mut thread = world.entity_mut(entity).take::<Thread>().unwrap();
        thread.state = ThreadState::Runnable;
        execute(world, &program, &mut thread, now);
        if thread.state == ThreadState::Complete {
            kill(world, entity, serial);
        } else {
            world.entity_mut(entity).insert(thread);
        }
        if world.resource::<Runtime>().fault.is_some() {
            break;
        }
    }
    let deletes = std::mem::take(&mut world.resource_mut::<Runtime>().pending_deletes);
    for object in deletes {
        world.resource_mut::<Runtime>().delete_entity(object);
    }
    let mut runtime = world.resource_mut::<Runtime>();
    if runtime.buckets.get(&now).is_some_and(VecDeque::is_empty) {
        runtime.buckets.remove(&now);
    }
    runtime.loading = false;
    collect_heap(world);
}

fn execute(world: &mut World, program: &Program, thread: &mut Thread, now: i64) {
    while thread.state == ThreadState::Runnable {
        let frame = thread.frames.last().unwrap();
        let function = &program.functions[frame.function];
        let Some((location, op)) = function.code.get(frame.pc).cloned() else {
            world.resource_mut::<Runtime>().fault = Some(Fault::at(
                &function.location,
                "invalid IR: instruction position out of range",
            ));
            break;
        };
        let result = if world.resource::<Runtime>().budget == 0 {
            Err("script instruction budget exhausted".into())
        } else {
            world.resource_mut::<Runtime>().budget -= 1;
            thread.frames.last_mut().unwrap().pc += 1;
            instruction(world, program, thread, op, now)
        };
        if let Err(message) = result {
            if world.resource::<Runtime>().fault.is_some() {
                break;
            }
            let mut fault = Fault::at(&location, message);
            fault.callers = thread
                .frames
                .iter()
                .rev()
                .skip(1)
                .rev()
                .map(|f| {
                    program.functions[f.function].code[f.pc.saturating_sub(1)]
                        .0
                        .clone()
                })
                .collect();
            world.resource_mut::<Runtime>().fault = Some(fault);
            break;
        }
    }
}

pub(crate) fn healthy(runtime: bevy_ecs::prelude::Res<Runtime>) -> bool {
    runtime.fault.is_none()
}

pub(crate) fn preflight(
    world: &World,
    tick: crate::Tick,
    reason: crate::StepReason,
) -> Result<(), Fault> {
    let runtime = world.resource::<Runtime>();
    if let Some(fault) = &runtime.fault {
        return Err(fault.clone());
    }
    if reason.advances_authority_world() && runtime.last_tick.is_some_and(|last| last.0 >= tick.0) {
        return Err(Fault::at(
            &Location {
                module: "<scheduler>".into(),
                function: String::new(),
                line: 0,
                column: 0,
            },
            "authority script ticks must increase; restore the runtime before replaying",
        ));
    }
    if reason.advances_authority_world() && (!runtime.started || runtime.program.is_none()) {
        return Err(Fault::at(
            &Location {
                module: "<runtime>".into(),
                function: "step".into(),
                line: 0,
                column: 0,
            },
            "no loaded and started GSC program; handwritten gameplay dispatch has been removed",
        ));
    }
    Ok(())
}

fn array_key(value: Value) -> Result<ArrayKey, String> {
    match value {
        Value::Int(n) if n >= 0 => Ok(ArrayKey::Integer(n)),
        Value::String(s) => Ok(ArrayKey::String(s)),
        other => Err(format!(
            "array index must be a nonnegative int or string, found {}",
            type_name(&other)
        )),
    }
}

fn allocate_array(world: &mut World) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<Runtime>();
    let id = runtime.next_object;
    runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
    runtime.arrays.insert(id, BTreeMap::new());
    Ok(Value::Array(id))
}

fn copy_value(world: &mut World, value: Value) -> Result<Value, String> {
    fn copy(
        world: &mut World,
        value: Value,
        depth: usize,
        remaining: &mut usize,
    ) -> Result<Value, String> {
        let Value::Array(id) = value else {
            return Ok(value);
        };
        if depth >= 256 || *remaining == 0 {
            return Err("array copy budget exceeded".into());
        }
        *remaining -= 1;
        let entries = world
            .resource::<Runtime>()
            .arrays
            .get(&id)
            .ok_or("invalid array reference")?
            .clone();
        if entries.len() > *remaining {
            return Err("array copy budget exceeded".into());
        }
        *remaining -= entries.len();
        let result = allocate_array(world)?;
        let Value::Array(new_id) = result else {
            unreachable!()
        };
        for (key, value) in entries {
            let value = copy(world, value, depth + 1, remaining)?;
            world
                .resource_mut::<Runtime>()
                .arrays
                .get_mut(&new_id)
                .unwrap()
                .insert(key, value);
        }
        Ok(result)
    }
    copy(world, value, 0, &mut 100_000)
}

/// Deleting a waited-on object ends the thread; a thread whose self is deleted keeps running.
fn entity_receivers(world: &World, thread: &Thread) -> Vec<Value> {
    world
        .resource::<Runtime>()
        .waiters
        .iter()
        .filter(|w| w.thread == thread.serial)
        .map(|w| &w.receiver)
        .filter(|value| matches!(value, Value::Entity(_) | Value::Object(_)))
        .cloned()
        .collect()
}

/// A receiver is gone once its kernel entity is freed or its script entity deleted.
fn any_deleted(world: &mut World, receivers: &[Value]) -> bool {
    let objects = &world.resource::<Runtime>().objects;
    if receivers
        .iter()
        .any(|value| matches!(value, Value::Object(id) if !objects.contains_key(id)))
    {
        return true;
    }
    let frame = crate::frame::FrameWorld::from_world(world);
    receivers.iter().any(|value| match value {
        Value::Entity(entity) => frame.entity_kernel().resolve(*entity).is_err(),
        _ => false,
    })
}

fn collect_heap(world: &mut World) {
    let mut pending = vec![Value::Object(0), Value::Object(1), Value::Object(2)];
    pending.extend(
        world
            .resource::<Runtime>()
            .entities
            .keys()
            .map(|id| Value::Object(*id)),
    );
    for thread in world.query::<&Thread>().iter(world) {
        pending.extend(thread.stack.iter().cloned());
        for frame in &thread.frames {
            pending.push(frame.receiver.clone());
            pending.extend(frame.locals.iter().cloned());
        }
    }
    let mut objects = std::collections::BTreeSet::new();
    let mut arrays = std::collections::BTreeSet::new();
    let mut runtime = world.resource_mut::<Runtime>();
    for waiter in &runtime.waiters {
        pending.push(waiter.receiver.clone());
        if let WaiterKind::Match { values } = &waiter.kind {
            pending.extend(values.iter().cloned());
        }
    }
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(id) if objects.insert(id) => {
                if let Some(fields) = runtime.objects.get(&id) {
                    pending.extend(fields.values().cloned());
                }
            }
            Value::Array(id) if arrays.insert(id) => {
                if let Some(values) = runtime.arrays.get(&id) {
                    pending.extend(values.values().cloned());
                }
            }
            _ => {}
        }
    }
    runtime.objects.retain(|id, _| objects.contains(id));
    runtime.arrays.retain(|id, _| arrays.contains(id));
}
