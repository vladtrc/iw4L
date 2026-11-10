use playerstate_iw4::AnimPair;
use sim::{
    AreaEntityLinkSnapshot, AreaEntityWorldSnapshot, AreaSectorSnapshot, ClassId,
    ClassRejectReason, ClientAction, ClientId, ClientLifecycle, ClientSnapshotMeta, DamageSource,
    DroppedItemAmmo, EntityEventPayload, EntityEventRecord, EntityRef, EventAudience, EventRecord,
    EventSequence, ItemPickupRecord, LifeSequence, LoadoutSpec, MatchEndReason, MatchPhase,
    PelletFxRecord, PlayerCorpsePool, PlayerCorpseSlot, ScriptModelId, SimEvent, SnapshotMeta,
    SpawnPick, Tick,
};

use super::codec::{
    Bytes, Len8, Len32, Padded, Sentinel8, Sentinel16, Text8, Via, Wire, put_len, put_slice,
    wire_enum, wire_newtype, wire_struct,
};
use crate::transport::wire::{WireError, WireReader, WireWriter};

pub use super::world_sync::{
    WORLD_SYNC_PERIOD_TICKS, WorldObjectSyncDecoder, WorldObjectSyncEncoder, decode_snapshot_meta,
    decode_world_object_sync_wire, encode_world_object_sync,
};

wire_newtype!(
    ClientId,
    ClassId,
    LifeSequence,
    EventSequence,
    Tick,
    sim::CommandSequence,
    sim::ShotId,
    sim::ProjectileId,
    entity_iw4::EntityEventKind,
);

impl Wire for ScriptModelId {
    fn put(&self, out: &mut WireWriter) {
        out.put_u32(self.to_wire());
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        Ok(Self::from_wire(input.get_u32()?))
    }
}

impl Wire for EntityRef {
    fn put(&self, out: &mut WireWriter) {
        out.put_i32(self.number());
        out.put_u32(self.generation());
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        EntityRef::from_parts(input.get_i32()?, input.get_u32()?)
            .map_err(|_| WireError::Malformed("EntityKernel relation number out of range"))
    }
}

wire_enum!(MatchPhase, "unknown MatchPhase tag" {
    0 => Warmup,
    1 => Playing,
    2 => Intermission,
    3 => PostGame,
});

wire_enum!(ClientLifecycle, "unknown ClientLifecycle tag" {
    0 => Connecting,
    1 => ChoosingClass,
    2 => Alive,
    3 => Dead,
    4 => Spectating,
    5 => SpawnPending,
    6 => RespawnPending,
    7 => Intermission,
});

wire_enum!(ClassRejectReason, "unknown ClassRejectReason tag" {
    1 => UnknownOrStaleClass,
    2 => NoSpawnAvailable,
    3 => LockedContent,
    4 => UnknownWeaponId,
});

wire_enum!(MatchEndReason, "unknown MatchEndReason tag" {
    0 => ScoreLimit,
    1 => TimeLimit,
});

wire_enum!(DamageSource, "unknown DamageSource tag" {
    1 => Shot(id),
    2 => Projectile(id),
    3 => Radius(id),
    4 => Melee,
});

wire_enum!(SimEvent, "unknown SimEvent tag" {
    3 => Spawned { class_id, spawn_index, life_sequence },
    5 => AttackReleased,
    6 => Died {
        victim,
        life_sequence,
        attacker,
        attacker_life,
        source,
        weapon,
        killcam_entity_start_time,
    },
    11 => ScoreChanged { client, score, kills, deaths },
    12 => MatchEnded { reason },
    18 => WeaponSwitchRequested { weapon },
});

wire_enum!(EventAudience, "unknown EventAudience tag" {
    0 => All,
    3 => AllExcept(id),
    1 => Client(id),
    2 => Clients(ids: Len8),
});

wire_enum!(SpawnPick, "unknown SpawnPick tag" {
    0 => Seeded(seed),
    1 => At { origin, yaw },
});

wire_struct!(sim::PersonalClass {
    weapons,
    perks,
    deathstreak,
    camos
});
wire_struct!(sim::PlayerProfile {
    title,
    emblem,
    killstreaks
});

wire_enum!(ClientAction, "unknown ClientAction tag" {
    1 => SelectClass { class_id, loadout },
    2 => JoinMatch,
    3 => LeaveMatch,
    4 => ForceDeath,
    5 => SetMatchPhase { phase },
    6 => GiveWeapon { weapon, model },
    7 => Move { origin, angles },
    8 => SpawnClient,
    9 => BeginScriptMoverRotateVelocity { speed },
    10 => DebugDamage { amount },
    11 => SetName { name: Bytes },
    12 => SpawnIntermission,
    13 => UseCopycat,
    14 => ForceSpawn { pick },
    15 => ChangeWeaponConfiguration { from, to },
    16 => ChooseDefaultClass { index },
    17 => MenuResponse { menu: Bytes, response: Bytes },
    18 => GiveKillstreak { name: Bytes },
    19 => ActionSlot { slot },
    20 => ResupplyAmmo,
    21 => SetProfile { profile },
    22 => ToggleGod,
    23 => ChangeWeaponCamo { weapon, model },
    24 => Chat { team_only, text: Bytes },
    25 => ToggleProjectileFlight,
    26 => EndGracePeriod,
    27 => InputCommand { command },
    28 => SoundDone { entity, alias_index },
});

pub fn encode_actions(out: &mut WireWriter, actions: &[(ClientId, ClientAction)]) {
    put_slice(actions, out);
}

pub fn decode_actions(
    input: &mut WireReader<'_>,
) -> Result<Vec<(ClientId, ClientAction)>, WireError> {
    Wire::get(input)
}

pub(crate) fn encode_action(out: &mut WireWriter, action: &ClientAction) {
    action.put(out);
}

pub(crate) fn decode_action(input: &mut WireReader<'_>) -> Result<ClientAction, WireError> {
    Wire::get(input)
}

pub(crate) fn encode_event(out: &mut WireWriter, event: &SimEvent) {
    event.put(out);
}

pub(crate) fn decode_event(input: &mut WireReader<'_>) -> Result<SimEvent, WireError> {
    Wire::get(input)
}

