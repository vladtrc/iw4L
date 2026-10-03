//! The pass ABI of a DXBC (Shader Model 4/5) pass: T6 technique passes, run
//! through the same constant arena and texture tables as SM3 passes.
//!
//! A DXBC stage reads constant-buffer rows (`cb2[5]`) rather than numbered
//! registers. The rows a stage reads, in sorted order, are numbered from 0
//! and those numbers are the `destination` registers its arguments bind; the
//! texture/sampler pairs the program samples are numbered the same way. The
//! importer that builds the pass arguments numbers them identically (see
//! [`dxbc_constant_rows`] and [`dxbc_texture_slots`]).

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

/// A shader program in a `DXBC` container rather than a D3D9 token stream.
pub fn is_dxbc_program(program: &[u8]) -> bool {
    program.starts_with(b"DXBC")
}

/// The constant-buffer rows a stage reads, in the order its virtual
/// registers number them.
pub fn dxbc_constant_rows(shader: &Shader) -> Result<Vec<ConstantRow>, PassAbiRefusal> {
    Ok(shader
        .constant_rows()
        .map_err(|_| PassAbiRefusal::DxbcProgram)?
        .into_iter()
        .collect())
}

/// The texture/sampler pairs a pass samples, both stages together, in the
/// order its virtual sampler registers number them.
pub fn dxbc_texture_slots(
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

/// The T5/T6 reflection probe is bound by custom sampler flag 0x01 to
/// texture register 15.
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

/// The WGSL that turns an IW4 packed-vertex attribute into what a T6 vertex
/// shader reads for `semantic`:
///
/// * texture coordinates: two halves, `u` in the high 16 bits;
/// * normals and tangents (`NORMAL`, `TEXCOORD2+`): IW4 scaled bytes
///   (`(b - 127) * (w + 192) / 32385`) re-encoded the way T6's unorm
///   10:10:10 fields read (`n / 2`, plus one when negative), which its
///   shaders decode with `x >= 0.5 ? 2x - 2 : 2x`;
/// * position: `xyz` and the binormal sign in `w`;
/// * colour: the T6 bytes as stored.
fn vertex_input(attribute: &VertexAttribute, register: u32) -> VertexInput {
    let location = attribute.location;
    let a = format!("attribute_{location}");
    let (attribute_type, expression) = match attribute.layout.decl_type {
        vd::D3dDeclType::UByte4 => {
            let packed = format!("({a}.x | ({a}.y << 8u) | ({a}.z << 16u) | ({a}.w << 24u))");
            let unit = format!(
                "((vec3<f32>({a}.xyz) - vec3<f32>(127.0)) * ((f32({a}.w) + 192.0) / 32385.0))"
            );
            let expression = if attribute.semantic.usage == vd::D3DDECLUSAGE_TEXCOORD
                && attribute.semantic.usage_index < 2
            {
                format!("vec4<f32>(unpack2x16float({packed}).yx, 0.0, 1.0)")
            } else {
                format!(
                    "vec4<f32>(select({unit} * 0.5 + vec3<f32>(1.0), {unit} * 0.5, {unit} >= vec3<f32>(0.0)), 0.0)"
                )
            };
            ("vec4<u32>", expression)
        }
        vd::D3dDeclType::Float4 | vd::D3dDeclType::UByte4N | vd::D3dDeclType::D3dColor => {
            ("vec4<f32>", a)
        }
        vd::D3dDeclType::Float3 => ("vec3<f32>", format!("vec4<f32>({a}, 1.0)")),
        vd::D3dDeclType::Float2 | vd::D3dDeclType::Float16x2 => {
            ("vec2<f32>", format!("vec4<f32>({a}, 0.0, 1.0)"))
        }
        vd::D3dDeclType::Unknown(_) => ("vec4<f32>", "vec4<f32>(0.0)".to_string()),
    };
    VertexInput {
        register,
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
                // A DXBC stage can read any rows of a matrix, so its
                // arguments name the row they start at.
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

/// The pass's program ABI, for the arena and pipeline layout, and the
/// lowering ABI the WGSL is generated against.
pub fn build_dxbc_pass_abi(
    vertex: &Shader,
    pixel: &Shader,
    decl: &RuntimeVertexDecl,
    vertex_type: u8,
    arguments: &[RuntimeArgumentBinding],
    custom_sampler_flags: u8,
) -> Result<(PassProgramAbi, PassAbi), PassAbiRefusal> {
    let attributes = routed_attributes(decl, vertex_type)?;
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
        vertex_inputs.push(vertex_input(attribute, element.register));
        used_attributes.push(*attribute);
    }

    let vertex_rows = dxbc_constant_rows(vertex)?;
    let pixel_rows = dxbc_constant_rows(pixel)?;
    let vertex_constants = constant_bindings(&vertex_rows, RuntimeShaderStage::Vertex, arguments)?;
    let pixel_constants = constant_bindings(&pixel_rows, RuntimeShaderStage::Pixel, arguments)?;

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
            .or_else(|| {
                (custom_sampler_flags & REFLECTION_PROBE_FLAG != 0
                    && slot.texture == REFLECTION_PROBE_TEXTURE)
                    .then_some(SamplerSource::SurfaceReflectionProbe)
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
            // T6 shaders compare shadow-map depth themselves.
            depth_compare: false,
        });
    }

    let model_lighting = SamplerSource::CodeTexture {
        index: u32::from(lighting_iw4::TEXTURE_SRC_CODE_MODEL_LIGHTING),
    };
    let sample_adapters = samplers
        .iter()
        .enumerate()
        .filter_map(|(slot, binding)| match binding.source {
            // IW4's reflection probes are LDR and gamma-space; T6 shaders
            // read linear colour divided by an HDR alpha.
            SamplerSource::SurfaceReflectionProbe => Some(SampleAdapter {
                slot,
                opaque_alpha: true,
                square_rgb: true,
                rgb_scale: 1.0,
            }),
            // T6 lights a model with its light-grid sample as `32 s²`;
            // IW4's volume holds what IW4 lights with as `(2 s)²`. Read
            // through 1/√8, T6's lighting comes out as IW4's.
            source if source == model_lighting => Some(SampleAdapter {
                slot,
                opaque_alpha: false,
                square_rgb: false,
                rgb_scale: core::f32::consts::FRAC_1_SQRT_2 / 2.0,
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
