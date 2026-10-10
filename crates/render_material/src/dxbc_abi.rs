use std::collections::BTreeSet;

use asset_iw4::vertex_decl as vd;
use dxbc_sm5::wgsl::{
    ConstantRow, PassAbi, SampleAdapter, Shader, TextureDimension, TextureSlot, VertexInput,
};

use crate::argument::RuntimeArgumentBinding;
use crate::sm3::{Sm3Register, Sm3RegisterFile};
use crate::sm3_abi::{
    ConstantBinding, ConstantSource, PassAbiRefusal, PassProgramAbi, SamplerBinding, SamplerSource,
    SamplerTextureDimension, Semantic, VaryingBinding, VertexAttribute, routed_attributes,
};
use crate::stage::RuntimeShaderStage;
use crate::vertex_decl::RuntimeVertexDecl;

pub const CODE_T6_REFLECTION_SH: [u16; 3] = [0x303, 0x304, 0x305];
pub const CODE_T6_GRID_SH: [u16; 3] = [0x30c, 0x30d, 0x30e];
/// x: model-lighting texel scale; y: weight of the reflection-probe alpha
/// (T6 probes store rgb / alpha, probes from other maps keep no scale there).
pub const CODE_T6_SAMPLE_DECODE: u16 = 0x30f;

// D3D11 binds at most 14 constant buffers, so no shader reads buffer 14.
const SAMPLE_DECODE_ROW: ConstantRow = ConstantRow { buffer: 14, row: 0 };

pub(crate) fn is_dxbc_program(program: &[u8]) -> bool {
    program.starts_with(b"DXBC")
}

pub(crate) fn dxbc_constant_rows(shader: &Shader) -> Result<Vec<ConstantRow>, PassAbiRefusal> {
    Ok(shader
        .constant_rows()
        .map_err(|_| PassAbiRefusal::DxbcProgram)?
        .into_iter()
        .collect())
}

pub(crate) fn dxbc_texture_slots(
    vertex: &Shader,
    pixel: &Shader,
) -> Result<Vec<TextureSlot>, PassAbiRefusal> {
    let mut slots: BTreeSet<TextureSlot> = vertex
        .texture_slots()
        .map_err(|_| PassAbiRefusal::DxbcProgram)?;
    slots.extend(
        pixel
            .texture_slots()
            .map_err(|_| PassAbiRefusal::DxbcProgram)?,
    );
    Ok(slots.into_iter().collect())
}

const REFLECTION_PROBE_FLAG: u8 = 0x01;
const REFLECTION_PROBE_TEXTURE: u32 = 15;

fn semantic_of(name: &str, index: u32) -> Option<Semantic> {
    let usage = match name.to_ascii_uppercase().as_str() {
        "POSITION" => vd::D3DDECLUSAGE_POSITION,
        "NORMAL" => vd::D3DDECLUSAGE_NORMAL,
        "COLOR" => vd::D3DDECLUSAGE_COLOR,
        "TEXCOORD" => vd::D3DDECLUSAGE_TEXCOORD,
        _ => return None,
    };
    Some(Semantic {
        usage,
        usage_index: u8::try_from(index).ok()?,
    })
}

fn vertex_input(attribute: &VertexAttribute, register: u32, vertex_type: u8) -> VertexInput {
    let location = attribute.location;
    let a = format!("attribute_{location}");
    let (attribute_type, expression) = match attribute.layout.decl_type {
        vd::D3dDeclType::UByte4 => {
            let packed = format!("({a}.x | ({a}.y << 8u) | ({a}.z << 16u) | ({a}.w << 24u))");
            let unit = format!(
                "((vec3<f32>({a}.xyz) - vec3<f32>(127.0)) * ((f32({a}.w) + 192.0) / 32385.0))"
            );
            let expression = if attribute.semantic.usage == vd::D3DDECLUSAGE_TEXCOORD
                && attribute.semantic.usage_index == 0
            {
                format!("vec4<f32>(unpack2x16float({packed}).yx, 0.0, 1.0)")
            } else {
                // T6 decodes UNORM components >= 0.5 as negative. Repacked
                // IW4 vectors can reach or exceed +1, so keep the positive
                // endpoint below that boundary and the negative endpoint at -1.
                let unit = format!("clamp({unit}, vec3<f32>(-1.0), vec3<f32>(1022.0 / 1023.0))");
                format!(
                    "vec4<f32>(select({unit} * 0.5 + vec3<f32>(1.0), {unit} * 0.5, {unit} >= vec3<f32>(0.0)), 0.0)"
                )
            };
            ("vec4<u32>", expression)
        }
        vd::D3dDeclType::Float4
            if vertex_type >= 2
                && attribute.semantic.usage == vd::D3DDECLUSAGE_TEXCOORD
                && attribute.semantic.usage_index == 1 =>
        {
            ("vec4<f32>", format!("vec4<f32>({a}.zw, 0.0, 1.0)"))
        }
        vd::D3dDeclType::D3dColor => ("vec4<f32>", format!("{a}.zyxw")),
        vd::D3dDeclType::Float4 | vd::D3dDeclType::UByte4N => ("vec4<f32>", a),
        vd::D3dDeclType::Float3 => ("vec3<f32>", format!("vec4<f32>({a}, 1.0)")),
        vd::D3dDeclType::Float2 | vd::D3dDeclType::Float16x2 => {
            ("vec2<f32>", format!("vec4<f32>({a}, 0.0, 1.0)"))
        }
        vd::D3dDeclType::Unknown(_) => ("vec4<f32>", "vec4<f32>(0.0)".to_string()),
    };
    VertexInput {
        register: Some(register),
        location,
        attribute_type: attribute_type.to_string(),
        expression,
    }
}

