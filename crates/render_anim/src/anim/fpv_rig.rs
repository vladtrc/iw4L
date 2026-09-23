//! The prepared first-person rig.
//!
//! The rig owns the whole first-person plan. Both hands of a dual-wield
//! composition are slots in one layout, so the left gun is a second destination
//! range over the same prepared model rather than a second pass over the same
//! decisions.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::math::{Mat4, Vec3};
use render_scene::SmodelPassMaterial;

use crate::anim::fpv_pose::{
    FpvBoltFrame, FpvBoltTags, PosedClip, tag_camera_lens_local, tag_view_to_bevy_camera,
};
use crate::anim::xmodel_pose::{FpvSurfOwner, SkinLayout, build_skin_layout, skin_packed_into};
use crate::draw::FpvSurfaceDraw;
use anim_iw4::dobj_surface_hidden;
use assets::{
    AnimInstance, FpvAssembly, FpvClipTracks, FpvMeshCatalog, FpvPartRole, FpvSkel, PartBits,
};

/// One model inside the combined skeleton: where its bones start, and how its
/// surfaces map onto a contiguous run of destination vertices.
struct PreparedModel {
    catalog_entry: usize,
    owner: FpvSurfOwner,
    bone_base: usize,
    posed_surface_n: usize,
    layout: SkinLayout,
}

/// One model drawn for one hand. The same prepared model appears twice in a
/// dual-wield rig — once per hand — with a destination range of its own.
struct PlanSlot {
    hand: usize,
    model: usize,
    dest_base: usize,
}

/// The rows the plan publishes for as long as the composition holds.
pub struct PreparedFpvGeometry {
    pub indices: Vec<u32>,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub draws: Vec<FpvSurfaceDraw>,
    pub dest_n: usize,
    pub packed_ok: bool,

    pub hands_plan_n: u32,
    pub gun_plan_n: u32,
    pub scope_plan_n: u32,
    pub scope_house_plan_n: u32,
    pub scope_lens_plan_n: u32,
    pub plan_draw_n: u32,
    pub plan_skip_n: u32,
}

pub struct FpvHandPose {
    skin: Vec<Mat4>,
    eye_from_world: Mat4,
    offset: Vec3,
    pub lens: Mat4,
    pub bolt: FpvBoltFrame,
}

pub struct FpvRigInputs<'a> {
    pub catalog: &'a FpvMeshCatalog,
    pub materials: &'a [SmodelPassMaterial],
    pub material_by_authored: &'a HashMap<usize, u32>,
    pub clip_tracks: &'a FpvClipTracks,
    pub clip_orders: [&'a [Option<usize>]; 2],
}

#[derive(Debug)]
pub enum FpvRigError {
    Catalog(&'static str),
    Layout(&'static str),
}

impl core::fmt::Display for FpvRigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Catalog(what) => write!(f, "{what} missing from the first-person catalog"),
            Self::Layout(what) => write!(f, "{what}"),
        }
    }
}

pub struct PreparedFpvRig {
    assembly: Arc<FpvAssembly>,
    dual: bool,
    generation: u64,
    parts: PartBits,
    tags: FpvBoltTags,
    models: Vec<PreparedModel>,
    slots: Vec<PlanSlot>,
    orders: [Vec<Option<usize>>; 2],
    tracks: [Vec<Option<Vec<Option<usize>>>>; 2],
    pub geometry: PreparedFpvGeometry,
}

pub fn leftover_scope_surf_is_lens(name: &str) -> bool {
    name.contains("lens")
}

/// Which surfaces of one model reach the plan: not hidden by this weapon's hide
/// tags, and carrying an authored material the session admitted.
fn surface_admitted(
    skel: &FpvSkel,
    words: Option<&[u32; 6]>,
    admitted_materials: &HashMap<usize, u32>,
    surface_index: usize,
) -> bool {
    if let Some(words) = words
        && let Some(bits) = skel.surface_part_bits.get(surface_index)
        && dobj_surface_hidden(bits, words, 0)
    {
        return false;
    }
    let Some(authored) = skel
        .surface_materials
        .get(surface_index)
        .copied()
        .flatten()
        .map(|index| index.get())
    else {
        return false;
    };
    admitted_materials.contains_key(&authored)
}

fn surf_owner(role: FpvPartRole) -> FpvSurfOwner {
    match role {
        FpvPartRole::Hands => FpvSurfOwner::Hands,
        FpvPartRole::Gun => FpvSurfOwner::Gun,
        FpvPartRole::Attachment => FpvSurfOwner::Scope,
        FpvPartRole::Rocket => FpvSurfOwner::Rocket,
    }
}

