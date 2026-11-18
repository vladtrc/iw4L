use super::entities::{EntityKind, SPAWNED_PRESENCE_BASE, ScriptEntity};
use crate::ScriptModelId;
use crate::bullet_collision::{AuthorityDObjState, EntityCollisionCapabilities};
use crate::frame::FrameWorld;
use crate::gentity::ScriptMoverGentity;
use crate::script::{Arc, BTreeMap, RoundScript, Value};
use bevy_ecs::prelude::World;

type SpeakingModels = Vec<(ScriptModelId, Arc<str>, [f32; 3], Option<u32>)>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shown {
    origin: [f32; 3],
    angles: [f32; 3],
    model: Option<Arc<str>>,
    attachments: Vec<(Arc<str>, Arc<str>)>,
    hidden: bool,
    thermal_draw: bool,
    hero_lighting: bool,
    less_frequent_lighting: bool,
    aim_assist: bool,
    shown_to: u64,
    solid: bool,
    contents: i32,
    moving: bool,
}

struct Wanted {
    collision_only: bool,
    object: u64,
    presence: ScriptModelId,
    origin: [f32; 3],
    angles: [f32; 3],
    model: Option<Arc<str>>,
    attachments: Vec<(Arc<str>, Arc<str>)>,
    hidden: bool,
    thermal_draw: bool,
    hero_lighting: bool,
    less_frequent_lighting: bool,
    aim_assist: bool,
    shown_to: u64,
    solid: bool,
    contents: i32,
    anim_op: Option<Option<Arc<str>>>,
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.01)
}

fn near_angles(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter()
        .zip(b)
        .all(|(a, b)| math_iw4::angle_subtract(*a, b).abs() < 0.01)
}

fn vector(runtime: &mut RoundScript, id: u64, name: &str) -> [f32; 3] {
    match runtime.object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn model_field(runtime: &mut RoundScript, id: u64) -> Option<Arc<str>> {
    match runtime.object_field(id, "model") {
        Value::String(model) if !model.is_empty() && !model.starts_with('*') => Some(model.into()),
        _ => None,
    }
}

pub(crate) fn initialize_map_models(world: &mut World) {
    let models: BTreeMap<ScriptModelId, Arc<str>> = FrameWorld::from_world(world)
        .entity_collision_capabilities()
        .iter()
        .filter_map(|row| {
            Some((
                row.owner.script_model()?,
                row.dobj.as_ref()?.current_model.as_str().into(),
            ))
        })
        .collect();
    let mut runtime = world.resource_mut::<RoundScript>();
    let objects: Vec<_> = runtime
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Map)
        .filter_map(|(object, entity)| Some((*object, models.get(&entity.presence?)?.clone())))
        .collect();
    for (object, model) in objects {
        runtime.set_object_field(object, "model", Value::String(model.into()));
    }
}

pub(crate) fn spawn_presence(world: &mut World, origin: [f32; 3]) -> Result<ScriptModelId, String> {
    let mut runtime = world.resource_mut::<crate::script::MatchScript>();
    let serial = runtime.next_spawned_presence;
    runtime.next_spawned_presence = serial
        .checked_add(1)
        .filter(|n| *n < SPAWNED_PRESENCE_BASE)
        .ok_or("spawned script model identifiers exhausted")?;
    let id = ScriptModelId::from_wire(SPAWNED_PRESENCE_BASE + serial);
    let mut frame = FrameWorld::from_world(world);
    frame
        .spawn_script_mover(id, origin, [0.0; 3])
        .map_err(|_| "no free entities".to_owned())?;
    frame.insert_collision_owner(EntityCollisionCapabilities::current_tick(
        crate::AuthorityModelOwner::ScriptModel(id),
        None,
        Vec::new(),
    ));
    Ok(id)
}

/// The observability boundary for script writes: native collision reads go
/// through here, so a query sees every pose, solidity and visibility change a
/// native or field store made earlier in the same tick.
pub fn settled(world: &mut World) -> FrameWorld<'_> {
    settle_collision(world);
    FrameWorld::from_world(world)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CollisionPlacement {
    origin: [f32; 3],
    angles: [f32; 3],
    hidden: bool,
    solid: bool,
    contents: i32,
}

// Snapshot-local placement lookup. Sorting by (presence, object) preserves
// BTreeMap's last-write-wins rule when several script objects alias a presence.
#[derive(Default)]
struct CollisionPlacements(Vec<(ScriptModelId, u64, CollisionPlacement)>);
impl CollisionPlacements {
    fn sort(&mut self) {
        self.0
            .sort_unstable_by_key(|&(presence, object, _)| (presence, object));
    }
    fn iter(&self) -> impl Iterator<Item = (ScriptModelId, &CollisionPlacement)> {
        self.0.chunk_by(|a, b| a.0 == b.0).map(|rows| {
            let (presence, _, placement) = rows.last().unwrap();
            (*presence, placement)
        })
    }
    fn get(&self, id: ScriptModelId) -> Option<&CollisionPlacement> {
        let end = self.0.partition_point(|&(presence, _, _)| presence <= id);
        let (presence, _, placement) = self.0.get(end.checked_sub(1)?)?;
        (*presence == id).then_some(placement)
    }
}

#[derive(Clone, Debug)]
struct CollisionInput {
    object: u64,
    presence: ScriptModelId,
    shown: Shown,
}

#[derive(Clone, Copy, PartialEq)]
struct CollisionSource<'a> {
    object: u64,
    presence: ScriptModelId,
    origin: [f32; 3],
    angles: [f32; 3],
    model: Option<&'a str>,
    attachments: &'a [(Arc<str>, Arc<str>)],
    hidden: bool,
    thermal_draw: bool,
    hero_lighting: bool,
    less_frequent_lighting: bool,
    aim_assist: bool,
    shown_to: u64,
    solid: bool,
    contents: i32,
    has_operations: bool,
}

