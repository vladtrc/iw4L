use crate::asset_graph::AssetRef;
use std::sync::Arc;

use fastfile_iw4::{AssetLinkSink, AssetType, GfxImageGeometry, Ptr, Result, ZoneStream};

mod definitions;
mod zone_link;
pub use definitions::{
    AssetPointerIdentity, AssetRefDumpCensus, AuthoredImage, AuthoredMaterial, AuthoredShader,
    AuthoredVertexDecl, CrossGameReason, CrossGameTechsetResolution, ImageVariantId,
    MaterialConstant, MaterialDefinitions, MaterialImageMemory, MaterialLinkAddress,
    MaterialTextureBinding, OwnedMaterialPass, OwnedShaderArgument, OwnedShaderRef, OwnedTechnique,
    OwnedTechniqueGraph, ShaderSourceCensus, SourceTechniqueOccupancy, StandInTextures, TS_2D,
    TS_COLOR_MAP, TS_DETAIL_MAP, TS_FUNCTION, TS_NORMAL_MAP, TS_SPECULAR_MAP, TS_T5_COLOR0_MAP,
    TS_T5_COLOR15_MAP, TS_T5_THROW_MAP, TS_WATER_MAP, TechniqueSetFacts, TechniqueTable,
    TechsetKey, TechsetResolve, VertexDeclStreamCensus, alternate_techset_name,
};
pub use zone_link::{LinkKind, ZoneLinkState};

/// The build. It owns the definitions while they are still being assembled,
/// plus the state that assembling needs; `publish` hands the definitions on and
/// drops the rest on the floor, which is the whole point of the two types.
#[derive(Clone, Debug, Default)]
pub struct MaterialCatalog {
    defs: MaterialDefinitions,

    link: ZoneLinkState,
    capture_zone: crate::asset_graph::ZoneOwner,
    capture_ns: Option<crate::FamilyId>,

    cross_zone_link: bool,
}

impl std::ops::Deref for MaterialCatalog {
    type Target = MaterialDefinitions;

    fn deref(&self) -> &Self::Target {
        &self.defs
    }
}

impl std::ops::DerefMut for MaterialCatalog {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.defs
    }
}

impl MaterialCatalog {
    pub fn mark_images_common_owned(&mut self) {
        for image in &mut self.images {
            image.common_owned = true;
        }
    }

    /// Ends the build: drops the reference rows, resolves the technique-set
    /// edges, and hands the definitions on with the remap from provisional row
    /// to final row. The link state does not travel with them — it is dropped
    /// here, along with the catalog that needed it, which is the one thing this
    /// pair of types exists to guarantee.
    pub fn publish(mut self) -> (MaterialDefinitions, Vec<Option<usize>>) {
        let remap = self.finalize_asset_population();
        (self.defs, remap)
    }
}