impl PreparedFpvRig {
    /// A plan holding this generation is a plan whose indices, ranges,
    /// materials and draws are this rig's.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn matches(
        &self,
        assembly: &Arc<FpvAssembly>,
        dual: bool,
        orders: [&[Option<usize>]; 2],
    ) -> bool {
        Arc::ptr_eq(&self.assembly, assembly)
            && self.dual == dual
            && self.orders[0] == orders[0]
            && self.orders[1] == orders[1]
    }

    pub fn build(
        assembly: Arc<FpvAssembly>,
        dual: bool,
        inputs: FpvRigInputs<'_>,
    ) -> Result<Self, FpvRigError> {
        let catalog = inputs.catalog;
        let skel_of = |index: usize| catalog.get_at(index).map(|entry| &entry.skel);

        let mut models: Vec<PreparedModel> = Vec::with_capacity(assembly.parts.len());
        let mut packed_ok = true;
        for part in &assembly.parts {
            let index = part.model.order();
            let skel = skel_of(index).ok_or(FpvRigError::Catalog("model"))?;
            let lod_range = skel.surfaces_for_lod(0);
            let first_surface = lod_range.start;
            let layout = build_skin_layout(skel, 0, |lod_local| {
                surface_admitted(
                    skel,
                    part.hide.as_ref(),
                    inputs.material_by_authored,
                    first_surface + lod_local,
                )
            })
            .ok_or(FpvRigError::Layout("skin layout refused the model"))?;
            if skel.packed_vertices.len() != skel.positions.len() {
                packed_ok = false;
            }
            models.push(PreparedModel {
                catalog_entry: index,
                owner: surf_owner(part.role),
                bone_base: part.bone_base,
                posed_surface_n: lod_range.len(),
                layout,
            });
        }

        // Every hand's view hands first, then every hand's gun and whatever
        // hangs off it.
        let hand_n = if dual { 2 } else { 1 };
        let mut order: Vec<(usize, usize)> = Vec::new();
        for hand in 0..hand_n {
            order.push((hand, 0));
        }
        for hand in 0..hand_n {
            for model in 1..models.len() {
                // The scope and the rocket hang off the right gun only.
                if hand == 1 && model != 1 {
                    continue;
                }
                order.push((hand, model));
            }
        }

        let mut geometry = PreparedFpvGeometry {
            indices: Vec::new(),
            surface_ranges: Vec::new(),
            materials: Vec::new(),
            draws: Vec::new(),
            dest_n: 0,
            packed_ok,
            hands_plan_n: 0,
            gun_plan_n: 0,
            scope_plan_n: 0,
            scope_house_plan_n: 0,
            scope_lens_plan_n: 0,
            plan_draw_n: 0,
            plan_skip_n: 0,
        };
        let mut slots: Vec<PlanSlot> = Vec::new();
        let mut material_key: HashMap<usize, u32> = HashMap::new();
        let mut dest_base = 0usize;
        let mut posed_surface_n = 0usize;
        for (hand, model_index) in order {
            let model = &models[model_index];
            let skel = skel_of(model.catalog_entry).ok_or(FpvRigError::Catalog("model"))?;
            let entry = catalog
                .get_at(model.catalog_entry)
                .ok_or(FpvRigError::Catalog("model"))?;
            posed_surface_n += model.posed_surface_n;
            for surface in &model.layout.surfaces {
                if !surface.visible || surface.index_count == 0 {
                    continue;
                }
                let Some(authored) = skel
                    .surface_materials
                    .get(surface.surface_index)
                    .copied()
                    .flatten()
                    .map(|index| index.get())
                else {
                    continue;
                };
                let Some(&session_index) = inputs.material_by_authored.get(&authored) else {
                    continue;
                };
                let Some(material) = inputs.materials.get(session_index as usize).cloned() else {
                    continue;
                };
                let is_scope = model.owner == FpvSurfOwner::Scope;
                let is_lens = is_scope
                    && entry
                        .material_names
                        .get(surface.surface_index)
                        .and_then(|name| name.as_deref())
                        .is_some_and(leftover_scope_surf_is_lens);

                let index_start = geometry.indices.len() as u32;
                let src = surface.index_start as usize;
                let end = src.saturating_add(surface.index_count as usize);
                let rebase = dest_base as u32;
                geometry.indices.extend(
                    model.layout.indices[src..end]
                        .iter()
                        .map(|index| index + rebase),
                );
                let range_index = geometry.surface_ranges.len() as u32;
                geometry
                    .surface_ranges
                    .push((index_start, surface.index_count));
                let material_index = *material_key.entry(authored).or_insert_with(|| {
                    let index = geometry.materials.len() as u32;
                    geometry.materials.push(material);
                    index
                });
                geometry.draws.push(FpvSurfaceDraw {
                    surface: range_index,
                    material: material_index,
                    is_scope: is_scope && !is_lens,
                });
                if hand == 0 {
                    match model.owner {
                        FpvSurfOwner::Hands => geometry.hands_plan_n += 1,
                        _ => geometry.gun_plan_n += 1,
                    }
                }
                if is_scope {
                    geometry.scope_plan_n += 1;
                    if is_lens {
                        geometry.scope_lens_plan_n += 1;
                    } else {
                        geometry.scope_house_plan_n += 1;
                    }
                }
            }
            slots.push(PlanSlot {
                hand,
                model: model_index,
                dest_base,
            });
            dest_base += model.layout.dest_vertex_n;
        }
        geometry.dest_n = dest_base;
        geometry.plan_draw_n = geometry.draws.len() as u32;
        geometry.plan_skip_n = posed_surface_n.saturating_sub(geometry.draws.len()) as u32;

        let orders = inputs.clip_orders.map(<[Option<usize>]>::to_vec);
        let tracks = [0, 1].map(|hand| {
            orders[hand]
                .iter()
                .map(|order| assembly.compose_tracks((*order)?, inputs.clip_tracks))
                .collect()
        });
        let tags = FpvBoltTags {
            flash: assembly.tags.flash,
            flash_silenced: assembly.tags.flash_silenced,
            brass: assembly.tags.brass,
            knife: assembly.tags.knife,
            laser: assembly.tags.laser,
        };

        static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let parts = assembly.dobj.all_parts();
        Ok(Self {
            assembly,
            dual,
            generation: GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            parts,
            tags,
            models,
            slots,
            orders,
            tracks,
            geometry,
        })
    }

    /// One hand's bones for this frame. No skeleton is built here.
    pub fn pose_hand(
        &mut self,
        hand: usize,
        anims: &[PosedClip<'_>],
        offset: Vec3,
    ) -> Option<FpvHandPose> {
        if anims.is_empty() {
            return None;
        }
        let instances: Vec<AnimInstance<'_>> = anims
            .iter()
            .filter_map(|anim| {
                let tracks = self.tracks[hand].get(anim.node)?.as_ref()?;
                Some(AnimInstance {
                    clip: anim.clip,
                    tracks,
                    time: anim.time,
                    weight: anim.weight,
                    parts: None,
                })
            })
            .collect();
        if !instances
            .iter()
            .any(|instance| instance.tracks.iter().any(Option::is_some))
        {
            return None;
        }
        let world = self
            .assembly
            .dobj
            .pose(&instances, &self.parts, Mat4::IDENTITY);
        let skin = self.assembly.dobj.skin_matrices(&world);
        let eye_from_world = tag_view_to_bevy_camera() * world[self.assembly.view_bone].inverse();
        let lens = self
            .assembly
            .camera_bone
            .map(|camera| tag_camera_lens_local(world[self.assembly.view_bone], world[camera]))
            .unwrap_or(Mat4::IDENTITY);
        let published = if hand == 0 {
            world.len()
        } else {
            self.assembly.paired_bones.min(world.len())
        };
        let shift = (offset != Vec3::ZERO).then(|| Mat4::from_translation(offset));
        let bones = world[..published]
            .iter()
            .map(|bone| {
                let bone = eye_from_world * *bone;
                match shift {
                    Some(shift) => shift * bone,
                    None => bone,
                }
            })
            .collect();
        Some(FpvHandPose {
            skin,
            eye_from_world,
            offset,
            lens,
            bolt: FpvBoltFrame {
                bones,
                tags: self.tags,
            },
        })
    }

    /// Skin every slot into the destination buffer the plan published. A vertex
    /// update only: indices, surface ranges and materials are untouched.
    pub fn skin_into(
        &self,
        catalog: &FpvMeshCatalog,
        poses: &[Option<FpvHandPose>; 2],
        dest: &mut [[u8; asset_iw4::size::GFX_PACKED_VERTEX]],
    ) {
        for slot in &self.slots {
            let Some(pose) = poses[slot.hand].as_ref() else {
                continue;
            };
            let model = &self.models[slot.model];
            let Some(skel) = catalog.get_at(model.catalog_entry).map(|entry| &entry.skel) else {
                continue;
            };
            let end = slot.dest_base.saturating_add(model.layout.dest_vertex_n);
            if end > dest.len() {
                continue;
            }
            let rows = &mut dest[slot.dest_base..end];
            skin_packed_into(
                skel,
                |bone| pose.eye_from_world * pose.skin[model.bone_base + bone],
                |_| false,
                0,
                &model.layout,
                rows,
            );
            if pose.offset != Vec3::ZERO {
                translate_packed_rows(rows, pose.offset);
            }
        }
    }
}

/// A dual-wield left hand is the right hand's pose moved sideways, the offset
/// landing on the posed position.
fn translate_packed_rows(rows: &mut [[u8; asset_iw4::size::GFX_PACKED_VERTEX]], delta: Vec3) {
    for row in rows {
        let x = f32::from_le_bytes([row[0], row[1], row[2], row[3]]) + delta.x;
        let y = f32::from_le_bytes([row[4], row[5], row[6], row[7]]) + delta.y;
        let z = f32::from_le_bytes([row[8], row[9], row[10], row[11]]) + delta.z;
        row[0..4].copy_from_slice(&x.to_le_bytes());
        row[4..8].copy_from_slice(&y.to_le_bytes());
        row[8..12].copy_from_slice(&z.to_le_bytes());
    }
}