impl CollisionInput {
    fn from_source(source: CollisionSource<'_>) -> Self {
        Self {
            object: source.object,
            presence: source.presence,
            shown: Shown {
                origin: source.origin,
                angles: source.angles,
                model: source.model.map(Arc::from),
                attachments: source.attachments.to_vec(),
                hidden: source.hidden,
                thermal_draw: source.thermal_draw,
                hero_lighting: source.hero_lighting,
                less_frequent_lighting: source.less_frequent_lighting,
                aim_assist: source.aim_assist,
                shown_to: source.shown_to,
                solid: source.solid,
                contents: source.contents,
                moving: false,
            },
        }
    }
    fn non_reflexive(&self) -> bool {
        self.shown
            .origin
            .iter()
            .chain(&self.shown.angles)
            .any(|v| v.is_nan())
    }
    fn source(&self) -> CollisionSource<'_> {
        CollisionSource {
            object: self.object,
            presence: self.presence,
            origin: self.shown.origin,
            angles: self.shown.angles,
            model: self.shown.model.as_deref(),
            attachments: &self.shown.attachments,
            hidden: self.shown.hidden,
            thermal_draw: self.shown.thermal_draw,
            hero_lighting: self.shown.hero_lighting,
            less_frequent_lighting: self.shown.less_frequent_lighting,
            aim_assist: self.shown.aim_assist,
            shown_to: self.shown.shown_to,
            solid: self.shown.solid,
            contents: self.shown.contents,
            has_operations: false,
        }
    }
}

fn presentation_matches(shown: Option<&Shown>, source: CollisionSource<'_>) -> bool {
    !source.has_operations
        && shown.is_some_and(|shown| {
            !shown.moving
                && shown.hidden == source.hidden
                && shown.thermal_draw == source.thermal_draw
                && shown.hero_lighting == source.hero_lighting
                && shown.less_frequent_lighting == source.less_frequent_lighting
                && shown.aim_assist == source.aim_assist
                && shown.shown_to == source.shown_to
                && shown.solid == source.solid
                && shown.contents == source.contents
                && shown.model.as_deref() == source.model
                && shown.attachments == source.attachments
                && near(shown.origin, source.origin)
                && near_angles(shown.angles, source.angles)
        })
}

fn collision_sources(
    runtime: &RoundScript,
    symbols: [u32; 3],
) -> impl Iterator<Item = CollisionSource<'_>> {
    runtime
        .entities
        .iter()
        .filter_map(move |(&object, entity)| collision_source(runtime, symbols, object, entity))
}

fn collision_source<'a>(
    runtime: &'a RoundScript,
    symbols: [u32; 3],
    object: u64,
    entity: &'a ScriptEntity,
) -> Option<CollisionSource<'a>> {
    let presence = entity.presence?;
    let fields = runtime.objects.get(&object);
    let vector = |symbol| match fields.and_then(|fields| fields.get(&symbol)) {
        Some(Value::Vector(value)) => *value,
        _ => [0.; 3],
    };
    let model = match fields.and_then(|fields| fields.get(&symbols[2])) {
        Some(Value::String(model)) if !model.is_empty() && !model.starts_with('*') => {
            Some(&**model)
        }
        _ => None,
    };
    let actor = runtime.actors.get(&object);
    Some(CollisionSource {
        object,
        presence,
        origin: vector(symbols[0]),
        angles: vector(symbols[1]),
        model,
        attachments: &entity.attachments,
        hidden: entity.hidden,
        thermal_draw: actor.is_some_and(|actor| actor.thermal_draw),
        hero_lighting: entity.hero_only_lighting || actor.is_some(),
        less_frequent_lighting: entity.less_frequent_lighting,
        aim_assist: entity.aim_assist,
        shown_to: entity.shown_to,
        solid: entity.solid,
        contents: entity.contents,
        has_operations: entity.anim_op.is_some(),
    })
}

