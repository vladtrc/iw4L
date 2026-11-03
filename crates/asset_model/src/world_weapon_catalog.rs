use std::collections::HashMap;

use fastfile_iw4::{ScriptStrings, ZoneStream};

use crate::asset_graph::{
    AssetEdge, AssetEdgeCensus, capture_xmodel_material_slots, stamp_xmodel_material_edges,
};
use crate::model_skel::{ModelSkel, capture_world_weapon_skel};
use asset_core::FamilyId;
use asset_material::{MaterialCatalog, MaterialDefinitions};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorldWeaponKey {
    pub namespace: FamilyId,
    pub name: String,
}

impl WorldWeaponKey {
    pub fn new(namespace: FamilyId, name: &str) -> Self {
        Self {
            namespace,
            name: name.to_ascii_lowercase(),
        }
    }

    pub fn display(&self) -> String {
        format!("{}:xmodel/{}", self.namespace.as_str(), self.name)
    }
}

#[derive(Clone, Debug)]
pub struct WorldWeaponEntry {
    pub namespace: FamilyId,
    pub skel: std::sync::Arc<ModelSkel>,

    pub material_keys: Vec<Option<asset_core::MaterialKey>>,

    pub material_edges: Vec<AssetEdge<crate::MaterialSpace>>,
}

impl WorldWeaponEntry {
    fn from_skel(
        namespace: FamilyId,
        skel: std::sync::Arc<ModelSkel>,
        materials: Option<&MaterialCatalog>,
    ) -> Self {
        let (material_keys, material_edges) =
            capture_xmodel_material_slots(&skel.surface_materials, materials.map(|c| &**c));
        Self {
            namespace,
            skel,
            material_keys,
            material_edges,
        }
    }

    pub fn key(&self) -> WorldWeaponKey {
        WorldWeaponKey::new(self.namespace, &self.skel.name)
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

    pub fn has_tag_flash(&self) -> bool {
        self.skel.bone_names.iter().any(|n| n == "tag_flash")
    }
}

#[derive(Clone, Debug, Default)]
pub struct WorldWeaponCatalog {
    identity: u64,
    entries: Vec<WorldWeaponEntry>,
    indices: HashMap<WorldWeaponKey, usize>,
    order: Vec<WorldWeaponKey>,
    zones: Vec<crate::ZoneOwner>,
}

#[derive(Clone, Debug, Default)]
pub struct WorldWeaponBuild {
    catalog: WorldWeaponCatalog,
    capture_zone: crate::ZoneOwner,
    capture_ns: Option<FamilyId>,
    strings: ScriptStrings,
}

impl std::ops::Deref for WorldWeaponBuild {
    type Target = WorldWeaponCatalog;

    fn deref(&self) -> &Self::Target {
        &self.catalog
    }
}

impl WorldWeaponBuild {
    pub fn seal_identity(&mut self) -> u64 {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        if self.catalog.identity == 0 {
            self.catalog.identity = NEXT_ID
                .fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |next| next.checked_add(1),
                )
                .expect("world weapon product revision exhausted");
        }
        self.catalog.identity
    }

