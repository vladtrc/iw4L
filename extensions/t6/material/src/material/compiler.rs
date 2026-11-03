use asset_material::material_compile::{
    compiler::{MaterialCompiler, MaterialDrawInputs},
    state,
};
use render_material::PassColorSpace;

pub struct T6Compiler;

impl MaterialCompiler for T6Compiler {
    fn yields_sort_capacity(&self, _: &str, _: bool) -> bool {
        true
    }
    fn zone_preview_names(&self, map: &str) -> Option<Vec<String>> {
        Some(vec![
            format!("loadscreen_{map}"),
            format!("menu_{map}_map_select_final"),
        ])
    }

    fn draw_rules(
        &self,
        material: &MaterialDrawInputs,
        unlit: bool,
    ) -> render_material::MaterialDrawRules {
        render_material::MaterialDrawRules {
            colour_camera_region: material.camera_region,
            smodel_colour_emits: true,
            unlit_sky: unlit
                && matches!(
                    material.sort_key,
                    asset_iw4::SORT_KEY_SKY | asset_iw4::SORT_KEY_SKYBOX
                ),
            postfx_host_supported: false,
        }
    }
    fn compile_state(&self, words: [u32; 2]) -> render_material::CompiledPassState {
        state::packed_state(words, None)
    }
    fn color_space(&self, _slot: u8) -> PassColorSpace {
        PassColorSpace::Unknown
    }
    fn hardware_shadow_compare(&self) -> bool {
        false
    }
    fn depth_to_colour(&self, slot: u8) -> bool {
        slot == 3
    }
}