pub(super) fn encode_fire_cause(out: &mut WireWriter, cause: Option<sim::FireCause>) {
    cause.put(out);
}

pub(super) fn decode_fire_cause(
    input: &mut WireReader<'_>,
) -> Result<Option<sim::FireCause>, WireError> {
    Wire::get(input)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnapshotMetaSectionBytes {
    pub match_header: usize,
    pub events: usize,
    pub aliases: usize,
    pub entity_dobjs: usize,
    pub corpses: usize,
    pub entities: usize,
    pub script_movers: usize,
    pub generations: usize,
    pub item_tables: usize,
    pub area_entities: usize,
    pub objectives: usize,
    pub world_objects: usize,
}

impl SnapshotMetaSectionBytes {
    pub fn segments(self) -> [usize; META_SEGMENTS] {
        [
            self.match_header,
            self.events,
            self.aliases,
            self.entity_dobjs,
            self.corpses,
            self.entities,
            self.script_movers,
            self.generations,
            self.item_tables,
            self.area_entities,
            self.objectives,
            self.world_objects,
        ]
    }

    pub fn total(self) -> usize {
        self.segments().into_iter().sum()
    }
}

pub fn encode_world_objects_wire(out: &mut WireWriter, world_objects_wire: &[u8]) -> usize {
    let mark = out.len();
    put_len::<u16>(world_objects_wire.len(), out);
    out.put_bytes(world_objects_wire);
    out.len() - mark
}

pub fn encode_snapshot_meta_body(
    out: &mut WireWriter,
    meta: &SnapshotMeta,
    keep_journal: impl Fn(&EventRecord) -> bool,
) -> SnapshotMetaSectionBytes {
    let mut sizes = SnapshotMetaSectionBytes::default();
    let mut mark = out.len();
    let mut close = |out: &WireWriter, size: &mut usize| {
        *size = out.len() - mark;
        mark = out.len();
    };
    meta.phase.put(out);
    meta.round_serial.put(out);
    meta.match_elapsed_ms.put(out);
    let (prematch_tag, elapsed_ms) = match meta.prematch {
        gamemode_iw4::PrematchStep::Waiting { elapsed_ms } => (0, elapsed_ms),
        gamemode_iw4::PrematchStep::Starting { elapsed_ms } => (1, elapsed_ms),
        gamemode_iw4::PrematchStep::Done => (2, 0),
    };
    out.put_u8(prematch_tag);
    out.put_u32(elapsed_ms);
    meta.score_limit.put(out);
    meta.time_limit_ms.put(out);
    out.put_u8(meta.kind.wire_tag());
    meta.clients.put(out);
    close(out, &mut sizes.match_header);
    let journal: Vec<&EventRecord> = meta.journal.iter().filter(|r| keep_journal(r)).collect();
    put_len::<u16>(journal.len(), out);
    for record in journal {
        record.put(out);
    }
    meta.entity_events.put(out);
    meta.pellet_fx.put(out);
    close(out, &mut sizes.events);
    AliasTable::put(&meta.sound_aliases, out);
    AliasTable::put(&meta.effect_names, out);
    AliasTable::put(&meta.hud_materials, out);
    meta.hud_strings.put(out);
    close(out, &mut sizes.aliases);
    meta.entity_dobjs.put(out);
    close(out, &mut sizes.entity_dobjs);
    meta.corpses.put(out);
    close(out, &mut sizes.corpses);
    meta.entities.put(out);
    meta.eq_disabled_entities.put(out);
    close(out, &mut sizes.entities);
    meta.script_movers.put(out);
    close(out, &mut sizes.script_movers);
    meta.generations.put(out);
    close(out, &mut sizes.generations);
    meta.item_ammo.put(out);
    meta.item_pickups.put(out);
    close(out, &mut sizes.item_tables);
    meta.area_entities.put(out);
    close(out, &mut sizes.area_entities);
    meta.objectives.put(out);
    meta.campaign_dvars.put(out);
    close(out, &mut sizes.objectives);
    sizes
}

pub const META_SEGMENTS: usize = 12;

pub(super) struct DecodedSnapshotMeta {
    pub(super) body: SnapshotMeta,
    pub(super) world_wire: Vec<u8>,
    pub(super) world_update: super::world_object_wire::WorldObjectUpdate,
}

pub(super) fn decode_snapshot_meta_body(
    input: &mut WireReader<'_>,
    remaining_after: &mut [usize; META_SEGMENTS],
) -> Result<DecodedSnapshotMeta, WireError> {
    let body = decode_body(input, remaining_after)?;
    let mut wire = vec![0u8; usize::from(input.get_u16()?)];
    input.get_bytes(&mut wire)?;
    remaining_after[11] = input.remaining();
    let world_update = super::world_object_wire::decode_sync(&wire)?;
    Ok(DecodedSnapshotMeta {
        body,
        world_wire: wire,
        world_update,
    })
}

fn decode_body(
    input: &mut WireReader<'_>,
    remaining_after: &mut [usize; META_SEGMENTS],
) -> Result<SnapshotMeta, WireError> {
    let phase = Wire::get(input)?;
    let round_serial = Wire::get(input)?;
    let match_elapsed_ms = Wire::get(input)?;
    let prematch_tag = input.get_u8()?;
    let elapsed_ms = input.get_u32()?;
    let prematch = match prematch_tag {
        0 => gamemode_iw4::PrematchStep::Waiting { elapsed_ms },
        1 => gamemode_iw4::PrematchStep::Starting { elapsed_ms },
        2 => gamemode_iw4::PrematchStep::Done,
        _ => return Err(WireError::Malformed("unknown prematch tag")),
    };
    let score_limit = Wire::get(input)?;
    let time_limit_ms = Wire::get(input)?;
    let kind = gamemode_iw4::GameModeKind::from_wire_tag(input.get_u8()?)
        .ok_or(WireError::Malformed("unknown GameModeKind tag"))?;
    let clients = Wire::get(input)?;
    remaining_after[0] = input.remaining();
    let journal = Wire::get(input)?;
    let entity_events = Wire::get(input)?;
    let pellet_fx = Wire::get(input)?;
    remaining_after[1] = input.remaining();
    let sound_aliases = AliasTable::get(input)?;
    let effect_names = AliasTable::get(input)?;
    let hud_materials = AliasTable::get(input)?;
    let hud_strings: Vec<(u16, String)> = Wire::get(input)?;
    if hud_strings.len() >= sim::CS_LOCALIZED_STRINGS_SLOTS {
        return Err(WireError::Malformed(
            "hud string count exceeds its configstrings",
        ));
    }
    remaining_after[2] = input.remaining();
    let entity_dobjs = Wire::get(input)?;
    remaining_after[3] = input.remaining();
    let corpses = Wire::get(input)?;
    remaining_after[4] = input.remaining();
    let entities = Wire::get(input)?;
    let eq_disabled_entities: Vec<u16> = Wire::get(input)?;
    if eq_disabled_entities.len() > 2048
        || eq_disabled_entities.iter().any(|number| *number >= 2048)
        || eq_disabled_entities
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(WireError::Malformed("invalid sound entity EQ bypass list"));
    }
    remaining_after[5] = input.remaining();
    let script_movers = Wire::get(input)?;
    remaining_after[6] = input.remaining();
    let generations = Wire::get(input)?;
    remaining_after[7] = input.remaining();
    let item_ammo = Wire::get(input)?;
    let item_pickups = Wire::get(input)?;
    remaining_after[8] = input.remaining();
    let area_entities = Wire::get(input)?;
    remaining_after[9] = input.remaining();
    let objectives = Wire::get(input)?;
    let campaign_dvars = Wire::get(input)?;
    remaining_after[10] = input.remaining();
    Ok(SnapshotMeta {
        campaign_dvars,
        phase,
        round_serial,
        match_elapsed_ms,
        prematch,
        score_limit,
        time_limit_ms,
        kind,
        clients,
        journal,
        entity_events,
        pellet_fx,
        sound_aliases,
        effect_names,
        hud_materials,
        hud_strings,
        world_objects: sim::WorldObjectSnapshot::default(),
        area_entities,
        objectives,
        entity_dobjs,
        entities,
        script_movers,
        eq_disabled_entities,
        generations,
        corpses,
        item_ammo,
        item_pickups,
    })
}

