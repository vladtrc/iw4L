use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::ScopeApp;
use playerstate_iw4::{ENTITYNUM_NONE, PLAYER_CORPSE_ENTITY_BASE};
use sim::{ClientId, PlayerCorpsePool, PlayerCorpseSlot};

use crate::client::presentation::centity_runtime::{
    CEntityRuntime, apply_existing, corpse_slot_to_entity_state, player_state_to_entity_state,
    reset_entity, shutdown_entity,
};
use crate::client::presentation::projectiles::{
    ClientPredictedProjectile, install_projectile_motion,
};
use crate::{AuthoritySet, ClientSet, LastAdoptedSnapshot, ServerTick};

pub const CLIENT_ENTITY_SLOT_COUNT: usize = ENTITYNUM_NONE as usize;

#[derive(Component, Clone, Copy, Debug)]
#[component(immutable)]
pub struct CEntity {
    number: u16,
    client: Option<ClientId>,
    generation: u32,
}

impl CEntity {
    pub const fn number(&self) -> u16 {
        self.number
    }

    pub const fn generation(&self) -> u32 {
        self.generation
    }

    pub const fn client(&self) -> Option<ClientId> {
        self.client
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CEntitySlot {
    client: Option<ClientId>,
    entity: Entity,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct CEntityBirthCensus {
    pub authority_births: u32,
    pub client_births: u32,
    pub client_applies: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CEntitySyncArm {
    Authority,
    Client,
}

impl CEntityBirthCensus {
    fn note_birth(&mut self, arm: CEntitySyncArm) {
        match arm {
            CEntitySyncArm::Authority => {
                self.authority_births = self.authority_births.saturating_add(1);
            }
            CEntitySyncArm::Client => {
                self.client_births = self.client_births.saturating_add(1);
            }
        }
    }

    fn note_apply(&mut self, arm: CEntitySyncArm) {
        if arm == CEntitySyncArm::Client {
            self.client_applies = self.client_applies.saturating_add(1);
        }
    }
}

#[derive(Resource, Debug)]
pub struct CEntitySlots {
    slots: Vec<Option<CEntitySlot>>,
}

impl Default for CEntitySlots {
    fn default() -> Self {
        Self {
            slots: vec![None; CLIENT_ENTITY_SLOT_COUNT],
        }
    }
}

impl CEntitySlots {
    pub fn entity_for_number(&self, number: u16) -> Option<Entity> {
        self.slots
            .get(usize::from(number))
            .and_then(|slot| slot.map(|slot| slot.entity))
    }

    pub fn entity_for_client(&self, client: ClientId) -> Option<Entity> {
        self.slots
            .iter()
            .flatten()
            .find(|slot| slot.client == Some(client))
            .map(|slot| slot.entity)
    }

    pub fn iter(&self) -> impl Iterator<Item = (u16, Option<ClientId>, Entity)> + '_ {
        self.slots.iter().enumerate().filter_map(|(number, slot)| {
            let slot = (*slot)?;
            Some((number as u16, slot.client, slot.entity))
        })
    }
}

fn player_entity_number(client: ClientId) -> Option<u16> {
    let number = usize::try_from(client.0).ok()?;
    (number < CLIENT_ENTITY_SLOT_COUNT).then_some(number as u16)
}

fn snapshot_time_ms(tick: sim::Tick) -> i32 {
    crate::ServerTime::from_tick(tick).ms()
}

#[derive(Resource, Default)]
pub struct SelectedEntityTick(Option<(sim::Tick, bool)>);

#[derive(SystemParam)]
pub struct CEntityOccupancy<'w, 's> {
    commands: Commands<'w, 's>,
    slots: ResMut<'w, CEntitySlots>,
    identities: Query<'w, 's, &'static CEntity>,
    runtimes: Query<'w, 's, &'static mut CEntityRuntime>,
    census: ResMut<'w, CEntityBirthCensus>,
}

pub fn sync_client_entities(
    mut occupancy: CEntityOccupancy,
    adopted: Res<LastAdoptedSnapshot>,
    proxy: Option<Res<crate::RemoteProxyState>>,
    clock: Option<Res<crate::FrameClock>>,
    local: Option<Res<crate::LocalPresentClient>>,
    mut selected_tick: ResMut<SelectedEntityTick>,
    predicted: Query<(Entity, &ClientPredictedProjectile)>,
) {
    let archived = adopted.next().is_some_and(|snap| {
        snap.players.iter().any(|(id, ps)| {
            local.as_ref().is_some_and(|local| *id == local.0) && !ps.is_live_frame()
        })
    });
    let selected = if archived {
        proxy
            .as_ref()
            .zip(clock.as_ref())
            .zip(local.as_ref())
            .and_then(|((proxy, clock), local)| proxy.0.snapshot_at(local.0, clock.time()))
    } else {
        None
    };
    let next = selected.as_deref().or_else(|| adopted.next());
    let key = next.map(|snap| (snap.tick, archived));
    if archived {
        if selected_tick.0 == key {
            return;
        }
    } else if !client_entity_sync_needed(adopted.applied_this_frame, next.is_some())
        && selected_tick.0 == key
    {
        return;
    }
    selected_tick.0 = key;
    occupancy.sync_snapshot(
        next,
        adopted.snap.is_some(),
        archived
            .then(|| local.as_ref().map(|local| local.0))
            .flatten(),
        CEntitySyncArm::Client,
        Some(&predicted),
    );
}

pub(crate) fn sync_authority_entities(
    mut occupancy: CEntityOccupancy,
    server_tick: Res<ServerTick>,
) {
    occupancy.sync_snapshot(
        server_tick.0.as_ref().map(|tick| &tick.snapshot),
        false,
        None,
        CEntitySyncArm::Authority,
        None,
    );
}

#[derive(Clone, Copy)]
struct WantedOccupant {
    client: Option<ClientId>,
    next_state: entity_iw4::EntityState,
}

fn occupant_from_player(client: ClientId, ps: &playerstate_iw4::PlayerState) -> WantedOccupant {
    WantedOccupant {
        client: Some(client),
        next_state: player_state_to_entity_state(client, ps),
    }
}

fn occupant_from_corpse(slot: &PlayerCorpseSlot) -> WantedOccupant {
    WantedOccupant {
        client: Some(slot.victim),
        next_state: corpse_slot_to_entity_state(slot),
    }
}

fn occupant_from_replicated(es: entity_iw4::EntityState) -> WantedOccupant {
    let client = (es.e_type == entity_iw4::ET_PLAYER)
        .then(|| u32::try_from(es.client_num).ok().map(ClientId))
        .flatten();
    WantedOccupant {
        client,
        next_state: es,
    }
}

fn client_entity_sync_needed(applied_this_frame: bool, has_next: bool) -> bool {
    applied_this_frame || !has_next
}

fn replicated_entity_occupies(es: &entity_iw4::EntityState) -> bool {
    es.e_type == entity_iw4::ET_SCRIPTMOVER
        || es.e_type == entity_iw4::ET_MISSILE
        || es.e_type == entity_iw4::ET_ITEM
        || es.e_type == entity_iw4::ET_PLAYER
}

impl CEntityOccupancy<'_, '_> {
    fn sync_snapshot(
        &mut self,
        snapshot: Option<&sim::Snapshot>,
        pair_ready: bool,
        seated_viewer: Option<ClientId>,
        arm: CEntitySyncArm,
        predicted: Option<&Query<(Entity, &ClientPredictedProjectile)>>,
    ) {
        let players = snapshot
            .map(|snapshot| snapshot.players.as_slice())
            .unwrap_or(&[]);
        let empty = PlayerCorpsePool::default();
        let corpses = snapshot
            .map(|snapshot| &snapshot.meta.corpses)
            .unwrap_or(&empty);
        let empty_entities: &[entity_iw4::EntityState] = &[];
        let entities = snapshot
            .map(|snapshot| snapshot.meta.entities.as_slice())
            .unwrap_or(empty_entities);
        let at_time_ms = snapshot
            .map(|snapshot| snapshot_time_ms(snapshot.tick))
            .unwrap_or(0);
        self.slots.set_changed();
        self.census.set_changed();
        let generations = snapshot.map(|snapshot| &snapshot.meta.generations);
        if pair_ready {
            for mut runtime in self.runtimes.iter_mut() {
                if runtime.in_next_snap() {
                    runtime.transition_copy_lerp();
                }
            }
        }

        let mut claimed = [false; CLIENT_ENTITY_SLOT_COUNT];
        let mut wanted: Vec<(u16, WantedOccupant)> = Vec::new();
        let mut claim = |number: u16, occupant: WantedOccupant| {
            let index = usize::from(number);
            if claimed[index] {
                diag::warn_once!(
                    Net,
                    "entity number {number} was claimed twice in one snapshot"
                );
                return;
            }
            claimed[index] = true;
            wanted.push((number, occupant));
        };

        for (client, ps) in players {
            if Some(*client) == seated_viewer {
                continue;
            }
            let Some(number) = player_entity_number(*client) else {
                diag::warn_once!(
                    Net,
                    "client {client} is outside the {slots}-slot entity-number space",
                    client = client.0,
                    slots = crate::CLIENT_ENTITY_SLOT_COUNT
                );
                continue;
            };
            claim(number, occupant_from_player(*client, ps));
        }

        for slot in &corpses.slots {
            if !slot.occupied || slot.entnum < PLAYER_CORPSE_ENTITY_BASE {
                continue;
            }
            let Ok(number) = u16::try_from(slot.entnum) else {
                continue;
            };
            if usize::from(number) >= CLIENT_ENTITY_SLOT_COUNT {
                continue;
            }
            claim(number, occupant_from_corpse(slot));
        }

        for es in entities {
            if !replicated_entity_occupies(es) {
                continue;
            }
            let Ok(number) = u16::try_from(es.number) else {
                continue;
            };
            if usize::from(number) >= CLIENT_ENTITY_SLOT_COUNT {
                continue;
            }
            claim(number, occupant_from_replicated(*es));
        }

        for (number, occupant) in wanted {
            let index = usize::from(number);
            let client = occupant.client;
            let next_state = occupant.next_state;
            let Some(generation) =
                generations.and_then(|generations| generations.get(usize::from(number)).copied())
            else {
                diag::warn_once!(Net, "entity {number} has no replicated generation");
                claimed[index] = false;
                continue;
            };
            let valid = self.slots.slots[index].is_some_and(|slot| {
                self.identities.get(slot.entity).is_ok_and(|identity| {
                    identity.number == number && identity.generation == generation
                }) && self.runtimes.get(slot.entity).is_ok()
            });
            if valid {
                let entity = self.slots.slots[index].expect("valid slot").entity;
                if let Ok(mut runtime) = self.runtimes.get_mut(entity) {
                    apply_existing(&mut runtime, next_state, at_time_ms);
                }
                install_predicted_item(&mut self.commands, entity, next_state, snapshot);
                install_projectile_motion(&mut self.commands, entity, next_state, snapshot);
                self.census.note_apply(arm);
                continue;
            }
            if let Some(stale) = self.slots.slots[index].take() {
                if let Ok(previous) = self.identities.get(stale.entity) {
                    perf::instant!("centity_replace", "iw4l.sim", "number" => number as u64,
                    "old_generation" => previous.generation() as u64, "generation" => generation as u64);
                }
                if let Ok(mut runtime) = self.runtimes.get_mut(stale.entity) {
                    shutdown_entity(&mut runtime);
                }
                self.commands.entity(stale.entity).despawn();
            }
            let mut runtime = CEntityRuntime::default();
            reset_entity(&mut runtime, next_state, at_time_ms, true);
            let identity = CEntity {
                number,
                client,
                generation,
            };
            let launched = (next_state.e_type == entity_iw4::ET_MISSILE)
                .then(|| {
                    let cause = snapshot?
                        .projectiles
                        .iter()
                        .find(|p| p.entnum == next_state.number)?
                        .cause?;
                    predicted?
                        .iter()
                        .find(|(_, predicted)| predicted.cause == cause)
                })
                .flatten();
            let entity = match launched {
                Some((entity, predicted)) => {
                    perf::instant!("predicted_projectile_matched", "iw4l.sim", |ctx| {
                        let cause = predicted.cause;
                        ctx.arg("client", i64::from(cause.client.0));
                        ctx.arg("life", i64::from(cause.life.0));
                        ctx.arg("command", i64::from(cause.command.0));
                        ctx.arg("ordinal", i64::from(cause.ordinal));
                        ctx.arg("hand", i64::from(cause.hand));
                        ctx.arg("number", i64::from(number));
                    });
                    self.commands
                        .entity(entity)
                        .remove::<ClientPredictedProjectile>()
                        .insert((identity, runtime));
                    entity
                }
                None => self.commands.spawn((identity, runtime)).id(),
            };
            install_predicted_item(&mut self.commands, entity, next_state, snapshot);
            install_projectile_motion(&mut self.commands, entity, next_state, snapshot);
            self.slots.slots[index] = Some(CEntitySlot { client, entity });
            self.census.note_birth(arm);
        }

        for (index, slot) in self.slots.slots.iter_mut().enumerate() {
            if claimed[index] {
                continue;
            }
            if let Some(stale) = slot.take() {
                if let Ok(mut runtime) = self.runtimes.get_mut(stale.entity) {
                    shutdown_entity(&mut runtime);
                }
                self.commands.entity(stale.entity).despawn();
            }
        }
    }
}

pub(crate) fn register_authority_entities(app: &mut App) {
    register_entity_ownership(app);
    app.scoped::<CEntitySlots>(frame::Timeline::Live)
        .scoped::<CEntityBirthCensus>(frame::Timeline::Live)
        .add_systems(
            FixedUpdate,
            sync_authority_entities
                .in_set(frame::InMatch)
                .in_set(AuthoritySet::Fanout),
        );
}

pub fn register_client_entities(app: &mut App) {
    register_entity_ownership(app);
    app.scoped::<SelectedEntityTick>(frame::Timeline::Live);

    app.scoped::<CEntitySlots>(frame::Timeline::Live)
        .scoped::<CEntityBirthCensus>(frame::Timeline::Live)
        .add_systems(
            Update,
            sync_client_entities
                .in_set(frame::InMatch)
                .in_set(ClientSet::Reconcile)
                .after(crate::reconcile_prediction),
        );
}

fn register_entity_ownership(app: &mut App) {
    app.add_observer(
        |event: On<Add<CEntity>>,
         timeline: Res<State<frame::Timeline>>,
         identities: Query<(Entity, &CEntity)>,
         mut commands: Commands| {
            commands.entity(event.entity).insert(DespawnOnExit(*timeline.get()));
            if perf::recording()
                && let Ok((_, identity)) = identities.get(event.entity) {
                    let survivors = identities.iter().filter(|(entity, old)| {
                        *entity != event.entity && old.number() == identity.number()
                    }).count();
                    perf::instant!("centity_birth", "iw4l.sim", "number" => identity.number() as u64, "generation" => identity.generation() as u64, "survivors" => survivors as u64);
                }
        },
    );
}

fn install_predicted_item(
    commands: &mut Commands,
    entity: Entity,
    state: entity_iw4::EntityState,
    snapshot: Option<&sim::Snapshot>,
) {
    use crate::client::prediction_scene::PredictedItem;
    if state.e_type != entity_iw4::ET_ITEM {
        commands.entity(entity).remove::<PredictedItem>();
        return;
    }
    let Some(snapshot) = snapshot else {
        return;
    };
    let Some(ammo) = snapshot
        .meta
        .item_ammo
        .iter()
        .find(|ammo| ammo.entnum == state.number)
    else {
        diag::warn_once!(Net, "client item {} omitted ammo", state.number);
        return;
    };
    let pos = entity_iw4::Trajectory {
        tr_time: state.tr_time,
        tr_type: state.tr_type,
        tr_duration: state.tr_duration,
        tr_delta: state.tr_delta,
        tr_base: state.tr_base,
    };
    commands.entity(entity).insert(PredictedItem {
        item: sim::DroppedItem {
            state,
            origin: entity_iw4::evaluate_trajectory(&pos, sim::level_time_ms(snapshot.tick)),
            falling: state.tr_type == entity_iw4::TR_GRAVITY,
            clip_r: ammo.clip_r,
            clip_l: ammo.clip_l,
            stock: ammo.stock,
            scavenger: ammo.scavenger != 0,
            drop_seq: 0,
        },
        present: true,
        parented: ammo.parented,
    });
}
