pub const BODY_TARGETNAME: &str = "digger_body";

pub const BLADE_TARGETNAME: &str = "digger_blade";

pub const ARM_MOVE_MS: u32 = 11_000;

pub const BLADE_SPIN_MS: u32 = 80_000;

pub const BLADE_SPIN_UP_MS: u32 = 3_000;

pub const DIG_DELAY_MS: u32 = 20_000;

pub const ARM_PITCH_DEG: f32 = 45.0;

pub const BLADE_SPIN_DEG: f32 = 1800.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiggerPhase {
    BodyOut,
    ArmDown,
    BladeSpin,
    ArmUp,
    BodyBack,
    Delay,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiggerPose {
    pub body_yaw: f32,
    pub arm_pitch: f32,
    pub blade_pitch: f32,
    pub fx: bool,
}

pub fn is_digger_body(targetname: &str) -> bool {
    targetname == BODY_TARGETNAME
}

pub fn is_digger_blade(targetname: &str) -> bool {
    targetname == BLADE_TARGETNAME
}

/// The body always takes some time to swing, so a zero turn still costs one degree.
fn turn_magnitude_deg(turn: i32) -> i32 {
    i32::try_from(turn.unsigned_abs().max(1)).unwrap_or(1)
}

pub fn turn_deg(digger: usize, cycle: u32) -> i32 {
    let n = (digger as u32)
        .wrapping_mul(13)
        .wrapping_add(cycle.wrapping_mul(17))
        % 31;
    n as i32 - 15
}

pub fn turn_speed_ms(turn: i32) -> u32 {
    (turn_magnitude_deg(turn) as u32).saturating_mul(300)
}

fn linear(elapsed_ms: u32, duration_ms: u32) -> f32 {
    if duration_ms == 0 {
        1.0
    } else {
        (elapsed_ms as f32 / duration_ms as f32).clamp(0.0, 1.0)
    }
}

pub fn pose_at(elapsed_ms: u32, digger: usize) -> DiggerPose {
    let _ = BLADE_SPIN_UP_MS;
    let mut remaining = elapsed_ms;
    let mut cycle = 0u32;
    let turn = loop {
        let turn = turn_deg(digger, cycle);
        let body_ms = turn_speed_ms(turn);
        let total = body_ms
            .saturating_add(ARM_MOVE_MS)
            .saturating_add(BLADE_SPIN_MS)
            .saturating_add(ARM_MOVE_MS)
            .saturating_add(body_ms)
            .saturating_add(DIG_DELAY_MS);
        if remaining < total {
            break turn;
        }
        remaining -= total;
        cycle = cycle.saturating_add(1);
    };
    let body_ms = turn_speed_ms(turn);
    let phases = [
        (DiggerPhase::BodyOut, body_ms),
        (DiggerPhase::ArmDown, ARM_MOVE_MS),
        (DiggerPhase::BladeSpin, BLADE_SPIN_MS),
        (DiggerPhase::ArmUp, ARM_MOVE_MS),
        (DiggerPhase::BodyBack, body_ms),
        (DiggerPhase::Delay, DIG_DELAY_MS),
    ];
    let mut phase = DiggerPhase::Delay;
    let mut in_phase = 0u32;
    let mut duration = DIG_DELAY_MS;
    for (name, ms) in phases {
        if remaining < ms {
            phase = name;
            in_phase = remaining;
            duration = ms;
            break;
        }
        remaining -= ms;
    }
    let p = linear(in_phase, duration);
    let yaw = turn as f32;
    let spun = cycle as f32 * BLADE_SPIN_DEG;
    let (body_yaw, arm_pitch, blade_pitch) = match phase {
        DiggerPhase::BodyOut => (yaw * p, 0.0, spun),
        DiggerPhase::ArmDown => (yaw, -ARM_PITCH_DEG * p, spun),
        DiggerPhase::BladeSpin => (yaw, -ARM_PITCH_DEG, spun + BLADE_SPIN_DEG * p),
        DiggerPhase::ArmUp => (yaw, -ARM_PITCH_DEG * (1.0 - p), spun + BLADE_SPIN_DEG),
        DiggerPhase::BodyBack => (yaw * (1.0 - p), 0.0, spun + BLADE_SPIN_DEG),
        DiggerPhase::Delay => (0.0, 0.0, spun + BLADE_SPIN_DEG),
    };
    DiggerPose {
        body_yaw,
        arm_pitch,
        blade_pitch,
        fx: phase == DiggerPhase::BladeSpin && in_phase < 50,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_stays_within_the_authored_swing() {
        for cycle in 0..64 {
            let t = turn_deg(0, cycle);
            assert!((-15..=15).contains(&t));
        }
    }

    #[test]
    fn blade_spin_is_1800_at_end_of_spin() {
        let turn = turn_deg(0, 0);
        let body_ms = turn_speed_ms(turn);
        let at_spin_end = body_ms + ARM_MOVE_MS + BLADE_SPIN_MS - 1;
        let pose = pose_at(at_spin_end, 0);
        assert!(pose.blade_pitch > 1790.0);
        assert!((pose.arm_pitch + ARM_PITCH_DEG).abs() < 0.01);
        assert!(pose.body_yaw.abs() > 0.0 || turn == 0);
    }

    #[test]
    fn fx_only_on_first_spin_tick() {
        let turn = turn_deg(0, 0);
        let spin_start = turn_speed_ms(turn) + ARM_MOVE_MS;
        assert!(pose_at(spin_start, 0).fx);
        assert!(!pose_at(spin_start + 50, 0).fx);
    }

    #[test]
    fn blade_keeps_spun_angle_into_the_next_cycle() {
        let turn = turn_deg(0, 0);
        let cycle_ms = turn_speed_ms(turn)
            .saturating_add(ARM_MOVE_MS)
            .saturating_add(BLADE_SPIN_MS)
            .saturating_add(ARM_MOVE_MS)
            .saturating_add(turn_speed_ms(turn))
            .saturating_add(DIG_DELAY_MS);
        let pose = pose_at(cycle_ms, 0);
        assert!((pose.blade_pitch - BLADE_SPIN_DEG).abs() < 1.0);
    }
}
