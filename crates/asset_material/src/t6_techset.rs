//! T6 technique sets in the IW4 technique graph.
//!
//! T6 (PC) shaders are DX11: they read constant-buffer rows, and a pass's
//! arguments fill rows by byte offset and buffer rather than registers. A
//! T6 technique set becomes a technique set like any other here: each IW4
//! technique slot holds the T6 technique of the same kind, its passes keep
//! their DXBC programs, and their arguments are written in IW4 terms on
//! virtual registers — the rows each stage reads, and the textures the pass
//! samples, numbered in sorted order (the order `render_material::dxbc_abi`
//! binds them in).
//!
//! A row a T6 argument fills keeps that argument's source; a row the T6
//! renderer fills itself (its per-scene and per-object buffers) is named
//! in the shader's reflection and bound to the IW4 code constant of that
//! meaning, or to a fixed value where IW4 has none.

use std::collections::BTreeSet;

use dxbc_sm5::wgsl::{ConstantRow, Shader, TextureDimension, TextureSlot};
use dxbc_sm5::{BindingKind, Reflection};
use fastfile_iw4::AssetType;

use crate::asset_graph::AssetRef;
use crate::material_catalog::{
    AssetPointerIdentity, AuthoredShader, AuthoredVertexDecl, MaterialCatalog, OwnedMaterialPass,
    OwnedShaderArgument, OwnedShaderRef, OwnedTechnique, OwnedTechniqueGraph, TechniqueSetFacts,
    TechniqueTable,
};
use crate::t5_code_remap::{iw4_code_const_index, iw4_code_texture_index};
use crate::t5_tech_map::{IW4_TECHNIQUE_TYPE_COUNT, IW4_TECHNIQUE_TYPE_NAMES};

/// `MaterialShaderArgument::type`.
pub mod argument_type {
    pub const MATERIAL_VERTEX_CONST: u16 = 0;
    pub const LITERAL_VERTEX_CONST: u16 = 1;
    pub const MATERIAL_PIXEL_SAMPLER: u16 = 2;
    pub const CODE_VERTEX_CONST: u16 = 3;
    pub const CODE_PIXEL_SAMPLER: u16 = 4;
    pub const CODE_PIXEL_CONST: u16 = 5;
    pub const MATERIAL_PIXEL_CONST: u16 = 6;
    pub const LITERAL_PIXEL_CONST: u16 = 7;
}

/// A T6 `MaterialShaderArgument`: `{ u16 type; u16 offset; u8 size; ...;
/// u16 buffer; u32 def }`, with the literal a literal argument points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct T6Argument {
    pub kind: u16,
    /// Bytes into the constant buffer; for samplers `texture | sampler << 8`.
    pub offset: u16,
    /// Bytes the argument fills.
    pub size: u8,
    pub buffer: u16,
    /// A name hash (material constants and samplers) or a code index.
    pub def: u32,
    pub literal: Option<[u32; 4]>,
}

#[derive(Clone, Debug)]
pub struct T6Pass {
    pub vertex_name: String,
    pub vertex: Vec<u8>,
    pub pixel_name: String,
    pub pixel: Vec<u8>,
    pub custom_sampler_flags: u8,
    pub arguments: Vec<T6Argument>,
}

#[derive(Clone, Debug)]
pub struct T6Technique {
    pub flags: u16,
    pub passes: Vec<T6Pass>,
}

#[derive(Clone, Debug)]
pub struct T6TechniqueSet {
    pub name: String,
    /// `MaterialTechniqueSet::techniques`, indexed by [`T6_TECHNIQUE_TYPE_NAMES`].
    pub techniques: Vec<Option<T6Technique>>,
}

/// `MaterialTechniqueType` (T6, 36 slots).
/// The `lit` technique's index in [`T6_TECHNIQUE_TYPE_NAMES`].
pub const T6_TECHNIQUE_LIT: usize = 4;
/// The `emissive` technique's index.
pub const T6_TECHNIQUE_EMISSIVE: usize = 3;

/// How a T6 material draws: lit like a gun body, or emissive like an optic's
/// reticle (a material whose `lit` technique is off). T6's `unlit` technique
/// is Radiant's preview, not what the game draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T6Draw {
    Lit,
    Emissive,
}

impl T6Draw {
    /// The technique set `set` is linked as for this kind of drawing.
    pub fn technique_set_name(self, set: &str) -> String {
        match self {
            Self::Lit => set.to_owned(),
            Self::Emissive => format!("{set}$emissive"),
        }
    }
}