fn collision_turrets(
    runtime: &RoundScript,
    angles: u32,
) -> Vec<(u64, ScriptModelId, Vec<xmodel_runtime::TagRotation>)> {
    runtime
        .engine
        .turrets
        .iter()
        .filter_map(|(&object, turret)| {
            let presence = runtime.entities.get(&object)?.presence?;
            let base = match runtime
                .objects
                .get(&object)
                .and_then(|fields| fields.get(&angles))
            {
                Some(Value::Vector(value)) => *value,
                _ => [0.; 3],
            };
            Some((object, presence, turret.model_rotations(base)))
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CollisionSettlement {
    revision: Option<u64>,
    inputs: Vec<CollisionInput>,
    turrets: Vec<(u64, ScriptModelId, Vec<xmodel_runtime::TagRotation>)>,
    checks: u64,
    reused: u64,
    source_maps: Option<[Arc<()>; 3]>,
    source_symbols: Option<[u32; 3]>,
    non_reflexive: bool,
    untracked_sources: bool,
}

impl CollisionSettlement {
    fn input(&self, object: u64) -> Option<&CollisionInput> {
        self.inputs
            .binary_search_by_key(&object, |row| row.object)
            .ok()
            .map(|index| &self.inputs[index])
    }

    fn changes(
        &self,
        runtime: &RoundScript,
        symbols: [u32; 3],
        revision: u64,
    ) -> Option<Vec<CollisionInput>> {
        if self.revision != Some(revision) {
            return None;
        }
        let identities = [
            runtime.entities.identity(),
            runtime.objects.identity(),
            runtime.actors.identity(),
        ];
        let same_maps = self
            .source_maps
            .as_ref()
            .is_some_and(|old| old.iter().zip(identities).all(|(a, b)| Arc::ptr_eq(a, b)));
        if !same_maps
            || self.source_symbols != Some(symbols)
            || self.non_reflexive
            || self.untracked_sources
        {
            return self.changes_full(runtime, symbols, revision);
        }
        let (Some(entities), Some(objects), Some(actors)) = (
            runtime.entities.changed(),
            runtime.objects.changed(),
            runtime.actors.changed(),
        ) else {
            return self.changes_full(runtime, symbols, revision);
        };
        let mut ids = Vec::with_capacity(entities.len() + objects.len() + actors.len());
        ids.extend(entities);
        ids.extend(objects);
        ids.extend(actors);
        ids.sort_unstable();
        ids.dedup();
        let mut changed = Vec::new();
        for object in ids {
            let previous = self.input(object);
            let source = runtime
                .entities
                .get(&object)
                .and_then(|entity| collision_source(runtime, symbols, object, entity));
            match (previous, source) {
                (None, None) => {}
                (Some(previous), Some(source)) if previous.presence == source.presence => {
                    if source != previous.source() {
                        changed.push(CollisionInput::from_source(source));
                    }
                }
                _ => return None,
            }
        }
        Some(changed)
    }

    fn changes_full(
        &self,
        runtime: &RoundScript,
        symbols: [u32; 3],
        revision: u64,
    ) -> Option<Vec<CollisionInput>> {
        if self.revision != Some(revision) {
            return None;
        }
        let mut old = self.inputs.iter();
        let mut changed = Vec::new();
        for source in collision_sources(runtime, symbols) {
            let previous = old.next()?;
            if (previous.object, previous.presence) != (source.object, source.presence) {
                return None;
            }
            if source != previous.source() {
                changed.push(CollisionInput::from_source(source));
            }
        }
        old.next().is_none().then_some(changed)
    }

    fn capture(
        &mut self,
        inputs: Vec<CollisionInput>,
        turrets: Vec<(u64, ScriptModelId, Vec<xmodel_runtime::TagRotation>)>,
        revision: u64,
    ) {
        let mut presences: Vec<_> = inputs.iter().map(|input| input.presence).collect();
        presences.sort_unstable();
        // Aliases can alternate DObj models and advance revisions on every settlement.
        self.revision = (!presences.windows(2).any(|pair| pair[0] == pair[1])).then_some(revision);
        self.non_reflexive = inputs.iter().any(CollisionInput::non_reflexive);
        self.inputs = inputs;
        self.turrets = turrets;
    }
}

/// Collision rows only. Networked mover state is left to `present`: posing a
/// mover mid-tick would break its tick-to-tick velocity.
pub(crate) fn settle_collision(world: &mut World) {
    let _span = perf::span_handle!("script_collision_settle", "iw4l.sim").enter();
    let anim_span = perf::span_handle!("script_settle_animation", "iw4l.sim").enter();
    super::animations::settle_actor_trees(world);
    drop(anim_span);
    let revision = FrameWorld::from_world(world).collision_revision();
    let (changes, turrets, symbols) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.collision_settlement.checks += 1;
        let symbols = [
            runtime.symbol("origin"),
            runtime.symbol("angles"),
            runtime.symbol("model"),
        ];
        let changes = runtime
            .collision_settlement
            .changes(&runtime, symbols, revision);
        let turrets = collision_turrets(&runtime, symbols[1]);
        if changes.as_ref().is_some_and(Vec::is_empty)
            && runtime.collision_settlement.turrets == turrets
        {
            runtime.collision_settlement.reused += 1;
            acknowledge_sources(&mut runtime, symbols, false);
            return;
        }
        (changes, turrets, symbols)
    };
    let incremental = changes.is_some();
    let mut placements = CollisionPlacements::default();
    let mut inputs = Vec::new();
    let wanted = if let Some(changes) = changes {
        inputs = changes;
        collect_changed(world, &inputs, &turrets, &mut placements)
    } else {
        collect_wanted(world, Some(&mut placements), Some(&mut inputs))
    };
    placements.sort();
    let model_span = perf::span_handle!("script_settle_models", "iw4l.sim").enter();
    let mut frame = FrameWorld::from_world(world);
    for want in &wanted {
        present_model(&mut frame, want);
    }
    drop(model_span);
    let placement_span = perf::span_handle!("script_settle_placements", "iw4l.sim").enter();
    let rotations: BTreeMap<_, _> = turrets
        .iter()
        .map(|(_, presence, rotations)| (*presence, rotations.clone()))
        .collect();
    let mut frame = FrameWorld::from_world(world);
    let content = frame.content();
    let place = |row: &mut EntityCollisionCapabilities, placement: &CollisionPlacement| {
        let &CollisionPlacement {
            origin,
            angles,
            hidden,
            solid,
            contents,
        } = placement;
        row.hidden = hidden;
        row.solid = solid;
        // The content frame is immutable during the match. Repeated reads of
        // exactly the settled pose do not need another frame lookup or matrix.
        // Nonidentical poses still validate the frame before the near check.
        let model_pose = if row.followed_pose == Some((origin, angles)) {
            None
        } else {
            let Some(pose) = content.simulation_model_transform(origin, angles) else {
                row.hidden = true;
                row.solid = false;
                row.followed_pose = None;
                return;
            };
            Some(pose)
        };
        if let Some(body) = &mut row.body {
            body.bounds.contents = contents as u32;
        }
        if let Some(rotations) = row.owner.script_model().and_then(|id| rotations.get(&id))
            && let Some(dobj) = row.dobj.as_mut()
        {
            dobj.set_tag_rotations(rotations.clone());
        }
        let settled = row
            .followed_pose
            .is_some_and(|(at, facing)| near(at, origin) && near_angles(facing, angles));
        if settled {
            return;
        }
        row.followed_pose = Some((origin, angles));
        if let Some(body) = &mut row.body {
            body.origin = origin;
        }
        if let Some(dobj) = row.dobj.as_mut() {
            dobj.set_world_transform(model_pose.expect("unsettled pose has a resolved frame"));
        }
        for brush in &mut row.linked_brushes {
            brush.origin = origin;
            brush.angles = angles;
        }
    };
    if incremental {
        for (presence, placement) in placements.iter() {
            for row in frame.collision_owners_mut(presence) {
                place(row, placement);
            }
        }
    } else {
        for row in frame.entity_collision_capabilities_mut() {
            if let Some(placement) = row.owner.script_model().and_then(|id| placements.get(id)) {
                place(row, placement);
            }
        }
    }
    drop(placement_span);
    let _materialize_span = perf::span_handle!("script_settle_materialize", "iw4l.sim").enter();
    let materialize = |row: &mut EntityCollisionCapabilities| {
        if row.solid
            && let Some(dobj) = &mut row.dobj
        {
            dobj.materialize();
        }
    };
    if incremental {
        for (presence, _) in placements.iter() {
            for row in frame.collision_owners_mut(presence) {
                materialize(row);
            }
        }
    } else {
        for row in frame.entity_collision_capabilities_mut() {
            materialize(row);
        }
    }
    let revision = frame.collision_revision();
    let mut runtime = world.resource_mut::<RoundScript>();
    if incremental {
        let cache = &mut runtime.collision_settlement;
        for input in inputs {
            let index = cache
                .inputs
                .binary_search_by_key(&input.object, |old| old.object)
                .expect("unchanged collision topology");
            cache.non_reflexive |= input.non_reflexive();
            cache.inputs[index] = input;
        }
        cache.revision = Some(revision);
        cache.turrets = turrets;
    } else {
        runtime
            .collision_settlement
            .capture(inputs, turrets, revision);
    }
    acknowledge_sources(&mut runtime, symbols, !incremental);
}

fn acknowledge_sources(runtime: &mut RoundScript, symbols: [u32; 3], refreshed: bool) {
    let pending = |entity: &ScriptEntity| entity.presence.is_some() && entity.anim_op.is_some();
    let verify_topology = refreshed || runtime.collision_settlement.untracked_sources;
    let incomplete = verify_topology
        && !runtime
            .collision_settlement
            .inputs
            .iter()
            .map(|row| (row.object, row.presence))
            .eq(runtime
                .entities
                .iter()
                .filter_map(|(&id, entity)| Some((id, entity.presence?))));
    let operations = if verify_topology {
        runtime.entities.values().any(pending)
    } else if let Some(ids) = runtime.entities.changed() {
        ids.iter()
            .any(|id| runtime.entities.get(id).is_some_and(pending))
    } else {
        runtime.entities.values().any(pending)
    };
    runtime.collision_settlement.untracked_sources = incomplete || operations;
    runtime.collision_settlement.source_maps = Some([
        runtime.entities.identity().clone(),
        runtime.objects.identity().clone(),
        runtime.actors.identity().clone(),
    ]);
    runtime.collision_settlement.source_symbols = Some(symbols);
    runtime.entities.acknowledge();
    runtime.objects.acknowledge();
    runtime.actors.acknowledge();
}

pub(crate) fn sync_presence(world: &mut World) {
    let _span = perf::span_handle!("script_presence_sync", "iw4l.sim").enter();
    super::model_state::player_appearance::sync_all(world);
    let request = world.resource::<crate::step::StepRequest>();
    let tick = request.tick;
    let now = crate::level_time_ms(tick);
    settle_collision(world);
    {
        let runtime = world.resource::<RoundScript>();
        perf::counter!(
            "script_collision_checks",
            "iw4l.sim",
            runtime.collision_settlement.checks as f64
        );
        perf::counter!(
            "script_collision_reused",
            "iw4l.sim",
            runtime.collision_settlement.reused as f64
        );
    }
    super::entity_damage::apply_script_blasts(world, tick);
    super::audio::waits::advance(world);
    publish_loop_sounds(world);
    super::objectives::publish(world);
    super::controls::sync_script_locks(world);
    super::triggers::dispatch_triggers(world);
    super::actors::suppression::advance(world, now);
    if let Err(message) = super::actors::targeting::advance(world, now) {
        world.resource_mut::<RoundScript>().fault = Some(crate::script::Fault::at(
            &crate::script::Location {
                module: "<engine>".into(),
                function: "actor target selection".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    if let Err(message) = super::actors::death::advance(world, now)
        .and_then(|()| super::actors::corpses::advance(world, now))
    {
        world.resource_mut::<RoundScript>().fault = Some(crate::script::Fault::at(
            &crate::script::Location {
                module: "<engine>".into(),
                function: "actor death".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    if let Err(message) = super::actors::scripted::think(world, now) {
        world.resource_mut::<RoundScript>().fault = Some(crate::script::Fault::at(
            &crate::script::Location {
                module: "<engine>".into(),
                function: "actor scripted animation".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    if let Err(message) = super::actors::turret::advance(world, now)
        .and_then(|()| super::actors::traversal::advance(world, now))
        .and_then(|()| super::actors::bad_places::advance(world, now))
        .and_then(|()| super::actors::arrival::advance(world, now))
        .and_then(|()| super::actors::cover::advance(world, now))
        .and_then(|()| super::actors::reacquire::advance(world, now))
        .and_then(|()| super::actors::grenades::think(world, now))
        .and_then(|()| super::actors::animation::think(world, now))
    {
        world.resource_mut::<RoundScript>().fault = Some(crate::script::Fault::at(
            &crate::script::Location {
                module: "<engine>".into(),
                function: "actor animation".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    if let Err(message) = super::actors::orientation::advance(world) {
        world.resource_mut::<RoundScript>().fault = Some(crate::script::Fault::at(
            &crate::script::Location {
                module: "<engine>".into(),
                function: "actor orientation".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    if let Err(message) = super::actors::physics::advance(world) {
        world.resource_mut::<RoundScript>().fault = Some(crate::script::Fault::at(
            &crate::script::Location {
                module: "<engine>".into(),
                function: "actor physics".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    if let Err(message) = super::actors::turret::pose::advance(world) {
        world.resource_mut::<RoundScript>().fault = Some(crate::script::Fault::at(
            &crate::script::Location {
                module: "<engine>".into(),
                function: "actor turret pose".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    settle_collision(world);
    present(world, now);
    if let Err(message) = super::actors::prone::advance(world, now) {
        world.resource_mut::<RoundScript>().fault = Some(crate::script::Fault::at(
            &crate::script::Location {
                module: "<engine>".into(),
                function: "actor prone update".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    if let Err(message) = super::actors::goals::advance(world, now) {
        world.resource_mut::<RoundScript>().fault = Some(crate::script::Fault::at(
            &crate::script::Location {
                module: "<engine>".into(),
                function: "actor goal update".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    super::animations::publish(world);
    if let Err(message) = super::actors::grenades::advance(world, now)
        .and_then(|()| super::actors::weapon_drops::advance(world, i64::from(now)))
    {
        world.resource_mut::<RoundScript>().fault = Some(crate::script::Fault::at(
            &crate::script::Location {
                module: "<engine>".into(),
                function: "actor weapon drop".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    publish_killcam_cameras(world);
    settle_collision(world);
}

fn publish_killcam_cameras(world: &mut World) {
    let runtime = world.resource::<RoundScript>();
    let cameras: Vec<_> = runtime
        .entities
        .iter()
        .filter_map(|(object, entity)| {
            entity.presence?;
            let mode = if runtime.vehicles.contains_key(object) {
                Some(playerstate_iw4::KillCamMode::Mode1Heli)
            } else if runtime.engine.turrets.contains_key(object) {
                Some(playerstate_iw4::KillCamMode::Mode6Turret)
            } else {
                None
            };
            Some((entity.number, mode))
        })
        .collect();
    let mut frame = FrameWorld::from_world(world);
    for (number, mode) in cameras {
        if let Some(mover) = frame.script_mover_mut_by_number(number) {
            mover.killcam_camera = mode;
        }
    }
}

fn present(world: &mut World, now: i32) {
    let _span = perf::span_handle!("script_presence_present", "iw4l.sim").enter();
    let retired = std::mem::take(&mut world.resource_mut::<RoundScript>().retired_presence);
    let wanted = collect_wanted(world, None, None);
    if retired.is_empty() && wanted.is_empty() {
        return;
    }
    let mut frame = FrameWorld::from_world(world);
    let movers: BTreeMap<ScriptModelId, ScriptMoverGentity> =
        crate::frame::collect_script_movers(frame.ecs())
            .into_iter()
            .map(|mover| (mover.id, mover))
            .collect();
    for (id, spawned) in retired {
        let Some(mover) = movers.get(&id) else {
            continue;
        };
        if spawned {
            frame.remove_script_mover_by_number(mover.state.number);
        } else if let Some(mover) = frame.script_mover_mut_by_number(mover.state.number) {
            mover.state.e_flags |= entity_iw4::CG_SCRIPT_MOVER_NODRAW;
            mover.nonsolid = true;
        }
    }
    let mut updates = Vec::new();
    for want in wanted {
        let Some(mover) = movers.get(&want.presence) else {
            continue;
        };
        let first = frame
            .ecs()
            .resource::<RoundScript>()
            .shown
            .get(&want.object)
            .cloned();
        let number = first.is_none().then_some(mover.state.number);
        let shown = first.unwrap_or_else(|| Shown {
            origin: mover.state.tr_base,
            angles: mover.state.apos_tr_base,
            model: frame
                .collision_owner_mut(want.presence)
                .and_then(|row| row.dobj.as_ref())
                .map(|dobj| dobj.current_model.as_str().into()),
            attachments: Vec::new(),
            hidden: mover.state.e_flags & entity_iw4::CG_SCRIPT_MOVER_NODRAW != 0,
            thermal_draw: mover.thermal_draw,
            hero_lighting: mover.state.e_flags & entity_iw4::SP_HERO_ONLY_LIGHTING != 0,
            less_frequent_lighting: mover.state.e_flags & entity_iw4::SP_LESS_FREQUENT_LIGHTING
                != 0,
            aim_assist: mover.state.e_flags & entity_iw4::SP_SCRIPT_MODEL_AIM_ASSIST != 0,
            shown_to: mover.shown_to,
            solid: !mover.nonsolid,
            contents: mover.body.map_or(0, |body| body.contents as i32),
            moving: false,
        });
        let posed = !near(shown.origin, want.origin) || !near_angles(shown.angles, want.angles);
        if let Some(mover) = frame.script_mover_mut_by_number(mover.state.number) {
            if want.hidden || want.collision_only {
                mover.state.e_flags |= entity_iw4::CG_SCRIPT_MOVER_NODRAW;
            } else {
                mover.state.e_flags &= !entity_iw4::CG_SCRIPT_MOVER_NODRAW;
            }
            mover.nonsolid = !want.solid;
            if let Some(body) = &mut mover.body {
                body.contents = want.contents as u32;
            }
            mover.shown_to = want.shown_to;
            mover.thermal_draw = want.thermal_draw;
            if want.aim_assist {
                mover.state.e_flags |= entity_iw4::SP_SCRIPT_MODEL_AIM_ASSIST;
            } else {
                mover.state.e_flags &= !entity_iw4::SP_SCRIPT_MODEL_AIM_ASSIST;
            }
            if want.less_frequent_lighting {
                mover.state.e_flags |= entity_iw4::SP_LESS_FREQUENT_LIGHTING;
            } else {
                mover.state.e_flags &= !entity_iw4::SP_LESS_FREQUENT_LIGHTING;
            }
            if want.hero_lighting {
                mover.state.e_flags |= entity_iw4::SP_HERO_ONLY_LIGHTING;
            } else {
                mover.state.e_flags &= !entity_iw4::SP_HERO_ONLY_LIGHTING;
            }
        }
        if posed || shown.moving {
            frame.set_script_mover_pose(mover.state.number, now, want.origin, want.angles);
        }
        let reshaped = want.model != shown.model || want.attachments != shown.attachments;
        if reshaped || want.anim_op.is_some() {
            present_model(&mut frame, &want);
        }
        updates.push((
            want.object,
            number,
            Shown {
                origin: if posed { want.origin } else { shown.origin },
                angles: if posed { want.angles } else { shown.angles },
                model: want.model,
                attachments: want.attachments,
                hidden: want.hidden,
                thermal_draw: want.thermal_draw,
                hero_lighting: want.hero_lighting,
                less_frequent_lighting: want.less_frequent_lighting,
                aim_assist: want.aim_assist,
                shown_to: want.shown_to,
                solid: want.solid,
                contents: want.contents,
                moving: posed,
            },
        ));
    }
    let movers = crate::frame::collect_script_movers(frame.ecs());
    let content = frame.content();
    crate::presence::follow_movers(
        frame.entity_collision_capabilities_mut(),
        &movers,
        now,
        &content,
    );
    let mut runtime = world.resource_mut::<RoundScript>();
    for (object, number, shown) in updates {
        let Some(entity) = runtime.entities.get_mut(&object) else {
            continue;
        };
        if let Some(number) = number.filter(|_| !matches!(entity.kind, EntityKind::Missile(_))) {
            entity.number = number;
        }
        runtime.shown.insert(object, shown);
    }
}

fn with_tag_dobj<T>(
    world: &mut World,
    object: u64,
    read: impl FnOnce(&AuthorityDObjState) -> Option<T>,
) -> Option<T> {
    if world
        .resource::<RoundScript>()
        .player_client(object)
        .is_none()
    {
        settle_collision(world);
    }
    with_tag_dobj_settled(world, object, read)
}

// Internal read-only query batch: the caller has settled poses and animation.
fn with_tag_dobj_settled<T>(
    world: &mut World,
    object: u64,
    read: impl FnOnce(&AuthorityDObjState) -> Option<T>,
) -> Option<T> {
    if let Some(client) = world.resource::<RoundScript>().player_client(object) {
        super::model_state::player_appearance::sync_one(world, client);
        let runtime = world.resource::<RoundScript>();
        let slot = runtime.players.get(&client)?;
        if slot.object != object {
            return None;
        }
        return read(slot.appearance.as_ref()?.dobj.as_ref()?);
    }
    let presence = world
        .resource::<RoundScript>()
        .entities
        .get(&object)?
        .presence?;
    let frame = FrameWorld::from_world(world);
    if let Some(dobj) = frame
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(presence))?
        .dobj
        .as_ref()
    {
        return read(dobj);
    }
    let (model, origin, angles) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        (
            model_field(&mut runtime, object)?,
            vector(&mut runtime, object, "origin"),
            vector(&mut runtime, object, "angles"),
        )
    };
    let capability = FrameWorld::from_world(world)
        .model_capability(&model)
        .flatten()?;
    let pose = FrameWorld::from_world(world)
        .content()
        .simulation_model_transform(origin, angles)?;
    read(&AuthorityDObjState::new_dirty(
        model.to_string(),
        Some(capability),
        pose,
    ))
}

/// Model-space offset of `tag`: `None` when the entity has no model to ask,
/// `Some(None)` when its model lacks the tag.
pub(crate) fn tag_lookup(world: &mut World, object: u64, tag: &str) -> Option<Option<[f32; 3]>> {
    with_tag_dobj(world, object, |dobj| {
        Some(dobj.tag_world_pose(tag).map(|(at, _)| {
            dobj.world_from_model
                .inverse()
                .transform_point3(glam::Vec3::from(at))
                .to_array()
        }))
    })
}

pub(crate) fn tag_world(
    world: &mut World,
    object: u64,
    tag: &str,
) -> Option<([f32; 3], [[f32; 3]; 3])> {
    if world
        .resource::<RoundScript>()
        .player_client(object)
        .is_none()
    {
        settle_collision(world);
    }
    tag_world_settled(world, object, tag)
}

/// Query in a read-only batch after collision settlement; no pose/animation writes.
pub(crate) fn tag_world_settled(
    world: &mut World,
    object: u64,
    tag: &str,
) -> Option<([f32; 3], [[f32; 3]; 3])> {
    let mut matrix = with_tag_dobj_settled(world, object, |dobj| dobj.tag_world_matrix(tag))?;
    let simulation_origin = {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime
            .player_client(object)
            .is_none()
            .then(|| vector(&mut runtime, object, "origin"))
    };
    if let Some(origin) = simulation_origin {
        let content = FrameWorld::from_world(world).content();
        let owner = if content.atlas().is_some() {
            asset_core::world_frame::SimPoint::from_slab(origin).world()
        } else {
            asset_core::atlas::WorldId::ROOT
        };
        matrix = content.scene_model_transform(owner, matrix)?;
    }
    let axis = |v: glam::Vec4| v.truncate().normalize_or_zero().to_array();
    Some((
        matrix.w_axis.truncate().to_array(),
        [
            axis(matrix.x_axis),
            axis(matrix.y_axis),
            axis(matrix.z_axis),
        ],
    ))
}

pub(crate) fn link_parent_pose(
    world: &mut World,
    link: &super::entities::Link,
) -> Option<([f32; 3], [[f32; 3]; 3])> {
    if let Some(tag) = &link.tag {
        return tag_world(world, link.parent, tag);
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let Value::Vector(origin) = runtime.object_field(link.parent, "origin") else {
        return None;
    };
    let Value::Vector(angles) = runtime.object_field(link.parent, "angles") else {
        return None;
    };
    Some((origin, math_iw4::angles_to_axis(angles)))
}

fn collect_changed(
    world: &mut World,
    inputs: &[CollisionInput],
    turrets: &[(u64, ScriptModelId, Vec<xmodel_runtime::TagRotation>)],
    placements: &mut CollisionPlacements,
) -> Vec<Wanted> {
    let _span = perf::span_handle!("script_settle_incremental_collect", "iw4l.sim").enter();
    let mut runtime = world.resource_mut::<RoundScript>();
    let mut wanted = Vec::with_capacity(inputs.len());
    let RoundScript {
        entities,
        collision_settlement,
        ..
    } = &mut *runtime;
    for input in inputs {
        let entity = entities.get_mut(&input.object).unwrap();
        let shown = &input.shown;
        placements.0.push((
            input.presence,
            input.object,
            CollisionPlacement {
                origin: shown.origin,
                angles: shown.angles,
                hidden: shown.hidden,
                solid: shown.solid,
                contents: shown.contents,
            },
        ));
        if presentation_matches(
            collision_settlement
                .input(input.object)
                .map(|row| &row.shown),
            CollisionSource {
                has_operations: entity.anim_op.is_some(),
                ..input.source()
            },
        ) {
            continue;
        }
        wanted.push(Wanted {
            collision_only: matches!(entity.kind, EntityKind::Missile(_)),
            object: input.object,
            presence: input.presence,
            origin: shown.origin,
            angles: shown.angles,
            model: shown.model.clone(),
            attachments: shown.attachments.clone(),
            hidden: shown.hidden,
            thermal_draw: shown.thermal_draw,
            hero_lighting: shown.hero_lighting,
            less_frequent_lighting: shown.less_frequent_lighting,
            aim_assist: shown.aim_assist,
            shown_to: shown.shown_to,
            solid: shown.solid,
            contents: shown.contents,
            anim_op: entity.anim_op.take(),
        });
    }
    for &(object, presence, _) in turrets {
        let origin = vector(&mut runtime, object, "origin");
        let angles = vector(&mut runtime, object, "angles");
        let entity = &runtime.entities[&object];
        placements.0.push((
            presence,
            object,
            CollisionPlacement {
                origin,
                angles,
                hidden: entity.hidden,
                solid: entity.solid,
                contents: entity.contents,
            },
        ));
    }
    wanted
}

fn collect_wanted(
    world: &mut World,
    mut placements: Option<&mut CollisionPlacements>,
    mut settled_inputs: Option<&mut Vec<CollisionInput>>,
) -> Vec<Wanted> {
    let _span = perf::span_handle!("script_presence_collect", "iw4l.sim").enter();
    let settling = settled_inputs.is_some();
    let mut runtime = world.resource_mut::<RoundScript>();
    let origin_field = runtime.symbol("origin");
    let angles_field = runtime.symbol("angles");
    let model_field = runtime.symbol("model");
    let RoundScript {
        entities,
        objects,
        shown,
        collision_settlement,
        actors,
        ..
    } = &mut *runtime;
    let mut wanted = Vec::new();
    for (&object, entity) in entities {
        let Some(presence) = entity.presence else {
            continue;
        };
        let fields = objects.get(&object);
        let vector = |symbol| match fields.and_then(|fields| fields.get(&symbol)) {
            Some(Value::Vector(value)) => *value,
            _ => [0.; 3],
        };
        let origin = vector(origin_field);
        let angles = vector(angles_field);
        if let Some(placements) = placements.as_deref_mut() {
            placements.0.push((
                presence,
                object,
                CollisionPlacement {
                    origin,
                    angles,
                    hidden: entity.hidden,
                    solid: entity.solid,
                    contents: entity.contents,
                },
            ));
        }
        let model = match fields.and_then(|fields| fields.get(&model_field)) {
            Some(Value::String(model)) if !model.is_empty() && !model.starts_with('*') => {
                Some(model)
            }
            _ => None,
        };
        let hidden = entity.hidden;
        let shown_to = entity.shown_to;
        let solid = entity.solid;
        let collision_only = matches!(entity.kind, EntityKind::Missile(_));
        let contents = entity.contents;
        let aim_assist = entity.aim_assist;
        let actor = actors.get(&object);
        let hero_lighting = entity.hero_only_lighting || actor.is_some();
        let less_frequent_lighting = entity.less_frequent_lighting;
        let thermal_draw = actor.is_some_and(|actor| actor.thermal_draw);
        if let Some(inputs) = settled_inputs.as_deref_mut() {
            inputs.push(CollisionInput {
                object,
                presence,
                shown: Shown {
                    origin,
                    angles,
                    model: model.map(|model| model.clone().into()),
                    attachments: entity.attachments.clone(),
                    hidden,
                    thermal_draw,
                    hero_lighting,
                    less_frequent_lighting,
                    aim_assist,
                    shown_to,
                    solid,
                    contents,
                    moving: false,
                },
            });
        }
        let unchanged = presentation_matches(
            if settling {
                collision_settlement.input(object).map(|row| &row.shown)
            } else {
                shown.get(&object)
            },
            CollisionSource {
                object,
                presence,
                origin,
                angles,
                model: model.map(|model| &**model),
                attachments: &entity.attachments,
                hidden,
                thermal_draw,
                hero_lighting,
                less_frequent_lighting,
                aim_assist,
                shown_to,
                solid,
                contents,
                has_operations: entity.anim_op.is_some(),
            },
        );
        if unchanged {
            continue;
        }
        wanted.push(Wanted {
            collision_only,
            object,
            presence,
            origin,
            angles,
            model: model.map(|model| model.clone().into()),
            attachments: entity.attachments.clone(),
            hidden,
            thermal_draw,
            hero_lighting,
            less_frequent_lighting,
            aim_assist,
            shown_to,
            solid,
            contents,
            anim_op: entity.anim_op.take(),
        });
    }
    wanted
}

fn present_model(frame: &mut FrameWorld, want: &Wanted) {
    let model_pose = frame
        .content()
        .simulation_model_transform(want.origin, want.angles);
    let capability = want
        .model
        .as_deref()
        .and_then(|model| frame.model_capability(model))
        .flatten();
    let anim = want
        .anim_op
        .as_ref()
        .and_then(|op| op.as_deref().and_then(|clip| frame.script_model_anim(clip)));
    let attachments: Vec<_> = want
        .attachments
        .iter()
        .map(|(model, tag)| (&**model, &**tag, frame.model_capability(model).flatten()))
        .collect();
    let Some(row) = frame.collision_owner_mut(want.presence) else {
        return;
    };
    match (&want.model, row.dobj.as_mut()) {
        (None, _) => row.dobj = None,
        (Some(model), Some(dobj)) => {
            if dobj.current_model != **model {
                dobj.replace_model(model, capability);
            }
        }
        (Some(model), None) => {
            let Some(model_pose) = model_pose else {
                return;
            };
            row.dobj = Some(AuthorityDObjState::new_dirty(
                model.to_string(),
                capability,
                model_pose,
            ));
            row.followed_pose = Some((want.origin, want.angles));
        }
    }
    let Some(dobj) = row.dobj.as_mut() else {
        return;
    };
    dobj.set_attachments(&attachments);
    match &want.anim_op {
        Some(Some(clip)) => {
            let anim = anim.unwrap_or(crate::ScriptModelPlayAnim {
                looping: false,
                frequency: 0.0,
            });
            dobj.begin_script_model_play_anim(clip, anim.looping, anim.frequency);
        }
        Some(None) => dobj.clear_script_model_play_anim(),
        None => {}
    }
}

const UNPRESENTED_LOOP_OWNER: u32 = 0x2000_0000;

fn publish_loop_sounds(world: &mut World) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let speaking: Vec<(u64, Option<ScriptModelId>, Arc<str>, i32)> = runtime
        .entities
        .iter()
        .filter_map(|(object, e)| Some((*object, e.presence, e.loop_sound.clone()?, e.number)))
        .collect();
    let placed: SpeakingModels = speaking
        .into_iter()
        .map(|(object, presence, alias, number)| {
            let owner = presence.unwrap_or_else(|| {
                ScriptModelId::from_wire(UNPRESENTED_LOOP_OWNER | (object as u32 & 0x0fff_ffff))
            });
            (
                owner,
                alias,
                vector(&mut runtime, object, "origin"),
                u32::try_from(number).ok(),
            )
        })
        .collect();
    let mut placed: Vec<_> = placed
        .into_iter()
        .map(|(owner, alias, origin, number)| (owner, alias, origin, number, 1.0))
        .collect();
    // Each vehicle engine has separate source keys from its script loop voice.
    const VEHICLE_ENGINE_LOOP_OWNER: u32 = 0x3000_0000;
    for (object, slot, alias, gain) in super::vehicles::engine_sounds(&runtime) {
        if let Some(entity) = runtime.entities.get(&object) {
            let owner = ScriptModelId::from_wire(
                VEHICLE_ENGINE_LOOP_OWNER | ((object as u32 & 0x01ff_ffff) << 3) | u32::from(slot),
            );
            let number = u32::try_from(entity.number).ok();
            let origin = vector(&mut runtime, object, "origin");
            placed.push((owner, alias, origin, number, gain));
        }
    }
    let mut frame = FrameWorld::from_world(world);
    let rows = placed
        .into_iter()
        .map(
            |(owner, alias, origin, snd_ent, gain)| crate::world_objects::DestructibleLoopSound {
                snd_ent,
                gain,
                owner,
                alias_index: frame.sound_alias_index(&alias),
                origin,
            },
        )
        .collect();
    frame.world_objects_mut().set_destructible_loop_sounds(rows);
}

impl RoundScript {
    // GSC can link solid visual models to a missile (for example its grenade indicator).
    // Those models remain hittable, but must not obstruct their own parent missile.
    pub(crate) fn missile_collision_models(&self, id: crate::ProjectileId) -> Vec<ScriptModelId> {
        let Some(&missile) = self.missiles.get(&id) else {
            return Vec::new();
        };
        self.entities
            .iter()
            .filter_map(|(&object, entity)| {
                let presence = entity.presence?;
                let mut parent = object;
                for _ in 0..self.entities.len() {
                    if parent == missile {
                        return Some(presence);
                    }
                    parent = self.entities.get(&parent)?.linked_to.as_ref()?.parent;
                }
                None
            })
            .collect()
    }

    pub fn presence_of(&self, value: &Value) -> Option<ScriptModelId> {
        self.entity(value).and_then(|(_, e)| e.presence)
    }

    pub(crate) fn presented_by(&self, id: ScriptModelId) -> Option<u64> {
        self.entities
            .iter()
            .find(|(_, e)| e.presence == Some(id) && e.kind != EntityKind::HudElem)
            .map(|(object, _)| *object)
    }
}
