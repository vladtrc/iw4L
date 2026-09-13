use bevy::prelude::*;
use entity_iw4::{
    EntityEventAction, EntityEventKind, UnsupportedEntityEvent, cg_entity_event_action,
    cg_packet_entity_uses_event_ring, consume_entity_events,
};
use sim::{EntityEventPayload, EventSequence, Tick};

use crate as net;
use crate::CEntity;
use crate::CEntitySlots;
use crate::client::centity_runtime::CEntityRuntime;
use crate::client::entity_event_registry::{EntityEventDispatch, ev_dispatch_row};
use crate::client::input::ClientActionInput;
use crate::client::presented::LocalPresentClient;
use crate::client::runtime::{LastAdoptedSnapshot, PendingPresentedEntityEvents};
use crate::gaps::{NetGapCause, NetIdentityGaps};
use crate::schedule::ClientSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DispatchedEntityEvent {
    pub sequence: EventSequence,
    pub tick: Tick,
    pub event: EntityEventKind,
    pub payload: EntityEventPayload,
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
pub struct EntityPlayFx {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityObituary {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
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

#[derive(Resource, Default, Debug)]
pub struct EntityEventCursor {
    seen_through: u32,
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
                    row.retail_name,
                    match row.dispatch {
                        EntityEventDispatch::Unsupported(reason) => reason,
                        EntityEventDispatch::Observer(_) => {
                            "declared as classified in EV_DISPATCH_REGISTRY, yet the classifier \
                             rejected it — G-BUS-1 should have caught this"
                        }
                    },
                ),
                None => (
                    "unmodelled",
                    "not in entity_iw4::EntityEventKind::TAXONOMY: a retail table slot this build \
                     does not model at all",
                ),
            };
            diag::warn!(
                Net,
                "entity events: {} (#{:#x}) has no ported CG_EntityEvent branch — {} \
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

    pub const fn seen_through(&self) -> EventSequence {
        EventSequence(self.seen_through)
    }
}

fn dispatch_entity_events(
    mut commands: Commands,
    adopted: Res<LastAdoptedSnapshot>,
    mut pending: ResMut<PendingPresentedEntityEvents>,
    local: Res<LocalPresentClient>,
    slots: Res<CEntitySlots>,
    mut cursor: ResMut<EntityEventCursor>,
    mut walk: ResMut<AppliedEntityEventWalk>,
    mut unsupported: ResMut<UnsupportedEntityEvents>,
    mut target_gaps: ResMut<NetIdentityGaps>,
    mut runtimes: Query<(Entity, &CEntity, &mut CEntityRuntime)>,
) {
    walk.walked = 0;
    walk.dispatched = 0;
    walk.local_fire = 0;
    walk.occupancy_fired = 0;
    walk.last_event = 0;
    walk.last_number = 0;
    walk.seen_through = cursor.seen_through;
    if !adopted.applied_this_frame {
        return;
    }
    let tick = adopted
        .next()
        .map(|snapshot| snapshot.tick)
        .unwrap_or(Tick(0));
    let local_number = i32::try_from(local.0.0).unwrap_or(-1);
    for (entity, identity, mut runtime) in runtimes.iter_mut() {
        if !cg_packet_entity_uses_event_ring(runtime.next_state.e_type) {
            continue;
        }

        let next_state = runtime.next_state;
        let origin = runtime.origin;
        let mut cursor = runtime.previous_event_sequence;
        consume_entity_events(&next_state, &mut cursor, |ev| {
            walk.occupancy_fired = walk.occupancy_fired.saturating_add(1);
            let number = i32::from(identity.number());
            dispatch_classified(
                &mut commands,
                local_number,
                number,
                entity,
                DispatchedEntityEvent {
                    sequence: EventSequence(u32::try_from(ev.sequence).unwrap_or(0)),
                    tick,
                    event: ev.event,
                    payload: EntityEventPayload {
                        number,
                        event_parm: ev.event_parm,
                        origin,
                        surf_type: (ev.event_parm & 0x1f) as u8,
                        weapon: u32::try_from(next_state.index).unwrap_or(0),
                        ..Default::default()
                    },
                },
                &mut walk,
                &mut unsupported,
            );
        });
        runtime.previous_event_sequence = cursor;
    }
    for record in pending.0.iter() {
        walk.walked = walk.walked.saturating_add(1);
        if !cursor.accepts(record, local.0) {
            continue;
        }
        walk.last_event = record.event.0;
        walk.last_number = record.payload.number;

        let dispatched = DispatchedEntityEvent {
            sequence: record.sequence,
            tick: record.tick,
            event: record.event,
            payload: record.payload,
        };
        let number = record.payload.number;
        let resolved = match u16::try_from(number) {
            Ok(number) => slots.entity_for_number(number),
            Err(_) => {
                target_gaps.raise(NetGapCause::EventNumberOutOfRange { number });
                continue;
            }
        };

        let entity = match resolved {
            Some(entity) => entity,
            None if origin_space_without_centity(record.event) => Entity::PLACEHOLDER,
            None => {
                target_gaps.raise(NetGapCause::EventNumberHasNoEntity { number });
                continue;
            }
        };
        dispatch_classified(
            &mut commands,
            local_number,
            number,
            entity,
            dispatched,
            &mut walk,
            &mut unsupported,
        );
    }
    pending.0.clear();
    walk.seen_through = cursor.seen_through;
}

fn dispatch_classified(
    commands: &mut Commands,
    local_number: i32,
    number: i32,
    entity: Entity,
    dispatched: DispatchedEntityEvent,
    walk: &mut AppliedEntityEventWalk,
    unsupported: &mut UnsupportedEntityEvents,
) {
    let mut did = false;
    match cg_entity_event_action(dispatched.event) {
        Ok(EntityEventAction::None) => {}
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

fn origin_space_without_centity(event: EntityEventKind) -> bool {
    matches!(cg_entity_event_action(event), Ok(EntityEventAction::PlayFx))
        || event == EntityEventKind::SOUND_ALIAS
        || event == EntityEventKind::SOUND_ALIAS_AS_MASTER
}

fn cl_set_ads_from_reset(
    reset: On<net::EntityResetAds>,
    mut input: ResMut<ClientActionInput>,
    local: Res<LocalPresentClient>,
) {
    if (*reset).event.payload.number != local.0.0 as i32 {
        return;
    }
    input_iw4::cl_set_ads(&mut input.client, false);
}

pub fn register_entity_event_dispatch(app: &mut App) {
    app.init_resource::<EntityEventCursor>()
        .init_resource::<AppliedEntityEventWalk>()
        .init_resource::<UnsupportedEntityEvents>()
        .init_resource::<NetIdentityGaps>()
        .init_resource::<WeaponFirePingBus>()
        .add_observer(cl_set_ads_from_reset)
        .add_systems(
            Update,
            dispatch_entity_events
                .in_set(ClientSet::Reconcile)
                .after(crate::sync_client_entities),
        );
}
