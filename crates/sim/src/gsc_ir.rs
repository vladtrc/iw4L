mod compiler;
mod entities;
mod entity_damage;
mod hud;
mod iw4_builtins;
mod iw4_natives;
mod natives;
mod natives_engine;
mod natives_math;
mod natives_player;
mod natives_t5;
mod objectives;
mod physics;
mod players;
mod presence;
mod runtime;
mod t5_builtins;
mod triggers;
mod vehicles;
mod weapons;

use bevy_ecs::prelude::{Component, Resource};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

pub use entities::{LevelData, StringTable, parse_entity_string};
pub(crate) use entity_damage::{
    EntityHit, HitTarget, ScriptBlast, ScriptHit, damage_entity, radius_targets,
};
pub(crate) use iw4_natives::set_dvar;
pub use natives::{Builtin, Catalog, Namespace, Owner};
pub use natives_engine::{EXIT_LEVEL, MAP_RESTART};
pub(crate) use players::{
    answer_join, answer_menu, choose_class, choose_default_class, describe_players, flashbang,
    force_death, note_team_answer, player_damage, script_seats, sync_players,
};
pub(crate) use presence::sync_presence;
pub use runtime::{Native, NativeRegistry};
pub(crate) use runtime::{
    advance_scheduler, copy_state, healthy, install, preflight, reset, start, take_signals,
};
pub(crate) use weapons::sync_engine_events;

pub const IR_VERSION: u32 = 3;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Undefined,
    Int(i32),
    Float(f32),
    String(Arc<str>),
    Vector([f32; 3]),
    Entity(crate::EntityRef),
    Object(u64),
    Array(u64),
    Function(u32),
    Builtin(u32),
    LocalizedString(Arc<str>),
    Animation { tree: Arc<str>, name: Arc<str> },
    AnimationTree(Arc<str>),
}

impl Value {
    pub fn string(text: &str) -> Self {
        Self::String(text.into())
    }

