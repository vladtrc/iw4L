//! The prepared first-person rig.
//!
//! Everything about the equipped composition that a pose does not change — the
//! combined skeleton, the clip bindings, which surfaces are drawn, and where
//! their vertices and indices sit in the published plan — is decided when that
//! composition changes and then left alone. An ordinary frame evaluates bones
//! and skins into the layout prepared here; it rediscovers none of it.
//!
//! The rig owns the whole first-person plan. Both hands of a dual-wield
//! composition are slots in one layout, so the left gun is a second destination
//! range over the same prepared model rather than a second pass over the same
//! decisions.

use std::collections::HashMap;

use bevy::math::{Mat4, Vec3};
use render_scene::SmodelPassMaterial;

use crate::anim::fpv_pose::{
    FpvBoltFrame, FpvBoltTags, PosedClip, tag_camera_lens_local, tag_view_to_bevy_camera,
};
use crate::anim::xmodel_pose::{FpvSurfOwner, SkinLayout, build_skin_layout, skin_packed_into};
use crate::draw::FpvSurfaceDraw;
use anim_iw4::{dobj_surface_hidden, set_hide_part_bit};
use assets::{
    AnimClip, AnimInstance, AssetNamespace, Attach, DObj, DObjError, FpvHands, FpvMeshCatalog,
    FpvSkel, ModelPoseSrc, PartBits,
};

/// The scope attach tags, most specific first. A scope whose own root bone
/// names a gun bone wins over any of them.
const SCOPE_ATTACH_TAGS: &[&str] = &[
    "tag_scope",
    "tag_acog",
    "tag_red_dot",
    "tag_reflex",
    "tag_hybrid",
    "tag_thermal",
    "tag_thermal_scope",
    "tag_eotech",
];

const ROCKET_ATTACH_TAG: &str = "tag_clip";

/// A frame whose composition still answers this key reuses the rig below it;
/// anything else rebuilds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FpvRigKey {
    pub namespace: AssetNamespace,
    pub gun: String,
    pub hands: FpvHands,
    pub scope: Option<String>,
    pub rocket: Option<String>,
    pub hide_tags: Vec<String>,
    pub dual: bool,
}

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

/// A clip bound to this rig's bones. The address identifies the clip the
/// scheduler node held when the binding was made — the node owns an `Arc` of
/// it, so a node still holding that clip is a node whose binding still stands.
/// The address is compared, never dereferenced.
struct TrackBinding {
    clip: usize,
    tracks: Vec<Option<usize>>,
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

/// The material table is the session's: a surface whose authored material never
/// made it in is not drawn, and the rig settles that once.
pub struct FpvRigInputs<'a> {
    pub catalog: &'a FpvMeshCatalog,
    pub materials: &'a [SmodelPassMaterial],
    pub material_by_authored: &'a HashMap<usize, u32>,
}

#[derive(Debug)]
pub enum FpvRigError {
    Catalog(&'static str),
    Skeleton(DObjError),
    NoTagView,
    Layout(&'static str),
}

impl core::fmt::Display for FpvRigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Catalog(what) => write!(f, "{what} missing from the first-person catalog"),
            Self::Skeleton(error) => write!(f, "combined skeleton: {error}"),
            Self::NoTagView => write!(f, "combined skeleton has no tag_view"),
            Self::Layout(what) => write!(f, "{what}"),
        }
    }
}

pub struct PreparedFpvRig {
    key: FpvRigKey,
    generation: u64,
    dobj: DObj,
    parts: PartBits,
    view_bone: usize,
    camera_bone: Option<usize>,
    /// Bones of hands+gun. A dual-wield left hand publishes exactly those: the
    /// scope and the rocket hang off the right gun, and the left never had them.
    paired_bones: usize,
    tags: FpvBoltTags,
    models: Vec<PreparedModel>,
    slots: Vec<PlanSlot>,
    tracks: [Vec<Option<TrackBinding>>; 2],
    pub geometry: PreparedFpvGeometry,
}

fn scope_attach_tag_name<'a>(gun_bones: &'a [String], scope_bones: &[String]) -> Option<&'a str> {
    if let Some(root) = scope_bones.first()
        && let Some(hit) = gun_bones
            .iter()
            .find(|name| name.eq_ignore_ascii_case(root))
    {
        return Some(hit.as_str());
    }
    for tag in SCOPE_ATTACH_TAGS {
        let on_scope = scope_bones
            .iter()
            .any(|name| name.eq_ignore_ascii_case(tag));
        if !on_scope {
            continue;
        }
        if let Some(hit) = gun_bones.iter().find(|name| name.eq_ignore_ascii_case(tag)) {
            return Some(hit.as_str());
        }
    }
    gun_bones.iter().find_map(|name| {
        SCOPE_ATTACH_TAGS
            .iter()
            .copied()
            .find(|tag| name.eq_ignore_ascii_case(tag))
            .map(|_| name.as_str())
    })
}

