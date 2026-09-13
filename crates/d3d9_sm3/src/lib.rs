#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod abi;
pub mod bytecode;
pub mod ir;
pub mod wgsl;

pub use abi::{
    AlphaTest, CompareFunc, ConstantSlot, DeclType, PassLoweringAbi, SamplerSlot,
    SamplerTextureDimension, Semantic, VaryingLink, VertexInput, missing_vertex_element,
};
pub use bytecode::{Opcode, ShaderStage, TokenError, TokenStream};
pub use ir::{
    OpcodeSurfaceCoverage, SUPPORTED_OPCODE_SURFACE, Sm3Destination, Sm3Instruction,
    Sm3InstructionBody, Sm3IrError, Sm3LoweringRefusal, Sm3Opcode, Sm3ProgramIr, Sm3Register,
    Sm3RegisterFile, Sm3RelativeAddress, Sm3Source, Sm3SourceModifier, TEXLD_PROJECT,
    decode_sm3_program, named_sm3_opcode_count, validate_supported_opcode_surface,
};
pub use wgsl::{
    PASS_FRAGMENT_ENTRY, PASS_VERTEX_ENTRY, PassWgsl, Sm3Wgsl, Sm3WgslError,
    TEXTURE_TABLE_BINDING_2D, TEXTURE_TABLE_BINDING_3D, TEXTURE_TABLE_BINDING_CUBE,
    TEXTURE_TABLE_BINDING_SAMPLERS, TEXTURE_TABLE_GROUP, lower_pass_to_wgsl, lower_sm3_to_wgsl,
    pass_fragment_alpha_test_entry, texture_slot_rows, texture_slot_word,
};