    pub fn publish(mut self) -> WorldWeaponCatalog {
        self.seal_identity();
        self.catalog
    }

    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn set_capture_zone(&mut self, zone: crate::ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn set_capture_ns(&mut self, ns: FamilyId) {
        self.capture_ns = Some(ns);
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
        self.insert_in(
            self.capture_ns
                .expect("asset capture requires an explicit family"),
            skel,
            Some(materials),
        );
    }

    pub fn insert_captured(&mut self, skel: ModelSkel, materials: Option<&MaterialCatalog>) {
        self.insert_in(
            self.capture_ns
                .expect("asset capture requires an explicit family"),
            skel,
            materials,
        );
    }

    pub fn capture_shared(
        &mut self,
        ns: Option<FamilyId>,
        skel: &std::sync::Arc<ModelSkel>,
        materials: &MaterialCatalog,
    ) {
        if crate::classify_model(
            ns.or(self.capture_ns)
                .expect("asset capture requires an explicit family"),
            &skel.name,
        ) == Some(crate::ModelKind::WorldWeapon)
        {
            self.insert_in(
                ns.unwrap_or(
                    self.capture_ns
                        .expect("asset capture requires an explicit family"),
                ),
                skel.clone(),
                Some(materials),
            );
        }
    }

    pub fn insert_in(
        &mut self,
        ns: FamilyId,
        skel: impl Into<std::sync::Arc<ModelSkel>>,
        materials: Option<&MaterialCatalog>,
    ) {
        self.catalog.identity = 0;
        let entry = WorldWeaponEntry::from_skel(ns, skel.into(), materials);
        let key = entry.key();
        self.retain(key, entry);
    }

    fn retain(&mut self, key: WorldWeaponKey, entry: WorldWeaponEntry) {
        self.catalog.identity = 0;
        if let Some(&pos) = self.catalog.indices.get(&key) {
            self.catalog.zones[pos] = self.capture_zone;
            self.catalog.entries[pos] = entry;
        } else {
            self.catalog
                .indices
                .insert(key.clone(), self.catalog.entries.len());
            self.catalog.order.push(key);
            self.catalog.zones.push(self.capture_zone);
            self.catalog.entries.push(entry);
        }
    }

    pub fn absorb(&mut self, mut other: Self) -> usize {
        self.catalog.identity = 0;
        let saved = self.capture_zone;
        let mut added = 0;
        let order = std::mem::take(&mut other.catalog.order);
        let entries = std::mem::take(&mut other.catalog.entries);
        for (i, (key, entry)) in order.into_iter().zip(entries).enumerate() {
            self.capture_zone = other
                .catalog
                .zones
                .get(i)
                .copied()
                .unwrap_or(other.capture_zone);
            let vacant = !self.catalog.indices.contains_key(&key);
            self.retain(key, entry);
            if vacant {
                added += 1;
            }
        }
        self.capture_zone = saved;
        added
    }

    pub fn resolve_materials(&mut self, materials: &MaterialDefinitions) {
        for entry in &mut self.catalog.entries {
            entry.resolve_materials(materials);
        }
    }
}

impl WorldWeaponCatalog {
    pub fn identity(&self) -> u64 {
        self.identity
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn get(&self, ns: FamilyId, name: &str) -> Option<&WorldWeaponEntry> {
        self.index_by_name(ns, name)
            .and_then(|index| self.get_at(index))
    }

    pub fn index_by_name(&self, ns: FamilyId, name: &str) -> Option<usize> {
        self.indices.get(&WorldWeaponKey::new(ns, name)).copied()
    }

    pub fn zone_of(&self, index: usize) -> crate::ZoneOwner {
        self.zones
            .get(index)
            .copied()
            .unwrap_or(asset_core::ZoneOwner::UNKNOWN)
    }

    pub fn get_at(&self, index: usize) -> Option<&WorldWeaponEntry> {
        self.entries.get(index)
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.order.get(index).map(|k| k.name.as_str())
    }

    pub fn key_at(&self, index: usize) -> Option<&WorldWeaponKey> {
        self.order.get(index)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(|k| k.name.as_str())
    }

    pub fn names_in(&self, ns: FamilyId) -> impl Iterator<Item = &str> {
        self.order
            .iter()
            .filter(move |k| k.namespace == ns)
            .map(|k| k.name.as_str())
    }

    pub fn contains(&self, ns: FamilyId, name: &str) -> bool {
        self.indices.contains_key(&WorldWeaponKey::new(ns, name))
    }

    pub fn namespace_count(&self, ns: FamilyId) -> usize {
        self.order.iter().filter(|k| k.namespace == ns).count()
    }

    pub fn collide_name_count(&self) -> usize {
        let mut seen: HashMap<&str, std::collections::BTreeSet<FamilyId>> = HashMap::new();
        for key in &self.order {
            seen.entry(key.name.as_str())
                .or_default()
                .insert(key.namespace);
        }
        seen.values().filter(|families| families.len() >= 2).count()
    }

    pub fn material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in &self.entries {
            for edge in &entry.material_edges {
                census.push(*edge);
            }
        }
        census
    }

    pub fn material_unresolved_hints(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for entry in &self.entries {
            for (edge, key) in entry.material_edges.iter().zip(entry.material_keys.iter()) {
                if edge.is_unresolved()
                    && let Some(name) = key.as_ref().map(|key| key.name.as_str())
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
                .iter()
                .flat_map(|entry| entry.material_edges.iter()),
        )
    }

    pub fn report_lines(&self) -> Vec<String> {
        let flash = self.entries.iter().filter(|e| e.has_tag_flash()).count();
        let census = self.material_edge_census();
        let names = self
            .order
            .iter()
            .map(|k| k.display())
            .collect::<Vec<_>>()
            .join(",");
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
