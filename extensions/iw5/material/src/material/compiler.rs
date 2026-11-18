use asset_material::material_compile::{
    compiler::{MaterialCompiler, MaterialDrawInputs},
    state,
};
use render_material::PassColorSpace;

pub struct Iw5Compiler;

impl MaterialCompiler for Iw5Compiler {
    fn yields_sort_capacity(&self, zone: &str, has_native_selectors: bool) -> bool {
        has_native_selectors && zone == "common_mp"
    }
    fn selector_count(&self) -> Option<usize> {
        Some(super::iw5_tech_map::IW5_TECHNIQUE_TYPE_COUNT)
    }
    fn selector_census(&self, entries: &[&[u8]]) -> Option<String> {
        super::iw5_tech_map::leftover_selector_census(
            entries.iter().filter_map(|entry| (*entry).try_into().ok()),
        )
    }

    fn draw_rules(
        &self,
        material: &MaterialDrawInputs,
        _unlit: bool,
    ) -> render_material::MaterialDrawRules {
        render_material::MaterialDrawRules {
            colour_camera_region: material.camera_region,
            smodel_colour_emits: true,
            unlit_sky: false,
            postfx_host_supported: false,
        }
    }
    fn compile_state(&self, words: [u32; 2]) -> render_material::CompiledPassState {
        state::iw_state(words)
    }
    fn color_space(&self, _slot: u8) -> PassColorSpace {
        PassColorSpace::Linear
    }
    fn hardware_shadow_compare(&self) -> bool {
        false
    }
}
