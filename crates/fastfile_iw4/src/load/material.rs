use asset_iw4::size::{self as sz, mtl_arg};

use super::{AssetLinkSink, asset_ptr_at, begin_temp_body, follow_name};
use crate::asset_type::AssetType;
use crate::zone::{
    GfxImageGeometry, MaterialGeometry, Ptr, Result, TechniqueArgumentGeometry,
    TechniquePassGeometry, TechniqueSetGeometry, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream,
};

pub(super) fn load_technique_set(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::TECHNIQUE_SET, 408))?;
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

    let mut technique_slots = 0u64;
    let mut technique0 = None;
    let mut technique_body_by_slot = [None; sz::TECHNIQUE_SLOT_COUNT];
    for (i, body_identity) in technique_body_by_slot.iter_mut().enumerate() {
        match s.ptr_at(p, s.layout(12, 24) + i * s.pointer_bytes())? {
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
        technique_slots |= 1 << i;
    }

    let mut scanned = 0u64;
    let mut uses_model_lighting_const = false;
    let mut max_pass_count = 0u16;
    let mut pass_count_by_slot = [0u8; sz::TECHNIQUE_SLOT_COUNT];
    let mut technique_flags_by_slot = [0u16; sz::TECHNIQUE_SLOT_COUNT];
    for i in 0..sz::TECHNIQUE_SLOT_COUNT {
        if s.begin_body(p.at(s.layout(12, 24) + i * s.pointer_bytes()))? {
            let (head, pass_count) =
                load_technique(s, links, &mut uses_model_lighting_const, i as u8)?;
            technique_body_by_slot[i] = Some(head);
            scanned |= 1 << i;
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
        name,
        technique_slots,
        technique_slots_scanned: scanned,
        technique_body_by_slot,
        technique0_flags,
        world_vert_format,
        uses_model_lighting_const,
        max_pass_count,
        pass_count_by_slot,
        technique_flags_by_slot,
    });

    s.pop()
}

