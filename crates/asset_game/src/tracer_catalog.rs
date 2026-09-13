use std::collections::HashMap;

use crate::asset_graph::{AssetEdgeCensus, AssetEdgeFromPtrs, AssetEdgeReason, ZoneOwner};
use fastfile_iw4::{Ptr, Result, TracerDefGeometry, ZoneStream};

pub type TracerMaterial = crate::asset_graph::AssetEdge<crate::asset_graph::MaterialSpace>;

pub type TracerMaterialReason = AssetEdgeReason;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OwnedTracerDef {
    pub name: String,
    pub material: TracerMaterial,

    pub material_hint: Option<String>,

    /// Whether the zone named a material for this tracer, and whether it did so
    /// through an alias. The pointers that answered it stop at the walk.
    pub material_authored: crate::AuthoredRef,
    pub draw_interval: u32,
    pub speed: f32,
    pub beam_length: f32,
    pub beam_width: f32,
    pub screw_radius: f32,
    pub screw_dist: f32,
    pub colors: [[f32; 4]; 5],
}

impl OwnedTracerDef {
    pub fn material_report(&self) -> &str {
        match self.material {
            TracerMaterial::Bound(_) => self.material_hint.as_deref().unwrap_or("bound"),
            TracerMaterial::Unresolved(_) | TracerMaterial::Absent => self.material.report(),
        }
    }

    pub fn present_name(&self) -> Option<&str> {
        self.material
            .is_bound()
            .then(|| self.material_hint.as_deref())
            .flatten()
    }

    pub fn decode_hint(&self) -> Option<&str> {
        self.material_hint
            .as_deref()
            .filter(|name| !name.is_empty())
    }
}

#[derive(Clone, Debug)]
enum TracerLink {
    Direct(String),
    Alias(Ptr),
}

/// The tracers a build finished with: named, material-bound, and with no zone
/// link map to resolve one more pointer against.
#[derive(Clone, Debug, Default)]
pub struct TracerDefinitions {
    by_name: HashMap<String, OwnedTracerDef>,
    order: Vec<String>,
    zones: Vec<ZoneOwner>,
    capture_zone: ZoneOwner,
    pub capture_gaps: usize,
}

/// The build, holding the definitions plus what the walk needs to add to them.
#[derive(Clone, Debug, Default)]
pub struct TracerCatalog {
    published: TracerDefinitions,
    links: HashMap<Ptr, TracerLink>,
    last_captured: Option<String>,
}

impl std::ops::Deref for TracerCatalog {
    type Target = TracerDefinitions;

    fn deref(&self) -> &Self::Target {
        &self.published
    }
}

impl std::ops::DerefMut for TracerCatalog {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.published
    }
}

impl TracerCatalog {
    pub fn note_loaded(&mut self, slot: Ptr, insert_slot: Option<Ptr>) {
        let Some(name) = self.last_captured.take() else {
            return;
        };
        self.links.insert(slot, TracerLink::Direct(name.clone()));
        if let Some(insert_slot) = insert_slot {
            self.links.insert(insert_slot, TracerLink::Direct(name));
        }
    }

    pub fn note_alias(&mut self, slot: Ptr, target: Ptr) {
        self.links.insert(slot, TracerLink::Alias(target));
    }

    pub fn name_at_slot(&self, slot: Ptr) -> Option<&str> {
        let mut cur = slot;
        for _ in 0..8 {
            match self.links.get(&cur)? {
                TracerLink::Direct(name) => return Some(name.as_str()),
                TracerLink::Alias(next) => cur = *next,
            }
        }
        None
    }

    pub fn bind_last_material(&mut self, name: String) {
        if name.is_empty() {
            return;
        }
        let Some(key) = self.last_captured.clone() else {
            return;
        };
        if let Some(def) = self.by_name.get_mut(&key) {
            def.material_hint = Some(name);
        }
    }

