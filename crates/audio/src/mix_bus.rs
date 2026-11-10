use asset_core::FamilyId;

use crate::render_core::{QUANTUM, SAMPLE_RATE};

fn coefficient(seconds: f32) -> f32 {
    1.0 - (-1.0 / (seconds * SAMPLE_RATE as f32)).exp()
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

struct Bus {
    namespace: FamilyId,
    stage: asset_audio::MixBusStage,
    buffer: [[f32; 2]; QUANTUM],
    wet: [[f32; 2]; QUANTUM],
    used: bool,
    envelope: f32,
}

pub(crate) struct MixBuses {
    buses: Vec<Bus>,
}

impl Default for MixBuses {
    fn default() -> Self {
        let mut buses: Vec<_> = asset_audio::audio_families()
            .iter()
            .map(|(namespace, policy)| Bus {
                namespace,
                stage: policy.mix_bus,
                buffer: [[0.0; 2]; QUANTUM],
                wet: [[0.0; 2]; QUANTUM],
                used: false,
                envelope: 0.0,
            })
            .collect();
        buses.sort_by_key(|bus| (bus.stage.order, bus.namespace));
        Self { buses }
    }
}

impl MixBuses {
    pub(crate) fn bus(
        &mut self,
        namespace: FamilyId,
    ) -> (&mut [[f32; 2]; QUANTUM], &mut [[f32; 2]; QUANTUM]) {
        let bus = self
            .buses
            .iter_mut()
            .find(|bus| bus.namespace == namespace)
            .expect("render media bus must belong to an installed audio family");
        if !bus.used {
            bus.used = true;
            bus.buffer = [[0.0; 2]; QUANTUM];
            bus.wet = [[0.0; 2]; QUANTUM];
        }
        (&mut bus.buffer, &mut bus.wet)
    }

    pub(crate) fn mix_into(
        &mut self,
        output: &mut [[f32; 2]; QUANTUM],
        wet: &mut [[f32; 2]; QUANTUM],
    ) {
        for bus in &mut self.buses {
            let stage = bus.stage;
            let Some(compressor) = stage.compressor else {
                if bus.used {
                    let trim = db_to_gain(stage.trim_db);
                    for ((target, frame), (send, wet_frame)) in output
                        .iter_mut()
                        .zip(&bus.buffer)
                        .zip(wet.iter_mut().zip(&bus.wet))
                    {
                        target[0] += frame[0] * trim;
                        target[1] += frame[1] * trim;
                        send[0] += wet_frame[0] * trim;
                        send[1] += wet_frame[1] * trim;
                    }
                }
                bus.used = false;
                continue;
            };
            if !bus.used {
                if bus.envelope < 1e-12 {
                    continue;
                }
                bus.buffer = [[0.0; 2]; QUANTUM];
                bus.wet = [[0.0; 2]; QUANTUM];
            }
            let attack = coefficient(compressor.attack_s);
            let release = coefficient(compressor.release_s);
            let slope = 1.0 - 1.0 / compressor.ratio;
            let mut envelope = bus.envelope;
            for ((target, frame), (send, wet_frame)) in output
                .iter_mut()
                .zip(&bus.buffer)
                .zip(wet.iter_mut().zip(&bus.wet))
            {
                let power = (frame[0] * frame[0] + frame[1] * frame[1]) * 0.5;
                let power = if power.is_finite() { power } else { 0.0 };
                envelope += (power - envelope) * if power > envelope { attack } else { release };
                let level_db = 10.0 * envelope.max(1e-12).log10();
                let over = (level_db - compressor.threshold_db).max(0.0);
                let gain = db_to_gain(stage.trim_db + compressor.makeup_db - over * slope);
                target[0] += frame[0] * gain;
                target[1] += frame[1] * gain;
                send[0] += wet_frame[0] * gain;
                send[1] += wet_frame[1] * gain;
            }
            bus.envelope = envelope;
            bus.used = false;
        }
    }
}

pub(crate) struct MasterLimiter {
    gain: f32,
}

impl Default for MasterLimiter {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}

const LIMITER_CEILING: f32 = 0.95;
const LIMITER_RELEASE_S: f32 = 0.08;

impl MasterLimiter {
    pub(crate) fn apply(&mut self, output: &mut [[f32; 2]; QUANTUM]) {
        let release = coefficient(LIMITER_RELEASE_S);
        for frame in output {
            let peak = frame[0].abs().max(frame[1].abs());
            let target = if peak > LIMITER_CEILING {
                LIMITER_CEILING / peak
            } else {
                1.0
            };
            self.gain = if target < self.gain {
                target
            } else {
                self.gain + (target - self.gain) * release
            };
            frame[0] *= self.gain;
            frame[1] *= self.gain;
        }
    }
}
