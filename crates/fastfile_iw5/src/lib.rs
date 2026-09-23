#![no_std]
#![forbid(unsafe_code)]

mod asset_type;
mod attachment;
mod content;
mod envelope;
mod load;
pub mod size;
mod weapon_override;
mod wire;
mod zone;

pub use asset_type::AssetType;
pub use attachment::{
    AttachmentAddOns, AttachmentAdsSettings, AttachmentAimAssist, AttachmentAmmoGeneral,
    AttachmentAmmunition, AttachmentDamage, AttachmentFacts, AttachmentGeneral, AttachmentGunKick,
    AttachmentHipSpread, AttachmentIdleSettings, AttachmentReload, AttachmentScales,
    AttachmentSight,
};
pub use content::{
    AssetSink, AssetTable, ScriptStrings, XASSET_ENTRY_LEN, XASSET_LIST_LEN, load_zone,
    open_asset_table,
};
pub use envelope::{
    FILE_PREAMBLE_LEN, FileHeader, FileHeaderError, MAGIC_AUTH_HEADER, MAGIC_SIGNED,
    MAGIC_UNSIGNED, Signing, ZONE_VERSION_PC, parse_file_header,
};
pub use load::{AssetLinkSink, load_asset_at, load_asset_at_observed, load_asset_body};
pub use weapon_override::{
    AnimOverride, FxOverride, NoteTrackOverride, ReloadOverride, ScriptStringMap, SoundOverride,
};
pub use wire::{Iw5WireFormat, WireAssetTable, WirePointer, WireTableError, read_wire_asset_table};
pub use zone::{
    AttachmentGeometry, BLOCK_STACK_CAP, BlockType, ClipMapGeometry, ComWorldGeometry,
    FxEffectDefGeometry, GfxImageGeometry, GfxLightDefGeometry, GfxLightGridGeometry,
    GfxLightmapPair, GfxWorldGeometry, MAX_LIGHT_DEFS, MAX_LIGHTMAP_PAGES, MAX_XFILE_COUNT,
    MapEntsGeometry, MaterialGeometry, PTR_SIZE, Ptr, Result, ShaderGeometry,
    TECHNIQUE_ARGUMENT_CAP, TECHNIQUE_PASS_ROW_CAP, TechniqueArgumentGeometry,
    TechniqueGraphGeometry, TechniquePassGeometry, TechniqueSetGeometry, VertexDeclGeometry,
    WeaponGeometry, XAnimPartsGeometry, XFILE_BLOCK_CALLBACK, XFILE_BLOCK_INDEX, XFILE_BLOCK_LARGE,
    XFILE_BLOCK_PHYSICAL, XFILE_BLOCK_RUNTIME, XFILE_BLOCK_SCRIPT, XFILE_BLOCK_TEMP,
    XFILE_BLOCK_VERTEX, XFILE_BLOCK_VIRTUAL, XFILE_HEADER_LEN, XModelGeometry, ZoneError,
    ZoneHeader, ZonePtr, ZoneStream, block_is_aliasable, parse_zone_header,
};