/// The load-bits word 0 fields T6 shares with IW4 for how a pass writes
/// colour: the colour blend (bits 0–10), the alpha test (11–13), the alpha
/// blend (16–26) and sRGB writes (30). T6 shaders return gamma-encoded
/// colour (`sqrt`), written as is.
const T6_COLOUR_OUTPUT: u32 = 0x47ff_3fff;
/// Load-bits word 1's depth write.
const GFXS1_DEPTHWRITE: u32 = 0x1;

pub const T6_TECHNIQUE_TYPE_NAMES: [&str; 36] = [
    "depth prepass",
    "build shadowmap depth",
    "unlit",
    "emissive",
    "lit",
    "lit sun",
    "lit sun shadow",
    "lit spot",
    "lit spot shadow",
    "lit spot square",
    "lit spot square shadow",
    "lit spot round",
    "lit spot round shadow",
    "lit omni",
    "lit omni shadow",
    "lit dlight glight",
    "lit sun dlight glight",
    "lit sun shadow dlight glight",
    "lit spot dlight glight",
    "lit spot shadow dlight glight",
    "lit spot square dlight glight",
    "lit spot square shadow dlight glight",
    "lit spot round dlight glight",
    "lit spot round shadow dlight glight",
    "lit omni dlight glight",
    "lit omni shadow dlight glight",
    "light spot",
    "light omni",
    "fakelight normal",
    "fakelight view",
    "sunlight preview",
    "case texture",
    "solid wireframe",
    "shaded wireframe",
    "debug bumpmap",
    "debug performance",
];

/// The T6 techniques an IW4 slot can draw with, best first. Distance-fog
/// variants draw the plain technique; instanced ones (static model batches)
/// have none. A lit technique a set lacks falls back to the same light
/// without its shadow, then to plain `lit`.
fn t6_techniques_for_iw4_slot(iw4: &str) -> Vec<usize> {
    let name = iw4.strip_suffix(" dfog").unwrap_or(iw4);
    let mut names = vec![name.to_owned()];
    if let Some(unshadowed) = name.strip_suffix(" shadow") {
        names.push(unshadowed.to_owned());
    }
    if name.starts_with("lit") && !name.contains("instanced") {
        names.push("lit".to_owned());
        // Last, any lit technique the set has.
        names.extend(
            T6_TECHNIQUE_TYPE_NAMES
                .iter()
                .filter(|t6| t6.starts_with("lit"))
                .map(|t6| (*t6).to_owned()),
        );
    }
    names
        .iter()
        .filter_map(|name| T6_TECHNIQUE_TYPE_NAMES.iter().position(|t6| t6 == name))
        .collect()
}