    pub fn level() -> Self {
        Self::Object(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub module: String,
    pub function: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
    pub location: Location,
    pub message: String,
    pub callers: Vec<Location>,
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{} in {}: {}",
            self.location.module,
            self.location.line,
            self.location.column,
            self.location.function,
            self.message
        )?;
        for caller in self.callers.iter().rev() {
            write!(
                f,
                "\n  called from {}:{}:{} in {}",
                caller.module, caller.line, caller.column, caller.function
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for Fault {}

impl Fault {
    pub(crate) fn at(location: &Location, message: impl Into<String>) -> Self {
        Self {
            location: location.clone(),
            message: message.into(),
            callers: Vec::new(),
        }
    }
}

pub trait SourceResolver {
    fn read(&self, module: &str) -> Result<String, String>;
    fn read_bytes(&self, module: &str) -> Result<Vec<u8>, String> {
        self.read(module).map(String::into_bytes)
    }
}

/// Install runs the struct initializer itself, before the map's structs exist.
pub struct Iw4Startup {
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}
impl Iw4Startup {
    pub fn new(resolver: &impl SourceResolver, gametype: &str, map: &str) -> Self {
        let gametype = format!("maps/mp/gametypes/{gametype}");
        let map = format!("maps/mp/{map}");
        let callbacks = "maps/mp/gametypes/_callbacksetup";
        let mut roots = vec![
            "codescripts/delete".to_owned(),
            "codescripts/struct".to_owned(),
            callbacks.to_owned(),
            gametype.clone(),
        ];
        let mut entries = vec![format!("{gametype}::main")];
        if resolver.read_bytes(&map).is_ok() {
            entries.push(format!("{map}::main"));
            roots.push(map);
        }
        entries.push(format!("{callbacks}::codecallback_startgametype"));
        Self { roots, entries }
    }
}

impl SourceResolver for BTreeMap<String, String> {
    fn read(&self, module: &str) -> Result<String, String> {
        self.get(module)
            .cloned()
            .ok_or_else(|| format!("missing script module {module}"))
    }
}

pub struct FileSources(pub std::path::PathBuf);
impl SourceResolver for FileSources {
    fn read(&self, module: &str) -> Result<String, String> {
        self.read_bytes(module).map(|bytes| decode_source(&bytes))
    }
    fn read_bytes(&self, module: &str) -> Result<Vec<u8>, String> {
        std::fs::read(self.0.join(format!("{module}.gsc"))).map_err(|e| e.to_string())
    }
}

pub fn normalize_module(module: &str) -> Result<String, String> {
    let path = module.replace('\\', "/").to_ascii_lowercase();
    let path = path.strip_suffix(".gsc").unwrap_or(&path);
    if path.is_empty()
        || path.split('/').any(|s| {
            s.is_empty()
                || s == "."
                || s == ".."
                || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        return Err(format!("invalid script module {module:?}"));
    }
    Ok(path.to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Site {
    Server,
    Client,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Realm {
    Iw4,
    Iw5,
    T5,
}

#[derive(Clone, Debug)]
pub struct ModuleIdentity {
    pub site: Site,
    pub realm: Realm,
    pub module: String,
    pub sha256: [u8; 32],
}

#[derive(Resource, Clone, Debug)]
pub struct Program {
    functions: Vec<Function>,
    names: BTreeMap<String, usize>,
    modules: Vec<ModuleIdentity>,
    symbols: Vec<Arc<str>>,
    symbol_ids: BTreeMap<Arc<str>, u32>,
    natives: Vec<Builtin>,
}
impl Program {
    pub fn load(
        resolver: &impl SourceResolver,
        roots: &[&str],
        catalog: &Catalog,
    ) -> Result<Self, Fault> {
        compiler::compile(resolver, roots, catalog)
    }
    pub fn modules(&self) -> &[ModuleIdentity] {
        &self.modules
    }
    pub fn realm(&self) -> Realm {
        self.modules.first().map_or(Realm::Iw4, |m| m.realm)
    }
    pub fn function_count(&self) -> usize {
        self.functions.len()
    }
    pub fn native_count(&self) -> usize {
        self.natives.len()
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        digest.update(IR_VERSION.to_le_bytes());
        for module in &self.modules {
            digest.update([module.site as u8, module.realm as u8]);
            digest.update((module.module.len() as u64).to_le_bytes());
            digest.update(module.module.as_bytes());
            digest.update(module.sha256);
        }
        digest.finalize().into()
    }
}

#[derive(Clone, Debug)]
struct Function {
    location: Location,
    parameters: usize,
    slots: usize,
    code: Vec<(Location, Op)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Callee {
    Script(u32),
    Native(u32),
    Unlinked(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Global {
    SelfRef,
    Level,
    Game,
    Anim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unary {
    Not,
    Complement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Binary {
    Or,
    Xor,
    And,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Case,
}

#[derive(Clone, Debug)]
enum Op {
    Constant(Value),
    FunctionRef(u32),
    Global(Global),
    Load(u32),
    Store(u32),
    Pop,
    Unary(Unary),
    Binary(Binary),
    Vector,
    Jump(usize),
    JumpFalse(usize),
    Call(Callee, usize, bool),
    Spawn(Callee, usize, bool),
    Indirect(usize, bool, bool),
    Array,
    ArrayKeys,
    EnsureLocalArray(u32),
    EnsureFieldArray(u32),
    EnsureIndexArray,
    LoadIndex,
    StoreIndex,
    Dup,
    DupPair,
    Wait,
    FrameEnd,
    Return,
    Size,
    LoadField(u32),
    StoreField(u32),
    Notify(usize),
    Await(Vec<u32>),
    AwaitMatch(usize),
    Endon,
}

#[derive(Clone, Debug)]
struct Frame {
    function: usize,
    pc: usize,
    stack_base: usize,
    receiver: Value,
    locals: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq)]
enum ThreadState {
    Runnable,
    Queued,
    Awaiting,
    Complete,
}

#[derive(Component, Clone, Debug)]
pub(crate) struct Thread {
    serial: u64,
    frames: Vec<Frame>,
    stack: Vec<Value>,
    state: ThreadState,
}

#[derive(Clone, Debug)]
enum WaiterKind {
    Endon { frame: usize },
    Waittill { outputs: Vec<u32> },
    Match { values: Vec<Value> },
}

#[derive(Clone, Debug)]
struct Waiter {
    receiver: Value,
    name: Arc<str>,
    thread: u64,
    kind: WaiterKind,
}

#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct Runtime {
    program: Option<Arc<Program>>,
    natives: Vec<Native>,
    next_serial: u64,
    pub fault: Option<Fault>,
    last_tick: Option<crate::Tick>,
    started: bool,
    objects: BTreeMap<u64, BTreeMap<u32, Value>>,
    next_object: u64,
    arrays: BTreeMap<u64, BTreeMap<ArrayKey, Value>>,
    dynamic_symbols: BTreeMap<Arc<str>, u32>,
    buckets: BTreeMap<i64, VecDeque<u64>>,
    spawned: Vec<u64>,
    waiters: Vec<Waiter>,
    dvars: BTreeMap<String, String>,
    loading: bool,
    precached: BTreeMap<(&'static str, String), i32>,
    /// Client-facing state a script sets and the simulation never reads; last write wins.
    presented: BTreeMap<&'static str, Vec<Value>>,
    budget: usize,
    /// Threads suspended in a `thread` call, innermost last, and their frame count.
    suspended: Vec<u64>,
    suspended_frames: usize,
    /// Endons that fired on a suspended thread; applied when its child yields.
    pending_unwinds: Vec<(u64, usize)>,
    entities: BTreeMap<u64, entities::ScriptEntity>,
    hud_slots: BTreeMap<u64, usize>,
    next_entity_number: i32,
    tables: Arc<BTreeMap<String, StringTable>>,
    rng: u32,
    pending_notifies: Vec<(Value, Arc<str>, Vec<Value>)>,
    signals: Vec<Arc<str>>,
    timers: Vec<(i64, Value, Arc<str>)>,
    engine: entities::EngineState,
    players: BTreeMap<u32, players::PlayerSlot>,
    menu_answers: BTreeMap<u32, VecDeque<players::MenuAnswer>>,
    joined: std::collections::BTreeSet<u32>,
    current_hit: Option<crate::script_player::Hit>,
    deaths: VecDeque<(u32, &'static str, Vec<Value>)>,
    pub(crate) exit_level: bool,
    shown: BTreeMap<u64, presence::Shown>,
    retired_presence: Vec<(crate::ScriptModelId, bool)>,
    next_spawned_presence: u32,
    blasts: Vec<entity_damage::ScriptBlast>,
    hits: Vec<entity_damage::ScriptHit>,
    use_held: std::collections::BTreeSet<u32>,
    fired_once: std::collections::BTreeSet<u64>,
    require_look_at: std::collections::BTreeSet<u64>,
    server_info: std::collections::BTreeSet<String>,
    missiles: BTreeMap<crate::ProjectileId, u64>,
    missiles_seen_ms: i32,
    lingering: Vec<(i64, u64)>,
    pending_deletes: Vec<u64>,
    vehicles: BTreeMap<u64, vehicles::Heli>,
    use_selected: BTreeMap<u32, u64>,
    t5: natives_t5::T5State,
}

impl Runtime {
    pub(crate) fn program_fingerprint(&self) -> Option<[u8; 32]> {
        self.program.as_ref().map(|program| program.fingerprint())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ArrayKey {
    Integer(i32),
    String(Arc<str>),
}

pub fn decode_source(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(source) => source.to_owned(),
        Err(_) => bytes.iter().copied().map(char::from).collect(),
    }
}
