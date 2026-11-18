use std::collections::{HashMap, HashSet};

use asset_iw4::size::{self as sz, fx_elem};
use fastfile_iw4::{
    AssetLinkSink, AssetType, FxEffectDefGeometry, Ptr, Result, ZonePtr, ZoneStream,
};
use fx_iw4::{
    FX_EFFECT_DEF_SIZE, FX_ELEM_DEF_STRIDE, FX_SPARK_FOUNTAIN_DEF_OFF_SPARK_COUNT,
    FX_SPARK_FOUNTAIN_DEF_SIZE, FxEffectDefView, FxElemDefView, FxTrailVertex, effect_def_view,
    elem_def_view,
};

use crate::asset_graph::AssetEdgeFromPtrs;
use crate::graph_support::AuthoredRef;
use asset_core::{AssetEdge, AssetEdgeCensus, AssetEdgeReason, ZoneOwner};
use asset_material::{MaterialCatalog, MaterialDefinitions, TS_2D, TS_COLOR_MAP};

#[inline]
pub fn material_bind_name(name: &str) -> &str {
    crate::AssetRef::bare_name(name)
}

pub fn insert_fx_color_image<V>(map: &mut HashMap<String, V>, authored_name: &str, image: V) {
    let bind = material_bind_name(authored_name);
    if bind.is_empty() {
        return;
    }
    map.entry(bind.to_owned()).or_insert(image);
}

pub fn alias_fx_color_map_stubs<V: Clone>(map: &mut HashMap<String, V>) -> usize {
    let pairs: Vec<(String, String)> = map
        .keys()
        .filter_map(|name| {
            let bind = material_bind_name(name);
            (bind != name.as_str() && !map.contains_key(bind))
                .then(|| (bind.to_owned(), name.clone()))
        })
        .collect();
    let mut added = 0usize;
    for (bare, source) in pairs {
        if let Some(image) = map.get(&source).cloned() {
            map.insert(bare, image);
            added += 1;
        }
    }
    added
}

pub fn lookup_fx_color_image<'a, V>(
    map: &'a HashMap<String, V>,
    authored_name: &str,
) -> Option<&'a V> {
    map.get(material_bind_name(authored_name))
}

pub mod elem_type {
    use asset_iw4::size::fx_elem;

    pub const BILLBOARD: u8 = 0;
    pub const ORIENTED: u8 = 1;
    pub const TAIL: u8 = fx_elem::TAIL;
    pub const TRAIL: u8 = fx_elem::TRAIL;
    pub const CLOUD: u8 = 4;
    pub const SPARK_CLOUD: u8 = 5;
    pub const SPARK_FOUNTAIN: u8 = fx_elem::SPARK_FOUNTAIN;
    pub const MODEL: u8 = fx_elem::MODEL;
    pub const OMNI_LIGHT: u8 = 8;
    pub const SPOT_LIGHT: u8 = 9;
    pub const SOUND: u8 = fx_elem::SOUND;
    pub const DECAL: u8 = fx_elem::DECAL;
    pub const RUNNER: u8 = fx_elem::RUNNER;

    pub fn name(t: u8) -> &'static str {
        match t {
            BILLBOARD => "billboard",
            ORIENTED => "oriented",
            TAIL => "tail",
            TRAIL => "trail",
            CLOUD => "cloud",
            SPARK_CLOUD => "spark_cloud",
            SPARK_FOUNTAIN => "spark_fountain",
            MODEL => "model",
            OMNI_LIGHT => "omni_light",
            SPOT_LIGHT => "spot_light",
            SOUND => "sound",
            DECAL => "decal",
            RUNNER => "runner",
            _ => "unknown",
        }
    }
}

pub type FxElemMaterialReason = AssetEdgeReason;
pub type FxElemMaterial = AssetEdge<asset_core::MaterialSpace>;

pub type FxChildEdge = AssetEdge<asset_core::FxSpace>;

pub type FxElemModelEdge = AssetEdge<asset_core::FxModelSpace>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxBankSound<'a> {
    Gap,
    Silent,
    Play {
        namespace: crate::FamilyId,
        alias: &'a str,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnedFxVisual {
    Material {
        material: FxElemMaterial,
        hint: Option<String>,
        material_namespace: crate::FamilyId,
        /// Whether the zone authored this visual at all, and whether it reached
        /// it through an alias. The zone pointers themselves stop at the walk:
        /// nothing downstream ever dereferenced them, only asked whether they
        /// were there, and a pointer into a closed zone is not an answer.
        authored: AuthoredRef,
    },

    Mark {
        materials: [FxElemMaterial; 2],
        hints: [Option<String>; 2],
        material_namespaces: [crate::FamilyId; 2],
        authored: [AuthoredRef; 2],
    },
    Model {
        edge: FxElemModelEdge,
        hint: Option<String>,
    },

    Runner {
        edge: FxChildEdge,
        hint: Option<String>,
    },

    Sound {
        hint: Option<String>,
    },
    UnresolvedMaterial,
    None,
}

impl OwnedFxVisual {
    pub fn present_name(&self) -> Option<&str> {
        match self {
            Self::Material { material, hint, .. } if material.is_bound() => {
                hint.as_deref().filter(|name| !name.is_empty())
            }
            _ => None,
        }
    }