fn constant_bindings(
    rows: &[ConstantRow],
    stage: RuntimeShaderStage,
    arguments: &[RuntimeArgumentBinding],
) -> Result<Vec<ConstantBinding>, PassAbiRefusal> {
    (0..rows.len() as u16)
        .map(|register| {
            let source = arguments.iter().find_map(|argument| match *argument {
                RuntimeArgumentBinding::LiteralConstant {
                    stage: s,
                    destination,
                    words,
                } if s == stage && destination == register => {
                    Some(ConstantSource::Literal { words })
                }
                RuntimeArgumentBinding::MaterialConstant {
                    stage: s,
                    destination,
                    name_hash,
                } if s == stage && destination == register => {
                    Some(ConstantSource::Material { name_hash })
                }
                RuntimeArgumentBinding::CodeConstant {
                    stage: s,
                    destination,
                    index,
                    first_row,
                    row_count,
                } if s == stage
                    && register >= destination
                    && register < destination + u16::from(row_count) =>
                {
                    Some(ConstantSource::Code {
                        index,
                        row: first_row + (register - destination) as u8,
                    })
                }
                _ => None,
            });
            source
                .map(|source| ConstantBinding { register, source })
                .ok_or(PassAbiRefusal::ConstantRegisterUnbound { stage, register })
        })
        .collect()
}

