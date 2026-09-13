use crate::UiLayer;
use crate::model::{Content, Enabled, Modality, Screen, ScreenCmd, Style, Widget, screen_cmds_all};
use assets::{ITEM_TYPE_BUTTON, ITEM_TYPE_TEXT, MenuCatalog, MenuDef, MenuItem};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnsupportedReason {
    OwnerDraw(i32),
    ItemType(i32),
    BranchedOpen,
    VisibleExp,
}

impl UnsupportedReason {
    pub fn as_str(&self) -> String {
        match self {
            Self::OwnerDraw(n) => format!("ownerDraw={n}"),
            Self::ItemType(n) => format!("type={n}"),
            Self::BranchedOpen => "action has two open branches".into(),
            Self::VisibleExp => "visibleExp outside imported templates".into(),
        }
    }
}

pub fn import_open(catalog: &MenuCatalog, name: &str) -> Option<Screen> {
    if name == "main" {
        return import_main(catalog);
    }
    catalog
        .get(name)
        .map(|menu| import_menu(menu, expr_dvars(catalog, menu)))
}

pub fn import_main(catalog: &MenuCatalog) -> Option<Screen> {
    let main = catalog.get("main")?;
    let mut screen = import_menu(main, expr_dvars(catalog, main));
    let mut opened = Vec::new();
    screen.on_open.retain(|cmd| match cmd {
        ScreenCmd::Open(id) => {
            opened.push(id.clone());
            false
        }
        _ => true,
    });
    for id in opened {
        let Some(child) = catalog.get(&id) else {
            continue;
        };
        let child_screen = import_menu(child, expr_dvars(catalog, child));
        screen.widgets.extend(child_screen.widgets);
    }
    Some(screen)
}

pub fn import_menu(menu: &MenuDef, expr_dvars: &str) -> Screen {
    let background = {
        let stem = assets::AssetRef::bare_name(&menu.window_background);
        if stem.is_empty() {
            None
        } else {
            Some(stem.to_owned())
        }
    };
    let bed = if menu.sound_name.is_empty() {
        None
    } else {
        Some(menu.sound_name.clone())
    };
    let (on_open, _) = screen_cmds_all(&menu.on_open);
    let (on_back, _) = screen_cmds_all(&menu.on_esc);
    let mut widgets = Vec::with_capacity(menu.items.len());
    for (index, item) in menu.items.iter().enumerate() {
        widgets.push(import_item(&menu.name, index, item, expr_dvars));
    }
    Screen {
        id: menu.name.clone(),
        layer: UiLayer::Shell,
        modality: if menu.fullscreen != 0 {
            Modality::Opaque
        } else {
            Modality::Overlay
        },
        background,
        bed,
        widgets,
        focus_overrides: Vec::new(),
        on_open,
        on_back,
    }
}

fn import_item(menu: &str, index: usize, item: &MenuItem, expr_dvars: &str) -> Widget {
    let id = if item.name.is_empty() {
        format!("{menu}[{index}]")
    } else {
        format!("{menu}/{}", item.name)
    };
    let (on_activate, _) = screen_cmds_all(&item.action);
    let (mut on_focus, _) = screen_cmds_all(&item.mouse_enter);
    let (from_on_focus, _) = screen_cmds_all(&item.on_focus);
    on_focus.extend(from_on_focus);
    if !item.focus_sound.is_empty() {
        on_focus.push(ScreenCmd::PlaySound(item.focus_sound.clone()));
    }
    let enabled = enabled_of(item, expr_dvars);
    let reason = first_reason(item, &on_activate, &enabled);
    let on_activate = match &reason {
        Some(UnsupportedReason::BranchedOpen) => on_activate
            .into_iter()
            .filter(|cmd| !matches!(cmd, ScreenCmd::Open(_)))
            .collect(),
        _ => on_activate,
    };
    let content = match reason {
        Some(reason) => Content::Unsupported {
            reason: reason.as_str(),
        },
        None if item.item_type == ITEM_TYPE_BUTTON => Content::Button,
        None if !item.text_key.is_empty() => Content::Label,
        None if !assets::AssetRef::bare_name(&item.background).is_empty() => Content::Image,
        None => Content::Panel,
    };
    let focusable = item.item_type == ITEM_TYPE_BUTTON && !item.text_key.is_empty();
    Widget {
        id,
        rect: item.rect.into(),
        style: Style {
            canvas: Default::default(),
            image_contain: false,
            text_wrap: false,
            fore_color: item.fore_color,
            text_scale: item.text_scale,
            font_enum: item.font_enum,
            text_align_mode: item.text_align_mode,
            text_align_x: item.text_align_x,
            text_align_y: item.text_align_y,
            background: assets::AssetRef::bare_name(&item.background).to_owned(),
            text_key: item.text_key.clone(),
            animation: Default::default(),
        },
        content,
        focusable,
        enabled,
        on_focus,
        on_activate,
        icon: String::new(),
        help: None,
        focus_order: None,
    }
}

