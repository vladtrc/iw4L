#![no_std]
#![forbid(unsafe_code)]

mod asset_type;
mod content;
mod envelope;
mod load;
pub mod shader;
mod stream;
mod wire;
mod zone;

pub use wire::{Iw4WireFormat, WireAssetTable, WirePointer, WireTableError, read_wire_asset_table};

pub use asset_type::AssetType;
pub use content::{
    AssetSink, AssetTable, ScriptStrings, XASSET_ENTRY_LEN, XASSET_LIST_LEN, load_zone,
    open_asset_table,
};
pub use envelope::{
    FILE_PREAMBLE_LEN, FileHeader, FileHeaderError, MAGIC_AUTH_HEADER, MAGIC_SIGNED,
    MAGIC_UNSIGNED, Signing, ZONE_VERSION_PC, parse_file_header,
};
pub use load::{
    AssetLinkSink, FontCapture, GlyphCapture, MenuDefCapture, MenuItemLayout, MenuRectCapture,
    MenuScriptKind, load_asset_at, load_asset_at_observed, load_asset_at_with, load_asset_body,
};
pub use stream::{
    alloc_stream_pos, convert_offset_to_alias, convert_offset_to_pointer, inc_stream_pos,
    insert_pointer, load_stream, pop_stream_pos, push_stream_pos,
};
pub use zone::{
    BLOCK_STACK_CAP, BlockType, ClipMapGeometry, ComWorldGeometry, FxEffectDefGeometry,
    FxImpactTableGeometry, FxWorldGeometry, GGlassDataGeometry, GfxImageGeometry,
    GfxLightDefGeometry, GfxLightGridGeometry, GfxLightmapPair, GfxSunEffectsGeometry,
    GfxWorldGeometry, MAX_LIGHTMAP_PAGES, MAX_XFILE_COUNT, MapEntsGeometry, MaterialGeometry,
    PTR_SIZE, PhysPresetGeometry, Ptr, Result, ShaderGeometry, TECHNIQUE_ARGUMENT_CAP,
    TECHNIQUE_PASS_ROW_CAP, TechniqueArgumentGeometry, TechniqueGraphGeometry,
    TechniquePassGeometry, TechniqueSetGeometry, TracerDefGeometry, VertexDeclGeometry,
    WeaponGeometry, WeaponIdleCapture, WeaponKickCapture, WeaponMovementOfsCapture,
    WeaponSwayCapture, XAnimDeltaTransGeometry, XAnimPartsGeometry, XFILE_BLOCK_CALLBACK,
    XFILE_BLOCK_INDEX, XFILE_BLOCK_LARGE, XFILE_BLOCK_PHYSICAL, XFILE_BLOCK_RUNTIME,
    XFILE_BLOCK_TEMP, XFILE_BLOCK_VERTEX, XFILE_BLOCK_VIRTUAL, XFILE_HEADER_LEN, XModelGeometry,
    ZoneError, ZoneHeader, ZonePtr, ZoneStream, block_is_aliasable, parse_zone_header,
};
