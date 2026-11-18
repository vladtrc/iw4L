use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use bevy::prelude::*;
use replay::{Playback, Recording, SeekIndex};
use serde::{Deserialize, Serialize};
use sim::{ClientId, SimEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CameraMode {
    FirstPerson,
    Chase,
    Free,
    Dolly,
    Attached,
}

impl CameraMode {
    pub fn next(self) -> Self {
        match self {
            Self::FirstPerson => Self::Chase,
            Self::Chase => Self::Free,
            _ => Self::FirstPerson,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraKey {
    pub frame: u64,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
    pub fov: f32,
    #[serde(default)]
    pub world: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub source_frames: u64,
    pub source_sha256: String,
    pub segments: Vec<Segment>,
    pub cameras: Vec<CameraKey>,
    pub bookmarks: Vec<u64>,
    pub player: u32,
    pub camera_mode: CameraMode,
    #[serde(default)]
    pub attached_entity: Option<i32>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: 1,
            source_frames: 0,
            source_sha256: String::new(),
            segments: Vec::new(),
            cameras: Vec::new(),
            bookmarks: Vec::new(),
            player: 0,
            camera_mode: CameraMode::FirstPerson,
            attached_entity: None,
        }
    }
}

impl Project {
    pub fn validate(&self, frames: u64, source_sha256: &str) -> Result<(), String> {
        if self.version != 1
            || frames == 0
            || self.source_frames != frames
            || self.source_sha256 != source_sha256
            || source_sha256.len() != 64
        {
            return Err("project version or recording length does not match".into());
        }
        if self
            .segments
            .iter()
            .any(|s| s.start >= s.end || s.end > frames)
            || self.bookmarks.iter().any(|&f| f >= frames)
            || self.cameras.iter().any(|c| {
                c.frame >= frames
                    || !c
                        .origin
                        .iter()
                        .chain(c.rotation.iter())
                        .all(|v| v.is_finite())
                    || !(10.0..=150.0).contains(&c.fov)
                    || (Quat::from_array(c.rotation).length_squared() - 1.0).abs() > 0.01
            })
            || self
                .cameras
                .windows(2)
                .any(|pair| pair[0].frame >= pair[1].frame)
        {
            return Err("project contains invalid ranges or camera keys".into());
        }
        Ok(())
    }

    pub fn add_camera(&mut self, key: CameraKey) {
        self.cameras.retain(|c| c.frame != key.frame);
        self.cameras.push(key);
        self.cameras.sort_by_key(|c| c.frame);
    }

    pub fn camera_at(&self, frame: f64) -> Option<(Transform, f32)> {
        self.camera_at_mapped(frame, |key, _| Some(key))
    }

    pub fn camera_at_in_atlas(
        &self,
        frame: f64,
        atlas: &sim::world_graph::Atlas,
    ) -> Option<(Transform, f32)> {
        self.camera_at_mapped(frame, |mut key, target| {
            let (Some(source), Some(target)) = (key.world, target) else {
                return Some(key);
            };
            if source == target {
                return Some(key);
            }
            let mut links = atlas.portals.iter().filter_map(|pair| {
                if pair.a.world.index() == source && pair.b.world.index() == target {
                    Some(pair.oriented(true))
                } else if pair.b.world.index() == source && pair.a.world.index() == target {
                    Some(pair.oriented(false))
                } else {
                    None
                }
            });
            let (entry, exit, yaw) = links.next()?;
            // Different doors between the same worlds require an explicit route;
            // selecting an arbitrary portal would produce an unrelated camera.
            if links.next().is_some() {
                return None;
            }
            let from = atlas.space.frame(entry.world).ok()?;
            let to = atlas.space.frame(exit.world).ok()?;
            let rotation = Quat::from_rotation_z(yaw);
            let local = Vec3::from_array(from.point_to_local(key.origin));
            let mapped =
                rotation * (local - Vec3::from_array(entry.base)) + Vec3::from_array(exit.base);
            key.origin = to.point_to_scene(mapped.to_array());
            let basis = |world: sim::world_graph::WorldId| {
                let axes = [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| {
                    Vec3::from_array(atlas.space.vector_to_scene(world, axis.to_array()))
                });
                Quat::from_mat3(&Mat3::from_cols(axes[0], axes[1], axes[2]))
            };
            key.rotation = (basis(exit.world)
                * rotation
                * basis(entry.world).inverse()
                * Quat::from_array(key.rotation))
            .to_array();
            key.world = Some(target);
            Some(key)
        })
    }

    fn camera_at_mapped(
        &self,
        frame: f64,
        map: impl Fn(CameraKey, Option<usize>) -> Option<CameraKey>,
    ) -> Option<(Transform, f32)> {
        let first = self.cameras.first()?;
        let next = self
            .cameras
            .partition_point(|key| (key.frame as f64) <= frame);
        let (a, b) = if next == 0 {
            (first, first)
        } else if next == self.cameras.len() {
            let last = self.cameras.last()?;
            (last, last)
        } else {
            (&self.cameras[next - 1], &self.cameras[next])
        };
        let world = a.world;
        let a = map(*a, world)?;
        let b = map(*b, world)?;
        let t = if a.frame == b.frame {
            0.0
        } else {
            ((frame - a.frame as f64) / (b.frame - a.frame) as f64).clamp(0.0, 1.0) as f32
        };
        let origin = if a.frame == b.frame {
            Vec3::from_array(a.origin)
        } else {
            let before = map(self.cameras[next.saturating_sub(2)], world)?;
            let after = map(self.cameras[(next + 1).min(self.cameras.len() - 1)], world)?;
            let p0 = Vec3::from_array(before.origin);
            let p1 = Vec3::from_array(a.origin);
            let p2 = Vec3::from_array(b.origin);
            let p3 = Vec3::from_array(after.origin);
            let tangent1 = (p2 - p0) * 0.5;
            let tangent2 = (p3 - p1) * 0.5;
            let t2 = t * t;
            let t3 = t2 * t;
            p1 * (2.0 * t3 - 3.0 * t2 + 1.0)
                + tangent1 * (t3 - 2.0 * t2 + t)
                + p2 * (-2.0 * t3 + 3.0 * t2)
                + tangent2 * (t3 - t2)
        };
        Some((
            Transform {
                translation: origin,
                rotation: Quat::from_array(a.rotation).slerp(Quat::from_array(b.rotation), t),
                ..default()
            },
            a.fov + (b.fov - a.fov) * t,
        ))
    }

    /// Ownership changes at the authored crossing key while the position path
    /// stays continuous in scene space, including overlapping portal worlds.
    pub fn camera_world_at(&self, frame: f64) -> Option<usize> {
        self.cameras
            .iter()
            .rev()
            .find(|key| key.frame as f64 <= frame)
            .or_else(|| self.cameras.first())
            .and_then(|key| key.world)
    }

    pub fn highlights(&mut self, markers: &[TimelineEvent], player: u32, frames: u64) {
        let before = 3000 / net::AUTHORITY_MS as u64;
        let after = 2000 / net::AUTHORITY_MS as u64;
        let mut segments: Vec<Segment> = Vec::new();
        for marker in markers
            .iter()
            .filter(|m| m.attacker == Some(player) && m.victim != player)
        {
            let range = Segment {
                start: marker.frame.saturating_sub(before),
                end: (marker.frame + after + 1).min(frames),
            };
            if let Some(last) = segments.last_mut()
                && range.start <= last.end
            {
                last.end = last.end.max(range.end);
            } else {
                segments.push(range);
            }
        }
        self.segments = segments;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TimelineEvent {
    pub frame: u64,
    pub victim: u32,
    pub attacker: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct FilmInfo {
    pub frames: u64,
    pub source_sha256: String,
    pub players: Vec<u32>,
    pub killcam_viewer: Option<u32>,
    pub events: Vec<TimelineEvent>,
    pub seek: Arc<SeekIndex>,
}

pub fn inspect(path: &Path) -> Result<FilmInfo, String> {
    let mut playback = Playback::open_path(path).map_err(|e| e.to_string())?;
    let mut info = FilmInfo::default();
    let mut players = BTreeSet::new();
    let mut events = BTreeSet::new();
    let mut seek = SeekIndex::default();
    while let Some(tick) = playback.next_tick().map_err(|e| e.to_string())? {
        seek.note(&playback);
        if info.killcam_viewer.is_none() {
            info.killcam_viewer = tick
                .snapshot
                .meta
                .clients
                .iter()
                .find_map(|(id, meta)| meta.killcam_hud.as_ref().map(|_| id.0));
        }
        for (ClientId(id), _) in &tick.snapshot.players {
            players.insert(*id);
        }
        for record in &tick.snapshot.meta.journal {
            if let SimEvent::Died {
                victim, attacker, ..
            } = record.event
                && events.insert(record.sequence.0)
            {
                info.events.push(TimelineEvent {
                    frame: info.frames,
                    victim: victim.0,
                    attacker: attacker.map(|id| id.0),
                });
            }
        }
        info.frames += 1;
    }
    info.players = players.into_iter().collect();
    info.seek = Arc::new(seek);
    info.source_sha256 = source_digest(path)?;
    Ok(info)
}

pub fn export(source: &Path, destination: &Path, project: &Project) -> Result<u64, String> {
    project.validate(project.source_frames, &source_digest(source)?)?;
    if project.segments.len() != 1 {
        return Err("export one segment at a time".into());
    }
    if destination == source || destination.exists() {
        return Err("export destination already exists".into());
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temporary = destination.with_extension("iw4ldemo.part");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    drop(file);
    let result = (|| {
        let identity = Playback::open_path(source)
            .map_err(|e| e.to_string())?
            .identity();
        let mut output =
            Recording::start_at_path(temporary.clone(), identity).map_err(|e| e.to_string())?;
        let mut count = 0u64;
        for segment in &project.segments {
            let mut input = Playback::open_path(source).map_err(|e| e.to_string())?;
            input
                .position_before(segment.start)
                .map_err(|e| e.to_string())?;
            for _ in segment.start..segment.end {
                let played = input
                    .next_tick()
                    .map_err(|e| e.to_string())?
                    .ok_or("recording ended inside a segment")?;
                output
                    .record_frame(&played.snapshot, &played.frame)
                    .map_err(|e| e.to_string())?;
                count += 1;
            }
        }
        output.stop().map_err(|e| e.to_string())?;
        std::fs::hard_link(&temporary, destination).map_err(|e| e.to_string())?;
        std::fs::remove_file(&temporary).map_err(|e| e.to_string())?;
        Ok(count)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn source_digest(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
