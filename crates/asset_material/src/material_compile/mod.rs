mod sort;
pub mod state;
pub use state::compile_material_state;
pub mod compiler;
mod technique;

use render_material::{
    MaterialAssetId, RemapResolution, RuntimeImageId, RuntimeMaterial, RuntimeMaterialBuild,
    RuntimeMaterialCatalog, RuntimeShaderProgram, RuntimeShaderProgramId, RuntimeShaderStage,
    RuntimeSortedMaterialTable, RuntimeTechniqueSet, RuntimeTechniqueSetId, RuntimeTextureBinding,
    RuntimeVertexDecl, TECHNIQUE_SLOT_COUNT,
};
use sort::build_sorted_material_table;
use std::collections::BTreeMap;

fn program_hash(program: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in program {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn compile_material_catalog(
    source: &crate::MaterialDefinitions,
) -> Result<RuntimeMaterialCatalog, compiler::MaterialCompileError> {
    for family in source
        .materials
        .iter()
        .map(|material| material.namespace)
        .chain(
            source
                .technique_set_facts()
                .iter()
                .map(|facts| facts.namespace),
        )
    {
        compiler::compiler_for(family)
            .ok_or(compiler::MaterialCompileError::UnknownFamily(family))?;
    }
    for material in &source.materials {
        if let Some(entries) = &material.native_state_bits_entry {
            let expected = compiler::compiler_for(material.namespace)
                .expect("validated material compiler")
                .selector_count();
            if expected != Some(entries.len()) {
                return Err(compiler::MaterialCompileError::InvalidSelectorCount {
                    family: material.namespace,
                    expected,
                    actual: entries.len(),
                });
            }
        }
    }
    let mut technique_sets = Vec::new();
    let mut vertex_decls = Vec::<(u32, RuntimeVertexDecl)>::new();
    for facts in source.technique_set_facts() {
        let compiler =
            compiler::compiler_for(facts.namespace).expect("validated material compiler");
        let mut slots = vec![None; TECHNIQUE_SLOT_COUNT];
        if let Some(graph) = facts.table.as_ref().and_then(|table| table.graph.as_ref()) {
            for (slot_index, technique) in graph.slots.iter().enumerate().take(TECHNIQUE_SLOT_COUNT)
            {
                let Some(technique) = technique else {
                    continue;
                };
                slots[slot_index] = Some(technique::compile_technique(
                    compiler,
                    source,
                    slot_index as u8,
                    technique,
                    &mut vertex_decls,
                ));
            }
        }
        technique_sets.push(
            RuntimeTechniqueSet::new(slots)
                .expect("capture technique set is TECHNIQUE_SLOT_COUNT slots")
                .with_identity(
                    facts.world_vert_format,
                    facts.namespace,
                    facts.name.to_string(),
                ),
        );
    }

    let mut sorted_first_skip = None;
    let mut skip_tech = None;
    let mut skip_cause = None;
    let sorted_materials = match build_sorted_material_table(source) {
        Ok((
            asset_ids_by_ordinal,
            ordinals_by_asset_id,
            skipped_n,
            slot_gap_n,
            first,
            tech,
            cause,
        )) => {
            sorted_first_skip = first;
            skip_tech = tech;
            skip_cause = cause;
            RuntimeSortedMaterialTable::Ready {
                asset_ids_by_ordinal,
                ordinals_by_asset_id,
                skipped_n,
                slot_gap_n,
            }
        }
        Err(cause) => RuntimeSortedMaterialTable::BuildFailed(cause),
    };

    let materials: Vec<RuntimeMaterial> = source
        .materials
        .iter()
        .enumerate()
        .map(|(material_index, material)| {
            let key = crate::TechsetKey::new(material.namespace, material.technique_set.as_str());
            let local_index = match source.resolve_technique_set(key) {
                crate::TechsetResolve::Hit { index, .. } => Some(index),
                crate::TechsetResolve::GraphMissing { .. }
                | crate::TechsetResolve::Foreign { .. }
                | crate::TechsetResolve::Missing => None,
            };
            let local_technique_set = RuntimeTechniqueSetId(
                local_index
                    .and_then(|index| u32::try_from(index).ok())
                    .unwrap_or(u32::MAX),
            );
            let asset_id = MaterialAssetId(
                u16::try_from(material_index)
                    .expect("material catalog index exceeds the u16 asset-id domain"),
            );
            let baked_draw_surf =
                sorted_materials
                    .ordinal_for_asset_id(material_index)
                    .map(|ordinal| {
                        let table = material.technique_table.as_ref();
                        dpvs_iw4::bake_material_draw_surf_key(dpvs_iw4::MaterialDrawSurfBakeInput {
                            sort_key: material.sort_key,
                            info_game_flags: material.info_game_flags,
                            material_sorted_index: ordinal.sort_band(),
                            technique0_absent: table.is_none_or(|table| table.slots & 1 == 0),
                            technique1_present: table.is_some_and(|table| table.slots & 2 != 0),
                            material_byte_4b: material.state_flags,
                            technique0_flags: table.map_or(0, |table| table.technique0_flags),
                        })
                        .packed
                    });
            let [uv_anim, falloff_parms, falloff_begin, falloff_end] =
                crate::MaterialDefinitions::material_animation(material);
            RuntimeMaterial {
                asset_id,
                name: material.name.to_string(),
                namespace: material.namespace,
                technique_set: material.technique_set.to_string(),
                baked_draw_surf,
                local_technique_set,
                remap: RemapResolution::SelfSet,
                state_bits_entry: material.state_bits_entry,
                pass_states: material
                    .state_bits
                    .iter()
                    .map(|words| {
                        compiler::compiler_for(material.namespace)
                            .expect("validated material compiler")
                            .compile_state(*words)
                    })
                    .collect(),
                camera_region: material.camera_region,
                draw_rules: compiler::compiler_for(material.namespace)
                    .expect("validated material compiler")
                    .draw_rules(
                        &compiler::MaterialDrawInputs {
                            camera_region: material.camera_region,
                            info_game_flags: material.info_game_flags,
                            sort_key: material.sort_key,
                        },
                        material.is_unlit().unwrap_or(false),
                    ),
                sort_key: material.sort_key,
                info_game_flags: material.info_game_flags,
                texture_atlas: material.texture_atlas.unwrap_or([1, 1]),
                state_flags: material.state_flags,
                surface_type_bits: material.surface_type_bits,
                unlit: material.is_unlit().unwrap_or(false),
                takes_model_lighting: material.takes_model_lighting() == Some(true),
                uses_model_lighting_const: material
                    .route
                    .is_some_and(|route| route.uses_model_lighting_const),
                square_color_map: material.color_map_transform()
                    == crate::ColorMapTransform::Square,
                shadow_only: material.is_shadowcaster(),
                cull_mode: match material.cull_face() {
                    Some(crate::MaterialCullFace::Back) => Some(0),
                    Some(crate::MaterialCullFace::Front) => Some(1),
                    Some(crate::MaterialCullFace::None) | None => None,
                },
                uv_anim_bits: uv_anim.map(f32::to_bits),
                falloff_parms_bits: falloff_parms.map(f32::to_bits),
                falloff_begin_bits: falloff_begin.map(f32::to_bits),
                falloff_end_bits: falloff_end.map(f32::to_bits),
                env_map_parms_bits: material
                    .constants
                    .iter()
                    .find(|constant| constant.name.starts_with(b"envMapParms"))
                    .map(|constant| constant.literal.map(f32::to_bits))
                    .unwrap_or([0; 4]),
                textures: {
                    let mut textures = material
                        .textures
                        .iter()
                        .map(|texture| {
                            (
                                texture.name_hash,
                                texture.image.and_then(|index| {
                                    u32::try_from(index)
                                        .ok()
                                        .map(|image| RuntimeTextureBinding {
                                            image: RuntimeImageId(image),
                                            sampler_state: texture.sampler_state,
                                            semantic: texture.semantic,
                                        })
                                }),
                            )
                        })
                        .collect::<Vec<_>>();
                    textures.sort_by_key(|(hash, _)| *hash);
                    textures
                },
                constants: {
                    let mut constants = material
                        .constants
                        .iter()
                        .map(|constant| (constant.name_hash, constant.literal.map(f32::to_bits)))
                        .collect::<Vec<_>>();
                    constants.sort_by_key(|(hash, _)| *hash);
                    constants
                },
            }
        })
        .collect();
    let mut material_indices_by_name = BTreeMap::new();
    for (index, material) in materials.iter().enumerate() {
        material_indices_by_name
            .entry(material.name.clone())
            .or_insert(index);
    }
    let material_indices_by_name = material_indices_by_name.into_iter().collect();
    let material_indices_by_key = materials
        .iter()
        .enumerate()
        .map(|(index, material)| {
            (
                asset_core::MaterialKey {
                    namespace: material.namespace,
                    name: material.name.clone(),
                },
                index,
            )
        })
        .collect();

    let shader_programs = source
        .shaders
        .iter()
        .enumerate()
        .map(|(asset_slot, shader)| {
            let stage = if shader.is_vertex() {
                RuntimeShaderStage::Vertex
            } else if shader.is_pixel() {
                RuntimeShaderStage::Pixel
            } else {
                panic!(
                    "material shader corpus contains non-shader asset type {:?} at slot {asset_slot}",
                    shader.kind
                );
            };
            let asset_slot = u32::try_from(asset_slot)
                .expect("shader catalog index exceeds the runtime u32 asset-slot domain");

            if shader.program.is_empty() {
                return None;
            }
            Some(RuntimeShaderProgram {
                id: RuntimeShaderProgramId {
                    asset_slot,
                    program_hash: program_hash(&shader.program),
                },
                stage,
                name: shader.name.to_string(),
                program: shader.program.clone(),
            })
        })
        .collect();

    Ok(RuntimeMaterialBuild {
        materials,
        material_indices_by_name,
        material_indices_by_key,
        technique_sets,
        shader_programs,
        vertex_decls,
        sorted_materials,
        native_diagnostics: {
            let families: std::collections::BTreeSet<_> = source
                .materials
                .iter()
                .map(|material| material.namespace)
                .chain(
                    source
                        .technique_set_facts()
                        .iter()
                        .map(|facts| facts.namespace),
                )
                .chain(source.argument_gaps.keys().copied())
                .collect();
            families
                .into_iter()
                .map(|family| {
                    let entries: Vec<_> = source
                        .materials
                        .iter()
                        .filter(|material| material.namespace == family)
                        .filter_map(|material| material.native_state_bits_entry.as_deref())
                        .collect();
                    (
                        family,
                        render_material::NativeMaterialDiagnostics {
                            selector_census: compiler::compiler_for(family)
                                .and_then(|compiler| compiler.selector_census(&entries)),
                            fallback_count: source
                                .technique_set_facts()
                                .iter()
                                .filter(|facts| {
                                    facts.namespace == family && facts.fallback_table.is_some()
                                })
                                .count() as u32,
                            argument_gaps: source
                                .argument_gaps
                                .get(&family)
                                .cloned()
                                .unwrap_or_else(BTreeMap::new),
                        },
                    )
                })
                .collect()
        },
        sorted_first_skip,
        skip_tech,
        skip_cause,
        leftover_unknown_n: remaining_unknown_arg_n(source),
    }
    .publish())
}

fn remaining_unknown_arg_n(source: &crate::MaterialDefinitions) -> u32 {
    let mut n = 0u32;
    for facts in source.technique_set_facts() {
        for table in facts.table.iter().chain(facts.fallback_table.iter()) {
            let Some(graph) = &table.graph else {
                continue;
            };
            for technique in graph.slots.iter().flatten() {
                for pass in &technique.passes {
                    for argument in &pass.arguments {
                        if matches!(argument, crate::OwnedShaderArgument::Unknown { .. }) {
                            n = n.saturating_add(1);
                        }
                    }
                }
            }
        }
    }
    n
}
