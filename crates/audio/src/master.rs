use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use bevy::prelude::*;

pub(crate) const DEFAULT_FADE_MS: u32 = 500;

#[derive(Clone, Copy, Debug)]
pub(crate) struct MasterVoice {
    pub master: bool,
    pub master_percentage: f32,
    slave_percentage: Option<f32>,
}

impl MasterVoice {
    pub fn from_policy(
        policy: Option<asset_audio::MasterSlavePolicy>,
        forced: bool,
    ) -> Result<Self, crate::StartFailure> {
        let (mut master, mut master_percentage, mut slave) = (forced, 0.0, None);
        if let Some(policy) = policy {
            let percentage = if policy.master || policy.slave {
                policy
                    .percentage
                    .filter(|v| v.is_finite())
                    .ok_or(crate::StartFailure::InvalidMasterSlavePolicy)?
            } else {
                0.0
            };
            master |= policy.master;
            if policy.master {
                master_percentage = percentage;
            }
            if policy.slave {
                slave = Some(percentage);
            }
        }
        Ok(Self {
            master,
            master_percentage,
            slave_percentage: slave,
        })
    }

    pub fn gain(self, mix: &SlaveMixer) -> f32 {
        if self.master || mix.phase == 0.0 {
            return 1.0;
        }
        self.slave_percentage.map_or(1.0, |percentage| {
            (1.0 - mix.maximum) * percentage * mix.phase + (1.0 - mix.phase)
        })
    }
}

pub(crate) struct SlaveControl {
    fade_ms: AtomicU32,
    epoch: AtomicU64,
}

impl Default for SlaveControl {
    fn default() -> Self {
        Self {
            fade_ms: AtomicU32::new(DEFAULT_FADE_MS),
            epoch: AtomicU64::new(0),
        }
    }
}

impl SlaveControl {
    pub fn set_fade_ms(&self, value: u32) {
        if value <= 5000 {
            self.fade_ms.store(value, Ordering::Release);
        }
    }
    pub fn reset(&self) {
        self.epoch.fetch_add(1, Ordering::Release);
    }
}

#[derive(Default)]
pub(crate) struct SlaveMixer {
    phase: f32,
    maximum: f32,
    epoch: u64,
}

impl SlaveMixer {
    pub fn advance(&mut self, active: bool, maximum: f32, frames: u64, control: &SlaveControl) {
        let epoch = control.epoch.load(Ordering::Acquire);
        if self.epoch != epoch {
            *self = Self {
                epoch,
                ..Default::default()
            };
        }
        let fade_ms = control.fade_ms.load(Ordering::Acquire);
        if fade_ms == 0 {
            self.phase = f32::from(active);
            self.maximum = if active { maximum } else { 0.0 };
        } else {
            let step = frames as f32 * 1000.0 / (crate::SAMPLE_RATE as f32 * fade_ms as f32);
            self.phase = (self.phase + if active { step } else { -step }).clamp(0.0, 1.0);
            self.maximum += (maximum - self.maximum).clamp(-step, step);
        }
    }
}

fn update(
    runtime: Res<crate::AudioRuntime>,
    local: Option<Res<net::LocalPresentClient>>,
    presented: Option<Res<net::PresentedSnapshot>>,
    dvars: Option<Res<frame::UiMenuDvars>>,
) {
    let setting = local
        .as_ref()
        .and_then(|local| {
            presented
                .as_ref()?
                .snapshot()?
                .meta
                .script_dvars(local.0)
                .string("snd_slaveFadeTime")
        })
        .or_else(|| dvars.as_ref()?.get("snd_slaveFadeTime"));
    if let Some(value) = setting {
        let value = sim::script::host::natives::iw4::atoi(value);
        if (0..=5000).contains(&value) {
            runtime.set_slave_fade_time(value as u32);
        }
    } else {
        runtime.set_slave_fade_time(DEFAULT_FADE_MS);
    }
}

pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        update
            .in_set(frame::InMatch)
            .in_set(net::ClientSet::Effects),
    );
}
