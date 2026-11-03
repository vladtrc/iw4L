use super::{AssetLinkSink, asset_ptr_at, begin_temp_body, follow_name};
use crate::asset_type::AssetType;
use crate::size::{self as sz, mtl_arg};
use crate::zone::{
    GfxImageGeometry, MaterialGeometry, Ptr, Result, ShaderGeometry, TechniqueArgumentGeometry,
    TechniquePassGeometry, TechniqueSetGeometry, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream,
};

pub(super) fn load_technique_set(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    s.walk_stage = "technique_set";
    let p = s.alloc_load(4, s.layout(sz::TECHNIQUE_SET, 456))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };

    s.record_technique_set(TechniqueSetGeometry {
        header: Some(p),
        name,
        ..TechniqueSetGeometry::default()
    });
    s.begin_technique_graph();

    let mut occupancy: u64 = 0;
    let mut occupancy_scanned: u64 = 0;
    let mut technique0 = None;
    let mut technique_body_by_slot = [None; sz::TECHNIQUE_SLOT_COUNT];
    for (i, body_identity) in technique_body_by_slot.iter_mut().enumerate() {
        match s.ptr_at(
            p,
            s.layout(sz::TECHNIQUE_SLOTS_OFF, 24) + i * s.pointer_bytes(),
        )? {
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
        occupancy |= 1 << i;
    }

    let mut max_pass_count = 0u16;
    let mut pass_count_by_slot = [0u8; sz::TECHNIQUE_SLOT_COUNT];
    let mut technique_flags_by_slot = [0u16; sz::TECHNIQUE_SLOT_COUNT];
    for i in 0..sz::TECHNIQUE_SLOT_COUNT {
        s.walk_stage = "technique_set.techniques";
        if s.begin_body(p.at(s.layout(sz::TECHNIQUE_SLOTS_OFF, 24) + i * s.pointer_bytes()))? {
            let (head, pass_count) = load_technique(s, links, i as u8)?;
            technique_body_by_slot[i] = Some(head);
            occupancy_scanned |= 1 << i;
            max_pass_count = max_pass_count.max(pass_count);
            pass_count_by_slot[i] = pass_count.min(u8::MAX as u16) as u8;
            technique_flags_by_slot[i] = s.u16_at(head, s.layout(4, 8))?;
            if i == 0 {
                technique0 = Some(head);
            }
        }
    }

    let technique0_flags = match technique0 {
        Some(head) => s.u8_at(head, s.layout(4, 8))?,
        None => 0,
    };
    let world_vert_format = s.u8_at(p, s.layout(sz::TECHNIQUE_SET_WORLD_VERT_FORMAT_OFF, 8))?;
    s.record_technique_set(TechniqueSetGeometry {
        header: Some(p),
        name,
        occupancy,
        occupancy_scanned,
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
    s.walk_stage = "technique";
    let head = s.alloc_load(4, s.layout(8, 16))?;
    let flags = s.u16_at(head, s.layout(4, 8))?;
    let pass_count = s.u16_at(head, s.layout(6, 10))?;
    let pass_size = s.layout(sz::MATERIAL_PASS, 40);
    let passes = s.alloc_load(1, pass_size * pass_count as usize)?;
    for i in 0..pass_count as usize {
        let row = load_material_pass(
            s,
            links,
            passes.at(i * pass_size),
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
    let per_prim = s.u8_at(p, s.layout(12, 24))?;
    let per_obj = s.u8_at(p, s.layout(13, 25))?;
    let stable = s.u8_at(p, s.layout(14, 26))?;
    let custom_sampler_flags = s.u8_at(p, s.layout(15, 27))?;
    let arg_count = usize::from(per_prim) + usize::from(per_obj) + usize::from(stable);
    let argument_start = s.technique_argument_count();
    let mut captured = 0u16;
    let mut arguments_truncated = false;

    s.walk_stage = "pass.vertex_decl";
    asset_ptr_at(s, links, AssetType::VertexDecl, p.at(0))?;
    s.walk_stage = "pass.vertex_shader";
    asset_ptr_at(s, links, AssetType::VertexShader, p.at(s.layout(4, 8)))?;
    s.walk_stage = "pass.pixel_shader";
    asset_ptr_at(s, links, AssetType::PixelShader, p.at(s.layout(8, 16)))?;

    s.walk_stage = "pass.args";
    if s.begin_body(p.at(s.layout(16, 32)))? {
        let arg_size = s.layout(sz::MATERIAL_SHADER_ARGUMENT, 16);
        let arg_union = s.layout(4, 8);
        let args = s.alloc_load(4, arg_size * arg_count)?;
        for i in 0..arg_count {
            let a = args.at(i * arg_size);
            let ty = s.u16_at(a, 0)?;
            let mut argument = TechniqueArgumentGeometry::default();

            for (offset, byte) in argument.raw.iter_mut().enumerate() {
                *byte = s.u8_at(
                    a,
                    if offset < 4 {
                        offset
                    } else {
                        arg_union + offset - 4
                    },
                )?;
            }
            if ty == mtl_arg::LITERAL_VERTEX_CONST || ty == mtl_arg::LITERAL_PIXEL_CONST {
                let literal = match s.ptr_at(a, arg_union)? {
                    ZonePtr::Null => None,
                    ZonePtr::Offset(body) => {
                        s.begin_body(a.at(arg_union))?;
                        Some(s.resolve_alias(body))
                    }
                    ZonePtr::Following | ZonePtr::Insert => {
                        s.begin_body(a.at(arg_union))?;
                        Some(s.alloc_load(4, 16)?)
                    }
                };
                if let Some(literal) = literal {
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
        vertex_shader_slot: p.at(s.layout(4, 8)),
        pixel_shader_slot: p.at(s.layout(8, 16)),
        per_prim_arg_count: per_prim,
        per_obj_arg_count: per_obj,
        stable_arg_count: stable,
        custom_sampler_flags,
        argument_start,
        argument_count: captured,
        arguments_truncated,
    })
}

pub(super) fn load_shader(s: &mut ZoneStream<'_>) -> Result<()> {
    s.walk_stage = "shader";
    let p = s.alloc_load(4, s.layout(sz::PIXEL_SHADER, 32))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let load_def = p.at(s.layout(8, 16));
    let program_words = s.u16_at(load_def, s.layout(4, 8))? as usize;
    s.walk_stage = "shader.program";
    let program = s.plain_array(load_def, 0, 4, 4, program_words)?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    s.record_shader(ShaderGeometry {
        name,
        program,
        program_words,
    });
    s.pop()
}

pub(super) fn load_vertex_decl(s: &mut ZoneStream<'_>) -> Result<()> {
    s.walk_stage = "vertex_decl";
    let p = s.alloc_load(4, s.layout(sz::VERTEX_DECL, 176))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    let stream_count = s.u8_at(p, s.layout(sz::VERTEX_DECL_STREAM_COUNT_OFF, 8))?;
    let has_optional_source = s.u8_at(p, s.layout(sz::VERTEX_DECL_HAS_OPTIONAL_SOURCE_OFF, 9))?;
    let mut routing = [[0u8; 2]; sz::VERTEX_DECL_ROUTING_COUNT];
    for (index, pair) in routing.iter_mut().enumerate() {
        let offset = s.layout(sz::VERTEX_DECL_ROUTING_OFF, 16) + index * 2;
        pair[0] = s.u8_at(p, offset)?;
        pair[1] = s.u8_at(p, offset + 1)?;
    }
    follow_name(s, p, 0)?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    s.record_vertex_decl(crate::zone::VertexDeclGeometry {
        name,
        stream_count,
        has_optional_source,
        routing,
    });
    s.pop()
}

pub(super) fn load_material(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    s.walk_stage = "material";

    let p = s.alloc_load(4, s.layout(sz::MATERIAL, 128))?;
    let texture_count_off = s.layout(sz::MATERIAL_TEXTURE_COUNT_OFF, 86);
    let texture_count = s.u8_at(p, texture_count_off)? as usize;
    let constant_count = s.u8_at(p, texture_count_off + 1)? as usize;
    let state_bits_count = s.u8_at(p, texture_count_off + 2)? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    s.walk_stage = "material.technique_set";
    asset_ptr_at(
        s,
        links,
        AssetType::TechniqueSet,
        p.at(s.layout(sz::MATERIAL_TECHNIQUE_SET_OFF, 96)),
    )?;

    let mut textures = None;
    s.walk_stage = "material.textures";
    if s.begin_body(p.at(s.layout(sz::MATERIAL_TEXTURE_TABLE_OFF, 104)))? {
        let texture_def = s.layout(sz::MATERIAL_TEXTURE_DEF, 16);
        let arr = s.alloc_load(4, texture_def * texture_count)?;
        textures = Some(arr);
        for i in 0..texture_count {
            let b = arr.at(i * texture_def);
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

    s.walk_stage = "material.constants";
    let constants = s.plain_array(
        p,
        s.layout(sz::MATERIAL_CONSTANT_TABLE_OFF, 112),
        16,
        sz::MATERIAL_CONSTANT_DEF,
        constant_count,
    )?;
    s.walk_stage = "material.state_bits";
    let state_bits = s.plain_array(
        p,
        s.layout(sz::MATERIAL_STATE_BITS_OFF, 120),
        4,
        sz::GFX_STATE_BITS,
        state_bits_count,
    )?;

    let mut state_bits_entry = [0u8; sz::TECHNIQUE_SLOT_COUNT];
    let state_bits_entry_off = s.layout(sz::MATERIAL_STATE_BITS_ENTRY_OFF, 32);
    for (index, entry) in state_bits_entry.iter_mut().enumerate() {
        *entry = s.u8_at(p, state_bits_entry_off + index)?;
    }

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    s.record_material(MaterialGeometry {
        header: Some(p),
        name,
        draw_surf: u64::from(s.u32_at(p, s.layout(8, 16))?)
            | (u64::from(s.u32_at(p, s.layout(12, 20))?) << 32),
        sort_key: s.u8_at(p, s.layout(5, 9))?,
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
    s.walk_stage = "water";
    if !s.begin_body(slot)? {
        return Ok(());
    }

    let water = s.alloc_load(4, s.layout(sz::WATER, 96))?;
    let m = s.i32_at(water, s.layout(sz::WATER_M_OFF, 32))?.max(0) as usize;
    let n = s.i32_at(water, s.layout(sz::WATER_N_OFF, 36))?.max(0) as usize;
    let count = n.saturating_mul(m);
    if s.wire_format() == crate::wire::Iw5WireFormat::X64 {
        s.plain_array(water, 8, 4, 4, count)?;
        s.plain_array(water, 16, 4, 4, count)?;
        s.plain_array(water, 24, 4, 4, count)?;
    } else {
        s.plain_array(water, sz::WATER_H0_OFF, 4, 8, count)?;
        s.plain_array(water, sz::WATER_WTERM_OFF, 4, 4, count)?;
    }
    asset_ptr_at(
        s,
        links,
        AssetType::Image,
        water.at(s.layout(sz::WATER_IMAGE_OFF, 88)),
    )
}

pub(super) fn load_image(s: &mut ZoneStream<'_>) -> Result<()> {
    s.walk_stage = "image";
    let p = s.alloc_load(4, s.layout(sz::GFX_IMAGE, 40))?;
    let map_type = s.u8_at(p, s.layout(4, 8))?;
    let semantic = s.u8_at(p, s.layout(5, 9))?;
    let category = s.u8_at(p, s.layout(6, 10))?;
    // Only bit 0 means sRGB; zones also carry 0x2 and 0x10 in this byte.
    let use_srgb_reads = s.u8_at(p, s.layout(7, 11))? & 1 != 0;
    let width = s.u16_at(p, s.layout(20, 24))?;
    let height = s.u16_at(p, s.layout(22, 26))?;
    let depth = s.u16_at(p, s.layout(24, 28))?;

    s.push(XFILE_BLOCK_VIRTUAL)?;

    let name_off = s.layout(sz::GFX_IMAGE_NAME_OFF, 32);
    follow_name(s, p, name_off)?;
    let name = match s.ptr_at(p, name_off)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };

    let mut level_count = 0;
    let mut format = 0;
    let mut source_offset = 0;
    let mut source_len = 0;
    s.walk_stage = "image.load_def";
    if begin_temp_body(s, p)? {
        let d = s.alloc_load(4, sz::GFX_IMAGE_LOAD_DEF)?;
        level_count = s.u8_at(d, 0)?;
        format = s.u32_at(d, 8)?;
        source_len = s.u32_at(d, 12)? as usize;
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
