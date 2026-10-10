//! Black Ops' own HUD in a Black Ops session: the menus of its HUD menu lists
//! (`weaponinfo_zombie`, `dpad_zombie`, `competitivemodescores`, ...) drawn
//! through the menu engine, each shown by its own visibility expression.
//!
//! Owner draws paint only what their Black Ops name states (the clip count of
//! `CG_PLAYER_WEAPON_AMMO_CLIP` in the item's font, ...); the ones whose draw
//! rule lives only in the executable report a gap and draw nothing
//! (docs/fidelity/t5.md, "Waiting on the maintainer").

use asset_game::{LocalizeCatalog, MenuCatalog, T5_HUD_MENU_LISTS};
use assets::BoundWeapons;
use hud_iw4::{ExprError, ExprHost, Operand};
use playerstate_iw4::{PM_TYPE_DEAD, PlayerState};

use crate::ammo::{WeaponbarAmmo, offhand_weapon_index};
use crate::chrome::{
    ChromeAssets, ChromeFrame, ChromeGapKind, ChromeMenuAnim, MenuVisOnError, OwnerDrawArgs,
    OwnerDrawPaint, execute_chrome_menu_ex, push_owner_text,
};
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance};

const CG_PLAYER_HEAT_VALUE: i32 = 7;
const CG_PLAYER_WEAPON_NAME: i32 = 81;
const CG_OFFHAND_WEAPON_ICON_FRAG: i32 = 103;
const CG_OFFHAND_WEAPON_ICON_SMOKEFLASH: i32 = 104;
const CG_PLAYER_WEAPON_BACKGROUND: i32 = 116;
const CG_PLAYER_WEAPON_AMMO_CLIP: i32 = 120;
const CG_PLAYER_WEAPON_AMMO_CLIP_DUAL_WIELD: i32 = 121;
const CG_PLAYER_WEAPON_AMMO_STOCK: i32 = 122;
const CG_PLAYER_ACTIONSLOT_1: i32 = 166;
const CG_PLAYER_ACTIONSLOT_4: i32 = 169;

/// What the local client knows that Black Ops' HUD expressions ask for.
pub(crate) struct T5HudState<'a> {
    pub ms: i32,
    pub dvars: sim::ScriptDvars<'a>,
    pub ui_dvars: Option<&'a frame::UiMenuDvars>,
    pub zombies: bool,
    pub input: Option<&'a frame::HudInputView>,
    pub vis: crate::weaponbar::HudPlayerVis,
    pub in_killcam: bool,
    pub ps: &'a PlayerState,
    pub weapons: Option<&'a BoundWeapons<'a>>,
    pub ammo: Option<WeaponbarAmmo>,
    pub weapon_name: Option<String>,
    pub hide_ammo: bool,
    pub ammo_widths: [i32; 2],
}

impl T5HudState<'_> {
    fn dvar(&self, name: &str) -> Option<String> {
        if let Some(value) = self.dvars.string(name) {
            return Some(value.to_owned());
        }
        if let Some(value) = self.ui_dvars.and_then(|d| d.get(name)) {
            return Some(value.to_owned());
        }
        let flag = |on: bool| String::from(if on { "1" } else { "0" });
        match name.to_ascii_lowercase().as_str() {
            "zombiemode" => Some(flag(self.zombies)),
            "zombietron" => Some(flag(
                self.dvars
                    .string("mapname")
                    .is_some_and(|map| map.eq_ignore_ascii_case("zombietron")),
            )),
            // One local player per process, and the runtime never pauses.
            "splitscreen" | "cl_paused" => Some(flag(false)),
            "ui_ammo_stock_width" => Some(self.ammo_widths[0].to_string()),
            "ui_right_ammo_width" => Some(self.ammo_widths[1].to_string()),
            _ => None,
        }
    }
}