/// What the T6 renderer itself puts in a constant-buffer row, in IW4 terms.
enum EngineValue {
    Code(&'static str),
    Literal([f32; 4]),
}

/// Constant-buffer variables the T6 renderer fills, by reflection name.
/// Unlisted names read zeros.
///
/// T6 constant buffers pack matrices column-major: the row a `dp4` reads is
/// a column of the matrix, which is the row IW4's `TRANSPOSE_*` code
/// matrices hold (its shaders `dp4` the same way). An inverse matrix is
/// therefore IW4's `INVERSE_TRANSPOSE_*`, and an inverse-transpose its
/// plain `INVERSE_*`.
fn engine_value(name: &str) -> EngineValue {
    use EngineValue::{Code, Literal};
    match name {
        "viewProjectionMatrix" => Code("TRANSPOSE_VIEW_PROJECTION_MATRIX"),
        "projectionMatrix" => Code("TRANSPOSE_PROJECTION_MATRIX"),
        "inverseViewMatrix" => Code("INVERSE_TRANSPOSE_VIEW_MATRIX"),
        "inverseViewProjectionMatrix" => Code("INVERSE_TRANSPOSE_VIEW_PROJECTION_MATRIX"),
        "shadowLookupMatrix" => Code("TRANSPOSE_SHADOW_LOOKUP_MATRIX"),
        "worldOutdoorLookupMatrix" => Code("TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX"),
        "worldMatrix" => Code("TRANSPOSE_WORLD_MATRIX0"),
        "worldViewMatrix" => Code("TRANSPOSE_WORLD_VIEW_MATRIX0"),
        "worldViewProjectionMatrix" => Code("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX0"),
        "inverseWorldViewMatrix" => Code("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX0"),
        "inverseTransposeWorldViewMatrix" => Code("INVERSE_WORLD_VIEW_MATRIX0"),
        "inverseTransposeWorldMatrix" => Code("INVERSE_WORLD_MATRIX0"),
        // IW4 draws the sun as the pass light of its sun techniques.
        "sunPosition" | "lightPosition" => Code("LIGHT_POSITION"),
        "sunDiffuse" | "lightDiffuse" => Code("LIGHT_DIFFUSE"),
        "lightSpotDir" => Code("LIGHT_SPOTDIR"),
        "lightSpotFactors" => Code("LIGHT_SPOTFACTORS"),
        "shadowmapSwitchPartition" => Code("SHADOWMAP_SWITCH_PARTITION"),
        "sunShadowmapPixelSize" => Code("SUN_SHADOWMAP_PIXEL_ADJUST"),
        "spotShadowmapPixelAdjust" => Code("SPOT_SHADOWMAP_PIXEL_ADJUST"),
        "shadowmapPolygonOffset" => Code("SHADOWMAP_POLYGON_OFFSET"),
        "zNear" => Code("ZNEAR"),
        "materialColor" => Code("MATERIAL_COLOR"),
        "renderTargetSize" => Code("RENDER_TARGET_SIZE"),
        "gameTime" => Code("GAMETIME"),
        "clipSpaceLookupScale" => Code("CLIP_SPACE_LOOKUP_SCALE"),
        "clipSpaceLookupOffset" => Code("CLIP_SPACE_LOOKUP_OFFSET"),
        "colorMatrixR" => Code("COLOR_MATRIX_R"),
        "colorMatrixG" => Code("COLOR_MATRIX_G"),
        "colorMatrixB" => Code("COLOR_MATRIX_B"),
        "outdoorFeatherParms" => Code("OUTDOOR_FEATHER_PARMS"),
        "particleCloudColor" => Code("PARTICLE_CLOUD_COLOR"),
        // HDR exposure: none, IW4 draws in the gamma space T6 resolves to.
        "hdrControl0" | "hdrControl1" | "colorTint" | "lightHeroScale" => Literal([1.0; 4]),
        // The colour transform character shaders end with: identity.
        "heroLightingR" => Literal([1.0, 0.0, 0.0, 0.0]),
        "heroLightingG" => Literal([0.0, 1.0, 0.0, 0.0]),
        "heroLightingB" => Literal([0.0, 0.0, 1.0, 0.0]),
        // Reticles scale by `1 - y` and fade in with `w`: drawn whole.
        "weaponParam0" => Literal([0.0, 0.0, 0.0, 1.0]),
        // The light grid: T6 reads its model lighting volume where IW4 reads
        // its own, at the model's base coordinates offset along the normal.
        "gridLightingCoordsAndVis" => Code("BASE_LIGHTING_COORDS"),
        "lightingLookupScale" => Code("LIGHTING_LOOKUP_SCALE"),
        // Both spherical harmonics agree, so the reflection probe is scaled
        // by one.
        "gridLightingSH0"
        | "gridLightingSH1"
        | "gridLightingSH2"
        | "reflectionLightingSH0"
        | "reflectionLightingSH1"
        | "reflectionLightingSH2" => Literal([0.25; 4]),
        // Fog colours with no weight leave the colour unfogged.
        _ => Literal([0.0; 4]),
    }
}

/// The reflection variable covering a constant-buffer row: its name and
/// the row it starts at.
fn variable_at(reflection: &Reflection, row: ConstantRow) -> Option<(&str, u32)> {
    let binding = reflection
        .bindings
        .iter()
        .find(|b| b.kind == BindingKind::ConstantBuffer && b.bind_point == row.buffer)?;
    let buffer = reflection
        .constant_buffers
        .iter()
        .find(|buffer| buffer.name == binding.name)?;
    let at = row.row * 16;
    buffer
        .variables
        .iter()
        .find(|v| v.start_offset <= at && at < v.start_offset + v.size.max(1))
        .map(|v| (v.name.as_str(), v.start_offset / 16))
}

fn texture_name(reflection: &Reflection, texture: u32) -> Option<&str> {
    reflection
        .bindings
        .iter()
        .find(|b| b.kind == BindingKind::Texture && b.bind_point == texture)
        .map(|b| b.name.as_str())
}

/// When the runtime refreshes an argument: per primitive (world matrices),
/// per object (other code values), or once per material.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Primitive,
    Object,
    Stable,
}

