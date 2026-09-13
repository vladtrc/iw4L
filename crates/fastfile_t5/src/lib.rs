#![no_std]
#![forbid(unsafe_code)]

mod asset_type;
mod content;
mod envelope;
pub mod light_grid;
mod load;
pub mod size;
pub mod state_bits;
pub mod vertex_decl;
pub mod xmodel_lod;
mod zone;

pub use asset_type::AssetType;
pub use content::{
    AssetSink, AssetTable, ScriptStrings, XASSET_ENTRY_LEN, XASSET_LIST_LEN, load_zone,
    open_asset_table,
};
pub use envelope::{
    FILE_PREAMBLE_LEN, FileHeader, FileHeaderError, MAGIC_SIGNED, MAGIC_UNSIGNED, Signing,
    ZONE_VERSION_PC, parse_file_header,
};
pub use load::{
    AssetLinkSink, NestedShaderKind, load_asset_at, load_asset_at_observed, load_asset_body,
};
pub use size::{
    TECHNIQUE_OCCUPANCY_WORDS, TECHNIQUE_SLOT_COUNT, occupancy_any, occupancy_bit, occupancy_set,
    occupancy_test,
};
pub use zone::{
    ALIGN_WASTE_SITE_COUNT, AlignWasteSite, AlignWasteStats, BLOCK_STACK_CAP, BlockType,
    ClipMapGeometry, ComWorldGeometry, FxEffectDefGeometry, GfxImageGeometry, GfxLightGridGeometry,
    GfxLightmapImages, GfxWorldGeometry, MAX_LIGHTMAP_PAGES, MAX_XFILE_COUNT, MapEntsGeometry,
    MaterialGeometry, PTR_SIZE, Ptr, Result, ShaderGeometry, TECHNIQUE_ARGUMENT_CAP,
    TECHNIQUE_PASS_ROW_CAP, TechniqueArgumentGeometry, TechniqueGraphGeometry,
    TechniquePassGeometry, TechniqueSetGeometry, VertexDeclGeometry, WeaponGeometry,
    XAnimPartsGeometry, XFILE_BLOCK_LARGE, XFILE_BLOCK_LARGE_RUNTIME, XFILE_BLOCK_PHYSICAL,
    XFILE_BLOCK_PHYSICAL_RUNTIME, XFILE_BLOCK_RUNTIME, XFILE_BLOCK_TEMP, XFILE_BLOCK_VIRTUAL,
    XFILE_HEADER_LEN, XModelGeometry, ZoneError, ZoneHeader, ZonePtr, ZoneStream,
    block_is_aliasable, parse_zone_header,
};
