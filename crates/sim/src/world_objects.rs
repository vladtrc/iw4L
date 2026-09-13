use entity_iw4::{
    GGlassPiece, GLASS_DAMAGE_INVALID, GlassStateChange, glass_apply_damage, glass_collapse_piece,
    glass_is_solid, glass_shatter_seed_from_hit, glass_should_notify_destroyed,
    glass_state_from_damage, glass_weakened_collapse_time_cs,
};
use gamemode_iw4::{
    VEHICLE_HEALTHDRAIN_AMOUNT, VEHICLE_HEALTHDRAIN_INTERVAL_MS, VEHICLE_LOOPFX_INTERVAL_MS,
    VehicleBodyState, apply_destructible_part_player_bullet, apply_toy_player_bullet,
    apply_vehicle_player_bullet, g_radius_damage_amount, radius_damage_distance_to_aabb,
    toy_healthdrain_arms, vehicle_active_loop_fx, vehicle_death_fx_if_destroyed,
    vehicle_death_presentation_if_destroyed, vehicle_healthdrain_arms,
};

pub use gamemode_iw4::{ToyDestructibleKind, VehicleDestructibleKind};

use crate::bullet_collision::EntityCollisionEpoch;
use crate::identities::{DamageSource, LifeSequence, PelletId, ScriptModelId};
use crate::world::ClientId;

pub use entity_iw4::{
    GLASS_DAMAGE_TO_DESTROY, GLASS_DAMAGE_TO_WEAKEN, GLASS_MELEE_DAMAGE, GlassPaneBasis,
    GlassPieceState, GlassShatterSeed,
};