/// A dvar nothing in the session registered reads as unset (`0`, `""`), as
/// the menu engine reads a missing dvar; each name is logged once so the
/// ledger can list them against Black Ops' defaults.
fn unset_dvar(name: &str) -> String {
    static SEEN: std::sync::Mutex<std::collections::BTreeSet<String>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());
    let mut seen = SEEN.lock().unwrap_or_else(|poison| poison.into_inner());
    if seen.insert(name.to_ascii_lowercase()) {
        diag::info!(
            Ui,
            "t5 hud: dvar `{name}` is not registered; reads as unset"
        );
    }
    String::new()
}

struct T5HudHost<'a> {
    state: &'a T5HudState<'a>,
}

impl ExprHost for T5HudHost<'_> {
    fn binding_label(&self, command: &str) -> Option<&str> {
        self.state
            .input?
            .binding_keys
            .get(command)
            .map(String::as_str)
    }
    fn milliseconds(&self) -> i32 {
        self.state.ms
    }
    fn static_dvar_int(&self, _index: i32) -> Result<i32, ExprError> {
        Err(ExprError::Host("static dvar"))
    }
    fn team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("team field"))
    }
    fn player_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("player field"))
    }
    fn other_team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("other team field"))
    }
    fn local_var_string(&self, _name: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("local var"))
    }
    fn time_left(&self) -> Result<i32, ExprError> {
        Err(ExprError::Host("timeleft"))
    }
    fn score_at_rank(&self, _rank: i32) -> Result<i32, ExprError> {
        Err(ExprError::Host("score"))
    }
    fn gametype_name(&self) -> Result<Operand, ExprError> {
        Err(ExprError::Host("gametype"))
    }
    fn weapon_lock(&self) -> Result<hud_iw4::WeaponLockView, ExprError> {
        Ok(hud_iw4::WeaponLockView {
            ads_javelin: self.state.vis.ads_javelin,
            ..hud_iw4::WeaponLockView::default()
        })
    }
    fn dvar_int(&self, name: &str) -> Result<i32, ExprError> {
        let value = self.state.dvar(name).unwrap_or_else(|| unset_dvar(name));
        let value = value.trim();
        Ok(value
            .parse::<i32>()
            .ok()
            .or_else(|| value.parse::<f32>().ok().map(|v| v as i32))
            .unwrap_or(0))
    }
    fn dvar_string(&self, name: &str) -> Result<String, ExprError> {
        Ok(self.state.dvar(name).unwrap_or_else(|| unset_dvar(name)))
    }
    fn ui_active(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.state.vis.ui_active))
    }
    fn flashbanged(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.state.vis.flashbanged))
    }
    fn in_killcam(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.state.in_killcam))
    }
    fn key_binding(&self, command: &str) -> Result<Operand, ExprError> {
        let slot = command
            .strip_prefix("+actionslot ")
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|i| i.checked_sub(1))
            .ok_or(ExprError::Host("keybinding"))?;
        let key = self
            .state
            .input
            .and_then(|input| input.action_slot_keys.get(slot))
            .and_then(|key| key.as_deref())
            .unwrap_or("");
        Ok(Operand::Str(key.to_owned()))
    }
    fn is_dual_wield(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.state.ammo.as_ref().is_some_and(|a| a.dual)))
    }
    fn is_fuel_weapon(&self) -> Result<i32, ExprError> {
        let weapon = weapon_iw4::get_viewmodel_weapon_index(self.state.ps);
        Ok(i32::from(
            self.state
                .weapons
                .and_then(|w| w.row(weapon))
                .and_then(|w| w.hud_facts())
                .is_some_and(|facts| facts.fuel_tank),
        ))
    }
}

