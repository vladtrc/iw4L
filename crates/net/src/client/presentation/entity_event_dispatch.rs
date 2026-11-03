use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use entity_iw4::{
    EntityEventAction, EntityEventKind, UnsupportedEntityEvent, consume_entity_events,
    entity_event_action, packet_entity_uses_event_ring,
};
use frame::ScopeApp;
use sim::{EntityEventPayload, EventSequence, Tick};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate as net;
use crate::CEntity;
use crate::CEntitySlots;
use crate::client::input::ClientActionInput;
use crate::client::presentation::centity_runtime::CEntityRuntime;
use crate::client::presentation::entity_event_registry::{EntityEventDispatch, ev_dispatch_row};
use crate::client::presentation::presented::LocalPresentClient;
use crate::client::presentation::projectiles::{
    expire_predicted_projectiles, spawn_predicted_projectile,
};
use crate::client::runtime::{
    ClientPredictionState, LastAdoptedSnapshot, PendingPresentedEntityEvents,
};
use crate::schedule::ClientSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityEventDomain {
    Snapshot,
    Ring,
    Predicted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DispatchedEntityEvent {
    pub domain: EntityEventDomain,
    pub sequence: EventSequence,
    pub tick: Tick,
    pub event: EntityEventKind,
    pub payload: EntityEventPayload,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityRumble {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityEventSound {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityWeaponFire {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(Clone, Copy, Debug)]
pub struct WeaponFirePing {
    pub number: i32,
    pub origin_xy: [f32; 2],
}

#[derive(Resource, Default, Debug)]
pub struct WeaponFirePingBus {
    pub pings: Vec<WeaponFirePing>,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityEjectBrass {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityBulletHit {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityGrenadeContact {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityExplosion {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityPhysicsSphere {
    pub entity: Entity,
    pub origin: [f32; 3],
    pub outer_radius: f32,
    pub inner_radius: f32,
    pub magnitude: f32,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityPlayFx {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityObituary {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
    pub in_killcam: bool,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityMovementSound {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityResetAds {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityMeleeBlood {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

const TARGET_RETRY_CAPACITY: usize = 8192;
const TARGET_RETRY_WORK: usize = 256;
const TARGET_RETRY_WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug)]
struct DeferredTarget {
    client: sim::ClientId,
    life: sim::LifeSequence,
    deadline: Instant,
}

#[derive(Debug)]
struct DeferredEntityEvent {
    record: sim::EntityEventRecord,
    target: DeferredTarget,
}

#[derive(Resource, Debug, Default)]
pub struct EntityEventCursor {
    seen_through: u32,

    archived_through: Option<(EventSequence, Tick)>,
    start_tick: Option<Tick>,
    predicted_shots: u32,
    deferred: VecDeque<DeferredEntityEvent>,
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedEntityEventWalk {
    pub walked: u32,
    pub dispatched: u32,
    pub local_fire: u32,
    pub seen_through: u32,

    pub occupancy_fired: u32,

    pub last_event: i32,

    pub last_number: i32,
}

#[derive(Resource, Default, Debug)]
pub struct UnsupportedEntityEvents {
    pub total: u32,

    pub first: Option<EntityEventKind>,
    warned: bool,
}

impl UnsupportedEntityEvents {
    fn note(&mut self, event: EntityEventKind) {
        self.total = self.total.saturating_add(1);
        if self.first.is_none() {
            self.first = Some(event);
        }
        if !self.warned {
            self.warned = true;

            let (name, why) = match ev_dispatch_row(event) {
                Some(row) => (
                    row.name,
                    match row.dispatch {
                        EntityEventDispatch::Unsupported(reason) => reason,
                        EntityEventDispatch::Observer(_) | EntityEventDispatch::OwnerInput => {
                            "declared as classified in EV_DISPATCH_REGISTRY, yet the classifier \
                             rejected it — G-BUS-1 should have caught this"
                        }
                    },
                ),
                None => (
                    "unmodelled",
                    "not in entity_iw4::EntityEventKind::TAXONOMY: a table slot this build \
                     does not model at all",
                ),
            };
            diag::warn!(
                Net,
                "entity events: {} (#{:#x}) has no client handler — {} \
                 (further unsupported events counted, not printed)",
                name,
                event.0,
                why
            );
        }
    }
}

impl EntityEventCursor {
    pub fn accepts(&mut self, record: &sim::EntityEventRecord, local: sim::ClientId) -> bool {
        if !record.audience.projects_to(local) {
            return false;
        }
        if !record
            .sequence
            .is_newer_than(EventSequence(self.seen_through))
        {
            return false;
        }
        self.seen_through = record.sequence.0;
        true
    }

    pub fn accepts_archived(
        &mut self,
        record: &sim::EntityEventRecord,
        local: sim::ClientId,
    ) -> bool {
        if !record.audience.projects_to(local) {
            return false;
        }
        if let Some((sequence, _)) = self.archived_through
            && !record.sequence.is_newer_than(sequence)
        {
            return false;
        }
        self.archived_through = Some((record.sequence, record.tick));
        true
    }

    pub const fn seen_through(&self) -> EventSequence {
        EventSequence(self.seen_through)
    }
}

#[derive(SystemParam)]
struct ClientEventDelivery<'w, 's> {
    commands: Commands<'w, 's>,
    adopted: Res<'w, LastAdoptedSnapshot>,
    local: Res<'w, LocalPresentClient>,
    slots: Res<'w, CEntitySlots>,
    cursor: ResMut<'w, EntityEventCursor>,
    walk: ResMut<'w, AppliedEntityEventWalk>,
    unsupported: ResMut<'w, UnsupportedEntityEvents>,
}

fn dispatch_entity_events(
    mut delivery: ClientEventDelivery,
    pending: ResMut<PendingPresentedEntityEvents>,
    mut runtimes: Query<(Entity, &CEntity, &mut CEntityRuntime)>,
    role: Res<frame::RuntimeRole>,
) {
    delivery.deliver_snapshot(pending, &mut runtimes, role);
}

fn dispatch_owner_events(
    mut delivery: ClientEventDelivery,
    prediction: ResMut<ClientPredictionState>,
    pellet_fx: ResMut<crate::PendingPelletFx>,
    timeline: Res<State<frame::Timeline>>,
    (collision, level): (Res<crate::ClientCollision>, Res<crate::ClientLevel>),
) {
    delivery.deliver_owner(prediction, pellet_fx, timeline, &collision, &level);
}

impl ClientEventDelivery<'_, '_> {
    fn deliver_snapshot(
        &mut self,
        mut pending: ResMut<PendingPresentedEntityEvents>,
        runtimes: &mut Query<(Entity, &CEntity, &mut CEntityRuntime)>,
        role: Res<frame::RuntimeRole>,
    ) {
        // Replay never predicts, so the recorded owner fire is the only one.
        let owner_predicted = *role != frame::RuntimeRole::Replay;
        self.walk.walked = 0;
        self.walk.dispatched = 0;
        self.walk.local_fire = 0;
        self.walk.occupancy_fired = 0;
        self.walk.last_event = 0;
        self.walk.last_number = 0;
        self.walk.seen_through = self.cursor.seen_through;
        if !self.adopted.applied_this_frame && self.cursor.deferred.is_empty() {
            return;
        }
        let tick = self
            .adopted
            .next()
            .map(|snapshot| snapshot.view_tick(self.local.0))
            .unwrap_or(Tick(0));
        let local_number = i32::try_from(self.local.0.0).unwrap_or(-1);
        if self.cursor.start_tick.is_none() {
            self.cursor.start_tick = self
                .adopted
                .next()
                .map(|snapshot| snapshot.view_tick(self.local.0));
        }
        for (entity, identity, mut runtime) in runtimes.iter_mut() {
            if !packet_entity_uses_event_ring(runtime.next_state.e_type) {
                continue;
            }

            let next_state = runtime.next_state;
            let origin = runtime.origin;
            let mut cursor = runtime.previous_event_sequence;
            consume_entity_events(&next_state, &mut cursor, |ev| {
                self.walk.occupancy_fired = self.walk.occupancy_fired.saturating_add(1);
                let number = i32::from(identity.number());
                EventView {
                    local_number,
                    in_killcam: pending.in_killcam,
                    owner_predicted,
                }
                .dispatch(
                    &mut self.commands,
                    number,
                    entity,
                    DispatchedEntityEvent {
                        domain: EntityEventDomain::Ring,
                        sequence: EventSequence(u32::try_from(ev.sequence).unwrap_or(0)),
                        tick,
                        event: ev.event,
                        payload: EntityEventPayload {
                            number,
                            other_entity_num: next_state.other_entity_num,
                            attacker_entity_num: next_state.attacker_entity_num,
                            event_parm: ev.event_parm,
                            origin,
                            surf_type: (ev.event_parm & 0x1f) as u8,
                            weapon: u32::try_from(next_state.index).unwrap_or(0),
                            ..Default::default()
                        },
                    },
                    &mut self.walk,
                    &mut self.unsupported,
                );
            });
            runtime.previous_event_sequence = cursor;
        }
        let in_killcam = pending.in_killcam;
        let pending = &mut *pending;
        let count = self.cursor.deferred.len().min(TARGET_RETRY_WORK);
        let remaining = self.cursor.deferred.split_off(count);
        let retry = std::mem::replace(&mut self.cursor.deferred, remaining);
        let records = retry
            .into_iter()
            .map(|retry| (false, retry.record, Some(retry.target)))
            .chain(pending.live.drain(..).map(|record| (false, record, None)))
            .chain(
                pending
                    .archived
                    .drain(..)
                    .map(|record| (true, record, None)),
            );
        let now = Instant::now();
        for (archived, record, retry) in records {
            if archived != in_killcam
                || self
                    .cursor
                    .start_tick
                    .is_some_and(|start| record.tick.0 < start.0)
            {
                continue;
            }
            self.walk.walked = self.walk.walked.saturating_add(1);
            if let Some(target) = retry {
                if !record.audience.projects_to(self.local.0) {
                    continue;
                }
                let number = record.payload.number;
                if now >= target.deadline {
                    diag::warn_once!(
                        Net,
                        "event {sequence} for entity {number} expired while waiting for its target",
                        sequence = (record.sequence).0
                    );
                    continue;
                }
                let life = self
                    .adopted
                    .next()
                    .and_then(|snapshot| snapshot.meta.for_client(target.client))
                    .map(|meta| meta.life_sequence);
                if life != Some(target.life) {
                    diag::warn_once!(
                        Net,
                        "event for entity {number} requires life {expected}, current life is {current:?}",
                        expected = (target.life).0,
                        current = life
                    );
                    continue;
                }
            }
            let accepted = if retry.is_some() {
                true
            } else if archived {
                self.cursor.accepts_archived(&record, self.local.0)
            } else {
                self.cursor.accepts(&record, self.local.0)
            };
            if !accepted {
                continue;
            }
            self.walk.last_event = record.event.0;
            self.walk.last_number = record.payload.number;

            let dispatched = DispatchedEntityEvent {
                domain: EntityEventDomain::Snapshot,
                sequence: record.sequence,
                tick: record.tick,
                event: record.event,
                payload: record.payload,
            };
            let number = record.payload.number;
            if !archived
                && let Some(cause) = record.payload.fire_cause
                && i32::try_from(cause.client.0).ok() == Some(number)
            {
                let life = self
                    .adopted
                    .next()
                    .and_then(|snapshot| snapshot.meta.for_client(cause.client))
                    .map(|meta| meta.life_sequence);
                if life.is_some_and(|life| life != cause.life) {
                    diag::warn_once!(
                        Net,
                        "event for entity {number} requires life {expected}, current life is {current:?}",
                        expected = (cause.life).0,
                        current = life
                    );
                    continue;
                }
            }
            let resolved = match u16::try_from(number) {
                Ok(number) => self.slots.entity_for_number(number),
                Err(_) => {
                    diag::warn_once!(Net, "entity number {number} is not a u16 entity number");
                    continue;
                }
            };

            let entity = match super::entity_event_recipient::recipient(record.event, resolved) {
                Some(entity) => entity,
                None => {
                    diag::warn_once!(Net, "entity number {number} has no live entity");
                    if !archived {
                        let target = retry.or_else(|| {
                            let client = sim::ClientId(u32::try_from(number).ok()?);
                            let life = self.adopted.next()?.meta.for_client(client)?.life_sequence;
                            Some(DeferredTarget {
                                client,
                                life,
                                deadline: now + TARGET_RETRY_WAIT,
                            })
                        });
                        if let Some(target) = target {
                            if self.cursor.deferred.len() == TARGET_RETRY_CAPACITY {
                                diag::warn_once!(
                                    Net,
                                    "event {sequence} for entity {number} exceeded the target retry budget",
                                    sequence = (record.sequence).0
                                );
                            } else {
                                self.cursor
                                    .deferred
                                    .push_back(DeferredEntityEvent { record, target });
                            }
                        }
                    }
                    continue;
                }
            };
            EventView {
                local_number,
                in_killcam: archived,
                owner_predicted,
            }
            .dispatch(
                &mut self.commands,
                number,
                entity,
                dispatched,
                &mut self.walk,
                &mut self.unsupported,
            );
        }
        self.walk.seen_through = self.cursor.seen_through;
    }

    fn deliver_owner(
        &mut self,
        mut prediction: ResMut<ClientPredictionState>,
        mut pellet_fx: ResMut<crate::PendingPelletFx>,
        timeline: Res<State<frame::Timeline>>,
        collision: &crate::ClientCollision,
        level: &crate::ClientLevel,
    ) {
        if *timeline.get() == frame::Timeline::Killcam {
            prediction.0.take_actions();
            return;
        }
        let local_number = self.local.0.0 as i32;
        let Some(entity) = u16::try_from(local_number)
            .ok()
            .and_then(|number| self.slots.entity_for_number(number))
        else {
            return;
        };
        let life = prediction.0.local_life();
        let roundtrip = prediction.0.roundtrip();
        let actions = prediction.0.take_actions();
        let mut launches = Vec::new();
        let mut emit =
            |tick: sim::Tick, event: EntityEventKind, payload: sim::EntityEventPayload| {
                if entity_event_action(event) == Ok(EntityEventAction::WeaponFire) {
                    self.cursor.predicted_shots = self.cursor.predicted_shots.wrapping_add(1);
                    perf::instant!("owner_shot", "iw4l.sim", |ctx| {
                        ctx.arg("source", "predicted");
                        ctx.arg("ordinal", i64::from(self.cursor.predicted_shots));
                        ctx.arg("tick", i64::from(tick.0));
                    });
                }
                let sequence = prediction.0.next_owner_event();
                EventView {
                    local_number,
                    in_killcam: false,
                    owner_predicted: true,
                }
                .dispatch(
                    &mut self.commands,
                    local_number,
                    entity,
                    DispatchedEntityEvent {
                        domain: EntityEventDomain::Predicted,
                        sequence,
                        tick,
                        event,
                        payload,
                    },
                    &mut self.walk,
                    &mut self.unsupported,
                );
            };
        for (tick, action) in actions {
            match action {
                weapon_iw4::WeaponAction::Fire(fire) => {
                    if fire.cause.is_some_and(|cause| Some(cause.life) != life) {
                        continue;
                    }
                    perf::instant!("predicted_weapon_fire", "iw4l.sim", |ctx| {
                        ctx.arg("client", i64::from(self.local.0.0));
                        if let Some(cause) = fire.cause {
                            ctx.arg("life", i64::from(cause.life.0));
                            ctx.arg("command", i64::from(cause.command.0));
                            ctx.arg("ordinal", i64::from(cause.ordinal));
                            ctx.arg("hand", i64::from(cause.hand));
                        }
                    });
                    emit(
                        tick,
                        entity_iw4::predicted_weapon_fire_event(fire.hand as i32, fire.last_shot),
                        sim::EntityEventPayload {
                            number: local_number,
                            weapon: fire.weapon,
                            correlation: fire.shot_id.0,
                            fire_cause: fire.cause,
                            hand: fire.hand,
                            origin: fire.origin,
                            direction: fire.angles,
                            ..Default::default()
                        },
                    );
                    if let Some(snapshot) = self.adopted.next() {
                        let effects = sim::owner_shot_effects(
                            &collision.0,
                            snapshot,
                            tick,
                            self.local.0,
                            &fire,
                        );
                        for (event, payload) in effects.events {
                            emit(tick, event, payload);
                        }
                        pellet_fx.0.extend(effects.pellets);
                    }
                    if let (Some(cause), Some(combat_seed)) = (fire.cause, fire.combat_seed)
                        && let Some(launch) =
                            sim::shot_launch_motion(&level.content, tick, &fire, combat_seed)
                    {
                        launches.push((cause, fire.weapon, tick, launch));
                    }
                }
                weapon_iw4::WeaponAction::OffhandUsed {
                    weapon,
                    remaining_fuse_ms,
                    cause: Some(cause),
                    combat_seed: Some(combat_seed),
                    origin,
                    angles,
                    velocity,
                } => {
                    if Some(cause.life) != life {
                        continue;
                    }
                    let Some(facts) = level.content.equipment_facts_for(weapon) else {
                        continue;
                    };
                    let launch = sim::ProjectileLaunch {
                        tick,
                        origin,
                        angles,
                        owner_velocity: velocity,
                        combat_seed,
                    }
                    .grenade_motion(&facts, sim::GrenadeLaunchKind::Thrown { remaining_fuse_ms });
                    launches.push((cause, weapon, tick, launch));
                }
                weapon_iw4::WeaponAction::Brass {
                    weapon,
                    hand,
                    pending,
                    origin,
                } => {
                    if pending.is_some_and(|pending| Some(pending.life) != life) {
                        continue;
                    }
                    emit(
                        tick,
                        if hand == 1 {
                            EntityEventKind::EJECT_BRASS_LEFT
                        } else {
                            EntityEventKind::EJECT_BRASS
                        },
                        sim::EntityEventPayload {
                            number: local_number,
                            weapon,
                            hand,
                            origin,
                            correlation: pending.map_or(0, |pending| pending.shot.0),
                            fire_cause: pending.and_then(|pending| pending.cause),
                            ..Default::default()
                        },
                    );
                }
                _ => {}
            }
        }
        for (cause, weapon, tick, launch) in launches {
            spawn_predicted_projectile(&mut self.commands, cause, weapon, tick, launch, roundtrip);
        }
    }
}

#[derive(Clone, Copy)]
struct EventView {
    local_number: i32,
    in_killcam: bool,
    owner_predicted: bool,
}

impl EventView {
    fn dispatch(
        self,
        commands: &mut Commands,
        number: i32,
        entity: Entity,
        dispatched: DispatchedEntityEvent,
        walk: &mut AppliedEntityEventWalk,
        unsupported: &mut UnsupportedEntityEvents,
    ) {
        let Self {
            local_number,
            in_killcam,
            owner_predicted,
        } = self;
        let action = entity_event_action(dispatched.event);
        let owner_replica = owner_predicted
            && !in_killcam
            && dispatched.domain != EntityEventDomain::Predicted
            && number == local_number;
        let world_hit = !(0..playerstate_iw4::PLAYER_CORPSE_ENTITY_BASE)
            .contains(&dispatched.payload.other_entity_num);
        if owner_replica
            && (matches!(
                action,
                Ok(EntityEventAction::WeaponFire | EntityEventAction::EjectBrass)
            ) || action == Ok(EntityEventAction::BulletHit) && world_hit)
        {
            return;
        }
        if matches!(
            action,
            Ok(EntityEventAction::WeaponFire
                | EntityEventAction::EjectBrass
                | EntityEventAction::BulletHit)
        ) {
            perf::instant!("weapon_fx_event", "iw4l.sim", |ctx| {
                ctx.arg("entity", entity.to_bits());
                ctx.arg("number", i64::from(number));
                ctx.arg("local", i64::from(local_number));
                ctx.arg("archived", in_killcam);
                ctx.arg("sequence", i64::from(dispatched.sequence.0));
                ctx.arg("tick", i64::from(dispatched.tick.0));
                ctx.arg(
                    "domain",
                    match dispatched.domain {
                        EntityEventDomain::Predicted => "predicted",
                        EntityEventDomain::Snapshot => "snapshot",
                        EntityEventDomain::Ring => "ring",
                    },
                );
                ctx.arg(
                    "kind",
                    match action {
                        Ok(EntityEventAction::WeaponFire) => "fire",
                        Ok(EntityEventAction::EjectBrass) => "brass",
                        _ => "hit",
                    },
                );
                ctx.arg("world_hit", world_hit);
            });
        }
        let mut did = false;
        match entity_event_action(dispatched.event) {
            Ok(EntityEventAction::None | EntityEventAction::OwnerStance) => {}
            Ok(EntityEventAction::Sound) => {
                did = true;
                commands.trigger(EntityEventSound {
                    entity,
                    event: dispatched,
                });
            }
            Ok(EntityEventAction::WeaponFire) => {
                did = true;
                if number == local_number {
                    walk.local_fire = walk.local_fire.saturating_add(1);
                }
                commands.trigger(EntityWeaponFire {
                    entity,
                    event: dispatched,
                });
            }
            Ok(EntityEventAction::EjectBrass) => {
                did = true;
                commands.trigger(EntityEjectBrass {
                    entity,
                    event: dispatched,
                });
            }
            Ok(EntityEventAction::BulletHit) => {
                did = true;
                commands.trigger(EntityBulletHit {
                    entity,
                    event: dispatched,
                });
            }
            Ok(EntityEventAction::GrenadeContact) => {
                did = true;
                commands.trigger(EntityGrenadeContact {
                    entity,
                    event: dispatched,
                });
            }
            Ok(EntityEventAction::Explosion) => {
                did = true;
                commands.trigger(EntityExplosion {
                    entity,
                    event: dispatched,
                });
            }
            Ok(EntityEventAction::Rumble) => {
                did = true;
                commands.trigger(EntityRumble {
                    entity,
                    event: dispatched,
                });
            }
            Ok(EntityEventAction::PhysicsSphere) => {
                did = true;
                let [outer_radius, inner_radius, magnitude] = dispatched.payload.origin2;
                commands.trigger(EntityPhysicsSphere {
                    entity,
                    origin: dispatched.payload.origin,
                    outer_radius,
                    inner_radius,
                    magnitude,
                });
            }
            Ok(EntityEventAction::PlayFx) => {
                did = true;
                commands.trigger(EntityPlayFx {
                    entity,
                    event: dispatched,
                });
            }
            Ok(EntityEventAction::Obituary) => {
                did = true;
                commands.trigger(EntityObituary {
                    entity,
                    event: dispatched,
                    in_killcam,
                });
            }
            Ok(EntityEventAction::MovementSound) => {
                did = true;
                commands.trigger(EntityMovementSound {
                    entity,
                    event: dispatched,
                });
            }
            Ok(EntityEventAction::ResetAds) => {
                did = true;
                commands.trigger(EntityResetAds {
                    entity,
                    event: dispatched,
                });
            }
            Ok(EntityEventAction::MeleeBlood) => {
                did = true;
                commands.trigger(EntityMeleeBlood {
                    entity,
                    event: dispatched,
                });
            }
            Err(UnsupportedEntityEvent(event)) => {
                did = true;
                unsupported.note(event);
            }
        }
        if did {
            walk.dispatched = walk.dispatched.saturating_add(1);
        }
    }
}

fn set_ads_from_reset(
    reset: On<net::EntityResetAds>,
    mut input: ResMut<ClientActionInput>,
    local: Res<LocalPresentClient>,
) {
    if (*reset).event.payload.number != local.0.0 as i32 {
        return;
    }
    input_iw4::set_ads(&mut input.client, false);
}

pub fn register_entity_event_dispatch(app: &mut App) {
    app.scoped::<EntityEventCursor>(frame::Timeline::Live)
        .init_resource::<AppliedEntityEventWalk>()
        .init_resource::<UnsupportedEntityEvents>()
        .init_resource::<WeaponFirePingBus>()
        .add_observer(set_ads_from_reset)
        .add_systems(
            Update,
            (
                dispatch_entity_events
                    .in_set(frame::InMatch)
                    .in_set(ClientSet::Reconcile)
                    .after(crate::sync_client_entities),
                crate::client::runtime::publish_accepted_impacts
                    .in_set(frame::InMatch)
                    .in_set(frame::OwnerEventsPublished)
                    .after(dispatch_owner_events),
                dispatch_owner_events
                    .in_set(frame::InMatch)
                    .in_set(frame::OwnerEventsPublished)
                    .after(crate::predict_local_move),
                expire_predicted_projectiles
                    .in_set(frame::InMatch)
                    .after(crate::sync_client_entities),
            ),
        );
}