wire_struct!(AreaSectorSnapshot {
    index,
    contents_entities,
    linkcontents_entities,
    entities,
    dist,
    axis,
    parent,
    child,
});

wire_struct!(AreaEntityLinkSnapshot {
    entity_num,
    world_sector,
    next_entity,
    linkcontents,
    linkmin,
    linkmax,
    contents,
    bounds_mid,
    bounds_half,
});

wire_struct!(AreaEntityWorldSnapshot {
    world_mid,
    world_half,
    free_prefix,
    contiguous_free_head,
    sectors,
    entities,
} check |area: &AreaEntityWorldSnapshot| {
    area.free_prefix.len() <= 1022
        && area.sectors.len() <= 1023
        && area.entities.len() <= 1024
        && area.validate().is_ok()
} => "invalid CM area-sector snapshot";);

struct AliasTable;

impl Via<Vec<(u8, String)>> for AliasTable {
    fn put(value: &Vec<(u8, String)>, out: &mut WireWriter) {
        put_len::<u8>(value.len(), out);
        for (index, name) in value {
            index.put(out);
            Text8::put(name, out);
        }
    }
    fn get(input: &mut WireReader<'_>) -> Result<Vec<(u8, String)>, WireError> {
        (0..input.get_u8()?)
            .map(|_| Ok((u8::get(input)?, Text8::get(input)?)))
            .collect()
    }
}

wire_struct!(sim::FireCause { client, life, command, ordinal, hand }
    check |cause: &sim::FireCause| cause.hand <= 1 => "invalid fire cause hand";);

wire_struct!(EntityEventPayload {
    number,
    other_entity_num,
    attacker_entity_num,
    event_parm,
    weapon,
    correlation,
    fire_cause,
    pellet,
    segment,
    hand,
    origin,
    origin2,
    direction,
    surf_type,
    surface_flags,
    simulation_flags,
});

wire_struct!(EntityEventRecord { sequence, tick, audience, event, payload }
    check |record: &EntityEventRecord| {
        record.event != entity_iw4::EntityEventKind::NONE
            && (0..=entity_iw4::EntityEventKind::STOP_RUMBLE.0).contains(&record.event.0)
    } => "entity event outside the event table";);

wire_struct!(PelletFxRecord {
    attacker,
    weapon,
    correlation,
    fire_cause,
    pellet,
    segment,
    hand,
    start,
    end,
    normal,
    surf_type,
    surface_flags,
    flesh_flags,
});

wire_struct!(EventRecord {
    sequence,
    tick,
    audience,
    event
});

wire_struct!(sim::ScriptControls {
    frozen,
    weapons_disabled,
    offhands_disabled,
    switch_disabled,
    jump_disabled,
    usability_disabled,
    linked,
    switch_to,
});

impl Wire for sim::RemoteMissile {
    fn put(&self, out: &mut WireWriter) {
        self.projectile.put(out);
        self.entnum.put(out);
        self.angles.put(out);
        out.put_u8(
            u8::from(self.armed) | (u8::from(self.boosted) << 1) | (u8::from(self.attack) << 2),
        );
        out.put_i32(self.unlink_at_ms.unwrap_or(i32::MIN));
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        let projectile = Wire::get(input)?;
        let entnum = Wire::get(input)?;
        let angles = Wire::get(input)?;
        let flags = input.get_u8()?;
        Ok(Self {
            projectile,
            entnum,
            angles,
            armed: flags & 1 != 0,
            boosted: flags & 2 != 0,
            attack: flags & 4 != 0,
            unlink_at_ms: Some(input.get_i32()?).filter(|&ms| ms != i32::MIN),
        })
    }
}

wire_struct!(sim::LinkedWeaponView {
    entity_num,
    origin,
    angles
});

wire_struct!(LoadoutSpec {
    class_id,
    primary,
    secondary,
    primary_attachments,
    secondary_attachments,
    lethal,
    tactical,
    perks,
});

