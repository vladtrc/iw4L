use anim_iw4::{
    XANIM_LEGS_PARENT_WEIGHT_WHEN_TORSO, client_anim_blend_ms, client_anim_playback_rate,
    goal_time_from_blend_ms,
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerAnimProperties {
    pub ladder: bool,
    pub stationary: bool,
    pub blend_ms: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerBodyBranches {
    pub legs: crate::XAnimNodeId,
    pub torso: crate::XAnimNodeId,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClientAnimSample {
    pub origin: [f32; 3],
    pub time_ms: i32,
    pub move_speed: f32,
    pub ladder: bool,
}

pub fn apply_player_anim_rates(
    runtime: &mut crate::XAnimTreeRuntime,
    legs_sample: &mut ClientAnimSample,
    torso_sample: &mut ClientAnimSample,
    legs_index: u16,
    torso_index: u16,
    origin: [f32; 3],
    pose_time_ms: i32,
) -> Result<(), String> {
    apply_one_client_anim_rate(runtime, legs_index, origin, pose_time_ms, legs_sample)?;
    if torso_index != 0 {
        apply_one_client_anim_rate(runtime, torso_index, origin, pose_time_ms, torso_sample)?;
    }
    Ok(())
}

fn apply_one_client_anim_rate(
    runtime: &mut crate::XAnimTreeRuntime,
    index: u16,
    origin: [f32; 3],
    pose_time_ms: i32,
    sample: &mut ClientAnimSample,
) -> Result<(), String> {
    if index == 0 {
        return Ok(());
    }
    if pose_time_ms < sample.time_ms {
        sample.time_ms = 0;
    }
    let Some(rate) = client_anim_playback_rate(
        origin,
        sample.origin,
        pose_time_ms,
        sample.time_ms,
        sample.move_speed,
        sample.ladder,
    ) else {
        return Ok(());
    };
    runtime
        .set_rate(crate::XAnimNodeId(index), rate)
        .map_err(|error| error.to_string())?;
    *sample = ClientAnimSample {
        origin,
        time_ms: pose_time_ms,
        ..*sample
    };
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerAnimTransition {
    pub previous: u16,
    pub index: u16,
    pub restart: bool,
    pub was_moving: bool,
    pub moving: bool,
    pub authored_blend_ms: i32,
}

pub fn apply_player_anim_goals(
    runtime: &mut crate::XAnimTreeRuntime,
    branches: PlayerBodyBranches,
    legs: PlayerAnimTransition,
    torso: PlayerAnimTransition,
) -> Result<(), String> {
    let legs_time = goal_time_from_blend_ms(client_anim_blend_ms(
        legs.index,
        legs.authored_blend_ms,
        legs.previous != 0,
        false,
        legs.was_moving,
        legs.moving,
    ));
    let torso_time = goal_time_from_blend_ms(client_anim_blend_ms(
        torso.index,
        torso.authored_blend_ms,
        torso.previous != 0,
        true,
        torso.was_moving,
        torso.moving,
    ));
    if legs.previous != 0 && legs.previous != legs.index {
        runtime
            .set_goal_weight(crate::XAnimNodeId(legs.previous), 0.0, legs_time)
            .map_err(|error| error.to_string())?;
    }
    if torso.previous != 0 && torso.previous != torso.index {
        runtime
            .set_goal_weight(crate::XAnimNodeId(torso.previous), 0.0, torso_time)
            .map_err(|error| error.to_string())?;
    }
    if legs.previous == legs.index && !legs.restart {
        runtime
            .set_goal_weight(crate::XAnimNodeId(legs.index), 1.0, legs_time)
            .map_err(|error| error.to_string())?;
    } else {
        runtime
            .set_complete_goal_weight_in(crate::XAnimNodeId(legs.index), 0.0, 1.0, legs_time)
            .map_err(|error| error.to_string())?;
    }
    if torso.index != 0 {
        if torso.previous == torso.index && !torso.restart {
            runtime
                .set_goal_weight(crate::XAnimNodeId(torso.index), 1.0, torso_time)
                .map_err(|error| error.to_string())?;
        } else {
            runtime
                .set_complete_goal_weight_in(crate::XAnimNodeId(torso.index), 0.0, 1.0, torso_time)
                .map_err(|error| error.to_string())?;
        }
    }
    runtime
        .set_complete_goal_weight_in(
            branches.legs,
            0.0,
            if torso.index != 0 {
                XANIM_LEGS_PARENT_WEIGHT_WHEN_TORSO
            } else {
                1.0
            },
            torso_time,
        )
        .map_err(|error| error.to_string())?;
    runtime
        .set_complete_goal_weight_in(
            branches.torso,
            0.0,
            if torso.index != 0 { 1.0 } else { 0.0 },
            torso_time,
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn overlay_legs_clip(legs: &crate::AnimClip, torso: &crate::AnimClip) -> crate::AnimClip {
    let names: HashSet<&str> = torso
        .tracks
        .iter()
        .map(|track| track.name.as_str())
        .collect();
    let mut overlayed = legs.clone();
    overlayed
        .tracks
        .retain(|track| !names.contains(track.name.as_str()));
    overlayed
}
