use std::collections::HashMap;

use asset_game::{OwnedTracerDef, TracerDefinitions};
use assets::{AssetEdge, TracerSpace};
use bevy::prelude::*;
use entity_iw4::{Trajectory, evaluate_trajectory};
use fx::{
    FxMsec, LE_MOVING_TRACER, LE_TR_LINEAR, LocalEntityPool, LocalEntitySlot, tracer_travel_msec,
};
use fx_iw4::{
    FX_BEAM_ADD_CAP, FX_TRACER_FIRST_PERSON_MAX_WIDTH, FX_TRACER_MIN_DIST, FxBeamTess,
    beam_segment_count, vec3_normalize,
};

use crate::host::CombatFxDump;

type TracerShotHistory = HashMap<(Option<sim::FireCause>, u32, u32, u16), (i32, u16, Option<bool>)>;

#[derive(Resource, Default)]
pub struct PreparedTracers(pub TracerDefinitions);

#[derive(Clone, Copy, Debug)]
pub struct TracerPellet {
    pub cause: Option<sim::FireCause>,
    pub source_id: u32,
    pub correlation: u32,
    pub pellet: u16,
}

impl TracerPellet {
    fn key(self) -> (Option<sim::FireCause>, u32, u32, u16) {
        if self.cause.is_some() {
            (self.cause, 0, 0, self.pellet)
        } else {
            (None, self.source_id, self.correlation, self.pellet)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TracerSegment {
    pub pellet: TracerPellet,
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub own_shot: bool,
}

#[derive(Resource, Default)]
pub struct TracerDrawGate {
    counts: HashMap<u32, u8>,

    shots: TracerShotHistory,
}

impl TracerDrawGate {
    pub fn maintain(&mut self, now: i32) {
        self.shots
            .retain(|_, (at, _, _)| (0..5000).contains(&now.wrapping_sub(*at)));
    }

    pub fn first_segment_of_pellet(
        &mut self,
        pellet: TracerPellet,
        segment: u16,
        now: i32,
    ) -> bool {
        let key = pellet.key();
        if let Some((_, first, _)) = self.shots.get(&key) {
            return *first == segment;
        }
        if self.shots.len() >= 8192 {
            return false;
        }
        self.shots.insert(key, (now, segment, None));
        true
    }

    fn should_spawn_pellet(&mut self, pellet: TracerPellet, draw_interval: u32) -> Option<bool> {
        let key = pellet.key();
        let (_, _, previous) = self.shots.get(&key)?;
        if let Some(selected) = previous {
            return Some(*selected);
        }
        let selected = self.should_spawn(pellet.source_id, draw_interval);
        if let Some(slot) = self.shots.get_mut(&key) {
            slot.2 = Some(selected);
        }
        Some(selected)
    }

    pub fn should_spawn(&mut self, source_id: u32, draw_interval: u32) -> bool {
        if draw_interval == 0 {
            return false;
        }
        let period = draw_interval.min(255) as u8;
        let entry = self.counts.entry(source_id).or_insert(0);
        if *entry == 0 || period < *entry {
            *entry = period;
        }
        *entry = entry.saturating_sub(1);
        if *entry != 0 {
            return false;
        }
        *entry = period;
        true
    }
}

pub struct QueuedBeam {
    pub tess: FxBeamTess,
    pub material: Option<usize>,
}

#[derive(Resource, Default)]
pub struct TracerWorld {
    pub pool: LocalEntityPool,
    pub queued: Vec<QueuedBeam>,
}

impl TracerWorld {
    pub fn live_count(&self) -> usize {
        self.pool.live_count()
    }

    pub fn tr_time_min(&self) -> Option<i32> {
        self.pool.tr_time_min()
    }

    pub fn end_time_min(&self) -> Option<i32> {
        self.pool.end_time_min()
    }
}

#[derive(Debug)]
pub enum TracerSpawnSkip {
    NoDef,
    Interval,
    Short,
    HistoryCapacity,
}

impl TracerWorld {
    pub fn try_spawn(
        &mut self,
        gate: &mut TracerDrawGate,
        catalog: &TracerDefinitions,
        tracer: AssetEdge<TracerSpace>,
        segment: TracerSegment,
        clock: FxMsec,
        combat: &mut CombatFxDump,
    ) -> Result<(), TracerSpawnSkip> {
        let Some(def) = tracer.bound_index().and_then(|index| catalog.def_at(index)) else {
            return Err(TracerSpawnSkip::NoDef);
        };
        combat.last_tracer_name = Some(def.name.clone());
        combat.last_tracer_mat_edge = Some(def.material.edge_kind().to_owned());
        combat.last_tracer_mat_index = def.material.bound_index().map(|i| i as i64);
        combat.last_tracer_material = def.present_name().map(str::to_owned);
        combat.last_tracer_bind = def
            .present_name()
            .map(|name| asset_game::material_bind_name(name).to_owned());
        combat.last_tracer_speed = Some(def.speed);
        combat.last_tracer_beam_length = Some(def.beam_length);
        combat.last_tracer_draw_interval = Some(i64::from(def.draw_interval));
        match gate.should_spawn_pellet(segment.pellet, def.draw_interval) {
            Some(true) => {}
            Some(false) => return Err(TracerSpawnSkip::Interval),
            None => return Err(TracerSpawnSkip::HistoryCapacity),
        }
        spawn_moving_tracer(
            self,
            def,
            segment.start,
            segment.end,
            segment.own_shot,
            clock,
        )
        .map_err(|_| TracerSpawnSkip::Short)
    }
}

fn spawn_moving_tracer(
    world: &mut TracerWorld,
    def: &OwnedTracerDef,
    start: [f32; 3],
    end: [f32; 3],
    own_shot: bool,
    clock: FxMsec,
) -> Result<(), ()> {
    let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let dist = math_iw4::vec3_length(delta);
    if dist <= FX_TRACER_MIN_DIST {
        return Err(());
    }
    let travel = tracer_travel_msec(dist, def.speed).ok_or(())?;
    let dir = vec3_normalize(delta);
    let mut width = def.beam_width;
    if own_shot {
        width = width.min(FX_TRACER_FIRST_PERSON_MAX_WIDTH);
    }
    world.pool.alloc(LocalEntitySlot {
        le_type: LE_MOVING_TRACER,
        pos_tr_time: clock.0,
        pos_tr_type: LE_TR_LINEAR,
        pos_tr_duration: 0,
        pos_tr_delta: [dir[0] * def.speed, dir[1] * def.speed, dir[2] * def.speed],
        pos_tr_base: start,
        end_time: clock.0.wrapping_add(travel),
        material: def.material.bound_index(),
        tracer_clip_dist: dist,
        beam_length: def.beam_length.max(0.0),
        beam_width: width.max(0.0),
        screw_dist: def.screw_dist,
        screw_radius: def.screw_radius.max(0.0),
        colors: def.colors,
        own_shot,
    });
    Ok(())
}

pub fn tick_tracer_beams(world: &mut TracerWorld, clock: FxMsec) {
    world.pool.free_expired(clock);
    world.queued.clear();
    for tr in world.pool.live() {
        let begin = evaluate_trajectory(
            &Trajectory {
                tr_time: tr.pos_tr_time,
                tr_type: tr.pos_tr_type,
                tr_duration: tr.pos_tr_duration,
                tr_delta: tr.pos_tr_delta,
                tr_base: tr.pos_tr_base,
            },
            clock.0,
        );
        let dir = vec3_normalize(tr.pos_tr_delta);
        let start_from_base = [
            begin[0] - tr.pos_tr_base[0],
            begin[1] - tr.pos_tr_base[1],
            begin[2] - tr.pos_tr_base[2],
        ];
        let length_from_base =
            start_from_base[0] * dir[0] + start_from_base[1] * dir[1] + start_from_base[2] * dir[2];
        let remaining = (tr.tracer_clip_dist - length_from_base).max(0.0);
        let beam_len = tr.beam_length.min(remaining).max(0.0);
        if beam_len <= 0.0 {
            continue;
        }
        let end = [
            begin[0] + dir[0] * beam_len,
            begin[1] + dir[1] * beam_len,
            begin[2] + dir[2] * beam_len,
        ];
        if world.queued.len() >= FX_BEAM_ADD_CAP {
            break;
        }
        world.queued.push(QueuedBeam {
            tess: FxBeamTess {
                begin,
                end,
                begin_radius: tr.beam_width,
                end_radius: tr.beam_width,
                colors: tr.colors,
                segment_count: beam_segment_count(beam_len, tr.screw_dist),
                wiggle_dist: tr.screw_radius,
            },
            material: tr.material,
        });
    }
}
