pub const XMODEL: usize = 304;
pub const XMODEL_LOD_INFO: usize = 44;
pub const XMODEL_SURFS: usize = 36;
pub const XSURFACE: usize = 64;
pub const XRIGID_VERT_LIST: usize = 12;
pub const XSURFACE_COLLISION_TREE: usize = 40;
pub const XSURFACE_COLLISION_NODE: usize = 16;
pub const XSURFACE_COLLISION_LEAF: usize = 2;
pub const XMODEL_COLL_SURF: usize = 44;
pub const XMODEL_COLL_TRI: usize = 48;
pub const XBONE_INFO: usize = 28;
pub const XMODEL_QUAT: usize = 8;
pub const DOBJ_ANIM_MAT: usize = 32;

pub const GFX_PACKED_VERTEX: usize = 32;

pub const XSURFACE_TRI16: usize = 6;

pub const XANIM_PARTS: usize = 88;
pub const XANIM_DELTA_PART: usize = 12;
pub const XANIM_NOTIFY_INFO: usize = 8;

pub const MATERIAL: usize = 96;
pub const MATERIAL_TEXTURE_DEF: usize = 12;
pub const MATERIAL_CONSTANT_DEF: usize = 32;
pub const GFX_STATE_BITS: usize = 8;
pub const WATER: usize = 68;

pub const TECHNIQUE_SET: usize = 204;

pub const TECHNIQUE_SET_WORLD_VERT_FORMAT_OFF: usize = 4;
pub const MATERIAL_PASS: usize = 20;
pub const MATERIAL_SHADER_ARGUMENT: usize = 8;

pub const TECHNIQUE_SLOT_COUNT: usize = 48;

pub const GFX_IMAGE: usize = 32;

pub const GFX_IMAGE_LOAD_DEF: usize = 16;
pub const PIXEL_SHADER: usize = 16;
pub const VERTEX_SHADER: usize = 16;
pub const VERTEX_DECL: usize = 100;

pub const PHYS_PRESET: usize = 44;
pub const PHYS_COLLMAP: usize = 72;
pub const PHYS_GEOM_INFO: usize = 68;
pub const BRUSH_WRAPPER: usize = 68;
pub const CBRUSH_SIDE: usize = 8;
pub const CPLANE: usize = 20;

pub const FX_EFFECT_DEF: usize = 32;
pub const FX_ELEM_DEF: usize = 252;
pub const FX_ELEM_VEL_STATE_SAMPLE: usize = 96;
pub const FX_ELEM_VIS_STATE_SAMPLE: usize = 48;
pub const FX_ELEM_MARK_VISUALS: usize = 8;
pub const FX_ELEM_VISUALS: usize = 4;
pub const FX_TRAIL_DEF: usize = 36;
pub const FX_TRAIL_VERTEX: usize = 20;
pub const FX_SPARK_FOUNTAIN_DEF: usize = 52;

pub const COM_WORLD: usize = 16;
pub const COM_PRIMARY_LIGHT: usize = 68;
pub const GAME_WORLD_MP: usize = 8;
pub const G_GLASS_DATA: usize = 128;
pub const G_GLASS_PIECE: usize = 12;
pub const G_GLASS_NAME: usize = 12;
pub const MAP_ENTS: usize = 44;
pub const MAP_TRIGGERS: usize = 24;
pub const TRIGGER_MODEL: usize = 8;
pub const TRIGGER_HULL: usize = 32;
pub const TRIGGER_SLAB: usize = 20;
pub const STAGE: usize = 20;
pub const FX_WORLD: usize = 116;
pub const FX_GLASS_DEF: usize = 36;
pub const FX_GLASS_PIECE_PLACE: usize = 32;
pub const FX_GLASS_PIECE_STATE: usize = 32;
pub const FX_GLASS_PIECE_DYNAMICS: usize = 36;
pub const FX_GLASS_GEOMETRY_DATA: usize = 4;
pub const FX_GLASS_INIT_PIECE_STATE: usize = 52;

pub const FX_IMPACT_ENTRY: usize = 140;
pub const SURF_TYPE_NUM: usize = 31;

