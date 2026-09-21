mod animtree {
    pub use asset_anim::*;
}
mod arena {
    pub use asset_game::arena::*;
}
mod artifact_cache;
mod asset_graph;
mod attachment_hide {
    pub use asset_game::*;
}
mod body_catalog {
    pub use asset_model::*;
}
mod cac_stats {
    pub use asset_game::*;
}
mod clip_collision {
    pub use asset_world::*;
}
mod clip_scheduler {
    pub use asset_anim::*;
}
mod compass_map {
    pub use asset_world::*;
}
mod createart {
    pub use asset_world::*;
}
mod createfx {
    pub use asset_audio::*;
}
mod discover;
mod ent_channel {
    pub use asset_audio::*;
}
mod map_script_sound {
    pub use asset_audio::*;
}
mod playeranim_parse {
    pub use asset_anim::*;
}
pub mod dobj {
    pub use xmodel_runtime::*;
}
mod dyn_ents {
    pub use asset_world::*;
}
mod fpv_catalog {
    pub use asset_model::*;
}
mod fx_catalog {
    pub use asset_game::*;
}
mod fx_model_catalog {
    pub use asset_game::*;
}
mod glass_catalog {
    pub use asset_world::*;
}
mod gltf_export;
mod impact_fx_catalog {
    pub use asset_game::*;
}
mod iw5_tech_map {
    pub use asset_material::iw5_tech_map::*;
}
mod iwd;
mod lane;
mod lane_capability;
pub mod loading_screen;
mod localize {
    pub use asset_game::*;
}
mod map_entities {
    pub use asset_world::*;
}
mod match_load;
mod material_catalog {
    pub use asset_material::material_catalog::*;
}
mod material_draw {
    pub use asset_material::material_draw::*;
}
mod material_images {
    pub use asset_material::material_images::*;
}
mod menu_catalog {
    pub use asset_game::*;
}
pub mod model_lighting {
    pub use asset_model::*;
}
mod model_mesh {
    pub use asset_world::model_mesh::*;
}
mod model_skel {
    pub use asset_model::*;
}
mod penetration {
    pub use asset_game::*;
}
pub mod plugin;
pub mod prepared;
pub mod load_jobs {
    pub use asset_transport::load_jobs::*;
}
pub mod progress {
    pub use asset_transport::progress::*;
}
mod projectile_mesh_catalog {
    pub use asset_model::*;
}
mod map_load_process;
pub mod session_load;
mod soldiers {
    pub use asset_model::*;
}
mod sound_catalog {
    pub use asset_audio::*;
}
mod sound_load {
    pub use asset_audio::*;
}
mod sound_load_iw5 {
    pub use asset_audio::*;
}
mod sound_load_t5 {
    pub use asset_audio::*;
}
mod sound_wma_t5 {
    pub use asset_audio::*;
}
mod t5_code_remap {
    pub use asset_material::t5_code_remap::*;
}
pub mod t5_tech_map {
    pub use asset_material::t5_tech_map::*;
}
mod teardown;
mod tracer_catalog {
    pub use asset_game::*;
}
mod weapon_anim_dispatch {
    pub use asset_game::*;
}
mod weapon_animations {
    pub use asset_game::*;
}
mod weapon_catalog {
    pub use asset_game::*;
}
mod world_draw {
    pub use asset_world::world_draw::*;
}
mod world_iw5 {
    pub use asset_world::world_iw5::*;
}
mod world_mesh {
    pub use asset_world::world_mesh::*;
}
mod world_t5 {
    pub use asset_world::world_t5::*;
}
mod world_weapon_catalog {
    pub use asset_model::*;
}
mod xanim_catalog {
    pub use asset_anim::*;
}
mod xanim_clip {
    pub use xmodel_runtime::*;
}
mod zone;

