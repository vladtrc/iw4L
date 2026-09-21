use entity_iw4::{
    GGlassPiece, GLASS_DAMAGE_INVALID, GlassStateChange, glass_apply_damage, glass_collapse_piece,
    glass_is_solid, glass_shatter_seed_from_hit, glass_should_notify_destroyed,
    glass_state_from_damage, glass_weakened_collapse_time_cs,
};
use gamemode_iw4::{
    VEHICLE_HEALTHDRAIN_AMOUNT, VEHICLE_HEALTHDRAIN_INTERVAL_MS, VEHICLE_LOOPFX_INTERVAL_MS,
    VehicleBodyState, apply_destructible_part_player_bullet, apply_toy_damage,
    apply_vehicle_player_bullet, g_radius_damage_amount, radius_damage_distance_to_aabb,
    vehicle_active_loop_fx, vehicle_death_fx_if_destroyed, vehicle_death_presentation_if_destroyed,
    vehicle_healthdrain_arms,
};

pub use gamemode_iw4::{ToyDestructibleKind, VehicleDestructibleKind};

use crate::bullet_collision::EntityCollisionEpoch;
use crate::identities::{DamageSource, LifeSequence, PelletId, ScriptModelId};
use crate::world::ClientId;

pub use entity_iw4::{
    GLASS_BLAST_DAMAGE_SCALE, GLASS_BLAST_RADIUS_CAP, GLASS_DAMAGE_TO_DESTROY,
    GLASS_DAMAGE_TO_WEAKEN, GLASS_FRACTURE_PROFILE_VERSION, GLASS_MELEE_DAMAGE,
    GLASS_PROJECTILE_PANE_HOPS, GlassBreakRecord, GlassCause, GlassPaneBasis, GlassPieceState,
    GlassShatterSeed, MISSILE_GLASS_SHATTER_VEL, glass_blast_cone_keeps,
    glass_blast_integer_damage,
};

pub type GlassPieceId = u32;