/// The left-to-right widths `weaponinfo_zombie` positions the clip with, in
/// the fonts of its `ammoStock` and `clip` items.
pub(crate) fn ammo_widths(
    catalog: &MenuCatalog,
    stock: Option<i32>,
    clip: Option<i32>,
) -> [i32; 2] {
    let width = |item_name: &str, value: Option<i32>| {
        let Some(value) = value else {
            return 0;
        };
        catalog
            .get("weaponinfo_zombie")
            .and_then(|menu| menu.items.iter().find(|item| item.name == item_name))
            .and_then(|item| {
                let assets = crate::chrome::ChromeAssets {
                    catalog: Some(catalog),
                    localize: None,
                };
                let font = assets.font_name(item.font_enum, 1.0, item.text_scale)?;
                Some(crate::chrome::layout_text_width(
                    assets.layout()?,
                    catalog.font(font)?,
                    &value.to_string(),
                    item.text_scale,
                ) as i32)
            })
            .unwrap_or(0)
    };
    [width("ammoStock", stock), width("clip", clip)]
}

fn background_key(raw: &str) -> Option<(asset_core::AssetNamespace, String)> {
    let key = asset_core::AssetKey::parse(raw).ok()?;
    (key.kind == asset_core::AssetKind::Material).then(|| (key.namespace, key.name))
}

/// A material stretched over the owner draw's rect; negative sizes mirror it.
fn push_pic(
    args: &OwnerDrawArgs<'_>,
    namespace: asset_core::AssetNamespace,
    material: String,
    frame: &mut ChromeFrame,
) {
    let rect = args.rect;
    if rect.w.abs() <= f32::EPSILON || rect.h.abs() <= f32::EPSILON {
        return;
    }
    let applied = args.surface.apply_rect(
        rect.x,
        rect.y,
        rect.w.abs(),
        rect.h.abs(),
        rect.horz_align as i32,
        rect.vert_align as i32,
    );
    let (s0, s1) = if rect.w < 0.0 { (1.0, 0.0) } else { (0.0, 1.0) };
    let (t0, t1) = if rect.h < 0.0 { (1.0, 0.0) } else { (0.0, 1.0) };
    frame.list.cmds.push(Draw2dCmd {
        material_namespace: namespace,
        x: applied.x,
        y: applied.y,
        w: applied.w,
        h: applied.h,
        s0,
        t0,
        s1,
        t1,
        color: args.color,
        material,
        op: Draw2dOp::StretchPic,
        provenance: Draw2dProvenance::OwnerDraw(args.item.owner_draw),
        layer: 1,
    });
}

fn text(args: &OwnerDrawArgs<'_>, value: &str, frame: &mut ChromeFrame) -> OwnerDrawPaint {
    match push_owner_text(args, value, args.color, frame) {
        Ok(()) => OwnerDrawPaint::Painted,
        Err(kind) => OwnerDrawPaint::Gap(kind),
    }
}

