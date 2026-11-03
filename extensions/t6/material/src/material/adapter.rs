use asset_core::FamilyId;
use dxbc_sm5::wgsl::{PassAbi, Shader};
use render_material::{
    MATERIAL_ALPHA_TESTS, MaterialAdapter, PassAbiRefusal, PassProgramAbi, PreparedMaterialProgram,
    PreparedShaderProgram, ProgramAbiRefusal, ProgramLoweringError, ProgramPreparation,
};

pub struct T6MaterialAdapter;
struct PreparedT6 {
    vertex: Shader,
    pixel: Shader,
    lowering: PassAbi,
}
impl MaterialAdapter for T6MaterialAdapter {
    fn lighting_constants(&self) -> render_material::LightingConstantBindings {
        render_material::LightingConstantBindings {
            reflection_sh: &crate::material::codes::CODE_T6_REFLECTION_SH,
            grid_sh: &crate::material::codes::CODE_T6_GRID_SH,
        }
    }

    fn prepare(
        &self,
        input: ProgramPreparation<'_>,
    ) -> Result<PreparedMaterialProgram, ProgramAbiRefusal> {
        let parse = |bytes: &[u8]| {
            Shader::parse(bytes).map_err(|_| ProgramAbiRefusal::Abi(PassAbiRefusal::InvalidProgram))
        };
        let vertex = parse(&input.vertex.program)?;
        let pixel = parse(&input.pixel.program)?;
        for (shader, expected) in [
            (&vertex, dxbc_sm5::ProgramKind::Vertex),
            (&pixel, dxbc_sm5::ProgramKind::Pixel),
        ] {
            if shader.kind != expected {
                return Err(ProgramAbiRefusal::Adapter {
                    family: FamilyId::new("t6"),
                    reason: format!("shader stage {:?}, expected {:?}", shader.kind, expected),
                });
            }
        }
        let (mut abi, mut lowering) = super::dxbc_abi::build_dxbc_pass_abi(
            &vertex,
            &pixel,
            input.declaration,
            input.vertex_type,
            &input.pass.arguments,
            input.pass.t5_custom_sampler_flags | input.pass.custom_sampler_flags,
        )
        .map_err(ProgramAbiRefusal::Abi)?;
        lowering.alpha_tests = alpha_tests();
        abi.depth_to_colour = input.pass.depth_to_colour;
        lowering.depth_to_colour = input.pass.depth_to_colour;
        Ok(PreparedMaterialProgram {
            abi,
            program: Box::new(PreparedT6 {
                vertex,
                pixel,
                lowering,
            }),
        })
    }
}
impl PreparedShaderProgram for PreparedT6 {
    fn cache_key(&self, _: &PassProgramAbi) -> Option<Vec<u8>> {
        None
    }
    fn lower(&self, abi: &PassProgramAbi) -> Result<d3d9_sm3::PassWgsl, ProgramLoweringError> {
        self.lower_variant(abi, None)
    }
    fn lower_exit_clip(
        &self,
        abi: &PassProgramAbi,
        mode: d3d9_sm3::ExitClipMode,
    ) -> Result<d3d9_sm3::PassWgsl, ProgramLoweringError> {
        let mode = match mode {
            d3d9_sm3::ExitClipMode::ClipDistance => dxbc_sm5::wgsl::ExitClipMode::ClipDistance,
            d3d9_sm3::ExitClipMode::FragmentDiscard => {
                dxbc_sm5::wgsl::ExitClipMode::FragmentDiscard
            }
        };
        self.lower_variant(abi, Some(mode))
    }
}
impl PreparedT6 {
    fn lower_variant(
        &self,
        abi: &PassProgramAbi,
        mode: Option<dxbc_sm5::wgsl::ExitClipMode>,
    ) -> Result<d3d9_sm3::PassWgsl, ProgramLoweringError> {
        let source = dxbc_sm5::wgsl::lower_pass_with_exit_clip(
            &self.lowering,
            &self.vertex,
            &self.pixel,
            mode,
        )
        .map_err(|error| ProgramLoweringError {
            family: FamilyId::new("t6"),
            reason: error.to_string(),
            census_key: format!(
                "Wgsl/{}",
                format!("{error:?}")
                    .split(['{', '('])
                    .next()
                    .unwrap_or("Unknown")
                    .trim()
            ),
        })?;
        Ok(d3d9_sm3::PassWgsl {
            source,
            attribute_count: abi.attributes.len(),
            varying_count: 0,
            vertex_constant_len: abi.vertex_constants.len(),
            pixel_constant_len: abi.pixel_constants.len(),
            sampler_count: abi.samplers.len(),
        })
    }
}
fn alpha_tests() -> Vec<(String, String)> {
    MATERIAL_ALPHA_TESTS
        .iter()
        .enumerate()
        .map(|(index, test)| {
            let operator = match test.func {
                d3d9_sm3::CompareFunc::Never => return (index, Some("discard;".to_owned())),
                d3d9_sm3::CompareFunc::Always => return (index, Some(String::new())),
                d3d9_sm3::CompareFunc::Less => "<",
                d3d9_sm3::CompareFunc::Equal => "==",
                d3d9_sm3::CompareFunc::LessEqual => "<=",
                d3d9_sm3::CompareFunc::Greater => ">",
                d3d9_sm3::CompareFunc::NotEqual => "!=",
                d3d9_sm3::CompareFunc::GreaterEqual => ">=",
                d3d9_sm3::CompareFunc::Unknown(_) => return (index, None),
            };
            (
                index,
                Some(format!(
                    "if !(round(clamp(dx_colour.a, 0.0, 1.0) * {:.1}) {operator} {:.1}) {{ discard; }}",
                    d3d9_sm3::abi::ALPHA_REF_SCALE,
                    f32::from(test.reference)
                )),
            )
        })
        .filter_map(|(index, test)| {
            Some((d3d9_sm3::pass_fragment_alpha_test_entry(index), test?))
        })
        .collect()
}