pub const GFX_LIGHT_DEF: usize = 16;

pub const CLIP_MAP: usize = 256;
pub const C_STATIC_MODEL: usize = 76;
pub const CLIP_MATERIAL: usize = 12;
pub const C_NODE: usize = 8;
pub const C_LEAF: usize = 40;
pub const C_LEAF_BRUSH_NODE: usize = 20;
pub const COLLISION_BORDER: usize = 28;
pub const COLLISION_PARTITION: usize = 12;
pub const COLLISION_AABB_TREE: usize = 32;
pub const CMODEL: usize = 68;
pub const CBRUSH: usize = 36;
pub const SMODEL_AABB_NODE: usize = 28;
pub const DYN_ENTITY_DEF: usize = 92;
pub const DYN_ENTITY_POSE: usize = 32;
pub const DYN_ENTITY_CLIENT: usize = 12;
pub const DYN_ENTITY_COLL: usize = 20;

pub const GFX_WORLD: usize = 628;
pub const GFX_SKY: usize = 16;
pub const GFX_AABB_TREE: usize = 44;
pub const GFX_CELL: usize = 40;
pub const GFX_PORTAL: usize = 60;

pub const GFX_PORTAL_HULL_AXIS: usize = 0x24;
pub const GFX_LIGHTMAP_ARRAY: usize = 8;
pub const GFX_BRUSH_MODEL: usize = 60;
pub const MATERIAL_MEMORY: usize = 8;
pub const GFX_SHADOW_GEOMETRY: usize = 12;
pub const GFX_LIGHT_REGION: usize = 8;
pub const GFX_LIGHT_REGION_HULL: usize = 80;

pub const GFX_LIGHT_REGION_AXIS: usize = 20;
pub const GFX_SURFACE: usize = 24;

pub const GFX_SURFACE_BOUNDS: usize = 24;
pub const GFX_STATIC_MODEL_DRAW_INST: usize = 76;
pub const GFX_HERO_ONLY_LIGHT: usize = 56;

pub const GFX_WORLD_VERTEX: usize = 44;

pub const GFX_STATIC_MODEL_INST: usize = 36;

pub const GFX_LIGHT_GRID_COLORS: usize = 168;

pub const RAW_FILE: usize = 16;
pub const STRING_TABLE: usize = 16;
pub const STRING_TABLE_CELL: usize = 8;

pub const LEADERBOARD_DEF: usize = 24;

pub const LB_COLUMN_DEF: usize = 32;

pub const STRUCTURED_DATA_DEF_SET: usize = 12;

pub const STRUCTURED_DATA_DEF: usize = 52;
pub const STRUCTURED_DATA_ENUM: usize = 12;
pub const STRUCTURED_DATA_ENUM_ENTRY: usize = 8;
pub const STRUCTURED_DATA_STRUCT: usize = 16;
pub const STRUCTURED_DATA_STRUCT_PROPERTY: usize = 16;
pub const STRUCTURED_DATA_ARRAY: usize = 16;

pub const LOCALIZE_ENTRY: usize = 8;

pub const FONT: usize = 24;

pub const GLYPH: usize = 24;

pub const MENU_LIST: usize = 12;
pub const MENU_DEF: usize = 400;
pub const ITEM_DEF: usize = 380;
pub const WINDOW_DEF: usize = 164;
pub const EXPRESSION_SUPPORTING_DATA: usize = 24;
pub const STATEMENT: usize = 24;
pub const EXPRESSION_ENTRY: usize = 12;
pub const STATIC_DVAR: usize = 8;
pub const MENU_EVENT_HANDLER_SET: usize = 8;
pub const MENU_EVENT_HANDLER: usize = 8;
pub const CONDITIONAL_SCRIPT: usize = 8;
pub const SET_LOCAL_VAR_DATA: usize = 8;
pub const ITEM_KEY_HANDLER: usize = 12;
pub const ITEM_FLOAT_EXPRESSION: usize = 8;
pub const LIST_BOX_DEF: usize = 324;
pub const EDIT_FIELD_DEF: usize = 32;
pub const MULTI_DEF: usize = 392;
pub const NEWS_TICKER_DEF: usize = 28;
pub const TEXT_SCROLL_DEF: usize = 4;