wire_struct!(sim::PendingBrass {
    cause,
    life,
    shot,
    weapon
});
wire_struct!(sim::VisionChange {
    name,
    duration_ms,
    set_ms
});
wire_struct!(sim::ScriptBlur {
    from,
    to,
    set_ms,
    duration_ms
});

wire_struct!(sim::ScriptDepthOfField {
    near_start,
    near_end,
    far_start,
    far_end,
    near_blur,
    far_blur,
});

wire_enum!(sim::MenuCommandKind, "bad menu command tag" {
    0 => Open(menu),
    1 => ClosePopup,
    2 => CloseInGame,
});

wire_struct!(sim::MenuCommand { serial, kind });
wire_struct!(sim::LocationSelection {
    material,
    choose_direction,
    radius
});

struct MissileTargetWire;

impl Via<Option<sim::MissileTarget>> for MissileTargetWire {
    fn put(value: &Option<sim::MissileTarget>, out: &mut WireWriter) {
        super::delta::encode_missile_target(out, *value);
    }
    fn get(input: &mut WireReader<'_>) -> Result<Option<sim::MissileTarget>, WireError> {
        super::delta::decode_missile_target(input)
    }
}

wire_struct!(sim::WeaponLock {
    weapon,
    life,
    flags,
    target,
    aim: MissileTargetWire,
    acquire_started_at,
});

struct ShieldWire;

impl Via<Option<sim::ShieldAttachment>> for ShieldWire {
    fn put(value: &Option<sim::ShieldAttachment>, out: &mut WireWriter) {
        out.put_u32(value.map_or(0, |shield| shield.weapon));
        u8::from(value.is_some_and(|shield| shield.on_back)).put(out);
    }
    fn get(input: &mut WireReader<'_>) -> Result<Option<sim::ShieldAttachment>, WireError> {
        let weapon = input.get_u32()?;
        let on_back = bool::get(input)?;
        Ok((weapon != 0).then_some(sim::ShieldAttachment { weapon, on_back }))
    }
}

impl Wire for ClientSnapshotMeta {
    fn put(&self, out: &mut WireWriter) {
        let carry = &self.weapon_carry;
        self.equipped_class.put(out);
        self.class_refusal.put(out);
        self.controls.put(out);
        out.put_u8(match self.killcam_hud {
            None => 0,
            Some(hud) => 1 + u8::from(hud.final_kill),
        });
        self.lifecycle.put(out);
        self.life_sequence.put(out);
        self.item_use_spawn_ms.put(out);
        self.item_use_entity.put(out);
        self.item_use_press_ms.put(out);
        carry.ammo_clip.put(out);
        carry.ammo_stock.put(out);
        self.score.put(out);
        self.kills.put(out);
        self.deaths.put(out);
        self.kill_streak.put(out);
        out.put_u8(self.radar.wire_tag());
        self.radar_blocked.put(out);
        self.remote_missile.put(out);
        self.linked_weapon_view.put(out);
        self.loadout.put(out);
        carry.ammo_by_weapon.put(out);
        Len8::put(&carry.taped_mag_spent, out);
        carry.weapon_shot_count.put(out);
        out.put_u8(u8::from(carry.burst_latch) | (u8::from(carry.burst_latch_secondary) << 1));
        out.put_u8(
            u8::from(carry.rechamber_pending) | (u8::from(carry.rechamber_pending_secondary) << 1),
        );
        carry.pending_brass.put(out);
        self.god_mode.put(out);
        self.dead_since_tick.put(out);
        ShieldWire::put(&self.shield, out);
        put_shield_collision(out, self.shield_collision.as_ref());
        self.look_at_killer_yaw.put(out);
        Bytes::put(&self.name, out);
        put_hud_bank(out, &self.hud_archival);
        put_hud_bank(out, &self.hud_current);
        self.ffa_team.put(out);
        self.client_state_team.put(out);
        self.viewmodel.put(out);
        self.actor_overlay.is_some().put(out);
        if let Some(overlay) = &self.actor_overlay {
            overlay.entity.put(out);
            overlay.name.put(out);
            overlay.description.put(out);
            overlay.hostile.put(out);
        }
        self.rank.put(out);
        self.prestige.put(out);
        self.player_card_icon.put(out);
        self.player_card_title.put(out);
        self.player_card_nameplate.put(out);
        self.client_dvars.put(out);
        ShockWire::put(&self.shellshock, out);
        let effects = &self.view_effects;
        effects.naked_vision.put(out);
        effects.thermal_vision.put(out);
        effects.missile_vision.put(out);
        effects.night_vision.put(out);
        effects.pain_vision.put(out);
        effects.blur.put(out);
        effects.depth_of_field.put(out);
        debug_assert!(self.menu_commands.len() <= sim::MENU_COMMAND_TAIL);
        Len8::put(&self.menu_commands, out);
        self.location_selection.put(out);
        self.weapon_lock.put(out);
    }

    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        let equipped_class = Wire::get(input)?;
        let class_refusal = Wire::get(input)?;
        let controls = Wire::get(input)?;
        let killcam_hud = match input.get_u8()? {
            0 => None,
            tag @ (1 | 2) => Some(sim::KillcamHud {
                final_kill: tag == 2,
            }),
            _ => return Err(WireError::Malformed("bad killcam HUD tag")),
        };
        let lifecycle = Wire::get(input)?;
        let life_sequence = Wire::get(input)?;
        let item_use_spawn_ms = Wire::get(input)?;
        let item_use_entity = Wire::get(input)?;
        let item_use_press_ms = Wire::get(input)?;
        let ammo_clip = Wire::get(input)?;
        let ammo_stock = Wire::get(input)?;
        let score = Wire::get(input)?;
        let kills = Wire::get(input)?;
        let deaths = Wire::get(input)?;
        let kill_streak = Wire::get(input)?;
        let radar = sim::RadarMode::from_wire_tag(input.get_u8()?)
            .ok_or(WireError::Malformed("bad radar mode"))?;
        let radar_blocked = Wire::get(input)?;
        let remote_missile = Wire::get(input)?;
        let linked_weapon_view = Wire::get(input)?;
        let loadout = Wire::get(input)?;
        let ammo_by_weapon = Wire::get(input)?;
        let taped_mag_spent = Len8::get(input)?;
        let weapon_shot_count = Wire::get(input)?;
        let burst_latch_hands = input.get_u8()?;
        let rechamber_pending_hands = input.get_u8()?;
        let pending_brass = Wire::get(input)?;
        let god_mode = Wire::get(input)?;
        let dead_since_tick = Wire::get(input)?;
        let shield = ShieldWire::get(input)?;
        let shield_collision = get_shield_collision(input, shield.is_some())?;
        let look_at_killer_yaw = Wire::get(input)?;
        let name = Bytes::get(input)?;
        let hud_archival = get_hud_bank(input)?;
        let hud_current = get_hud_bank(input)?;
        let ffa_team = Wire::get(input)?;
        let client_state_team = Wire::get(input)?;
        let viewmodel = Wire::get(input)?;
        let actor_overlay = if bool::get(input)? {
            let entity = u16::get(input)?;
            if entity >= (playerstate_iw4::ENTITYNUM_NONE - 1) as u16 {
                return Err(WireError::Malformed("invalid actor overlay entity"));
            }
            Some(sim::ActorOverlay {
                entity,
                name: Wire::get(input)?,
                description: Wire::get(input)?,
                hostile: Wire::get(input)?,
            })
        } else {
            None
        };
        let rank = Wire::get(input)?;
        let prestige = Wire::get(input)?;
        let player_card_icon = Wire::get(input)?;
        let player_card_title = Wire::get(input)?;
        let player_card_nameplate = Wire::get(input)?;
        let client_dvars = Wire::get(input)?;
        let shellshock = ShockWire::get(input)?;
        let view_effects = sim::ViewEffects {
            naked_vision: Wire::get(input)?,
            thermal_vision: Wire::get(input)?,
            missile_vision: Wire::get(input)?,
            night_vision: Wire::get(input)?,
            pain_vision: Wire::get(input)?,
            blur: Wire::get(input)?,
            depth_of_field: Wire::get(input)?,
        };
        let menu_commands = Len8::get(input)?;
        let location_selection = Wire::get(input)?;
        let weapon_lock = Wire::get(input)?;
        Ok(ClientSnapshotMeta {
            weapon_carry: sim::WeaponCarry {
                ammo_clip,
                ammo_stock,
                ammo_by_weapon,
                taped_mag_spent,
                weapon_shot_count,
                burst_latch: burst_latch_hands & 1 != 0,
                burst_latch_secondary: burst_latch_hands & 2 != 0,
                rechamber_pending: rechamber_pending_hands & 1 != 0,
                rechamber_pending_secondary: rechamber_pending_hands & 2 != 0,
                pending_brass,
            },
            equipped_class,
            class_refusal,
            god_mode,
            controls,
            weapon_lock,
            killcam_hud,
            lifecycle,
            loadout,
            life_sequence,
            item_use_spawn_ms,
            item_use_entity,
            item_use_press_ms,
            score,
            kills,
            deaths,
            kill_streak,
            radar,
            radar_blocked,
            remote_missile,
            linked_weapon_view,
            dead_since_tick,
            shield,
            shield_collision,
            look_at_killer_yaw,
            name,
            hud_archival,
            hud_current,
            ffa_team,
            client_state_team,
            viewmodel,
            actor_overlay,
            rank,
            prestige,
            player_card_icon,
            player_card_title,
            player_card_nameplate,
            client_dvars,
            shellshock,
            view_effects,
            menu_commands,
            location_selection,
        })
    }
}

