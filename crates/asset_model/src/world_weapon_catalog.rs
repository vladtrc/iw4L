use std::collections::HashMap;

use fastfile_iw4::{ScriptStrings, ZoneStream};

use crate::asset_graph::{
    AssetEdge, AssetEdgeCensus, capture_xmodel_material_slots, stamp_xmodel_material_edges,
};
use crate::model_skel::{ModelSkel, capture_world_weapon_skel};
use asset_material::{MaterialCatalog, MaterialDefinitions};

#[derive(Clone, Debug)]
pub struct WorldWeaponEntry {
    pub skel: ModelSkel,

    pub material_names: Vec<Option<String>>,

    pub material_edges: Vec<AssetEdge<crate::MaterialSpace>>,
}

impl WorldWeaponEntry {
    fn from_skel(skel: ModelSkel, materials: Option<&MaterialCatalog>) -> Self {
        let (material_names, material_edges) =
            capture_xmodel_material_slots(&skel.surface_materials, materials.map(|c| &**c));
        Self {
            skel,
            material_names,
            material_edges,
        }
    }

    pub(crate) fn resolve_materials(&mut self, materials: &MaterialDefinitions) {
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

    pub fn has_tag_flash(&self) -> bool {
        self.skel.bone_names.iter().any(|n| n == "tag_flash")
    }
}

#[derive(Clone, Debug, Default)]
pub struct WorldWeaponCatalog {
    entries: HashMap<String, WorldWeaponEntry>,
    order: Vec<String>,
    zones: Vec<crate::ZoneOwner>,
}

#[derive(Clone, Debug, Default)]
pub struct WorldWeaponBuild {
    catalog: WorldWeaponCatalog,
    capture_zone: crate::ZoneOwner,
    strings: ScriptStrings,
}

impl std::ops::Deref for WorldWeaponBuild {
    type Target = WorldWeaponCatalog;

    fn deref(&self) -> &Self::Target {
        &self.catalog
    }
}

impl WorldWeaponBuild {
    pub fn publish(self) -> WorldWeaponCatalog {
        self.catalog
    }

    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn set_capture_zone(&mut self, zone: crate::ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn capture(&mut self, stream: &ZoneStream<'_>, materials: &MaterialCatalog) {
        let Some(geometry) = stream.xmodel() else {
            return;
        };
        let Some(skel) =
            capture_world_weapon_skel(stream, &self.strings, geometry, Some(materials))
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
        let Some(skel) = crate::model_skel::capture_world_weapon_skel_t5(
            stream,
            strings,
            geometry,
            Some(materials),
        ) else {
            return;
        };
        self.insert_captured(skel, Some(materials));
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
        let Some(skel) = crate::model_skel::capture_world_weapon_skel_iw5(
            stream,
            strings,
            geometry,
            Some(materials),
        ) else {
            return;
        };
        self.insert_captured(skel, Some(materials));
    }

    pub fn insert_captured(&mut self, skel: ModelSkel, materials: Option<&MaterialCatalog>) {
        let name = skel.name.clone();
        let entry = WorldWeaponEntry::from_skel(skel, materials);
        if let Some(pos) = self.catalog.order.iter().position(|n| n == &name) {
            self.catalog.zones[pos] = self.capture_zone;
        } else {
            self.catalog.order.push(name.clone());
            self.catalog.zones.push(self.capture_zone);
        }
        self.catalog.entries.insert(name, entry);
    }

    pub fn absorb(&mut self, mut other: Self) -> usize {
        let mut added = 0;
        let order = std::mem::take(&mut other.catalog.order);
        for (i, name) in order.into_iter().enumerate() {
            if self.catalog.entries.contains_key(&name) {
                continue;
            }
            let Some(entry) = other.catalog.entries.remove(&name) else {
                continue;
            };
            self.catalog.order.push(name.clone());
            self.catalog.zones.push(
                other
                    .catalog
                    .zones
                    .get(i)
                    .copied()
                    .unwrap_or(other.capture_zone),
            );
            self.catalog.entries.insert(name, entry);
            added += 1;
        }
        added
    }

    pub fn resolve_materials(&mut self, materials: &MaterialDefinitions) {
        for entry in self.catalog.entries.values_mut() {
            entry.resolve_materials(materials);
        }
    }
}

impl WorldWeaponCatalog {
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&WorldWeaponEntry> {
        self.entries.get(name)
    }

    pub fn index_by_name(&self, name: &str) -> Option<usize> {
        self.order.iter().position(|n| n == name)
    }

    pub fn zone_of(&self, index: usize) -> crate::ZoneOwner {
        self.zones.get(index).copied().unwrap_or_default()
    }

    pub fn get_at(&self, index: usize) -> Option<&WorldWeaponEntry> {
        let name = self.order.get(index)?;
        self.entries.get(name)
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

    pub fn material_unresolved_hints(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for name in &self.order {
            let Some(entry) = self.entries.get(name) else {
                continue;
            };
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

    pub fn report_lines(&self) -> Vec<String> {
        let flash = self
            .order
            .iter()
            .filter_map(|name| self.entries.get(name))
            .filter(|e| e.has_tag_flash())
            .count();
        let census = self.material_edge_census();
        let names = self.order.join(",");
        vec![
            format!(
                "world weapons: {} captured ({} with tag_flash); materialHandles bound={} unresolved={} absent={}",
                self.order.len(),
                flash,
                census.bound,
                census.unresolved,
                census.absent,
            ),
            format!("world weapon names: {names}"),
        ]
    }
}