    /// The zone reached this tracer's material through an alias. Which pointer
    /// it was does not survive the walk; that it was one does.
    pub fn note_last_material_alias(&mut self) {
        let Some(key) = self.last_captured.clone() else {
            return;
        };
        if let Some(def) = self.by_name.get_mut(&key) {
            def.material_authored.alias = true;
            if !def.material.is_bound() {
                def.material = def.material_authored.unresolved();
            }
        }
    }

    pub fn capture(&mut self, s: &ZoneStream<'_>, geometry: TracerDefGeometry) -> Result<()> {
        let name = match geometry.name {
            Some(ptr) => s.cstr(ptr).unwrap_or("").to_owned(),
            None => String::new(),
        };
        if name.is_empty() {
            self.capture_gaps += 1;
            self.last_captured = None;
            return Ok(());
        }
        self.last_captured = Some(name.clone());
        self.retain_order(name.clone());
        let captured_name = geometry.material_name.and_then(|ptr| {
            s.cstr(ptr)
                .ok()
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
        });
        let material = TracerMaterial::from_capture(geometry.material_slot, None);
        self.by_name.insert(
            name.clone(),
            OwnedTracerDef {
                name,
                material,
                material_hint: captured_name,
                material_authored: crate::AuthoredRef::from_ptrs(geometry.material_slot, None),
                draw_interval: geometry.draw_interval,
                speed: geometry.speed,
                beam_length: geometry.beam_length,
                beam_width: geometry.beam_width,
                screw_radius: geometry.screw_radius,
                screw_dist: geometry.screw_dist,
                colors: geometry.colors,
            },
        );
        Ok(())
    }

    /// Ends the build: the tracer definitions travel on without the link map.
    pub fn publish(self) -> TracerDefinitions {
        self.published
    }
}

impl TracerDefinitions {
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&OwnedTracerDef> {
        self.by_name.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(String::as_str)
    }

    pub fn resolve_materials(&mut self, materials: &crate::MaterialDefinitions) {
        for def in self.by_name.values_mut() {
            let index = def
                .material_hint
                .as_deref()
                .and_then(|name| materials.material_index_by_name(name));
            if let Some(index) = index.filter(|i| {
                materials
                    .materials
                    .get(i.order())
                    .is_some_and(|m| m.name.is_real() && !m.name.is_empty())
            }) {
                if def.material_hint.as_ref().is_none_or(|n| n.is_empty()) {
                    def.material_hint = Some(materials.materials[index.order()].name.to_string());
                }
                def.material = TracerMaterial::bind(index, materials.zone_of(index.order()));
            } else {
                def.material = def.material_authored.unresolved();
            }
        }
    }

    pub fn defs(&self) -> impl Iterator<Item = &OwnedTracerDef> {
        self.order.iter().filter_map(|name| self.by_name.get(name))
    }

    pub fn index_by_name(&self, name: &str) -> Option<usize> {
        self.order.iter().position(|n| n == name)
    }

    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn zone_of(&self, index: usize) -> ZoneOwner {
        self.zones.get(index).copied().unwrap_or(self.capture_zone)
    }

    fn retain_order(&mut self, name: String) {
        if let Some(pos) = self.order.iter().position(|n| n == &name) {
            self.zones[pos] = self.capture_zone;
        } else {
            self.order.push(name);
            self.zones.push(self.capture_zone);
        }
    }

    pub fn def_at(&self, index: usize) -> Option<&OwnedTracerDef> {
        self.order
            .get(index)
            .and_then(|name| self.by_name.get(name))
    }

    pub fn material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for def in self.by_name.values() {
            census.push(def.material);
        }
        census
    }

    pub fn bound_count(&self) -> usize {
        self.by_name
            .values()
            .filter(|def| def.material.is_bound())
            .count()
    }

    pub fn named_materials(&self) -> impl Iterator<Item = &str> {
        self.by_name
            .values()
            .filter_map(OwnedTracerDef::decode_hint)
    }

    pub fn unresolved_count(&self) -> usize {
        self.by_name
            .values()
            .filter(|def| def.material.is_unresolved())
            .count()
    }
}