fn put_shield_collision(out: &mut WireWriter, collision: Option<&sim::ShieldCarrierCollision>) {
    let bones = collision.map_or(&[][..], |collision| collision.bones.as_slice());
    put_len::<u16>(bones.len(), out);
    let Some(collision) = collision.filter(|_| !bones.is_empty()) else {
        return;
    };
    collision.normal.put(out);
    for bone in bones {
        bone.bone.put(out);
        bone.part_classification.put(out);
        bone.center.put(out);
        bone.axes.put(out);
        bone.half_size.put(out);
    }
}

fn get_shield_vector(input: &mut WireReader<'_>) -> Result<[f32; 3], WireError> {
    let vector: [f32; 3] = Wire::get(input)?;
    if vector.iter().all(|value| value.is_finite()) {
        Ok(vector)
    } else {
        Err(WireError::Malformed("nonfinite shield collision"))
    }
}

fn get_shield_collision(
    input: &mut WireReader<'_>,
    shielded: bool,
) -> Result<Option<sim::ShieldCarrierCollision>, WireError> {
    let count = usize::from(input.get_u16()?);
    if count > 256 || (count != 0 && !shielded) {
        return Err(WireError::Malformed("bad shield collision count"));
    }
    if count == 0 {
        return Ok(None);
    }
    let normal = get_shield_vector(input)?;
    let mut bones = Vec::with_capacity(count);
    for _ in 0..count {
        let bone = input.get_u16()?;
        let part_classification = input.get_u8()?;
        if part_classification > 19 {
            return Err(WireError::Malformed("bad shield carrier hit location"));
        }
        let center = get_shield_vector(input)?;
        let axes = [
            get_shield_vector(input)?,
            get_shield_vector(input)?,
            get_shield_vector(input)?,
        ];
        let half_size = get_shield_vector(input)?;
        if half_size.iter().any(|v| *v < 0.0) {
            return Err(WireError::Malformed("negative shield collision size"));
        }
        bones.push(xmodel_runtime::CollisionBone {
            bone,
            part_classification,
            center,
            axes,
            half_size,
        });
    }
    Ok(Some(sim::ShieldCarrierCollision { normal, bones }))
}

struct ShockWire;

