use asset_core::FamilyId;
use render_material::PassColorSpace;

#[derive(Clone, Copy, Debug)]
pub struct MaterialDrawInputs {
    pub camera_region: u8,
    pub info_game_flags: u8,
    pub sort_key: u8,
}

pub trait MaterialCompiler: Send + Sync {
    fn alternate_techset_name(&self, name: &str) -> String {
        name.to_owned()
    }
    fn yields_sort_capacity(&self, _zone: &str, _has_native_selectors: bool) -> bool {
        false
    }
    fn selector_count(&self) -> Option<usize> {
        None
    }
    fn selector_census(&self, _entries: &[&[u8]]) -> Option<String> {
        None
    }
    fn zone_preview_names(&self, _map: &str) -> Option<Vec<String>> {
        None
    }

    fn draw_rules(
        &self,
        material: &MaterialDrawInputs,
        unlit: bool,
    ) -> render_material::MaterialDrawRules;
    fn compile_state(&self, words: [u32; 2]) -> render_material::CompiledPassState;
    fn color_space(&self, slot: u8) -> PassColorSpace;
    fn hardware_shadow_compare(&self) -> bool;
    fn depth_to_colour(&self, _slot: u8) -> bool {
        false
    }
}

#[path = "compiler/iw4.rs"]
pub mod iw4;

#[derive(Default)]
pub struct MaterialCompilers(std::collections::BTreeMap<FamilyId, &'static dyn MaterialCompiler>);
impl MaterialCompilers {
    pub fn register(&mut self, id: FamilyId, compiler: &'static dyn MaterialCompiler) {
        assert!(!self.0.contains_key(&id), "duplicate material compiler");
        self.0.insert(id, compiler);
    }
}
static COMPILERS: std::sync::OnceLock<MaterialCompilers> = std::sync::OnceLock::new();
pub fn install_material_compilers(compilers: MaterialCompilers) -> Result<(), &'static str> {
    COMPILERS
        .set(compilers)
        .map_err(|_| "material compilers already installed")
}
pub fn compiler_for(namespace: FamilyId) -> Option<&'static dyn MaterialCompiler> {
    COMPILERS.get()?.0.get(&namespace).copied()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialCompileError {
    UnknownFamily(FamilyId),
    InvalidSelectorCount {
        family: FamilyId,
        expected: Option<usize>,
        actual: usize,
    },
}
impl std::fmt::Display for MaterialCompileError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownFamily(family) => write!(
                output,
                "unregistered material compiler for {}",
                family.as_str()
            ),
            Self::InvalidSelectorCount {
                family,
                expected,
                actual,
            } => write!(
                output,
                "invalid native material selector count for {}: expected {expected:?}, got {actual}",
                family.as_str()
            ),
        }
    }
}
impl std::error::Error for MaterialCompileError {}
