use playerstate_iw4::{PlayerState, UserCmd};
use sim::{ClientId, ProjectileId, ProjectileState, Snapshot, Tick};
use std::collections::HashMap;

use crate::transport::meta_wire::{WorldObjectSyncDecoder, WorldObjectSyncEncoder};
use crate::transport::netfields::{PS_FIELD_COUNT, field_differs, read_field, write_field};
use crate::transport::wire::{WireError, WireReader, WireWriter};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectileEntityDelta {
    pub changed: Vec<ProjectileState>,
    pub removed: Vec<ProjectileId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotDelta {
    pub tick: Tick,

    bytes: Vec<u8>,
}

impl SnapshotDelta {
    pub fn encode(&self, out: &mut WireWriter) {
        out.put_u32(self.tick.0);
        out.put_u32(self.bytes.len() as u32);
        out.put_bytes(&self.bytes);
    }

    pub fn decode(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        let tick = Tick(input.get_u32()?);
        let length = input.get_u32()? as usize;
        let mut bytes = vec![0u8; length];
        input.get_bytes(&mut bytes)?;
        Ok(Self { tick, bytes })
    }

    pub fn payload_len(&self) -> usize {
        self.bytes.len()
    }
}

#[derive(Debug, Default)]
pub struct SnapshotEncoder {
    baseline: Vec<(ClientId, PlayerState)>,
    projectile_baseline: HashMap<ProjectileId, ProjectileState>,
    last_projectile_delta: ProjectileEntityDelta,
    world_objects: WorldObjectSyncEncoder,
    world_objects_wire: Vec<u8>,
}

impl SnapshotEncoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn last_projectile_delta(&self) -> &ProjectileEntityDelta {
        &self.last_projectile_delta
    }

    pub fn encode_world_objects(
        &mut self,
        tick: Tick,
        current: &sim::WorldObjectSnapshot,
    ) -> &[u8] {
        self.world_objects_wire = self.world_objects.encode(tick, current);
        &self.world_objects_wire
    }

    pub fn encode(&mut self, snapshot: &Snapshot) -> SnapshotDelta {
        let mut out = WireWriter::with_capacity(64 * snapshot.players.len().max(1));
        debug_assert!(
            snapshot.players.len() <= u16::MAX as usize,
            "player count exceeds the wire width"
        );
        out.put_u16(snapshot.players.len() as u16);

        for (client, ps) in &snapshot.players {
            out.put_u32(client.0);
            let baseline = self
                .baseline
                .iter()
                .find(|(id, _)| id == client)
                .map(|(_, state)| state)
                .unwrap_or(&PlayerState::ZERO);
            encode_player(&mut out, baseline, ps);
        }
        let projectile_delta =
            encode_projectile_delta(&mut out, &self.projectile_baseline, &snapshot.projectiles);
        self.last_projectile_delta = projectile_delta;
        self.projectile_baseline = snapshot.projectiles.iter().map(|p| (p.id, *p)).collect();

        self.baseline = snapshot.players.clone();
        SnapshotDelta {
            tick: snapshot.tick,
            bytes: out.finish(),
        }
    }

    pub fn adopt_baseline(&mut self, snapshot: &Snapshot) {
        self.baseline = snapshot.players.clone();
        self.projectile_baseline = snapshot
            .projectiles
            .iter()
            .map(|projectile| (projectile.id, *projectile))
            .collect();
        self.world_objects
            .adopt_baseline(snapshot.meta.world_objects.clone());
    }

    pub fn reset(&mut self) {
        self.baseline.clear();
        self.projectile_baseline.clear();
        self.last_projectile_delta = ProjectileEntityDelta::default();
        self.world_objects.reset();
        self.world_objects_wire.clear();
    }
}

#[derive(Debug, Default)]
pub struct SnapshotDecoder {
    baseline: Vec<(ClientId, PlayerState)>,
    projectile_baseline: HashMap<ProjectileId, ProjectileState>,
    last_projectile_delta: ProjectileEntityDelta,
    world_objects: WorldObjectSyncDecoder,
}

