use d3d9_sm3::ShaderStage;

use crate::RuntimeShaderStage;

pub use d3d9_sm3::{
    OpcodeSurfaceCoverage, SUPPORTED_OPCODE_SURFACE, Sm3Destination, Sm3Instruction,
    Sm3InstructionBody, Sm3IrError, Sm3LoweringRefusal, Sm3Opcode, Sm3ProgramIr, Sm3Register,
    Sm3RegisterFile, Sm3RelativeAddress, Sm3Source, Sm3SourceModifier, named_sm3_opcode_count,
    validate_supported_opcode_surface,
};

fn to_stage(stage: RuntimeShaderStage) -> ShaderStage {
    match stage {
        RuntimeShaderStage::Vertex => ShaderStage::Vertex,
        RuntimeShaderStage::Pixel => ShaderStage::Pixel,
    }
}

pub fn decode_sm3_program(
    bytes: &[u8],
    expected_stage: RuntimeShaderStage,
) -> Result<Sm3ProgramIr, Sm3IrError> {
    d3d9_sm3::decode_sm3_program(bytes, to_stage(expected_stage))
}

pub fn measure_shader_surface_coverage<'a>(
    programs: impl IntoIterator<Item = (&'a [u8], RuntimeShaderStage)>,
) -> OpcodeSurfaceCoverage {
    OpcodeSurfaceCoverage::measure_shaders(
        programs
            .into_iter()
            .map(|(bytes, stage)| (bytes, to_stage(stage))),
    )
}