impl Via<Option<hud_iw4::ShockParams>> for ShockWire {
    fn put(value: &Option<hud_iw4::ShockParams>, out: &mut WireWriter) {
        let Some(shock) = value else {
            out.put_u8(0);
            return;
        };
        out.put_u8(3);
        shock.screen_type.put(out);
        shock.white_fade_ms.put(out);
        shock.shot_fade_ms.put(out);
        shock.blur_blend_ms.put(out);
        shock.blur_fade_ms.put(out);
        shock.look.affect.put(out);
        shock.look.fade_ms.put(out);
        shock.look.mouse_sensitivity.put(out);
        shock.look.max_pitch_speed.put(out);
        shock.look.max_yaw_speed.put(out);
        shock.sound.affect.put(out);
        shock.sound.loop_alias.put(out);
        shock.sound.end_alias.put(out);
        shock.sound.abort_alias.put(out);
        shock.movement.put(out);
        match &shock.sound.channel_volumes {
            None => out.put_u8(0),
            Some(volumes) => {
                out.put_u8(1);
                put_len::<u8>(volumes.len(), out);
                for (name, gain) in volumes {
                    name.put(out);
                    gain.put(out);
                }
            }
        }
    }

    fn get(input: &mut WireReader<'_>) -> Result<Option<hud_iw4::ShockParams>, WireError> {
        let tag = input.get_u8()?;
        if tag == 0 {
            return Ok(None);
        }
        if tag > 3 {
            return Err(WireError::Malformed("bad shellshock tag"));
        }
        let mut shock = hud_iw4::ShockParams {
            screen_type: input.get_i32()?,
            white_fade_ms: input.get_i32()?,
            shot_fade_ms: input.get_i32()?,
            blur_blend_ms: if tag == 3 { input.get_i32()? } else { 0 },
            blur_fade_ms: if tag == 3 { input.get_i32()? } else { 0 },
            look: hud_iw4::ShellshockLookParms {
                affect: input.get_u8()? != 0,
                fade_ms: input.get_i32()?,
                mouse_sensitivity: input.get_f32()?,
                max_pitch_speed: input.get_f32()?,
                max_yaw_speed: input.get_f32()?,
            },
            sound: hud_iw4::ShellshockSoundParms {
                channel_volumes: None,
                affect: input.get_u8()? != 0,
                loop_alias: Wire::get(input)?,
                end_alias: Wire::get(input)?,
                abort_alias: Wire::get(input)?,
            },
            movement: input.get_u8()? != 0,
        };
        if shock.blur_blend_ms < 0 || shock.blur_fade_ms < 0 {
            return Err(WireError::Malformed("negative shellshock blur time"));
        }
        if tag >= 2 && bool::get(input)? {
            let count = input.get_u8()?;
            if count > 64 {
                return Err(WireError::Malformed("too many shellshock channel volumes"));
            }
            let mut volumes = std::collections::BTreeMap::new();
            for _ in 0..count {
                let name = String::get(input)?;
                let gain = input.get_f32()?;
                if name.is_empty()
                    || name.len() > 64
                    || name != name.to_ascii_lowercase()
                    || !gain.is_finite()
                    || !(0.0..=1.0).contains(&gain)
                    || volumes.insert(name, gain).is_some()
                {
                    return Err(WireError::Malformed("invalid shellshock channel volume"));
                }
            }
            shock.sound.channel_volumes = Some(volumes);
        }
        Ok(Some(shock))
    }
}

fn put_hud_bank(out: &mut WireWriter, bank: &[hud_iw4::HudElem]) {
    let bank = &bank[..bank.len().min(hud_iw4::HUDELEM_BANK_CAPACITY)];
    put_len::<u8>(bank.len(), out);
    for elem in bank {
        elem.put(out);
    }
}

fn get_hud_bank(input: &mut WireReader<'_>) -> Result<Vec<hud_iw4::HudElem>, WireError> {
    let bank: Vec<hud_iw4::HudElem> = Len8::get(input)?;
    if bank.len() > hud_iw4::HUDELEM_BANK_CAPACITY {
        return Err(WireError::Malformed("hud bank longer than 31"));
    }
    Ok(bank)
}

wire_struct!(hud_iw4::HudElem {
    elem_type,
    y,
    x,
    z,
    target_ent_num,
    font_scale,
    from_font_scale,
    font_scale_start_time,
    font_scale_time,
    label,
    font,
    align_org,
    align_screen,
    color_rgba,
    from_color_rgba,
    fade_start_time,
    fade_time,
    height,
    width,
    material_index,
    from_y,
    from_x,
    from_align_org,
    from_align_screen,
    move_start_time,
    move_time,
    from_height,
    from_width,
    scale_start_time,
    scale_time,
    value,
    time,
    duration,
    text,
    sort,
    glow_color_rgba,
    fx_birth_time,
    fx_letter_time,
    fx_decay_start_time,
    fx_decay_duration,
    sound_id,
    flags,
});

wire_struct!(AnimPair {
    legs_anim,
    torso_anim
});

wire_struct!(PlayerCorpseSlot {
    weapon_model,
    client_state_team,
    ffa_team: Sentinel8,
    victim,
    viewangles,
    anim,
    weapon,
    e_flags,
    entnum,
    tr_type,
    tr_time,
    tr_duration,
    tr_delta,
    tr_base,
    ground_entity_num,
    spawn_time_ms,
    shield: ShieldWire,
} rest PlayerCorpseSlot {
    occupied: true,
    ..PlayerCorpseSlot::default()
};);

impl Wire for PlayerCorpsePool {
    fn put(&self, out: &mut WireWriter) {
        for slot in &self.slots {
            slot.occupied.put(out);
            if slot.occupied {
                slot.put(out);
            }
        }
    }

    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        let mut pool = PlayerCorpsePool::default();
        for slot in &mut pool.slots {
            if bool::get(input)? {
                *slot = PlayerCorpseSlot::get(input)?;
            }
        }
        let mut entities = std::collections::HashSet::new();
        for slot in pool.slots.iter().filter(|slot| slot.occupied) {
            if !entities.insert(slot.entnum)
                || slot.entnum < playerstate_iw4::PLAYER_CORPSE_ENTITY_BASE
                || slot.entnum
                    >= playerstate_iw4::PLAYER_CORPSE_ENTITY_BASE
                        + playerstate_iw4::MAX_CLIENT_CORPSES
                || slot
                    .viewangles
                    .iter()
                    .chain(slot.tr_base.iter())
                    .chain(slot.tr_delta.iter())
                    .any(|n| !n.is_finite())
            {
                return Err(WireError::Malformed("invalid corpse occupancy"));
            }
        }
        Ok(pool)
    }
}

