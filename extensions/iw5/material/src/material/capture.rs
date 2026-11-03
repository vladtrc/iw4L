use asset_material::material_catalog::{
    LinkKind, decode_reference_shader_argument as owned_shader_argument,
    read_material_state_bits as read_state_bits,
};
use asset_material::*;
use fastfile_iw4::AssetType;
use std::sync::Arc;

pub trait NativeMaterialCatalog {
    fn iw5_loaded(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        insert_slot: Option<ext_iw5_fastfile::Ptr>,
    );
    fn iw5_alias(
        &mut self,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        target: ext_iw5_fastfile::Ptr,
    );
    fn capture_image_iw5(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        geometry: ext_iw5_fastfile::GfxImageGeometry,
    ) -> Option<usize>;
    fn capture_material_iw5(&mut self, s: &ext_iw5_fastfile::ZoneStream<'_>) -> Option<usize>;
    fn capture_technique_set_iw5(&mut self, s: &ext_iw5_fastfile::ZoneStream<'_>) -> Option<usize>;
    fn capture_owned_technique_graph_iw5(
        &mut self,
        geometry: ext_iw5_fastfile::TechniqueSetGeometry,
        graph: &ext_iw5_fastfile::TechniqueGraphGeometry,
    ) -> TechniqueTable;
    fn capture_shader_iw5(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        kind: ext_iw5_fastfile::AssetType,
    ) -> Option<usize>;
    fn capture_vertex_decl_iw5(&mut self, s: &ext_iw5_fastfile::ZoneStream<'_>) -> Option<usize>;
    fn note_leftover_iw5_arg(&mut self, iw5_type: u16, raw: [u8; 8]);
}

impl NativeMaterialCatalog for MaterialCatalog {
    fn iw5_loaded(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        insert_slot: Option<ext_iw5_fastfile::Ptr>,
    ) {
        use ext_iw5_fastfile::AssetType as Iw5;
        let slot = link_address(slot);
        let insert_slot = insert_slot.map(link_address);
        let (map, index) = match ty {
            Iw5::Image => {
                let Some(geometry) = s.latest_image() else {
                    self.capture_gaps += 1;
                    return;
                };
                let Some(index) = self.capture_image_iw5(s, geometry) else {
                    self.capture_gaps += 1;
                    return;
                };
                (LinkKind::Image, index)
            }
            Iw5::Material => {
                let Some(index) = self.capture_material_iw5(s) else {
                    self.capture_gaps += 1;
                    return;
                };
                (LinkKind::Material, index)
            }
            Iw5::TechniqueSet => {
                let Some(index) = self.capture_technique_set_iw5(s) else {
                    self.capture_gaps += 1;
                    return;
                };
                (LinkKind::Techset, index)
            }
            Iw5::VertexDecl => {
                let Some(index) = self.capture_vertex_decl_iw5(s) else {
                    self.capture_gaps += 1;
                    return;
                };
                (LinkKind::VertexDecl, index)
            }
            Iw5::PixelShader | Iw5::VertexShader => {
                let Some(index) = self.capture_shader_iw5(s, ty) else {
                    self.capture_gaps += 1;
                    return;
                };
                let links = match ty {
                    Iw5::VertexShader => LinkKind::VertexShader,
                    Iw5::PixelShader => LinkKind::PixelShader,
                    _ => unreachable!("shader arm accepts only vertex or pixel assets"),
                };
                self.links_mut().bind_direct(links, slot, index);
                if let Some(insert_slot) = insert_slot {
                    self.links_mut().bind_direct(links, insert_slot, index);
                }
                return;
            }
            _ => return,
        };
        self.links_mut().bind_direct(map, slot, index);
        if let Some(insert_slot) = insert_slot {
            self.links_mut().bind_direct(map, insert_slot, index);
        }
    }

    fn iw5_alias(
        &mut self,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        target: ext_iw5_fastfile::Ptr,
    ) {
        use ext_iw5_fastfile::AssetType as Iw5;
        let slot = link_address(slot);
        let target = link_address(target);
        let map = match ty {
            Iw5::Image => LinkKind::Image,
            Iw5::Material => LinkKind::Material,
            Iw5::TechniqueSet => {
                let selection = self
                    .links()
                    .resolve(LinkKind::Techset, target)
                    .and_then(|index| self.technique_set_facts().get(index).cloned());
                self.links_mut().select_techset(selection);
                LinkKind::Techset
            }
            Iw5::VertexDecl => LinkKind::VertexDecl,
            Iw5::VertexShader => LinkKind::VertexShader,
            Iw5::PixelShader => LinkKind::PixelShader,
            _ => return,
        };
        self.links_mut().bind_alias(map, slot, target);
    }

    fn capture_image_iw5(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        geometry: ext_iw5_fastfile::GfxImageGeometry,
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

    fn capture_material_iw5(&mut self, s: &ext_iw5_fastfile::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_material()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let technique_set = self.links_mut().take_techset()?;
        let mut textures = Vec::with_capacity(geometry.texture_count);
        if let Some(table) = geometry.textures {
            for i in 0..geometry.texture_count {
                let texture =
                    table.at(i * s.layout(ext_iw5_fastfile::size::MATERIAL_TEXTURE_DEF, 16));
                let semantic = s.u8_at(texture, 7).ok()?;
                textures.push(MaterialTextureBinding {
                    name_hash: s.u32_at(texture, 0).ok()?,
                    name_start: s.u8_at(texture, 4).ok()?,
                    name_end: s.u8_at(texture, 5).ok()?,
                    sampler_state: s.u8_at(texture, 6).ok()?,
                    semantic,
                    image: (semantic != 11)
                        .then(|| {
                            self.links().resolve(
                                LinkKind::Image,
                                link_address(
                                    texture.at(ext_iw5_fastfile::size::MATERIAL_TEXTURE_DEF_U_OFF),
                                ),
                            )
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
        let header = geometry.header?;

        let info_game_flags = s.u8_at(header, s.layout(4, 8)).ok()?;
        let counts = s.layout(ext_iw5_fastfile::size::MATERIAL_TEXTURE_COUNT_OFF, 86);
        let state_flags = s.u8_at(header, counts + 3).ok()?;
        let camera_region = s.u8_at(header, counts + 4).ok()?;
        let native_state_bits_entry = geometry.state_bits_entry;
        let state_bits_entry = native_state_bits_entry
            .as_ref()
            .map(crate::material::iw5_tech_map::remap_state_bits_entry);
        Some(self.take_material_slot(AuthoredMaterial {
            name,
            namespace: self.capture_ns(),
            technique_set_edge: if technique_set.name.is_empty() {
                asset_material::AssetEdge::Absent
            } else {
                asset_material::AssetEdge::Unresolved(asset_material::AssetEdgeReason::CatalogMiss)
            },
            technique_set: technique_set.name,
            draw_surf: geometry.draw_surf,
            sort_key: geometry.sort_key,
            info_game_flags,
            texture_atlas: None,
            surface_type_bits: None,
            layered_surface_types: None,
            state_flags,
            camera_region,
            state_bits: read_state_bits(
                |offset| s.u32_at(geometry.state_bits?, offset).ok(),
                geometry.state_bits_count,
            ),
            state_bits_entry,

            native_state_bits_entry: native_state_bits_entry.map(|entry| entry.to_vec()),
            technique_table: technique_set.table,
            route: None,
            textures,
            constants,
            zone: self.capture_zone(),
        }))
    }

    fn capture_technique_set_iw5(&mut self, s: &ext_iw5_fastfile::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_technique_set()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_else(|| AssetRef::Real(String::new()));
        let fallback = s.latest_technique_graph().and_then(|graph| {
            (geometry.occupancy != 0)
                .then(|| self.capture_owned_technique_graph_iw5(geometry, graph))
        });
        if let Some(graph) = s.latest_technique_graph() {
            if graph.rows_truncated != 0 || graph.arguments_truncated != 0 {
                diag::warn!(
                    Zone,
                    "IW5 technique graph truncated for {name}: rows_truncated={} arguments_truncated={}",
                    graph.rows_truncated,
                    graph.arguments_truncated
                );
            }
        }

        Some(self.take_techset_slot(TechniqueSetFacts {
            namespace: self.capture_ns(),
            name,
            zone: self.capture_zone(),
            table: None,
            source_occupancy: None,
            fallback_table: fallback,
            world_vert_format: geometry.world_vert_format,
        }))
    }

    fn capture_owned_technique_graph_iw5(
        &mut self,
        geometry: ext_iw5_fastfile::TechniqueSetGeometry,
        graph: &ext_iw5_fastfile::TechniqueGraphGeometry,
    ) -> TechniqueTable {
        let slots_bits = crate::material::iw5_tech_map::remap_occupancy_bits(geometry.occupancy);
        let scanned_bits =
            crate::material::iw5_tech_map::remap_occupancy_bits(geometry.occupancy_scanned);
        let pass_count_by_slot =
            crate::material::iw5_tech_map::remap_pass_count_by_slot(&geometry.pass_count_by_slot);
        let flags_by_slot = crate::material::iw5_tech_map::remap_technique_flags_by_slot(
            &geometry.technique_flags_by_slot,
        );
        let mut slots = Vec::with_capacity(asset_iw4::size::TECHNIQUE_SLOT_COUNT);
        for iw4_slot in 0..asset_iw4::size::TECHNIQUE_SLOT_COUNT {
            if slots_bits & (1u64 << iw4_slot) == 0 {
                slots.push(None);
                continue;
            }
            let mut passes = Vec::new();
            for row in graph.iter_rows() {
                let Some(mapped) =
                    crate::material::iw5_tech_map::iw5_slot_to_iw4(usize::from(row.tech_slot))
                else {
                    continue;
                };
                if mapped != iw4_slot {
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
                    match take_iw5_pass_argument(
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
                        None => {
                            let iw5_type = u16::from_le_bytes([argument.raw[0], argument.raw[1]]);
                            self.note_leftover_iw5_arg(iw5_type, argument.raw);
                        }
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
                    custom_sampler_flags: row.custom_sampler_flags,
                    t5_custom_sampler_flags: 0,
                    arguments,
                    arguments_truncated: row.arguments_truncated,
                });
            }
            let scanned = scanned_bits & (1u64 << iw4_slot) != 0;
            let iw5_slot = crate::material::iw5_tech_map::iw4_slot_to_iw5(iw4_slot);
            let technique = if scanned {
                Some(OwnedTechnique {
                    source_selection: None,
                    flags: flags_by_slot[iw4_slot],
                    passes,
                    body_scanned: true,
                })
            } else {
                iw5_slot
                    .and_then(|slot| geometry.technique_body_by_slot.get(slot).copied().flatten())
                    .and_then(|body| self.links().technique(link_address(body)))
            };
            if let (Some(slot), Some(technique)) = (iw5_slot, &technique) {
                if let Some(body) = geometry.technique_body_by_slot.get(slot).copied().flatten() {
                    self.links_mut()
                        .remember_technique(link_address(body), technique.clone());
                }
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

    fn capture_shader_iw5(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        kind: ext_iw5_fastfile::AssetType,
    ) -> Option<usize> {
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
        let kind = match kind {
            ext_iw5_fastfile::AssetType::VertexShader => AssetType::VertexShader,
            ext_iw5_fastfile::AssetType::PixelShader => AssetType::PixelShader,
            _ => return None,
        };
        Some(self.take_shader_slot(AuthoredShader {
            namespace: self.capture_ns(),
            name,
            kind,
            program: bytes,
        }))
    }

    fn capture_vertex_decl_iw5(&mut self, s: &ext_iw5_fastfile::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_vertex_decl()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_else(|| AssetRef::Real(String::new()));
        let mut routing = [[0u8; 2]; asset_iw4::vertex_decl::ROUTING_COUNT];
        for (index, pair) in geometry.routing.iter().enumerate() {
            if let Some(slot) = routing.get_mut(index) {
                *slot = *pair;
            }
        }
        Some(self.take_vertex_decl_slot(AuthoredVertexDecl {
            layout: render_material::IW4_VERTEX_LAYOUT,
            name,
            stream_count: geometry.stream_count,
            has_optional_source: geometry.has_optional_source,
            routing,
        }))
    }

    fn note_leftover_iw5_arg(&mut self, iw5_type: u16, raw: [u8; 8]) {
        let index = u16::from_le_bytes([raw[4], raw[5]]);
        let key = format!("t{iw5_type}i{index}");
        self.note_argument_gap(asset_core::FamilyId::new("iw5"), key);
    }
}

fn remap_iw5_code_const_source(iw5: u16) -> Option<u16> {
    crate::material::iw5_tech_map::remap_code_const_index(iw5)
        .or_else(|| crate::material::iw5_tech_map::leftover_iw5_code_bank(iw5))
}

fn remap_iw5_owned_shader_argument(argument: OwnedShaderArgument) -> Option<OwnedShaderArgument> {
    match argument {
        OwnedShaderArgument::CodeVertexConstant {
            destination,
            index,
            first_row,
            row_count,
        } => remap_iw5_code_const_source(index).map(|index| {
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
        } => {
            remap_iw5_code_const_source(index).map(|index| OwnedShaderArgument::CodePixelConstant {
                destination,
                index,
                first_row,
                row_count,
            })
        }
        OwnedShaderArgument::CodePixelSampler { destination, index } => {
            crate::material::iw5_tech_map::remap_code_texture_index(index)
                .map(|index| OwnedShaderArgument::CodePixelSampler { destination, index })
        }
        other => Some(other),
    }
}

fn take_iw5_pass_argument(
    raw: [u8; 8],
    literal_words: [u32; 4],
    literal_present: bool,
) -> Option<OwnedShaderArgument> {
    let iw5_type = u16::from_le_bytes([raw[0], raw[1]]);
    let argument_type = crate::material::iw5_tech_map::remap_shader_arg_type(iw5_type)?;
    remap_iw5_owned_shader_argument(owned_shader_argument(
        argument_type,
        raw,
        literal_words,
        literal_present,
    ))
}

fn link_address(pointer: ext_iw5_fastfile::Ptr) -> MaterialLinkAddress {
    MaterialLinkAddress::new(
        pointer.block,
        pointer.offset,
        ext_iw5_fastfile::block_is_aliasable(pointer.block),
    )
}
