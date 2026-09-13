pub use render_material::{
    OpcodeSurfaceCoverage, SUPPORTED_OPCODE_SURFACE, Sm3Destination, Sm3Instruction,
    Sm3InstructionBody, Sm3IrError, Sm3LoweringRefusal, Sm3Opcode, Sm3ProgramIr, Sm3Register,
    Sm3RegisterFile, Sm3RelativeAddress, Sm3Source, Sm3SourceModifier, decode_sm3_program,
    measure_shader_surface_coverage, named_sm3_opcode_count, validate_supported_opcode_surface,
};