    pub fn decode_keys(&self) -> Vec<asset_core::MaterialKey> {
        let key = |namespace: crate::FamilyId, hint: &Option<String>| {
            hint.as_deref()
                .filter(|name| !name.is_empty())
                .map(|name| asset_core::MaterialKey {
                    namespace,
                    name: name.to_owned(),
                })
        };
        match self {
            Self::Material {
                hint,
                material_namespace,
                ..
            } => key(*material_namespace, hint).into_iter().collect(),
            Self::Mark {
                hints,
                material_namespaces,
                ..
            } => (0..2)
                .filter_map(|i| key(material_namespaces[i], &hints[i]))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn bound_index(&self) -> Option<usize> {
        match self {
            Self::Material { material, .. } => material.bound_index(),
            _ => None,
        }
    }

    pub fn edge_kind(&self) -> Option<&'static str> {
        match self {
            Self::Material { material, .. } => Some(material.edge_kind()),
            Self::Mark { materials, .. } => Some(materials[0].edge_kind()),
            Self::UnresolvedMaterial => Some("decal:array_unpatched"),
            _ => None,
        }
    }

    pub fn mark_bound_name(&self, slot: usize) -> Option<&str> {
        match self {
            Self::Mark {
                materials, hints, ..
            } if slot < 2 && materials[slot].is_bound() => {
                hints[slot].as_deref().filter(|n| !n.is_empty())
            }
            _ => None,
        }
    }

    pub fn mark_edge_kind(&self, slot: usize) -> Option<&'static str> {
        match self {
            Self::Mark { materials, .. } if slot < 2 => Some(materials[slot].edge_kind()),
            Self::UnresolvedMaterial if slot == 0 => Some("decal:array_unpatched"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnedFxTrailDef {
    pub scroll_time_msec: i32,
    pub repeat_dist: i32,
    pub inv_split_dist: f32,
    pub inv_split_arc_dist: f32,
    pub inv_split_time: f32,
    pub verts: Vec<FxTrailVertex>,
    pub inds: Vec<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnedFxSparkFountainDef {
    pub gravity: f32,
    pub bounce_frac: f32,
    pub bounce_rand: f32,
    pub spark_spacing: f32,
    pub spark_length: f32,
    pub spark_count: i32,
    pub loop_time: f32,
    pub vel_min: f32,
    pub vel_max: f32,
    pub vel_cone_frac: f32,
    pub rest_speed: f32,
    pub boost_time: f32,
    pub boost_factor: f32,
}

#[derive(Clone, Debug)]
pub struct OwnedFxElemDef {
    pub view: FxElemDefView,

    pub raw: Vec<u8>,
    pub vel_samples: Vec<u8>,

    pub vel_graph_local: Vec<fx_iw4::FxElemVec3Range>,

    pub vel_graph_world: Vec<fx_iw4::FxElemVec3Range>,
    pub vis_samples: Vec<u8>,
    pub visuals: Vec<OwnedFxVisual>,

    pub effect_on_impact: FxChildEdge,
    pub effect_on_impact_hint: Option<String>,

    pub effect_on_death: FxChildEdge,
    pub effect_on_death_hint: Option<String>,

    pub effect_emitted: FxChildEdge,
    pub effect_emitted_hint: Option<String>,

    pub has_extended: bool,

    pub trail_def: Option<OwnedFxTrailDef>,

    pub spark_fountain_def: Option<OwnedFxSparkFountainDef>,

    pub phys_pieces: Vec<Option<fx_iw4::FxPhysPiece>>,
}

impl OwnedFxElemDef {
    pub const GAP_RANGES: &'static [(usize, usize, &'static str)] = &[
        (0x0a8, 0x0b0, "FxElemAtlas before elemType"),
        (0x0d8, 0x0f4, "effectOn* / emitDist before extended"),
        (0x0f8, 0x0f9, "byte before lightingFrac"),
        (0x0fb, 0x0fc, "tail byte after useItemClip"),
    ];

    pub fn runner_child_edge(&self, random_seed: u64) -> Option<FxChildEdge> {
        if self.view.elem_type != elem_type::RUNNER {
            return None;
        }
        let idx = fx_iw4::elem_visual_index(self.view.visual_count, random_seed);
        match self.visuals.get(idx)? {
            OwnedFxVisual::Runner { edge, .. } => Some(*edge),
            _ => None,
        }
    }

    pub fn sound_in_bank<'a>(
        &self,
        random_seed: u64,
        sounds: &'a crate::SoundCatalog,
    ) -> FxBankSound<'a> {
        if self.view.elem_type != elem_type::SOUND {
            return FxBankSound::Gap;
        }
        let idx = fx_iw4::elem_visual_index(self.view.visual_count, random_seed);
        let hint = match self.visuals.get(idx) {
            None => return FxBankSound::Gap,
            Some(OwnedFxVisual::None) => return FxBankSound::Silent,
            Some(OwnedFxVisual::Sound { hint }) => hint.as_deref().filter(|name| !name.is_empty()),
            Some(_) => return FxBankSound::Gap,
        };
        let Some(name) = hint else {
            return FxBankSound::Silent;
        };
        match sounds
            .index_in(crate::FamilyId::IW4, name)
            .or_else(|| sounds.index_unique(name))
            .and_then(|index| {
                sounds
                    .name_at(index)
                    .and_then(|alias| Some((sounds.namespace_of_alias(index)?, alias)))
            }) {
            Some((namespace, alias)) => FxBankSound::Play { namespace, alias },
            None => FxBankSound::Gap,
        }
    }

    pub fn model_edge(&self, random_seed: u64) -> Option<FxElemModelEdge> {
        if self.view.elem_type != elem_type::MODEL {
            return None;
        }
        let idx = fx_iw4::elem_visual_index(self.view.visual_count, random_seed);
        match self.visuals.get(idx)? {
            OwnedFxVisual::Model { edge, .. } => Some(*edge),
            OwnedFxVisual::None => Some(AssetEdge::Absent),
            _ => None,
        }
    }

    pub fn phys_piece(&self, random_seed: u64) -> Option<fx_iw4::FxPhysPiece> {
        let idx = fx_iw4::elem_visual_index(self.view.visual_count, random_seed);
        self.phys_pieces.get(idx).copied().flatten()
    }

    pub fn decal_mark_pair(&self, random_seed: u64) -> Option<&OwnedFxVisual> {
        if self.view.elem_type != elem_type::DECAL {
            return None;
        }
        let idx = fx_iw4::elem_visual_index(self.view.visual_count, random_seed);
        self.visuals.get(idx)
    }

    pub fn primary_material(&self) -> Option<(usize, &str)> {
        self.visuals.iter().find_map(|v| {
            let index = v.bound_index()?;
            let name = v.present_name()?;
            Some((index, name))
        })
    }
}

#[derive(Clone, Debug)]
pub struct OwnedFxEffectDef {
    pub namespace: crate::FamilyId,
    pub name: String,
    pub view: FxEffectDefView,
    pub header_raw: Vec<u8>,
    pub elems: Vec<OwnedFxElemDef>,
}

/// The effects a build finished with. There is no zone link map here and no
/// half-captured name waiting for the slot it belongs to: the names are the
/// answers, and a consumer holding this cannot resolve one more pointer.
#[derive(Clone, Debug, Default)]
pub struct FxDefinitions {
    mark_capabilities: std::sync::OnceLock<Vec<bool>>,
    by_key: HashMap<(crate::FamilyId, String), usize>,

    defs: Vec<OwnedFxEffectDef>,

    zones: Vec<ZoneOwner>,
    capture_zone: ZoneOwner,
    map_namespace: Option<crate::FamilyId>,
    pub capture_gaps: usize,
}

/// The build. `links` and `last_captured` belong to the walk, and `publish`
/// leaves them behind with it.
#[derive(Clone, Debug, Default)]
pub struct FxCatalog {
    published: FxDefinitions,

    links: HashMap<asset_material::AssetPointerIdentity, FxLink>,
    last_captured: Option<String>,
    capture_ns: Option<crate::FamilyId>,
}

impl std::ops::Deref for FxCatalog {
    type Target = FxDefinitions;

    fn deref(&self) -> &Self::Target {
        &self.published
    }
}

impl std::ops::DerefMut for FxCatalog {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.published
    }
}

impl FxCatalog {
    pub fn insert_captured(&mut self, def: OwnedFxEffectDef) {
        self.last_captured = Some(def.name.clone());
        self.insert_owned(def);
    }

