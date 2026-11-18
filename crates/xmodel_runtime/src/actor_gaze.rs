use crate::DObj;
use anim_iw4::Local;
use glam::{Mat4, Quat, Vec3};

const HEAD_PITCH_LIMIT: f32 = 45.0;
const HEAD_YAW_LIMIT: f32 = 60.0;
const EYE_PITCH_LIMIT: f32 = 5.0;
const EYE_YAW_LIMIT: f32 = 20.0;
const HEAD_YAW_SHARE: f32 = 0.7;
const RESPONSE_PER_REFERENCE_FRAME: f32 = 0.15;
const REFERENCE_HZ: f32 = 60.0;
const MIN_GAZE_WEIGHT: f32 = 0.0001;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorGazePose {
    pub angles: [f32; 2],
    pub weight: f32,
    pub limited: bool,
    pub eyes: u8,
}

#[derive(Clone, Debug, Default)]
pub struct ActorGazeController {
    angles: [f32; 2],
    weight: f32,
    limited: bool,
    at_ms: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorGazeError {
    MissingHead,
    MissingNeck,
    MissingEyeOrigin,
    InvalidPose,
    InvalidTarget,
}

struct Rig {
    head: usize,
    neck: usize,
    eye: usize,
    eyes: [Option<usize>; 2],
}
impl Rig {
    fn find(dobj: &DObj) -> Result<Self, ActorGazeError> {
        Ok(Self {
            head: dobj.find("j_head").ok_or(ActorGazeError::MissingHead)?,
            neck: dobj.find("j_neck").ok_or(ActorGazeError::MissingNeck)?,
            eye: dobj
                .find("tag_eye")
                .ok_or(ActorGazeError::MissingEyeOrigin)?,
            eyes: [dobj.find("j_eyeball_le"), dobj.find("j_eyeball_ri")],
        })
    }
}

fn rotation(matrix: Mat4) -> Result<Quat, ActorGazeError> {
    if !matrix.is_finite() {
        return Err(ActorGazeError::InvalidPose);
    }
    let (scale, rotation, _) = matrix.to_scale_rotation_translation();
    if !rotation.is_finite() || scale.abs().min_element() <= f32::EPSILON {
        return Err(ActorGazeError::InvalidPose);
    }
    Ok(rotation.normalize())
}
fn relative_frame(dobj: &DObj, bone: usize, pose: &[Mat4]) -> Result<Quat, ActorGazeError> {
    Ok(rotation(pose[bone])? * rotation(dobj.bones[bone].bind_world)?.conjugate())
}
fn angles(direction: Vec3) -> [f32; 2] {
    [
        (-direction.z)
            .atan2(direction.x.hypot(direction.y))
            .to_degrees(),
        direction.y.atan2(direction.x).to_degrees(),
    ]
}
fn turn(delta: f32) -> f32 {
    (delta + 180.).rem_euclid(360.) - 180.
}
fn aim([pitch, yaw]: [f32; 2]) -> Quat {
    Quat::from_rotation_z(yaw.to_radians()) * Quat::from_rotation_y(pitch.to_radians())
}

impl ActorGazeController {
    pub fn active(&self) -> bool {
        self.weight > 0.0
    }
    pub fn advance(
        &mut self,
        dobj: &DObj,
        locals: &mut [Local],
        target: Option<[f32; 3]>,
        at_ms: i32,
    ) -> Result<Option<ActorGazePose>, ActorGazeError> {
        let rig = Rig::find(dobj)?;
        let pose = dobj.compose(locals, Mat4::IDENTITY);
        let neck = relative_frame(dobj, rig.neck, &pose)?;
        let current = neck.conjugate() * relative_frame(dobj, rig.head, &pose)?;
        let reset = self.at_ms.is_none_or(|old| at_ms < old);
        if reset {
            self.angles = angles(current * Vec3::X);
            self.weight = 0.;
            self.limited = false;
        }
        let dt_ms = self
            .at_ms
            .filter(|_| !reset)
            .map_or(1000. / REFERENCE_HZ, |old| at_ms.saturating_sub(old) as f32);
        self.at_ms = Some(at_ms);
        let wanted = if let Some(target) = target {
            let target = Vec3::from_array(target);
            if !target.is_finite() {
                return Err(ActorGazeError::InvalidTarget);
            }
            let direction = target - pose[rig.eye].w_axis.truncate();
            if direction.length_squared() <= f32::EPSILON {
                None
            } else {
                let direction = neck.conjugate() * direction.normalize();
                let limit = if self.limited { 120_f32 } else { 135_f32 };
                self.limited = direction.x < limit.to_radians().cos();
                Some(if self.limited {
                    [0., 0.]
                } else {
                    angles(direction)
                })
            }
        } else {
            None
        };
        let response = 1. - (1. - RESPONSE_PER_REFERENCE_FRAME).powf(dt_ms * REFERENCE_HZ / 1000.);
        if let Some(wanted) = wanted {
            for (angle, wanted) in self.angles.iter_mut().zip(wanted) {
                *angle += turn(wanted - *angle) * response;
            }
        }
        self.weight += ((if wanted.is_some() { 1. } else { 0. }) - self.weight) * response;
        if self.weight < MIN_GAZE_WEIGHT {
            self.weight = 0.;
            return Ok(None);
        }
        self.apply_current(dobj, locals)
    }