fn paint_owner(
    state: &T5HudState<'_>,
    args: OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let ammo = state.ammo.as_ref().filter(|_| !state.hide_ammo);
    match args.item.owner_draw {
        CG_PLAYER_WEAPON_BACKGROUND => {
            if let Some((namespace, material)) = background_key(&args.item.background) {
                push_pic(&args, namespace, material, frame);
            }
            OwnerDrawPaint::Painted
        }
        CG_PLAYER_WEAPON_AMMO_CLIP => match ammo.and_then(|a| a.clip) {
            Some(clip) => text(&args, &clip.to_string(), frame),
            None => OwnerDrawPaint::Painted,
        },
        CG_PLAYER_WEAPON_AMMO_CLIP_DUAL_WIELD => match ammo.and_then(|a| a.clip_alt) {
            Some(clip) => text(&args, &clip.to_string(), frame),
            None => OwnerDrawPaint::Painted,
        },
        CG_PLAYER_WEAPON_AMMO_STOCK => match ammo.and_then(|a| a.stock) {
            Some(stock) => text(&args, &stock.to_string(), frame),
            None => OwnerDrawPaint::Painted,
        },
        CG_PLAYER_WEAPON_NAME => match state.weapon_name.as_deref() {
            Some(name) if !state.hide_ammo => text(&args, name, frame),
            _ => OwnerDrawPaint::Painted,
        },
        CG_OFFHAND_WEAPON_ICON_FRAG | CG_OFFHAND_WEAPON_ICON_SMOKEFLASH => {
            if state.ps.pm_type >= PM_TYPE_DEAD || state.hide_ammo {
                return OwnerDrawPaint::Painted;
            }
            let Some(weapons) = state.weapons else {
                return OwnerDrawPaint::Painted;
            };
            let class = if args.item.owner_draw == CG_OFFHAND_WEAPON_ICON_FRAG {
                state.ps.offhand_primary
            } else {
                state.ps.offhand_secondary
            };
            let Some(index) = offhand_weapon_index(state.ps, weapons, class) else {
                return OwnerDrawPaint::Painted;
            };
            let registry = weapons.registry();
            match (
                registry.hud_icon_image_of(index),
                registry.hud_icon_namespace_of(index),
            ) {
                (Some(image), Some(namespace)) => {
                    push_pic(&args, namespace, image.to_owned(), frame);
                    OwnerDrawPaint::Painted
                }
                _ => OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp),
            }
        }
        slot @ CG_PLAYER_ACTIONSLOT_1..=CG_PLAYER_ACTIONSLOT_4 => {
            let slot = slot - CG_PLAYER_ACTIONSLOT_1;
            let Some(weapon) = crate::weaponbar::action_slot_weapon(state.ps, slot, state.weapons)
            else {
                return OwnerDrawPaint::Painted;
            };
            let Some(weapons) = state.weapons else {
                return OwnerDrawPaint::Painted;
            };
            let registry = weapons.registry();
            let Some((material, _)) = registry.dpad_icon_of(weapon) else {
                return OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp);
            };
            let Some(namespace) =
                registry.component_namespace_of(weapon, asset_game::WeaponComponent::Material)
            else {
                return OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp);
            };
            push_pic(&args, namespace, material.to_owned(), frame);
            OwnerDrawPaint::Painted
        }
        CG_PLAYER_HEAT_VALUE => OwnerDrawPaint::Gap(ChromeGapKind::OwnerDraw),
        _ => OwnerDrawPaint::Gap(ChromeGapKind::OwnerDraw),
    }
}

/// Every menu of Black Ops' HUD lists, in list order, through the menu
/// engine. Returns the draw list and the first failure per menu.
pub(crate) fn paint(
    catalog: &MenuCatalog,
    localize: Option<&LocalizeCatalog>,
    state: &T5HudState<'_>,
    surface: &crate::surface::Hud2dSurface,
    exprs: &mut crate::expr_cache::MenuExprCache,
) -> (Draw2dList, Vec<String>) {
    let host = T5HudHost { state };
    let mut list = Draw2dList::default();
    let mut errors = Vec::new();
    let mut drawn = std::collections::BTreeSet::new();
    for menu in T5_HUD_MENU_LISTS
        .iter()
        .filter_map(|name| catalog.list_menus.get(*name))
        .flatten()
    {
        if !drawn.insert(menu.to_ascii_lowercase()) {
            continue;
        }
        let Some(menu) = catalog.get(menu) else {
            continue;
        };
        let mut hook =
            |args: OwnerDrawArgs<'_>, frame: &mut ChromeFrame| paint_owner(state, args, frame);
        let frame = execute_chrome_menu_ex(
            menu,
            &host,
            surface,
            ChromeAssets {
                catalog: Some(catalog),
                localize,
            },
            ChromeMenuAnim::IDENTITY,
            exprs,
            MenuVisOnError::HideAll,
            Some(&mut hook),
        );
        if let Some((item, error)) = frame.vis_errors.first() {
            errors.push(format!("{} item {item}: {error}", menu.name));
        } else if let Some((item, kind)) = frame.coverage.gap_ids.first() {
            errors.push(format!("{} item {item}: {kind:?}", menu.name));
        }
        list.cmds.extend(frame.list.cmds);
    }
    (list, errors)
}