    pub fn hint_edge(
        &self,
        authored_slot: bool,
        hint: Option<&str>,
        ns: crate::FamilyId,
    ) -> AssetEdge<asset_core::FxSpace> {
        let hint = hint.filter(|name| !name.is_empty());
        if !authored_slot && hint.is_none() {
            return AssetEdge::Absent;
        }
        match hint.and_then(|name| self.index_in(ns, name)) {
            Some(index) => AssetEdge::bind_order(index, self.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        }
    }

    /// Ends the build: the effect definitions travel on, the zone pointer map
    /// that produced them does not.
    pub fn publish(self) -> FxDefinitions {
        if let Some(effect) = self.published.defs.first() {
            self.published.can_emit_marks(effect);
        }
        self.published
    }

    pub fn note_loaded(
        &mut self,
        slot: impl Into<asset_material::AssetPointerIdentity>,
        insert_slot: Option<asset_material::AssetPointerIdentity>,
    ) {
        let slot = slot.into();
        let Some(name) = self.last_captured.take() else {
            return;
        };
        self.links.insert(slot, FxLink::Direct(name.clone()));
        if let Some(insert_slot) = insert_slot {
            self.links.insert(insert_slot, FxLink::Direct(name));
        }
    }

    pub fn note_alias(
        &mut self,
        slot: impl Into<asset_material::AssetPointerIdentity>,
        target: impl Into<asset_material::AssetPointerIdentity>,
    ) {
        let (slot, target) = (slot.into(), target.into());
        self.links.insert(slot, FxLink::Alias(target));
    }

    pub fn set_capture_ns(&mut self, ns: crate::FamilyId) {
        self.capture_ns = Some(ns);
    }

    pub fn absorb(&mut self, other: FxCatalog) {
        let FxCatalog {
            published:
                FxDefinitions {
                    defs,
                    zones,
                    capture_gaps,
                    ..
                },
            links,
            ..
        } = other;
        self.capture_gaps += capture_gaps;

        self.links.extend(links);
        for (effect, zone) in defs.into_iter().zip(zones) {
            let saved = self.capture_zone;
            self.capture_zone = zone;
            self.insert_owned(effect);
            self.capture_zone = saved;
        }
    }

    pub fn absorb_missing(&mut self, other: FxCatalog) {
        let FxCatalog {
            published:
                FxDefinitions {
                    defs,
                    zones,
                    capture_gaps,
                    ..
                },
            ..
        } = other;
        self.capture_gaps += capture_gaps;
        for (effect, zone) in defs.into_iter().zip(zones) {
            if self.index_in(effect.namespace, &effect.name).is_some() {
                continue;
            }
            let saved = self.capture_zone;
            self.capture_zone = zone;
            self.insert_owned(effect);
            self.capture_zone = saved;
        }
    }

    pub fn capture(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: FxEffectDefGeometry,
        materials: &MaterialCatalog,
        xmodel_names: &HashMap<Ptr, Ptr>,
    ) -> Result<()> {
        let Some(name_ptr) = geometry.name else {
            self.capture_gaps += 1;
            return Ok(());
        };
        let Ok(name) = s.cstr(name_ptr) else {
            self.capture_gaps += 1;
            return Ok(());
        };
        if name.is_empty() {
            self.capture_gaps += 1;
            return Ok(());
        }
        let Ok(header_raw) = s.slice_at(geometry.header, 0, s.layout(FX_EFFECT_DEF_SIZE, 40))
        else {
            self.capture_gaps += 1;
            return Ok(());
        };
        let view = if s.wire_format() == fastfile_iw4::Iw4WireFormat::X64 {
            Some(fx_iw4::FxEffectDefView {
                flags: s.i32_at(geometry.header, 8)?,
                msec_looping_life: s.i32_at(geometry.header, 16)?,
                looping_count: geometry.looping_count,
                one_shot_count: geometry.one_shot_count,
                emission_count: geometry.emission_count,
            })
        } else {
            effect_def_view(header_raw)
        };
        let Some(view) = view else {
            self.capture_gaps += 1;
            return Ok(());
        };
        if view.looping_count != geometry.looping_count
            || view.one_shot_count != geometry.one_shot_count
            || view.emission_count != geometry.emission_count
        {
            self.capture_gaps += 1;
            return Ok(());
        }

        let mut elems = Vec::with_capacity(geometry.elem_def_count);
        if let Some(arr) = geometry.elem_defs {
            for i in 0..geometry.elem_def_count {
                let elem_ptr = arr.at(i * s.layout(sz::FX_ELEM_DEF, 288));
                match capture_elem(s, elem_ptr, materials, xmodel_names) {
                    Some(elem) => elems.push(elem),
                    None => {
                        self.capture_gaps += 1;
                        return Ok(());
                    }
                }
            }
        } else if geometry.elem_def_count > 0 {
            self.capture_gaps += 1;
            return Ok(());
        }

        self.last_captured = Some(name.to_owned());
        let namespace = self
            .capture_ns
            .expect("asset capture requires an explicit family");
        self.insert_owned(OwnedFxEffectDef {
            namespace,
            name: name.to_owned(),
            view,
            header_raw: header_raw.to_vec(),
            elems,
        });
        Ok(())
    }

    pub fn name_at_slot(
        &self,
        slot: impl Into<asset_material::AssetPointerIdentity>,
    ) -> Option<&str> {
        let mut at = slot.into();
        for _ in 0..8 {
            match self.links.get(&at)? {
                FxLink::Direct(name) => return Some(name.as_str()),
                FxLink::Alias(next) => at = *next,
            }
        }
        None
    }

    pub fn bind_named_slot(
        &mut self,
        slot: impl Into<asset_material::AssetPointerIdentity>,
        name: &str,
    ) {
        let slot = slot.into();
        let ns = self
            .capture_ns
            .expect("asset capture requires an explicit family");
        if self.index_in(ns, name).is_none() {
            self.insert_owned(OwnedFxEffectDef {
                namespace: ns,
                name: name.to_owned(),
                view: FxEffectDefView {
                    flags: 0,
                    msec_looping_life: 0,
                    looping_count: 0,
                    one_shot_count: 0,
                    emission_count: 0,
                },
                header_raw: Vec::new(),
                elems: Vec::new(),
            });
        }
        self.links.insert(slot, FxLink::Direct(name.to_owned()));
    }
}

#[derive(Clone, Debug)]
enum FxLink {
    Direct(String),
    Alias(asset_material::AssetPointerIdentity),
}

impl FxDefinitions {
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn index_in(&self, ns: crate::FamilyId, name: &str) -> Option<usize> {
        if let Some(index) = self.by_key.get(&(ns, name.to_owned())) {
            return Some(*index);
        }
        name.bytes()
            .any(|b| b.is_ascii_uppercase())
            .then(|| self.by_key.get(&(ns, ascii_lower(name))).copied())
            .flatten()
    }

    pub fn get_in(&self, ns: crate::FamilyId, name: &str) -> Option<&OwnedFxEffectDef> {
        self.index_in(ns, name)
            .and_then(|index| self.defs.get(index))
    }

    pub fn index_of(&self, def: &OwnedFxEffectDef) -> Option<usize> {
        self.index_in(def.namespace, &def.name)
            .filter(|&index| std::ptr::eq(&self.defs[index], def))
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.iter().map(|def| def.name.as_str())
    }

    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn zone_of(&self, index: usize) -> ZoneOwner {
        self.zones.get(index).copied().unwrap_or(self.capture_zone)
    }

    pub fn insert_owned(&mut self, def: OwnedFxEffectDef) {
        self.mark_capabilities.take();
        let key = (def.namespace, ascii_lower(&def.name));
        if let Some(index) = self.by_key.get(&key).copied() {
            self.zones[index] = self.capture_zone;
            self.defs[index] = def;
        } else {
            let index = self.defs.len();
            self.by_key.insert(key, index);
            self.zones.push(self.capture_zone);
            self.defs.push(def);
        }
    }