fn code_tier(index: u16) -> Tier {
    let world = iw4_code_const_index("WORLD_MATRIX0").unwrap_or(u16::MAX);
    let last = iw4_code_const_index("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX2").unwrap_or(0);
    if (world..=last).contains(&index) {
        Tier::Primitive
    } else {
        Tier::Object
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Vertex,
    Pixel,
}

/// The material constants of `kind` a pass fills row `row` of `buffer`
/// with.
fn material_arguments_in_row(
    arguments: &[T6Argument],
    kind: u16,
    buffer: u32,
    row: u32,
) -> Vec<&T6Argument> {
    arguments
        .iter()
        .filter(|a| {
            a.kind == kind
                && u32::from(a.buffer) == buffer
                && u32::from(a.offset) / 16 <= row
                && row * 16 < u32::from(a.offset) + u32::from(a.size).max(1)
        })
        .collect()
}

/// A row that T6 packs with several material constants (`Specular_Amount`,
/// `Specular_Decay` and `Reflection_Amount` in one), or with one that does
/// not start it, binds a constant of its own: this hash names it, and
/// [`MaterialCatalog::t6_material`] assembles its value from the material's
/// constants.
fn packed_row_hash(arguments: &[&T6Argument]) -> Option<u32> {
    const FNV_OFFSET: u32 = 0x811c_9dc5;
    const FNV_PRIME: u32 = 0x0100_0193;
    let packed = arguments.len() > 1 || arguments.iter().any(|a| a.offset % 16 != 0 || a.size < 16);
    packed.then(|| {
        arguments.iter().fold(FNV_OFFSET, |hash, a| {
            [a.def, u32::from(a.offset % 16)]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .fold(hash, |hash, byte| {
                    (hash ^ u32::from(byte)).wrapping_mul(FNV_PRIME)
                })
        })
    })
}

fn constant_argument(
    stage: Stage,
    destination: u16,
    row: ConstantRow,
    reflection: &Reflection,
    arguments: &[T6Argument],
    gaps: &mut BTreeSet<String>,
) -> (Tier, OwnedShaderArgument) {
    use argument_type as t;
    let (material_kind, literal_kind, code_kind) = match stage {
        Stage::Vertex => (
            t::MATERIAL_VERTEX_CONST,
            t::LITERAL_VERTEX_CONST,
            t::CODE_VERTEX_CONST,
        ),
        Stage::Pixel => (
            t::MATERIAL_PIXEL_CONST,
            t::LITERAL_PIXEL_CONST,
            t::CODE_PIXEL_CONST,
        ),
    };
    let covering = arguments.iter().find(|a| {
        [material_kind, literal_kind, code_kind].contains(&a.kind)
            && u32::from(a.buffer) == row.buffer
            && u32::from(a.offset) / 16 <= row.row
            && row.row * 16 < u32::from(a.offset) + u32::from(a.size).max(16)
    });
    match covering {
        Some(a) if a.kind == material_kind => {
            let in_row = material_arguments_in_row(arguments, material_kind, row.buffer, row.row);
            let name_hash = packed_row_hash(&in_row).unwrap_or(a.def);
            let argument = match stage {
                Stage::Vertex => OwnedShaderArgument::MaterialVertexConstant {
                    destination,
                    name_hash,
                },
                Stage::Pixel => OwnedShaderArgument::MaterialPixelConstant {
                    destination,
                    name_hash,
                },
            };
            return (Tier::Stable, argument);
        }
        Some(a) if a.kind == literal_kind => {
            return (
                Tier::Stable,
                literal_argument(stage, destination, a.literal),
            );
        }
        _ => {}
    }
    let Some((name, first)) = variable_at(reflection, row) else {
        gaps.insert(format!("cb{}[{}]", row.buffer, row.row));
        return (
            Tier::Stable,
            literal_argument(stage, destination, Some([0; 4])),
        );
    };
    match engine_value(name) {
        EngineValue::Code(code) => {
            let Some(index) = iw4_code_const_index(code) else {
                gaps.insert(format!("{name}: no IW4 {code}"));
                return (
                    Tier::Stable,
                    literal_argument(stage, destination, Some([0; 4])),
                );
            };
            let first_row = (row.row - first) as u8;
            let argument = match stage {
                Stage::Vertex => OwnedShaderArgument::CodeVertexConstant {
                    destination,
                    index,
                    first_row,
                    row_count: 1,
                },
                Stage::Pixel => OwnedShaderArgument::CodePixelConstant {
                    destination,
                    index,
                    first_row,
                    row_count: 1,
                },
            };
            (code_tier(index), argument)
        }
        EngineValue::Literal(value) => (
            Tier::Stable,
            literal_argument(stage, destination, Some(value.map(f32::to_bits))),
        ),
    }
}

fn literal_argument(
    stage: Stage,
    destination: u16,
    words: Option<[u32; 4]>,
) -> OwnedShaderArgument {
    match stage {
        Stage::Vertex => OwnedShaderArgument::LiteralVertexConstant { destination, words },
        Stage::Pixel => OwnedShaderArgument::LiteralPixelConstant { destination, words },
    }
}

/// The argument binding texture slot `destination`, or `None` when a
/// custom sampler flag binds it (the reflection probe).
fn sampler_argument(
    destination: u16,
    slot: TextureSlot,
    reflection: &Reflection,
    arguments: &[T6Argument],
    gaps: &mut BTreeSet<String>,
) -> Option<(Tier, OwnedShaderArgument)> {
    use argument_type as t;
    let bound = arguments.iter().find(|a| {
        (a.kind == t::MATERIAL_PIXEL_SAMPLER || a.kind == t::CODE_PIXEL_SAMPLER)
            && u32::from(a.offset & 0xff) == slot.texture
    });
    if let Some(a) = bound.filter(|a| a.kind == t::MATERIAL_PIXEL_SAMPLER) {
        return Some((
            Tier::Stable,
            OwnedShaderArgument::MaterialPixelSampler {
                destination,
                name_hash: a.def,
            },
        ));
    }
    let name = texture_name(reflection, slot.texture).unwrap_or("");
    let lower = name.to_ascii_lowercase();
    let code = if lower.contains("reflectionprobe") {
        return None;
    } else if lower.contains("modellighting") {
        "MODEL_LIGHTING"
    } else if lower.contains("shadowmap") && lower.contains("sun") {
        "SHADOWMAP_SUN"
    } else if lower.contains("shadowmap") {
        "SHADOWMAP_SPOT"
    } else if lower.contains("attenuation") {
        "LIGHT_ATTENUATION"
    } else if lower.contains("outdoor") {
        "OUTDOOR"
    } else if lower.contains("floatz") {
        // T6 viewmodel shaders (reticles) discard against the scene's
        // device depth, which IW4's FLOATZ does not hold and a viewmodel
        // pass binds no code texture for: a black stand-in never fails it.
        return Some((
            Tier::Stable,
            OwnedShaderArgument::MaterialPixelSampler {
                destination,
                name_hash: FLOAT_Z_HASH,
            },
        ));
    } else {
        gaps.insert(format!("t{} {name}", slot.texture));
        if slot.dimension == TextureDimension::D2 {
            "WHITE"
        } else {
            "BLACK"
        }
    };
    let index = iw4_code_texture_index(code)?;
    Some((
        Tier::Object,
        OwnedShaderArgument::CodePixelSampler { destination, index },
    ))
}

/// Whether `pass` samples the scene depth (`floatZSampler`).
fn reads_float_z(pass: &T6Pass) -> bool {
    let Ok(pixel) = Shader::parse(&pass.pixel) else {
        return false;
    };
    pixel.texture_slots().is_ok_and(|slots| {
        slots.iter().any(|slot| {
            texture_name(&pixel.reflection, slot.texture)
                .is_some_and(|name| name.to_ascii_lowercase().contains("floatz"))
        })
    })
}

/// The vertex declaration every T6 pass reads: the five IW4 packed-vertex
/// sources (position with the binormal sign, colour, texture coordinates,
/// normal, tangent) under the semantics T6 shaders name them by.
fn t6_vertex_decl() -> AuthoredVertexDecl {
    let mut routing = [[0u8; 2]; asset_iw4::vertex_decl::ROUTING_COUNT];
    // [source, destination]: POSITION, COLOR0, TEXCOORD0, NORMAL, TEXCOORD2.
    for (at, pair) in [[0, 0], [1, 2], [2, 5], [3, 1], [4, 7]]
        .into_iter()
        .enumerate()
    {
        routing[at] = pair;
    }
    AuthoredVertexDecl {
        family: crate::VertexLayoutFamily::Iw4,
        name: AssetRef::Real("$t6_packed_vertex".to_owned()),
        stream_count: 5,
        has_optional_source: 0,
        routing,
    }
}

impl MaterialCatalog {
    fn t6_pass(
        &mut self,
        pass: &T6Pass,
        pass_index: u8,
        vertex_decl: usize,
        gaps: &mut BTreeSet<String>,
    ) -> Result<OwnedMaterialPass, String> {
        let vertex =
            Shader::parse(&pass.vertex).map_err(|e| format!("{}: {e}", pass.vertex_name))?;
        let pixel = Shader::parse(&pass.pixel).map_err(|e| format!("{}: {e}", pass.pixel_name))?;
        let rows = |shader: &Shader| -> Result<Vec<ConstantRow>, String> {
            Ok(shader
                .constant_rows()
                .map_err(|e| e.to_string())?
                .into_iter()
                .collect())
        };
        let mut arguments = Vec::new();
        for (stage, shader) in [(Stage::Vertex, &vertex), (Stage::Pixel, &pixel)] {
            for (destination, row) in (0u16..).zip(rows(shader)?) {
                arguments.push(constant_argument(
                    stage,
                    destination,
                    row,
                    &shader.reflection,
                    &pass.arguments,
                    gaps,
                ));
            }
        }
        let mut slots = vertex.texture_slots().map_err(|e| e.to_string())?;
        slots.extend(pixel.texture_slots().map_err(|e| e.to_string())?);
        for (destination, slot) in (0u16..).zip(slots) {
            arguments.extend(sampler_argument(
                destination,
                slot,
                &pixel.reflection,
                &pass.arguments,
                gaps,
            ));
        }
        arguments.sort_by_key(|(tier, _)| *tier);
        let count = |tier| arguments.iter().filter(|(t, _)| *t == tier).count() as u8;
        let (per_prim, per_obj, stable) = (
            count(Tier::Primitive),
            count(Tier::Object),
            count(Tier::Stable),
        );
        let shader_ref =
            |this: &mut Self, name: &str, kind, program: &[u8], tag: u32| OwnedShaderRef {
                pointer_identity: AssetPointerIdentity {
                    block: u8::MAX,
                    offset: tag,
                },
                shader: Some(this.link_shader(AuthoredShader {
                    namespace: crate::AssetNamespace::Iw4,
                    name: AssetRef::Real(name.to_owned()),
                    kind,
                    program: program.to_vec(),
                })),
            };
        Ok(OwnedMaterialPass {
            pass_index,
            vertex_decl_identity: AssetPointerIdentity {
                block: u8::MAX,
                offset: vertex_decl as u32,
            },
            vertex_decl: Some(vertex_decl),
            vertex_shader: shader_ref(
                self,
                &pass.vertex_name,
                AssetType::VertexShader,
                &pass.vertex,
                0,
            ),
            pixel_shader: shader_ref(
                self,
                &pass.pixel_name,
                AssetType::PixelShader,
                &pass.pixel,
                1,
            ),
            per_prim_arg_count: per_prim,
            per_obj_arg_count: per_obj,
            stable_arg_count: stable,
            custom_sampler_flags: 0,
            // The T5 flag layout: 0x01 binds the reflection probe to t15.
            t5_custom_sampler_flags: pass.custom_sampler_flags,
            arguments: arguments
                .into_iter()
                .map(|(_, argument)| argument)
                .collect(),
            arguments_truncated: false,
        })
    }

    /// Links `set` as an IW4-namespace technique set of the same name, and
    /// returns its index; `report` gets a line per technique that could not
    /// be translated and per value IW4 has no source for.
    pub fn link_t6_technique_set(
        &mut self,
        set: &T6TechniqueSet,
        draw: T6Draw,
        report: &mut Vec<String>,
    ) -> usize {
        let vertex_decl = self.link_vertex_decl(t6_vertex_decl());
        let mut gaps = BTreeSet::new();
        let mut slots = vec![None; IW4_TECHNIQUE_TYPE_COUNT];
        let mut table = TechniqueTable::default();
        for (iw4_slot, iw4_name) in IW4_TECHNIQUE_TYPE_NAMES.iter().enumerate() {
            // Emissive, every slot that draws colour (lit ones included)
            // draws the emissive technique; depth and shadow-map passes skip it.
            let candidates = match draw {
                T6Draw::Lit => t6_techniques_for_iw4_slot(iw4_name),
                T6Draw::Emissive
                    if iw4_name.starts_with("depth") || iw4_name.starts_with("build") =>
                {
                    Vec::new()
                }
                T6Draw::Emissive => vec![T6_TECHNIQUE_EMISSIVE],
            };
            let Some(technique) = candidates
                .into_iter()
                .find_map(|t6| set.techniques.get(t6)?.as_ref())
            else {
                continue;
            };
            let passes: Result<Vec<_>, String> = (0u8..)
                .zip(&technique.passes)
                .map(|(index, pass)| self.t6_pass(pass, index, vertex_decl, &mut gaps))
                .collect();
            match passes {
                Ok(passes) => {
                    table.slots |= 1 << iw4_slot;
                    table.scanned |= 1 << iw4_slot;
                    table.pass_count_by_slot[iw4_slot] = passes.len() as u8;
                    table.max_pass_count = table.max_pass_count.max(passes.len() as u16);
                    slots[iw4_slot] = Some(OwnedTechnique {
                        flags: 0,
                        passes,
                        body_scanned: true,
                    });
                }
                Err(error) => report.push(format!("t6 techset {} {iw4_name}: {error}", set.name)),
            }
        }
        report.push(format!(
            "t6 techset {}: T6 techniques {:?}, IW4 slots filled {}",
            set.name,
            set.techniques
                .iter()
                .enumerate()
                .filter(|(_, t)| t.is_some())
                .map(|(i, _)| T6_TECHNIQUE_TYPE_NAMES.get(i).copied().unwrap_or("?"))
                .collect::<Vec<_>>(),
            table.slots.count_ones()
        ));
        if !gaps.is_empty() {
            report.push(format!(
                "t6 techset {}: no IW4 source for {:?}",
                set.name, gaps
            ));
        }
        table.graph = Some(OwnedTechniqueGraph {
            slots,
            rows_truncated: 0,
            arguments_truncated: 0,
        });
        self.link_techset(TechniqueSetFacts {
            namespace: crate::AssetNamespace::Iw4,
            name: AssetRef::Real(draw.technique_set_name(&set.name)),
            table: Some(table),
            ..Default::default()
        })
    }
}

/// The samplers T6 materials name their colour, normal and specular maps
/// by (`colorMap`, `normalMap`, `specularMap`, the first-person arms' own,
/// and the lenses' colour).
const COLOR_MAP_HASHES: [u32; 3] = [0xa0ab_1041, 0xf039_ec2d, 0xb607_c0fe];
const NORMAL_MAP_HASH: u32 = 0x59d3_0d0f;
const ARM_NORMAL_MAP_HASH: u32 = 0x942c_bff0;
const SPECULAR_MAP_HASH: u32 = 0x34ec_ccb3;
/// The material sampler a pass's `floatZSampler` binds instead (no T6
/// name hashes to it).
const FLOAT_Z_HASH: u32 = 0x666c_745a;
const ARM_SPECULAR_MAP_HASH: u32 = 0x8c29_7e80;

/// The names one map's sampler goes by. A material drawn with another
/// kind's technique set (the arms' datapad and watch glass, named the
/// weapon and lens way) names its maps differently from the samplers.
fn sampler_aliases(hash: u32) -> &'static [u32] {
    const NORMAL: [u32; 2] = [NORMAL_MAP_HASH, ARM_NORMAL_MAP_HASH];
    const SPECULAR: [u32; 2] = [SPECULAR_MAP_HASH, ARM_SPECULAR_MAP_HASH];
    [&COLOR_MAP_HASHES[..], &NORMAL[..], &SPECULAR[..]]
        .into_iter()
        .find(|group| group.contains(&hash))
        .unwrap_or(&[])
}