fn load_technique(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    uses_model_lighting_const: &mut bool,
    tech_slot: u8,
) -> Result<(Ptr, u16)> {
    let head = s.alloc_load(4, s.layout(8, 16))?;
    let flags = s.u16_at(head, s.layout(4, 8))?;
    let pass_count = s.u16_at(head, s.layout(6, 10))?;
    let passes = s.alloc_load(1, s.layout(sz::MATERIAL_PASS, 40) * pass_count as usize)?;

    for i in 0..pass_count as usize {
        let row = load_material_pass(
            s,
            links,
            passes.at(i * s.layout(sz::MATERIAL_PASS, 40)),
            uses_model_lighting_const,
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
    uses_model_lighting_const: &mut bool,
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

    asset_ptr_at(s, links, AssetType::VertexDecl, p.at(0))?;
    asset_ptr_at(s, links, AssetType::VertexShader, p.at(s.layout(4, 8)))?;
    asset_ptr_at(s, links, AssetType::PixelShader, p.at(s.layout(8, 16)))?;

    if s.begin_body(p.at(s.layout(16, 32)))? {
        let args = s.alloc_load(4, s.layout(sz::MATERIAL_SHADER_ARGUMENT, 16) * arg_count)?;
        for i in 0..arg_count {
            let a = args.at(i * s.layout(sz::MATERIAL_SHADER_ARGUMENT, 16));
            let ty = s.u16_at(a, 0)?;
            let mut argument = TechniqueArgumentGeometry::default();
            for (offset, byte) in argument.raw.iter_mut().enumerate() {
                *byte = s.u8_at(
                    a,
                    if offset < 4 {
                        offset
                    } else {
                        s.layout(4, 8) + offset - 4
                    },
                )?;
            }
            if ty == mtl_arg::LITERAL_VERTEX_CONST || ty == mtl_arg::LITERAL_PIXEL_CONST {
                let literal = match s.ptr_at(a, s.layout(4, 8))? {
                    ZonePtr::Null => None,
                    ZonePtr::Offset(body) => {
                        s.begin_body(a.at(s.layout(4, 8)))?;
                        Some(s.resolve_alias(body))
                    }
                    ZonePtr::Following | ZonePtr::Insert => {
                        s.begin_body(a.at(s.layout(4, 8)))?;
                        Some(s.alloc_load(4, 16)?)
                    }
                };
                if let Some(literal) = literal {
                    for (index, word) in argument.literal_words.iter_mut().enumerate() {
                        *word = s.u32_at(literal, index * 4)?;
                    }
                    argument.literal_present = true;
                }
            } else if s.u16_at(a, s.layout(4, 8))? == asset_iw4::material::CODE_CONST_MODEL_LIGHTING
            {
                *uses_model_lighting_const = true;
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

pub(super) fn load_material(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::MATERIAL, 120))?;

    let texture_count = s.u8_at(p, s.layout(72, 80))? as usize;
    let constant_count = s.u8_at(p, s.layout(73, 81))? as usize;
    let state_bits_count = s.u8_at(p, s.layout(74, 82))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    asset_ptr_at(s, links, AssetType::TechniqueSet, p.at(s.layout(80, 88)))?;

    let mut textures = None;
    if s.begin_body(p.at(s.layout(84, 96)))? {
        let arr = s.alloc_load(4, s.layout(sz::MATERIAL_TEXTURE_DEF, 16) * texture_count)?;
        textures = Some(arr);
        for i in 0..texture_count {
            let b = arr.at(i * s.layout(sz::MATERIAL_TEXTURE_DEF, 16));

            const SEMANTIC_WATER: u8 = 11;
            if s.u8_at(b, 7)? == SEMANTIC_WATER {
                load_water(s, links, b.at(8))?;
            } else {
                asset_ptr_at(s, links, AssetType::Image, b.at(8))?;
            }
        }
    }

    let constants = s.plain_array(
        p,
        s.layout(88, 104),
        16,
        sz::MATERIAL_CONSTANT_DEF,
        constant_count,
    )?;
    let state_bits = s.plain_array(
        p,
        s.layout(92, 112),
        4,
        sz::GFX_STATE_BITS,
        state_bits_count,
    )?;

    let mut state_bits_entry = [0u8; sz::TECHNIQUE_SLOT_COUNT];
    for (index, entry) in state_bits_entry.iter_mut().enumerate() {
        *entry = s.u8_at(
            p,
            s.layout(asset_iw4::material::MATERIAL_STATE_BITS_ENTRY, 32) + index,
        )?;
    }

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    s.record_material(MaterialGeometry {
        name,
        header: Some(p),
        draw_surf: u64::from(s.u32_at(p, s.layout(8, 16))?)
            | (u64::from(s.u32_at(p, s.layout(12, 20))?) << 32),
        sort_key: s.u8_at(p, s.layout(5, 9))?,
        info_game_flags: s.u8_at(p, s.layout(4, 8))?,
        texture_atlas: [s.u8_at(p, s.layout(6, 10))?, s.u8_at(p, s.layout(7, 11))?],
        surface_type_bits: Some(s.u32_at(
            p,
            s.layout(asset_iw4::material::MATERIAL_SURFACE_TYPE_BITS, 24),
        )?),
        state_flags: s.u8_at(p, s.layout(asset_iw4::material::MATERIAL_STATE_FLAGS, 83))?,
        camera_region: s.u8_at(p, s.layout(asset_iw4::material::MATERIAL_CAMERA_REGION, 84))?,
        state_bits,
        state_bits_count,
        state_bits_entry: Some(state_bits_entry),
        technique_set: Some(p.at(s.layout(80, 88))),
        textures,
        texture_count,
        texture_stride: s.layout(sz::MATERIAL_TEXTURE_DEF, 16),
        constants,
        constant_count,
    });

    s.pop()
}

fn load_water(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, slot: Ptr) -> Result<()> {
    if !s.begin_body(slot)? {
        return Ok(());
    }
    let water = s.alloc_load(4, s.layout(sz::WATER, 96))?;
    let width = s.i32_at(water, s.layout(12, 32))?.max(0) as usize;
    let height = s.i32_at(water, s.layout(16, 36))?.max(0) as usize;
    let count = width.saturating_mul(height);
    if s.wire_format() == crate::Iw4WireFormat::X64 {
        s.plain_array(water, 8, 4, 4, count)?;
        s.plain_array(water, 16, 4, 4, count)?;
        s.plain_array(water, 24, 4, 4, count)?;
    } else {
        s.plain_array(water, 4, 4, 8, count)?;
        s.plain_array(water, 8, 4, 4, count)?;
    }
    asset_ptr_at(s, links, AssetType::Image, water.at(s.layout(64, 88)))
}

pub(super) fn load_shader(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::PIXEL_SHADER, 32))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let load_def = p.at(s.layout(8, 16));
    let program_words = s.u16_at(load_def, s.layout(4, 8))? as usize;
    let program = s.plain_array(load_def, 0, 4, 4, program_words)?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    s.record_shader(crate::zone::ShaderGeometry {
        name,
        program,
        program_words,
    });

    s.pop()
}

pub(super) fn load_image(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::GFX_IMAGE, 40))?;

    let map_type = s.u8_at(p, s.layout(4, 8))?;
    let semantic = s.u8_at(p, s.layout(5, 9))?;
    let category = s.u8_at(p, s.layout(6, 10))?;
    let use_srgb_reads = s.u8_at(p, s.layout(7, 11))? != 0;
    let width = s.u16_at(p, s.layout(20, 24))?;
    let height = s.u16_at(p, s.layout(22, 26))?;
    let depth = s.u16_at(p, s.layout(24, 28))?;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, s.layout(28, 32))?;
    let name = match s.ptr_at(p, s.layout(28, 32))? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };

    let mut level_count = 0;
    let mut format = 0;
    let mut source_offset = 0;
    let mut source_len = 0;
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

pub(super) fn load_phys_collmap(
    s: &mut ZoneStream<'_>,
    _links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::PHYS_COLLMAP, 88))?;
    let count = s.u32_at(p, s.layout(4, 8))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    if s.begin_body(p.at(s.layout(8, 16)))? {
        let arr = s.alloc_load(4, s.layout(sz::PHYS_GEOM_INFO, 72) * count)?;
        for i in 0..count {
            let g = arr.at(i * s.layout(sz::PHYS_GEOM_INFO, 72));
            if s.begin_body(g.at(0))? {
                load_brush_wrapper(s)?;
            }
        }
    }

    s.pop()
}

fn load_brush_wrapper(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::BRUSH_WRAPPER, 88))?;
    let numsides = s.u16_at(p, 24)? as usize;
    let total_edge_count = s.i32_at(p, s.layout(60, 72))?.max(0) as usize;

    if s.begin_body(p.at(s.layout(28, 32)))? {
        let sides = s.alloc_load(4, s.layout(sz::CBRUSH_SIDE, 16) * numsides)?;
        for i in 0..numsides {
            let sp = sides.at(i * s.layout(sz::CBRUSH_SIDE, 16));
            if s.begin_body(sp.at(0))? {
                s.alloc_load(4, sz::CPLANE)?;
            }
        }
    }

    s.plain_array(p, s.layout(32, 40), 1, 1, total_edge_count)?;
    s.plain_array(p, s.layout(64, 80), 4, sz::CPLANE, numsides)?;
    Ok(())
}