pub const VEHICLE_DEF: usize = 720;

pub const SND_ALIAS_LIST: usize = 12;

pub const SND_ALIAS: usize = 100;

pub const SOUND_FILE: usize = 12;

pub const SPEAKER_MAP: usize = 408;

pub const LOADED_SOUND: usize = 44;

pub const SND_CURVE: usize = 136;

pub const WEAPON_COMPLETE_DEF: usize = 116;
pub const WEAPON_DEF: usize = 1668;
pub const SND_ALIAS_CUSTOM: usize = 4;
pub const TRACER_DEF: usize = 112;
pub const WEAPON_ANIM_COUNT: usize = 37;

pub const WEAP_ANIM_IDLE: usize = weap_anim::IDLE;

pub mod weap_anim {
    pub const ROOT: usize = 0x0;
    pub const IDLE: usize = 0x1;
    pub const EMPTY_IDLE: usize = 0x2;
    pub const FIRE: usize = 0x3;
    pub const HOLD_FIRE: usize = 0x4;
    pub const LASTSHOT: usize = 0x5;
    pub const RECHAMBER: usize = 0x6;
    pub const MELEE: usize = 0x7;
    pub const MELEE_CHARGE: usize = 0x8;
    pub const RELOAD: usize = 0x9;
    pub const RELOAD_EMPTY: usize = 0xA;
    pub const RELOAD_START: usize = 0xB;
    pub const RELOAD_END: usize = 0xC;
    pub const RAISE: usize = 0xD;
    pub const FIRST_RAISE: usize = 0xE;
    pub const BREACH_RAISE: usize = 0xF;
    pub const DROP: usize = 0x10;
    pub const ALT_RAISE: usize = 0x11;
    pub const ALT_DROP: usize = 0x12;
    pub const QUICK_RAISE: usize = 0x13;
    pub const QUICK_DROP: usize = 0x14;
    pub const EMPTY_RAISE: usize = 0x15;
    pub const EMPTY_DROP: usize = 0x16;
    pub const SPRINT_IN: usize = 0x17;
    pub const SPRINT_LOOP: usize = 0x18;
    pub const SPRINT_OUT: usize = 0x19;
    pub const STUNNED_START: usize = 0x1A;
    pub const STUNNED_LOOP: usize = 0x1B;
    pub const STUNNED_END: usize = 0x1C;
    pub const DETONATE: usize = 0x1D;
    pub const NIGHTVISION_WEAR: usize = 0x1E;
    pub const NIGHTVISION_REMOVE: usize = 0x1F;
    pub const ADS_FIRE: usize = 0x20;
    pub const ADS_LASTSHOT: usize = 0x21;
    pub const ADS_RECHAMBER: usize = 0x22;
    pub const ADS_UP: usize = 0x23;
    pub const ADS_DOWN: usize = 0x24;
}

pub mod fx_elem {
    pub const TAIL: u8 = 2;
    pub const TRAIL: u8 = 3;
    pub const SPARK_FOUNTAIN: u8 = 6;
    pub const MODEL: u8 = 7;
    pub const SOUND: u8 = 10;
    pub const DECAL: u8 = 11;
    pub const RUNNER: u8 = 12;

    pub const fn is_sprite(t: u8) -> bool {
        matches!(t, 0 | 1 | TAIL | TRAIL | 4 | 5 | SPARK_FOUNTAIN)
    }
}

pub mod mtl_arg {
    pub const MATERIAL_VERTEX_CONST: u16 = 0;

    pub const LITERAL_VERTEX_CONST: u16 = 1;

    pub const MATERIAL_PIXEL_SAMPLER: u16 = 2;

    pub const CODE_VERTEX_CONST: u16 = 3;

    pub const CODE_PIXEL_SAMPLER: u16 = 4;

    pub const CODE_PIXEL_CONST: u16 = 5;

    pub const MATERIAL_PIXEL_CONST: u16 = 6;

    pub const LITERAL_PIXEL_CONST: u16 = 7;
}