impl MaterialCatalog {
    pub fn links(&self) -> &ZoneLinkState {
        &self.link
    }
    pub fn links_mut(&mut self) -> &mut ZoneLinkState {
        &mut self.link
    }
    pub fn capture_zone(&self) -> crate::ZoneOwner {
        self.capture_zone
    }
    pub fn set_capture_zone(&mut self, zone: crate::asset_graph::ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn set_capture_ns(&mut self, ns: crate::FamilyId) {
        self.capture_ns = Some(ns);
    }

    pub fn capture_ns(&self) -> crate::FamilyId {
        self.capture_ns
            .expect("asset capture requires an explicit family")
    }

    /// Pointer→row lookup: this is the walk's question, and it only has an
    /// answer while the walk is still on.
    pub fn material_index(
        &self,
        slot: impl Into<AssetPointerIdentity>,
    ) -> Option<crate::WalkLocalMaterialIndex> {
        self.link
            .resolve(LinkKind::Material, slot)
            .map(crate::WalkLocalMaterialIndex::from_walk)
    }

    pub fn image_index(&self, slot: Ptr) -> Option<usize> {
        self.link.resolve(LinkKind::Image, slot)
    }

    pub fn prepare_for_next_zone(&mut self) {
        self.link.begin_zone();
        self.cross_zone_link = !self.materials.is_empty()
            || !self.images.is_empty()
            || !self.shaders.is_empty()
            || !self.vertex_decls.is_empty()
            || !self.techsets.is_empty();
        self.link_reused_materials = 0;
        self.link_reused_images = 0;
    }

    pub fn link_image(&mut self, mut incoming: AuthoredImage) -> usize {
        if let Some(index) = self.images.iter().position(|owned| {
            owned.namespace == incoming.namespace && owned.name.same_name(&incoming.name)
        }) {
            let existing = &self.images[index];
            if existing.name.is_real()
                && incoming.name.is_real()
                && existing.use_srgb_reads != incoming.use_srgb_reads
            {
                let index = self.images.len();
                self.images.push(incoming);
                return index;
            }
            self.link_reused_images = self.link_reused_images.saturating_add(1);
            let take_body = AssetRef::incoming_owns_slot(&self.images[index].name, &incoming.name)
                && !(incoming.payload.is_empty() && !self.images[index].payload.is_empty());
            if take_body {
                let mut owned = incoming;
                if owned.decoded.is_none() && owned.accepts_decoded_from(&self.images[index]) {
                    owned.take_decoded_from(&mut self.images[index]);
                }
                self.images[index] = owned;
            } else if incoming.decoded.is_some()
                && self.images[index].decoded.is_none()
                && self.images[index].accepts_decoded_from(&incoming)
            {
                self.images[index].take_decoded_from(&mut incoming);
            }
            index
        } else {
            let index = self.images.len();
            self.images.push(incoming);
            index
        }
    }

    pub fn take_image_slot(&mut self, incoming: AuthoredImage) -> usize {
        if self.cross_zone_link {
            self.link_image(incoming)
        } else {
            let index = self.images.len();
            self.images.push(incoming);
            index
        }
    }

    fn linkable_material_index(&self, incoming: &AuthoredMaterial) -> Option<usize> {
        self.materials.iter().position(|owned| {
            owned.namespace == incoming.namespace && owned.name.same_name(&incoming.name)
        })
    }

    pub fn link_material(&mut self, incoming: AuthoredMaterial) -> usize {
        let canonical = incoming.name.clone();
        if let Some(index) = self.linkable_material_index(&incoming) {
            self.link_reused_materials = self.link_reused_materials.saturating_add(1);
            if AssetRef::incoming_owns_slot(&self.materials[index].name, &canonical) {
                self.materials[index] = incoming;
            }
            index
        } else {
            let index = self.materials.len();
            self.materials.push(incoming);
            index
        }
    }

    pub fn take_material_slot(&mut self, incoming: AuthoredMaterial) -> usize {
        if self.cross_zone_link {
            self.link_material(incoming)
        } else {
            let index = self.materials.len();
            self.materials.push(incoming);
            index
        }
    }

    fn link_material_host_real_wins(&mut self, incoming: AuthoredMaterial) -> usize {
        if let Some(index) = self.linkable_material_index(&incoming)
            && self.materials[index].name.is_real()
        {
            self.link_reused_materials = self.link_reused_materials.saturating_add(1);
            index
        } else {
            self.link_material(incoming)
        }
    }

    pub fn link_shader(&mut self, incoming: AuthoredShader) -> usize {
        if let Some(index) = self.shaders.iter().position(|owned| {
            owned.namespace == incoming.namespace
                && owned.kind == incoming.kind
                && owned.name.same_name(&incoming.name)
        }) {
            let take_body = AssetRef::incoming_owns_slot(&self.shaders[index].name, &incoming.name)
                && !(incoming.program.is_empty() && !self.shaders[index].program.is_empty());
            if take_body {
                self.shaders[index] = incoming;
            }
            index
        } else {
            let index = self.shaders.len();
            self.shaders.push(incoming);
            index
        }
    }

    pub fn take_shader_slot(&mut self, incoming: AuthoredShader) -> usize {
        if self.cross_zone_link {
            self.link_shader(incoming)
        } else {
            let index = self.shaders.len();
            self.shaders.push(incoming);
            index
        }
    }

    pub fn link_vertex_decl(&mut self, incoming: AuthoredVertexDecl) -> usize {
        if let Some(index) = self.vertex_decls.iter().position(|owned| {
            owned.layout == incoming.layout
                && if incoming.name.is_empty() {
                    owned == &incoming
                } else {
                    owned.name.same_name(&incoming.name)
                }
        }) {
            let take_body =
                AssetRef::incoming_owns_slot(&self.vertex_decls[index].name, &incoming.name)
                    && !(incoming.stream_count == 0 && self.vertex_decls[index].stream_count != 0);
            if take_body {
                self.vertex_decls[index] = incoming;
            }
            index
        } else {
            let index = self.vertex_decls.len();
            self.vertex_decls.push(incoming);
            index
        }
    }

    pub fn take_vertex_decl_slot(&mut self, incoming: AuthoredVertexDecl) -> usize {
        if self.cross_zone_link && !incoming.name.is_empty() {
            self.link_vertex_decl(incoming)
        } else {
            let index = self.vertex_decls.len();
            self.vertex_decls.push(incoming);
            index
        }
    }

    fn shader_identity_complete(technique: &OwnedTechnique) -> bool {
        technique
            .passes
            .iter()
            .all(|pass| pass.vertex_shader.shader.is_some() && pass.pixel_shader.shader.is_some())
    }

    fn same_techset_identity(owned: &TechniqueSetFacts, incoming: &TechniqueSetFacts) -> bool {
        owned.namespace == incoming.namespace && owned.name.same_name(&incoming.name)
    }

    pub fn link_techset(&mut self, mut facts: TechniqueSetFacts) -> usize {
        if let Some(index) = self
            .techsets
            .iter()
            .position(|owned| Self::same_techset_identity(owned, &facts))
        {
            let existing_occ = self.techsets[index].source_occupancy.clone();
            let merged_occ = merge_source_occupancy(facts.source_occupancy.clone(), existing_occ);
            if let Some(mut incoming) = facts.table.take() {
                if let Some(existing) = &self.techsets[index].table {
                    incoming.slots |= existing.slots;
                    incoming.scanned |= existing.scanned;
                    match (&mut incoming.graph, &existing.graph) {
                        (Some(incoming_graph), Some(existing_graph)) => {
                            if incoming_graph.slots.len() < existing_graph.slots.len() {
                                incoming_graph
                                    .slots
                                    .resize(existing_graph.slots.len(), None);
                            }
                            for (slot, existing_technique) in
                                existing_graph.slots.iter().enumerate()
                            {
                                let incoming_lost_shader_identity = incoming_graph.slots[slot]
                                    .as_ref()
                                    .is_some_and(|incoming_technique| {
                                        !Self::shader_identity_complete(incoming_technique)
                                            && existing_technique.as_ref().is_some_and(|existing| {
                                                Self::shader_identity_complete(existing)
                                            })
                                    });
                                if incoming_graph.slots[slot].is_none()
                                    || incoming_lost_shader_identity
                                {
                                    incoming_graph.slots[slot] = existing_technique.clone();
                                }
                            }
                        }
                        (None, Some(existing_graph)) => {
                            incoming.graph = Some(existing_graph.clone());
                        }
                        _ => {}
                    }
                }
                let existing_wvf = self.techsets[index].world_vert_format;
                let fallback = facts
                    .fallback_table
                    .clone()
                    .or_else(|| self.techsets[index].fallback_table.clone());

                self.techsets[index] = TechniqueSetFacts {
                    namespace: facts.namespace,
                    name: facts.name,
                    zone: facts.zone,
                    table: Some(incoming),
                    source_occupancy: merged_occ,
                    fallback_table: fallback,

                    world_vert_format: merge_world_vert_format(
                        facts.world_vert_format,
                        existing_wvf,
                    ),
                };
            } else {
                if AssetRef::incoming_owns_slot(&self.techsets[index].name, &facts.name) {
                    self.techsets[index].name = facts.name.clone();
                    self.techsets[index].zone = facts.zone;
                }
                self.techsets[index].source_occupancy = merged_occ;
                if self.techsets[index].fallback_table.is_none() {
                    self.techsets[index].fallback_table = facts.fallback_table;
                }
                self.techsets[index].world_vert_format = merge_world_vert_format(
                    facts.world_vert_format,
                    self.techsets[index].world_vert_format,
                );
            }
            index
        } else {
            let index = self.techsets.len();
            self.techsets.push(facts);
            index
        }
    }

    pub fn take_techset_slot(&mut self, facts: TechniqueSetFacts) -> usize {
        let index = if self.cross_zone_link {
            self.link_techset(facts)
        } else {
            let index = self.techsets.len();
            self.techsets.push(facts);
            index
        };
        self.link.select_techset(self.techsets.get(index).cloned());
        index
    }

    pub fn resolve_technique_set_edges(&mut self) {
        let MaterialDefinitions {
            techsets,
            materials,
            ..
        } = &mut self.defs;
        let techsets = &*techsets;
        for material in materials {
            if material.technique_set.is_empty() {
                material.technique_set_edge = crate::AssetEdge::Absent;
                continue;
            }
            material.technique_set_edge = match MaterialDefinitions::resolve_technique_set_in(
                techsets,
                TechsetKey::new(material.namespace, material.technique_set.as_str()),
            ) {
                TechsetResolve::Hit { index, .. } | TechsetResolve::GraphMissing { index } => {
                    crate::AssetEdge::bind_order(index, techsets[index].zone)
                }
                TechsetResolve::Foreign { .. } | TechsetResolve::Missing => {
                    crate::AssetEdge::Unresolved(crate::AssetEdgeReason::CatalogMiss)
                }
            };
        }
    }

    pub fn absorb_asset_population(&mut self, donor: MaterialCatalog) -> Vec<usize> {
        self.absorb_asset_population_with_material_policy(donor, false)
    }

    pub fn absorb_asset_population_host_materials_win(
        &mut self,
        donor: MaterialCatalog,
    ) -> Vec<usize> {
        self.absorb_asset_population_with_material_policy(donor, true)
    }

    fn absorb_asset_population_with_material_policy(
        &mut self,
        donor: MaterialCatalog,
        host_materials_win: bool,
    ) -> Vec<usize> {
        let MaterialCatalog {
            defs:
                MaterialDefinitions {
                    images,
                    vertex_decls,
                    shaders,
                    techsets,
                    materials,
                    cross_game_techset_resolutions,
                    argument_gaps,
                    ..
                },
            ..
        } = donor;
        for (family, hits) in argument_gaps {
            for (key, count) in hits {
                *self
                    .argument_gaps
                    .entry(family)
                    .or_default()
                    .entry(key)
                    .or_default() += count;
            }
        }
        let image_ids = images
            .into_iter()
            .map(|image| self.link_image(image))
            .collect::<Vec<_>>();
        let vertex_decl_ids = vertex_decls
            .into_iter()
            .map(|decl| self.link_vertex_decl(decl))
            .collect::<Vec<_>>();
        let shader_ids = shaders
            .into_iter()
            .map(|shader| self.link_shader(shader))
            .collect::<Vec<_>>();
        let rebase_table = |mut table: TechniqueTable| {
            if let Some(graph) = &mut table.graph {
                for technique in graph.slots.iter_mut().flatten() {
                    for pass in &mut technique.passes {
                        pass.vertex_decl = pass
                            .vertex_decl
                            .and_then(|index| vertex_decl_ids.get(index).copied());
                        pass.vertex_shader.shader = pass
                            .vertex_shader
                            .shader
                            .and_then(|index| shader_ids.get(index).copied());
                        pass.pixel_shader.shader = pass
                            .pixel_shader
                            .shader
                            .and_then(|index| shader_ids.get(index).copied());
                    }
                }
            }
            table
        };
        self.cross_game_techset_resolutions
            .extend(cross_game_techset_resolutions);
        for mut facts in techsets {
            facts.table = facts.table.map(&rebase_table);
            facts.fallback_table = facts.fallback_table.map(&rebase_table);
            self.link_techset(facts);
        }

        materials
            .into_iter()
            .map(|mut material| {
                for texture in &mut material.textures {
                    texture.image = texture
                        .image
                        .and_then(|index| image_ids.get(index).copied());
                }
                material.technique_table = material.technique_table.map(&rebase_table);
                if host_materials_win {
                    self.link_material_host_real_wins(material)
                } else {
                    self.link_material(material)
                }
            })
            .collect()
    }

    pub fn absorb_missing_reals(&mut self, donor: MaterialCatalog) {
        let MaterialCatalog {
            defs:
                MaterialDefinitions {
                    images,
                    vertex_decls,
                    shaders,
                    techsets,
                    materials,
                    cross_game_techset_resolutions,
                    argument_gaps,
                    ..
                },
            ..
        } = donor;
        for (family, hits) in argument_gaps {
            for (key, count) in hits {
                *self
                    .argument_gaps
                    .entry(family)
                    .or_default()
                    .entry(key)
                    .or_default() += count;
            }
        }
        let image_ids = images
            .into_iter()
            .map(|image| {
                if let Some(index) = self.real_image_index(image.namespace, &image.name) {
                    return index;
                }
                self.link_image(image)
            })
            .collect::<Vec<_>>();
        let vertex_decl_ids = vertex_decls
            .into_iter()
            .map(|decl| {
                if let Some(index) = self.real_vertex_decl_index(&decl) {
                    return index;
                }
                self.link_vertex_decl(decl)
            })
            .collect::<Vec<_>>();
        let shader_ids = shaders
            .into_iter()
            .map(|shader| {
                if let Some(index) =
                    self.real_shader_index(shader.namespace, &shader.name, shader.kind)
                {
                    return index;
                }
                self.link_shader(shader)
            })
            .collect::<Vec<_>>();
        let rebase_table = |mut table: TechniqueTable| {
            if let Some(graph) = &mut table.graph {
                for technique in graph.slots.iter_mut().flatten() {
                    for pass in &mut technique.passes {
                        pass.vertex_decl = pass
                            .vertex_decl
                            .and_then(|index| vertex_decl_ids.get(index).copied());
                        pass.vertex_shader.shader = pass
                            .vertex_shader
                            .shader
                            .and_then(|index| shader_ids.get(index).copied());
                        pass.pixel_shader.shader = pass
                            .pixel_shader
                            .shader
                            .and_then(|index| shader_ids.get(index).copied());
                    }
                }
            }
            table
        };
        self.cross_game_techset_resolutions
            .extend(cross_game_techset_resolutions);
        for mut facts in techsets {
            if self
                .real_techset_index(facts.namespace, &facts.name)
                .is_some()
            {
                continue;
            }
            facts.table = facts.table.map(&rebase_table);
            facts.fallback_table = facts.fallback_table.map(&rebase_table);
            self.link_techset(facts);
        }
        for mut material in materials {
            if self.real_material_index(&material).is_some() {
                continue;
            }
            for texture in &mut material.textures {
                texture.image = texture
                    .image
                    .and_then(|index| image_ids.get(index).copied());
            }
            material.technique_table = material.technique_table.map(&rebase_table);
            self.link_material(material);
        }
    }

    fn real_image_index(&self, namespace: crate::FamilyId, name: &AssetRef) -> Option<usize> {
        self.images.iter().position(|owned| {
            owned.namespace == namespace && owned.name.is_real() && owned.name.same_name(name)
        })
    }

    fn real_shader_index(
        &self,
        namespace: crate::FamilyId,
        name: &AssetRef,
        kind: AssetType,
    ) -> Option<usize> {
        self.shaders.iter().position(|owned| {
            owned.namespace == namespace
                && owned.kind == kind
                && owned.name.is_real()
                && owned.name.same_name(name)
        })
    }

    fn real_vertex_decl_index(&self, decl: &AuthoredVertexDecl) -> Option<usize> {
        if decl.name.is_empty() {
            return None;
        }
        self.vertex_decls.iter().position(|owned| {
            owned.name.is_real() && owned.layout == decl.layout && owned.name.same_name(&decl.name)
        })
    }

    fn real_techset_index(&self, namespace: crate::FamilyId, name: &AssetRef) -> Option<usize> {
        self.techsets.iter().position(|owned| {
            owned.namespace == namespace && owned.name.is_real() && owned.name.same_name(name)
        })
    }

    fn real_material_index(&self, material: &AuthoredMaterial) -> Option<usize> {
        self.materials.iter().position(|owned| {
            owned.name.is_real()
                && owned.namespace == material.namespace
                && owned.name.same_name(&material.name)
        })
    }

    pub fn finalize_asset_population(&mut self) -> Vec<Option<usize>> {
        let mut remap = vec![None; self.materials.len()];
        let mut finalized = Vec::with_capacity(self.materials.len());
        for (old_id, material) in self.materials.drain(..).enumerate() {
            if material.name.is_reference() {
                continue;
            }
            remap[old_id] = Some(finalized.len());
            finalized.push(material);
        }
        self.materials = finalized;
        self.resolve_technique_set_edges();
        self.link.begin_zone();
        remap
    }

    pub fn absorb_technique_set_tables(&mut self, donor: &[TechniqueSetFacts]) -> usize {
        let mut absorbed = 0usize;
        for facts in donor {
            if facts.name.is_reference() {
                continue;
            }
            let mut did = false;
            if facts.table.is_some()
                && !self.techsets.iter().any(|owned| {
                    owned.namespace == facts.namespace
                        && owned.name.same_name(&facts.name)
                        && owned.table.is_some()
                })
            {
                let table = facts.table.as_ref().map(Self::table_without_donor_indices);
                if let Some(owned) = self.techsets.iter_mut().find(|owned| {
                    owned.namespace == facts.namespace && owned.name.same_name(&facts.name)
                }) {
                    if AssetRef::incoming_owns_slot(&owned.name, &facts.name) {
                        owned.name = facts.name.clone();
                        owned.zone = facts.zone;
                    }
                    owned.table = table;
                    owned.world_vert_format =
                        merge_world_vert_format(facts.world_vert_format, owned.world_vert_format);
                } else {
                    self.techsets.push(TechniqueSetFacts {
                        namespace: facts.namespace,
                        name: facts.name.clone(),
                        zone: facts.zone,
                        table,
                        source_occupancy: None,
                        fallback_table: None,
                        world_vert_format: facts.world_vert_format,
                    });
                }
                did = true;
            }
            if facts.source_occupancy.is_some()
                && !self.techsets.iter().any(|owned| {
                    owned.namespace == facts.namespace
                        && owned.name.same_name(&facts.name)
                        && owned.source_occupancy.is_some()
                })
            {
                if let Some(owned) = self.techsets.iter_mut().find(|owned| {
                    owned.namespace == facts.namespace && owned.name.same_name(&facts.name)
                }) {
                    if AssetRef::incoming_owns_slot(&owned.name, &facts.name) {
                        owned.name = facts.name.clone();
                        owned.zone = facts.zone;
                    }
                    owned.source_occupancy = facts.source_occupancy.clone();
                    owned.world_vert_format =
                        merge_world_vert_format(facts.world_vert_format, owned.world_vert_format);
                } else {
                    self.techsets.push(TechniqueSetFacts {
                        namespace: facts.namespace,
                        name: facts.name.clone(),
                        zone: facts.zone,
                        table: None,
                        source_occupancy: facts.source_occupancy.clone(),
                        fallback_table: None,
                        world_vert_format: facts.world_vert_format,
                    });
                }
                did = true;
            }
            if did {
                absorbed += 1;
            }
        }
        absorbed
    }

    pub fn absorb_feature_variant_donors(&mut self) -> usize {
        let mut copies = Vec::new();
        let mut keep_fallback = Vec::new();
        for (index, owned) in self.techsets.iter().enumerate() {
            if owned
                .table
                .as_ref()
                .is_some_and(table_shader_identity_ready)
            {
                continue;
            }

            if owned
                .fallback_table
                .as_ref()
                .is_some_and(table_shader_identity_ready)
            {
                keep_fallback.push(index);
                continue;
            }

            if owned.source_occupancy.is_none() && owned.name.is_real() {
                continue;
            }
            let stripped = alternate_techset_name(owned.namespace, owned.name.as_str());
            if stripped == owned.name.as_str() {
                continue;
            }
            let Some(donor) = self.techsets.iter().find(|donor| {
                donor.name.is_real()
                    && donor.name.as_str() == stripped
                    && donor
                        .table
                        .as_ref()
                        .is_some_and(table_shader_identity_ready)
            }) else {
                continue;
            };
            copies.push((
                index,
                donor.table.clone(),
                donor.world_vert_format,
                donor.namespace,
                donor.name.as_str().to_owned(),
            ));
        }
        for index in keep_fallback {
            if !self.techsets[index]
                .table
                .as_ref()
                .is_some_and(table_shader_identity_ready)
            {
                self.techsets[index].table = self.techsets[index].fallback_table.clone();
            }
        }
        let absorbed = copies.len();
        for (index, table, world_vert_format, donor_ns, donor_name) in copies {
            if self.techsets[index].name.is_reference() {
                self.techsets[index].name =
                    AssetRef::Real(self.techsets[index].name.as_str().to_owned());
            }
            let resolution = CrossGameTechsetResolution {
                want_namespace: self.techsets[index].namespace,
                want_name: self.techsets[index].name.as_str().to_owned(),
                got_namespace: donor_ns,
                got_name: donor_name,
                reason: CrossGameReason::FeatureVariantDonor,
            };
            self.cross_game_techset_resolutions.push(resolution);
            self.techsets[index].table = table;
            self.techsets[index].world_vert_format = world_vert_format;
        }
        absorbed
    }

    fn table_without_donor_indices(table: &TechniqueTable) -> TechniqueTable {
        let mut table = table.clone();
        if let Some(graph) = &mut table.graph {
            for technique in graph.slots.iter_mut().flatten() {
                for pass in &mut technique.passes {
                    pass.vertex_decl = None;
                    pass.vertex_shader.shader = None;
                    pass.pixel_shader.shader = None;
                }
            }
        }
        table
    }

    pub fn reroute_stub_materials(&mut self) -> usize {
        let mut routed = 0usize;
        let MaterialDefinitions {
            techsets,
            materials,
            ..
        } = &mut self.defs;
        let techsets = &*techsets;
        for material in materials {
            if material.route.is_some() {
                continue;
            }
            let Some(table) = Self::resolve_technique_table(
                techsets,
                material.namespace,
                &material.technique_set,
            ) else {
                continue;
            };
            material.technique_table = Some(table.clone());
            material.route = Some(route_from_table(
                material.sort_key,
                material.info_game_flags,
                material.state_flags,
                &table,
            ));
            routed += 1;
        }
        routed
    }

    pub fn promote_fallback_tables(&mut self) -> usize {
        let mut promoted = 0usize;
        for facts in &mut self.techsets {
            if facts.table.is_some() {
                continue;
            }
            let Some(table) = facts.fallback_table.clone() else {
                continue;
            };
            facts.table = Some(table);
            promoted = promoted.saturating_add(1);
        }
        promoted
    }

    fn resolve_technique_table(
        techsets: &[TechniqueSetFacts],
        namespace: crate::FamilyId,
        technique_set: impl AsRef<str>,
    ) -> Option<TechniqueTable> {
        match MaterialDefinitions::resolve_technique_set_in(
            techsets,
            TechsetKey::new(namespace, technique_set.as_ref()),
        ) {
            TechsetResolve::Hit { facts, .. } => facts.table.clone(),
            TechsetResolve::GraphMissing { .. }
            | TechsetResolve::Foreign { .. }
            | TechsetResolve::Missing => None,
        }
    }

    fn capture_image(&mut self, s: &ZoneStream<'_>, geometry: GfxImageGeometry) -> Option<usize> {
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let payload = if geometry.source_len == 0 {
            Vec::new()
        } else {
            s.source_slice(geometry.source_offset, geometry.source_len)
                .ok()?
                .to_vec()
        };
        Some(
            self.take_image_slot(AuthoredImage {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                map_type: geometry.map_type,
                semantic: geometry.semantic,
                category: geometry.category,
                use_srgb_reads: geometry.use_srgb_reads,
                width: geometry.width,
                height: geometry.height,
                depth: geometry.depth,
                level_count: geometry.level_count,
                format: geometry.format,
                payload: Arc::new(payload),
                decoded: None,
                common_owned: false,
                decoded_variant: None,
                decoded_by: None,
                pending_decode: None,
            }),
        )
    }

    fn reference_material_index(&self, s: &ZoneStream<'_>) -> Option<usize> {
        let name = s
            .latest_material()?
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        if !name.is_reference() {
            return None;
        }
        let namespace = self.capture_ns?;
        self.materials
            .iter()
            .position(|m| m.namespace == namespace && m.name.is_real() && m.name.same_name(&name))
    }

    fn capture_material(&mut self, s: &ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_material()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;

        let technique_set = self.link.take_techset()?;
        let mut textures = Vec::with_capacity(geometry.texture_count);
        if let Some(table) = geometry.textures {
            for i in 0..geometry.texture_count {
                let texture = table.at(i * geometry.texture_stride);
                let semantic = s.u8_at(texture, 7).ok()?;
                textures.push(MaterialTextureBinding {
                    name_hash: s.u32_at(texture, 0).ok()?,
                    name_start: s.u8_at(texture, 4).ok()?,
                    name_end: s.u8_at(texture, 5).ok()?,
                    sampler_state: s.u8_at(texture, 6).ok()?,
                    semantic,
                    image: (semantic != 11)
                        .then(|| self.link.resolve(LinkKind::Image, texture.at(8)))
                        .flatten(),
                });
            }
        }
        let mut constants = Vec::with_capacity(geometry.constant_count);
        if let Some(table) = geometry.constants {
            for i in 0..geometry.constant_count {
                let constant = table.at(i * asset_iw4::size::MATERIAL_CONSTANT_DEF);
                let mut name = [0; 12];
                for (offset, byte) in name.iter_mut().enumerate() {
                    *byte = s.u8_at(constant, 4 + offset).ok()?;
                }
                constants.push(MaterialConstant {
                    name_hash: s.u32_at(constant, 0).ok()?,
                    name,
                    literal: [
                        s.f32_at(constant, 16).ok()?,
                        s.f32_at(constant, 20).ok()?,
                        s.f32_at(constant, 24).ok()?,
                        s.f32_at(constant, 28).ok()?,
                    ],
                });
            }
        }

        let table = technique_set.table.clone().or_else(|| {
            Self::resolve_technique_table(
                &self.techsets,
                self.capture_ns
                    .expect("asset capture requires an explicit family"),
                &technique_set.name,
            )
        });
        let route = table.as_ref().map(|table| {
            route_from_table(
                geometry.sort_key,
                geometry.info_game_flags,
                geometry.state_flags,
                table,
            )
        });
        Some(
            self.take_material_slot(AuthoredMaterial {
                name,
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                technique_set_edge: if technique_set.name.is_empty() {
                    crate::AssetEdge::Absent
                } else {
                    crate::AssetEdge::Unresolved(crate::AssetEdgeReason::CatalogMiss)
                },
                technique_set: technique_set.name,
                draw_surf: geometry.draw_surf,
                sort_key: geometry.sort_key,
                info_game_flags: geometry.info_game_flags,
                texture_atlas: Some(geometry.texture_atlas),
                surface_type_bits: geometry.surface_type_bits,
                layered_surface_types: None,
                state_flags: geometry.state_flags,
                camera_region: geometry.camera_region,
                state_bits: read_material_state_bits(
                    |offset| s.u32_at(geometry.state_bits?, offset).ok(),
                    geometry.state_bits_count,
                ),
                state_bits_entry: geometry.state_bits_entry,
                native_state_bits_entry: None,

                technique_table: table,
                route,
                textures,
                constants,
                zone: self.capture_zone,
            }),
        )
    }

    fn capture_owned_technique_graph(
        &mut self,
        geometry: fastfile_iw4::TechniqueSetGeometry,
        graph: &fastfile_iw4::TechniqueGraphGeometry,
    ) -> OwnedTechniqueGraph {
        let mut slots = Vec::with_capacity(asset_iw4::size::TECHNIQUE_SLOT_COUNT);
        for tech_slot in 0..asset_iw4::size::TECHNIQUE_SLOT_COUNT {
            if geometry.technique_slots & (1u64 << tech_slot) == 0 {
                slots.push(None);
                continue;
            }
            let mut passes = Vec::new();
            for row in graph
                .iter_rows()
                .filter(|row| usize::from(row.tech_slot) == tech_slot)
            {
                let arguments = graph
                    .arguments_for(row)
                    .iter()
                    .map(|argument| {
                        let argument_type = u16::from_le_bytes([argument.raw[0], argument.raw[1]]);
                        let destination = u16::from_le_bytes([argument.raw[2], argument.raw[3]]);
                        let payload = u32::from_le_bytes([
                            argument.raw[4],
                            argument.raw[5],
                            argument.raw[6],
                            argument.raw[7],
                        ]);
                        let code_constant = || {
                            (
                                u16::from_le_bytes([argument.raw[4], argument.raw[5]]),
                                argument.raw[6],
                                argument.raw[7],
                            )
                        };
                        match argument_type {
                            asset_iw4::size::mtl_arg::MATERIAL_VERTEX_CONST => {
                                OwnedShaderArgument::MaterialVertexConstant {
                                    destination,
                                    name_hash: payload,
                                }
                            }
                            asset_iw4::size::mtl_arg::LITERAL_VERTEX_CONST => {
                                OwnedShaderArgument::LiteralVertexConstant {
                                    destination,
                                    words: argument
                                        .literal_present
                                        .then_some(argument.literal_words),
                                }
                            }
                            asset_iw4::size::mtl_arg::MATERIAL_PIXEL_SAMPLER => {
                                OwnedShaderArgument::MaterialPixelSampler {
                                    destination,
                                    name_hash: payload,
                                }
                            }
                            asset_iw4::size::mtl_arg::CODE_VERTEX_CONST => {
                                let (index, first_row, row_count) = code_constant();
                                OwnedShaderArgument::CodeVertexConstant {
                                    destination,
                                    index,
                                    first_row,
                                    row_count,
                                }
                            }
                            asset_iw4::size::mtl_arg::CODE_PIXEL_SAMPLER => {
                                OwnedShaderArgument::CodePixelSampler {
                                    destination,
                                    index: payload,
                                }
                            }
                            asset_iw4::size::mtl_arg::CODE_PIXEL_CONST => {
                                let (index, first_row, row_count) = code_constant();
                                OwnedShaderArgument::CodePixelConstant {
                                    destination,
                                    index,
                                    first_row,
                                    row_count,
                                }
                            }
                            asset_iw4::size::mtl_arg::MATERIAL_PIXEL_CONST => {
                                OwnedShaderArgument::MaterialPixelConstant {
                                    destination,
                                    name_hash: payload,
                                }
                            }
                            asset_iw4::size::mtl_arg::LITERAL_PIXEL_CONST => {
                                OwnedShaderArgument::LiteralPixelConstant {
                                    destination,
                                    words: argument
                                        .literal_present
                                        .then_some(argument.literal_words),
                                }
                            }
                            _ => OwnedShaderArgument::Unknown {
                                argument_type,
                                raw: argument.raw,
                            },
                        }
                    })
                    .collect();
                passes.push(OwnedMaterialPass {
                    pass_index: row.pass_index,
                    vertex_decl_identity: row.vertex_decl_slot.into(),
                    vertex_decl: self
                        .link
                        .resolve(LinkKind::VertexDecl, row.vertex_decl_slot),
                    vertex_shader: OwnedShaderRef {
                        pointer_identity: row.vertex_shader_slot.into(),
                        shader: self
                            .link
                            .resolve(LinkKind::VertexShader, row.vertex_shader_slot),
                    },
                    pixel_shader: OwnedShaderRef {
                        pointer_identity: row.pixel_shader_slot.into(),
                        shader: self
                            .link
                            .resolve(LinkKind::PixelShader, row.pixel_shader_slot),
                    },
                    per_prim_arg_count: row.per_prim_arg_count,
                    per_obj_arg_count: row.per_obj_arg_count,
                    stable_arg_count: row.stable_arg_count,
                    custom_sampler_flags: row.custom_sampler_flags,
                    t5_custom_sampler_flags: 0,
                    arguments,
                    arguments_truncated: row.arguments_truncated,
                });
            }
            let scanned = geometry.technique_slots_scanned & (1u64 << tech_slot) != 0;
            let technique = if scanned {
                Some(OwnedTechnique {
                    source_selection: None,
                    flags: geometry.technique_flags_by_slot[tech_slot],
                    passes,
                    body_scanned: true,
                })
            } else {
                geometry.technique_body_by_slot[tech_slot]
                    .and_then(|body| self.link.technique(body))
            };
            if let (Some(body), Some(technique)) =
                (geometry.technique_body_by_slot[tech_slot], &technique)
            {
                self.link.remember_technique(body, technique.clone());
            }
            slots.push(technique);
        }
        OwnedTechniqueGraph {
            slots,
            rows_truncated: graph.rows_truncated,
            arguments_truncated: graph.arguments_truncated,
        }
    }

    fn capture_technique_set(&mut self, s: &ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_technique_set()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_else(|| AssetRef::Real(String::new()));

        let graph = s
            .latest_technique_graph()
            .map(|graph| self.capture_owned_technique_graph(geometry, graph));
        let table = (geometry.technique_slots != 0).then_some(TechniqueTable {
            slots: geometry.technique_slots,
            scanned: geometry.technique_slots_scanned,
            technique0_flags: geometry.technique0_flags,
            model_lighting_const: Some(geometry.uses_model_lighting_const),
            max_pass_count: geometry.max_pass_count,
            pass_count_by_slot: geometry.pass_count_by_slot,
            graph,
        });

        if let Some(graph) = s.latest_technique_graph() {
            if graph.rows_truncated != 0 || graph.arguments_truncated != 0 {
                diag::warn!(
                    Zone,
                    "technique graph truncated for {name}: rows_truncated={} arguments_truncated={} (fixed capture cap)",
                    graph.rows_truncated,
                    graph.arguments_truncated
                );
            }
        }
        Some(
            self.take_techset_slot(TechniqueSetFacts {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                zone: self.capture_zone,
                table,
                source_occupancy: None,
                fallback_table: None,
                world_vert_format: geometry.world_vert_format,
            }),
        )
    }

    fn capture_shader(&mut self, s: &ZoneStream<'_>, kind: AssetType) -> Option<usize> {
        let geometry = s.latest_shader()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let mut bytes = Vec::with_capacity(geometry.program_words * 4);
        if let Some(program) = geometry.program {
            for i in 0..geometry.program_words {
                bytes.extend_from_slice(&s.u32_at(program, i * 4).ok()?.to_le_bytes());
            }
        }
        Some(
            self.take_shader_slot(AuthoredShader {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                kind,
                program: bytes,
            }),
        )
    }

    fn capture_vertex_decl(&mut self, s: &ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_vertex_decl()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_else(|| AssetRef::Real(String::new()));
        Some(self.take_vertex_decl_slot(AuthoredVertexDecl {
            layout: render_material::IW4_VERTEX_LAYOUT,
            name,
            stream_count: geometry.stream_count,
            has_optional_source: geometry.has_optional_source,
            routing: geometry.routing,
        }))
    }
}

impl AssetLinkSink for MaterialCatalog {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> Result<()> {
        let (map, index) = match ty {
            AssetType::Image => {
                let Some(geometry) = s.latest_image() else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                let Some(index) = self.capture_image(s, geometry) else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                (LinkKind::Image, index)
            }
            AssetType::Material => {
                let header = s.latest_material().and_then(|g| g.header);
                let Some(index) = self
                    .capture_material(s)
                    .or_else(|| self.reference_material_index(s))
                else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                if let Some(header) = header {
                    self.link.bind_direct(LinkKind::Material, header, index);
                }
                (LinkKind::Material, index)
            }
            AssetType::TechniqueSet => {
                let Some(index) = self.capture_technique_set(s) else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                (LinkKind::Techset, index)
            }
            AssetType::VertexDecl => {
                let Some(index) = self.capture_vertex_decl(s) else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                (LinkKind::VertexDecl, index)
            }
            AssetType::PixelShader | AssetType::VertexShader => {
                let Some(index) = self.capture_shader(s, ty) else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                let links = match ty {
                    AssetType::VertexShader => LinkKind::VertexShader,
                    AssetType::PixelShader => LinkKind::PixelShader,
                    _ => unreachable!("shader arm accepts only vertex or pixel assets"),
                };
                self.link.bind_direct(links, slot, index);
                if let Some(insert_slot) = insert_slot {
                    self.link.bind_direct(links, insert_slot, index);
                }
                return Ok(());
            }
            _ => return Ok(()),
        };
        self.link.bind_direct(map, slot, index);
        if let Some(insert_slot) = insert_slot {
            self.link.bind_direct(map, insert_slot, index);
        }

