use super::compiler::MaterialCompiler;
use super::program_hash;
use crate::{MaterialDefinitions, OwnedTechnique};
use render_material::{
    RuntimeArgumentBinding, RuntimePass, RuntimeShaderPair, RuntimeShaderProgramId,
    RuntimeShaderStage, RuntimeTechnique, RuntimeVertexDecl, sort_pass_args,
};

pub(super) fn compile_technique(
    compiler: &dyn MaterialCompiler,
    source: &MaterialDefinitions,
    slot: u8,
    technique: &OwnedTechnique,
    vertex_decls: &mut Vec<(u32, RuntimeVertexDecl)>,
) -> RuntimeTechnique {
    let passes = technique
        .passes
        .iter()
        .map(|pass| {
            let shader = |reference: &crate::OwnedShaderRef| {
                reference
                    .shader
                    .and_then(|index| source.shaders.get(index))
                    .and_then(|shader| {
                        u32::try_from(reference.shader?).ok().map(|asset_slot| {
                            RuntimeShaderProgramId {
                                asset_slot,
                                program_hash: program_hash(&shader.program),
                            }
                        })
                    })
            };

            let vertex_decl_slot = pass
                .vertex_decl
                .and_then(|id| u32::try_from(id).ok())
                .unwrap_or(u32::MAX);
            if let Some(authored) = pass
                .vertex_decl
                .and_then(|index| source.vertex_decls.get(index))
            {
                let record = RuntimeVertexDecl {
                    layout: authored.layout,
                    name: authored.name.to_string(),
                    stream_count: authored.stream_count,
                    has_optional_source: authored.has_optional_source,
                    routing: authored.routing,
                };
                match vertex_decls
                    .binary_search_by_key(&vertex_decl_slot, |(identity, _)| *identity)
                {
                    Ok(_) => {}
                    Err(at) => vertex_decls.insert(at, (vertex_decl_slot, record)),
                }
            }
            let shader_pair = shader(&pass.vertex_shader)
                .zip(shader(&pass.pixel_shader))
                .map(|(vertex, pixel)| RuntimeShaderPair {
                    vertex,
                    pixel,
                    vertex_decl_slot,
                });
            let arguments = pass
                .arguments
                .iter()
                .map(|argument| match argument {
                    crate::OwnedShaderArgument::MaterialVertexConstant {
                        destination,
                        name_hash,
                    } => RuntimeArgumentBinding::MaterialConstant {
                        stage: RuntimeShaderStage::Vertex,
                        destination: *destination,
                        name_hash: *name_hash,
                    },
                    crate::OwnedShaderArgument::LiteralVertexConstant { destination, words } => {
                        RuntimeArgumentBinding::LiteralConstant {
                            stage: RuntimeShaderStage::Vertex,
                            destination: *destination,
                            words: *words,
                        }
                    }
                    crate::OwnedShaderArgument::MaterialPixelSampler {
                        destination,
                        name_hash,
                    } => RuntimeArgumentBinding::MaterialTexture {
                        destination: *destination,
                        name_hash: *name_hash,
                    },
                    crate::OwnedShaderArgument::CodeVertexConstant {
                        destination,
                        index,
                        first_row,
                        row_count,
                    } => RuntimeArgumentBinding::CodeConstant {
                        stage: RuntimeShaderStage::Vertex,
                        destination: *destination,
                        index: *index,
                        first_row: *first_row,
                        row_count: *row_count,
                    },
                    crate::OwnedShaderArgument::CodePixelSampler { destination, index } => {
                        RuntimeArgumentBinding::CodeTexture {
                            destination: *destination,
                            index: *index,
                        }
                    }
                    crate::OwnedShaderArgument::CodePixelConstant {
                        destination,
                        index,
                        first_row,
                        row_count,
                    } => RuntimeArgumentBinding::CodeConstant {
                        stage: RuntimeShaderStage::Pixel,
                        destination: *destination,
                        index: *index,
                        first_row: *first_row,
                        row_count: *row_count,
                    },
                    crate::OwnedShaderArgument::MaterialPixelConstant {
                        destination,
                        name_hash,
                    } => RuntimeArgumentBinding::MaterialConstant {
                        stage: RuntimeShaderStage::Pixel,
                        destination: *destination,
                        name_hash: *name_hash,
                    },
                    crate::OwnedShaderArgument::LiteralPixelConstant { destination, words } => {
                        RuntimeArgumentBinding::LiteralConstant {
                            stage: RuntimeShaderStage::Pixel,
                            destination: *destination,
                            words: *words,
                        }
                    }
                    crate::OwnedShaderArgument::Unknown { argument_type, raw } => {
                        RuntimeArgumentBinding::Unknown {
                            argument_type: *argument_type,
                            raw: *raw,
                        }
                    }
                })
                .collect();
            let mut runtime_pass = RuntimePass {
                shader_pair,
                custom_sampler_flags: pass.custom_sampler_flags,
                t5_custom_sampler_flags: pass.t5_custom_sampler_flags,
                per_prim_arg_count: pass.per_prim_arg_count,
                per_obj_arg_count: pass.per_obj_arg_count,
                stable_arg_count: pass.stable_arg_count,
                arguments,
                color_space: compiler.color_space(slot),
                hardware_shadow_compare: compiler.hardware_shadow_compare(),
                depth_to_colour: compiler.depth_to_colour(slot),
            };

            sort_pass_args(&mut runtime_pass);
            runtime_pass
        })
        .collect();
    RuntimeTechnique {
        source_selection: technique.source_selection,
        flags: technique.flags,
        passes,
    }
}
