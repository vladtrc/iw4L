use super::{
    AssetLinkSink, NestedShaderKind, always_alloc, asset_ptr_at, begin_temp_body, follow_name,
};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{
    AlignWasteSite, GfxImageGeometry, MaterialGeometry, Ptr, Result, ShaderGeometry,
    TechniqueArgumentGeometry, TechniquePassGeometry, TechniqueSetGeometry, VertexDeclGeometry,
    XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream,
};

pub(super) fn load_technique_set(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, sz::TECHNIQUE_SET)?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    s.record_technique_set(TechniqueSetGeometry {
        name,
        ..TechniqueSetGeometry::default()
    });
    s.begin_technique_graph();

    let mut technique_slots = [0u64; sz::TECHNIQUE_OCCUPANCY_WORDS];
    let mut technique0 = None;
    let mut technique_body_by_slot = [None; sz::TECHNIQUE_SLOT_COUNT];
    for (i, body_identity) in technique_body_by_slot.iter_mut().enumerate() {
        match s.ptr_at(p, sz::TECHNIQUE_SET_TECHNIQUES_OFF + i * 4)? {
            ZonePtr::Null => continue,
            ZonePtr::Offset(q) => {
                let body = s.resolve_alias(q);
                *body_identity = Some(body);
                if i == 0 {
                    technique0 = Some(body);
                }
            }
            ZonePtr::Following | ZonePtr::Insert => {}
        }
        sz::occupancy_set(&mut technique_slots, i);
    }

    let mut scanned = [0u64; sz::TECHNIQUE_OCCUPANCY_WORDS];
    let mut max_pass_count = 0u16;
    let mut pass_count_by_slot = [0u8; sz::TECHNIQUE_SLOT_COUNT];
    let mut technique_flags_by_slot = [0u16; sz::TECHNIQUE_SLOT_COUNT];
    for i in 0..sz::TECHNIQUE_SLOT_COUNT {
        if s.begin_body(p.at(sz::TECHNIQUE_SET_TECHNIQUES_OFF + i * 4))? {
            let tech_slot = i.min(u8::MAX as usize) as u8;
            let (head, pass_count) = load_technique(s, links, tech_slot)?;
            technique_body_by_slot[i] = Some(head);
            sz::occupancy_set(&mut scanned, i);
            max_pass_count = max_pass_count.max(pass_count);
            pass_count_by_slot[i] = pass_count.min(u8::MAX as u16) as u8;
            technique_flags_by_slot[i] = s.u16_at(head, sz::TECHNIQUE_FLAGS_OFF)?;
            if i == 0 {
                technique0 = Some(head);
            }
        }
    }

    let technique0_flags = match technique0 {
        Some(head) => s.u8_at(head, sz::TECHNIQUE_FLAGS_OFF)?,
        None => 0,
    };
    let world_vert_format = s.u8_at(p, sz::TECHNIQUE_SET_WORLD_VERT_FORMAT_OFF)?;
    s.record_technique_set(TechniqueSetGeometry {
        name,
        technique_slots,
        technique_slots_scanned: scanned,
        technique_body_by_slot,
        technique0_flags,
        world_vert_format,
        max_pass_count,
        pass_count_by_slot,
        technique_flags_by_slot,
    });
    s.pop()
}

fn load_technique(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    tech_slot: u8,
) -> Result<(Ptr, u16)> {
    s.align_pos(4)?;
    let peek = s.peek(8);
    if peek.len() < 8 {
        return Err(crate::zone::ZoneError::Truncated {
            at: s.cursor(),
            needed: 8,
            len: s.len(),
        });
    }
    let pass_count_peek = u16::from_le_bytes([peek[6], peek[7]]) as usize;
    let total = 8 + sz::MATERIAL_PASS * pass_count_peek;
    let head = s.alloc_load(1, total)?;
    let flags = s.u16_at(head, sz::TECHNIQUE_FLAGS_OFF)?;
    let pass_count = s.u16_at(head, sz::TECHNIQUE_PASS_COUNT_OFF)?;
    let passes = head.at(8);
    for i in 0..pass_count as usize {
        let row = load_material_pass(
            s,
            links,
            passes.at(i * sz::MATERIAL_PASS),
            tech_slot,
            i.min(u8::MAX as usize) as u8,
            flags,
        )?;
        s.push_technique_row(row);
    }
    follow_name(s, head, 0)?;
    Ok((head, pass_count))
}

