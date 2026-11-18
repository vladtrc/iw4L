use crate::flags::{elem_uses_vel_local, elem_uses_vel_world};
use crate::integrate::sample_vel_graph_at_age_sampled;

pub const FX_VEL_AT_TIME_SCALE: f64 = 1000.0;

#[derive(Clone, Copy, Debug)]
pub struct FxVelocityField<'a> {
    pub flags: i32,
    pub local_samples: &'a [crate::FxElemVec3Range],
    pub world_samples: &'a [crate::FxElemVec3Range],
}

impl FxVelocityField<'_> {
    #[inline]
    pub fn at_time(
        &self,
        base_vel: [f32; 3],
        age_msec: f32,
        life_ms: f32,
        orient_axis: [[f32; 3]; 3],
        seed: u64,
    ) -> [f32; 3] {
        if !(elem_uses_vel_world(self.flags) && self.world_samples.len() >= 2)
            && !(elem_uses_vel_local(self.flags) && self.local_samples.len() >= 2)
        {
            return base_vel;
        }
        self.at_time_sampled(
            base_vel,
            age_msec,
            life_ms,
            orient_axis,
            crate::random::velocity_random(seed),
        )
    }

    #[inline]
    pub fn at_time_sampled(
        &self,
        base_vel: [f32; 3],
        age_msec: f32,
        life_ms: f32,
        orient_axis: [[f32; 3]; 3],
        random: [f32; 3],
    ) -> [f32; 3] {
        let mut out = base_vel;
        let age01 = if life_ms > 0.0 {
            let t = age_msec / life_ms;
            t.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let scale = FX_VEL_AT_TIME_SCALE as f32;

        if elem_uses_vel_world(self.flags) && self.world_samples.len() >= 2 {
            let s = sample_vel_graph_at_age_sampled(self.world_samples, age01, random);
            out[0] += s[0] * scale;
            out[1] += s[1] * scale;
            out[2] += s[2] * scale;
        }
        if elem_uses_vel_local(self.flags) && self.local_samples.len() >= 2 {
            let s = sample_vel_graph_at_age_sampled(self.local_samples, age01, random);

            let wx = s[0] * orient_axis[0][0] + s[1] * orient_axis[1][0] + s[2] * orient_axis[2][0];
            let wy = s[0] * orient_axis[0][1] + s[1] * orient_axis[1][1] + s[2] * orient_axis[2][1];
            let wz = s[0] * orient_axis[0][2] + s[1] * orient_axis[1][2] + s[2] * orient_axis[2][2];
            out[0] += wx * scale;
            out[1] += wy * scale;
            out[2] += wz * scale;
        }
        out
    }

    #[inline]
    pub fn launch_velocity(&self, orient_axis: [[f32; 3]; 3], random: [f32; 3]) -> [f32; 3] {
        let mut out = [0.0f32; 3];
        if elem_uses_vel_world(self.flags) && self.world_samples.len() >= 2 {
            let s = sample_vel_graph_at_age_sampled(self.world_samples, 0.0, random);
            let scale = graph_speed_scale(self.world_samples.len());
            for (out, s) in out.iter_mut().zip(s) {
                *out += s * scale;
            }
        }
        if elem_uses_vel_local(self.flags) && self.local_samples.len() >= 2 {
            let s = sample_vel_graph_at_age_sampled(self.local_samples, 0.0, random);
            let scale = graph_speed_scale(self.local_samples.len());
            for (axis, out) in out.iter_mut().enumerate() {
                let w = s[0] * orient_axis[0][axis]
                    + s[1] * orient_axis[1][axis]
                    + s[2] * orient_axis[2][axis];
                *out += w * scale;
            }
        }
        out
    }
}

fn graph_speed_scale(fenceposts: usize) -> f32 {
    (fenceposts - 1) as f32 * FX_VEL_AT_TIME_SCALE as f32
}