impl Wire for entity_iw4::EntityState {
    fn put(&self, out: &mut WireWriter) {
        self.number.put(out);
        self.e_type.put(out);
        self.e_flags.put(out);
        self.tr_time.put(out);
        self.tr_type.put(out);
        self.tr_duration.put(out);
        self.tr_delta.put(out);
        self.tr_base.put(out);
        self.apos_tr_time.put(out);
        self.apos_tr_type.put(out);
        self.apos_tr_duration.put(out);
        self.apos_tr_delta.put(out);
        self.apos_tr_base.put(out);
        self.solid.put(out);
        self.index.put(out);
        self.launch_time().put(out);
        self.client_num.put(out);
        self.legs_anim.put(out);
        self.torso_anim.put(out);
        self.ground_entity_num.put(out);
        self.event_sequence.put(out);
        self.events.put(out);
        self.event_parms.put(out);
    }

    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            number: Wire::get(input)?,
            e_type: Wire::get(input)?,
            e_flags: Wire::get(input)?,
            tr_time: Wire::get(input)?,
            tr_type: Wire::get(input)?,
            tr_duration: Wire::get(input)?,
            tr_delta: Wire::get(input)?,
            tr_base: Wire::get(input)?,
            apos_tr_time: Wire::get(input)?,
            apos_tr_type: Wire::get(input)?,
            apos_tr_duration: Wire::get(input)?,
            apos_tr_delta: Wire::get(input)?,
            apos_tr_base: Wire::get(input)?,
            solid: Wire::get(input)?,
            index: Wire::get(input)?,
            cull_dist: f32::from_bits(i32::get(input)? as u32),
            client_num: Wire::get(input)?,
            legs_anim: Wire::get(input)?,
            torso_anim: Wire::get(input)?,
            ground_entity_num: Wire::get(input)?,
            event_sequence: Wire::get(input)?,
            events: Wire::get(input)?,
            event_parms: Wire::get(input)?,
            ..Self::default()
        })
    }
}

impl Wire for sim::EntityBodyBounds {
    fn put(&self, out: &mut WireWriter) {
        self.midpoint.put(out);
        self.half_size.put(out);
        self.contents.put(out);
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        let value = Self {
            midpoint: Wire::get(input)?,
            half_size: Wire::get(input)?,
            contents: Wire::get(input)?,
        };
        if !value.valid() {
            return Err(WireError::Malformed("entity body bounds"));
        }
        Ok(value)
    }
}

impl Wire for sim::ScriptMoverGentity {
    fn put(&self, out: &mut WireWriter) {
        self.id.put(out);
        self.nonsolid.put(out);
        self.thermal_draw.put(out);
        out.put_u8(self.killcam_camera.map_or(0, |mode| mode.as_u8()));
        self.state.put(out);
        self.body.put(out);
    }

    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        let id = Wire::get(input)?;
        let nonsolid = Wire::get(input)?;
        let thermal_draw = Wire::get(input)?;
        let killcam_camera = match input.get_u8()? {
            0 => None,
            1 => Some(playerstate_iw4::KillCamMode::Mode1Heli),
            6 => Some(playerstate_iw4::KillCamMode::Mode6Turret),
            _ => return Err(WireError::Malformed("script mover camera")),
        };
        Ok(Self {
            id,
            state: Wire::get(input)?,
            body: Wire::get(input)?,
            killcam_camera,
            thermal_draw,
            nonsolid,
            ..Default::default()
        })
    }
}

wire_struct!(DroppedItemAmmo {
    entnum,
    clip_r,
    clip_l,
    stock,
    scavenger,
    parented,
    authored,
    dual_wield,
    alternate,
});

wire_struct!(sim::ItemWeaponAmmo { weapon, clip_r, clip_l, stock, dual_wield }
    check |ammo: &sim::ItemWeaponAmmo| ammo.weapon > 0 && ammo.clip_r >= 0 && ammo.clip_l >= 0 && ammo.stock >= 0 => "invalid alternate item ammo";);

wire_struct!(ItemPickupRecord {
    seq,
    picker,
    weapon,
    from_entnum,
    clip_r,
    clip_l,
    stock,
    swapped_entnum,
    picker_pm_type,
});

wire_enum!(sim::AuthorityModelOwner, "unknown DObj owner tag" {
    0 => ScriptModel(id),
});

wire_struct!(xmodel_runtime::DObjModelDescriptor {
    model,
    parent_model: Sentinel16,
    attach_tag,
    ignore_collision,
});

wire_struct!(xmodel_runtime::DObjCompositionDescriptor { revision, models });

wire_struct!(xmodel_runtime::TagRotation { tag, angles }
    check |rotation: &xmodel_runtime::TagRotation| {
        !rotation.tag.is_empty() && rotation.angles.iter().all(|angle| angle.is_finite())
    } => "invalid tag rotation";);

impl Wire for anim_iw4::PartBits {
    fn put(&self, out: &mut WireWriter) {
        for word in self.words() {
            word.put(out);
        }
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        Ok(Self::from_words(Wire::get(input)?))
    }
}

impl Wire for xmodel_runtime::HidePartBits {
    fn put(&self, out: &mut WireWriter) {
        for word in self.words() {
            word.put(out);
        }
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        Ok(Self::from_words(Wire::get(input)?))
    }
}

wire_enum!(xmodel_runtime::XAnimSemanticNodeKind, "unknown XAnim node tag" {
    0 => Blend,
    1 => Additive,
    2 => Leaf,
});

struct NodeParent;

impl Via<Option<xmodel_runtime::XAnimNodeId>> for NodeParent {
    fn put(value: &Option<xmodel_runtime::XAnimNodeId>, out: &mut WireWriter) {
        out.put_u16(value.map_or(u16::MAX, |id| id.0));
    }
    fn get(input: &mut WireReader<'_>) -> Result<Option<xmodel_runtime::XAnimNodeId>, WireError> {
        let parent = input.get_u16()?;
        Ok((parent != u16::MAX).then_some(xmodel_runtime::XAnimNodeId(parent)))
    }
}