    pub fn apply_current(
        &self,
        dobj: &DObj,
        locals: &mut [Local],
    ) -> Result<Option<ActorGazePose>, ActorGazeError> {
        if self.weight == 0. {
            return Ok(None);
        }
        let rig = Rig::find(dobj)?;
        let pose = dobj.compose(locals, Mat4::IDENTITY);
        let neck = relative_frame(dobj, rig.neck, &pose)?;
        let head_angles = [
            self.angles[0].clamp(-HEAD_PITCH_LIMIT, HEAD_PITCH_LIMIT),
            (self.angles[1] * HEAD_YAW_SHARE).clamp(-HEAD_YAW_LIMIT, HEAD_YAW_LIMIT),
        ];
        let controlled = neck * aim(head_angles) * rotation(dobj.bones[rig.head].bind_world)?;
        set_world_rotation(
            dobj,
            locals,
            &pose,
            rig.head,
            rotation(pose[rig.head])?.slerp(controlled, self.weight),
        )?;
        let pose = dobj.compose(locals, Mat4::IDENTITY);
        let head = relative_frame(dobj, rig.head, &pose)?;
        let residual = [
            turn(self.angles[0] - head_angles[0]).clamp(-EYE_PITCH_LIMIT, EYE_PITCH_LIMIT),
            turn(self.angles[1] - head_angles[1]).clamp(-EYE_YAW_LIMIT, EYE_YAW_LIMIT),
        ];
        let mut eyes = 0;
        for eye in rig.eyes.into_iter().flatten() {
            let controlled = head * aim(residual) * rotation(dobj.bones[eye].bind_world)?;
            set_world_rotation(
                dobj,
                locals,
                &pose,
                eye,
                rotation(pose[eye])?.slerp(controlled, self.weight),
            )?;
            eyes += 1;
        }
        Ok(Some(ActorGazePose {
            angles: self.angles,
            weight: self.weight,
            limited: self.limited,
            eyes,
        }))
    }
}

fn set_world_rotation(
    dobj: &DObj,
    locals: &mut [Local],
    pose: &[Mat4],
    bone: usize,
    wanted: Quat,
) -> Result<(), ActorGazeError> {
    let parent = dobj.bones[bone]
        .parent
        .map(|parent| rotation(pose[parent]))
        .transpose()?
        .unwrap_or(Quat::IDENTITY);
    locals[bone].rotation = (parent.conjugate() * wanted).normalize().to_array();
    locals[bone].control = false;
    Ok(())
}