fn load_material_pass(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
    tech_slot: u8,
    pass_index: u8,
    technique_flags: u16,
) -> Result<TechniquePassGeometry> {
    let per_prim = s.u8_at(p, 12)?;
    let per_obj = s.u8_at(p, 13)?;
    let stable = s.u8_at(p, 14)?;
    let custom_sampler_flags = s.u8_at(p, 15)?;
    let arg_count = usize::from(per_prim) + usize::from(per_obj) + usize::from(stable);
    let argument_start = s.technique_argument_count();
    let mut captured = 0u16;
    let mut arguments_truncated = false;

    load_nested_vertex_decl(s, links, p.at(0))?;
    load_nested_shader(s, links, NestedShaderKind::Vertex, p.at(4))?;
    load_nested_shader(s, links, NestedShaderKind::Pixel, p.at(8))?;

    if always_alloc(s, p.at(16))? {
        let args = s.alloc_load(4, sz::MATERIAL_SHADER_ARGUMENT * arg_count)?;
        s.fixup_slot(p.at(16), args)?;
        for i in 0..arg_count {
            let a = args.at(i * sz::MATERIAL_SHADER_ARGUMENT);
            let ty = s.u16_at(a, 0)?;
            let mut argument = TechniqueArgumentGeometry::default();
            for (offset, byte) in argument.raw.iter_mut().enumerate() {
                *byte = s.u8_at(a, offset)?;
            }
            if ty == sz::LITERAL_VERTEX_CONST || ty == sz::LITERAL_PIXEL_CONST {
                if s.begin_body(a.at(4))? {
                    let literal = s.alloc_load(4, 16)?;
                    for (index, word) in argument.literal_words.iter_mut().enumerate() {
                        *word = s.u32_at(literal, index * 4)?;
                    }
                    argument.literal_present = true;
                }
            }
            if s.push_technique_argument(argument) {
                captured = captured.saturating_add(1);
            } else {
                arguments_truncated = true;
            }
        }
    } else if arg_count != 0 {
        s.note_technique_arguments_truncated(arg_count.min(u16::MAX as usize) as u16);
        arguments_truncated = true;
    }

    Ok(TechniquePassGeometry {
        tech_slot,
        pass_index,
        technique_flags,
        vertex_decl_slot: p.at(0),
        vertex_shader_slot: p.at(4),
        pixel_shader_slot: p.at(8),
        per_prim_arg_count: per_prim,
        per_obj_arg_count: per_obj,
        stable_arg_count: stable,
        custom_sampler_flags,
        argument_start,
        argument_count: captured,
        arguments_truncated,
    })
}

fn load_nested_vertex_decl(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
) -> Result<()> {
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => Ok(()),
        ZonePtr::Offset(target) => links.nested_vertex_decl_alias(slot, s.resolve_alias(target)),
        ZonePtr::Following | ZonePtr::Insert => {
            if s.begin_body(slot)? {
                load_vertex_decl(s)?;
                links.nested_vertex_decl(s, slot)?;
            }
            Ok(())
        }
    }
}

fn load_nested_shader(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    kind: NestedShaderKind,
    slot: Ptr,
) -> Result<()> {
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => Ok(()),
        ZonePtr::Offset(target) => links.nested_shader_alias(kind, slot, s.resolve_alias(target)),
        ZonePtr::Following | ZonePtr::Insert => {
            if s.begin_body(slot)? {
                load_shader(s)?;
                links.nested_shader(s, kind, slot)?;
            }
            Ok(())
        }
    }
}

fn load_vertex_decl(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, sz::VERTEX_DECL)?;
    let stream_count = s.u8_at(p, sz::VERTEX_DECL_STREAM_COUNT_OFF)?;
    let has_optional_source = s.u8_at(p, sz::VERTEX_DECL_HAS_OPTIONAL_SOURCE_OFF)?;
    let mut routing = [[0u8; 2]; sz::VERTEX_DECL_ROUTING_COUNT];
    for (index, pair) in routing.iter_mut().enumerate() {
        let offset = sz::VERTEX_DECL_ROUTING_OFF + index * 2;
        pair[0] = s.u8_at(p, offset)?;
        pair[1] = s.u8_at(p, offset + 1)?;
    }
    s.record_vertex_decl(VertexDeclGeometry {
        header: Some(p),
        stream_count,
        has_optional_source,
        routing,
    });
    Ok(())
}

fn load_shader(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, sz::VERTEX_SHADER)?;
    follow_name(s, p, 0)?;
    let words = s.u16_at(p, 12)? as usize;
    let program = s.plain_array(p, 8, 4, 4, words)?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    s.record_shader(ShaderGeometry {
        header: Some(p),
        name,
        program,
        program_words: words,
    });
    Ok(())
}

