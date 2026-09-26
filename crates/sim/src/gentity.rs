use entity_iw4::{
    ET_ITEM, ET_MISSILE, ET_SCRIPTMOVER, EntityState, TR_LINEAR_STOP, TR_STATIONARY, Trajectory,
    bg_evaluate_trajectory,
};
use playerstate_iw4::{GENTITY_SPAWN_BASE, buttons};
use trace_iw4::ENTITYNUM_WORLD;

use crate::identities::ScriptModelId;

pub const GENTITY_RESERVED_COUNT: i32 = GENTITY_SPAWN_BASE;
pub const GENTITY_REUSE_QUARANTINE_MS: i32 = 500;
pub const GENTITY_TEMP_EVENT_LIFETIME_MS: i32 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KernelPhase {
    AdvanceTime,
    ExpireTransientEvents,
    ApplyActions,
    RunPlayers,
    RecordCollisionState,
    RunEntityTypes,
    DispatchTouches,
    Finalize,
    PublishSnapshot,
}

pub const KERNEL_PHASE_ORDER: &[KernelPhase] = &[
    KernelPhase::AdvanceTime,
    KernelPhase::ExpireTransientEvents,
    KernelPhase::ApplyActions,
    KernelPhase::RunPlayers,
    KernelPhase::RecordCollisionState,
    KernelPhase::RunEntityTypes,
    KernelPhase::DispatchTouches,
    KernelPhase::Finalize,
    KernelPhase::PublishSnapshot,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntityRunKind {
    ScriptMover,
    Item,
    Missile,
    TempEvent,

    PrimaryLight,

    General,

    PlayerCorpse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityRef {
    number: i32,
    generation: u32,
}

impl EntityRef {
    pub fn from_parts(number: i32, generation: u32) -> Result<Self, EntityRefError> {
        if !(GENTITY_RESERVED_COUNT..i32::from(ENTITYNUM_WORLD)).contains(&number) {
            return Err(EntityRefError::ReservedOrOutOfRange);
        }
        Ok(Self { number, generation })
    }

    pub const fn number(self) -> i32 {
        self.number
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityAllocError {
    CapacityExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityRefError {
    ReservedOrOutOfRange,
    SnapshotMalformed,
    Free,
    StaleGeneration { current: u32 },
    RelationTargetInvalid,

    CyclicParent,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityRelations {
    pub owner: Option<EntityRef>,
    pub parent: Option<EntityRef>,
    pub ground: Option<EntityRef>,

    pub parent_tag: i32,

    pub parent_link_axis: [[f32; 3]; 3],

    pub parent_link_origin: [f32; 3],
}

impl Default for EntityRelations {
    fn default() -> Self {
        Self {
            owner: None,
            parent: None,
            ground: None,
            parent_tag: -1,
            parent_link_axis: entity_iw4::PARENT_LINK_AXIS_IDENTITY,
            parent_link_origin: [0.0; 3],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntitySlotView {
    pub entity: EntityRef,
    pub kind: EntityRunKind,
    pub linked: bool,
    pub relations: EntityRelations,
    pub next_think_ms: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityKernelOccupiedSnapshot {
    pub kind: EntityRunKind,
    pub linked: bool,
    pub relations: EntityRelations,
    pub next_think_ms: Option<i32>,
    pub transient_event_time_ms: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityKernelSlotSnapshot {
    pub generation: u32,
    pub occupied: Option<EntityKernelOccupiedSnapshot>,
    pub freed_at_ms: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityKernelSnapshot {
    pub slots: Vec<EntityKernelSlotSnapshot>,
    pub high_water: i32,
    pub free_fifo: Vec<i32>,
    pub level_time_ms: i32,
    pub frame_serial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKernelSnapshotError {
    HighWaterOutOfRange,
    SlotCountMismatch,
    FreeFifoInvalid,
    RelationInvalid,
}

impl Default for EntityKernelSnapshot {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            high_water: GENTITY_RESERVED_COUNT,
            free_fifo: Vec::new(),
            level_time_ms: 0,
            frame_serial: 0,
        }
    }
}

impl EntityKernelSnapshot {
    pub fn current_ref(&self, number: i32) -> Result<EntityRef, EntityRefError> {
        if !(GENTITY_RESERVED_COUNT..self.high_water).contains(&number) {
            return Err(EntityRefError::ReservedOrOutOfRange);
        }
        let slot = self
            .slots
            .get((number - GENTITY_RESERVED_COUNT) as usize)
            .ok_or(EntityRefError::SnapshotMalformed)?;
        if slot.occupied.is_none() {
            return Err(EntityRefError::Free);
        }
        Ok(EntityRef {
            number,
            generation: slot.generation,
        })
    }

    pub fn occupied_kind(&self, number: i32) -> Option<EntityRunKind> {
        if !(GENTITY_RESERVED_COUNT..self.high_water).contains(&number) {
            return None;
        }
        self.slots[(number - GENTITY_RESERVED_COUNT) as usize]
            .occupied
            .map(|occupied| occupied.kind)
    }

    pub fn validate(&self) -> Result<(), EntityKernelSnapshotError> {
        if !(GENTITY_RESERVED_COUNT..=i32::from(ENTITYNUM_WORLD)).contains(&self.high_water) {
            return Err(EntityKernelSnapshotError::HighWaterOutOfRange);
        }
        if self.slots.len() != (self.high_water - GENTITY_RESERVED_COUNT) as usize {
            return Err(EntityKernelSnapshotError::SlotCountMismatch);
        }

        let mut free_seen = vec![false; self.slots.len()];
        for &number in &self.free_fifo {
            if !(GENTITY_RESERVED_COUNT..self.high_water).contains(&number) {
                return Err(EntityKernelSnapshotError::FreeFifoInvalid);
            }
            let index = (number - GENTITY_RESERVED_COUNT) as usize;
            if free_seen[index] || self.slots[index].occupied.is_some() {
                return Err(EntityKernelSnapshotError::FreeFifoInvalid);
            }
            free_seen[index] = true;
        }
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.occupied.is_none() != free_seen[index] {
                return Err(EntityKernelSnapshotError::FreeFifoInvalid);
            }
            let Some(occupied) = slot.occupied else {
                continue;
            };
            for relation in [
                occupied.relations.owner,
                occupied.relations.parent,
                occupied.relations.ground,
            ]
            .into_iter()
            .flatten()
            {
                if !(GENTITY_RESERVED_COUNT..self.high_water).contains(&relation.number) {
                    return Err(EntityKernelSnapshotError::RelationInvalid);
                }
                let target = &self.slots[(relation.number - GENTITY_RESERVED_COUNT) as usize];
                if target.generation != relation.generation || target.occupied.is_none() {
                    return Err(EntityKernelSnapshotError::RelationInvalid);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThinkEnter {
    Skip,
    Run {
        entity: EntityRef,
        kind: EntityRunKind,
        parent: Option<EntityRef>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OccupiedSlot {
    kind: EntityRunKind,
    linked: bool,
    relations: EntityRelations,
    next_think_ms: Option<i32>,
    transient_event_time_ms: Option<i32>,

    think_serial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EntitySlot {
    generation: u32,
    occupied: Option<OccupiedSlot>,
    freed_at_ms: i32,
}

impl Default for EntitySlot {
    fn default() -> Self {
        Self {
            generation: 0,
            occupied: None,
            freed_at_ms: i32::MIN,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityKernel {
    slots: Vec<EntitySlot>,
    high_water: i32,
    free_fifo: std::collections::VecDeque<i32>,
    level_time_ms: i32,
    frame_serial: u32,
}

impl Default for EntityKernel {
    fn default() -> Self {
        Self {
            slots: vec![EntitySlot::default(); usize::from(ENTITYNUM_WORLD)],
            high_water: GENTITY_RESERVED_COUNT,
            free_fifo: std::collections::VecDeque::new(),
            level_time_ms: 0,
            frame_serial: 0,
        }
    }
}

impl EntityKernel {
    pub fn to_snapshot(&self) -> EntityKernelSnapshot {
        EntityKernelSnapshot {
            slots: self.slots[GENTITY_RESERVED_COUNT as usize..self.high_water as usize]
                .iter()
                .map(|slot| EntityKernelSlotSnapshot {
                    generation: slot.generation,
                    occupied: slot.occupied.map(|occupied| EntityKernelOccupiedSnapshot {
                        kind: occupied.kind,
                        linked: occupied.linked,
                        relations: occupied.relations,
                        next_think_ms: occupied.next_think_ms,
                        transient_event_time_ms: occupied.transient_event_time_ms,
                    }),
                    freed_at_ms: slot.freed_at_ms,
                })
                .collect(),
            high_water: self.high_water,
            free_fifo: self.free_fifo.iter().copied().collect(),
            level_time_ms: self.level_time_ms,
            frame_serial: self.frame_serial,
        }
    }

    pub fn from_snapshot(
        snapshot: &EntityKernelSnapshot,
    ) -> Result<Self, EntityKernelSnapshotError> {
        snapshot.validate()?;
        let mut slots = vec![EntitySlot::default(); usize::from(ENTITYNUM_WORLD)];
        for (index, source) in snapshot.slots.iter().enumerate() {
            slots[GENTITY_RESERVED_COUNT as usize + index] = EntitySlot {
                generation: source.generation,
                occupied: source.occupied.map(|occupied| OccupiedSlot {
                    kind: occupied.kind,
                    linked: occupied.linked,
                    relations: occupied.relations,
                    next_think_ms: occupied.next_think_ms,
                    transient_event_time_ms: occupied.transient_event_time_ms,
                    think_serial: 0,
                }),
                freed_at_ms: source.freed_at_ms,
            };
        }
        Ok(Self {
            slots,
            high_water: snapshot.high_water,
            free_fifo: snapshot.free_fifo.iter().copied().collect(),
            level_time_ms: snapshot.level_time_ms,
            frame_serial: snapshot.frame_serial,
        })
    }

    pub fn begin_frame(&mut self, level_time_ms: i32) {
        self.level_time_ms = level_time_ms;
        self.frame_serial = self.frame_serial.wrapping_add(1);
    }

    pub const fn level_time_ms(&self) -> i32 {
        self.level_time_ms
    }

    pub const fn frame_serial(&self) -> u32 {
        self.frame_serial
    }

    pub const fn high_water(&self) -> i32 {
        self.high_water
    }

    pub fn occupied_kind(&self, number: i32) -> Option<EntityRunKind> {
        self.slot(number)
            .ok()?
            .occupied
            .map(|occupied| occupied.kind)
    }

    pub fn occupied_numbers(&self, mut matches: impl FnMut(EntityRunKind) -> bool) -> Vec<i32> {
        let mut numbers = Vec::new();
        let mut number = GENTITY_RESERVED_COUNT;
        while number < self.high_water {
            if let Some(kind) = self.occupied_kind(number) {
                if matches(kind) {
                    numbers.push(number);
                }
            }
            number += 1;
        }
        numbers
    }

    pub fn allocate(&mut self, kind: EntityRunKind) -> Result<EntityRef, EntityAllocError> {
        let reusable = self.free_fifo.front().copied().filter(|number| {
            let slot = &self.slots[*number as usize];
            self.high_water >= i32::from(ENTITYNUM_WORLD)
                || self.level_time_ms.saturating_sub(slot.freed_at_ms)
                    >= GENTITY_REUSE_QUARANTINE_MS
        });
        let number = if let Some(number) = reusable {
            self.free_fifo.pop_front();
            number
        } else if self.high_water < i32::from(ENTITYNUM_WORLD) {
            let number = self.high_water;
            self.high_water += 1;
            number
        } else {
            return Err(EntityAllocError::CapacityExhausted);
        };
        let slot = &mut self.slots[number as usize];
        slot.occupied = Some(OccupiedSlot {
            kind,
            linked: false,
            relations: EntityRelations::default(),
            next_think_ms: None,
            transient_event_time_ms: None,
            think_serial: 0,
        });
        Ok(EntityRef {
            number,
            generation: slot.generation,
        })
    }

    pub fn current_ref(&self, number: i32) -> Result<EntityRef, EntityRefError> {
        let slot = self.slot(number)?;
        if slot.occupied.is_none() {
            return Err(EntityRefError::Free);
        }
        Ok(EntityRef {
            number,
            generation: slot.generation,
        })
    }

    pub fn resolve(&self, entity: EntityRef) -> Result<EntitySlotView, EntityRefError> {
        let slot = self.slot(entity.number)?;
        if slot.generation != entity.generation {
            return Err(EntityRefError::StaleGeneration {
                current: slot.generation,
            });
        }
        let occupied = slot.occupied.ok_or(EntityRefError::Free)?;
        Ok(EntitySlotView {
            entity,
            kind: occupied.kind,
            linked: occupied.linked,
            relations: occupied.relations,
            next_think_ms: occupied.next_think_ms,
        })
    }

    pub fn set_linked(&mut self, entity: EntityRef, linked: bool) -> Result<(), EntityRefError> {
        self.occupied_mut(entity)?.linked = linked;
        Ok(())
    }

    pub fn take_due_think(&mut self, entity: EntityRef) -> Result<bool, EntityRefError> {
        let time = self.level_time_ms;
        let occupied = self.occupied_mut(entity)?;
        if occupied
            .next_think_ms
            .is_some_and(|at| at > 0 && at <= time)
        {
            occupied.next_think_ms = None;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn begin_think(&mut self, number: i32) -> Result<ThinkEnter, EntityRefError> {
        let serial = self.frame_serial;
        if !(GENTITY_RESERVED_COUNT..i32::from(ENTITYNUM_WORLD)).contains(&number) {
            return Ok(ThinkEnter::Skip);
        }
        let slot = &mut self.slots[number as usize];
        let Some(occupied) = slot.occupied.as_mut() else {
            return Ok(ThinkEnter::Skip);
        };
        if occupied.think_serial == serial {
            return Ok(ThinkEnter::Skip);
        }
        occupied.think_serial = serial;
        Ok(ThinkEnter::Run {
            entity: EntityRef {
                number,
                generation: slot.generation,
            },
            kind: occupied.kind,
            parent: occupied.relations.parent,
        })
    }

    pub fn mark_transient_event(
        &mut self,
        entity: EntityRef,
        event_time_ms: i32,
    ) -> Result<(), EntityRefError> {
        self.occupied_mut(entity)?.transient_event_time_ms = Some(event_time_ms);
        Ok(())
    }

    pub fn expire_transient_events(&mut self) -> Vec<EntityRef> {
        let mut expired = Vec::new();
        for number in GENTITY_RESERVED_COUNT..self.high_water {
            let slot = &self.slots[number as usize];
            let Some(occupied) = slot.occupied else {
                continue;
            };
            if occupied.transient_event_time_ms.is_some_and(|event_time| {
                self.level_time_ms.saturating_sub(event_time) > GENTITY_TEMP_EVENT_LIFETIME_MS
            }) {
                expired.push(EntityRef {
                    number,
                    generation: slot.generation,
                });
            }
        }
        for entity in &expired {
            self.free(*entity)
                .expect("transient event was resolved immediately before free");
        }
        expired
    }

    pub fn free(&mut self, entity: EntityRef) -> Result<(), EntityRefError> {
        self.resolve(entity)?;
        for slot in &mut self.slots[GENTITY_RESERVED_COUNT as usize..self.high_water as usize] {
            let Some(occupied) = slot.occupied.as_mut() else {
                continue;
            };
            if occupied.relations.owner == Some(entity) {
                occupied.relations.owner = None;
            }
            if occupied.relations.parent == Some(entity) {
                occupied.relations.parent = None;
            }
            if occupied.relations.ground == Some(entity) {
                occupied.relations.ground = None;
            }
        }
        let slot = &mut self.slots[entity.number as usize];
        slot.occupied = None;
        slot.freed_at_ms = self.level_time_ms;
        slot.generation = slot.generation.wrapping_add(1);
        self.free_fifo.push_back(entity.number);
        Ok(())
    }

    fn slot(&self, number: i32) -> Result<&EntitySlot, EntityRefError> {
        if !(GENTITY_RESERVED_COUNT..i32::from(ENTITYNUM_WORLD)).contains(&number) {
            return Err(EntityRefError::ReservedOrOutOfRange);
        }
        Ok(&self.slots[number as usize])
    }

    fn occupied_mut(&mut self, entity: EntityRef) -> Result<&mut OccupiedSlot, EntityRefError> {
        let slot = self.slot(entity.number)?;
        if slot.generation != entity.generation {
            return Err(EntityRefError::StaleGeneration {
                current: slot.generation,
            });
        }
        self.slots[entity.number as usize]
            .occupied
            .as_mut()
            .ok_or(EntityRefError::Free)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsePress {
    pub client: u32,
    pub held: bool,
    pub edge: bool,
}

pub fn collect_use_presses(cmds: &[(u32, u32)], old_buttons: &[(u32, u32)]) -> Vec<UsePress> {
    let mut presses = Vec::new();
    let mut previous = old_buttons.to_vec();
    for &(client, now) in cmds {
        let old = previous
            .iter()
            .find(|(id, _)| *id == client)
            .map(|(_, bits)| *bits)
            .unwrap_or(0);
        if let Some((_, bits)) = previous.iter_mut().find(|(id, _)| *id == client) {
            *bits = now;
        } else {
            previous.push((client, now));
        }
        let held = now & (buttons::USE | buttons::USE_RELOAD) != 0;
        if !held {
            continue;
        }
        presses.push(UsePress {
            client,
            held: true,
            edge: old & (buttons::USE | buttons::USE_RELOAD) == 0,
        });
    }
    presses
}

pub fn init_script_mover_state(number: i32, origin: [f32; 3], angles: [f32; 3]) -> EntityState {
    let mut es = EntityState::default();
    es.number = number;
    es.e_type = ET_SCRIPTMOVER;
    es.tr_type = TR_STATIONARY;
    es.tr_base = origin;
    es.apos_tr_type = TR_STATIONARY;
    es.apos_tr_base = angles;
    es
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptMoverGentity {
    pub id: ScriptModelId,
    pub state: EntityState,

    pub box_mid: [f32; 3],

    pub box_half: [f32; 3],

    pub link_mid: [f32; 3],

    pub link_half: [f32; 3],

    pub nonsolid: bool,

    pub shown_to: u64,
}

impl Default for ScriptMoverGentity {
    fn default() -> Self {
        Self {
            id: ScriptModelId::from_wire(0),
            state: EntityState::default(),
            box_mid: [0.0; 3],
            box_half: [0.0; 3],
            link_mid: [0.0; 3],
            link_half: [0.0; 3],
            nonsolid: false,
            shown_to: 0,
        }
    }
}

pub fn pos_from_entity_state(es: &EntityState) -> Trajectory {
    Trajectory {
        tr_time: es.tr_time,
        tr_type: es.tr_type,
        tr_duration: es.tr_duration,
        tr_delta: es.tr_delta,
        tr_base: es.tr_base,
    }
}

pub fn apos_from_entity_state(es: &EntityState) -> Trajectory {
    Trajectory {
        tr_time: es.apos_tr_time,
        tr_type: es.apos_tr_type,
        tr_duration: es.apos_tr_duration,
        tr_delta: es.apos_tr_delta,
        tr_base: es.apos_tr_base,
    }
}

fn write_pos(es: &mut EntityState, tr: Trajectory) {
    es.tr_time = tr.tr_time;
    es.tr_type = tr.tr_type;
    es.tr_duration = tr.tr_duration;
    es.tr_delta = tr.tr_delta;
    es.tr_base = tr.tr_base;
}

fn write_apos(es: &mut EntityState, tr: Trajectory) {
    es.apos_tr_time = tr.tr_time;
    es.apos_tr_type = tr.tr_type;
    es.apos_tr_duration = tr.tr_duration;
    es.apos_tr_delta = tr.tr_delta;
    es.apos_tr_base = tr.tr_base;
}

pub fn init_missile_state(
    number: i32,
    weapon: u32,
    pos: Trajectory,
    apos: Trajectory,
    launch_time: i32,
) -> EntityState {
    let mut es = EntityState::default();
    es.number = number;
    es.e_type = ET_MISSILE;
    es.e_flags = 0;
    es.index = weapon as i32;
    write_pos(&mut es, pos);
    write_apos(&mut es, apos);
    es.set_launch_time(launch_time);
    es
}

pub fn init_item_state(
    number: i32,
    weapon: u32,
    pos: Trajectory,
    apos: Trajectory,
    owner_num: i32,
) -> EntityState {
    let mut es = EntityState::default();
    es.number = number;
    es.e_type = ET_ITEM;
    es.e_flags = 0;
    es.index = weapon as i32;
    es.client_num = owner_num;
    write_pos(&mut es, pos);
    write_apos(&mut es, apos);
    es
}

pub fn rotate_velocity_apos(
    current: &Trajectory,
    speed: [f32; 3],
    total_time_seconds: f32,
    level_time_ms: i32,
) -> Trajectory {
    let tr_base = if current.tr_type != 0 {
        bg_evaluate_trajectory(current, level_time_ms)
    } else {
        current.tr_base
    };
    Trajectory {
        tr_time: level_time_ms,
        tr_type: TR_LINEAR_STOP,
        tr_duration: (total_time_seconds * 1000.0) as i32,
        tr_delta: speed,
        tr_base,
    }
}

pub fn begin_script_mover_rotate_velocity(
    state: &mut EntityState,
    speed: [f32; 3],
    total_time_seconds: f32,
    level_time_ms: i32,
) {
    let next = rotate_velocity_apos(
        &apos_from_entity_state(state),
        speed,
        total_time_seconds,
        level_time_ms,
    );
    write_apos(state, next);
}

pub fn gentity_spawn_base() -> i32 {
    GENTITY_SPAWN_BASE
}