pub(crate) fn build_dxbc_pass_abi(
    vertex: &Shader,
    pixel: &Shader,
    decl: &RuntimeVertexDecl,
    vertex_type: u8,
    arguments: &[RuntimeArgumentBinding],
    custom_sampler_flags: u8,
) -> Result<(PassProgramAbi, PassAbi), PassAbiRefusal> {
    let cloud_corners = vertex_type == vd::POS_TEX_VERTEX_TYPE
        && vertex
            .reflection
            .constant_buffers
            .iter()
            .flat_map(|buffer| &buffer.variables)
            .any(|variable| variable.name == "particleCloudMatrix" && variable.flags & 2 != 0);
    if usize::from(decl.stream_count) > vd::ROUTING_COUNT {
        return Err(PassAbiRefusal::StreamCountOutOfRange {
            stream_count: decl.stream_count,
        });
    }
    let mut selected = decl.clone();
    selected.stream_count = 0;
    for &[source, dest] in decl.routed() {
        let required = match decl.family.destination_usage(dest) {
            Some((usage, usage_index)) => {
                let semantic = Semantic { usage, usage_index };
                vertex.input.elements.iter().any(|element| {
                    semantic_of(&element.semantic, element.semantic_index) == Some(semantic)
                }) || (cloud_corners && usage == vd::D3DDECLUSAGE_TEXCOORD && usage_index == 0)
            }
            None => true,
        };
        if required {
            selected.routing[usize::from(selected.stream_count)] = [source, dest];
            selected.stream_count += 1;
        }
    }
    let attributes = routed_attributes(&selected, vertex_type)?;
    let mut vertex_inputs = Vec::new();
    let mut used_attributes = Vec::new();
    for element in &vertex.input.elements {
        let semantic = semantic_of(&element.semantic, element.semantic_index).ok_or(
            PassAbiRefusal::DxbcVertexInputUnrouted {
                register: element.register,
            },
        )?;
        let Some(attribute) = attributes.iter().find(|a| a.semantic == semantic) else {
            if semantic.usage == vd::D3DDECLUSAGE_POSITION {
                return Err(PassAbiRefusal::DxbcVertexInputUnrouted {
                    register: element.register,
                });
            }
            continue;
        };
        vertex_inputs.push(vertex_input(attribute, element.register, vertex_type));
        used_attributes.push(*attribute);
    }

    if cloud_corners {
        let position = attributes
            .iter()
            .find(|a| a.semantic.usage == vd::D3DDECLUSAGE_POSITION);
        let uv = attributes
            .iter()
            .find(|a| a.semantic.usage == vd::D3DDECLUSAGE_TEXCOORD && a.semantic.usage_index == 0);
        if let (Some(position), Some(uv)) = (position, uv)
            && let Some(input) = vertex_inputs
                .iter_mut()
                .find(|input| input.location == position.location)
        {
            input.expression = format!(
                "vec4<f32>(attribute_{}, (attribute_{}.x + 2.0 * attribute_{}.y) * 0.25)",
                position.location, uv.location, uv.location
            );
            if !used_attributes.iter().any(|a| a.location == uv.location) {
                let mut input = vertex_input(uv, 0, vertex_type);
                input.register = None;
                vertex_inputs.push(input);
                used_attributes.push(*uv);
            }
        }
    }

    let vertex_rows = dxbc_constant_rows(vertex)?;
    let mut pixel_rows = dxbc_constant_rows(pixel)?;
    let vertex_constants = constant_bindings(&vertex_rows, RuntimeShaderStage::Vertex, arguments)?;
    let mut pixel_constants = constant_bindings(&pixel_rows, RuntimeShaderStage::Pixel, arguments)?;

    let textures = dxbc_texture_slots(vertex, pixel)?;
    let mut samplers = Vec::with_capacity(textures.len());
    for (register, slot) in (0u16..).zip(&textures) {
        let source = arguments
            .iter()
            .find_map(|argument| match *argument {
                RuntimeArgumentBinding::MaterialTexture {
                    destination,
                    name_hash,
                } if destination == register => Some(SamplerSource::MaterialTexture { name_hash }),
                RuntimeArgumentBinding::CodeTexture { destination, index }
                    if destination == register =>
                {
                    Some(SamplerSource::CodeTexture { index })
                }
                _ => None,
            })
            .or_else(|| match (slot.texture, custom_sampler_flags) {
                (REFLECTION_PROBE_TEXTURE, flags) if flags & REFLECTION_PROBE_FLAG != 0 => {
                    Some(SamplerSource::SurfaceReflectionProbe)
                }
                (13, flags) if flags & 2 != 0 => Some(SamplerSource::SurfaceSecondaryLightmap),
                _ => None,
            })
            .ok_or(PassAbiRefusal::SamplerRegisterUnbound { register })?;
        samplers.push(SamplerBinding {
            register,
            source,
            dimension: match slot.dimension {
                TextureDimension::D2 => SamplerTextureDimension::D2,
                TextureDimension::Cube => SamplerTextureDimension::Cube,
                TextureDimension::D3 => SamplerTextureDimension::D3,
            },
            depth_compare: false,
        });
    }

    let model_lighting = SamplerSource::CodeTexture {
        index: u32::from(lighting_iw4::TEXTURE_SRC_CODE_MODEL_LIGHTING),
    };
    if samplers.iter().any(|binding| {
        binding.source == model_lighting || binding.source == SamplerSource::SurfaceReflectionProbe
    }) {
        pixel_rows.push(SAMPLE_DECODE_ROW);
        pixel_constants = constant_bindings(&pixel_rows, RuntimeShaderStage::Pixel, arguments)?;
    }
    let depth_near_row = pixel_constants.iter().find_map(|binding| {
        matches!(
            binding.source,
            ConstantSource::Code {
                index: 0x21,
                row: 0
            }
        )
        .then(|| pixel_rows[usize::from(binding.register)])
    });
    let sample_adapters = samplers
        .iter()
        .enumerate()
        .filter_map(|(slot, binding)| match binding.source {
            source if source == model_lighting => Some(SampleAdapter {
                slot,
                opaque_alpha: false,
                square_rgb: false,
                rgb_scale: 1.0,
                scale_row: Some(SAMPLE_DECODE_ROW),
                alpha_row: None,
                depth_near_row: None,
            }),
            SamplerSource::SurfaceReflectionProbe => Some(SampleAdapter {
                slot,
                opaque_alpha: false,
                square_rgb: false,
                rgb_scale: 1.0,
                scale_row: None,
                alpha_row: Some(SAMPLE_DECODE_ROW),
                depth_near_row: None,
            }),
            SamplerSource::CodeTexture { index: 0x0f } => depth_near_row.map(|row| SampleAdapter {
                slot,
                opaque_alpha: false,
                square_rgb: false,
                rgb_scale: 1.0,
                scale_row: None,
                alpha_row: None,
                depth_near_row: Some(row),
            }),
            _ => None,
        })
        .collect();
    let position = VaryingBinding {
        semantic: Semantic {
            usage: vd::D3DDECLUSAGE_POSITION,
            usage_index: 0,
        },
        vertex_register: Sm3Register {
            file: Sm3RegisterFile::Output,
            index: 0,
        },
        pixel_register: None,
        location: 0,
    };
    let program = PassProgramAbi {
        vertex_family: decl.family,
        vertex_type,
        attributes: used_attributes,
        vertex_inputs: Vec::new(),
        position,
        varyings: Vec::new(),
        vertex_constants,
        pixel_constants,
        samplers,
    };
    let lowering = PassAbi {
        vertex_inputs,
        vertex_constants: vertex_rows,
        pixel_constants: pixel_rows,
        sample_adapters,
        textures,
        alpha_tests: Vec::new(),
    };
    Ok((program, lowering))
}