pub type GlassPieceId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlassPieceSnapshot {
    pub state: GlassPieceState,
    pub last_state_change_time: i32,
    pub shatter_seed: Option<GlassShatterSeed>,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldObjectSnapshot {
    pub destructible_stages: Vec<(ScriptModelId, u8)>,
    pub glass_pieces: Vec<(GlassPieceId, GlassPieceSnapshot)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructibleDamageIntent {
    pub source: DamageSource,
    pub pellet: PelletId,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub target: ScriptModelId,
    pub amount: u32,

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

#[derive(Clone, Copy, Debug, PartialEq)]
struct VehicleHealthDrain {
    wait_ms: u32,
    attacker: ClientId,
    attacker_life: LifeSequence,
    source: DamageSource,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleFxPulse {
    pub owner: ScriptModelId,
    pub origin: [f32; 3],
    pub def_name: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleSoundPulse {
    pub owner: ScriptModelId,
    pub origin: [f32; 3],
    pub alias: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DestructibleApplyReport {
    pub changed: usize,
    pub explodes: Vec<DestructibleExplodeEvent>,
}

enum VehicleApply {
    Unchanged,
    Changed,
    Exploded(DestructibleExplodeEvent),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldObjectState {
    destructible_stages: Vec<(ScriptModelId, u8)>,
    vehicle_bodies: Vec<(ScriptModelId, VehicleDestructibleKind, VehicleBodyState)>,

    vehicle_origins: Vec<(ScriptModelId, [f32; 3])>,
    vehicle_drains: Vec<(ScriptModelId, VehicleHealthDrain)>,

    vehicle_loopfx: Vec<(ScriptModelId, u32)>,

    vehicle_death_fx_emitted: Vec<ScriptModelId>,
    death_anim_times: Vec<(ScriptModelId, f32)>,

    toy_bodies: Vec<(ScriptModelId, ToyDestructibleKind, VehicleBodyState)>,
    toy_origins: Vec<(ScriptModelId, [f32; 3])>,
    toy_drains: Vec<(ScriptModelId, VehicleHealthDrain)>,
    toy_cap_start: Vec<VehicleFxPulse>,

    barrel_bodies: Vec<(ScriptModelId, VehicleBodyState)>,
    barrel_origins: Vec<(ScriptModelId, [f32; 3])>,

    barrel_drains: Vec<(ScriptModelId, VehicleHealthDrain)>,

    barrel_burn_start: Vec<VehicleFxPulse>,
    glass_pieces: Vec<(GlassPieceId, GGlassPiece)>,

    glass_panes: Vec<(GlassPieceId, GlassPaneBasis)>,

    pending_glass_destroyed: Vec<GlassPieceId>,
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
        self.toy_cap_start.clear();
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
        for (id, origin) in barrels {
            self.register_barrel_at(id, origin);
        }
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

    pub fn explodable_barrel_bodies(&self) -> &[(ScriptModelId, VehicleBodyState)] {
        &self.barrel_bodies
    }

    pub fn vehicle_bodies(&self) -> &[(ScriptModelId, VehicleDestructibleKind, VehicleBodyState)] {
        &self.vehicle_bodies
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
        let owner_cap =
            self.vehicle_bodies.len() + self.toy_bodies.len() + self.barrel_bodies.len() + 1;
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
            match self.apply_vehicle_amount(
                intent.target,
                intent.amount,
                intent.attacker,
                intent.attacker_life,
                intent.source,
            ) {
                VehicleApply::Unchanged => match self.apply_toy_amount(
                    intent.target,
                    intent.amount,
                    intent.attacker,
                    intent.attacker_life,
                    intent.source,
                ) {
                    VehicleApply::Unchanged => match self.apply_barrel_amount(
                        intent.target,
                        intent.amount,
                        intent.attacker,
                        intent.attacker_life,
                        intent.source,
                    ) {
                        VehicleApply::Unchanged => {}
                        VehicleApply::Changed => changed += 1,
                        VehicleApply::Exploded(explode) => {
                            changed += 1;
                            explodes.push(explode);
                        }
                    },
                    VehicleApply::Changed => changed += 1,
                    VehicleApply::Exploded(explode) => {
                        changed += 1;
                        explodes.push(explode);
                    }
                },
                VehicleApply::Changed => changed += 1,
                VehicleApply::Exploded(explode) => {
                    changed += 1;
                    explodes.push(explode);
                }
            }
        }
        DestructibleApplyReport { changed, explodes }
    }

    pub fn tick_vehicle_healthdrain(&mut self, dt_ms: u32) -> DestructibleApplyReport {
        let ids: Vec<ScriptModelId> = self.vehicle_drains.iter().map(|(id, _)| *id).collect();
        let mut changed = 0;
        let mut explodes = Vec::new();
        for id in ids {
            let Some(index) = self
                .vehicle_drains
                .iter()
                .position(|(drain_id, _)| *drain_id == id)
            else {
                continue;
            };
            self.vehicle_drains[index].1.wait_ms =
                self.vehicle_drains[index].1.wait_ms.saturating_add(dt_ms);
            while self.vehicle_drains[index].1.wait_ms >= VEHICLE_HEALTHDRAIN_INTERVAL_MS {
                self.vehicle_drains[index].1.wait_ms -= VEHICLE_HEALTHDRAIN_INTERVAL_MS;
                let drain = self.vehicle_drains[index].1;
                match self.apply_vehicle_amount(
                    id,
                    VEHICLE_HEALTHDRAIN_AMOUNT,
                    drain.attacker,
                    drain.attacker_life,
                    drain.source,
                ) {
                    VehicleApply::Unchanged => {}
                    VehicleApply::Changed => changed += 1,
                    VehicleApply::Exploded(explode) => {
                        changed += 1;
                        explodes.push(explode);
                        break;
                    }
                }
                if self
                    .vehicle_drains
                    .iter()
                    .position(|(drain_id, _)| *drain_id == id)
                    .is_none()
                {
                    break;
                }
            }
        }
        DestructibleApplyReport { changed, explodes }
    }

    pub fn take_new_death_fx_pulses(&mut self) -> (Vec<VehicleFxPulse>, Vec<VehicleSoundPulse>) {
        let mut fx = Vec::new();
        let mut sounds = Vec::new();
        let pending: Vec<(ScriptModelId, &'static str, &'static str, [f32; 3])> = self
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
                Some((*id, def.death_fx, def.death_sound, origin))
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
                ))
            }))
            .chain(self.toy_bodies.iter().filter_map(|(id, kind, body)| {
                if body.state_index < kind.destroyed_state() {
                    return None;
                }
                if self.vehicle_death_fx_emitted.iter().any(|have| have == id) {
                    return None;
                }
                let origin = lookup_origin(&self.toy_origins, *id)?;
                let def = kind.definition();
                Some((*id, def.death_fx, def.death_sound, origin))
            }))
            .collect();
        for (id, def_name, alias, origin) in pending {
            self.vehicle_death_fx_emitted.push(id);
            fx.push(VehicleFxPulse {
                owner: id,
                origin,
                def_name,
            });
            if !alias.is_empty() {
                sounds.push(VehicleSoundPulse {
                    owner: id,
                    origin,
                    alias,
                });
            }
        }
        (fx, sounds)
    }

    pub fn take_new_burn_fx_pulses(&mut self) -> Vec<VehicleFxPulse> {
        let mut pulses = std::mem::take(&mut self.barrel_burn_start);
        pulses.append(&mut self.toy_cap_start);
        pulses
    }

    pub fn tick_explodable_barrel_burn(&mut self, dt_ms: u32) -> DestructibleApplyReport {
        let ids: Vec<ScriptModelId> = self.barrel_drains.iter().map(|(id, _)| *id).collect();
        let mut changed = 0;
        let mut explodes = Vec::new();
        for id in ids {
            let Some(index) = self
                .barrel_drains
                .iter()
                .position(|(drain_id, _)| *drain_id == id)
            else {
                continue;
            };
            self.barrel_drains[index].1.wait_ms =
                self.barrel_drains[index].1.wait_ms.saturating_add(dt_ms);
            while self.barrel_drains[index].1.wait_ms
                >= crate::barrel_policy::EXPLODABLE_BARREL_BURN_DRAIN_INTERVAL_MS
            {
                self.barrel_drains[index].1.wait_ms -=
                    crate::barrel_policy::EXPLODABLE_BARREL_BURN_DRAIN_INTERVAL_MS;
                let drain = self.barrel_drains[index].1;
                match self.apply_barrel_amount(
                    id,
                    crate::barrel_policy::EXPLODABLE_BARREL_BURN_DRAIN,
                    drain.attacker,
                    drain.attacker_life,
                    drain.source,
                ) {
                    VehicleApply::Unchanged => {}
                    VehicleApply::Changed => changed += 1,
                    VehicleApply::Exploded(explode) => {
                        changed += 1;
                        explodes.push(explode);
                        break;
                    }
                }
                if self
                    .barrel_drains
                    .iter()
                    .position(|(drain_id, _)| *drain_id == id)
                    .is_none()
                {
                    break;
                }
            }
        }
        DestructibleApplyReport { changed, explodes }
    }

    pub fn tick_toy_healthdrain(&mut self, dt_ms: u32) -> DestructibleApplyReport {
        let ids: Vec<ScriptModelId> = self.toy_drains.iter().map(|(id, _)| *id).collect();
        let mut changed = 0;
        let mut explodes = Vec::new();
        for id in ids {
            let Some(index) = self
                .toy_drains
                .iter()
                .position(|(drain_id, _)| *drain_id == id)
            else {
                continue;
            };
            let Some((_, kind, _)) = self.toy_bodies.iter().find(|(have, _, _)| *have == id) else {
                remove_drain(&mut self.toy_drains, id);
                continue;
            };
            let Some((amount, wait_s, _, _)) = kind.definition().health_drain else {
                remove_drain(&mut self.toy_drains, id);
                continue;
            };
            let interval_ms = (wait_s * 1000.0) as u32;
            if interval_ms == 0 {
                remove_drain(&mut self.toy_drains, id);
                continue;
            }
            self.toy_drains[index].1.wait_ms =
                self.toy_drains[index].1.wait_ms.saturating_add(dt_ms);
            while self.toy_drains[index].1.wait_ms >= interval_ms {
                self.toy_drains[index].1.wait_ms -= interval_ms;
                let drain = self.toy_drains[index].1;
                match self.apply_toy_amount(
                    id,
                    amount,
                    drain.attacker,
                    drain.attacker_life,
                    drain.source,
                ) {
                    VehicleApply::Unchanged => {}
                    VehicleApply::Changed => changed += 1,
                    VehicleApply::Exploded(explode) => {
                        changed += 1;
                        explodes.push(explode);
                        break;
                    }
                }
                if self
                    .toy_drains
                    .iter()
                    .position(|(drain_id, _)| *drain_id == id)
                    .is_none()
                {
                    break;
                }
            }
        }
        DestructibleApplyReport { changed, explodes }
    }

    pub fn tick_vehicle_loopfx(&mut self, dt_ms: u32) -> Vec<VehicleFxPulse> {
        let desired: Vec<(ScriptModelId, &'static str, [f32; 3], u32)> = self
            .vehicle_bodies
            .iter()
            .filter_map(|(id, kind, body)| {
                let def = kind.definition();
                let fx = vehicle_active_loop_fx(def, body.state_index, kind.destroyed_state())?;
                let origin = lookup_origin(&self.vehicle_origins, *id)?;
                Some((*id, fx, origin, VEHICLE_LOOPFX_INTERVAL_MS))
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
                    *id,
                    crate::barrel_policy::EXPLODABLE_BARREL_BURN_LOOP_FX,
                    origin,
                    crate::barrel_policy::EXPLODABLE_BARREL_BURN_LOOP_INTERVAL_MS,
                ))
            }))
            .chain(self.toy_bodies.iter().filter_map(|(id, kind, body)| {
                if body.state_index >= kind.destroyed_state() {
                    return None;
                }
                if !self.toy_drains.iter().any(|(have, _)| have == id) {
                    return None;
                }
                let def = kind.definition();
                let fx = def.leak_loop_fx?;
                let origin = lookup_origin(&self.toy_origins, *id)?;
                Some((*id, fx, origin, VEHICLE_LOOPFX_INTERVAL_MS))
            }))
            .collect();
        self.vehicle_loopfx
            .retain(|(id, _)| desired.iter().any(|(want, _, _, _)| want == id));
        for (id, _, _, _) in &desired {
            if !self.vehicle_loopfx.iter().any(|(have, _)| have == id) {
                self.vehicle_loopfx.push((*id, 0));
            }
        }
        let mut pulses = Vec::new();
        for (id, fx, origin, interval) in desired {
            let Some((_, wait)) = self.vehicle_loopfx.iter_mut().find(|(have, _)| *have == id)
            else {
                continue;
            };
            if *wait > dt_ms {
                *wait -= dt_ms;
                continue;
            }
            pulses.push(VehicleFxPulse {
                owner: id,
                origin,
                def_name: fx,
            });
            *wait = interval;
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
    ) -> VehicleApply {
        let Some(index) = self
            .vehicle_bodies
            .iter()
            .position(|(id, _, _)| *id == target)
        else {
            return VehicleApply::Unchanged;
        };
        let kind = self.vehicle_bodies[index].1;
        let def = kind.definition();
        let destroyed = kind.destroyed_state();
        let body = self.vehicle_bodies[index].2;
        let was_destroyed = body.state_index >= destroyed;
        let next = apply_vehicle_player_bullet(def, destroyed, body, amount);
        if next == body {
            return VehicleApply::Unchanged;
        }
        self.vehicle_bodies[index].2 = next;
        self.set_destructible_state(target, DestructibleStateIndex::new(next.state_index));
        if vehicle_healthdrain_arms(body.state_index, next.state_index, destroyed) {
            if !self.vehicle_drains.iter().any(|(id, _)| *id == target) {
                self.vehicle_drains.push((
                    target,
                    VehicleHealthDrain {
                        wait_ms: 0,
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
            return VehicleApply::Exploded(DestructibleExplodeEvent {
                owner: target,
                origin,
                attacker,
                attacker_life,
                source: DamageSource::Radius(target),
                explode_range_mp: def.explode_range_mp,
                explode_damage: def.explode_damage,
            });
        }
        VehicleApply::Changed
    }

    fn apply_toy_amount(
        &mut self,
        target: ScriptModelId,
        amount: u32,
        attacker: ClientId,
        attacker_life: LifeSequence,
        source: DamageSource,
    ) -> VehicleApply {
        let Some(index) = self.toy_bodies.iter().position(|(id, _, _)| *id == target) else {
            return VehicleApply::Unchanged;
        };
        let kind = self.toy_bodies[index].1;
        let def = kind.definition();
        let destroyed = kind.destroyed_state();
        let body = self.toy_bodies[index].2;
        let was_destroyed = body.state_index >= destroyed;
        let next = apply_toy_player_bullet(def, body, amount);
        if next == body {
            return VehicleApply::Unchanged;
        }
        self.toy_bodies[index].2 = next;
        self.set_destructible_state(target, DestructibleStateIndex::new(next.state_index));

        if toy_healthdrain_arms(body.state_index, next.state_index, destroyed)
            && def.health_drain.is_some()
            && def.leak_loop_fx.is_some()
            && !self.toy_drains.iter().any(|(id, _)| *id == target)
        {
            self.toy_drains.push((
                target,
                VehicleHealthDrain {
                    wait_ms: 0,
                    attacker,
                    attacker_life,
                    source,
                },
            ));
            self.toy_drains.sort_by_key(|(id, _)| *id);
            if let (Some(origin), Some(cap_fx)) =
                (lookup_origin(&self.toy_origins, target), def.cap_fx)
            {
                self.toy_cap_start.push(VehicleFxPulse {
                    owner: target,
                    origin,
                    def_name: cap_fx,
                });
            }
        }
        if next.state_index >= destroyed {
            remove_drain(&mut self.toy_drains, target);
        }
        if !was_destroyed
            && next.state_index >= destroyed
            && let Some(origin) = lookup_origin(&self.toy_origins, target)
        {
            return VehicleApply::Exploded(DestructibleExplodeEvent {
                owner: target,
                origin,
                attacker,
                attacker_life,
                source: DamageSource::Radius(target),
                explode_range_mp: def.explode_range_mp,
                explode_damage: def.explode_damage,
            });
        }
        VehicleApply::Changed
    }

    fn apply_barrel_amount(
        &mut self,
        target: ScriptModelId,
        amount: u32,
        attacker: ClientId,
        attacker_life: LifeSequence,
        source: DamageSource,
    ) -> VehicleApply {
        let Some(index) = self.barrel_bodies.iter().position(|(id, _)| *id == target) else {
            return VehicleApply::Unchanged;
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
            return VehicleApply::Unchanged;
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
                });
            }
        }
        if !was_destroyed
            && next.state_index >= destroyed
            && let Some(origin) = lookup_origin(&self.barrel_origins, target)
        {
            return VehicleApply::Exploded(DestructibleExplodeEvent {
                owner: target,
                origin,
                attacker,
                attacker_life,
                source: DamageSource::Radius(target),
                explode_range_mp: crate::barrel_policy::EXPLODABLE_BARREL_EXPLODE_RANGE,
                explode_damage: crate::barrel_policy::EXPLODABLE_BARREL_EXPLODE_DAMAGE,
            });
        }
        VehicleApply::Changed
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

    pub fn apply_glass_damage(
        &mut self,
        id: GlassPieceId,
        damage: u32,
        at_time_ms: i32,
        weakened_collapse_time_cs: Option<u16>,
        shatter_seed: Option<GlassShatterSeed>,
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
        }
        if piece.damage == 0 {
            remove_key(&mut self.glass_pieces, id);
        } else {
            upsert_value(&mut self.glass_pieces, id, piece);
        }
        self.glass_piece_state(id)
    }

    pub fn install_glass_panes(&mut self, panes: Vec<(GlassPieceId, GlassPaneBasis)>) {
        self.glass_panes = panes;
        sort_pairs(&mut self.glass_panes);
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
        self.apply_glass_damage(id, damage, at_time_ms, collapse, seed)
    }

    pub fn glass_update(&mut self, at_time_ms: i32) {
        let ids: Vec<GlassPieceId> = self.glass_pieces.iter().map(|(id, _)| *id).collect();
        for id in ids {
            let mut piece = self.glass_piece(id);
            if let Some(change) = glass_collapse_piece(&mut piece, at_time_ms) {
                self.note_glass_destroyed(id, change);
                upsert_value(&mut self.glass_pieces, id, piece);
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
                        last_state_change_time: piece.last_state_change_time,
                        shatter_seed: piece.shatter_seed(),
                    },
                ))
            })
            .collect::<Vec<_>>();
        sort_pairs(&mut glass_pieces);

        WorldObjectSnapshot {
            destructible_stages,
            glass_pieces,
        }
    }

    pub fn adopt_snapshot(&mut self, snap: &WorldObjectSnapshot) {
        self.destructible_stages = snap.destructible_stages.clone();
        sort_pairs(&mut self.destructible_stages);

        self.glass_pieces.clear();
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
            }
        }
    }

    fn glass_piece(&self, id: GlassPieceId) -> GGlassPiece {
        match lookup_value(&self.glass_pieces, id) {
            Some(piece) => piece,
            None => GGlassPiece::default(),
        }
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
        targets.sort_by_key(|row| row.id);
        targets
    }
}