#[derive(Clone, Debug, PartialEq)]
pub struct DestructableInstall {
    pub id: ScriptModelId,
    pub origin: [f32; 3],
    pub accumulate: Option<i32>,
    pub threshold: Option<i32>,
    pub script_destructable_area: String,
    pub has_fx: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlammableCrateInstall {
    pub id: ScriptModelId,
    pub origin: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FlammableCrateBody {
    health: i32,
    burning: bool,
    destroyed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructableDown {
    pub id: ScriptModelId,
    pub play_fx: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct DestructableBody {
    accumulate: i32,
    threshold: i32,
    dmg: i32,
    destroyed: bool,
    areas: Vec<String>,
    has_fx: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlassPieceSnapshot {
    pub state: GlassPieceState,
    pub revision: u32,
    pub last_state_change_time: i32,
    pub shatter_seed: Option<GlassShatterSeed>,
    pub deterministic_seed: u64,
    pub cause: GlassCause,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DestructibleStateIndex(u8);

impl DestructibleStateIndex {
    pub const INITIAL: Self = Self(0);

    pub const fn new(raw: u8) -> Self {
        Self(raw)
    }

    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldObjectSnapshot {
    pub as_of_ms: i32,
    pub map_round_epoch: u32,
    pub fracture_profile_version: u32,
    pub destructible_stages: Vec<(ScriptModelId, u8)>,
    pub glass_pieces: Vec<(GlassPieceId, GlassPieceSnapshot)>,

    /// The loops a destructible keeps speaking while it sits in its stage --
    /// leaking gas, burning -- each one an alias the client resolves out of
    /// the sound alias configstring.
    pub destructible_loop_sounds: Vec<DestructibleLoopSound>,
}

/// One looping alias a destructible is speaking, placed in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DestructibleLoopSound {
    pub owner: ScriptModelId,
    pub alias_index: u8,
    pub origin: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructibleDamageIntent {
    pub source: DamageSource,
    pub pellet: PelletId,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub target: ScriptModelId,
    pub amount: u32,

    /// `true` for radius damage, which is what the GSC damage filters and the
    /// per-type splash scaler key off.
    pub splash: bool,

    pub epoch: EntityCollisionEpoch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DestructibleExplodeEvent {
    pub owner: ScriptModelId,
    pub origin: [f32; 3],
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub source: DamageSource,

    pub explode_range_mp: u32,

    pub explode_damage: (u32, u32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleDumpRow {
    pub id: ScriptModelId,
    pub kind: VehicleDestructibleKind,
    pub body: VehicleBodyState,
    pub healthdrain: bool,
    pub loop_fx: Option<&'static str>,
    pub death_fx: Option<&'static str>,
    pub death_sound: Option<&'static str>,
    pub death_clip: Option<&'static str>,
    pub husk: Option<&'static str>,
    pub death_anim_time: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrainFamily {
    Vehicle,
    Barrel,
    Toy,
    Crate,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VehicleHealthDrain {
    wait_ms: u32,
    amount: u32,
    interval_ms: u32,
    attacker: ClientId,
    attacker_life: LifeSequence,
    source: DamageSource,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleFxPulse {
    pub owner: ScriptModelId,
    pub origin: [f32; 3],
    pub def_name: &'static str,
    pub tag: Option<&'static str>,

    /// `false` plays the effect at the tag with a fixed world forward instead
    /// of the tag's own orientation.
    pub use_tag_angles: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleSoundPulse {
    pub owner: ScriptModelId,
    pub origin: [f32; 3],
    pub alias: &'static str,
    pub tag: Option<&'static str>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DestructibleApplyReport {
    pub changed: usize,
    pub explodes: Vec<DestructibleExplodeEvent>,
}

/// What one damage application did to one destructible. `None` from an
/// `apply_*_amount` means the target is not of that family, or nothing moved.
#[derive(Clone, Debug, Default, PartialEq)]
struct DestructibleApply {
    changed: bool,
    explodes: Vec<DestructibleExplodeEvent>,
}

impl DestructibleApply {
    fn changed() -> Self {
        Self {
            changed: true,
            explodes: Vec::new(),
        }
    }

    fn exploded(explode: DestructibleExplodeEvent) -> Self {
        Self {
            changed: true,
            explodes: vec![explode],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldObjectState {
    destructible_stages: Vec<(ScriptModelId, u8)>,
    vehicle_bodies: Vec<(ScriptModelId, VehicleDestructibleKind, VehicleBodyState)>,

    vehicle_origins: Vec<(ScriptModelId, [f32; 3])>,
    vehicle_drains: Vec<(ScriptModelId, VehicleHealthDrain)>,

    vehicle_loopfx: Vec<(ScriptModelId, &'static str, u32)>,

    vehicle_death_fx_emitted: Vec<ScriptModelId>,
    death_anim_times: Vec<(ScriptModelId, f32)>,

    toy_bodies: Vec<(ScriptModelId, ToyDestructibleKind, VehicleBodyState)>,
    toy_origins: Vec<(ScriptModelId, [f32; 3])>,
    toy_drains: Vec<(ScriptModelId, VehicleHealthDrain)>,
    toy_fx_pulses: Vec<VehicleFxPulse>,
    toy_sound_pulses: Vec<VehicleSoundPulse>,
    toy_part_launches: Vec<ScriptModelId>,
    destructible_loop_sounds: Vec<DestructibleLoopSound>,

    barrel_bodies: Vec<(ScriptModelId, VehicleBodyState)>,
    barrel_origins: Vec<(ScriptModelId, [f32; 3])>,

    barrel_drains: Vec<(ScriptModelId, VehicleHealthDrain)>,

    barrel_burn_start: Vec<VehicleFxPulse>,
    pending_barrel_downs: Vec<ScriptModelId>,
    glass_pieces: Vec<(GlassPieceId, GGlassPiece)>,

    glass_native: Vec<(GlassPieceId, GlassNativeMeta)>,

    glass_panes: Vec<(GlassPieceId, GlassPaneBasis)>,

    pending_glass_destroyed: Vec<GlassPieceId>,

    destructable_bodies: Vec<(ScriptModelId, DestructableBody)>,
    destructable_origins: Vec<(ScriptModelId, [f32; 3])>,
    blocked_spawn_areas: Vec<String>,
    pending_destructable_downs: Vec<DestructableDown>,
    crate_bodies: Vec<(ScriptModelId, FlammableCrateBody)>,
    crate_origins: Vec<(ScriptModelId, [f32; 3])>,
    crate_drains: Vec<(ScriptModelId, VehicleHealthDrain)>,
    pending_crate_downs: Vec<ScriptModelId>,
    map_round_epoch: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GlassNativeMeta {
    revision: u32,
    cause: GlassCause,
    deterministic_seed: u64,
}

impl WorldObjectState {
    pub fn set_destructible_state(&mut self, id: ScriptModelId, state: DestructibleStateIndex) {
        if state == DestructibleStateIndex::INITIAL {
            remove_key(&mut self.destructible_stages, id);
            return;
        }
        upsert_value(&mut self.destructible_stages, id, state.as_u8());
    }

    pub fn install_vehicle_destructibles(
        &mut self,
        vehicles: impl IntoIterator<Item = (ScriptModelId, VehicleDestructibleKind, [f32; 3])>,
    ) {
        self.vehicle_bodies.clear();
        self.vehicle_origins.clear();
        self.vehicle_drains.clear();
        self.vehicle_loopfx.clear();
        self.vehicle_death_fx_emitted.clear();
        self.death_anim_times.clear();
        self.destructible_stages.clear();
        for (id, kind, origin) in vehicles {
            self.register_vehicle_at(id, kind, Some(origin));
        }
    }

    pub fn install_toy_destructibles(
        &mut self,
        toys: impl IntoIterator<Item = (ScriptModelId, ToyDestructibleKind, [f32; 3])>,
    ) {
        self.toy_bodies.clear();
        self.toy_origins.clear();
        self.toy_drains.clear();
        self.toy_fx_pulses.clear();
        self.toy_sound_pulses.clear();
        self.toy_part_launches.clear();
        self.destructible_loop_sounds.clear();
        for (id, kind, origin) in toys {
            self.register_toy_at(id, kind, Some(origin));
        }
    }

    pub fn install_explodable_barrels(
        &mut self,
        barrels: impl IntoIterator<Item = (ScriptModelId, [f32; 3])>,
    ) {
        crate::barrel_policy::approve_explodable_barrel_host_policy();
        self.barrel_bodies.clear();
        self.barrel_origins.clear();
        self.barrel_drains.clear();
        self.barrel_burn_start.clear();
        self.pending_barrel_downs.clear();
        for (id, origin) in barrels {
            self.register_barrel_at(id, origin);
        }
    }

    pub fn install_flammable_crates(
        &mut self,
        crates: impl IntoIterator<Item = FlammableCrateInstall>,
    ) {
        self.crate_bodies.clear();
        self.crate_origins.clear();
        self.crate_drains.clear();
        self.pending_crate_downs.clear();
        for ent in crates {
            upsert_origin(&mut self.crate_origins, ent.id, ent.origin);
            self.crate_bodies.push((
                ent.id,
                FlammableCrateBody {
                    health: gamemode_iw4::FLAMMABLE_CRATE_HEALTH,
                    burning: false,
                    destroyed: false,
                },
            ));
        }
        self.crate_bodies.sort_by_key(|(id, _)| *id);
    }

    pub fn install_destructables(
        &mut self,
        ents: impl IntoIterator<Item = DestructableInstall>,
        missing_tdm_spawns: bool,
    ) -> bool {
        self.destructable_bodies.clear();
        self.destructable_origins.clear();
        self.blocked_spawn_areas.clear();
        self.pending_destructable_downs.clear();
        let mut block_area_gap = false;
        for ent in ents {
            if !gamemode_iw4::init_keeps_ents("") {
                continue;
            }
            let areas: Vec<String> = gamemode_iw4::areas_from_script(&ent.script_destructable_area)
                .map(str::to_owned)
                .collect();
            if !areas.is_empty() {
                if missing_tdm_spawns {
                    block_area_gap = true;
                }
                for area in &areas {
                    self.block_area(area);
                }
            }
            upsert_origin(&mut self.destructable_origins, ent.id, ent.origin);
            self.destructable_bodies.push((
                ent.id,
                DestructableBody {
                    accumulate: gamemode_iw4::accumulate_of(ent.accumulate),
                    threshold: gamemode_iw4::threshold_of(ent.threshold),
                    dmg: 0,
                    destroyed: false,
                    areas,
                    has_fx: ent.has_fx,
                },
            ));
        }
        self.destructable_bodies.sort_by_key(|(id, _)| *id);
        block_area_gap
    }

    pub fn blocked_spawn_areas(&self) -> &[String] {
        &self.blocked_spawn_areas
    }

    pub fn take_destructable_downs(&mut self) -> Vec<DestructableDown> {
        core::mem::take(&mut self.pending_destructable_downs)
    }

    pub fn destroyed_destructable_ids(&self) -> Vec<ScriptModelId> {
        self.destructable_bodies
            .iter()
            .filter(|(_, body)| body.destroyed)
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn destructable_is_destroyed(&self, id: ScriptModelId) -> bool {
        self.destructable_bodies
            .iter()
            .find(|(have, _)| *have == id)
            .is_some_and(|(_, body)| body.destroyed)
    }

    fn block_area(&mut self, area: &str) {
        if self.blocked_spawn_areas.iter().any(|have| have == area) {
            return;
        }
        self.blocked_spawn_areas.push(area.to_owned());
        self.blocked_spawn_areas.sort();
    }

    fn unblock_area(&mut self, area: &str) {
        self.blocked_spawn_areas.retain(|have| have != area);
    }

    fn register_vehicle_at(
        &mut self,
        id: ScriptModelId,
        kind: VehicleDestructibleKind,
        origin: Option<[f32; 3]>,
    ) {
        if self
            .vehicle_bodies
            .iter()
            .any(|(existing, _, _)| *existing == id)
        {
            if let Some(origin) = origin {
                upsert_origin(&mut self.vehicle_origins, id, origin);
            }
            return;
        }
        self.vehicle_bodies.push((id, kind, kind.initial_body()));
        self.vehicle_bodies.sort_by_key(|(id, _, _)| *id);
        if let Some(origin) = origin {
            upsert_origin(&mut self.vehicle_origins, id, origin);
        }
    }

    fn register_toy_at(
        &mut self,
        id: ScriptModelId,
        kind: ToyDestructibleKind,
        origin: Option<[f32; 3]>,
    ) {
        if self
            .toy_bodies
            .iter()
            .any(|(existing, _, _)| *existing == id)
        {
            if let Some(origin) = origin {
                upsert_origin(&mut self.toy_origins, id, origin);
            }
            return;
        }
        self.toy_bodies.push((id, kind, kind.initial_body()));
        self.toy_bodies.sort_by_key(|(id, _, _)| *id);
        if let Some(origin) = origin {
            upsert_origin(&mut self.toy_origins, id, origin);
        }
    }

    fn register_barrel_at(&mut self, id: ScriptModelId, origin: [f32; 3]) {
        if self
            .barrel_bodies
            .iter()
            .any(|(existing, _)| *existing == id)
        {
            upsert_origin(&mut self.barrel_origins, id, origin);
            return;
        }
        self.barrel_bodies.push((
            id,
            VehicleBodyState {
                state_index: 0,
                health: crate::barrel_policy::EXPLODABLE_BARREL_HEALTH,
            },
        ));
        self.barrel_bodies.sort_by_key(|(id, _)| *id);
        upsert_origin(&mut self.barrel_origins, id, origin);
    }

    pub fn take_explodable_barrel_downs(&mut self) -> Vec<ScriptModelId> {
        core::mem::take(&mut self.pending_barrel_downs)
    }

    pub fn explodable_barrel_bodies(&self) -> &[(ScriptModelId, VehicleBodyState)] {
        &self.barrel_bodies
    }

    pub fn vehicle_bodies(&self) -> &[(ScriptModelId, VehicleDestructibleKind, VehicleBodyState)] {
        &self.vehicle_bodies
    }

    pub fn toy_bodies(&self) -> &[(ScriptModelId, ToyDestructibleKind, VehicleBodyState)] {
        &self.toy_bodies
    }

    pub fn vehicle_dump_rows(&self) -> Vec<VehicleDumpRow> {
        self.vehicle_bodies
            .iter()
            .map(|(id, kind, body)| {
                let def = kind.definition();
                let destroyed = kind.destroyed_state();
                let death =
                    vehicle_death_presentation_if_destroyed(def, body.state_index, destroyed);
                let death_fx = vehicle_death_fx_if_destroyed(def, body.state_index, destroyed);
                VehicleDumpRow {
                    id: *id,
                    kind: *kind,
                    body: *body,
                    healthdrain: self
                        .vehicle_drains
                        .iter()
                        .any(|(drain_id, _)| drain_id == id),
                    loop_fx: vehicle_active_loop_fx(def, body.state_index, destroyed),
                    death_fx: death_fx.map(|(fx, _)| fx),
                    death_sound: death_fx.map(|(_, sound)| sound),
                    death_clip: death.map(|d| d.clip),
                    husk: death.map(|d| d.husk),
                    death_anim_time: lookup_anim_time(&self.death_anim_times, *id),
                }
            })
            .collect()
    }

    pub fn apply_destructible_damage_batch(
        &mut self,
        intents: &[DestructibleDamageIntent],
    ) -> DestructibleApplyReport {
        let mut pending = intents.to_vec();
        let mut changed = 0;
        let mut explodes = Vec::new();
        let owner_cap = self.vehicle_bodies.len()
            + self.toy_bodies.len()
            + self.barrel_bodies.len()
            + self.crate_bodies.len()
            + self.destructable_bodies.len()
            + 1;
        for _ in 0..owner_cap {
            if pending.is_empty() {
                break;
            }
            let wave = self.apply_destructible_damage_wave(&pending);
            changed += wave.changed;
            let mut newly = Vec::new();
            for explode in wave.explodes {
                if explodes
                    .iter()
                    .any(|have: &DestructibleExplodeEvent| have.owner == explode.owner)
                {
                    continue;
                }
                newly.push(explode);
                explodes.push(explode);
            }
            pending = self.destructible_radius_intents(&newly);
        }
        DestructibleApplyReport { changed, explodes }
    }

    pub fn destructible_radius_intents(
        &self,
        explodes: &[DestructibleExplodeEvent],
    ) -> Vec<DestructibleDamageIntent> {
        let mut intents = Vec::new();
        let targets = self.destructible_splash_targets();
        for explode in explodes {
            let inner = explode.explode_damage.1 as f32;
            let outer = explode.explode_damage.0 as f32;
            let radius = explode.explode_range_mp as f32;
            for target in &targets {
                if target.id == explode.owner || target.terminal {
                    continue;
                }
                let dist = radius_damage_distance_to_aabb(explode.origin, target.origin, [0.0; 3]);
                let amount = g_radius_damage_amount(inner, outer, radius, dist, 1.0);
                if amount <= 0 {
                    continue;
                }
                intents.push(DestructibleDamageIntent {
                    source: DamageSource::Radius(explode.owner),
                    pellet: PelletId(0),
                    attacker: explode.attacker,
                    attacker_life: explode.attacker_life,
                    target: target.id,
                    amount: amount as u32,
                    splash: true,
                    epoch: EntityCollisionEpoch::CurrentTick,
                });
            }
        }
        intents.sort_by_key(|intent| {
            (
                intent.source,
                intent.target,
                intent.attacker.0,
                intent.attacker_life,
            )
        });
        intents
    }

    fn apply_destructible_damage_wave(
        &mut self,
        intents: &[DestructibleDamageIntent],
    ) -> DestructibleApplyReport {
        let mut ordered = intents.to_vec();
        ordered.sort_by_key(|intent| {
            (
                intent.source,
                intent.pellet,
                intent.target,
                intent.attacker.0,
                intent.attacker_life,
            )
        });

        let mut changed = 0;
        let mut explodes = Vec::new();
        for intent in ordered {
            if intent.amount == 0 {
                continue;
            }
            let mut applied = self.apply_vehicle_amount(
                intent.target,
                intent.amount,
                intent.attacker,
                intent.attacker_life,
                intent.source,
            );
            if applied.is_none() {
                applied = self.apply_toy_amount(
                    intent.target,
                    intent.amount,
                    intent.attacker,
                    intent.attacker_life,
                    intent.source,
                    intent.splash,
                );
            }
            if applied.is_none() {
                applied = self.apply_barrel_amount(
                    intent.target,
                    intent.amount,
                    intent.attacker,
                    intent.attacker_life,
                    intent.source,
                );
            }
            if applied.is_none() {
                applied = self.apply_crate_amount(
                    intent.target,
                    intent.amount,
                    intent.attacker,
                    intent.attacker_life,
                    intent.source,
                );
            }
            if applied.is_none() {
                applied = self.apply_destructable_amount(intent.target, intent.amount);
            }
            let Some(apply) = applied else {
                continue;
            };
            if apply.changed {
                changed += 1;
            }
            explodes.extend(apply.explodes);
        }
        DestructibleApplyReport { changed, explodes }
    }

    pub fn tick_vehicle_healthdrain(&mut self, dt_ms: u32) -> DestructibleApplyReport {
        self.tick_drains(DrainFamily::Vehicle, dt_ms)
    }

    pub fn tick_explodable_barrel_burn(&mut self, dt_ms: u32) -> DestructibleApplyReport {
        self.tick_drains(DrainFamily::Barrel, dt_ms)
    }

    pub fn tick_toy_healthdrain(&mut self, dt_ms: u32) -> DestructibleApplyReport {
        self.tick_drains(DrainFamily::Toy, dt_ms)
    }

    pub fn tick_flammable_crate_burn(&mut self, dt_ms: u32) -> DestructibleApplyReport {
        self.tick_drains(DrainFamily::Crate, dt_ms)
    }

    fn drain_table(
        &mut self,
        family: DrainFamily,
    ) -> &mut Vec<(ScriptModelId, VehicleHealthDrain)> {
        match family {
            DrainFamily::Vehicle => &mut self.vehicle_drains,
            DrainFamily::Barrel => &mut self.barrel_drains,
            DrainFamily::Toy => &mut self.toy_drains,
            DrainFamily::Crate => &mut self.crate_drains,
        }
    }

    /// Every armed drain of one family: the drain carries its own amount and
    /// interval, so one loop serves all of them.
    fn tick_drains(&mut self, family: DrainFamily, dt_ms: u32) -> DestructibleApplyReport {
        let ids: Vec<ScriptModelId> = self.drain_table(family).iter().map(|(id, _)| *id).collect();
        let mut changed = 0;
        let mut explodes = Vec::new();
        for id in ids {
            let table = self.drain_table(family);
            let Some(index) = table.iter().position(|(drain_id, _)| *drain_id == id) else {
                continue;
            };
            table[index].1.wait_ms = table[index].1.wait_ms.saturating_add(dt_ms);
            loop {
                let table = self.drain_table(family);
                let Some(index) = table.iter().position(|(drain_id, _)| *drain_id == id) else {
                    break;
                };
                let drain = table[index].1;
                if drain.interval_ms == 0 || drain.wait_ms < drain.interval_ms {
                    break;
                }
                table[index].1.wait_ms -= drain.interval_ms;
                let applied = match family {
                    DrainFamily::Vehicle => self.apply_vehicle_amount(
                        id,
                        drain.amount,
                        drain.attacker,
                        drain.attacker_life,
                        drain.source,
                    ),
                    DrainFamily::Barrel => self.apply_barrel_amount(
                        id,
                        drain.amount,
                        drain.attacker,
                        drain.attacker_life,
                        drain.source,
                    ),
                    // A health drain is never splash damage.
                    DrainFamily::Toy => self.apply_toy_amount(
                        id,
                        drain.amount,
                        drain.attacker,
                        drain.attacker_life,
                        drain.source,
                        false,
                    ),
                    DrainFamily::Crate => self.apply_crate_amount(
                        id,
                        drain.amount,
                        drain.attacker,
                        drain.attacker_life,
                        drain.source,
                    ),
                };
                let Some(apply) = applied else {
                    continue;
                };
                if apply.changed {
                    changed += 1;
                }
                let exploded = !apply.explodes.is_empty();
                explodes.extend(apply.explodes);
                if exploded {
                    break;
                }
            }
        }
        DestructibleApplyReport { changed, explodes }
    }

    pub fn take_new_death_fx_pulses(&mut self) -> (Vec<VehicleFxPulse>, Vec<VehicleSoundPulse>) {
        let mut fx = Vec::new();
        let mut sounds = Vec::new();
        let pending: Vec<(
            ScriptModelId,
            &'static str,
            &'static str,
            [f32; 3],
            Option<&'static str>,
        )> = self
            .vehicle_bodies
            .iter()
            .filter_map(|(id, kind, body)| {
                if body.state_index < kind.destroyed_state() {
                    return None;
                }
                if self.vehicle_death_fx_emitted.iter().any(|have| have == id) {
                    return None;
                }
                let origin = lookup_origin(&self.vehicle_origins, *id)?;
                let def = kind.definition();
                Some((
                    *id,
                    def.death_fx,
                    def.death_sound,
                    origin,
                    Some(def.death_fx_tag),
                ))
            })
            .chain(self.barrel_bodies.iter().filter_map(|(id, body)| {
                if body.state_index < crate::barrel_policy::EXPLODABLE_BARREL_DESTROYED_STATE {
                    return None;
                }
                if self.vehicle_death_fx_emitted.iter().any(|have| have == id) {
                    return None;
                }
                let origin = lookup_origin(&self.barrel_origins, *id)?;
                Some((
                    *id,
                    crate::barrel_policy::EXPLODABLE_BARREL_DEATH_FX,
                    crate::barrel_policy::EXPLODABLE_BARREL_DEATH_SOUND,
                    origin,
                    None,
                ))
            }))
            .collect();
        for (id, def_name, alias, origin, tag) in pending {
            self.vehicle_death_fx_emitted.push(id);
            fx.push(VehicleFxPulse {
                owner: id,
                origin,
                def_name,
                tag,
                use_tag_angles: true,
            });
            if !alias.is_empty() {
                sounds.push(VehicleSoundPulse {
                    owner: id,
                    origin,
                    alias,
                    tag,
                });
            }
        }
        (fx, sounds)
    }

    /// Effects and sounds queued by state changes: the barrel burn start and
    /// every destructible action state a toy has left this tick.
    pub fn take_new_stage_pulses(&mut self) -> (Vec<VehicleFxPulse>, Vec<VehicleSoundPulse>) {
        let mut fx = std::mem::take(&mut self.barrel_burn_start);
        fx.append(&mut self.toy_fx_pulses);
        (fx, std::mem::take(&mut self.toy_sound_pulses))
    }

    pub fn take_toy_part_launches(&mut self) -> Vec<ScriptModelId> {
        core::mem::take(&mut self.toy_part_launches)
    }

    /// Every loop a destructible is speaking right now, by alias, for the step
    /// to intern and publish.
    pub fn speaking_loop_sounds(&self) -> Vec<(ScriptModelId, &'static str, [f32; 3])> {
        self.toy_bodies
            .iter()
            .flat_map(|(id, kind, body)| {
                let origin = lookup_origin(&self.toy_origins, *id);
                kind.definition()
                    .active_loop_sounds(body.state_index)
                    .iter()
                    .filter_map(move |alias| Some((*id, *alias, origin?)))
            })
            .collect()
    }

    pub fn set_destructible_loop_sounds(&mut self, rows: Vec<DestructibleLoopSound>) {
        self.destructible_loop_sounds = rows;
    }

    pub fn tick_vehicle_loopfx(&mut self, dt_ms: u32) -> Vec<VehicleFxPulse> {
        let desired: Vec<(VehicleFxPulse, u32)> = self
            .vehicle_bodies
            .iter()
            .filter_map(|(id, kind, body)| {
                let def = kind.definition();
                let fx = vehicle_active_loop_fx(def, body.state_index, kind.destroyed_state())?;
                let origin = lookup_origin(&self.vehicle_origins, *id)?;
                Some((
                    VehicleFxPulse {
                        owner: *id,
                        origin,
                        def_name: fx,
                        tag: None,
                        use_tag_angles: true,
                    },
                    VEHICLE_LOOPFX_INTERVAL_MS,
                ))
            })
            .chain(self.barrel_bodies.iter().filter_map(|(id, body)| {
                if body.state_index >= crate::barrel_policy::EXPLODABLE_BARREL_DESTROYED_STATE {
                    return None;
                }
                if !self.barrel_drains.iter().any(|(have, _)| have == id) {
                    return None;
                }
                let origin = lookup_origin(&self.barrel_origins, *id)?;
                Some((
                    VehicleFxPulse {
                        owner: *id,
                        origin,
                        def_name: crate::barrel_policy::EXPLODABLE_BARREL_BURN_LOOP_FX,
                        tag: None,
                        use_tag_angles: true,
                    },
                    crate::barrel_policy::EXPLODABLE_BARREL_BURN_LOOP_INTERVAL_MS,
                ))
            }))
            .chain(self.toy_bodies.iter().flat_map(|(id, kind, body)| {
                let origin = lookup_origin(&self.toy_origins, *id);
                kind.definition()
                    .active_loop_fx(body.state_index)
                    .iter()
                    .filter_map(move |fx| {
                        Some((
                            VehicleFxPulse {
                                owner: *id,
                                origin: origin?,
                                def_name: fx.name,
                                tag: Some(fx.tag),
                                use_tag_angles: true,
                            },
                            fx.interval_ms,
                        ))
                    })
            }))
            .collect();
        self.vehicle_loopfx.retain(|(id, name, _)| {
            desired
                .iter()
                .any(|(pulse, _)| pulse.owner == *id && pulse.def_name == *name)
        });
        for (pulse, _) in &desired {
            if !self
                .vehicle_loopfx
                .iter()
                .any(|(id, name, _)| *id == pulse.owner && *name == pulse.def_name)
            {
                self.vehicle_loopfx.push((pulse.owner, pulse.def_name, 0));
            }
        }
        let mut pulses = Vec::new();
        for (pulse, interval) in desired {
            let Some((_, _, wait)) = self
                .vehicle_loopfx
                .iter_mut()
                .find(|(id, name, _)| *id == pulse.owner && *name == pulse.def_name)
            else {
                continue;
            };
            if *wait > dt_ms {
                *wait -= dt_ms;
                continue;
            }
            *wait = interval;
            pulses.push(pulse);
        }
        pulses
    }

    pub fn note_death_anim_started(&mut self, id: ScriptModelId) {
        if self
            .death_anim_times
            .iter()
            .any(|(existing, _)| *existing == id)
        {
            return;
        }
        self.death_anim_times.push((id, 0.0));
        self.death_anim_times.sort_by_key(|(id, _)| *id);
    }

    pub fn set_death_anim_time(&mut self, id: ScriptModelId, time: f32) {
        upsert_anim_time(&mut self.death_anim_times, id, time);
    }

    fn apply_vehicle_amount(
        &mut self,
        target: ScriptModelId,
        amount: u32,
        attacker: ClientId,
        attacker_life: LifeSequence,
        source: DamageSource,
    ) -> Option<DestructibleApply> {
        let Some(index) = self
            .vehicle_bodies
            .iter()
            .position(|(id, _, _)| *id == target)
        else {
            return None;
        };
        let kind = self.vehicle_bodies[index].1;
        let def = kind.definition();
        let destroyed = kind.destroyed_state();
        let body = self.vehicle_bodies[index].2;
        let was_destroyed = body.state_index >= destroyed;
        let next = apply_vehicle_player_bullet(def, destroyed, body, amount);
        if next == body {
            return None;
        }
        self.vehicle_bodies[index].2 = next;
        self.set_destructible_state(target, DestructibleStateIndex::new(next.state_index));
        if vehicle_healthdrain_arms(body.state_index, next.state_index, destroyed) {
            if !self.vehicle_drains.iter().any(|(id, _)| *id == target) {
                self.vehicle_drains.push((
                    target,
                    VehicleHealthDrain {
                        wait_ms: 0,
                        amount: VEHICLE_HEALTHDRAIN_AMOUNT,
                        interval_ms: VEHICLE_HEALTHDRAIN_INTERVAL_MS,
                        attacker,
                        attacker_life,
                        source,
                    },
                ));
                self.vehicle_drains.sort_by_key(|(id, _)| *id);
            }
        }
        if next.state_index >= destroyed {
            remove_drain(&mut self.vehicle_drains, target);
        }
        if !was_destroyed
            && next.state_index >= destroyed
            && let Some(origin) = lookup_origin(&self.vehicle_origins, target)
        {
            return Some(DestructibleApply::exploded(DestructibleExplodeEvent {
                owner: target,
                origin,
                attacker,
                attacker_life,
                source: DamageSource::Radius(target),
                explode_range_mp: def.explode_range_mp,
                explode_damage: def.explode_damage,
            }));
        }
        Some(DestructibleApply::changed())
    }

    fn apply_toy_amount(
        &mut self,
        target: ScriptModelId,
        amount: u32,
        attacker: ClientId,
        attacker_life: LifeSequence,
        source: DamageSource,
        splash: bool,
    ) -> Option<DestructibleApply> {
        let index = self
            .toy_bodies
            .iter()
            .position(|(id, _, _)| *id == target)?;
        let kind = self.toy_bodies[index].1;
        let def = kind.definition();
        let destroyed = def.destroyed_state();
        let body = self.toy_bodies[index].2;
        let amount = if splash {
            (amount as f32 * def.splash_damage_scaler()) as u32
        } else {
            amount
        };
        let next = apply_toy_damage(def, body, amount, splash);
        if next == body {
            return None;
        }
        self.toy_bodies[index].2 = next;
        self.set_destructible_state(target, DestructibleStateIndex::new(next.state_index));
        let origin = lookup_origin(&self.toy_origins, target)?;

        let mut explodes = Vec::new();
        for entered in (body.state_index + 1)..=next.state_index {
            // The action list belongs to the state being left, not the one entered.
            let Some(left) = def.left_stage(entered) else {
                continue;
            };
            for fx in left.fx {
                if !fx.cause.accepts(splash) {
                    continue;
                }
                self.toy_fx_pulses.push(VehicleFxPulse {
                    owner: target,
                    origin,
                    def_name: fx.name,
                    tag: Some(fx.tag),
                    use_tag_angles: fx.use_tag_angles,
                });
            }
            for alias in left.sounds {
                self.toy_sound_pulses.push(VehicleSoundPulse {
                    owner: target,
                    origin,
                    alias,
                    tag: None,
                });
            }
            if !left.throws.is_empty() {
                self.toy_part_launches.push(target);
            }
            if let Some(drain) = left.health_drain {
                // Only a state that declares a drain restarts one; a state
                // that declares none leaves a running drain alone.
                upsert_value(
                    &mut self.toy_drains,
                    target,
                    VehicleHealthDrain {
                        wait_ms: 0,
                        amount: drain.amount,
                        interval_ms: drain.interval_ms,
                        attacker,
                        attacker_life,
                        source,
                    },
                );
            }
            if let Some(explode) = left.explode {
                let mut at = origin;
                at[2] += explode.origin_offset_z;
                explodes.push(DestructibleExplodeEvent {
                    owner: target,
                    origin: at,
                    attacker,
                    attacker_life,
                    source: DamageSource::Radius(target),
                    explode_range_mp: explode.range_mp,
                    explode_damage: explode.damage,
                });
            }
        }
        if next.state_index >= destroyed {
            remove_drain(&mut self.toy_drains, target);
        }
        Some(DestructibleApply {
            changed: true,
            explodes,
        })
    }

    fn apply_barrel_amount(
        &mut self,
        target: ScriptModelId,
        amount: u32,
        attacker: ClientId,
        attacker_life: LifeSequence,
        source: DamageSource,
    ) -> Option<DestructibleApply> {
        let Some(index) = self.barrel_bodies.iter().position(|(id, _)| *id == target) else {
            return None;
        };
        let destroyed = crate::barrel_policy::EXPLODABLE_BARREL_DESTROYED_STATE;
        let body = self.barrel_bodies[index].1;
        let was_destroyed = body.state_index >= destroyed;
        let next = apply_destructible_part_player_bullet(
            &crate::barrel_policy::EXPLODABLE_BARREL_HEALTH_TABLE,
            destroyed,
            body,
            amount,
        );
        if next == body {
            return None;
        }
        self.barrel_bodies[index].1 = next;
        self.set_destructible_state(target, DestructibleStateIndex::new(next.state_index));
        if next.state_index >= destroyed {
            remove_drain(&mut self.barrel_drains, target);
        } else if !self.barrel_drains.iter().any(|(id, _)| *id == target) {
            self.barrel_drains.push((
                target,
                VehicleHealthDrain {
                    wait_ms: 0,
                    amount: crate::barrel_policy::EXPLODABLE_BARREL_BURN_DRAIN,
                    interval_ms: crate::barrel_policy::EXPLODABLE_BARREL_BURN_DRAIN_INTERVAL_MS,
                    attacker,
                    attacker_life,
                    source,
                },
            ));
            self.barrel_drains.sort_by_key(|(id, _)| *id);
            if let Some(origin) = lookup_origin(&self.barrel_origins, target) {
                self.barrel_burn_start.push(VehicleFxPulse {
                    owner: target,
                    origin,
                    def_name: crate::barrel_policy::EXPLODABLE_BARREL_BURN_START_FX,
                    tag: None,
                    use_tag_angles: true,
                });
            }
        }
        if !was_destroyed && next.state_index >= destroyed {
            if !self.pending_barrel_downs.iter().any(|have| *have == target) {
                self.pending_barrel_downs.push(target);
            }
            if let Some(origin) = lookup_origin(&self.barrel_origins, target) {
                return Some(DestructibleApply::exploded(DestructibleExplodeEvent {
                    owner: target,
                    origin,
                    attacker,
                    attacker_life,
                    source: DamageSource::Radius(target),
                    explode_range_mp: crate::barrel_policy::EXPLODABLE_BARREL_EXPLODE_RANGE,
                    explode_damage: crate::barrel_policy::EXPLODABLE_BARREL_EXPLODE_DAMAGE,
                }));
            }
        }
        Some(DestructibleApply::changed())
    }

    fn apply_crate_amount(
        &mut self,
        target: ScriptModelId,
        amount: u32,
        attacker: ClientId,
        attacker_life: LifeSequence,
        source: DamageSource,
    ) -> Option<DestructibleApply> {
        let Some(index) = self.crate_bodies.iter().position(|(id, _)| *id == target) else {
            return None;
        };
        if self.crate_bodies[index].1.destroyed {
            return None;
        }
        let attacker_is_player = !matches!(source, DamageSource::Radius(_));
        if !gamemode_iw4::flammable_crate_damage_applies(false, attacker_is_player) {
            return None;
        }
        let amount = i32::try_from(amount).unwrap_or(i32::MAX);
        let next =
            gamemode_iw4::flammable_crate_health_after(self.crate_bodies[index].1.health, amount);
        if next == self.crate_bodies[index].1.health {
            return None;
        }
        self.crate_bodies[index].1.health = next;
        if gamemode_iw4::flammable_crate_should_ignite(next)
            && !self.crate_bodies[index].1.burning
            && !self.crate_drains.iter().any(|(id, _)| *id == target)
        {
            self.crate_bodies[index].1.burning = true;
            self.crate_drains.push((
                target,
                VehicleHealthDrain {
                    wait_ms: 0,
                    amount: u32::try_from(gamemode_iw4::FLAMMABLE_CRATE_BURN_DRAIN).unwrap_or(0),
                    interval_ms: gamemode_iw4::FLAMMABLE_CRATE_BURN_DRAIN_INTERVAL_MS,
                    attacker,
                    attacker_life,
                    source,
                },
            ));
            self.crate_drains.sort_by_key(|(id, _)| *id);
        }
        if !gamemode_iw4::flammable_crate_should_explode(next) {
            return Some(DestructibleApply::changed());
        }
        self.crate_bodies[index].1.destroyed = true;
        self.crate_bodies[index].1.burning = false;
        remove_drain(&mut self.crate_drains, target);
        self.set_destructible_state(target, DestructibleStateIndex::new(1));
        if !self.pending_crate_downs.iter().any(|have| *have == target) {
            self.pending_crate_downs.push(target);
        }
        let Some(origin) = lookup_origin(&self.crate_origins, target) else {
            return Some(DestructibleApply::changed());
        };
        let mut explode_at = origin;
        explode_at[2] += gamemode_iw4::FLAMMABLE_CRATE_EXPLODE_ORIGIN_Z;
        Some(DestructibleApply::exploded(DestructibleExplodeEvent {
            owner: target,
            origin: explode_at,
            attacker,
            attacker_life,
            source: DamageSource::Radius(target),
            explode_range_mp: gamemode_iw4::FLAMMABLE_CRATE_EXPLODE_RANGE,
            explode_damage: gamemode_iw4::FLAMMABLE_CRATE_EXPLODE_DAMAGE,
        }))
    }

    pub fn take_flammable_crate_downs(&mut self) -> Vec<ScriptModelId> {
        core::mem::take(&mut self.pending_crate_downs)
    }

    pub fn destroyed_flammable_crate_ids(&self) -> Vec<ScriptModelId> {
        self.crate_bodies
            .iter()
            .filter(|(_, body)| body.destroyed)
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn flammable_crate_is_destroyed(&self, id: ScriptModelId) -> bool {
        self.crate_bodies
            .iter()
            .find(|(have, _)| *have == id)
            .is_some_and(|(_, body)| body.destroyed)
    }

    pub fn flammable_crate_is_burning(&self, id: ScriptModelId) -> bool {
        self.crate_bodies
            .iter()
            .find(|(have, _)| *have == id)
            .is_some_and(|(_, body)| body.burning)
    }

    fn apply_destructable_amount(
        &mut self,
        target: ScriptModelId,
        amount: u32,
    ) -> Option<DestructibleApply> {
        let Some(index) = self
            .destructable_bodies
            .iter()
            .position(|(id, _)| *id == target)
        else {
            return None;
        };
        if self.destructable_bodies[index].1.destroyed {
            return None;
        }
        let amount = i32::try_from(amount).unwrap_or(i32::MAX);
        let threshold = self.destructable_bodies[index].1.threshold;
        if !gamemode_iw4::damage_applies(amount, threshold) {
            return None;
        }
        let next_dmg = self.destructable_bodies[index].1.dmg.saturating_add(amount);
        self.destructable_bodies[index].1.dmg = next_dmg;
        if !gamemode_iw4::should_destruct(next_dmg, self.destructable_bodies[index].1.accumulate) {
            return Some(DestructibleApply::changed());
        }
        self.destructable_bodies[index].1.destroyed = true;
        self.set_destructible_state(target, DestructibleStateIndex::new(1));
        let areas = self.destructable_bodies[index].1.areas.clone();
        for area in &areas {
            self.unblock_area(area);
        }
        let play_fx = self.destructable_bodies[index].1.has_fx;
        if !self
            .pending_destructable_downs
            .iter()
            .any(|down| down.id == target)
        {
            self.pending_destructable_downs.push(DestructableDown {
                id: target,
                play_fx,
            });
        }
        Some(DestructibleApply::changed())
    }

    pub fn glass_piece_state(&self, id: GlassPieceId) -> GlassPieceState {
        self.glass_piece(id).state()
    }

    pub fn glass_damage(&self, id: GlassPieceId) -> u16 {
        self.glass_piece(id).damage
    }

    pub fn glass_is_solid(&self, id: GlassPieceId) -> bool {
        glass_is_solid(self.glass_piece(id).damage)
    }

    pub fn glass_damage_pairs(&self) -> Vec<(GlassPieceId, u16)> {
        self.glass_pieces
            .iter()
            .map(|(id, piece)| (*id, piece.damage))
            .collect()
    }

    pub fn glass_radius_targets(&self) -> Vec<(GlassPieceId, GlassPaneBasis)> {
        self.glass_panes
            .iter()
            .copied()
            .filter(|(id, _)| self.glass_is_solid(*id))
            .collect()
    }

    pub fn apply_glass_damage(
        &mut self,
        id: GlassPieceId,
        damage: u32,
        at_time_ms: i32,
        weakened_collapse_time_cs: Option<u16>,
        shatter_seed: Option<GlassShatterSeed>,
    ) -> GlassPieceState {
        self.apply_glass_damage_caused(
            id,
            damage,
            at_time_ms,
            weakened_collapse_time_cs,
            shatter_seed,
            GlassCause::Impact,
        )
    }

    pub fn apply_glass_damage_caused(
        &mut self,
        id: GlassPieceId,
        damage: u32,
        at_time_ms: i32,
        weakened_collapse_time_cs: Option<u16>,
        shatter_seed: Option<GlassShatterSeed>,
        cause: GlassCause,
    ) -> GlassPieceState {
        if damage == 0 {
            return self.glass_piece_state(id);
        }
        let mut piece = self.glass_piece(id);
        if piece.damage == GLASS_DAMAGE_INVALID {
            return GlassPieceState::Deleted;
        }
        if let Some(change) = glass_apply_damage(
            &mut piece,
            damage,
            at_time_ms,
            weakened_collapse_time_cs,
            shatter_seed,
        ) {
            self.note_glass_destroyed(id, change);
            let mut meta = self.glass_native_meta(id);
            meta.revision = meta.revision.saturating_add(1);
            if change.current == GlassPieceState::Shattered {
                meta.cause = cause;
                meta.deterministic_seed = GlassBreakRecord::mix_seed(id, at_time_ms, shatter_seed);
            } else if change.current == GlassPieceState::Weakened {
                meta.cause = cause;
            }
            upsert_value(&mut self.glass_native, id, meta);
        }
        if piece.damage == 0 {
            remove_key(&mut self.glass_pieces, id);
            remove_key(&mut self.glass_native, id);
        } else {
            upsert_value(&mut self.glass_pieces, id, piece);
        }
        self.glass_piece_state(id)
    }

    pub fn script_destroy_glass(&mut self, id: GlassPieceId, at_time_ms: i32) -> GlassPieceState {
        let (hit, dir) = self
            .glass_pane(id)
            .map(|pane| (pane.origin, [0.0, 0.0, 1.0]))
            .unwrap_or(([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
        self.force_shatter_glass(id, at_time_ms, hit, dir, GlassCause::Script)
    }

    pub fn force_shatter_glass(
        &mut self,
        id: GlassPieceId,
        at_time_ms: i32,
        hit: [f32; 3],
        dir: [f32; 3],
        cause: GlassCause,
    ) -> GlassPieceState {
        if !self.glass_is_solid(id) {
            return self.glass_piece_state(id);
        }
        let seed = self
            .glass_pane(id)
            .and_then(|pane| glass_shatter_seed_from_hit(pane, hit, dir));
        self.apply_glass_damage_caused(
            id,
            u32::from(GLASS_DAMAGE_TO_DESTROY),
            at_time_ms,
            None,
            seed,
            cause,
        )
    }

    pub fn apply_glass_blast(
        &mut self,
        origin: [f32; 3],
        inner_damage: i32,
        outer_damage: i32,
        radius: f32,
        at_time_ms: i32,
        next_random: &mut impl FnMut() -> f32,
    ) {
        self.apply_glass_blast_oriented(
            origin,
            inner_damage,
            outer_damage,
            radius,
            [0.0; 3],
            0.0,
            at_time_ms,
            next_random,
        );
    }

    pub fn apply_glass_blast_oriented(
        &mut self,
        origin: [f32; 3],
        inner_damage: i32,
        outer_damage: i32,
        radius: f32,
        cone_dir: [f32; 3],
        cone_cos: f32,
        at_time_ms: i32,
        next_random: &mut impl FnMut() -> f32,
    ) {
        if inner_damage <= 0 && outer_damage <= 0 {
            return;
        }
        let r = if radius > entity_iw4::GLASS_BLAST_RADIUS_CAP {
            entity_iw4::GLASS_BLAST_RADIUS_CAP
        } else {
            radius
        };
        if !(r > 0.0) {
            return;
        }
        let panes: Vec<(GlassPieceId, GlassPaneBasis)> = self.glass_panes.clone();
        for (id, pane) in panes {
            if !self.glass_is_solid(id) {
                continue;
            }
            let dx = pane.origin[0] - origin[0];
            let dy = pane.origin[1] - origin[1];
            let dz = pane.origin[2] - origin[2];
            let d = (dx * dx + dy * dy + dz * dz).sqrt();
            if !glass_blast_cone_keeps(cone_dir, cone_cos, [dx, dy, dz]) {
                continue;
            }
            let amount = glass_blast_integer_damage(inner_damage, outer_damage, r, d);
            if amount == 0 {
                continue;
            }
            let dir = if d > 1.0e-4 {
                [dx / d, dy / d, dz / d]
            } else {
                [0.0, 0.0, 1.0]
            };
            let _ = self.apply_glass_hit_caused(
                id,
                amount,
                at_time_ms,
                origin,
                dir,
                next_random,
                GlassCause::Blast,
            );
        }
    }

    pub fn install_glass_panes(&mut self, panes: Vec<(GlassPieceId, GlassPaneBasis)>) {
        self.glass_panes = panes;
        sort_pairs(&mut self.glass_panes);
    }

    pub fn set_map_round_epoch(&mut self, epoch: u32) {
        self.map_round_epoch = epoch;
    }

    pub fn map_round_epoch(&self) -> u32 {
        self.map_round_epoch
    }

    pub fn glass_pane(&self, id: GlassPieceId) -> Option<GlassPaneBasis> {
        lookup_value(&self.glass_panes, id)
    }

    pub fn clone_glass_panes_from(&mut self, other: &Self) {
        self.glass_panes = other.glass_panes.clone();
    }

    pub fn apply_glass_hit(
        &mut self,
        id: GlassPieceId,
        damage: u32,
        at_time_ms: i32,
        hit: [f32; 3],
        dir: [f32; 3],
        next_random: &mut impl FnMut() -> f32,
    ) -> GlassPieceState {
        self.apply_glass_hit_caused(
            id,
            damage,
            at_time_ms,
            hit,
            dir,
            next_random,
            GlassCause::Impact,
        )
    }

    pub fn apply_glass_hit_caused(
        &mut self,
        id: GlassPieceId,
        damage: u32,
        at_time_ms: i32,
        hit: [f32; 3],
        dir: [f32; 3],
        next_random: &mut impl FnMut() -> f32,
        cause: GlassCause,
    ) -> GlassPieceState {
        if damage == 0 {
            return self.glass_piece_state(id);
        }
        let previous = self.glass_piece_state(id);
        if previous == GlassPieceState::Deleted {
            return previous;
        }
        let next =
            glass_state_from_damage(entity_iw4::glass_add_damage(self.glass_damage(id), damage));
        let collapse = (next != previous && next == GlassPieceState::Weakened)
            .then(|| glass_weakened_collapse_time_cs(next_random));
        let seed = (next != previous && next == GlassPieceState::Shattered)
            .then(|| {
                self.glass_pane(id)
                    .and_then(|pane| glass_shatter_seed_from_hit(pane, hit, dir))
            })
            .flatten();
        self.apply_glass_damage_caused(id, damage, at_time_ms, collapse, seed, cause)
    }

    pub fn glass_update(&mut self, at_time_ms: i32) {
        let ids: Vec<GlassPieceId> = self.glass_pieces.iter().map(|(id, _)| *id).collect();
        for id in ids {
            let mut piece = self.glass_piece(id);
            if let Some(change) = glass_collapse_piece(&mut piece, at_time_ms) {
                self.note_glass_destroyed(id, change);
                upsert_value(&mut self.glass_pieces, id, piece);
                if change.current == GlassPieceState::Shattered {
                    let mut meta = self.glass_native_meta(id);
                    meta.revision = meta.revision.saturating_add(1);
                    meta.cause = GlassCause::Collapse;
                    meta.deterministic_seed = GlassBreakRecord::mix_seed(id, at_time_ms, None);
                    upsert_value(&mut self.glass_native, id, meta);
                }
            }
        }
    }

    pub fn take_glass_destroyed(&mut self) -> Vec<GlassPieceId> {
        core::mem::take(&mut self.pending_glass_destroyed)
    }

    fn note_glass_destroyed(&mut self, id: GlassPieceId, change: GlassStateChange) {
        if glass_should_notify_destroyed(change) {
            self.pending_glass_destroyed.push(id);
        }
    }

    pub fn to_snapshot(&self) -> WorldObjectSnapshot {
        let mut destructible_stages = self.destructible_stages.clone();
        sort_pairs(&mut destructible_stages);

        let mut glass_pieces = self
            .glass_pieces
            .iter()
            .filter_map(|(id, piece)| {
                let state = piece.state();
                (state != GlassPieceState::Intact).then_some((
                    *id,
                    GlassPieceSnapshot {
                        state,
                        revision: self.glass_native_meta(*id).revision.max(1),
                        last_state_change_time: piece.last_state_change_time,
                        shatter_seed: piece.shatter_seed(),
                        deterministic_seed: self.glass_native_meta(*id).deterministic_seed,
                        cause: self.glass_native_meta(*id).cause,
                    },
                ))
            })
            .collect::<Vec<_>>();
        sort_pairs(&mut glass_pieces);

        WorldObjectSnapshot {
            as_of_ms: 0,
            map_round_epoch: self.map_round_epoch,
            fracture_profile_version: GLASS_FRACTURE_PROFILE_VERSION,
            destructible_stages,
            glass_pieces,
            destructible_loop_sounds: self.destructible_loop_sounds.clone(),
        }
    }

    pub fn adopt_snapshot(&mut self, snap: &WorldObjectSnapshot) {
        self.map_round_epoch = snap.map_round_epoch;
        self.destructible_loop_sounds = snap.destructible_loop_sounds.clone();
        self.destructible_stages = snap.destructible_stages.clone();
        sort_pairs(&mut self.destructible_stages);

        // Presentation reads the per-family bodies, so a client that only
        // receives stages has to see them there too.
        for (id, _, body) in &mut self.vehicle_bodies {
            body.state_index = lookup_value(&self.destructible_stages, *id).unwrap_or(0);
        }
        for (id, kind, body) in &mut self.toy_bodies {
            let state = lookup_value(&self.destructible_stages, *id).unwrap_or(0);
            *body = VehicleBodyState {
                state_index: state,
                health: kind.definition().stage(state).health,
            };
        }
        for (id, body) in &mut self.barrel_bodies {
            body.state_index = lookup_value(&self.destructible_stages, *id).unwrap_or(0);
        }

        self.glass_pieces.clear();
        self.glass_native.clear();
        for (id, snapshot) in &snap.glass_pieces {
            let damage = glass_damage_for_state(snapshot.state);
            if damage != 0 {
                let mut piece = GGlassPiece::default();
                piece.damage = damage;
                piece.last_state_change_time = snapshot.last_state_change_time;
                if let Some(seed) = snapshot.shatter_seed {
                    piece.impact_dir = seed.impact_dir;
                    piece.impact_pos = seed.impact_pos;
                } else if snapshot.state == GlassPieceState::Shattered {
                    piece.impact_dir = entity_iw4::GLASS_IMPACT_DIR_NONE;
                }
                upsert_value(&mut self.glass_pieces, *id, piece);
                upsert_value(
                    &mut self.glass_native,
                    *id,
                    GlassNativeMeta {
                        revision: snapshot.revision,
                        cause: snapshot.cause,
                        deterministic_seed: snapshot.deterministic_seed,
                    },
                );
            }
        }
    }

    fn glass_piece(&self, id: GlassPieceId) -> GGlassPiece {
        match lookup_value(&self.glass_pieces, id) {
            Some(piece) => piece,
            None => GGlassPiece::default(),
        }
    }

    fn glass_native_meta(&self, id: GlassPieceId) -> GlassNativeMeta {
        lookup_value(&self.glass_native, id).unwrap_or_default()
    }

    pub fn glass_break_record(&self, id: GlassPieceId) -> Option<GlassBreakRecord> {
        let piece = self.glass_piece(id);
        if piece.state() != GlassPieceState::Shattered {
            return None;
        }
        let meta = self.glass_native_meta(id);
        Some(GlassBreakRecord {
            break_tick: piece.last_state_change_time,
            deterministic_seed: meta.deterministic_seed,
            shatter_seed: piece.shatter_seed(),
            cause: meta.cause,
        })
    }

    pub fn glass_revision(&self, id: GlassPieceId) -> u32 {
        self.glass_native_meta(id).revision
    }
}

pub(crate) fn glass_piece_is_solid(damage: u16) -> bool {
    glass_is_solid(damage)
}

fn glass_damage_for_state(state: GlassPieceState) -> u16 {
    match state {
        GlassPieceState::Intact => 0,
        GlassPieceState::Weakened => GLASS_DAMAGE_TO_WEAKEN,
        GlassPieceState::Shattered => GLASS_DAMAGE_TO_DESTROY,
        GlassPieceState::Deleted => GLASS_DAMAGE_INVALID,
    }
}

fn lookup_value<K: Copy + PartialEq, V: Copy>(table: &[(K, V)], id: K) -> Option<V> {
    table.iter().find(|(key, _)| *key == id).map(|(_, v)| *v)
}

fn lookup_origin(table: &[(ScriptModelId, [f32; 3])], id: ScriptModelId) -> Option<[f32; 3]> {
    table
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, origin)| *origin)
}

fn upsert_origin(table: &mut Vec<(ScriptModelId, [f32; 3])>, id: ScriptModelId, origin: [f32; 3]) {
    if let Some(row) = table.iter_mut().find(|(key, _)| *key == id) {
        row.1 = origin;
        return;
    }
    table.push((id, origin));
    table.sort_by_key(|(id, _)| *id);
}

fn upsert_value<K: Copy + Ord, V: Copy>(table: &mut Vec<(K, V)>, id: K, value: V) {
    if let Some(row) = table.iter_mut().find(|(key, _)| *key == id) {
        row.1 = value;
        return;
    }
    table.push((id, value));
    sort_pairs(table);
}

fn remove_key<K: Copy + PartialEq, V>(table: &mut Vec<(K, V)>, id: K) {
    table.retain(|(key, _)| *key != id);
}

fn remove_drain(table: &mut Vec<(ScriptModelId, VehicleHealthDrain)>, id: ScriptModelId) {
    table.retain(|(key, _)| *key != id);
}

fn lookup_anim_time(table: &[(ScriptModelId, f32)], id: ScriptModelId) -> Option<f32> {
    table.iter().find(|(key, _)| *key == id).map(|(_, t)| *t)
}

fn upsert_anim_time(table: &mut Vec<(ScriptModelId, f32)>, id: ScriptModelId, time: f32) {
    if let Some(row) = table.iter_mut().find(|(key, _)| *key == id) {
        row.1 = time;
        return;
    }
    table.push((id, time));
    table.sort_by_key(|(id, _)| *id);
}

fn sort_pairs<K: Copy + Ord, V>(table: &mut [(K, V)]) {
    table.sort_by(|(a, _), (b, _)| a.cmp(b));
}

struct DestructibleSplashTarget {
    id: ScriptModelId,
    origin: [f32; 3],
    terminal: bool,
}

impl WorldObjectState {
    fn destructible_splash_targets(&self) -> Vec<DestructibleSplashTarget> {
        let mut targets = Vec::new();
        for (id, kind, body) in &self.vehicle_bodies {
            let Some(origin) = lookup_origin(&self.vehicle_origins, *id) else {
                continue;
            };
            targets.push(DestructibleSplashTarget {
                id: *id,
                origin,
                terminal: body.state_index >= kind.destroyed_state(),
            });
        }
        for (id, kind, body) in &self.toy_bodies {
            let Some(origin) = lookup_origin(&self.toy_origins, *id) else {
                continue;
            };
            targets.push(DestructibleSplashTarget {
                id: *id,
                origin,
                terminal: body.state_index >= kind.destroyed_state(),
            });
        }
        for (id, body) in &self.barrel_bodies {
            let Some(origin) = lookup_origin(&self.barrel_origins, *id) else {
                continue;
            };
            targets.push(DestructibleSplashTarget {
                id: *id,
                origin,
                terminal: body.state_index
                    >= crate::barrel_policy::EXPLODABLE_BARREL_DESTROYED_STATE,
            });
        }
        for (id, body) in &self.crate_bodies {
            let Some(origin) = lookup_origin(&self.crate_origins, *id) else {
                continue;
            };
            targets.push(DestructibleSplashTarget {
                id: *id,
                origin,
                terminal: body.destroyed,
            });
        }
        for (id, body) in &self.destructable_bodies {
            let Some(origin) = lookup_origin(&self.destructable_origins, *id) else {
                continue;
            };
            targets.push(DestructibleSplashTarget {
                id: *id,
                origin,
                terminal: body.destroyed,
            });
        }
        targets.sort_by_key(|row| row.id);
        targets
    }
}