/// `occlusionAmount`: how much of the sun's specular (x), the reflection
/// probe (y) and the ambient light (z) a T6 lit surface takes.
const OCCLUSION_AMOUNT_HASH: u32 = 0x9027_e5c1;
/// The share of T6's specular and probe reflection kept under IW4's
/// lighting. T6 lights in HDR and exposes the frame; IW4's sun colour and
/// LDR probes are already at display brightness, so T6's normalized
/// highlights read about twice as strong as BO2 shows them. Tuned by eye.
const T6_SPECULAR_SCALE: f32 = 0.5;

/// A texture a T6 material binds: its sampler's name hash and state, the
/// image name and its texels as T6 shaders read them.
#[derive(Clone, Debug)]
pub struct T6Texture {
    pub name_hash: u32,
    pub sampler_state: u8,
    pub semantic: u8,
    pub image: String,
    pub texels: std::sync::Arc<bevy::prelude::Image>,
}

impl MaterialCatalog {
    /// A T6 material drawn with its own technique set `set`: the `donor`
    /// (an IW4 material) lends its state bits, sort and draw route; the
    /// textures and constants are the T6 material's own. A texture the set
    /// samples that the material does not have reads a default, as in T6:
    /// a flat normal, no specular, white otherwise.
    #[allow(clippy::too_many_arguments)]
    pub fn t6_material(
        &mut self,
        donor: usize,
        name: &str,
        set: &T6TechniqueSet,
        textures: &[T6Texture],
        constants: Vec<crate::MaterialConstant>,
        lit_state: Option<u32>,
        draw: T6Draw,
    ) -> Option<usize> {
        let technique_set = draw.technique_set_name(&set.name);
        let technique_set = technique_set.as_str();
        let sampled: BTreeSet<u32> = set
            .techniques
            .iter()
            .flatten()
            .flat_map(|technique| &technique.passes)
            .flat_map(|pass| &pass.arguments)
            .filter(|a| a.kind == argument_type::MATERIAL_PIXEL_SAMPLER)
            .map(|a| a.def)
            .chain(
                set.techniques
                    .iter()
                    .flatten()
                    .flat_map(|technique| &technique.passes)
                    .any(reads_float_z)
                    .then_some(FLOAT_Z_HASH),
            )
            .collect();
        let defaults: Vec<T6Texture> = sampled
            .into_iter()
            .filter(|hash| !textures.iter().any(|t| t.name_hash == *hash))
            .map(|hash| {
                if let Some(texture) = sampler_aliases(hash)
                    .iter()
                    .find_map(|alias| textures.iter().find(|t| t.name_hash == *alias))
                {
                    return T6Texture {
                        name_hash: hash,
                        ..texture.clone()
                    };
                }
                let (image, rgba) = match hash {
                    NORMAL_MAP_HASH | ARM_NORMAL_MAP_HASH => {
                        ("$t6_flat_normal", [128, 128, 255, 255])
                    }
                    SPECULAR_MAP_HASH | ARM_SPECULAR_MAP_HASH | FLOAT_Z_HASH => {
                        ("$t6_black", [0, 0, 0, 0])
                    }
                    _ => ("$t6_white", [255; 4]),
                };
                T6Texture {
                    name_hash: hash,
                    sampler_state: 0x12,
                    semantic: crate::TS_2D,
                    image: image.to_owned(),
                    texels: std::sync::Arc::new(crate::solid_texture(rgba, false)),
                }
            })
            .collect();
        let textures: Vec<&T6Texture> = textures.iter().chain(&defaults).collect();
        let mut constants = constants;
        for constant in &mut constants {
            if constant.name_hash == OCCLUSION_AMOUNT_HASH {
                constant.literal[0] *= T6_SPECULAR_SCALE;
                constant.literal[1] *= T6_SPECULAR_SCALE;
            }
        }
        // The packed rows' constants, assembled per component from the
        // material's own.
        for pass in set
            .techniques
            .iter()
            .flatten()
            .flat_map(|technique| &technique.passes)
        {
            for kind in [
                argument_type::MATERIAL_VERTEX_CONST,
                argument_type::MATERIAL_PIXEL_CONST,
            ] {
                let rows: BTreeSet<(u32, u32)> = pass
                    .arguments
                    .iter()
                    .filter(|a| a.kind == kind)
                    .map(|a| (u32::from(a.buffer), u32::from(a.offset) / 16))
                    .collect();
                for (buffer, row) in rows {
                    let in_row = material_arguments_in_row(&pass.arguments, kind, buffer, row);
                    let Some(hash) = packed_row_hash(&in_row) else {
                        continue;
                    };
                    if constants.iter().any(|c| c.name_hash == hash) {
                        continue;
                    }
                    let mut literal = [0.0f32; 4];
                    for a in &in_row {
                        let Some(source) = constants.iter().find(|c| c.name_hash == a.def) else {
                            continue;
                        };
                        let first = (usize::from(a.offset) % 16) / 4;
                        let count = usize::from(a.size).div_ceil(4).clamp(1, 4 - first);
                        literal[first..first + count].copy_from_slice(&source.literal[..count]);
                    }
                    constants.push(crate::MaterialConstant {
                        name_hash: hash,
                        name: *b"t6_packed\0\0\0",
                        literal,
                    });
                }
            }
        }
        let mut material = self.materials.get(donor)?.clone();
        material.name = AssetRef::Real(name.to_owned());
        material.technique_set = AssetRef::Real(technique_set.to_owned());
        material.technique_set_edge = Default::default();
        material.technique_table = None;
        material.constants = constants;
        // A reticle draws over its sight's lens, which shares the donor.
        if draw == T6Draw::Emissive {
            material.sort_key = material.sort_key.saturating_add(1);
        }
        // The donor's draw states, with the T6 material's own blend, alpha
        // test and sRGB writes: a donor lens or alpha-tested body would
        // otherwise cut or fade an opaque T6 surface by its colour map's
        // alpha, and an IW4 donor's sRGB writes encode T6's gamma output
        // a second time.
        if let Some(lit) = lit_state {
            // A blended surface (a lens, a reticle) keeps the depth buffer
            // to the surfaces behind it, as IW4's own blended materials do.
            let blends = !matches!((lit >> 4) & 0xf, 0 | 1);
            for bits in &mut material.state_bits {
                bits[0] = (bits[0] & !T6_COLOUR_OUTPUT) | (lit & T6_COLOUR_OUTPUT);
                if blends {
                    bits[1] &= !GFXS1_DEPTHWRITE;
                }
            }
        }
        let namespace = material.namespace;
        material.textures = textures
            .into_iter()
            .map(|texture| {
                let texels = &texture.texels;
                let image = self.link_image(crate::AuthoredImage {
                    namespace,
                    name: AssetRef::Real(texture.image.clone()),
                    map_type: 3,
                    semantic: texture.semantic,
                    category: 0,
                    use_srgb_reads: false,
                    width: texels.width() as u16,
                    height: texels.height() as u16,
                    depth: 1,
                    level_count: texels.texture_descriptor.mip_level_count as u8,
                    format: 0,
                    payload: std::sync::Arc::new(Vec::new()),
                    decoded: Some(texels.clone()),
                    common_owned: false,
                    decoded_variant: None,
                    decoded_by: None,
                    pending_decode: None,
                });
                crate::MaterialTextureBinding {
                    name_hash: texture.name_hash,
                    name_start: 0,
                    name_end: 0,
                    sampler_state: texture.sampler_state,
                    semantic: texture.semantic,
                    image: Some(image),
                }
            })
            .collect();
        Some(self.link_material(material))
    }
}