pub use anim_iw4::PartBits;
pub use animtree::{
    AnimTreeDefinitionError, AnimTreeNodeRuntime, AtrCompileError, CompiledAnimNode,
    CompiledAnimTreeDefinition, DObjAnimTreeRuntime, MULTIPLAYER_ANIMTREE_PATH,
    PLAYERANIM_SCRIPT_PATH, PLAYERANIM_TYPES_PATH, PlayerAnimLeafBinds, PlayerAnimSources,
};
pub use arena::{
    ArenaCharsets, FACTION_ICON_COL, MapTeamSettings, SessionTeamSettings, arena_charsets,
    load_iw5_team_sources, parse_arena, read_basemaps_arena, read_iwd_named,
    t5_settings_from_teamset_gsc, t5_settings_from_teamset_rawfile, t5_teamset_from_map_gsc,
    t5_teamset_from_rawfile, t5_teamset_key_from_rawfile, team_settings, team_settings_for_zone,
};
pub use artifact_cache::{cache_flight, cache_get, cache_put, fnv1a64, fnv1a64_more};
pub use asset_core::{
    AssetKey, AssetKeyError, AssetKind, AssetNamespace, AssetRef, AssetRefCensus, BoundTarget,
    CatalogIndex, IndexSpace, WalkLocalMaterialIndex, ZoneGame, ZoneOwner, bound_zone_names,
};
pub(crate) use asset_graph::stamp_match_destructible_death;
pub use asset_graph::{
    AssetEdge, AssetEdgeCensus, AssetEdgeReason, AssetGraphCensus, DESTRUCTIBLE_DEATH_HINTS,
    DeathClipEdge, DeathHuskEdge, DestructibleDeathHint, DestructibleDeathRow, FpvMeshIndex,
    FpvMeshSpace, FxIndex, FxModelIndex, FxModelSpace, FxSpace, LoadedSoundIndex, LoadedSoundSpace,
    MapXModelIndex, MapXModelSpace, MaterialIndex, MaterialSpace, ProjectileModelIndex,
    ProjectileModelSpace, SoundAliasIndex, SoundAliasSpace, TechniqueSetIndex, TechniqueSetSpace,
    TracerIndex, TracerSpace, WorldWeaponIndex, WorldWeaponSpace, XAnimIndex, XAnimSpace,
    resolve_after_absorb,
};
pub use asset_iw4::{
    D3DCMP_ALWAYS, D3DCMP_EQUAL, D3DCMP_LESS, D3DCMP_LESSEQUAL, GFXS1_DEPTHTEST_DISABLE,
    GFXS1_DEPTHTEST_LESSEQUAL, GFXS1_DEPTHWRITE, GFXS1_POLYGON_OFFSET_MASK,
    GFXS1_POLYGON_OFFSET_SHADOWMAP_LEVEL, GFXS1_POLYGON_OFFSET_SHIFT, POLYGON_OFFSET_BIAS_TO_D3D,
    R_POLYGON_OFFSET_BIAS_DEFAULT, R_POLYGON_OFFSET_SCALE_DEFAULT, S_DEPTH_TEST_TABLE,
    SM_POLYGON_OFFSET_BIAS_DEFAULT, SM_POLYGON_OFFSET_SCALE_DEFAULT, SND_CURVE_DEFAULT_ASSET_NAME,
    SND_ENTCHANNEL_FILE, SndAliasFlags, SndAliasSampleKind, d3d_depth_bias_to_wgpu_constant,
    depth_state_from_state_bits, depth_test_enable, depth_write_enable, polygon_offset_d3d,
    polygon_offset_d3d_defaults, polygon_offset_level, polygon_offset_wgpu_defaults,
    size::{WEAP_ANIM_IDLE, WEAPON_ANIM_COUNT, weap_anim},
    snd_attenuate,
};
pub use asset_world::{
    FilmVision, FilmVisionParseError, MaterialSortTrigger, SurfaceCastsSunShadow, WorldCapture,
    parse_film_vision_rawfile, world_capture_from_casters,
};
pub use attachment_hide::{
    AttachmentVisual, attachment_catalog, bare_weapon_id, bone_has_hidden_ancestor,
    effective_hide_tags, resolve_weapon_for_attachments, surface_visible,
};
pub use body_catalog::{BODY_SPINE_BONES, BodyMeshBuild, BodyMeshCatalog, BodyMeshEntry};
pub use cac_stats::{
    CacAuthoredCategory, CacPerkRow, CacPerkSlot, CacStatBar, CacWeaponFact, CacWeaponPreview,
    PERK_ICON_COL, PERK_NAME_COL, PERK_REF_COL, PERK_SLOT_COL, STATS_GROUP_COL, STATS_IMAGE_COL,
    STATS_NAME_COL, STATS_REF_COL, cac_category_from_item_group, is_perk_table_name,
    is_stats_table_name, item_group_for_weapon, iw4_fallback_item_group, perk_row, perk_rows,
    primary_sniper_keys, weapon_preview,
};
pub use clip_collision::{
    ClipBrush, ClipCollision, ClipCollisionError, ClipMapMaterial, ClipPlacedStaticModel,
    ClipSweepHit, MASK_PLAYER_SOLID, XModelCollCatalog, attach_iw5_static_models,
    attach_static_models, build_clip_collision, build_iw5_clip_collision, build_t5_clip_collision,
    gate_leafbrush_index_fits,
};
pub use clip_scheduler::{ActiveAnim, ClipScheduler, ClipSchedulerError};
pub use compass_map::{
    MapCompassDeclaration, MapCompassSource, compass_max_range, setup_minimap_material,
};
pub use createart::{
    ExpFog, SunFog, decode_rawfile_text, is_createart_fog_file, is_createart_source,
    parse_createart_rawfile, parse_set_exp_fog, parse_set_vol_fog, parse_vision_set_fog,
};
pub use createfx::{
    CreateFxLoopSound, CreateFxOneshot, CreateFxOneshotEmitters, apply_createfx_effect_aliases,
    parse_createfx_effect_aliases, parse_createfx_loop_sounds, parse_createfx_oneshots,
};
pub use discover::{
    GamesRoot, ZoneFile, ensure_artifacts_dir, find_common_mp_for_envelope,
    find_common_mp_for_zone, find_localized_common_mp_for_zone, find_runtime_common_mp,
    find_runtime_zone, find_zone_file, find_zone_file_version, find_zone_for_tree,
    games_content_report, games_root_from_env, games_root_report, group_mp_maps, list_mp_maps,
    load_dotenv, map_load_title, peek_zone_version, split_zone_key, zone_game_for_path,
};
pub use dobj::{
    AIM_PITCH_CLAMP_RAD, AnimInstance, Attach, DObj, DObjError, ModelPoseSrc, TP_HEAD_ATTACH_TAG,
    TP_WEAPON_ATTACH_TAGS, apply_aim_pitches, apply_legs_yaw, build_body_head_weapon_dobj,
    build_body_weapon_dobj, pitch_spine_bone, tp_head_attach_tag, tp_weapon_attach_tag, yaw_bone,
};
pub use dyn_ents::{
    DYNENT_DRAW_BRUSH, DYNENT_DRAW_MODEL, DynEntCatalog, DynEntDef, DynEntDefScalars,
    DynEntDrawType, DynEntProps, DynEntType, OwnedPhysPreset, PhysPresetCatalog,
    RETAIL_DYN_ENT_PROPS, build_dyn_ent_catalog, parse_dyn_ent_def_scalars, retail_dyn_ent_props,
};
pub use ent_channel::{EntChannel, parse_ent_channel_file};
pub use fastfile_iw4::GlyphCapture;
pub use fpv_catalog::{
    FpvHands, FpvMeshBuild, FpvMeshCatalog, FpvMeshEntry, FpvMeshKey, PoseStats, TagViewBind,
    VIEWHANDS_NAME, VIEWHANDS_NAME_T5,
};
pub use fx_catalog::{
    FxBankSound, FxCatalog, FxChildEdge, FxDefinitions, FxElemMaterial, FxElemMaterialReason,
    FxElemModelEdge, OwnedFxEffectDef, OwnedFxElemDef, OwnedFxSparkFountainDef, OwnedFxTrailDef,
    OwnedFxVisual, alias_fx_color_map_stubs, elem_type as fx_elem_type,
    fx_color_decoded_in_catalog, fx_color_image_for_name, fx_material_bind_name,
    insert_fx_color_image, lookup_fx_color_image,
};
pub use fx_model_catalog::{FxModelCatalog, FxModelEntry};
pub use glass_catalog::{FxGlassReset, GlassZoneCensus, build_fx_glass_reset, build_glass_census};
pub use gltf_export::{GltfExportSummary, export_prepared_world_gltf};
pub use impact_fx_catalog::{ImpactFxCatalog, OwnedFxImpactEntry, OwnedFxImpactTable};
pub use iw5_tech_map::{
    IW5_CODE_COLOR_SATURATION_B, IW5_CODE_COLOR_SATURATION_G, IW5_CODE_COLOR_SATURATION_R,
    IW5_CODE_EYEOFFSET, IW5_NAME_MATCH_IW4_SLOT, IW5_TECHNIQUE_TYPE_COUNT,
    IW5_TECHNIQUE_TYPE_NAMES, LEFTOVER_IW5_CODE_BASE, iw5_slot_to_iw4, leftover_iw5_code_bank,
    leftover_iw5_slots, leftover_selector_census, remap_code_const_index, remap_code_texture_index,
    remap_occupancy_bits, remap_pass_count_by_slot, remap_shader_arg_type, remap_state_bits_entry,
    remap_technique_flags_by_slot,
};
pub use iwd::{IwdSoundIndex, NamespaceSoundIwd, NamespaceTree, NamespaceTrees};
pub use lane::{CommonCensus, LANE_GAPS, LaneGap, LoadedWorld, ZoneLane, lane};
pub use lane_capability::{LaneStatus, PreparedCapability, lane_status};
pub use lighting_iw4::MODEL_LIGHTING_TILE_BYTES;
pub use loading_screen::{LoadingPreviewSource, LoadingScreen};
pub use localize::{
    LocalizeCatalog, MP_LOCALIZED_ZONES, decode_localized_text, load_localize_catalog,
    load_localize_catalog_in_lane, load_localize_catalog_iw5, load_localize_catalog_t5,
    load_mp_localized_strings,
};
pub use map_entities::{
    FlagDescriptor, IntermissionView, MapEntsKeyCensus, MapScriptStruct, MapUseTrigger,
    MinimapCorners, ScriptBrushModelLink, ScriptBrushModelPlacement, ScriptModelId,
    ScriptModelPlacement, SpawnPoint, census_entity_string_keys, dm_spawn_points,
    dm_spawn_points_iw5, dm_spawn_points_t5, exploding_prop_machine, flag_descriptors,
    flag_descriptors_iw5, flag_descriptors_t5, intermission_view, intermission_view_iw5,
    intermission_view_t5, map_ents_entity_string, map_script_structs, map_script_structs_iw5,
    map_script_structs_t5, map_use_triggers, map_use_triggers_iw5, map_use_triggers_t5,
    minimap_corners, minimap_corners_iw5, minimap_corners_t5, parse_flag_descriptors,
    parse_map_script_structs, parse_map_use_triggers, script_brush_model_placements,
    script_brush_model_placements_iw5, script_brush_model_placements_t5, script_model_placements,
    script_model_placements_iw5, script_model_placements_t5, worldspawn_north_yaw,
    worldspawn_north_yaw_iw5, worldspawn_north_yaw_t5,
};
pub use map_script_sound::{
    MapScriptSoundFacts, MapScriptSoundSource, SessionMapScriptSound, ambient_play_alias,
    game_nested_string_assignment, game_string_assignment,
};
pub use match_load::{
    MapLoadApproval, MatchLoadAbort, MatchLoadAccepted, MatchLoadBusy, MatchLoadDispatch,
    MatchLoadRequest, PreparedMatchReady,
};
pub use material_catalog::{
    AssetPointerIdentity, AssetRefDumpCensus, AuthoredImage, AuthoredMaterial, AuthoredShader,
    AuthoredVertexDecl, CrossGameReason, CrossGameTechsetResolution, ImageVariantId,
    MaterialCatalog, MaterialDefinitions, MaterialImageMemory, MaterialTextureBinding,
    OwnedMaterialPass, OwnedShaderArgument, OwnedShaderRef, OwnedTechnique, OwnedTechniqueGraph,
    ShaderSourceCensus, T5TechniqueOccupancy, TS_2D, TS_COLOR_MAP, TS_DETAIL_MAP, TS_FUNCTION,
    TS_NORMAL_MAP, TS_SPECULAR_MAP, TS_T5_COLOR0_MAP, TS_T5_COLOR15_MAP, TS_T5_THROW_MAP,
    TS_WATER_MAP, TechniqueSetFacts, TechniqueTable, TechsetKey, TechsetResolve,
    VertexDeclStreamCensus, t5_feature_token_stripped,
};
pub use material_draw::{
    AlphaTest, ColorMapTransform, D3DCULL_CCW, D3DCULL_CW, D3DCULL_NONE, D3DRS_CULLMODE,
    GFXS0_ATEST_DISABLE, GFXS0_ATEST_GE_128, GFXS0_ATEST_GT_0, GFXS0_ATEST_LT_128,
    GFXS0_ATEST_MASK, GFXS0_ATEST_SHIFT, GFXS0_CULL_BACK, GFXS0_CULL_FRONT, GFXS0_CULL_MASK,
    GFXS0_CULL_NONE, GFXS0_CULL_SHIFT, GFXS0_SRGBWRITEENABLE, Gfxs0AlphaTest, MaterialCullFace,
    MaterialDrawMode, S_ALPHA_TEST_TABLE, S_CULL_TABLE, alpha_test_cutoff_from_state_bits,
    alpha_test_from_state_bits, cull_face_from_state_bits, d3d_cull_mode_from_state_bits,
    material_constant_name, srgb_write_enable_from_state_bits,
};
pub use material_images::{
    ImageWorkingSet, MaterialImageStats, cached_iwd_main_dirs, decode_catalog_images_from_iwd,
    decode_dxt5nm_xy, decode_in_zone_builtin_images, decode_map_preview,
    decode_material_color_maps, decode_menu_background, decode_reflection_probe_cubemap,
    decode_ui_image, decode_ui_image_from_main, decode_zone_image_rgba, game_main_for_zone,
    iwd_entry_reads, iwd_read_cost, last_image_working_set, mip_cache_cost, retail_lightmap_bake,
    retail_lit_color, shared_payload_copy_cost, shared_variant_census,
};
pub use menu_catalog::{
    CapturedStringTable, FontDef, HUD_CHROME_MENUS, ITEM_TYPE_BUTTON, ITEM_TYPE_TEXT, MenuCatalog,
    MenuDef, MenuItem, MenuRect, MenuSetLocalVar, UI_MENU_ZONES, ZoneUiImage, load_menu_catalog,
    load_ui_menu_catalog, ui_games_root,
};
pub use model_lighting::{
    BlockedReason, GridView, LitFragmentTileCensus, OwnedLightGrid, SampledLighting,
    SmodelLightingCensus, SmodelLightingSample, TileChromaSpan, build_smodel_lighting_samples,
    build_smodel_lighting_samples_from_world, build_smodel_lighting_samples_with_sight,
    census_lit_fragment_tiles, collect_iw5_smodel_lighting_origins,
    collect_smodel_lighting_origins, collect_t5_smodel_lighting_origins, grid_view_from_geometry,
    lit_fragment_mid_grey_from_tile, lit_fragment_white_from_tile,
    lit_fragment_white_from_tile_normal, lit_fragment_white_from_tile_texel,
    lit_fragment_white_sun_add, mean_tile_rgba01, packed_lighting_for_origins, sample_light_grid,
    sample_light_grid_with_lookup_fallback, sample_light_grid_with_sight, tile_chroma_span,
    tile_corners_match_compress,
};
pub use model_mesh::{
    MapXModelAssetKey, MapXModelMaterialCensus, MapXModelSceneAsset, MapXModelSceneCatalog,
    ModelColorCensus, ModelKind, ModelMesh, ModelMeshError, ModelSurfaceDraw,
    OwnedXRigidVertListCollision, OwnedXSurfaceCollisionTree, PreparedMapModels,
    RetailPackedVertexPayload, RetailXSurfaceCollisionPayload, ScriptModelMetadata,
    ScriptModelSceneInstance, StaticModelDraw, StaticModelDrawError, StaticModelInstance,
    StaticModelPlacement, SurfaceColorCensus, VertexColorStats, build_iw5_static_model_instances,
    build_iw5_xmodel_mesh, build_static_model_instances, build_t5_static_model_instances,
    build_t5_xmodel_mesh, build_xmodel_mesh, census_mesh_vertex_colors, census_model_mesh,
    census_rgba_f32, census_skel_vertex_colors, format_vertex_color_stats, model_kind,
};
pub use model_skel::{
    BoneBind, BoneCollision, FpvSkel, ModelLodSelector, ModelSkel, VertSkin, capture_body_skel,
    capture_body_skel_iw5, capture_body_skel_t5, capture_fpv_skel, capture_fpv_skel_iw5,
    capture_fpv_skel_t5, capture_world_weapon_skel, capture_xmodel_skel, capture_xmodel_skel_iw5,
    capture_xmodel_skel_t5, dobj_has_lod_for_dist, lod_surface_range, t5_lod, xmodel_lod_for_dist,
};
pub use penetration::{
    LochitTableError, PenTableError, capture_lochit_table, capture_pen_table,
    parse_lochit_info_string, parse_pen_table_info_string, parse_pen_table_rawfile,
};
pub use playeranim_parse::{
    ParsedAnimCommand, ParsedAnimCondition, ParsedAnimItem, ParsedPlayerAnimScript,
    PlayerAnimParseError,
};
pub use plugin::AssetPlugin;

