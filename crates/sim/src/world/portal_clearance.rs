//! Re-evaluate authored door routes when solid geometry changes, not per draw.
use super::SimState;
use std::hash::{Hash, Hasher};

impl SimState {
    pub(crate) fn refresh_portal_availability(&mut self) {
        let Some(atlas) = self.content.atlas() else {
            return;
        };
        // Solver flight atlases have no interactive, authored door catalog.
        if atlas.portal_thickness.len() != atlas.portals.len() {
            return;
        }
        let started = std::time::Instant::now();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        std::sync::Arc::as_ptr(&self.content).hash(&mut hash);
        for row in &self.entity_collision_capabilities {
            for brush in row.solid_brushes() {
                brush.cmodel_handle.hash(&mut hash);
                brush.origin.map(f32::to_bits).hash(&mut hash);
                brush.angles.map(f32::to_bits).hash(&mut hash);
            }
            if row.solid
                && let Some(dobj) = &row.dobj
                && let Some(capability) = &dobj.capability
                && !capability.movement_brushes.is_empty()
            {
                std::sync::Arc::as_ptr(capability).hash(&mut hash);
                dobj.world_from_model
                    .to_cols_array()
                    .map(f32::to_bits)
                    .hash(&mut hash);
            }
        }
        for piece in self.world_objects.non_solid_glass_ids() {
            piece.hash(&mut hash);
        }
        let signature = hash.finish();
        if self.portal_clearance_signature == Some(signature)
            && self.portal_clearance_job.is_none()
            && self.portal_clearance_rows == self.portal_clearance_validated
        {
            return;
        }
        if self.portal_clearance_signature != Some(signature) {
            let signatures = atlas
                .portals
                .iter()
                .zip(&atlas.portal_thickness)
                .map(|(pair, thickness)| {
                    std::array::from_fn(|side| {
                        let door = if side == 0 { pair.a } else { pair.b };
                        let mut hash = std::collections::hash_map::DefaultHasher::new();
                        std::sync::Arc::as_ptr(&self.content).hash(&mut hash);
                        // PlayerCollisionScene supplies mutable glass state only to
                        // the match world's clip; foreign atlas clips are immutable.
                        if door.world == crate::world_graph::WorldId::ROOT {
                            for brush in self.content.clip_brushes() {
                                let Some(piece) = brush.glass_encoded.checked_sub(1) else {
                                    continue;
                                };
                                if self.world_objects.glass_is_solid(u32::from(piece)) {
                                    continue;
                                }
                                if let Some((mid, half)) = plane_bounds(&brush.planes)
                                    && !near_door(
                                        door,
                                        thickness[side],
                                        mid,
                                        half,
                                        glam::Mat4::IDENTITY,
                                    )
                                {
                                    continue;
                                }
                                piece.hash(&mut hash);
                            }
                        }
                        for row in &self.entity_collision_capabilities {
                            for brush in row.solid_brushes() {
                                let Some(model) = self
                                    .content
                                    .clip_cmodels()
                                    .models
                                    .get(brush.cmodel_handle as usize)
                                else {
                                    continue;
                                };
                                let mid = glam::Vec3::from_array(std::array::from_fn(|i| {
                                    (model.mins[i] + model.maxs[i]) * 0.5
                                }));
                                let half = glam::Vec3::from_array(std::array::from_fn(|i| {
                                    (model.maxs[i] - model.mins[i]) * 0.5
                                }));
                                let pose = crate::bullet_collision::iw_angles_to_mat4(
                                    brush.origin.into(),
                                    brush.angles,
                                );
                                if !near_door(door, thickness[side], mid, half, pose) {
                                    continue;
                                }
                                0u8.hash(&mut hash);
                                brush.cmodel_handle.hash(&mut hash);
                                brush.origin.map(f32::to_bits).hash(&mut hash);
                                brush.angles.map(f32::to_bits).hash(&mut hash);
                            }
                            if row.solid
                                && let Some(dobj) = &row.dobj
                                && let Some(capability) = &dobj.capability
                                && !capability.movement_brushes.is_empty()
                            {
                                if let Some((mid, half)) =
                                    movement_bounds(&capability.movement_brushes)
                                    && !near_door(
                                        door,
                                        thickness[side],
                                        mid,
                                        half,
                                        dobj.world_from_model,
                                    )
                                {
                                    continue;
                                }
                                1u8.hash(&mut hash);
                                std::sync::Arc::as_ptr(capability).hash(&mut hash);
                                dobj.world_from_model
                                    .to_cols_array()
                                    .map(f32::to_bits)
                                    .hash(&mut hash);
                            }
                        }
                        Some(hash.finish())
                    })
                })
                .collect::<Vec<[Option<u64>; 2]>>();
            self.portal_clearance_rows = signatures;
            self.portal_clearance_signature = Some(signature);
        }
        let mut blocked = self.world_objects.portal_blocked_exits().to_vec();
        blocked.resize(atlas.portals.len(), [false; 2]);
        self.portal_clearance_validated
            .resize(atlas.portals.len(), [None; 2]);
        let mut published = 0;
        let mut stale = 0;
        if let Some(job) = &self.portal_clearance_job {
            for entry in job.0.iter() {
                if let Some(&answer) = entry.answer.get() {
                    if self.portal_clearance_rows[entry.index][entry.side] != Some(entry.signature)
                    {
                        stale += 1;
                    } else if self.portal_clearance_validated[entry.index][entry.side]
                        != Some(entry.signature)
                    {
                        self.portal_clearance_validated[entry.index][entry.side] =
                            Some(entry.signature);
                        blocked[entry.index][entry.side] = answer;
                        published += 1;
                    }
                }
            }
            if job.0.iter().all(|entry| entry.answer.get().is_some()) {
                self.portal_clearance_job = None;
                if stale > 0 {
                    diag::info!(Sim, "portal availability: stale_results={stale}");
                }
            }
        }
        if published > 0 {
            diag::info!(
                Sim,
                "portal availability: blocked_exits={blocked:?} published={published}"
            );
        }
        self.world_objects.set_portal_blocked_exits(blocked);
        if self.portal_clearance_job.is_some() {
            return;
        }
        let mut entries = Vec::new();
        for (index, (pair, thickness)) in atlas
            .portals
            .iter()
            .zip(&atlas.portal_thickness)
            .enumerate()
        {
            for (side, thickness_entry) in thickness[..2].iter().enumerate() {
                let signature = self.portal_clearance_rows[index][side];
                if signature == self.portal_clearance_validated[index][side] {
                    continue;
                }
                entries.push(ClearanceRequest {
                    index,
                    side,
                    signature: signature.expect("desired clearance signature"),
                    door: if side == 0 { pair.a } else { pair.b },
                    thickness: (*thickness_entry),
                    clearance: atlas
                        .portal_clearance
                        .get(index)
                        .copied()
                        .unwrap_or([asset_core::atlas::PortalClearance::Walk; 2])[side],
                    answer: std::sync::OnceLock::new(),
                });
            }
        }
        if !entries.is_empty() {
            let scene = self.collision_scene();
            let requested = entries.len();
            self.portal_clearance_job = ClearanceJob::spawn(entries, scene);
            diag::info!(
                Sim,
                "portal availability: requested={requested} dispatch_ms={:.3}",
                started.elapsed().as_secs_f64() * 1000.
            );
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ClearanceJob(std::sync::Arc<Vec<ClearanceRequest>>);

#[derive(Debug)]
struct ClearanceRequest {
    index: usize,
    side: usize,
    signature: u64,
    door: crate::world_graph::AtlasDoor,
    thickness: f32,
    clearance: crate::world_graph::PortalClearance,
    answer: std::sync::OnceLock<bool>,
}

impl ClearanceJob {
    fn spawn(entries: Vec<ClearanceRequest>, scene: crate::CollisionScene) -> Option<Self> {
        let entries = std::sync::Arc::new(entries);
        let work = std::sync::Arc::clone(&entries);
        match std::thread::Builder::new()
            .name("portal-clearance".into())
            .spawn(move || {
                let started = std::time::Instant::now();
                let player_scene = scene.player_collision();
                let mut checked = 0;
                for entry in work.iter() {
                    if std::sync::Arc::strong_count(&work) == 1 {
                        break;
                    }
                    let mut door = entry.door;
                    let offset = crate::world_graph::world_offset(door.world.index());
                    for (i, offset_entry) in offset[..3].iter().enumerate() {
                        door.base[i] += *offset_entry;
                    }
                    let backend = player_scene.backend(&[], u16::MAX, door.base);
                    let flight = entry.clearance == crate::world_graph::PortalClearance::Flight;
                    let validate = if flight {
                        crate::door_exit::validate_flight
                    } else {
                        crate::door_exit::validate
                    };
                    let blocked = validate(&door, entry.thickness, |start, end| {
                        movement_iw4::CollisionBackend::trace(
                            &backend,
                            movement_iw4::GroundTraceInput {
                                start,
                                end,
                                mins: if flight { [-2.; 3] } else { [-15., -15., 0.] },
                                maxs: if flight { [2.; 3] } else { [15., 15., 70.] },
                                tracemask: if flight {
                                    crate::MASK_SHOT
                                } else {
                                    crate::MASK_PLAYER_SOLID
                                },
                            },
                        )
                    })
                    .is_err();
                    let _ = entry.answer.set(blocked);
                    checked += 1;
                }
                diag::info!(
                    Sim,
                    "portal availability worker: checked={checked} elapsed_ms={:.3}",
                    started.elapsed().as_secs_f64() * 1000.
                );
            }) {
            Ok(_) => Some(Self(entries)),
            Err(error) => {
                diag::warn!(Sim, "portal clearance worker spawn failed: {error}");
                None
            }
        }
    }
}

// Conservative horizontal bound of the entire validator search plus the hull.
// Height is intentionally unbounded: a connected stair route can change floors.
fn near_door(
    door: crate::world_graph::AtlasDoor,
    thickness: f32,
    mid: glam::Vec3,
    half: glam::Vec3,
    pose: glam::Mat4,
) -> bool {
    let center = pose.transform_point3(mid);
    let half = pose.x_axis.truncate().abs() * half.x
        + pose.y_axis.truncate().abs() * half.y
        + pose.z_axis.truncate().abs() * half.z;
    if !center.is_finite() || !half.is_finite() || half.min_element() < 0. {
        return true;
    }
    let base = glam::Vec3::from_array(door.base)
        + glam::Vec3::from_array(crate::world_graph::world_offset(door.world.index()));
    let delta = center - base;
    let n = glam::Vec3::from_array(door.normal);
    let t = glam::Vec3::new(-n.y, n.x, 0.);
    n.dot(delta).abs() <= thickness * 0.5 + 80. + n.abs().dot(half)
        && t.dot(delta).abs() <= door.width * 0.5 + 64. + t.abs().dot(half)
}

// Use only bounds proven by the movement planes themselves. Visual model
// bounds need not enclose collision. Unknown/unbounded shapes stay in the key.
fn movement_bounds(
    brushes: &[xmodel_runtime::ModelMovementBrush],
) -> Option<(glam::Vec3, glam::Vec3)> {
    let mut union_lo = glam::Vec3::splat(f32::INFINITY);
    let mut union_hi = glam::Vec3::splat(f32::NEG_INFINITY);
    for brush in brushes {
        let (mid, half) = plane_bounds(&brush.planes)?;
        let lo = mid - half;
        let hi = mid + half;
        union_lo = union_lo.min(lo);
        union_hi = union_hi.max(hi);
    }
    (!brushes.is_empty()).then_some(((union_lo + union_hi) * 0.5, (union_hi - union_lo) * 0.5))
}

fn plane_bounds(planes: &[[f32; 4]]) -> Option<(glam::Vec3, glam::Vec3)> {
    let mut lo = [f32::NEG_INFINITY; 3];
    let mut hi = [f32::INFINITY; 3];
    for p in planes {
        for axis in 0..3 {
            if p[axis] == 0. || (0..3).any(|other| other != axis && p[other] != 0.) {
                continue;
            }
            let bound = p[3] / p[axis];
            if !bound.is_finite() {
                return None;
            }
            if p[axis] > 0. {
                hi[axis] = hi[axis].min(bound);
            } else {
                lo[axis] = lo[axis].max(bound);
            }
        }
    }
    let lo = glam::Vec3::from_array(lo);
    let hi = glam::Vec3::from_array(hi);
    if !lo.is_finite() || !hi.is_finite() || lo.cmpgt(hi).any() {
        return None;
    }
    Some(((lo + hi) * 0.5, (hi - lo) * 0.5))
}
