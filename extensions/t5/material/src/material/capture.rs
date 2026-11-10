use asset_material::material_catalog::{
    LinkKind, decode_reference_shader_argument as owned_shader_argument,
    read_material_state_bits as read_state_bits,
};
use asset_material::*;
use fastfile_iw4::AssetType;
use std::sync::Arc;

pub trait NativeMaterialCatalog {
    fn t5_loaded(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
        insert_slot: Option<ext_t5_fastfile::Ptr>,
    );
    fn t5_alias(
        &mut self,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    );
    fn t5_nested_shader(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        kind: ext_t5_fastfile::NestedShaderKind,
        slot: ext_t5_fastfile::Ptr,
    );
    fn t5_nested_shader_alias(
        &mut self,
        kind: ext_t5_fastfile::NestedShaderKind,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    );
    fn t5_nested_vertex_decl(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        slot: ext_t5_fastfile::Ptr,
    );
    fn t5_nested_vertex_decl_alias(
        &mut self,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    );
    fn capture_image_t5(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        geometry: ext_t5_fastfile::GfxImageGeometry,
    ) -> Option<usize>;
    fn capture_material_t5(&mut self, s: &ext_t5_fastfile::ZoneStream<'_>) -> Option<usize>;
    fn capture_technique_set_t5(&mut self, s: &ext_t5_fastfile::ZoneStream<'_>) -> Option<usize>;
    fn capture_owned_technique_graph_t5(
        &mut self,
        geometry: ext_t5_fastfile::TechniqueSetGeometry,
        graph: &ext_t5_fastfile::TechniqueGraphGeometry,
    ) -> TechniqueTable;
    fn capture_shader_t5(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        kind: AssetType,
    ) -> Option<usize>;
    fn capture_vertex_decl_t5(&mut self, s: &ext_t5_fastfile::ZoneStream<'_>) -> Option<usize>;
    fn note_leftover_t5_arg(&mut self, raw: [u8; 8]);
}

impl NativeMaterialCatalog for MaterialCatalog {
    fn t5_loaded(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
        insert_slot: Option<ext_t5_fastfile::Ptr>,
    ) {
        use ext_t5_fastfile::AssetType as T5;
        let slot = link_address(slot);
        let insert_slot = insert_slot.map(link_address);
        let (map, index) = match ty {
            T5::Image => {
                let Some(geometry) = s.latest_image() else {
                    self.capture_gaps += 1;
                    return;
                };
                let Some(index) = self.capture_image_t5(s, geometry) else {
                    self.capture_gaps += 1;
                    return;
                };
                (LinkKind::Image, index)
            }
            T5::Material => {
                let Some(index) = self.capture_material_t5(s) else {
                    self.capture_gaps += 1;
                    return;
                };
                (LinkKind::Material, index)
            }
            T5::TechniqueSet => {
                let Some(index) = self.capture_technique_set_t5(s) else {
                    self.capture_gaps += 1;
                    return;
                };
                (LinkKind::Techset, index)
            }
            _ => return,
        };
        self.links_mut().bind_direct(map, slot, index);
        if let Some(insert_slot) = insert_slot {
            self.links_mut().bind_direct(map, insert_slot, index);
        }
    }

    fn t5_alias(
        &mut self,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) {
        use ext_t5_fastfile::AssetType as T5;
        let slot = link_address(slot);
        let target = link_address(target);
        let map = match ty {
            T5::Image => LinkKind::Image,
            T5::Material => LinkKind::Material,
            T5::TechniqueSet => {
                let selection = self
                    .links()
                    .resolve(LinkKind::Techset, target)
                    .and_then(|index| self.technique_set_facts().get(index).cloned());
                self.links_mut().select_techset(selection);
                LinkKind::Techset
            }
            _ => return,
        };
        self.links_mut().bind_alias(map, slot, target);
    }

    fn t5_nested_shader(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        kind: ext_t5_fastfile::NestedShaderKind,
        slot: ext_t5_fastfile::Ptr,
    ) {
        let kind = match kind {
            ext_t5_fastfile::NestedShaderKind::Vertex => AssetType::VertexShader,
            ext_t5_fastfile::NestedShaderKind::Pixel => AssetType::PixelShader,
        };
        let Some(index) = self.capture_shader_t5(s, kind) else {
            self.capture_gaps += 1;
            return;
        };
        let links = match kind {
            AssetType::VertexShader => LinkKind::VertexShader,
            AssetType::PixelShader => LinkKind::PixelShader,
            _ => return,
        };
        let slot = link_address(slot);
        self.links_mut().bind_direct(links, slot, index);
        if let Some(header) = s.latest_shader().and_then(|g| g.header) {
            self.links_mut()
                .bind_direct(links, link_address(header), index);
        }
    }

    fn t5_nested_shader_alias(
        &mut self,
        kind: ext_t5_fastfile::NestedShaderKind,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) {
        let links = match kind {
            ext_t5_fastfile::NestedShaderKind::Vertex => LinkKind::VertexShader,
            ext_t5_fastfile::NestedShaderKind::Pixel => LinkKind::PixelShader,
        };
        self.links_mut()
            .bind_alias(links, link_address(slot), link_address(target));
    }

    fn t5_nested_vertex_decl(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        slot: ext_t5_fastfile::Ptr,
    ) {
        let Some(index) = self.capture_vertex_decl_t5(s) else {
            self.capture_gaps += 1;
            return;
        };
        let slot = link_address(slot);
        self.links_mut()
            .bind_direct(LinkKind::VertexDecl, slot, index);
        if let Some(header) = s.latest_vertex_decl().and_then(|g| g.header) {
            self.links_mut()
                .bind_direct(LinkKind::VertexDecl, link_address(header), index);
        }
    }

    fn t5_nested_vertex_decl_alias(
        &mut self,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) {
        self.links_mut().bind_alias(
            LinkKind::VertexDecl,
            link_address(slot),
            link_address(target),
        );
    }

    fn capture_image_t5(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        geometry: ext_t5_fastfile::GfxImageGeometry,
    ) -> Option<usize> {
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
        Some(self.take_image_slot(AuthoredImage {
            namespace: self.capture_ns(),
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
        }))
    }

    fn capture_material_t5(&mut self, s: &ext_t5_fastfile::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_material()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let technique_set = self.links_mut().take_techset()?;
        let mut textures = Vec::with_capacity(geometry.texture_count);
        if let Some(table) = geometry.textures {
            for i in 0..geometry.texture_count {
                let texture = table.at(i * 16);
                let semantic = s.u8_at(texture, 7).ok()?;
                textures.push(MaterialTextureBinding {
                    name_hash: s.u32_at(texture, 0).ok()?,
                    name_start: s.u8_at(texture, 4).ok()?,
                    name_end: s.u8_at(texture, 5).ok()?,
                    sampler_state: s.u8_at(texture, 6).ok()?,
                    semantic,
                    image: (semantic != 11)
                        .then(|| {
                            self.links()
                                .resolve(LinkKind::Image, link_address(texture.at(12)))
                        })
                        .flatten(),
                });
            }
        }
        let mut constants = Vec::with_capacity(geometry.constant_count);
        if let Some(table) = geometry.constants {
            for i in 0..geometry.constant_count {
                let constant = table.at(i * 32);
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
        Some(
            self.take_material_slot(AuthoredMaterial {
                name,
                namespace: self.capture_ns(),
                technique_set_edge: if technique_set.name.is_empty() {
                    asset_material::AssetEdge::Absent
                } else {
                    asset_material::AssetEdge::Unresolved(
                        asset_material::AssetEdgeReason::CatalogMiss,
                    )
                },
                technique_set: technique_set.name,
                draw_surf: geometry.draw_surf,
                sort_key: geometry.sort_key,

                info_game_flags: geometry.info_game_flags,
                texture_atlas: Some(geometry.texture_atlas),
                surface_type_bits: Some(geometry.surface_type_bits),
                layered_surface_types: Some(geometry.layered_surface_types),
                state_flags: geometry.state_flags,
                camera_region: geometry.camera_region,
                state_bits: t5_colour_keeps_prepass_depth(
                    geometry.state_bits_entry.as_ref(),
                    read_state_bits(
                        |offset| s.u32_at(geometry.state_bits?, offset).ok(),
                        geometry.state_bits_count,
                    ),
                ),

                state_bits_entry: geometry
                    .state_bits_entry
                    .as_ref()
                    .map(crate::material::t5_tech_map::remap_native_state_bits_entry),
                native_state_bits_entry: geometry.state_bits_entry.map(|entry| entry.to_vec()),

                technique_table: None,
                route: None,
                textures,
                constants,
                zone: self.capture_zone(),
            }),
        )
    }

    fn capture_technique_set_t5(&mut self, s: &ext_t5_fastfile::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_technique_set()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_else(|| AssetRef::Real(String::new()));
        let occupancy = ext_t5_fastfile::occupancy_any(&geometry.technique_slots).then_some(
            SourceTechniqueOccupancy {
                slots: geometry.technique_slots.to_vec(),
                scanned: geometry.technique_slots_scanned.to_vec(),
                technique0_flags: geometry.technique0_flags,
                max_pass_count: geometry.max_pass_count,
                pass_count_by_slot: geometry.pass_count_by_slot.to_vec(),
            },
        );

        let fallback = s.latest_technique_graph().and_then(|graph| {
            ext_t5_fastfile::occupancy_any(&geometry.technique_slots)
                .then(|| self.capture_owned_technique_graph_t5(geometry, graph))
        });
        if let Some(graph) = s.latest_technique_graph()
            && (graph.rows_truncated != 0 || graph.arguments_truncated != 0)
        {
            diag::warn!(
                Zone,
                "T5 technique graph truncated for {name}: rows_truncated={} arguments_truncated={}",
                graph.rows_truncated,
                graph.arguments_truncated
            );
        }
        Some(self.take_techset_slot(TechniqueSetFacts {
            namespace: self.capture_ns(),
            name,
            zone: self.capture_zone(),
            table: None,
            source_occupancy: occupancy,
            fallback_table: fallback,
            world_vert_format: geometry.world_vert_format,
        }))
    }

    fn capture_owned_technique_graph_t5(
        &mut self,
        geometry: ext_t5_fastfile::TechniqueSetGeometry,
        graph: &ext_t5_fastfile::TechniqueGraphGeometry,
    ) -> TechniqueTable {
        let slots_bits =
            crate::material::t5_tech_map::remap_occupancy_bits(&geometry.technique_slots);
        let scanned_bits =
            crate::material::t5_tech_map::remap_occupancy_bits(&geometry.technique_slots_scanned);
        let pass_count_by_slot =
            crate::material::t5_tech_map::remap_pass_count_by_slot(&geometry.pass_count_by_slot);
        let flags_by_slot = crate::material::t5_tech_map::remap_technique_flags_by_slot(
            &geometry.technique_flags_by_slot,
        );
        let mut slots = Vec::with_capacity(asset_iw4::size::TECHNIQUE_SLOT_COUNT);
        for (iw4_slot, flags_by_slot_entry) in flags_by_slot
            [..asset_iw4::size::TECHNIQUE_SLOT_COUNT]
            .iter()
            .enumerate()
        {
            if slots_bits & (1u64 << iw4_slot) == 0 {
                slots.push(None);
                continue;
            }
            let mut passes = Vec::new();
            for row in graph.iter_rows() {
                if crate::material::t5_tech_map::iw4_slot_to_t5(iw4_slot)
                    != Some(usize::from(row.tech_slot))
                {
                    continue;
                }
                let authored = graph.arguments_for(row);
                let prim_n = usize::from(row.per_prim_arg_count);
                let obj_n = usize::from(row.per_obj_arg_count);
                let mut arguments = Vec::with_capacity(authored.len());
                let mut per_prim_arg_count = 0u8;
                let mut per_obj_arg_count = 0u8;
                let mut stable_arg_count = 0u8;
                for (index, argument) in authored.iter().enumerate() {
                    match take_t5_pass_argument(
                        argument.raw,
                        argument.literal_words,
                        argument.literal_present,
                    ) {
                        Some(owned) => {
                            arguments.push(owned);
                            if index < prim_n {
                                per_prim_arg_count = per_prim_arg_count.saturating_add(1);
                            } else if index < prim_n.saturating_add(obj_n) {
                                per_obj_arg_count = per_obj_arg_count.saturating_add(1);
                            } else {
                                stable_arg_count = stable_arg_count.saturating_add(1);
                            }
                        }
                        None => self.note_leftover_t5_arg(argument.raw),
                    }
                }
                let vertex_decl_slot = link_address(row.vertex_decl_slot);
                let vertex_shader_slot = link_address(row.vertex_shader_slot);
                let pixel_shader_slot = link_address(row.pixel_shader_slot);
                passes.push(OwnedMaterialPass {
                    pass_index: row.pass_index,
                    vertex_decl_identity: vertex_decl_slot.into(),
                    vertex_decl: self.links().resolve(LinkKind::VertexDecl, vertex_decl_slot),
                    vertex_shader: OwnedShaderRef {
                        pointer_identity: vertex_shader_slot.into(),
                        shader: self
                            .links()
                            .resolve(LinkKind::VertexShader, vertex_shader_slot),
                    },
                    pixel_shader: OwnedShaderRef {
                        pointer_identity: pixel_shader_slot.into(),
                        shader: self
                            .links()
                            .resolve(LinkKind::PixelShader, pixel_shader_slot),
                    },
                    per_prim_arg_count,
                    per_obj_arg_count,
                    stable_arg_count,
                    custom_sampler_flags:
                        crate::material::t5_code_remap::remap_t5_custom_sampler_flags(
                            row.custom_sampler_flags,
                        ),
                    t5_custom_sampler_flags: row.custom_sampler_flags,
                    arguments,
                    arguments_truncated: row.arguments_truncated,
                });
            }
            let scanned = scanned_bits & (1u64 << iw4_slot) != 0;
            let t5_slot = crate::material::t5_tech_map::iw4_slot_to_t5(iw4_slot);
            let technique = if scanned {
                Some(OwnedTechnique {
                    source_selection: None,
                    flags: (*flags_by_slot_entry),
                    passes,
                    body_scanned: true,
                })
            } else {
                t5_slot
                    .and_then(|slot| geometry.technique_body_by_slot.get(slot).copied().flatten())
                    .and_then(|body| self.links().technique(link_address(body)))
            };
            if let (Some(slot), Some(technique)) = (t5_slot, &technique)
                && let Some(body) = geometry.technique_body_by_slot.get(slot).copied().flatten()
            {
                self.links_mut()
                    .remember_technique(link_address(body), technique.clone());
            }
            slots.push(technique);
        }
        TechniqueTable {
            slots: slots_bits,
            scanned: scanned_bits,
            technique0_flags: geometry.technique0_flags,
            model_lighting_const: Some(graph_slots_bind_model_lighting_const(&slots)),
            max_pass_count: geometry.max_pass_count,
            pass_count_by_slot,
            graph: Some(OwnedTechniqueGraph {
                slots,
                rows_truncated: graph.rows_truncated,
                arguments_truncated: graph.arguments_truncated,
            }),
        }
    }

    fn capture_shader_t5(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        kind: AssetType,
    ) -> Option<usize> {
        let geometry = s.latest_shader()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_else(|| AssetRef::Real(String::new()));
        let mut bytes = Vec::with_capacity(geometry.program_words * 4);
        if let Some(program) = geometry.program {
            for i in 0..geometry.program_words {
                bytes.extend_from_slice(&s.u32_at(program, i * 4).ok()?.to_le_bytes());
            }
        }
        Some(self.take_shader_slot(AuthoredShader {
            namespace: self.capture_ns(),
            name,
            kind,
            program: bytes,
        }))
    }

    fn capture_vertex_decl_t5(&mut self, s: &ext_t5_fastfile::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_vertex_decl()?;
        let mut routing = [[0u8; 2]; asset_iw4::vertex_decl::ROUTING_COUNT];
        for (index, pair) in geometry.routing.iter().enumerate() {
            if let Some(slot) = routing.get_mut(index) {
                *slot = *pair;
            }
        }
        Some(self.take_vertex_decl_slot(AuthoredVertexDecl {
            layout: crate::material::vertex_layout::LAYOUT,
            name: AssetRef::default(),
            stream_count: geometry.stream_count,
            has_optional_source: geometry.has_optional_source,
            routing,
        }))
    }

    fn note_leftover_t5_arg(&mut self, raw: [u8; 8]) {
        let dest = u16::from_le_bytes([raw[2], raw[3]]);
        let index = u16::from_le_bytes([raw[4], raw[5]]);
        let name = crate::material::t5_code_remap::t5_code_const_name(index).unwrap_or("?");
        let key = format!("d{dest}i{index}:{name}");
        self.note_argument_gap(asset_core::FamilyId::new("t5"), key);
    }
}

fn t5_colour_keeps_prepass_depth(
    entry: Option<&[u8; ext_t5_fastfile::TECHNIQUE_SLOT_COUNT]>,
    mut table: Vec<[u32; 2]>,
) -> Vec<[u32; 2]> {
    const DEPTH_PREPASS: usize = 0;
    let prepass_writes_depth = entry
        .map(|entry| usize::from(entry[DEPTH_PREPASS]))
        .and_then(|row| table.get(row))
        .is_some_and(|bits| bits[1] & asset_iw4::GFXS1_DEPTHWRITE != 0);
    if !prepass_writes_depth {
        return table;
    }
    for bits in &mut table {
        let unblended = matches!(
            asset_material::MaterialDrawMode::from_state_bits(*bits),
            asset_material::MaterialDrawMode::Opaque
                | asset_material::MaterialDrawMode::AlphaTest { .. }
        );
        if unblended && bits[1] & asset_iw4::GFXS1_DEPTHTEST_DISABLE == 0 {
            bits[1] |= asset_iw4::GFXS1_DEPTHWRITE;
        }
    }
    table
}

fn link_address(pointer: ext_t5_fastfile::Ptr) -> MaterialLinkAddress {
    MaterialLinkAddress::new(
        pointer.block,
        pointer.offset,
        ext_t5_fastfile::block_is_aliasable(pointer.block),
    )
}

fn remap_t5_owned_shader_argument(argument: OwnedShaderArgument) -> Option<OwnedShaderArgument> {
    match argument {
        OwnedShaderArgument::CodeVertexConstant {
            destination,
            index,
            first_row,
            row_count,
        } => crate::material::t5_code_remap::remap_t5_code_const_source(index).map(|index| {
            OwnedShaderArgument::CodeVertexConstant {
                destination,
                index,
                first_row,
                row_count,
            }
        }),
        OwnedShaderArgument::CodePixelConstant {
            destination,
            index,
            first_row,
            row_count,
        } => crate::material::t5_code_remap::remap_t5_code_const_source(index).map(|index| {
            OwnedShaderArgument::CodePixelConstant {
                destination,
                index,
                first_row,
                row_count,
            }
        }),
        OwnedShaderArgument::CodePixelSampler { destination, index }
            if (0x1d..=0x20).contains(&index) =>
        {
            Some(OwnedShaderArgument::MaterialPixelSampler {
                destination,
                name_hash: crate::material::t5_code_remap::terrain_scorch_binding_hash(
                    (index - 0x1d) as usize,
                ),
            })
        }
        OwnedShaderArgument::CodePixelSampler { destination, index } => {
            crate::material::t5_code_remap::remap_code_texture_index(index)
                .map(|index| OwnedShaderArgument::CodePixelSampler { destination, index })
        }
        other => Some(other),
    }
}

fn take_t5_pass_argument(
    raw: [u8; 8],
    literal_words: [u32; 4],
    literal_present: bool,
) -> Option<OwnedShaderArgument> {
    let argument_type = u16::from_le_bytes([raw[0], raw[1]]);
    remap_t5_owned_shader_argument(owned_shader_argument(
        argument_type,
        raw,
        literal_words,
        literal_present,
    ))
}