fn enabled_of(item: &MenuItem, expr_dvars: &str) -> Enabled {
    if item.vis_exp.is_empty() {
        return Enabled::Always;
    }
    match parse_vis_exp(&item.vis_exp) {
        VisPattern::StaticDvarEq { index, value } => {
            if static_dvar_at(expr_dvars, index) == Some("gameMode") {
                Enabled::Bound {
                    dvar: "gameMode".into(),
                    value: value.to_owned(),
                }
            } else {
                Enabled::Always
            }
        }
        VisPattern::AnyNewMapPacks | VisPattern::Unknown => Enabled::Always,
    }
}

fn first_reason(
    item: &MenuItem,
    on_activate: &[ScreenCmd],
    enabled: &Enabled,
) -> Option<UnsupportedReason> {
    if item.owner_draw != 0 {
        return Some(UnsupportedReason::OwnerDraw(item.owner_draw));
    }
    if item.item_type != ITEM_TYPE_TEXT && item.item_type != ITEM_TYPE_BUTTON {
        return Some(UnsupportedReason::ItemType(item.item_type));
    }
    let open_n = on_activate
        .iter()
        .filter(|cmd| matches!(cmd, ScreenCmd::Open(_)))
        .count();
    if open_n >= 2 {
        return Some(UnsupportedReason::BranchedOpen);
    }
    if !item.vis_exp.is_empty() && !matches!(enabled, Enabled::Bound { .. }) {
        return Some(UnsupportedReason::VisibleExp);
    }
    None
}

fn expr_dvars<'a>(catalog: &'a MenuCatalog, menu: &'a MenuDef) -> &'a str {
    if !menu.expr_dvars.is_empty() {
        return menu.expr_dvars.as_str();
    }
    catalog
        .get("main")
        .map(|def| def.expr_dvars.as_str())
        .unwrap_or("")
}

#[derive(Debug, PartialEq, Eq)]
enum VisPattern<'a> {
    StaticDvarEq { index: i32, value: &'a str },
    AnyNewMapPacks,
    Unknown,
}

fn parse_vis_exp(dump: &str) -> VisPattern<'_> {
    let tok: Vec<&str> = dump.split_whitespace().collect();
    if tok.len() == 10
        && tok[0] == "op"
        && tok[1] == "16"
        && tok[2] == "op"
        && tok[3] == "26"
        && tok[5] == "op"
        && tok[6] == "1"
        && tok[7] == "op"
        && tok[8] == "12"
    {
        if let Ok(index) = tok[4].parse() {
            return VisPattern::StaticDvarEq {
                index,
                value: tok[9],
            };
        }
    }
    if tok == ["op", "16", "op", "102", "op", "1"] {
        return VisPattern::AnyNewMapPacks;
    }
    VisPattern::Unknown
}

fn static_dvar_at(dump: &str, index: i32) -> Option<&str> {
    let tok: Vec<&str> = dump.split_whitespace().collect();
    let mut i = 0;
    while i + 2 < tok.len() {
        if tok[i] == "d" {
            if tok[i + 1].parse::<i32>().ok() == Some(index) {
                return Some(tok[i + 2]);
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    None
}