impl SnapshotDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn last_projectile_delta(&self) -> &ProjectileEntityDelta {
        &self.last_projectile_delta
    }

    pub fn apply_world_objects_wire(
        &mut self,
        wire: &[u8],
    ) -> Result<sim::WorldObjectSnapshot, WireError> {
        self.world_objects.apply_wire(wire)
    }

    pub fn decode(&mut self, delta: &SnapshotDelta) -> Result<Snapshot, WireError> {
        let mut input = WireReader::new(&delta.bytes);
        let player_count = input.get_u16()? as usize;
        let mut players = Vec::with_capacity(player_count.min(64));

        for _ in 0..player_count {
            let client = ClientId(input.get_u32()?);
            let mut state = self
                .baseline
                .iter()
                .find(|(id, _)| *id == client)
                .map(|(_, state)| *state)
                .unwrap_or(PlayerState::ZERO);
            decode_player(&mut input, &mut state)?;
            players.push((client, state));
        }
        let projectile_delta = decode_projectile_delta(&mut input, &mut self.projectile_baseline)?;
        self.last_projectile_delta = projectile_delta.clone();
        let mut projectiles: Vec<_> = self.projectile_baseline.values().copied().collect();
        projectiles.sort_by_key(|p| p.id.0);

        if !input.is_empty() {
            return Err(WireError::Malformed(
                "trailing bytes after the projectile entity delta",
            ));
        }

        self.baseline = players.clone();
        Ok(Snapshot {
            tick: delta.tick,
            players,
            projectiles,
            meta: sim::SnapshotMeta::default(),
        })
    }

    pub fn reset(&mut self) {
        self.baseline.clear();
        self.projectile_baseline.clear();
        self.last_projectile_delta = ProjectileEntityDelta::default();
        self.world_objects.reset();
    }

    pub fn adopt_baseline(&mut self, snapshot: &Snapshot) {
        self.baseline = snapshot.players.clone();
        self.projectile_baseline = snapshot
            .projectiles
            .iter()
            .map(|projectile| (projectile.id, *projectile))
            .collect();
        self.world_objects
            .adopt_baseline(snapshot.meta.world_objects.clone());
    }
}
fn encode_player(out: &mut WireWriter, baseline: &PlayerState, current: &PlayerState) {
    let mut changed = [false; PS_FIELD_COUNT];
    let mut last_changed: isize = -1;
    for (index, slot) in changed.iter_mut().enumerate() {
        if field_differs(baseline, current, index) {
            *slot = true;
            last_changed = index as isize;
        }
    }

    let count = (last_changed + 1) as usize;
    out.put_u8(count as u8);

    let mask_bytes = count.div_ceil(8);
    for byte in 0..mask_bytes {
        let mut bits = 0u8;
        for bit in 0..8 {
            let index = byte * 8 + bit;
            if index < count && changed[index] {
                bits |= 1 << bit;
            }
        }
        out.put_u8(bits);
    }

    for (index, _) in changed.iter().take(count).enumerate().filter(|(_, c)| **c) {
        write_field(out, current, index);
    }
}

fn decode_player(input: &mut WireReader<'_>, state: &mut PlayerState) -> Result<(), WireError> {
    let count = input.get_u8()? as usize;
    if count > PS_FIELD_COUNT {
        return Err(WireError::Malformed(
            "changed-field count exceeds the playerState registry",
        ));
    }

    let mask_bytes = count.div_ceil(8);
    let mut mask = [0u8; PS_FIELD_COUNT.div_ceil(8)];
    for slot in mask.iter_mut().take(mask_bytes) {
        *slot = input.get_u8()?;
    }

    for index in 0..count {
        if mask[index / 8] & (1 << (index % 8)) != 0 {
            read_field(input, state, index)?;
        }
    }
    Ok(())
}

fn projectile_differs(a: &ProjectileState, b: &ProjectileState) -> bool {
    a != b
}

fn encode_projectile(out: &mut WireWriter, projectile: &ProjectileState) {
    out.put_u32(projectile.id.0);
    out.put_u32(projectile.owner.0);
    out.put_u32(projectile.owner_life.0);
    out.put_u32(projectile.weapon);
    for value in projectile.origin {
        out.put_f32(value);
    }
    for value in projectile.velocity {
        out.put_f32(value);
    }
    out.put_f32(projectile.gravity);
    out.put_u32(projectile.age_ticks);
    out.put_u32(projectile.fuse_ticks);
    encode_trajectory(out, &projectile.pos);
    encode_trajectory(out, &projectile.apos);
    out.put_i32(projectile.entnum);
    out.put_i32(projectile.launch_time);
}

fn encode_trajectory(out: &mut WireWriter, tr: &entity_iw4::Trajectory) {
    out.put_i32(tr.tr_time);
    out.put_i32(tr.tr_type);
    out.put_i32(tr.tr_duration);
    for value in tr.tr_delta {
        out.put_f32(value);
    }
    for value in tr.tr_base {
        out.put_f32(value);
    }
}

fn decode_trajectory(input: &mut WireReader<'_>) -> Result<entity_iw4::Trajectory, WireError> {
    Ok(entity_iw4::Trajectory {
        tr_time: input.get_i32()?,
        tr_type: input.get_i32()?,
        tr_duration: input.get_i32()?,
        tr_delta: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
        tr_base: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
    })
}

