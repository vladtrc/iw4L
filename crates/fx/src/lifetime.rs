use crate::system::FxSystemHost;

pub const LOCAL_ENTITY_SIZE: usize = 100;

pub const LOCAL_ENTITY_POOL_CAPACITY: usize = 128;

pub const LE_MOVING_TRACER: i32 = 0;

pub const LE_TR_LINEAR: i32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FxMsec(pub i32);

impl FxMsec {
    pub fn from_host(host: &FxSystemHost) -> Self {
        Self(host.msec_now)
    }
}

#[derive(Clone, Debug)]
pub struct LocalEntitySlot {
    pub le_type: i32,

    pub pos_tr_time: i32,

    pub pos_tr_type: i32,

    pub pos_tr_duration: i32,

    pub pos_tr_delta: [f32; 3],

    pub pos_tr_base: [f32; 3],

    pub end_time: i32,

    pub material: Option<usize>,

    pub tracer_clip_dist: f32,

    pub beam_length: f32,

    pub beam_width: f32,

    pub screw_dist: f32,

    pub screw_radius: f32,

    pub colors: [[f32; 4]; 5],

    pub own_shot: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LocalEntityPool {
    live: Vec<LocalEntitySlot>,
}

impl LocalEntityPool {
    pub fn new() -> Self {
        Self { live: Vec::new() }
    }

    pub fn live(&self) -> &[LocalEntitySlot] {
        &self.live
    }

    pub fn live_count(&self) -> usize {
        self.live.len()
    }

    pub fn tr_time_min(&self) -> Option<i32> {
        self.live.iter().map(|le| le.pos_tr_time).min()
    }

    pub fn end_time_min(&self) -> Option<i32> {
        self.live.iter().map(|le| le.end_time).min()
    }

    pub fn alloc(&mut self, slot: LocalEntitySlot) {
        if self.live.len() >= LOCAL_ENTITY_POOL_CAPACITY {
            self.live.remove(0);
        }
        self.live.push(slot);
    }

    pub fn free_expired(&mut self, clock: FxMsec) {
        self.live
            .retain(|le| local_entity_is_live(le.pos_tr_time, le.end_time, clock));
    }
}

#[inline]
pub fn local_entity_is_live(tr_time: i32, end_time: i32, clock: FxMsec) -> bool {
    tr_time <= clock.0 && clock.0 < end_time
}

pub fn tracer_travel_msec(dist: f32, speed: f32) -> Option<i32> {
    if dist <= 0.0 || speed <= 0.0 {
        return None;
    }
    let ms = (dist * 1000.0 / speed).round() as i32;
    (ms > 0).then_some(ms)
}

pub fn set_presentation_clock(host: &mut FxSystemHost, clock: FxMsec) {
    host.set_msec_now(clock.0);
}