        if ty == AssetType::Material {
            if let Some(header) = s.latest_material().and_then(|g| g.header) {
                self.link.bind_direct(map, header, index);
            }
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> Result<()> {
        let map = match ty {
            AssetType::Image => LinkKind::Image,
            AssetType::Material => LinkKind::Material,
            AssetType::TechniqueSet => {
                self.link.select_techset(
                    self.link
                        .resolve(LinkKind::Techset, target)
                        .and_then(|index| self.techsets.get(index).cloned()),
                );
                LinkKind::Techset
            }
            AssetType::VertexShader => LinkKind::VertexShader,
            AssetType::PixelShader => LinkKind::PixelShader,
            AssetType::VertexDecl => LinkKind::VertexDecl,
            _ => return Ok(()),
        };
        self.link.bind_alias(map, slot, target);
        Ok(())
    }

    fn linked_asset_name(&self, slot: Ptr) -> Option<&str> {
        let index = self.material_index(slot)?;
        let name = self.materials.get(index.get())?.name.as_str();
        (!name.is_empty()).then_some(name)
    }
}

fn merge_world_vert_format(incoming: u8, existing: u8) -> u8 {
    if incoming != 0 { incoming } else { existing }
}

fn table_shader_identity_ready(table: &TechniqueTable) -> bool {
    table.graph.as_ref().is_some_and(|graph| {
        graph.slots.iter().flatten().any(|technique| {
            technique.passes.iter().any(|pass| {
                pass.vertex_shader.shader.is_some() && pass.pixel_shader.shader.is_some()
            })
        })
    })
}

pub fn merge_source_occupancy(
    incoming: Option<SourceTechniqueOccupancy>,
    existing: Option<SourceTechniqueOccupancy>,
) -> Option<SourceTechniqueOccupancy> {
    match (incoming, existing) {
        (None, None) => None,
        (Some(occupancy), None) | (None, Some(occupancy)) => Some(occupancy),
        (Some(mut incoming), Some(existing)) => {
            incoming
                .slots
                .resize(incoming.slots.len().max(existing.slots.len()), 0);
            incoming
                .scanned
                .resize(incoming.scanned.len().max(existing.scanned.len()), 0);
            for (slot, word) in incoming.slots.iter_mut().zip(existing.slots) {
                *slot |= word;
            }
            for (slot, word) in incoming.scanned.iter_mut().zip(existing.scanned) {
                *slot |= word;
            }
            incoming.max_pass_count = incoming.max_pass_count.max(existing.max_pass_count);
            if incoming.technique0_flags == 0 {
                incoming.technique0_flags = existing.technique0_flags;
            }
            incoming.pass_count_by_slot.resize(
                incoming
                    .pass_count_by_slot
                    .len()
                    .max(existing.pass_count_by_slot.len()),
                0,
            );
            for (count, existing) in incoming
                .pass_count_by_slot
                .iter_mut()
                .zip(existing.pass_count_by_slot)
            {
                if *count == 0 {
                    *count = existing;
                }
            }
            Some(incoming)
        }
    }
}

fn route_from_table(
    sort_key: u8,
    info_game_flags: u8,
    state_flags: u8,
    table: &TechniqueTable,
) -> asset_iw4::MaterialDrawRoute {
    asset_iw4::MaterialDrawRoute::route(
        sort_key,
        dpvs_iw4::material_prepass(
            table.slots & 1 == 0,
            table.slots & 2 != 0,
            state_flags,
            table.technique0_flags,
        ),
        info_game_flags >> 6,
        table.slots,
        table.model_lighting_const.unwrap_or(false),
    )
}

fn argument_is_model_lighting_const(argument: &OwnedShaderArgument) -> bool {
    matches!(
        argument,
        OwnedShaderArgument::CodeVertexConstant {
            index: asset_iw4::material::CODE_CONST_MODEL_LIGHTING,
            ..
        } | OwnedShaderArgument::CodePixelConstant {
            index: asset_iw4::material::CODE_CONST_MODEL_LIGHTING,
            ..
        }
    )
}

pub fn graph_slots_bind_model_lighting_const(slots: &[Option<OwnedTechnique>]) -> bool {
    slots.iter().flatten().any(|technique| {
        technique
            .passes
            .iter()
            .any(|pass| pass.arguments.iter().any(argument_is_model_lighting_const))
    })
}

pub fn read_material_state_bits(
    mut word: impl FnMut(usize) -> Option<u32>,
    count: usize,
) -> Vec<[u32; 2]> {
    let mut table = Vec::with_capacity(count);
    for index in 0..count {
        let offset = index * asset_iw4::size::GFX_STATE_BITS;
        let (Some(low), Some(high)) = (word(offset), word(offset + 4)) else {
            break;
        };
        table.push([low, high]);
    }
    table
}

pub fn decode_reference_shader_argument(
    argument_type: u16,
    raw: [u8; 8],
    literal_words: [u32; 4],
    literal_present: bool,
) -> OwnedShaderArgument {
    let destination = u16::from_le_bytes([raw[2], raw[3]]);
    let payload = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
    let code_constant = (u16::from_le_bytes([raw[4], raw[5]]), raw[6], raw[7]);
    match argument_type {
        asset_iw4::size::mtl_arg::MATERIAL_VERTEX_CONST => {
            OwnedShaderArgument::MaterialVertexConstant {
                destination,
                name_hash: payload,
            }
        }
        asset_iw4::size::mtl_arg::LITERAL_VERTEX_CONST => {
            OwnedShaderArgument::LiteralVertexConstant {
                destination,
                words: literal_present.then_some(literal_words),
            }
        }
        asset_iw4::size::mtl_arg::MATERIAL_PIXEL_SAMPLER => {
            OwnedShaderArgument::MaterialPixelSampler {
                destination,
                name_hash: payload,
            }
        }
        asset_iw4::size::mtl_arg::CODE_VERTEX_CONST => {
            let (index, first_row, row_count) = code_constant;
            OwnedShaderArgument::CodeVertexConstant {
                destination,
                index,
                first_row,
                row_count,
            }
        }
        asset_iw4::size::mtl_arg::CODE_PIXEL_SAMPLER => OwnedShaderArgument::CodePixelSampler {
            destination,
            index: payload,
        },
        asset_iw4::size::mtl_arg::CODE_PIXEL_CONST => {
            let (index, first_row, row_count) = code_constant;
            OwnedShaderArgument::CodePixelConstant {
                destination,
                index,
                first_row,
                row_count,
            }
        }
        asset_iw4::size::mtl_arg::MATERIAL_PIXEL_CONST => {
            OwnedShaderArgument::MaterialPixelConstant {
                destination,
                name_hash: payload,
            }
        }
        asset_iw4::size::mtl_arg::LITERAL_PIXEL_CONST => {
            OwnedShaderArgument::LiteralPixelConstant {
                destination,
                words: literal_present.then_some(literal_words),
            }
        }
        _ => OwnedShaderArgument::Unknown { argument_type, raw },
    }
}