fn decode_projectile(input: &mut WireReader<'_>) -> Result<ProjectileState, WireError> {
    Ok(ProjectileState {
        id: ProjectileId(input.get_u32()?),
        owner: ClientId(input.get_u32()?),
        owner_life: sim::LifeSequence(input.get_u32()?),
        weapon: input.get_u32()?,
        origin: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
        velocity: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
        gravity: input.get_f32()?,
        age_ticks: input.get_u32()?,
        fuse_ticks: input.get_u32()?,
        pos: decode_trajectory(input)?,
        apos: decode_trajectory(input)?,
        entnum: input.get_i32()?,
        launch_time: input.get_i32()?,
    })
}

fn encode_projectile_delta(
    out: &mut WireWriter,
    baseline: &HashMap<ProjectileId, ProjectileState>,
    current: &[ProjectileState],
) -> ProjectileEntityDelta {
    let mut changed = Vec::new();
    let mut current_ids = HashMap::new();
    for projectile in current {
        current_ids.insert(projectile.id, *projectile);
        match baseline.get(&projectile.id) {
            Some(old) if !projectile_differs(old, projectile) => {}
            _ => changed.push(*projectile),
        }
    }
    let mut removed = Vec::new();
    for id in baseline.keys() {
        if !current_ids.contains_key(id) {
            removed.push(*id);
        }
    }
    removed.sort_by_key(|id| id.0);
    changed.sort_by_key(|p| p.id.0);

    debug_assert!(changed.len() <= u16::MAX as usize);
    debug_assert!(removed.len() <= u16::MAX as usize);
    out.put_u16(changed.len() as u16);
    for projectile in &changed {
        encode_projectile(out, projectile);
    }
    out.put_u16(removed.len() as u16);
    for id in &removed {
        out.put_u32(id.0);
    }
    ProjectileEntityDelta { changed, removed }
}

fn decode_projectile_delta(
    input: &mut WireReader<'_>,
    baseline: &mut HashMap<ProjectileId, ProjectileState>,
) -> Result<ProjectileEntityDelta, WireError> {
    let changed_count = input.get_u16()? as usize;
    let mut changed = Vec::with_capacity(changed_count.min(256));
    for _ in 0..changed_count {
        let projectile = decode_projectile(input)?;
        baseline.insert(projectile.id, projectile);
        changed.push(projectile);
    }
    let removed_count = input.get_u16()? as usize;
    let mut removed = Vec::with_capacity(removed_count.min(256));
    for _ in 0..removed_count {
        let id = ProjectileId(input.get_u32()?);
        baseline.remove(&id);
        removed.push(id);
    }
    Ok(ProjectileEntityDelta { changed, removed })
}

pub(crate) fn encode_usercmd(out: &mut WireWriter, cmd: &UserCmd) {
    out.put_i32(cmd.server_time);
    out.put_u32(cmd.buttons);
    for axis in cmd.angles {
        out.put_i32(axis);
    }
    out.put_u16(cmd.weapon);
    out.put_u16(cmd.weapon_mapped);
    out.put_u16(cmd.off_hand_index);
    out.put_i8(cmd.forwardmove);
    out.put_i8(cmd.rightmove);
    out.put_f32(cmd.melee_charge_yaw);
    out.put_u8(cmd.melee_charge_dist);
    out.put_bytes(&cmd.selected_location);
    out.put_bytes(&cmd.remote_control);
}

pub(crate) fn decode_usercmd(input: &mut WireReader<'_>) -> Result<UserCmd, WireError> {
    let server_time = input.get_i32()?;
    let buttons = input.get_u32()?;
    let mut angles = [0i32; 3];
    for axis in &mut angles {
        *axis = input.get_i32()?;
    }
    let weapon = input.get_u16()?;
    let weapon_mapped = input.get_u16()?;
    let off_hand_index = input.get_u16()?;
    let forwardmove = input.get_i8()?;
    let rightmove = input.get_i8()?;
    let melee_charge_yaw = input.get_f32()?;
    let melee_charge_dist = input.get_u8()?;
    let mut selected_location = [0u8; 3];
    input.get_bytes(&mut selected_location)?;
    let mut remote_control = [0u8; 2];
    input.get_bytes(&mut remote_control)?;
    Ok(UserCmd {
        server_time,
        buttons,
        angles,
        weapon,
        weapon_mapped,
        off_hand_index,
        forwardmove,
        rightmove,
        melee_charge_yaw,
        melee_charge_dist,
        selected_location,
        remote_control,
    })
}
