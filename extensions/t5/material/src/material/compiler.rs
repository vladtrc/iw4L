use asset_material::material_compile::{
    compiler::{MaterialCompiler, MaterialDrawInputs},
    state,
};
use render_material::PassColorSpace;

pub struct T5Compiler;

impl MaterialCompiler for T5Compiler {
    fn alternate_techset_name(&self, name: &str) -> String {
        name.replace("x0", "").replace("x1", "")
    }
    fn yields_sort_capacity(&self, zone: &str, has_native_selectors: bool) -> bool {
        has_native_selectors && zone == "common_mp"
    }
    fn selector_count(&self) -> Option<usize> {
        Some(super::t5_tech_map::T5_TECHNIQUE_TYPE_COUNT)
    }
    fn selector_census(&self, entries: &[&[u8]]) -> Option<String> {
        super::t5_tech_map::leftover_t5_selector_census(
            entries.iter().filter_map(|entry| (*entry).try_into().ok()),
        )
    }

    fn draw_rules(
        &self,
        material: &MaterialDrawInputs,
        _unlit: bool,
    ) -> render_material::MaterialDrawRules {
        render_material::MaterialDrawRules {
            colour_camera_region: if material.camera_region == 3 {
                asset_iw4::CAMERA_REGION_NONE
            } else {
                material.camera_region
            },
            smodel_colour_emits: ext_t5_fastfile::state_bits::smodel_camera_emits(
                material.info_game_flags,
                material.camera_region,
            ),
            unlit_sky: false,
            postfx_host_supported: false,
        }
    }
    fn compile_state(&self, words: [u32; 2]) -> render_material::CompiledPassState {
        state::packed_state(
            words,
            ext_t5_fastfile::state_bits::alpha_test(words[0])
                .map(|(func, reference)| d3d9_state::AlphaTest::from_raw(func, reference)),
        )
    }
    fn color_space(&self, slot: u8) -> PassColorSpace {
        if lighting_iw4::is_lit_remap_slot(slot) {
            PassColorSpace::GammaEncoded
        } else {
            PassColorSpace::Unknown
        }
    }
    fn hardware_shadow_compare(&self) -> bool {
        true
    }
    fn depth_to_colour(&self, slot: u8) -> bool {
        slot == 3
    }
}