    pub fn can_emit_marks(&self, effect: &OwnedFxEffectDef) -> bool {
        let flags = self.mark_capabilities.get_or_init(|| {
            let edges = |elem: &OwnedFxElemDef| {
                [
                    elem.effect_on_impact,
                    elem.effect_on_death,
                    elem.effect_emitted,
                ]
                .into_iter()
                .chain(elem.visuals.iter().filter_map(|visual| match visual {
                    OwnedFxVisual::Runner { edge, .. } => Some(*edge),
                    _ => None,
                }))
                .collect::<Vec<_>>()
            };
            let children: Vec<Vec<FxChildEdge>> = self
                .defs
                .iter()
                .map(|effect| effect.elems.iter().flat_map(edges).collect())
                .collect();
            let mut flags: Vec<bool> = self
                .defs
                .iter()
                .zip(&children)
                .map(|(effect, children)| {
                    effect.elems.iter().any(|elem| {
                        fx_iw4::FxElemType::from_u8(elem.view.elem_type)
                            == Some(fx_iw4::FxElemType::Decal)
                    }) || children.iter().any(|edge| edge.is_unresolved())
                })
                .collect();
            loop {
                let mut changed = false;
                for (index, children) in children.iter().enumerate() {
                    if !flags[index]
                        && children
                            .iter()
                            .filter_map(|edge| edge.bound_index())
                            .any(|child| flags.get(child).copied().unwrap_or(true))
                    {
                        flags[index] = true;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            flags
        });
        self.index_of(effect)
            .and_then(|index| flags.get(index))
            .copied()
            .unwrap_or(true)
    }

    pub fn def_at(&self, index: usize) -> Option<&OwnedFxEffectDef> {
        self.defs.get(index)
    }

    pub fn effects(&self) -> impl Iterator<Item = &OwnedFxEffectDef> {
        self.defs.iter()
    }

    pub fn set_map_namespace(&mut self, ns: crate::FamilyId) {
        self.map_namespace = Some(ns);
    }

    pub fn map_namespace(&self) -> crate::FamilyId {
        self.map_namespace
            .expect("effects require an explicit map family")
    }

    pub fn resolve_createfx_id(&self, fxid: &str) -> Option<&OwnedFxEffectDef> {
        self.resolve_def_for_map(fxid).or_else(|| {
            self.get_in(
                self.map_namespace
                    .expect("effects require an explicit map family"),
                fxid,
            )
        })
    }

    pub fn map_fx_name<'a>(&'a self, name: &'a str) -> FxName<'a> {
        match self.resolve_def_for_map(name) {
            Some(def) => FxName::new(def.namespace, &def.name),
            None => FxName::new(
                self.map_namespace
                    .expect("effects require an explicit map family"),
                name,
            ),
        }
    }

    pub fn resolve_def_in(&self, ns: crate::FamilyId, name: &str) -> Option<&OwnedFxEffectDef> {
        if let Some(def) = self.get_in(ns, name).filter(|d| !d.elems.is_empty()) {
            return Some(def);
        }
        let bind = material_bind_name(name);
        if bind == name {
            return None;
        }
        self.get_in(ns, bind).filter(|d| !d.elems.is_empty())
    }

    pub fn alias_for_map(&mut self, from: &str, to: &str) -> bool {
        let map_ns = self
            .map_namespace
            .expect("effects require an explicit map family");
        let Some(def) = self.resolve_def_in(map_ns, to) else {
            return false;
        };
        let Some(index) = self.index_in(map_ns, &def.name) else {
            return false;
        };
        self.by_key.insert((map_ns, ascii_lower(from)), index);
        true
    }

    pub fn resolve_def_for_map(&self, name: &str) -> Option<&OwnedFxEffectDef> {
        if let Some((family, name)) = name.split_once(':') {
            return self.resolve_def_in(crate::FamilyId::parse(family)?, name);
        }
        let map_ns = self
            .map_namespace
            .expect("effects require an explicit map family");
        self.resolve_def_in(map_ns, name)
    }

    pub fn resolve_materials(&mut self, materials: &crate::MaterialDefinitions) {
        for effect in &mut self.defs {
            for elem in &mut effect.elems {
                for vis in &mut elem.visuals {
                    match vis {
                        OwnedFxVisual::Material {
                            material,
                            hint,
                            material_namespace,
                            authored,
                        } => remap_elem_material(
                            material,
                            hint,
                            *material_namespace,
                            *authored,
                            materials,
                        ),
                        OwnedFxVisual::Mark {
                            materials: mats,
                            hints,
                            material_namespaces,
                            authored,
                        } => {
                            for i in 0..2 {
                                remap_elem_material(
                                    &mut mats[i],
                                    &mut hints[i],
                                    material_namespaces[i],
                                    authored[i],
                                    materials,
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn resolve_nested_edges(&mut self) {
        self.mark_capabilities.take();
        let playable: HashMap<(crate::FamilyId, String), (usize, ZoneOwner)> = self
            .by_key
            .iter()
            .filter(|(_, index)| !self.defs[**index].elems.is_empty())
            .map(|(key, &index)| (key.clone(), (index, self.zone_of(index))))
            .collect();
        for effect in &mut self.defs {
            let ns = effect.namespace;
            let playable = |name: &str| lookup_playable_fx(&playable, ns, name);
            for elem in &mut effect.elems {
                remap_nested_child(
                    &mut elem.effect_on_impact,
                    elem.effect_on_impact_hint.as_deref(),
                    playable,
                );
                remap_nested_child(
                    &mut elem.effect_on_death,
                    elem.effect_on_death_hint.as_deref(),
                    playable,
                );
                remap_nested_child(
                    &mut elem.effect_emitted,
                    elem.effect_emitted_hint.as_deref(),
                    playable,
                );
                for vis in &mut elem.visuals {
                    if let OwnedFxVisual::Runner { edge, hint } = vis {
                        remap_nested_child(edge, hint.as_deref(), playable);
                    }
                }
            }
        }
    }

    pub fn resolve_model_edges(&mut self, models: &crate::FxModelCatalog) {
        for effect in &mut self.defs {
            let ns = effect.namespace;
            for elem in &mut effect.elems {
                for vis in &mut elem.visuals {
                    if let OwnedFxVisual::Model { edge, hint } = vis {
                        *edge = model_hint_edge(ns, hint.as_deref(), models);
                    }
                }
                elem.phys_pieces =
                    if fx_iw4::elem_skips_position_update(elem.view.elem_type, elem.view.flags) {
                        elem.visuals
                            .iter()
                            .map(|vis| model_phys_piece(vis, models))
                            .collect()
                    } else {
                        Vec::new()
                    };
            }
        }
    }

    pub fn material_visual_bound_count(&self) -> usize {
        self.effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
            .filter(|v| v.bound_index().is_some())
            .count()
    }

    pub fn material_visual_unresolved_count(&self) -> usize {
        self.effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
            .filter(|v| matches!(v, OwnedFxVisual::Material { material, .. } if material.is_unresolved()))
            .count()
    }

    pub fn material_visual_count(&self) -> usize {
        self.effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
            .filter(|v| matches!(v, OwnedFxVisual::Material { .. }))
            .count()
    }

    pub fn material_visual_unique_bound_count(&self) -> usize {
        self.unique_bound_hints().len()
    }

    pub fn unique_material_keys(&self) -> Vec<asset_core::MaterialKey> {
        let mut seen = HashSet::new();
        let mut keys = Vec::new();
        for vis in self
            .effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
        {
            for key in vis.decode_keys() {
                if seen.insert(key.clone()) {
                    keys.push(key);
                }
            }
        }
        keys
    }

    pub fn unique_bound_hints(&self) -> Vec<(usize, &str)> {
        let mut seen = std::collections::BTreeMap::new();
        for vis in self
            .effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
        {
            let OwnedFxVisual::Material { material, hint, .. } = vis else {
                continue;
            };
            let Some(index) = material.bound_index() else {
                continue;
            };
            seen.entry(index).or_insert(
                hint.as_deref()
                    .filter(|name| !name.is_empty())
                    .unwrap_or("-"),
            );
        }
        seen.into_iter().collect()
    }

    pub fn material_visual_decal_count(&self) -> usize {
        self.effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
            .filter(|v| {
                matches!(
                    v,
                    OwnedFxVisual::Mark { .. } | OwnedFxVisual::UnresolvedMaterial
                )
            })
            .count()
    }

    pub fn material_visual_decal_unpatched_count(&self) -> usize {
        self.effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
            .filter(|v| matches!(v, OwnedFxVisual::UnresolvedMaterial))
            .count()
    }

    pub fn decal_mark_bound_slot_count(&self) -> usize {
        self.decal_mark_slot_count(|m| m.is_bound())
    }

    pub fn decal_mark_unresolved_slot_count(&self) -> usize {
        self.decal_mark_slot_count(|m| m.is_unresolved())
    }

    pub fn decal_mark_temp_slot_count(&self) -> usize {
        self.decal_mark_slot_count(|m| {
            matches!(
                m,
                FxElemMaterial::Unresolved(FxElemMaterialReason::TempFieldNotAliasable)
            )
        })
    }

    pub fn unique_decal_mark_hints(&self) -> Vec<(usize, &str)> {
        self.unique_decal_mark_hints_in_slots(&[0, 1])
    }

    pub fn unique_decal_mark_mc_hints(&self) -> Vec<(usize, &str)> {
        self.unique_decal_mark_hints_in_slots(&[0])
    }

    pub fn unique_decal_mark_wc_hints(&self) -> Vec<(usize, &str)> {
        self.unique_decal_mark_hints_in_slots(&[1])
    }

    pub fn unique_decal_mark_count(&self) -> usize {
        self.unique_decal_mark_hints().len()
    }

    pub fn unique_decal_mark_mc_count(&self) -> usize {
        self.unique_decal_mark_mc_hints().len()
    }

    pub fn unique_decal_mark_wc_count(&self) -> usize {
        self.unique_decal_mark_wc_hints().len()
    }

    pub fn unique_decal_mark_decoded_color_count(&self, materials: &MaterialDefinitions) -> usize {
        self.unique_decal_mark_hints()
            .into_iter()
            .filter(|(index, _)| material_has_decoded_color(materials, *index))
            .count()
    }

    pub fn unique_decal_mark_decoded_miss_samples(
        &self,
        materials: &MaterialDefinitions,
        cap: usize,
    ) -> Vec<String> {
        let mut out = Vec::new();
        for (index, hint) in self.unique_decal_mark_hints() {
            if out.len() >= cap {
                break;
            }
            if material_has_decoded_color(materials, index) {
                continue;
            }
            out.push(hint.to_owned());
        }
        out
    }

    fn unique_decal_mark_hints_in_slots(&self, slots: &[usize]) -> Vec<(usize, &str)> {
        let mut seen = std::collections::BTreeMap::new();
        for vis in self
            .effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
        {
            let OwnedFxVisual::Mark {
                materials, hints, ..
            } = vis
            else {
                continue;
            };
            for &slot in slots {
                let Some(index) = materials[slot].bound_index() else {
                    continue;
                };
                seen.entry(index).or_insert(
                    hints[slot]
                        .as_deref()
                        .filter(|name| !name.is_empty())
                        .unwrap_or("-"),
                );
            }
        }
        seen.into_iter().collect()
    }

    fn decal_mark_slot_count(&self, pred: impl Fn(&FxElemMaterial) -> bool) -> usize {
        self.effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
            .filter_map(|v| match v {
                OwnedFxVisual::Mark { materials, .. } => Some(materials.as_slice()),
                _ => None,
            })
            .flatten()
            .filter(|m| pred(m))
            .count()
    }

    pub fn decal_mark_samples(&self, cap: usize) -> Vec<String> {
        let mut out = Vec::new();
        for effect in self.effects() {
            for (elem_i, elem) in effect.elems.iter().enumerate() {
                for vis in &elem.visuals {
                    if out.len() >= cap {
                        return out;
                    }
                    match vis {
                        OwnedFxVisual::UnresolvedMaterial => {
                            out.push(format!("{}:{elem_i}:array_unpatched", effect.name));
                        }
                        OwnedFxVisual::Mark {
                            materials, hints, ..
                        } => {
                            let a = hints[0].as_deref().filter(|n| !n.is_empty()).unwrap_or("-");
                            let b = hints[1].as_deref().filter(|n| !n.is_empty()).unwrap_or("-");
                            out.push(format!(
                                "{}:{elem_i}:{}|{}:{}|{}",
                                effect.name,
                                materials[0].edge_kind(),
                                materials[1].edge_kind(),
                                a,
                                b
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        out
    }

    pub fn material_visual_absent_count(&self) -> usize {
        self.effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
            .filter(|v| {
                matches!(
                    v,
                    OwnedFxVisual::Material {
                        material: FxElemMaterial::Absent,
                        ..
                    }
                )
            })
            .count()
    }

    pub fn nested_child_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for elem in self.effects().flat_map(|def| def.elems.iter()) {
            census.push(elem.effect_on_impact);
            census.push(elem.effect_on_death);
            census.push(elem.effect_emitted);
        }
        census
    }

    pub fn runner_child_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for vis in self
            .effects()
            .flat_map(|def| def.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
        {
            if let OwnedFxVisual::Runner { edge, .. } = vis {
                census.push(*edge);
            }
        }
        census
    }

    pub fn model_elem_n(&self) -> usize {
        self.effects()
            .flat_map(|def| def.elems.iter())
            .filter(|elem| elem.view.elem_type == elem_type::MODEL)
            .count()
    }

    pub fn model_named_n(&self) -> usize {
        self.effects()
            .flat_map(|def| def.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
            .filter(|vis| {
                matches!(vis, OwnedFxVisual::Model { hint: Some(name), .. } if !name.is_empty())
            })
            .count()
    }

    pub fn model_hints(&self) -> HashSet<(crate::FamilyId, String)> {
        self.effects()
            .flat_map(|def| def.elems.iter().map(move |elem| (def.namespace, elem)))
            .flat_map(|(ns, elem)| elem.visuals.iter().map(move |vis| (ns, vis)))
            .filter_map(|(ns, vis)| match vis {
                OwnedFxVisual::Model {
                    hint: Some(name), ..
                } if !name.is_empty() => Some((ns, name.clone())),
                _ => None,
            })
            .collect()
    }

    pub fn unique_type10_sound_hints(&self) -> Vec<&str> {
        let mut seen = std::collections::BTreeSet::new();
        for vis in self
            .effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
        {
            if let OwnedFxVisual::Sound { hint: Some(name) } = vis
                && !name.is_empty()
            {
                seen.insert(name.as_str());
            }
        }
        seen.into_iter().collect()
    }

    pub fn model_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for vis in self
            .effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
        {
            if let OwnedFxVisual::Model { edge, .. } = vis {
                census.push(*edge);
            }
        }
        census
    }

    pub fn material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for vis in self
            .effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
        {
            match vis {
                OwnedFxVisual::Material { material, .. } => census.push(*material),
                OwnedFxVisual::Mark { materials, .. } => {
                    census.push(materials[0]);
                    census.push(materials[1]);
                }
                OwnedFxVisual::UnresolvedMaterial
                | OwnedFxVisual::Model { .. }
                | OwnedFxVisual::Runner { .. }
                | OwnedFxVisual::Sound { .. }
                | OwnedFxVisual::None => {}
            }
        }
        census
    }

    pub fn material_visual_unresolved_temp_count(&self) -> usize {
        self.unresolved_reason_count(FxElemMaterialReason::TempFieldNotAliasable)
    }

    pub fn material_visual_unresolved_miss_count(&self) -> usize {
        self.unresolved_reason_count(FxElemMaterialReason::CatalogMiss)
    }

    fn unresolved_reason_count(&self, reason: FxElemMaterialReason) -> usize {
        self.effects()
            .flat_map(|effect| effect.elems.iter())
            .flat_map(|elem| elem.visuals.iter())
            .filter(|v| {
                matches!(
                    v,
                    OwnedFxVisual::Material {
                        material: FxElemMaterial::Unresolved(r),
                        ..
                    } if *r == reason
                )
            })
            .count()
    }

    pub fn unresolved_material_samples(&self, cap: usize) -> Vec<String> {
        let mut out = Vec::new();
        for effect in self.effects() {
            for (elem_i, elem) in effect.elems.iter().enumerate() {
                for vis in &elem.visuals {
                    let OwnedFxVisual::Material { material, hint, .. } = vis else {
                        continue;
                    };
                    if !material.is_unresolved() {
                        continue;
                    }
                    let hint = hint.as_deref().filter(|n| !n.is_empty()).unwrap_or("-");
                    out.push(format!(
                        "{}[{elem_i}] {} hint={hint}",
                        effect.name,
                        material.edge_kind()
                    ));
                    if out.len() >= cap {
                        return out;
                    }
                }
            }
        }
        out
    }
}

impl AssetLinkSink for FxCatalog {
    fn loaded(
        &mut self,
        _s: &ZoneStream<'_>,
        _ty: AssetType,
        _slot: Ptr,
        _insert_slot: Option<Ptr>,
    ) -> Result<()> {
        Ok(())
    }

    fn alias(&mut self, _ty: AssetType, _slot: Ptr, _target: Ptr) -> Result<()> {
        Ok(())
    }

    fn capture_fx(&mut self, s: &ZoneStream<'_>, geometry: FxEffectDefGeometry) -> Result<()> {
        let empty = MaterialCatalog::default();
        self.capture(s, geometry, &empty, &HashMap::new())
    }
}

fn capture_elem(
    s: &ZoneStream<'_>,
    p: Ptr,
    materials: &MaterialCatalog,
    xmodel_names: &HashMap<Ptr, Ptr>,
) -> Option<OwnedFxElemDef> {
    let raw = s
        .slice_at(p, 0, s.layout(sz::FX_ELEM_DEF, 288))
        .ok()?
        .to_vec();
    let view = if s.pointer_bytes() == 8 {
        fx_iw4::elem_def_view_x64(&raw)?
    } else {
        elem_def_view(&raw)?
    };
    let vel_count = view.vel_interval_count as usize + 1;
    let vis_count = view.vis_state_interval_count as usize + 1;
    let vel_samples = copy_ptr_array(
        s,
        p,
        s.layout(0xb4, 184),
        vel_count,
        sz::FX_ELEM_VEL_STATE_SAMPLE,
    );
    let vel_graph_local = parse_vel_graph_channel(&vel_samples, vel_count, false);
    let vel_graph_world = parse_vel_graph_channel(&vel_samples, vel_count, true);
    let vis_samples = copy_ptr_array(
        s,
        p,
        s.layout(0xb8, 192),
        vis_count,
        sz::FX_ELEM_VIS_STATE_SAMPLE,
    );
    let visuals = capture_visuals(s, p, &view, materials, xmodel_names);
    let (effect_on_impact, effect_on_impact_hint) =
        capture_named_child(read_name_field(s, p, s.layout(216, 232)));
    let (effect_on_death, effect_on_death_hint) =
        capture_named_child(read_name_field(s, p, s.layout(220, 240)));
    let (effect_emitted, effect_emitted_hint) =
        capture_named_child(read_name_field(s, p, s.layout(224, 248)));
    let has_extended = !matches!(s.ptr_at(p, s.layout(0xf4, 272)).ok()?, ZonePtr::Null);
    let trail_def = if view.elem_type == elem_type::TRAIL {
        capture_trail_def(s, p)
    } else {
        None
    };
    let spark_fountain_def = if view.elem_type == elem_type::SPARK_FOUNTAIN {
        capture_spark_fountain_def(s, p)
    } else {
        None
    };
    Some(OwnedFxElemDef {
        view,
        raw,
        vel_samples,
        vel_graph_local,
        vel_graph_world,
        vis_samples,
        visuals,
        effect_on_impact,
        effect_on_impact_hint,
        effect_on_death,
        effect_on_death_hint,
        effect_emitted,
        effect_emitted_hint,
        has_extended,
        trail_def,
        spark_fountain_def,
        phys_pieces: Vec::new(),
    })
}

fn capture_trail_def(s: &ZoneStream<'_>, elem: Ptr) -> Option<OwnedFxTrailDef> {
    let ZonePtr::Offset(trail) = s.ptr_at(elem, s.layout(0xf4, 272)).ok()? else {
        return None;
    };
    let trail = s.resolve_alias(trail);
    let header = s.slice_at(trail, 0, s.layout(sz::FX_TRAIL_DEF, 48)).ok()?;
    if header.len() < s.layout(sz::FX_TRAIL_DEF, 48) {
        return None;
    }
    let scroll_time_msec = i32::from_le_bytes(header[0x00..0x04].try_into().ok()?);
    let repeat_dist = i32::from_le_bytes(header[0x04..0x08].try_into().ok()?);
    let inv_split_dist = f32::from_le_bytes(header[0x08..0x0c].try_into().ok()?);
    let inv_split_arc_dist = f32::from_le_bytes(header[0x0c..0x10].try_into().ok()?);
    let inv_split_time = f32::from_le_bytes(header[0x10..0x14].try_into().ok()?);
    let vert_count = i32::from_le_bytes(header[0x14..0x18].try_into().ok()?).max(0) as usize;
    let ind_count = s.i32_at(trail, s.layout(0x1c, 32)).ok()?.max(0) as usize;

    let vert_bytes = copy_ptr_array(s, trail, 0x18, vert_count, sz::FX_TRAIL_VERTEX);
    let mut verts = Vec::with_capacity(vert_count);
    for chunk in vert_bytes.as_chunks::<{ sz::FX_TRAIL_VERTEX }>().0.iter() {
        let pos = [
            f32::from_le_bytes(chunk[0x00..0x04].try_into().ok()?),
            f32::from_le_bytes(chunk[0x04..0x08].try_into().ok()?),
        ];
        let normal = [
            f32::from_le_bytes(chunk[0x08..0x0c].try_into().ok()?),
            f32::from_le_bytes(chunk[0x0c..0x10].try_into().ok()?),
        ];
        let tex_coord = f32::from_le_bytes(chunk[0x10..0x14].try_into().ok()?);
        verts.push(FxTrailVertex {
            pos,
            normal,
            tex_coord,
        });
    }

    let ind_bytes = copy_ptr_array(s, trail, s.layout(0x20, 40), ind_count, 2);
    let mut inds = Vec::with_capacity(ind_count);
    for chunk in ind_bytes.as_chunks::<2>().0.iter() {
        inds.push(u16::from_le_bytes(*chunk));
    }

    Some(OwnedFxTrailDef {
        scroll_time_msec,
        repeat_dist,
        inv_split_dist,
        inv_split_arc_dist,
        inv_split_time,
        verts,
        inds,
    })
}

fn capture_spark_fountain_def(s: &ZoneStream<'_>, elem: Ptr) -> Option<OwnedFxSparkFountainDef> {
    let ZonePtr::Offset(def) = s.ptr_at(elem, s.layout(0xf4, 272)).ok()? else {
        return None;
    };
    let def = s.resolve_alias(def);
    let header = s.slice_at(def, 0, FX_SPARK_FOUNTAIN_DEF_SIZE).ok()?;
    if header.len() < FX_SPARK_FOUNTAIN_DEF_SIZE {
        return None;
    }
    let f32_at = |off: usize| -> Option<f32> {
        Some(f32::from_le_bytes(header[off..off + 4].try_into().ok()?))
    };
    let spark_count = i32::from_le_bytes(
        header[FX_SPARK_FOUNTAIN_DEF_OFF_SPARK_COUNT..FX_SPARK_FOUNTAIN_DEF_OFF_SPARK_COUNT + 4]
            .try_into()
            .ok()?,
    );
    Some(OwnedFxSparkFountainDef {
        gravity: f32_at(0x00)?,
        bounce_frac: f32_at(0x04)?,
        bounce_rand: f32_at(0x08)?,
        spark_spacing: f32_at(0x0c)?,
        spark_length: f32_at(0x10)?,
        spark_count,
        loop_time: f32_at(0x18)?,
        vel_min: f32_at(0x1c)?,
        vel_max: f32_at(0x20)?,
        vel_cone_frac: f32_at(0x24)?,
        rest_speed: f32_at(0x28)?,
        boost_time: f32_at(0x2c)?,
        boost_factor: f32_at(0x30)?,
    })
}

fn capture_visuals(
    s: &ZoneStream<'_>,
    p: Ptr,
    view: &FxElemDefView,
    materials: &MaterialCatalog,
    xmodel_names: &HashMap<Ptr, Ptr>,
) -> Vec<OwnedFxVisual> {
    let t = view.elem_type;
    let visual_count = view.visual_count as usize;
    let vis = p.at(s.layout(0xbc, 200));

    if matches!(t, elem_type::OMNI_LIGHT | elem_type::SPOT_LIGHT) || visual_count == 0 {
        return vec![OwnedFxVisual::None];
    }
    if t == elem_type::DECAL {
        let ptr = s.ptr_at(vis, 0);
        let Ok(ZonePtr::Offset(arr)) = ptr else {
            return (0..visual_count.max(1))
                .map(|_| OwnedFxVisual::UnresolvedMaterial)
                .collect();
        };
        let arr = s.resolve_alias(arr);
        return (0..visual_count.max(1))
            .map(|i| {
                let m = arr.at(i * s.layout(sz::FX_ELEM_MARK_VISUALS, 16));
                mark_pair(
                    resolve_material_visual(s, materials, m.at(0)),
                    resolve_material_visual(s, materials, m.at(s.layout(4, 8))),
                )
            })
            .collect();
    }
    if fx_elem::is_sprite(t) {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ZonePtr::Offset(arr)) = ptr else {
                let kind = match ptr {
                    Ok(ZonePtr::Following) => "following",
                    Ok(ZonePtr::Insert) => "insert",
                    Ok(ZonePtr::Null) => "null",
                    Ok(ZonePtr::Offset(_)) | Err(_) => "read_err",
                };
                return (0..visual_count)
                    .map(|_| OwnedFxVisual::Material {
                        material: FxElemMaterial::unresolved_for(Some(vis), None),
                        hint: Some(kind.to_string()),
                        material_namespace: materials.capture_ns(),
                        authored: AuthoredRef::from_ptrs(Some(vis), None),
                    })
                    .collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| {
                    resolve_material_visual(
                        s,
                        materials,
                        arr.at(i * s.layout(sz::FX_ELEM_VISUALS, 8)),
                    )
                })
                .collect();
        }
        return vec![resolve_material_visual(s, materials, vis)];
    }
    if t == elem_type::MODEL {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ZonePtr::Offset(arr)) = ptr else {
                return (0..visual_count).map(|_| OwnedFxVisual::None).collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| {
                    let slot = arr.at(i * s.layout(sz::FX_ELEM_VISUALS, 8));
                    model_visual(read_xmodel_name(s, slot, xmodel_names))
                })
                .collect();
        }
        return vec![model_visual(read_xmodel_name(s, vis, xmodel_names))];
    }
    if t == elem_type::RUNNER {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ZonePtr::Offset(arr)) = ptr else {
                return (0..visual_count).map(|_| OwnedFxVisual::None).collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| {
                    runner_visual(read_name_field(
                        s,
                        arr.at(i * s.layout(sz::FX_ELEM_VISUALS, 8)),
                        0,
                    ))
                })
                .collect();
        }
        return vec![runner_visual(read_name_field(s, vis, 0))];
    }
    if t == elem_type::SOUND {
        if visual_count > 1 {
            let ptr = s.ptr_at(vis, 0);
            let Ok(ZonePtr::Offset(arr)) = ptr else {
                return (0..visual_count).map(|_| OwnedFxVisual::None).collect();
            };
            let arr = s.resolve_alias(arr);
            return (0..visual_count)
                .map(|i| {
                    sound_visual(read_name_field(
                        s,
                        arr.at(i * s.layout(sz::FX_ELEM_VISUALS, 8)),
                        0,
                    ))
                })
                .collect();
        }
        return vec![sound_visual(read_name_field(s, vis, 0))];
    }
    vec![OwnedFxVisual::None]
}

pub fn runner_visual(name: String) -> OwnedFxVisual {
    let (edge, hint) = capture_named_child(name);
    OwnedFxVisual::Runner { edge, hint }
}

pub fn sound_visual(name: String) -> OwnedFxVisual {
    let hint = Some(name).filter(|name| !name.is_empty());
    match hint {
        Some(hint) => OwnedFxVisual::Sound { hint: Some(hint) },
        None => OwnedFxVisual::None,
    }
}

pub fn model_visual(name: Option<String>) -> OwnedFxVisual {
    let hint = name.map(|name| crate::AssetRef::bare_name(&name).to_owned());
    match hint.filter(|n| !n.is_empty()) {
        Some(hint) => OwnedFxVisual::Model {
            edge: AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
            hint: Some(hint),
        },
        None => OwnedFxVisual::None,
    }
}

fn model_hint_edge(
    ns: crate::FamilyId,
    hint: Option<&str>,
    models: &crate::FxModelCatalog,
) -> FxElemModelEdge {
    match hint.filter(|name| !name.is_empty()) {
        None => AssetEdge::Absent,
        Some(name) => match models.index_in(ns, name) {
            Some(index) => AssetEdge::bind_order(index, models.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn model_phys_piece(
    vis: &OwnedFxVisual,
    models: &crate::FxModelCatalog,
) -> Option<fx_iw4::FxPhysPiece> {
    let OwnedFxVisual::Model { edge, .. } = vis else {
        return None;
    };
    let (mid, half) = models.get_at(edge.bound_index()?)?.skel.bounds?;
    fx_iw4::FxPhysPiece::from_bounds(mid, half)
}

fn resolve_material_visual(
    s: &ZoneStream<'_>,
    materials: &MaterialCatalog,
    slot: Ptr,
) -> OwnedFxVisual {
    let alias = match s.ptr_at(slot, 0) {
        Ok(ZonePtr::Offset(target)) => Some(s.resolve_alias(target)),
        _ => None,
    };
    let index = materials
        .material_index(slot)
        .or_else(|| alias.and_then(|target| materials.material_index(target)));
    let name = index
        .and_then(|i| materials.materials.get(i.get()).map(|m| m.name.to_string()))
        .filter(|n| !n.is_empty());

    OwnedFxVisual::Material {
        material: FxElemMaterial::unresolved_for(Some(slot), alias),
        hint: name,
        material_namespace: materials.capture_ns(),
        authored: AuthoredRef::from_ptrs(Some(slot), alias),
    }
}

fn remap_elem_material(
    material: &mut FxElemMaterial,
    hint: &mut Option<String>,
    namespace: crate::FamilyId,
    authored: AuthoredRef,
    materials: &crate::MaterialDefinitions,
) {
    let index = hint
        .as_deref()
        .and_then(|name| materials.material_index_by_ns(namespace, name));
    if let Some(index) = index.filter(|i| {
        materials
            .materials
            .get(i.order())
            .is_some_and(|m| m.name.is_real() && !m.name.is_empty())
    }) {
        if hint.as_ref().is_none_or(|n| n.is_empty()) {
            *hint = Some(materials.materials[index.order()].name.to_string());
        }
        *material = FxElemMaterial::bind(index, materials.zone_of(index.order()));
    } else {
        *material = authored.unresolved();
    }
}

pub fn mark_pair(a: OwnedFxVisual, b: OwnedFxVisual) -> OwnedFxVisual {
    let peel = |v: OwnedFxVisual| match v {
        OwnedFxVisual::Material {
            material,
            hint,
            material_namespace,
            authored,
        } => (material, hint, material_namespace, authored),
        _ => (
            FxElemMaterial::Absent,
            None,
            crate::FamilyId::IW4,
            AuthoredRef::default(),
        ),
    };
    let (m0, h0, ns0, v0) = peel(a);
    let (m1, h1, ns1, v1) = peel(b);
    OwnedFxVisual::Mark {
        materials: [m0, m1],
        hints: [h0, h1],
        material_namespaces: [ns0, ns1],
        authored: [v0, v1],
    }
}

fn material_has_decoded_color(materials: &MaterialDefinitions, mat_i: usize) -> bool {
    let Some(mat) = materials.materials.get(mat_i) else {
        return false;
    };
    let img_i = mat
        .textures
        .iter()
        .find(|t| t.semantic == TS_COLOR_MAP)
        .and_then(|t| t.image)
        .or_else(|| {
            mat.textures
                .iter()
                .find(|t| t.semantic == TS_2D)
                .and_then(|t| t.image)
        });
    img_i
        .and_then(|i| materials.images.get(i))
        .is_some_and(|img| img.decoded.is_some())
}

pub fn color_decoded_in_catalog(materials: &MaterialDefinitions, material: usize) -> bool {
    material_has_decoded_color(materials, material)
}

fn read_xmodel_name(
    s: &ZoneStream<'_>,
    slot: Ptr,
    xmodel_names: &HashMap<Ptr, Ptr>,
) -> Option<String> {
    if let Some(name) = xmodel_names.get(&slot).copied() {
        return s.cstr(name).ok().map(str::to_owned);
    }
    let ZonePtr::Offset(body) = s.ptr_at(slot, 0).ok()? else {
        return None;
    };
    let body = s.resolve_alias(body);
    if let Some(name) = xmodel_names.get(&body).copied() {
        return s.cstr(name).ok().map(str::to_owned);
    }
    match s.ptr_at(body, 0).ok()? {
        ZonePtr::Offset(name) => s.cstr(s.resolve_alias(name)).ok().map(str::to_owned),
        _ => s.cstr(body).ok().map(str::to_owned),
    }
}

pub fn capture_named_child(name: String) -> (FxChildEdge, Option<String>) {
    if name.is_empty() {
        (FxChildEdge::Absent, None)
    } else {
        (
            FxChildEdge::Unresolved(AssetEdgeReason::CatalogMiss),
            Some(name),
        )
    }
}

fn remap_nested_child(
    edge: &mut FxChildEdge,
    hint: Option<&str>,
    playable: impl Fn(&str) -> Option<(usize, ZoneOwner)>,
) {
    let Some(name) = hint.filter(|n| !n.is_empty()) else {
        *edge = FxChildEdge::Absent;
        return;
    };
    *edge = match playable(name) {
        Some((index, zone)) => FxChildEdge::bind_order(index, zone),
        None => FxChildEdge::Unresolved(AssetEdgeReason::CatalogMiss),
    };
}

fn lookup_playable_fx(
    playable: &HashMap<(crate::FamilyId, String), (usize, ZoneOwner)>,
    ns: crate::FamilyId,
    name: &str,
) -> Option<(usize, ZoneOwner)> {
    let key = ascii_lower(name);
    let bind = ascii_lower(material_bind_name(name));
    playable
        .get(&(ns, key))
        .or_else(|| playable.get(&(ns, bind)))
        .copied()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxName<'a> {
    pub namespace: crate::FamilyId,
    pub name: &'a str,
}

impl<'a> FxName<'a> {
    pub fn new(namespace: crate::FamilyId, name: &'a str) -> Self {
        Self { namespace, name }
    }

    pub fn engine(name: &'a str) -> Self {
        Self::new(crate::FamilyId::IW4, name)
    }

    pub fn resolve(self, catalog: &'a FxDefinitions) -> Option<&'a OwnedFxEffectDef> {
        catalog.resolve_def_in(self.namespace, self.name)
    }
}

impl std::fmt::Display for FxName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

fn read_name_field(s: &ZoneStream<'_>, p: Ptr, field: usize) -> String {
    match s.ptr_at(p, field) {
        Ok(ZonePtr::Offset(name)) => s.cstr(s.resolve_alias(name)).ok().unwrap_or("").to_owned(),
        _ => String::new(),
    }
}

fn copy_ptr_array(
    s: &ZoneStream<'_>,
    parent: Ptr,
    field: usize,
    count: usize,
    stride: usize,
) -> Vec<u8> {
    if count == 0 {
        return Vec::new();
    }
    let Ok(ZonePtr::Offset(arr)) = s.ptr_at(parent, field) else {
        return Vec::new();
    };
    let arr = s.resolve_alias(arr);
    let nbytes = count.saturating_mul(stride);
    s.slice_at(arr, 0, nbytes)
        .map(|b| b.to_vec())
        .unwrap_or(Vec::new())
}

pub fn parse_vel_graph_channel(
    bytes: &[u8],
    fenceposts: usize,
    world: bool,
) -> Vec<fx_iw4::FxElemVec3Range> {
    const STRIDE: usize = 0x60;
    let base = if world { 0x30 } else { 0 };
    let mut out = Vec::with_capacity(fenceposts);
    for i in 0..fenceposts {
        let off = i * STRIDE + base;
        let Some(range) = bytes.get(off..off + 24) else {
            break;
        };
        let read =
            |at: usize| f32::from_le_bytes(range[at..at + 4].try_into().expect("range extent"));
        out.push(fx_iw4::FxElemVec3Range {
            base: core::array::from_fn(|axis| read(axis * 4)),
            amplitude: core::array::from_fn(|axis| read(12 + axis * 4)),
        });
    }
    out
}

fn ascii_lower(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                c
            }
        })
        .collect()
}

pub fn pack_fx_effect_header(view: &FxEffectDefView) -> Vec<u8> {
    let mut raw = vec![0u8; FX_EFFECT_DEF_SIZE];
    raw[0x04..0x08].copy_from_slice(&view.flags.to_le_bytes());
    raw[0x0c..0x10].copy_from_slice(&view.msec_looping_life.to_le_bytes());
    raw[0x10..0x14].copy_from_slice(&view.looping_count.to_le_bytes());
    raw[0x14..0x18].copy_from_slice(&view.one_shot_count.to_le_bytes());
    raw[0x18..0x1c].copy_from_slice(&view.emission_count.to_le_bytes());
    raw
}

pub fn pack_fx_elem_raw(view: &FxElemDefView, atlas: [u8; 8]) -> Vec<u8> {
    let mut raw = vec![0u8; FX_ELEM_DEF_STRIDE];
    let put_i32 = |raw: &mut [u8], off: usize, v: i32| {
        raw[off..off + 4].copy_from_slice(&v.to_le_bytes());
    };
    let put_f32 = |raw: &mut [u8], off: usize, v: f32| {
        raw[off..off + 4].copy_from_slice(&v.to_le_bytes());
    };
    put_i32(&mut raw, 0x00, view.flags);
    put_i32(&mut raw, 0x04, view.spawn_a);
    put_i32(&mut raw, 0x08, view.spawn_b);
    put_f32(&mut raw, 0x0c, view.spawn_range_base);
    put_f32(&mut raw, 0x10, view.spawn_range_amplitude);
    put_f32(&mut raw, 0x14, view.fade_in_range[0]);
    put_f32(&mut raw, 0x18, view.fade_in_range[1]);
    put_f32(&mut raw, 0x1c, view.fade_out_range[0]);
    put_f32(&mut raw, 0x20, view.fade_out_range[1]);
    put_f32(&mut raw, 0x24, view.spawn_frustum_cull_radius);
    put_i32(&mut raw, 0x28, view.spawn_delay_msec_base);
    put_i32(&mut raw, 0x2c, view.spawn_delay_msec_amplitude);
    put_i32(&mut raw, 0x30, view.life_span_msec_base);
    put_i32(&mut raw, 0x34, view.life_span_msec_amplitude);
    for i in 0..3 {
        put_f32(&mut raw, 0x38 + i * 8, view.spawn_origin[i][0]);
        put_f32(&mut raw, 0x3c + i * 8, view.spawn_origin[i][1]);
    }
    put_f32(&mut raw, 0x50, view.spawn_offset_radius_base);
    put_f32(&mut raw, 0x54, view.spawn_offset_radius_amplitude);
    put_f32(&mut raw, 0x58, view.spawn_offset_height_base);
    put_f32(&mut raw, 0x5c, view.spawn_offset_height_amplitude);
    for i in 0..3 {
        put_f32(&mut raw, 0x60 + i * 8, view.spawn_angles[i][0]);
        put_f32(&mut raw, 0x64 + i * 8, view.spawn_angles[i][1]);
    }
    for i in 0..3 {
        put_f32(&mut raw, 0x78 + i * 8, view.angular_velocity[i][0]);
        put_f32(&mut raw, 0x7c + i * 8, view.angular_velocity[i][1]);
    }
    put_f32(&mut raw, 0x90, view.initial_rotation[0]);
    put_f32(&mut raw, 0x94, view.initial_rotation[1]);
    put_f32(&mut raw, 0x98, view.gravity_base);
    put_f32(&mut raw, 0x9c, view.gravity_amplitude);
    put_f32(&mut raw, 0xa0, view.reflection_factor[0]);
    put_f32(&mut raw, 0xa4, view.reflection_factor[1]);
    raw[0xa8..0xb0].copy_from_slice(&atlas);
    raw[0xb0] = view.elem_type;
    raw[0xb1] = view.visual_count;
    raw[0xb2] = view.vel_interval_count;
    raw[0xb3] = view.vis_state_interval_count;
    for i in 0..3 {
        put_f32(&mut raw, 0xc0 + i * 4, view.coll_mins[i]);
        put_f32(&mut raw, 0xcc + i * 4, view.coll_maxs[i]);
    }
    put_f32(&mut raw, 0xe4, view.emit_dist[0]);
    put_f32(&mut raw, 0xe8, view.emit_dist[1]);
    put_f32(&mut raw, 0xec, view.emit_dist_variance[0]);
    put_f32(&mut raw, 0xf0, view.emit_dist_variance[1]);
    raw[0xf8] = view.sort_order;
    raw[0xf9] = view.lighting_frac;
    raw[0xfa] = view.use_item_clip;
    raw
}
