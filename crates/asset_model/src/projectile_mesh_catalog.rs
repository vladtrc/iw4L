use std::collections::{HashMap, HashSet};

use fastfile_iw4::{ScriptStrings, ZoneStream};

use crate::{
    asset_graph::{
        AssetEdge, AssetEdgeCensus, capture_xmodel_material_slots, stamp_xmodel_material_edges,
    },
    model_kind,
    model_skel::{ModelSkel, capture_untyped_skel},
};
use asset_material::{MaterialCatalog, MaterialDefinitions};

#[derive(Clone, Debug)]
pub struct ProjectileMeshEntry {
    pub skel: std::sync::Arc<ModelSkel>,
    pub material_keys: Vec<Option<asset_core::MaterialKey>>,
    pub material_edges: Vec<AssetEdge<crate::MaterialSpace>>,
}

impl ProjectileMeshEntry {
    fn from_skel(skel: ModelSkel, materials: Option<&MaterialCatalog>) -> Self {
        let (material_keys, material_edges) =
            capture_xmodel_material_slots(&skel.surface_materials, materials.map(|c| &**c));
        Self {
            skel: std::sync::Arc::new(skel),
            material_keys,
            material_edges,
        }
    }

    pub(crate) fn resolve_materials(&mut self, materials: &MaterialDefinitions) {
        stamp_xmodel_material_edges(
            &mut self.material_keys,
            &mut self.material_edges,
            &self.skel.surface_materials,
            materials,
        );
    }

    pub fn material_present_name(&self, surface: usize) -> Option<&str> {
        self.material_edges
            .get(surface)?
            .is_bound()
            .then(|| Some(self.material_keys.get(surface)?.as_ref()?.name.as_str()))
            .flatten()
    }

    pub fn material_index(&self, surface: usize) -> Option<crate::MaterialIndex> {
        self.material_edges.get(surface)?.bound()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProjectileMeshCatalog {
    entries: HashMap<String, ProjectileMeshEntry>,
    order: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectileMeshBuild {
    catalog: ProjectileMeshCatalog,
    strings: ScriptStrings,
}

impl std::ops::Deref for ProjectileMeshBuild {
    type Target = ProjectileMeshCatalog;

    fn deref(&self) -> &Self::Target {
        &self.catalog
    }
}

impl ProjectileMeshBuild {
    pub fn publish(self) -> ProjectileMeshCatalog {
        self.catalog
    }

    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn capture_unclassified(&mut self, stream: &ZoneStream<'_>, materials: &MaterialCatalog) {
        let Some(geometry) = stream.xmodel() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if model_kind(name).is_some() {
            return;
        }
        let Some(skel) = capture_untyped_skel(stream, &self.strings, geometry, Some(materials))
        else {
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
        let Some(skel) =
            crate::model_skel::capture_xmodel_skel_t5(stream, strings, geometry, materials)
        else {
            return;
        };
        self.insert_captured(skel, Some(materials));
    }

    pub fn absorb(&mut self, mut other: Self) {
        for name in std::mem::take(&mut other.catalog.order) {
            if !self.catalog.entries.contains_key(&name) {
                if let Some(entry) = other.catalog.entries.remove(&name) {
                    self.catalog.order.push(name.clone());
                    self.catalog.entries.insert(name, entry);
                }
            }
        }
    }

    fn insert_captured(&mut self, skel: ModelSkel, materials: Option<&MaterialCatalog>) {
        let name = skel.name.clone();
        if self.catalog.entries.contains_key(&name) {
            return;
        }
        self.catalog.order.push(name.clone());
        self.catalog
            .entries
            .insert(name, ProjectileMeshEntry::from_skel(skel, materials));
    }

    pub fn keep_referenced(&mut self, hints: &HashSet<String>) {
        self.catalog.order.retain(|name| hints.contains(name));
        self.catalog.entries.retain(|name, _| hints.contains(name));
    }

    pub fn absorb_world_weapon(&mut self, entry: &crate::WorldWeaponEntry) {
        let name = entry.skel.name.clone();
        if self.catalog.entries.contains_key(&name) {
            return;
        }
        self.catalog.order.push(name.clone());
        self.catalog.entries.insert(
            name,
            ProjectileMeshEntry {
                skel: entry.skel.clone(),
                material_keys: entry.material_keys.clone(),
                material_edges: entry.material_edges.clone(),
            },
        );
    }

    pub fn resolve_materials(&mut self, materials: &MaterialDefinitions) {
        for entry in self.catalog.entries.values_mut() {
            entry.resolve_materials(materials);
        }
    }
}

impl ProjectileMeshCatalog {
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&ProjectileMeshEntry> {
        self.entries.get(name)
    }

    pub fn index_by_name(&self, name: &str) -> Option<usize> {
        self.order.iter().position(|n| n == name)
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.order.get(index).map(String::as_str)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(String::as_str)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    pub fn material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for name in &self.order {
            if let Some(entry) = self.entries.get(name) {
                for edge in &entry.material_edges {
                    census.push(*edge);
                }
            }
        }
        census
    }

    pub fn report_line(&self) -> String {
        let mut names: Vec<&str> = self.order.iter().map(String::as_str).collect();
        names.sort_unstable();
        format!(
            "common_mp projectile meshes: {} retained ({})",
            self.order.len(),
            if names.is_empty() {
                "none".to_owned()
            } else {
                names.join(",")
            }
        )
    }
}