pub(super) fn load_material(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::MATERIAL)?;
    let texture_count = s.u8_at(p, sz::MATERIAL_TEXTURE_COUNT_OFF)? as usize;
    let constant_count = s.u8_at(p, sz::MATERIAL_TEXTURE_COUNT_OFF + 1)? as usize;
    let state_bits_count = s.u8_at(p, sz::MATERIAL_TEXTURE_COUNT_OFF + 2)? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;

    follow_name(s, p, 0)?;
    asset_ptr_at(
        s,
        links,
        AssetType::TechniqueSet,
        p.at(sz::MATERIAL_TECHNIQUE_SET_OFF),
    )?;

    let mut textures = None;
    if s.begin_body(p.at(sz::MATERIAL_TEXTURE_TABLE_OFF))? {
        let arr = s.alloc_load(4, sz::MATERIAL_TEXTURE_DEF * texture_count)?;
        s.fixup_slot(p.at(sz::MATERIAL_TEXTURE_TABLE_OFF), arr)?;
        textures = Some(arr);
        for i in 0..texture_count {
            let b = arr.at(i * sz::MATERIAL_TEXTURE_DEF);
            if s.u8_at(b, sz::MATERIAL_TEXTURE_DEF_SEMANTIC_OFF)? == sz::SEMANTIC_WATER {
                load_water(s, links, b.at(sz::MATERIAL_TEXTURE_DEF_U_OFF))?;
            } else {
                asset_ptr_at(
                    s,
                    links,
                    AssetType::Image,
                    b.at(sz::MATERIAL_TEXTURE_DEF_U_OFF),
                )?;
            }
        }
    }

    let constants = s.with_align_site(AlignWasteSite::MatConstants, |s| {
        s.plain_array(
            p,
            sz::MATERIAL_CONSTANT_TABLE_OFF,
            16,
            sz::MATERIAL_CONSTANT_DEF,
            constant_count,
        )
    })?;
    let state_bits = s.plain_array(
        p,
        sz::MATERIAL_STATE_BITS_OFF,
        4,
        sz::GFX_STATE_BITS,
        state_bits_count,
    )?;

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };

    let mut state_bits_entry = [0u8; sz::TECHNIQUE_SLOT_COUNT];
    for (index, entry) in state_bits_entry.iter_mut().enumerate() {
        *entry = s.u8_at(p, sz::MATERIAL_STATE_BITS_ENTRY_OFF + index)?;
    }

    s.record_material(MaterialGeometry {
        name,
        draw_surf: u64::from(s.u32_at(p, 16)?) | (u64::from(s.u32_at(p, 20)?) << 32),
        surface_type_bits: s.u32_at(p, 0x18)?,
        sort_key: s.u8_at(p, 9)?,
        info_game_flags: s.u8_at(p, sz::MATERIAL_INFO_GAME_FLAGS_OFF)?,
        state_flags: s.u8_at(p, sz::MATERIAL_STATE_FLAGS_OFF)?,
        camera_region: s.u8_at(p, sz::MATERIAL_CAMERA_REGION_OFF)?,
        state_bits,
        state_bits_count,
        state_bits_entry: Some(state_bits_entry),
        textures,
        texture_count,
        constants,
        constant_count,
    });

    s.pop()
}

fn load_water(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, slot: Ptr) -> Result<()> {
    if !s.begin_body(slot)? {
        return Ok(());
    }
    let water = s.alloc_load(4, sz::WATER)?;
    let width = s.i32_at(water, 12)?.max(0) as usize;
    let height = s.i32_at(water, 16)?.max(0) as usize;
    let count = width.saturating_mul(height);
    s.plain_array(water, 4, 4, 8, count)?;
    s.plain_array(water, 8, 4, 4, count)?;
    asset_ptr_at(s, links, AssetType::Image, water.at(64))
}

pub(super) fn load_image(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, sz::GFX_IMAGE)?;
    let map_type = s.u8_at(p, 4)?;
    let semantic = s.u8_at(p, 5)?;
    let category = s.u8_at(p, 6)?;
    let use_srgb_reads = s.u8_at(p, 7)? != 0;
    let width = s.u16_at(p, 20)?;
    let height = s.u16_at(p, 22)?;
    let depth = s.u16_at(p, 24)?;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, sz::GFX_IMAGE_NAME_OFF)?;
    let name = match s.ptr_at(p, sz::GFX_IMAGE_NAME_OFF)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };

    let mut level_count = 0u8;
    let mut format = 0u32;
    let mut source_offset = 0usize;
    let mut source_len = 0usize;
    if begin_temp_body(s, p)? {
        let d = s.alloc_load(4, sz::GFX_IMAGE_LOAD_DEF_HEAD)?;
        level_count = s.u8_at(d, 0)?;
        format = s.u32_at(d, 4)?;
        source_len = s.u32_at(d, 8)? as usize;
        source_offset = s.cursor();
        s.alloc_load(1, source_len)?;
    }
    s.pop()?;

    s.record_image(GfxImageGeometry {
        name,
        map_type,
        semantic,
        category,
        use_srgb_reads,
        width,
        height,
        depth,
        level_count,
        format,
        source_offset,
        source_len,
    });

    s.pop()
}
