use std::collections::HashMap;

use fastfile_iw4::{ScriptStrings, ZoneStream};

use crate::asset_graph::{
    AssetEdge, AssetEdgeCensus, capture_xmodel_material_slots, stamp_xmodel_material_edges,
};
use crate::model_skel::{FpvSkel, capture_fpv_skel, capture_fpv_skel_iw5, capture_fpv_skel_t5};
use crate::{ModelKind, model_kind};
use asset_core::AssetNamespace;
use asset_material::MaterialCatalog;

pub const VIEWHANDS_NAME: &str = "viewmodel_base_viewhands";

pub const VIEWHANDS_NAME_T5: &str = "viewhands_usmc";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FpvMeshKey {
    pub namespace: AssetNamespace,
    pub name: String,
}

impl FpvMeshKey {
    pub fn new(namespace: AssetNamespace, name: &str) -> Self {
        Self {
            namespace,
            name: ascii_lower(name),
        }
    }

    pub fn display(&self) -> String {
        format!("{}:xmodel/{}", self.namespace.as_str(), self.name)
    }
}

const fn game_default_hands_name(ns: AssetNamespace) -> &'static str {
    match ns {
        AssetNamespace::Iw4 | AssetNamespace::Iw5 => VIEWHANDS_NAME,
        AssetNamespace::T5 => VIEWHANDS_NAME_T5,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FpvHands {
    FromKit {
        namespace: AssetNamespace,
        name: String,
    },

    FromWeaponDef {
        namespace: AssetNamespace,
        name: String,
    },

    GameDefault {
        namespace: AssetNamespace,
        name: String,
    },
    Unresolved,
}

impl FpvHands {
    pub fn key(&self) -> Option<(AssetNamespace, &str)> {
        match self {
            Self::FromKit { namespace, name }
            | Self::FromWeaponDef { namespace, name }
            | Self::GameDefault { namespace, name } => Some((*namespace, name.as_str())),
            Self::Unresolved => None,
        }
    }

    pub fn game_default(ns: AssetNamespace) -> Self {
        Self::GameDefault {
            namespace: ns,
            name: game_default_hands_name(ns).to_owned(),
        }
    }

    pub fn resolve(
        catalog: &FpvMeshCatalog,
        map_ns: AssetNamespace,
        kit: Option<&crate::SoldierKit>,
        weapon_hand: Option<&str>,
        weapon_ns: AssetNamespace,
    ) -> Self {
        if let Some(kit) = kit {
            if let Some(name) = kit.arms.as_deref() {
                if catalog.contains(map_ns, name) {
                    return Self::FromKit {
                        namespace: map_ns,
                        name: name.to_owned(),
                    };
                }
            }
            let arm_names: Vec<&str> = catalog
                .names_in(map_ns)
                .filter(|n| crate::is_arms_model(n))
                .collect();
            if let Some(name) = crate::arms_for_body(&kit.body, &arm_names) {
                if catalog.contains(map_ns, &name) {
                    return Self::FromKit {
                        namespace: map_ns,
                        name,
                    };
                }
            }
        }
        if let Some(name) = weapon_hand {
            if weapon_ns == map_ns && catalog.contains(weapon_ns, name) {
                return Self::FromWeaponDef {
                    namespace: weapon_ns,
                    name: name.to_owned(),
                };
            }
        }
        let def = Self::game_default(map_ns);
        if let Some((ns, name)) = def.key() {
            if catalog.contains(ns, name) {
                return def;
            }
        }
        Self::Unresolved
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TagViewBind {
    pub quat: [f32; 4],
    pub trans: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct FpvMeshEntry {
    pub namespace: AssetNamespace,
    pub skel: FpvSkel,

    pub material_names: Vec<Option<String>>,

    pub material_edges: Vec<AssetEdge<crate::MaterialSpace>>,
}

impl FpvMeshEntry {
    fn from_skel(
        namespace: AssetNamespace,
        skel: FpvSkel,
        materials: Option<&MaterialCatalog>,
    ) -> Self {
        let (material_names, material_edges) =
            capture_xmodel_material_slots(&skel.surface_materials, materials);
        Self {
            namespace,
            skel,
            material_names,
            material_edges,
        }
    }

    pub fn key(&self) -> FpvMeshKey {
        FpvMeshKey::new(self.namespace, &self.skel.name)
    }

    pub fn resolve_materials(&mut self, materials: &MaterialCatalog) {
        stamp_xmodel_material_edges(
            &mut self.material_names,
            &mut self.material_edges,
            &self.skel.surface_materials,
            materials,
        );
    }

    pub fn material_present_name(&self, surface: usize) -> Option<&str> {
        self.material_edges
            .get(surface)?
            .is_bound()
            .then(|| self.material_names.get(surface)?.as_deref())
            .flatten()
    }

    pub fn tag_view(&self) -> Option<TagViewBind> {
        let i = self.skel.tag_view?;
        let b = self.skel.bones.get(i)?;
        Some(TagViewBind {
            quat: b.quat,
            trans: b.trans,
        })
    }

    pub fn tag_weapon(&self) -> Option<TagViewBind> {
        let i = self.skel.tag_weapon?;
        let b = self.skel.bones.get(i)?;
        Some(TagViewBind {
            quat: b.quat,
            trans: b.trans,
        })
    }

    pub fn bone_count(&self) -> usize {
        self.skel.bones.len()
    }
}

#[derive(Clone, Debug)]
pub struct FpvMeshCatalog {
    entries: HashMap<FpvMeshKey, FpvMeshEntry>,
    order: Vec<FpvMeshKey>,

    zones: Vec<crate::ZoneOwner>,
    capture_zone: crate::ZoneOwner,
    strings: ScriptStrings,
    capture_ns: AssetNamespace,

    pub map_namespace: Option<AssetNamespace>,

    pub materials: MaterialCatalog,
}

impl Default for FpvMeshCatalog {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
            zones: Vec::new(),
            capture_zone: crate::ZoneOwner::default(),
            strings: ScriptStrings::default(),
            capture_ns: AssetNamespace::Iw4,
            map_namespace: None,
            materials: MaterialCatalog::default(),
        }
    }
}

impl FpvMeshCatalog {
    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn set_capture_ns(&mut self, ns: AssetNamespace) {
        self.capture_ns = ns;
    }

    pub fn set_capture_zone(&mut self, zone: crate::ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn index_by_name(&self, ns: AssetNamespace, name: &str) -> Option<usize> {
        let key = FpvMeshKey::new(ns, name);
        self.order.iter().position(|k| k == &key)
    }

    pub fn zone_of(&self, index: usize) -> crate::ZoneOwner {
        self.zones.get(index).copied().unwrap_or_default()
    }

    pub fn get_at(&self, index: usize) -> Option<&FpvMeshEntry> {
        let key = self.order.get(index)?;
        self.entries.get(key)
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.order.get(index).map(|k| k.name.as_str())
    }

    pub fn capture(&mut self, stream: &ZoneStream<'_>, materials: &MaterialCatalog) {
        let Some(geometry) = stream.xmodel() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if model_kind(name) != Some(ModelKind::Fpv) {
            return;
        }
        let Some(skel) = capture_fpv_skel(stream, &self.strings, geometry, Some(materials)) else {
            return;
        };
        self.insert_captured(skel, Some(materials));
    }

    pub fn capture_t5(
        &mut self,
        stream: &fastfile_t5::ZoneStream<'_>,
        strings: &fastfile_t5::ScriptStrings,
        materials: &MaterialCatalog,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if model_kind(name) != Some(ModelKind::Fpv) {
            return;
        }
        let Some(skel) = capture_fpv_skel_t5(stream, strings, geometry, Some(materials)) else {
            return;
        };
        self.insert_in(AssetNamespace::T5, skel, Some(materials));
    }

    pub fn capture_iw5(
        &mut self,
        stream: &fastfile_iw5::ZoneStream<'_>,
        strings: &fastfile_iw5::ScriptStrings,
        materials: &MaterialCatalog,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if model_kind(name) != Some(ModelKind::Fpv) {
            return;
        }
        let Some(skel) = capture_fpv_skel_iw5(stream, strings, geometry, Some(materials)) else {
            return;
        };
        self.insert_in(AssetNamespace::Iw5, skel, Some(materials));
    }

    pub fn insert_captured(&mut self, skel: FpvSkel, materials: Option<&MaterialCatalog>) {
        self.insert_in(self.capture_ns, skel, materials);
    }

    pub fn insert_in(
        &mut self,
        ns: AssetNamespace,
        skel: FpvSkel,
        materials: Option<&MaterialCatalog>,
    ) {
        let entry = FpvMeshEntry::from_skel(ns, skel, materials);
        self.retain(entry.key(), entry);
    }

    fn retain(&mut self, key: FpvMeshKey, entry: FpvMeshEntry) {
        if let Some(pos) = self.order.iter().position(|k| k == &key) {
            self.zones[pos] = self.capture_zone;
        } else {
            self.order.push(key.clone());
            self.zones.push(self.capture_zone);
        }
        self.entries.insert(key, entry);
    }

    pub fn absorb(&mut self, other: FpvMeshCatalog) -> usize {
        let saved = self.capture_zone;
        let mut added = 0;
        for (i, key) in other.order.iter().enumerate() {
            let Some(entry) = other.entries.get(key).cloned() else {
                continue;
            };
            self.capture_zone = other.zones.get(i).copied().unwrap_or(other.capture_zone);
            let vacant = !self.entries.contains_key(key);
            self.retain(key.clone(), entry);
            if vacant {
                added += 1;
            }
        }
        self.capture_zone = saved;
        added
    }

    pub fn resolve_materials(&mut self, materials: &MaterialCatalog) {
        for entry in self.entries.values_mut() {
            entry.resolve_materials(materials);
        }
    }

    pub fn material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in self.entries.values() {
            for edge in &entry.material_edges {
                census.push(*edge);
            }
        }
        census
    }

    pub fn material_unresolved_hints(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for entry in self.entries.values() {
            for (edge, name) in entry.material_edges.iter().zip(entry.material_names.iter()) {
                if edge.is_unresolved()
                    && let Some(name) = name.as_deref()
                    && !name.is_empty()
                    && !names.contains(&name)
                {
                    names.push(name);
                }
            }
        }
        names.sort_unstable();
        names
    }

    pub fn material_bound_zones(&self) -> String {
        crate::bound_zone_names(
            self.entries
                .values()
                .flat_map(|entry| entry.material_edges.iter()),
        )
    }

    pub fn get(&self, ns: AssetNamespace, name: &str) -> Option<&FpvMeshEntry> {
        self.entries.get(&FpvMeshKey::new(ns, name))
    }

    pub fn bound_material_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries.values().flat_map(|entry| {
            entry
                .material_edges
                .iter()
                .filter_map(|edge| edge.bound_index())
        })
    }

    pub fn hands(&self) -> Option<&FpvMeshEntry> {
        self.hands_in(AssetNamespace::Iw4)
    }

    pub fn hands_in(&self, ns: AssetNamespace) -> Option<&FpvMeshEntry> {
        self.get_hands(&FpvHands::game_default(ns))
    }

    pub fn get_hands(&self, hands: &FpvHands) -> Option<&FpvMeshEntry> {
        let (ns, name) = hands.key()?;
        self.get(ns, name)
    }

    pub fn names_in(&self, ns: AssetNamespace) -> impl Iterator<Item = &str> {
        self.order
            .iter()
            .filter(move |k| k.namespace == ns)
            .map(|k| k.name.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, ns: AssetNamespace, name: &str) -> bool {
        self.entries.contains_key(&FpvMeshKey::new(ns, name))
    }

    pub fn namespace_count(&self, ns: AssetNamespace) -> usize {
        self.order.iter().filter(|k| k.namespace == ns).count()
    }

    pub fn collide_name_count(&self) -> usize {
        let mut seen: HashMap<&str, u8> = HashMap::new();
        for k in &self.order {
            *seen.entry(k.name.as_str()).or_insert(0) |= match k.namespace {
                AssetNamespace::Iw4 => 1,
                AssetNamespace::T5 => 2,
                AssetNamespace::Iw5 => 4,
            };
        }
        seen.values().filter(|bits| bits.count_ones() >= 2).count()
    }

    pub fn peer(&self, ns: AssetNamespace, name: &str) -> Option<&FpvMeshEntry> {
        const PREFER: [AssetNamespace; 3] =
            [AssetNamespace::T5, AssetNamespace::Iw5, AssetNamespace::Iw4];
        for other in PREFER {
            if other == ns {
                continue;
            }
            if let Some(entry) = self.entries.get(&FpvMeshKey::new(other, name)) {
                return Some(entry);
            }
        }
        None
    }

    pub fn tag_view_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.skel.tag_view.is_some())
            .count()
    }
}

fn ascii_lower(name: &str) -> String {
    name.to_ascii_lowercase()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PoseStats {
    pub hands_rigid: usize,
    pub hands_blend: usize,
    pub gun_rigid: usize,
    pub gun_blend: usize,
    pub idle_sampled: bool,
}