struct CycleCount;

impl Via<i16> for CycleCount {
    fn put(value: &i16, out: &mut WireWriter) {
        out.put_i32(i32::from(*value));
    }
    fn get(input: &mut WireReader<'_>) -> Result<i16, WireError> {
        i16::try_from(input.get_i32()?)
            .map_err(|_| WireError::Malformed("XAnim cycle count overflow"))
    }
}

wire_struct!(xmodel_runtime::XAnimNodeState {
    time,
    old_time,
    cycle_count: CycleCount,
    old_cycle_count: CycleCount,
    goal_time,
    goal_weight,
    weight,
    rate,
});

wire_struct!(xmodel_runtime::XAnimSemanticNode {
    parent: NodeParent,
    kind,
    clip,
    parts,
    state,
});

wire_struct!(xmodel_runtime::XAnimTreeSnapshot {
    definition_revision,
    state_revision,
    nodes
});

wire_struct!(xmodel_runtime::DObjSemanticState {
    composition,
    pose_revision,
    tag_rotations,
    requested_parts,
    hide_part_bits,
    tree,
} check |state: &xmodel_runtime::DObjSemanticState| {
    state.tag_rotations.len() <= anim_iw4::PartBits::CAPACITY
} => "too many tag rotations";);

wire_enum!(sim::ObjectiveState, "objective state" {
    0 => Empty,
    1 => Active,
    2 => Invisible,
    3 => Done,
    4 => Current,
    5 => Failed,
});

wire_struct!(sim::CampaignObjective {
    index,
    state,
    text,
    positions,
    entity,
    offset,
    icon,
    pointer_text,
    ring
});

impl Wire for sim::CompassObjective {
    fn put(&self, out: &mut WireWriter) {
        self.index.put(out);
        out.put_u8(self.state as u8);
        self.origin.put(out);
        out.put_u8(self.team as u8);
        self.icon.put(out);
        out.put_i32(self.viewer.map_or(-1, |viewer| viewer as i32));
    }

    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            index: Wire::get(input)?,
            state: sim::ObjectiveState::from_u8(input.get_u8()?)
                .ok_or(WireError::Malformed("objective state"))?,
            origin: Wire::get(input)?,
            team: gamemode_iw4::Team::from_packed_u8(input.get_u8()?)
                .ok_or(WireError::Malformed("objective team"))?,
            icon: Wire::get(input)?,
            viewer: u32::try_from(input.get_i32()?).ok(),
        })
    }
}

wire_struct!(sim::ScriptEffect {
    id,
    effect,
    origin,
    forward,
    up,
    start_ms: Padded,
    repeat_ms,
    cull_distance,
    viewers: Padded,
});

wire_struct!(sim::ScriptSunFog {
    color_rgb,
    sun_dir,
    begin_angle_deg,
    end_angle_deg,
    scale
});

wire_struct!(sim::ScriptFogParams { start_dist, halfway_dist, color_rgb, max_opacity, sun }
    check |fog: &sim::ScriptFogParams| fog.valid() => "invalid fog parameters";);

wire_struct!(sim::ScriptFog { from, to, start_ms, duration_ms }
    check |fog: &sim::ScriptFog| fog.duration_ms >= 0 => "negative fog duration";);

wire_struct!(sim::ScriptEarthquake { id, origin, scale, radius, start_ms, duration_ms }
    check |quake: &sim::ScriptEarthquake| quake.valid() => "invalid earthquake";);

wire_struct!(sim::CompassVehicle {
    origin,
    yaw,
    owner,
    team,
    icons,
    size
});

wire_struct!(sim::ScriptSlowMotion { from, to, start_ms, duration_ms }
    check |plan: &sim::ScriptSlowMotion| plan.valid() => "invalid slow-motion transition";);

wire_struct!(sim::ScriptAmbient { alias, start_ms, end_ms, rate_percent }
    check |plan: &sim::ScriptAmbient| plan.valid() => "invalid ambient transition";);

wire_struct!(sim::VehicleHudTarget {
    slot,
    entity,
    model,
    owner
});

wire_struct!(sim::ObjectiveMatch {
    scores,
    compass: Len8,
    campaign: Len8,
    campaign_mode,
    server_info,
    game_end_time,
    scripted_effects,
    effects,
    naked_vision,
    thermal_vision,
    missile_vision,
    night_vision,
    pain_vision,
    fog,
    earthquakes: Len8,
    vehicles,
    slow_motion,
    ambient,
    ac130_ambient,
    rumble_aliases: Len32,
    thermal_body_material,
    vehicle_targets: Len8,
} check objective_tables_valid => "invalid objective tables";);

fn objective_tables_valid(state: &sim::ObjectiveMatch) -> bool {
    let rumbles = &state.rumble_aliases;
    let targets = &state.vehicle_targets;
    state.campaign.len() <= 32
        && state.campaign.iter().enumerate().all(|(at, row)| {
            row.index < 32
                && row.text.len() <= 1023
                && row.pointer_text.len() <= 63
                && row.icon.len() <= 63
                && row.entity.is_none_or(|entity| entity < 2046)
                && row
                    .positions
                    .iter()
                    .flatten()
                    .chain(&row.offset)
                    .all(|v| v.is_finite())
                && state.campaign[..at]
                    .iter()
                    .all(|old| old.index != row.index)
        })
        && state.earthquakes.len() <= sim::MAX_SCRIPT_EARTHQUAKES
        && rumbles.len() <= 255
        && rumbles.iter().enumerate().all(|(at, (index, name))| {
            (1..=255).contains(index)
                && !name.is_empty()
                && name.len() <= 1023
                && rumbles[..at].iter().all(|(old, _)| old != index)
        })
        && targets.len() <= 8
        && targets.iter().enumerate().all(|(at, target)| {
            target.slot < 8
                && i32::from(target.entity) < playerstate_iw4::ENTITYNUM_NONE
                && targets[..at].iter().all(|old| {
                    old.slot != target.slot
                        && old.entity != target.entity
                        && old.model != target.model
                })
        })
}
