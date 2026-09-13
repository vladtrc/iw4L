pub const FX_ELEM_TYPE_TRAIL: u8 = 3;

pub const FX_ELEM_TYPE_SPARK_CLOUD: u8 = 5;

pub const FX_ELEM_TYPE_SPARK_FOUNTAIN: u8 = 6;

#[inline]
pub fn fx_sample_oneshot_spawn_count(base: i32, amplitude: i32, rand16: u16) -> i32 {
    crate::life::fx_sample_life_span_msec(base, amplitude, rand16)
}

#[inline]
pub fn fx_spawn_def_from_bytes(bytes: &[u8; 8]) -> (i32, i32) {
    crate::life::fx_life_span_range_from_bytes(bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxLoopingSpawn {
    pub msec: i32,

    pub sequence: i32,
}

#[inline]
pub fn fx_looping_catchup_begin(
    msec_update_begin: i32,
    msec_update_end: i32,
    elem_duration_msec: i32,
) -> i32 {
    if msec_update_end == i32::MAX {
        return msec_update_begin;
    }
    let window = msec_update_end.wrapping_sub(msec_update_begin);
    if window <= 0x80 {
        return msec_update_begin;
    }
    let keep = elem_duration_msec.wrapping_add(1);
    if keep < window {
        msec_update_begin.wrapping_add(window.wrapping_sub(keep))
    } else {
        msec_update_begin
    }
}

#[inline]
pub fn fx_looping_spawn_schedule(
    msec_when_played: i32,
    msec_update_begin: i32,
    msec_update_end: i32,
    interval_msec: i32,
    max_count: i32,
) -> FxLoopingSpawnSchedule {
    FxLoopingSpawnSchedule {
        msec_when_played,
        msec_update_end,
        interval_msec,
        max_count,
        spawned: if interval_msec <= 0 {
            max_count
        } else {
            (msec_update_begin.wrapping_sub(msec_when_played)) / interval_msec + 1
        },
    }
}

#[derive(Clone, Debug)]
pub struct FxLoopingSpawnSchedule {
    msec_when_played: i32,
    msec_update_end: i32,
    interval_msec: i32,
    max_count: i32,
    spawned: i32,
}

impl Iterator for FxLoopingSpawnSchedule {
    type Item = FxLoopingSpawn;

    fn next(&mut self) -> Option<Self::Item> {
        if self.interval_msec <= 0 || self.spawned >= self.max_count {
            return None;
        }
        let next_msec = self
            .msec_when_played
            .wrapping_add(self.interval_msec.wrapping_mul(self.spawned));
        if next_msec > self.msec_update_end {
            return None;
        }
        let out = FxLoopingSpawn {
            msec: next_msec,
            sequence: self.spawned,
        };
        self.spawned = self.spawned.wrapping_add(1);
        Some(out)
    }
}

#[inline]
pub const fn fx_spawn_effect_status(msec_looping_life: i32) -> u32 {
    let pending = if msec_looping_life != 0 { 0x8001 } else { 0 };
    0x4000_0001u32.wrapping_add(pending)
}