pub use map_load_process::MapLoadProcess;
pub use prepared::{
    MapFacts, MatchMaterials, MatchType10SoundHints, PreparedBodies, PreparedBodyClips,
    PreparedDestructibleDeath, PreparedFpvMeshes, PreparedGaps, PreparedLocalizedStrings,
    PreparedMap, PreparedProjectileMeshes, PreparedWeapons, PreparedWorldWeapons, PreparedXAnims,
    PreparedXModelWalkCensus, SessionCompass,
};
pub use progress::{
    LoadLaneTiming, LoadProgress, LoadSnapshot, StageEnd, StageHandle, StageId, StageKey,
    StageOutcome, StageScope, StageSnapshot, WorkCount, peak_resident_bytes,
    process_resident_bytes,
};
pub use projectile_mesh_catalog::{
    ProjectileMeshBuild, ProjectileMeshCatalog, ProjectileMeshEntry,
};
pub use session_load::{
    MatchLoadOutcome, MatchMaterialSeed, PreparedMatch, PreparedWorld, WorldDrawPolicy,
    apply_match_material_map, load_match_material_catalog, load_match_material_seed, load_pool,
    load_prepared_match, load_shell_weapon_registry, load_workers, publish_process_cpus,
};
pub use soldiers::{
    SoldierKit, SoldierKits, arms_for_body, body_has_tp_attach_bones, ffa_assignment_is_axis,
    is_arms_model, is_body_model, kit_assignment_is_axis, soldier_kits,
};
pub use sound_catalog::{
    CapturedAlias, CapturedSndCurve, CapturedSound, LoadedSoundEdge, LoadedSoundEdgeReason,
    LoadedSoundPcm, MSS_PCM, PickLoadedOutcome, PickedSound, SoundAliasKey, SoundCatalog,
    lerp_range, pick_weighted_variant_index, snd_advance_lcg, snd_unit_random,
};
pub use sound_load::{LoadedSoundBank, load_mp_sound_bank, load_sound_catalog, namespace_for_zone};
pub use sound_load_iw5::load_sound_catalog_iw5;
pub use sound_load_t5::load_sound_catalog_t5;
pub use sound_wma_t5::{
    T5_WMA, XwmaClip, XwmaDecodeCost, decode_t5_xwma, decode_t5_xwma_batch, xwma_decode_cost,
};
pub use t5_code_remap::{
    IW4_CUSTOM_SAMPLER_DEST, LEFTOVER_T5_CODE_BASE, T5_CODE_CUSTOMWIND_CENTER,
    T5_CODE_CUSTOMWIND_SPRING, T5_CODE_EXTRA_CAM_PARAM, T5_CODE_EYEOFFSET, T5_CODE_FOG,
    T5_CODE_FOG_COLOR, T5_CODE_FOG2, T5_CODE_GENERIC_PARAM0, T5_CODE_GENERIC_PARAM1,
    T5_CODE_GENERIC_PARAM4, T5_CODE_GENERIC_PARAM5, T5_CODE_GENERIC_PARAM6,
    T5_CODE_GRASS_WIND_FORCE0, T5_CODE_HDRCONTROL_0, T5_CODE_HDRCONTROL_1, T5_CODE_HERO_LIGHTING_B,
    T5_CODE_HERO_LIGHTING_G, T5_CODE_HERO_LIGHTING_R, T5_CODE_LIGHT_ATTENUATION,
    T5_CODE_LIGHT_CONE_CONTROL1, T5_CODE_LIGHT_CONE_CONTROL2, T5_CODE_LIGHT_FALLOFF_A,
    T5_CODE_LIGHT_FALLOFF_B, T5_CODE_LIGHT_HERO_SCALE, T5_CODE_LIGHT_SPOT_AABB,
    T5_CODE_LIGHT_SPOT_COOKIE_SLIDE, T5_CODE_LIGHT_SPOT_MATRIX0, T5_CODE_LIGHT_SPOT_MATRIX1,
    T5_CODE_LIGHT_SPOT_MATRIX2, T5_CODE_LIGHT_SPOT_MATRIX3, T5_CODE_POSTFX_CONTROL0,
    T5_CODE_POSTFX_CONTROL6, T5_CODE_SKY_COLOR_MULTIPLIER, T5_CODE_SKY_TRANSITION,
    T5_CODE_SUN_DIFFUSE, T5_CODE_SUN_FOG, T5_CODE_SUN_FOG_COLOR, T5_CODE_SUN_FOG_DIR,
    T5_CODE_SUN_POSITION, T5_CODE_SUN_SPECULAR, T5_CODE_TREECANOPY_PARMS,
    T5_CODE_VARIANT_WIND_SPRING_0, T5_CODE_VARIANT_WIND_SPRING_15, T5_CODE_VPOS1_TO_WORLD,
    T5_CODE_VPOSX_TO_WORLD, T5_CODE_VPOSY_TO_WORLD, T5_CODE_WIND_DIRECTION, T5_CUSTOM_SAMPLER_DEST,
    leftover_t5_code_bank, remap_code_const_index as remap_t5_code_const_index,
    remap_code_texture_index as remap_t5_code_texture_index, remap_t5_code_const_source,
    remap_t5_custom_sampler_flags, t5_code_const_name,
};
pub use t5_tech_map::{
    IW4_TECHNIQUE_TYPE_COUNT, IW4_TECHNIQUE_TYPE_NAMES, OccupancyProjectionCensus,
    T5_NAME_MATCH_IW4_SLOT, T5_TECHNIQUE_TYPE_COUNT, T5_TECHNIQUE_TYPE_NAMES, T5OccupancyRemapGap,
    T5SlotProjection, census_occupancy, leftover_t5_selector_census, leftover_t5_slots,
    occupancy_to_technique_table, project_t5_slot, remap_t5_state_bits_entry,
    remap_t5_technique_flags, t5_slot_to_iw4,
};
pub use tracer_catalog::{
    OwnedTracerDef, TracerCatalog, TracerDefinitions, TracerMaterial, TracerMaterialReason,
};
pub use weapon_anim_dispatch::{
    ACTION_GOAL_TIME_SECS, ACTIVE_GOAL_WEIGHT, ANIM_RATE_TABLE, AnimRateOffsets,
    IDLE_INTERRUPT_GOAL_TIME_SECS, INACTIVE_GOAL_WEIGHT, WEAP_ANIM_EVENT_MASK,
    known_complete_rate_timer_offset, known_rate_timer_offset, playback_rate,
    slot_for_weap_anim_event, slot_uses_native_rate,
};
pub use weapon_animations::{AdsOverlayConvention, WeaponAnimSlot, WeaponAnimations};
pub use weapon_catalog::{
    CacOffhandBucket, CatalogWeapon, LoadoutCatalogKind, LoadoutCatalogRow, NotetrackConvention,
    T5_NOTE_RUMBLE_PREFIX, T5_NOTE_SOUND_PREFIX, UnknownWeaponName, WeaponBodyFacts, WeaponBuild,
    WeaponCatalog, WeaponCombatFx, WeaponHudMaterialEdges, WeaponKickFacts, WeaponProjectileFx,
    WeaponRegistry, WeaponReticleAssets, WeaponSoundAliases, WeaponSoundSlot, WeaponSwayFacts,
    cac_offhand_bucket, gsc_weapon_script_name, gun_candidates_from_idle, overlay_name_is_hud_iris,
    t5_inline_note_alias,
};
pub use world_draw::{
    CapturedLightDef, DpvsWorldData, GfxBrushModelBounds, GfxBrushModelSurfs, OwnedPortal,
    RetailWorldVertexPayload, SunEffectsCapture, WorldBatch, WorldDraw, WorldLightRegionHull,
    WorldLightmap, WorldLightmapGap, WorldPrimaryLight, WorldReflectionProbe, WorldShadowGeometry,
    brush_model_vertex_centroid, build_world_draw,
};
pub(crate) use world_draw::{
    capture_light_defs, resolve_outdoor_image, resolve_primary_light_attenuation,
};
pub use world_iw5::build_iw5_world_draw;
pub(crate) use world_iw5::capture_iw5_light_defs;
pub use world_mesh::{
    WorldMeshError, WorldMeshStats, build_world_mesh, bumped_world_normal, half_to_f32,
    unpack_packed_tex_coords,
};
pub use world_t5::{build_t5_world_draw, build_t5_world_mesh};
pub use world_weapon_catalog::{WorldWeaponBuild, WorldWeaponCatalog, WorldWeaponEntry};
pub use xanim_catalog::{CapturedXAnim, XAnimBuild, XAnimCatalog, XAnimKey};
pub use xanim_clip::{
    AnimClip, ClipError, ClipNotify, FrameIndices, Keyed, RawXAnimParts, Rotation, SampledTrack,
    Track, Translation,
};
pub use zone::{
    Iw5ZoneMemory, T5ZoneMemory, ZoneImage, ZoneMemory, ZoneOpenError, open_zone, open_zone_shared,
    parse_zone_image,
};

pub use asset_material::vertex_layout::{T5_WORLD_LAYER_HOST_STRIDE, VertexLayoutFamily};

pub use asset_material::{material_alpha_test, t5_smodel_camera_emits};

pub mod image_handles;