fn bolt_tag_bone(dobj: &DObj, tag: &str) -> Option<u16> {
    dobj.find(tag).and_then(|index| u16::try_from(index).ok())
}

pub fn leftover_scope_surf_is_lens(name: &str) -> bool {
    name.contains("lens")
}

/// The part bits a hide-tag set hides, or `None` when this model carries no
/// per-surface part bits to test them against.
fn hide_words(skel: &FpvSkel, hide_tags: &[String]) -> Option<[u32; 6]> {
    if hide_tags.is_empty() || skel.surface_part_bits.len() != skel.surface_vertex_ranges.len() {
        return None;
    }
    let mut words = [0u32; 6];
    for bone in 0..skel.bone_names.len() {
        if assets::bone_has_hidden_ancestor(
            &skel.bone_names,
            |b| skel.parent_of(b),
            bone,
            hide_tags,
        ) {
            set_hide_part_bit(&mut words, bone);
        }
    }
    Some(words)
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

impl PreparedFpvRig {
    pub fn key(&self) -> &FpvRigKey {
        &self.key
    }

    /// A plan holding this generation is a plan whose indices, ranges,
    /// materials and draws are this rig's.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Compares what the caller already holds instead of building a key to
    /// throw away.
    #[allow(clippy::too_many_arguments)]
    pub fn matches(
        &self,
        namespace: AssetNamespace,
        gun: &str,
        hands: &FpvHands,
        scope: Option<&str>,
        rocket: Option<&str>,
        hide_tags: &[String],
        dual: bool,
    ) -> bool {
        self.key.namespace == namespace
            && self.key.gun == gun
            && &self.key.hands == hands
            && self.key.scope.as_deref() == scope
            && self.key.rocket.as_deref() == rocket
            && self.key.dual == dual
            && self.key.hide_tags == hide_tags
    }

    pub fn build(key: FpvRigKey, inputs: FpvRigInputs<'_>) -> Result<Self, FpvRigError> {
        let catalog = inputs.catalog;
        let ns = key.namespace;
        let (hands_ns, hands_name) = key.hands.key().ok_or(FpvRigError::Catalog("view hands"))?;
        let hands_i = catalog
            .index_by_name(hands_ns, hands_name)
            .ok_or(FpvRigError::Catalog("view hands"))?;
        let gun_i = catalog
            .index_by_name(ns, &key.gun)
            .ok_or(FpvRigError::Catalog("gun model"))?;
        let skel_of = |index: usize| catalog.get_at(index).map(|entry| &entry.skel);
        let pose_of = |index: usize| skel_of(index).and_then(|skel| skel.pose.as_ref());

        let gun_skel = skel_of(gun_i).ok_or(FpvRigError::Catalog("gun model"))?;
        let hands_pose = pose_of(hands_i).ok_or(FpvRigError::Catalog("view hands pose"))?;
        let gun_pose = pose_of(gun_i).ok_or(FpvRigError::Catalog("gun model pose"))?;

        // An attachment whose tag the gun does not carry is dropped here
        // instead of being re-attempted, and re-refused, on every pose.
        let mut specs: Vec<(&ModelPoseSrc, Option<Attach>)> = vec![
            (hands_pose, None),
            (
                gun_pose,
                Some(Attach {
                    parent_model: 0,
                    tag: "tag_weapon".into(),
                }),
            ),
        ];
        let mut dobj = DObj::build(&specs).map_err(FpvRigError::Skeleton)?;
        let paired_bones = dobj.bone_count();
        let mut attached: Vec<(usize, FpvSurfOwner)> = Vec::new();

        if let Some(index) = key
            .scope
            .as_deref()
            .and_then(|name| catalog.index_by_name(ns, name))
            && let Some(skel) = skel_of(index)
            && let Some(pose) = pose_of(index)
            && let Some(tag) = scope_attach_tag_name(&gun_skel.bone_names, &skel.bone_names)
        {
            specs.push((
                pose,
                Some(Attach {
                    parent_model: 1,
                    tag: tag.to_owned(),
                }),
            ));
            match DObj::build(&specs) {
                Ok(next) => {
                    dobj = next;
                    attached.push((index, FpvSurfOwner::Scope));
                }
                Err(DObjError::AttachTag { .. }) => {
                    specs.pop();
                }
                Err(error) => return Err(FpvRigError::Skeleton(error)),
            }
        }

        if let Some(index) = key
            .rocket
            .as_deref()
            .and_then(|name| catalog.index_by_name(ns, name))
            && let Some(pose) = pose_of(index)
            && gun_skel
                .bone_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(ROCKET_ATTACH_TAG))
        {
            specs.push((
                pose,
                Some(Attach {
                    parent_model: 1,
                    tag: ROCKET_ATTACH_TAG.into(),
                }),
            ));
            match DObj::build(&specs) {
                Ok(next) => {
                    dobj = next;
                    attached.push((index, FpvSurfOwner::Rocket));
                }
                Err(DObjError::AttachTag { .. }) => {
                    specs.pop();
                }
                Err(error) => return Err(FpvRigError::Skeleton(error)),
            }
        }

        let view_bone = dobj.find("tag_view").ok_or(FpvRigError::NoTagView)?;
        let camera_bone = dobj.find("tag_camera");
        let tags = FpvBoltTags {
            flash: bolt_tag_bone(&dobj, "tag_flash"),
            flash_silenced: bolt_tag_bone(&dobj, "tag_flash_silenced"),
            brass: bolt_tag_bone(&dobj, "tag_brass"),
            knife: bolt_tag_bone(&dobj, "tag_knife_fx"),
            laser: bolt_tag_bone(&dobj, fx_iw4::FX_LASER_TAG),
        };

        // Hide tags reach the gun only.
        let mut models: Vec<PreparedModel> = Vec::new();
        let mut bone_base = 0usize;
        let mut packed_ok = true;
        let prepare_model = |index: usize,
                             owner: FpvSurfOwner,
                             bone_base: usize,
                             hide: &[String],
                             models: &mut Vec<PreparedModel>,
                             packed_ok: &mut bool|
         -> Result<usize, FpvRigError> {
            let skel = skel_of(index).ok_or(FpvRigError::Catalog("model"))?;
            let words = hide_words(skel, hide);
            let lod_range = skel.surfaces_for_lod(0);
            let first_surface = lod_range.start;
            let layout = build_skin_layout(skel, 0, |lod_local| {
                surface_admitted(
                    skel,
                    words.as_ref(),
                    inputs.material_by_authored,
                    first_surface + lod_local,
                )
            })
            .ok_or(FpvRigError::Layout("skin layout refused the model"))?;
            if skel.packed_vertices.len() != skel.positions.len() {
                *packed_ok = false;
            }
            models.push(PreparedModel {
                catalog_entry: index,
                owner,
                bone_base,
                posed_surface_n: lod_range.len(),
                layout,
            });
            Ok(pose_of(index).map(|pose| pose.num_bones).unwrap_or(0))
        };

        bone_base += prepare_model(
            hands_i,
            FpvSurfOwner::Hands,
            bone_base,
            &[],
            &mut models,
            &mut packed_ok,
        )?;
        bone_base += prepare_model(
            gun_i,
            FpvSurfOwner::Gun,
            bone_base,
            &key.hide_tags,
            &mut models,
            &mut packed_ok,
        )?;
        for (index, owner) in attached {
            bone_base += prepare_model(index, owner, bone_base, &[], &mut models, &mut packed_ok)?;
        }

        // Every hand's view hands first, then every hand's gun and whatever
        // hangs off it.
        let hand_n = if key.dual { 2 } else { 1 };
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

        static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let parts = dobj.all_parts();
        Ok(Self {
            key,
            generation: GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            dobj,
            parts,
            view_bone,
            camera_bone,
            paired_bones,
            tags,
            models,
            slots,
            tracks: [Vec::new(), Vec::new()],
            geometry,
        })
    }

    /// Bind whatever this hand started playing that is not bound already.
    fn bind(&mut self, hand: usize, anims: &[PosedClip<'_>]) {
        for anim in anims {
            let node = anim.node;
            if self.tracks[hand].len() <= node {
                self.tracks[hand].resize_with(node + 1, || None);
            }
            let clip = core::ptr::from_ref::<AnimClip>(anim.clip) as usize;
            if self.tracks[hand][node]
                .as_ref()
                .is_some_and(|binding| binding.clip == clip)
            {
                continue;
            }
            let tracks = self.dobj.tracks_for(anim.clip);
            self.tracks[hand][node] = Some(TrackBinding { clip, tracks });
        }
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
        self.bind(hand, anims);
        let instances: Vec<AnimInstance<'_>> = anims
            .iter()
            .filter_map(|anim| {
                let binding = self.tracks[hand].get(anim.node)?.as_ref()?;
                Some(AnimInstance {
                    clip: anim.clip,
                    tracks: &binding.tracks,
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
        let world = self.dobj.pose(&instances, &self.parts, Mat4::IDENTITY);
        let skin = self.dobj.skin_matrices(&world);
        let eye_from_world = tag_view_to_bevy_camera() * world[self.view_bone].inverse();
        let lens = self
            .camera_bone
            .map(|camera| tag_camera_lens_local(world[self.view_bone], world[camera]))
            .unwrap_or(Mat4::IDENTITY);
        let published = if hand == 0 {
            world.len()
        } else {
            self.paired_bones.min(world.len())
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
