use core::fmt;

use bevy::prelude::*;
use diag::gap::{self as ledger, Gap as _, GapLedger};

const HUD_GAP_COUNT: usize = <HudGap as ledger::Gap>::ALL.len();

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum HudGap {
    ScriptedHudElem,

    ReticleWeaponDef,

    ReticleMaterial,

    ReticleSideQuadOffsets,

    CompassMap,

    CompassObjectives,

    WeaponDisplayName,

    LocalizedText,

    GameMessage,

    Obituary,

    Weaponbar,

    Scoreboard,

    BloodOverlay,

    FlashWhiteout,

    HudElemMaterial,

    AdsOverlay,

    DeathIcons,

    RetailFont,

    MenuVisExp,

    EngineSplash,

    MantleHint,

    OverheadNames,

    PlayerCard,

    PerkDisplay,

    CompassRing,

    TextDecodeFx,
}

impl ledger::Gap for HudGap {
    const ALL: &'static [HudGap] = &[
        HudGap::ScriptedHudElem,
        HudGap::ReticleWeaponDef,
        HudGap::ReticleMaterial,
        HudGap::ReticleSideQuadOffsets,
        HudGap::CompassMap,
        HudGap::CompassObjectives,
        HudGap::WeaponDisplayName,
        HudGap::LocalizedText,
        HudGap::GameMessage,
        HudGap::Obituary,
        HudGap::Weaponbar,
        HudGap::Scoreboard,
        HudGap::BloodOverlay,
        HudGap::FlashWhiteout,
        HudGap::HudElemMaterial,
        HudGap::AdsOverlay,
        HudGap::DeathIcons,
        HudGap::RetailFont,
        HudGap::MenuVisExp,
        HudGap::EngineSplash,
        HudGap::MantleHint,
        HudGap::OverheadNames,
        HudGap::PlayerCard,
        HudGap::PerkDisplay,
        HudGap::CompassRing,
        HudGap::TextDecodeFx,
    ];

    fn name(self) -> &'static str {
        match self {
            HudGap::ScriptedHudElem => "scripted-hudelem",
            HudGap::ReticleWeaponDef => "reticle-weapondef",
            HudGap::ReticleMaterial => "reticle-material",
            HudGap::ReticleSideQuadOffsets => "reticle-side-quad-offsets",
            HudGap::CompassMap => "compass-map",
            HudGap::CompassObjectives => "compass-objectives",
            HudGap::WeaponDisplayName => "weapon-display-name",
            HudGap::LocalizedText => "localized-text",
            HudGap::GameMessage => "game-message",
            HudGap::Obituary => "obituary",
            HudGap::Weaponbar => "weaponbar",
            HudGap::Scoreboard => "scoreboard",
            HudGap::BloodOverlay => "blood-overlay",
            HudGap::FlashWhiteout => "flash-whiteout",
            HudGap::HudElemMaterial => "hudelem-material",
            HudGap::AdsOverlay => "ads-overlay",
            HudGap::DeathIcons => "death-icons",
            HudGap::RetailFont => "retail-font",
            HudGap::MenuVisExp => "menu-visexp",
            HudGap::EngineSplash => "engine-splash",
            HudGap::MantleHint => "mantle-hint",
            HudGap::TextDecodeFx => "text-decode-fx",
            HudGap::OverheadNames => "overhead-names",
            HudGap::PlayerCard => "playercard",
            HudGap::PerkDisplay => "perk-display",
            HudGap::CompassRing => "compass-ring",
        }
    }

    fn is_standing(self) -> bool {
        !matches!(
            self,
            HudGap::ReticleWeaponDef
                | HudGap::ReticleMaterial
                | HudGap::WeaponDisplayName
                | HudGap::LocalizedText
                | HudGap::CompassMap
                | HudGap::BloodOverlay
                | HudGap::FlashWhiteout
                | HudGap::HudElemMaterial
                | HudGap::AdsOverlay
                | HudGap::MenuVisExp
                | HudGap::RetailFont
                | HudGap::EngineSplash
                | HudGap::Obituary
                | HudGap::MantleHint
                | HudGap::OverheadNames
                | HudGap::PlayerCard
                | HudGap::PerkDisplay
                | HudGap::CompassRing
                | HudGap::TextDecodeFx
                | HudGap::Weaponbar
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReticleSlot {
    Center,
    Side,
}

impl fmt::Display for ReticleSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReticleSlot::Center => f.write_str("centre"),
            ReticleSlot::Side => f.write_str("side"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageMiss {
    NoGamesRoot,

    NotDecoded,
}

impl fmt::Display for ImageMiss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageMiss::NoGamesRoot => f.write_str("no games root is set"),
            ImageMiss::NotDecoded => f.write_str("not a zone atlas and not in the IWD"),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GapCause {
    ReticleNoWeaponCatalog,

    ReticleWeaponNotInCatalog {
        viewmodel_index: u32,
    },

    ReticleNoAuthoredMaterials {
        viewmodel_index: u32,
    },

    ReticleSlotNamesNoImage {
        slot: ReticleSlot,
    },

    ReticleImageMissing {
        slot: ReticleSlot,
        name: String,
        miss: ImageMiss,
    },

    CompassNoImageDeclared,

    CompassImageMissing {
        name: String,
        miss: ImageMiss,
    },

    CompassNoMinimapCorners,

    CompassCornersDegenerate,

    CompassNoCatalog,

    CompassNoMapItem,

    NameNoWeaponCatalog,

    NameNoDisplayNameKey {
        viewmodel_index: u32,
    },

    NoStringTable,

    LocalizedRowMissing {
        key: String,
    },

    BloodOverlayImageMissing {
        name: String,
        miss: ImageMiss,
    },

    BloodOverlayMaterialUnsupported {
        detail: String,
    },

    FlashWhiteoutImageMissing {
        name: String,
        miss: ImageMiss,
    },

    HudElemImageMissing {
        name: String,
        miss: ImageMiss,
    },

    HudElemMaterialUnbound {
        material_index: i32,
    },

    AdsOverlayNamesNoImage {
        material: Option<String>,
    },

    AdsOverlayImageMissing {
        name: String,
        miss: ImageMiss,
    },

    AdsOverlayNoSize,

    ScorebarNoCatalog,

    VisExpUneval {
        menu: String,
        item: usize,
        err: String,
    },

    MenuExpression {
        menu: String,
        item: usize,
        err: String,
    },

    MenuScriptUnsupported {
        menu: String,
        command: String,
    },

    NoFontCatalog,

    FontMissing {
        name: String,
    },

    FontAtlasMissing {
        material: String,
        image: Option<String>,
    },

    SplashNoTable,

    SplashNoMenu {
        name: String,
    },

    SplashKeyMissing {
        key: String,
    },

    SplashImageMissing {
        name: String,
        miss: ImageMiss,
    },

    SplashEmptyPaint {
        name: String,
    },

    ObituaryNoClientInfo,

    ObituaryKillIconMissing {
        name: String,
        miss: ImageMiss,
    },

    GameNotifyNoClientInfo,

    TextDecodeFxAtlasMissing {
        material: String,
        image: Option<String>,
    },

    TextDecodeFxDecayTooShort {
        fx_decay_duration: i32,
    },

    MantleHintNoItem,

    CursorHintMissing {
        reason: String,
    },

    MantleHintImageMissing {
        name: String,
        miss: ImageMiss,
    },

    OverheadNoPresentedClient {
        client: u32,
    },

    OverheadVisibilityUnavailable,

    OverheadHeadUnavailable {
        entnum: u16,
    },

    OverheadFlashUnavailable,

    OverheadPartyUnavailable {
        client: u32,
    },

    OverheadFontUnavailable,

    OverheadRankUnavailable {
        client: u32,
    },

    PlayerCardNoCatalog,

    PlayerCardNoMenu {
        name: String,
    },

    PlayerCardNoCardSlot {
        slot: i32,
    },

    PlayerCardNoLocalVar {
        name: String,
    },

    PlayerCardTableMissing {
        name: String,
    },

    PlayerCardMaterialMissing {
        name: String,
        miss: ImageMiss,
    },

    PlayerCardEmptyPaint {
        name: String,
    },

    PerkNoCatalog,

    PerkNoMenu {
        name: String,
    },

    PerkTableMissing {
        name: String,
    },

    PerkMaterialMissing {
        name: String,
        miss: ImageMiss,
    },

    PerkEmptyPaint {
        name: String,
    },

    WeaponbarPaint {
        error: String,
    },
    PerkPaint {
        error: String,
    },

    CompassRingMaterialMissing {
        name: String,
        miss: ImageMiss,
    },

    CompassRingNoMenu {
        name: String,
    },
}

impl ledger::GapCause for GapCause {
    type Gap = HudGap;

    fn gap(&self) -> HudGap {
        match self {
            GapCause::ReticleNoWeaponCatalog | GapCause::ReticleWeaponNotInCatalog { .. } => {
                HudGap::ReticleWeaponDef
            }
            GapCause::ReticleNoAuthoredMaterials { .. }
            | GapCause::ReticleSlotNamesNoImage { .. }
            | GapCause::ReticleImageMissing { .. } => HudGap::ReticleMaterial,
            GapCause::CompassNoImageDeclared
            | GapCause::CompassImageMissing { .. }
            | GapCause::CompassNoMinimapCorners
            | GapCause::CompassCornersDegenerate
            | GapCause::CompassNoCatalog
            | GapCause::CompassNoMapItem => HudGap::CompassMap,
            GapCause::NameNoWeaponCatalog | GapCause::NameNoDisplayNameKey { .. } => {
                HudGap::WeaponDisplayName
            }
            GapCause::NoStringTable | GapCause::LocalizedRowMissing { .. } => HudGap::LocalizedText,
            GapCause::BloodOverlayImageMissing { .. }
            | GapCause::BloodOverlayMaterialUnsupported { .. } => HudGap::BloodOverlay,
            GapCause::FlashWhiteoutImageMissing { .. } => HudGap::FlashWhiteout,
            GapCause::HudElemImageMissing { .. } | GapCause::HudElemMaterialUnbound { .. } => {
                HudGap::HudElemMaterial
            }
            GapCause::AdsOverlayNamesNoImage { .. }
            | GapCause::AdsOverlayImageMissing { .. }
            | GapCause::AdsOverlayNoSize => HudGap::AdsOverlay,
            GapCause::ScorebarNoCatalog
            | GapCause::VisExpUneval { .. }
            | GapCause::MenuExpression { .. }
            | GapCause::MenuScriptUnsupported { .. } => HudGap::MenuVisExp,
            GapCause::NoFontCatalog
            | GapCause::FontMissing { .. }
            | GapCause::FontAtlasMissing { .. } => HudGap::RetailFont,
            GapCause::SplashNoTable
            | GapCause::SplashNoMenu { .. }
            | GapCause::SplashKeyMissing { .. }
            | GapCause::SplashImageMissing { .. }
            | GapCause::SplashEmptyPaint { .. } => HudGap::EngineSplash,
            GapCause::ObituaryNoClientInfo
            | GapCause::ObituaryKillIconMissing { .. }
            | GapCause::GameNotifyNoClientInfo => HudGap::Obituary,
            GapCause::TextDecodeFxAtlasMissing { .. }
            | GapCause::TextDecodeFxDecayTooShort { .. } => HudGap::TextDecodeFx,
            GapCause::CursorHintMissing { .. } => HudGap::Weaponbar,
            GapCause::MantleHintNoItem | GapCause::MantleHintImageMissing { .. } => {
                HudGap::MantleHint
            }
            GapCause::OverheadNoPresentedClient { .. }
            | GapCause::OverheadVisibilityUnavailable
            | GapCause::OverheadHeadUnavailable { .. }
            | GapCause::OverheadFlashUnavailable
            | GapCause::OverheadPartyUnavailable { .. }
            | GapCause::OverheadFontUnavailable
            | GapCause::OverheadRankUnavailable { .. } => HudGap::OverheadNames,
            GapCause::PlayerCardNoCatalog
            | GapCause::PlayerCardNoMenu { .. }
            | GapCause::PlayerCardNoCardSlot { .. }
            | GapCause::PlayerCardNoLocalVar { .. }
            | GapCause::PlayerCardTableMissing { .. }
            | GapCause::PlayerCardMaterialMissing { .. }
            | GapCause::PlayerCardEmptyPaint { .. } => HudGap::PlayerCard,
            GapCause::WeaponbarPaint { .. } => HudGap::Weaponbar,
            GapCause::PerkPaint { .. }
            | GapCause::PerkNoCatalog
            | GapCause::PerkNoMenu { .. }
            | GapCause::PerkTableMissing { .. }
            | GapCause::PerkMaterialMissing { .. }
            | GapCause::PerkEmptyPaint { .. } => HudGap::PerkDisplay,
            GapCause::CompassRingMaterialMissing { .. } | GapCause::CompassRingNoMenu { .. } => {
                HudGap::CompassRing
            }
        }
    }
}

impl fmt::Display for GapCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GapCause::ReticleNoWeaponCatalog => f.write_str("no weapon catalog is loaded"),
            GapCause::ReticleWeaponNotInCatalog { viewmodel_index } => {
                write!(
                    f,
                    "viewmodel weapon index {viewmodel_index} is not in the catalog"
                )
            }
            GapCause::ReticleNoAuthoredMaterials { viewmodel_index } => write!(
                f,
                "weapon index {viewmodel_index} authored no reticle material in either slot"
            ),
            GapCause::ReticleSlotNamesNoImage { slot } => {
                write!(f, "the {slot} material names no image")
            }
            GapCause::ReticleImageMissing { slot, name, miss } => {
                write!(f, "{slot} image `{name}` is {miss}")
            }
            GapCause::CompassNoImageDeclared => {
                f.write_str("the map declared no minimap material or that material names no image")
            }
            GapCause::CompassImageMissing { name, miss } => {
                write!(f, "minimap image `{name}` is {miss}")
            }
            GapCause::CompassNoMinimapCorners => {
                f.write_str("MapEnts carries no minimap_corner pair")
            }
            GapCause::CompassCornersDegenerate => {
                f.write_str("the minimap_corner pair spans no area")
            }
            GapCause::CompassNoCatalog => {
                f.write_str("MenuCatalog is not loaded; minimap_fullscreen cannot place")
            }
            GapCause::CompassNoMapItem => {
                f.write_str("minimap_fullscreen has no ownerDraw 159 minimap_map item")
            }
            GapCause::NameNoWeaponCatalog => f.write_str("no weapon catalog is loaded"),
            GapCause::NameNoDisplayNameKey { viewmodel_index } => write!(
                f,
                "weapon index {viewmodel_index} carries no display-name key"
            ),
            GapCause::NoStringTable => f.write_str("no localized string table is loaded"),
            GapCause::LocalizedRowMissing { key } => {
                write!(f, "key `{key}` has no row in the loaded language")
            }
            GapCause::BloodOverlayImageMissing { name, miss } => {
                write!(f, "overlay image `{name}` is {miss}")
            }
            GapCause::BloodOverlayMaterialUnsupported { detail } => f.write_str(detail),
            GapCause::FlashWhiteoutImageMissing { name, miss } => {
                write!(f, "flash white image `{name}` is {miss}")
            }
            GapCause::HudElemMaterialUnbound { material_index } => write!(
                f,
                "HudElem materialIndex {material_index} resolves to no CS_HUDMATERIALS name"
            ),
            GapCause::HudElemImageMissing { name, miss } => {
                write!(f, "hud elem material `{name}` is {miss}")
            }
            GapCause::AdsOverlayNamesNoImage { material } => match material {
                Some(name) => write!(f, "overlay material `{name}` names no image"),
                None => f.write_str("overlayMaterial is authored and names no material"),
            },
            GapCause::AdsOverlayImageMissing { name, miss } => {
                write!(f, "iris image `{name}` is {miss}")
            }
            GapCause::AdsOverlayNoSize => {
                f.write_str("adsOverlayWidth×Height is not a drawable rect")
            }
            GapCause::ScorebarNoCatalog => {
                f.write_str("MenuCatalog is not loaded; scorebar visExp cannot run")
            }
            GapCause::VisExpUneval { menu, item, err } => {
                write!(f, "{menu}[{item}] visExp: {err}")
            }
            GapCause::MenuExpression { menu, item, err } => {
                write!(f, "{menu}[{item}] expression: {err}")
            }
            GapCause::MenuScriptUnsupported { menu, command } => {
                write!(f, "{menu}: menu script `{command}` is not executed")
            }
            GapCause::NoFontCatalog => {
                f.write_str("MenuCatalog is not loaded; Font_s cannot be walked")
            }
            GapCause::FontMissing { name } => {
                write!(f, "catalog has no Font_s `{name}`")
            }
            GapCause::FontAtlasMissing { material, image } => match image {
                Some(image) => write!(
                    f,
                    "font material `{material}` image `{image}` did not upload"
                ),
                None => write!(f, "font material `{material}` has no zone atlas"),
            },
            GapCause::SplashNoTable => {
                f.write_str("mp/splashTable.csv was not in the UI-zone StringTable walk")
            }
            GapCause::SplashNoMenu { name } => {
                write!(f, "MenuCatalog has no `{name}` for this splash row")
            }
            GapCause::SplashKeyMissing { key } => {
                write!(f, "splashTable has no row `{key}`")
            }
            GapCause::SplashImageMissing { name, miss } => {
                write!(f, "splash image `{name}` is {miss}")
            }
            GapCause::SplashEmptyPaint { name } => {
                write!(
                    f,
                    "`{name}` Item_Paint emitted no tess quads for a live splash slot"
                )
            }
            GapCause::ObituaryNoClientInfo => {
                f.write_str("clientState.name empty; killfeed icon only")
            }
            GapCause::GameNotifyNoClientInfo => {
                f.write_str("clientState.name empty; gamenotify has no name")
            }
            GapCause::ObituaryKillIconMissing { name, miss } => {
                write!(f, "killfeed icon `{name}` is {miss}")
            }
            GapCause::TextDecodeFxAtlasMissing { material, image } => match image {
                Some(image) => write!(
                    f,
                    "decode FX material `{material}` image `{image}` did not upload"
                ),
                None => write!(f, "decode FX material `{material}` did not decode"),
            },
            GapCause::TextDecodeFxDecayTooShort { fx_decay_duration } => write!(
                f,
                "fxDecayDuration {fx_decay_duration} ms is under one 30 Hz decay tick"
            ),
            GapCause::CursorHintMissing { reason } => write!(f, "cursor hint: {reason}"),
            GapCause::MantleHintNoItem => f.write_str("hud_fullscreen has no ownerDraw 80 item"),
            GapCause::MantleHintImageMissing { name, miss } => {
                write!(f, "mantle hint image `{name}` is {miss}")
            }
            GapCause::OverheadNoPresentedClient { client } => {
                write!(f, "client {client} has no presented clientState row")
            }
            GapCause::OverheadVisibilityUnavailable => {
                f.write_str("client point trace or FX visibility is unavailable")
            }
            GapCause::OverheadHeadUnavailable { entnum } => {
                write!(f, "entity {entnum} has no same-Present posed-head product")
            }
            GapCause::OverheadFlashUnavailable => {
                f.write_str("local presented playerState is missing for CG_IsFlashbanged")
            }
            GapCause::OverheadPartyUnavailable { client } => {
                write!(f, "party relation to client {client} is unknown")
            }
            GapCause::OverheadFontUnavailable => {
                f.write_str("fonts/hudsmallfont or its atlas is unavailable")
            }
            GapCause::OverheadRankUnavailable { client } => {
                write!(
                    f,
                    "rank/prestige presentation for client {client} is unavailable"
                )
            }
            GapCause::PlayerCardNoCatalog => {
                f.write_str("MenuCatalog is not loaded; playercard menus cannot run")
            }
            GapCause::PlayerCardNoMenu { name } => {
                write!(f, "MenuCatalog has no `{name}`")
            }
            GapCause::PlayerCardNoCardSlot { slot } => {
                write!(f, "g_PlayerCardCache has no script slot {slot}")
            }
            GapCause::PlayerCardNoLocalVar { name } => {
                write!(f, "localvar `{name}` was not set by script-menu onOpen")
            }
            GapCause::PlayerCardTableMissing { name } => {
                write!(f, "string table `{name}` was not in the UI-zone walk")
            }
            GapCause::PlayerCardMaterialMissing { name, miss } => {
                write!(f, "playercard image `{name}` is {miss}")
            }
            GapCause::PlayerCardEmptyPaint { name } => {
                write!(f, "`{name}` Item_Paint emitted no tess quads")
            }
            GapCause::WeaponbarPaint { error } => write!(f, "weaponbar_hd: {error}"),
            GapCause::PerkPaint { error } => write!(f, "perks_info_hd: {error}"),
            GapCause::PerkNoCatalog => {
                f.write_str("MenuCatalog is not loaded; perks_info_hd cannot run")
            }
            GapCause::PerkNoMenu { name } => {
                write!(f, "MenuCatalog has no `{name}`")
            }
            GapCause::PerkTableMissing { name } => {
                write!(f, "string table `{name}` was not in the UI-zone walk")
            }
            GapCause::PerkMaterialMissing { name, miss } => {
                write!(f, "perk image `{name}` is {miss}")
            }
            GapCause::PerkEmptyPaint { name } => {
                write!(f, "`{name}` Item_Paint emitted no tess quads")
            }
            GapCause::CompassRingMaterialMissing { name, miss } => {
                write!(f, "compass ring image `{name}` is {miss}")
            }
            GapCause::CompassRingNoMenu { name } => {
                write!(f, "MenuCatalog has no `{name}`")
            }
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct HudPresentationGaps(GapLedger<GapCause, HUD_GAP_COUNT>);

impl HudPresentationGaps {
    pub fn raise(&mut self, cause: GapCause) {
        self.0.raise(cause);
    }

    pub fn clear(&mut self, gap: HudGap) {
        self.0.clear(gap);
    }

    pub fn is_live(&self, gap: HudGap) -> bool {
        self.0.is_live(gap)
    }

    pub fn cause(&self, gap: HudGap) -> Option<&GapCause> {
        self.0.cause(gap)
    }

    pub fn live(&self) -> impl Iterator<Item = HudGap> + '_ {
        self.0.live()
    }

    pub fn count(&self) -> usize {
        self.0.count()
    }
}

pub(crate) fn report_hud_gaps(gaps: Res<HudPresentationGaps>, mut last: Local<Option<String>>) {
    let signature = gap_signature(&gaps);
    if last.as_deref() == Some(signature.as_str()) {
        return;
    }
    let names: Vec<&str> = gaps.live().map(HudGap::name).collect();
    diag::info!(
        Ui,
        "hud: {} presentation gaps live: {}",
        gaps.count(),
        names.join(" ")
    );
    for gap in gaps.live() {
        if let Some(cause) = gaps.cause(gap) {
            diag::info!(Ui, "hud gap {}: {}", gap.name(), cause);
        }
    }
    *last = Some(signature);
}

fn gap_signature(gaps: &HudPresentationGaps) -> String {
    let mut signature = String::new();
    gaps.0
        .write_signature(&mut signature)
        .expect("writing into a String cannot fail");
    signature
}
