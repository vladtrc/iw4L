use crate::{PlayerAnimGrammar, REFERENCE_ANIM_GRAMMAR};
use asset_core::FamilyId;
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
pub struct AnimationFamily {
    pub grammar: &'static PlayerAnimGrammar,
    pub select_referenced_leaves: bool,
    pub use_type_aliases: bool,
    pub decode_source: fn(&str, &[u8], bool) -> Result<Vec<u8>, String>,
}
pub fn decode_standard_source(
    _: &str,
    data: &[u8],
    zlib_compressed: bool,
) -> Result<Vec<u8>, String> {
    if zlib_compressed {
        asset_transport::inflate_zlib(data)
    } else {
        Ok(data.to_vec())
    }
}
pub const REFERENCE_ANIMATION_FAMILY: AnimationFamily = AnimationFamily {
    grammar: &REFERENCE_ANIM_GRAMMAR,
    select_referenced_leaves: false,
    use_type_aliases: false,
    decode_source: decode_standard_source,
};
#[derive(Default)]
pub struct AnimationFamilies(BTreeMap<FamilyId, AnimationFamily>);
impl AnimationFamilies {
    pub fn register(&mut self, id: FamilyId, family: AnimationFamily) {
        assert!(!self.0.contains_key(&id), "duplicate animation family");
        self.0.insert(id, family);
    }
}
static FAMILIES: OnceLock<AnimationFamilies> = OnceLock::new();
pub fn install_animation_families(families: AnimationFamilies) -> Result<(), &'static str> {
    FAMILIES
        .set(families)
        .map_err(|_| "animation families already installed")
}
pub fn animation_family(id: FamilyId) -> Option<&'static AnimationFamily> {
    FAMILIES.get()?.0.get(&id)
}
