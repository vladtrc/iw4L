use crate::{
    ConstantSource, MATERIAL_ALPHA_TESTS, MaterialAdapter, PassProgramAbi, PreparedMaterialProgram,
    PreparedShaderProgram, ProgramAbiRefusal, ProgramLoweringError, ProgramPreparation,
    RuntimeShaderStage, Sm3ProgramIr,
};
use d3d9_sm3::{ConstantSlot, DeclType, PassLoweringAbi, SamplerSlot, VaryingLink, VertexInput};

pub struct Sm3MaterialAdapter;
struct PreparedSm3 {
    vertex: Sm3ProgramIr,
    pixel: Sm3ProgramIr,
}
impl MaterialAdapter for Sm3MaterialAdapter {
    fn lighting_constants(&self) -> crate::LightingConstantBindings {
        crate::LightingConstantBindings::default()
    }
    fn prepare(
        &self,
        input: ProgramPreparation<'_>,
    ) -> Result<PreparedMaterialProgram, ProgramAbiRefusal> {
        let decode = |program: &crate::RuntimeShaderProgram, stage| {
            crate::decode_sm3_program(&program.program, stage)
                .map_err(|cause| ProgramAbiRefusal::Decode { stage, cause })
        };
        let vertex = decode(input.vertex, RuntimeShaderStage::Vertex)?;
        let pixel = decode(input.pixel, RuntimeShaderStage::Pixel)?;
        let abi = crate::sm3_abi::build_pass_abi(
            &vertex,
            &pixel,
            input.declaration,
            input.vertex_type,
            input.pass,
        )
        .map_err(ProgramAbiRefusal::Abi)?;
        Ok(PreparedMaterialProgram {
            abi,
            program: Box::new(PreparedSm3 { vertex, pixel }),
        })
    }
}
impl PreparedShaderProgram for PreparedSm3 {
    fn lower_exit_clip(
        &self,
        abi: &PassProgramAbi,
        mode: d3d9_sm3::ExitClipMode,
    ) -> Result<d3d9_sm3::PassWgsl, ProgramLoweringError> {
        d3d9_sm3::lower_pass_to_wgsl_with_exit_clip(
            &pass_lowering_abi(abi),
            &self.vertex,
            &self.pixel,
            Some(mode),
        )
        .map_err(|error| ProgramLoweringError {
            family: abi.vertex_layout.owner,
            reason: format!("{error:?}"),
            census_key: lowering_census(&error),
        })
    }
    fn cache_key(&self, abi: &PassProgramAbi) -> Option<Vec<u8>> {
        Some(format!("sm3-v2:{:?}", pass_lowering_abi(abi)).into_bytes())
    }
    fn lower(&self, abi: &PassProgramAbi) -> Result<d3d9_sm3::PassWgsl, ProgramLoweringError> {
        d3d9_sm3::lower_pass_to_wgsl(&pass_lowering_abi(abi), &self.vertex, &self.pixel).map_err(
            |error| ProgramLoweringError {
                family: abi.vertex_layout.owner,
                reason: format!("{error:?}"),
                census_key: lowering_census(&error),
            },
        )
    }
}
fn known_decl_type(decl_type: DeclType) -> DeclType {
    match decl_type {
        DeclType::Unknown(_) => unreachable!("the ABI refuses unknown declaration types"),
        known => known,
    }
}

pub fn pass_lowering_abi(abi: &PassProgramAbi) -> PassLoweringAbi {
    PassLoweringAbi {
        vertex_inputs: abi
            .vertex_inputs
            .iter()
            .map(|input| VertexInput {
                register: input.register,
                location: input.attribute.map(|attribute| attribute.location),
                decl_type: input
                    .attribute
                    .map(|attribute| known_decl_type(attribute.layout.decl_type)),
                semantic: input.semantic,
            })
            .collect(),
        position: VaryingLink {
            semantic: abi.position.semantic,
            vertex_register: abi.position.vertex_register,
            pixel_register: abi.position.pixel_register,
            location: abi.position.location,
        },
        varyings: abi
            .varyings
            .iter()
            .map(|varying| VaryingLink {
                semantic: varying.semantic,
                vertex_register: varying.vertex_register,
                pixel_register: varying.pixel_register,
                location: varying.location,
            })
            .collect(),
        vertex_constants: abi
            .vertex_constants
            .iter()
            .map(|binding| ConstantSlot {
                register: binding.register,
                program_defined: binding.source == ConstantSource::ProgramDefined,
            })
            .collect(),
        pixel_constants: abi
            .pixel_constants
            .iter()
            .map(|binding| ConstantSlot {
                register: binding.register,
                program_defined: binding.source == ConstantSource::ProgramDefined,
            })
            .collect(),
        samplers: abi
            .samplers
            .iter()
            .map(|binding| SamplerSlot {
                register: binding.register,
                dimension: binding.dimension,
                depth_compare: binding.depth_compare,
            })
            .collect(),
        alpha_tests: MATERIAL_ALPHA_TESTS.to_vec(),
        depth_to_colour: abi.depth_to_colour,
    }
}

fn lowering_census(error: &d3d9_sm3::Sm3WgslError) -> String {
    use d3d9_sm3::Sm3WgslError as Error;
    match error {
        Error::LoweringSurface(crate::Sm3LoweringRefusal::UnknownOpcode { opcode, .. }) => {
            format!("UnknownOpcode({opcode:#x})")
        }
        Error::UnsupportedInstructionControls {
            opcode, controls, ..
        } => format!("Wgsl/UnsupportedInstructionControls({opcode:?},{controls})"),
        Error::UnsupportedSamplerTextureType { texture_type, .. } => {
            format!("Wgsl/UnsupportedSamplerTextureType({texture_type})")
        }
        Error::UnbalancedControlFlow { kind, .. } => format!("Wgsl/UnbalancedControlFlow({kind})"),
        other => format!(
            "Wgsl/{}",
            format!("{other:?}")
                .split(['{', '('])
                .next()
                .unwrap_or("Unknown")
                .trim()
        ),
    }
}
