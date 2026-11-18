pub use asset_game::SingleplayerMode;
use asset_game::{MenuCatalog, MenuEvent, MenuHandlers};
use frame::UiMenuDvars;

pub const PAGE_SIZE: usize = 10;

pub fn modes() -> &'static [SingleplayerMode] {
    asset_game::singleplayer_modes()
}

fn string_dvar(name: &str) -> String {
    let encoded: String = name.bytes().map(|byte| format!("{byte:02x}")).collect();
    format!("op 38 s:{encoded} op 1")
}

fn number_dvar(name: &str) -> String {
    let encoded: String = name.bytes().map(|byte| format!("{byte:02x}")).collect();
    format!("op 37 s:{encoded} op 1")
}

fn script(text: impl Into<String>) -> Vec<MenuEvent> {
    vec![MenuEvent::Script(text.into())]
}

pub(crate) fn install(catalog: &mut MenuCatalog) -> Result<(), String> {
    let main = catalog
        .menus
        .get_mut("iw4l_main")
        .ok_or("Missing main menu")?;
    let template = main
        .items
        .iter()
        .find(|item| item.name == "create_game")
        .cloned()
        .ok_or("Missing main menu button")?;
    let mut link = template.clone();
    link.name = "singleplayer".into();
    link.text_key = "SINGLEPLAYER (WIP)".into();
    link.text_literal = true;
    link.handlers.action = script("play mouse_click; open singleplayer;");
    for item in &mut main.items {
        if item.rect.vert_align == 1 {
            item.rect.y += 20.0;
        }
    }
    let first = main
        .items
        .iter()
        .position(|item| item.name == "create_game")
        .unwrap_or(0);
    main.items.insert(first, link);
    let mut menu = catalog
        .get("game_map_select")
        .cloned()
        .ok_or("Missing map selection menu")?;
    menu.name = "singleplayer".into();
    menu.handlers = MenuHandlers {
        open: script("exec ui_singleplayer; focusFirst;"),
        esc: script("close self;"),
        ..Default::default()
    };
    let mission_template = menu
        .items
        .iter()
        .find(|item| item.name == "map_row_0")
        .cloned()
        .ok_or("Missing map selection row")?;
    for item in &mut menu.items {
        if let Some(row) = item
            .name
            .strip_prefix("map_row_")
            .and_then(|row| row.parse::<usize>().ok())
        {
            item.name = format!("sp_mission_{row}");
            item.text_exp = string_dvar(&format!("ui_sp_mission_{row}"));
            item.text_literal = true;
            item.vis_exp = format!("{} op 13 s:", item.text_exp);
            item.disabled_exp = "0".into();
            item.handlers.action = vec![MenuEvent::If {
                condition: format!("{} op 12 s:31", string_dvar("ui_sp_available")),
                then: script(format!("play mouse_click; exec \"ui_sp_start {row}\";")),
            }];
            item.handlers.focus = script(format!(
                "play mouse_over; setItemColor self backcolor 0 0 0 1; exec \"ui_sp_hover {row}\";"
            ));
            item.float_exp = [4, 5, 6]
                .into_iter()
                .map(|target| (target, number_dvar("ui_sp_row_tint")))
                .collect();
        } else if let Some(index) = item
            .name
            .strip_prefix("map_pack_")
            .and_then(|index| index.parse::<usize>().ok())
        {
            item.name = format!("sp_mode_{index}");
            item.text_literal = true;
            item.text_exp = string_dvar(&format!("ui_sp_mode_{index}"));
            item.vis_exp = format!("{} op 13 s:", item.text_exp);
            item.handlers.action =
                script(format!("play mouse_click; exec \"ui_sp_mode {index}\";"));
            item.text_scale = 0.30;
            item.float_exp = [4, 5, 6]
                .into_iter()
                .map(|target| (target, number_dvar(&format!("ui_sp_tint_{index}"))))
                .collect();
        } else if item.text_key.contains("CHOOSE_MAP") || item.text_key.contains("SELECT_MAP") {
            item.text_key = "SINGLEPLAYER (WIP)".into();
            item.text_literal = true;
        }
    }
    let preview = menu.items.get_mut(35).ok_or("Missing map preview")?;
    preview.material_exp = string_dvar("ui_sp_preview");
    preview.background.clear();
    let preview_rect = preview.rect;
    let title = menu.items.get_mut(36).ok_or("Missing map preview title")?;
    title.text_exp = string_dvar("ui_sp_preview_title");
    title.text_literal = true;
    let mut no_preview = title.clone();
    no_preview.name = "sp_no_preview".into();
    no_preview.text_key = "PREVIEW UNAVAILABLE".into();
    no_preview.text_exp.clear();
    no_preview.rect = preview_rect;
    no_preview.rect.y += 48.0;
    no_preview.text_scale = 0.3;
    no_preview.vis_exp = format!("{} op 12 s:", string_dvar("ui_sp_preview"));
    menu.items.push(no_preview);
    let status = menu.items.get_mut(37).ok_or("Missing map preview status")?;
    status.text_exp = string_dvar("ui_sp_hint");
    status.text_literal = true;
    for (name, label, x, command, enabled) in [
        ("sp_previous", "<", 140.0, "ui_sp_page -1", "ui_sp_previous"),
        ("sp_next", ">", 308.0, "ui_sp_page 1", "ui_sp_next"),
    ] {
        let mut item = mission_template.clone();
        item.name = name.into();
        item.text_key = label.into();
        item.text_literal = true;
        item.text_exp.clear();
        item.vis_exp = "1".into();
        item.disabled_exp = format!("{} op 12 s:30", string_dvar(enabled));
        item.rect.x = x;
        item.rect.y = 248.0;
        item.rect.w = 32.0;
        item.text_align_mode = 8;
        item.text_align_x = 8.0;
        item.handlers.action = script(format!("play mouse_click; exec \"{command}\";"));
        item.handlers.focus = script("play mouse_over; setItemColor self backcolor 0 0 0 1;");
        menu.items.push(item);
    }
    let mut campaign = mission_template.clone();
    campaign.name = "sp_campaign_menu".into();
    campaign.text_key = "MW2 CAMPAIGN".into();
    campaign.text_literal = true;
    campaign.text_exp.clear();
    campaign.vis_exp = "1".into();
    campaign.disabled_exp = "0".into();
    campaign.rect.x = 140.0;
    campaign.rect.y = 270.0;
    campaign.handlers.action =
        script("play mouse_click; close self; close iw4l_main; open iw4l_campaign;");
    campaign.handlers.focus = script("play mouse_over; setItemColor self backcolor 0 0 0 1;");
    menu.items.push(campaign);
    let mut page = mission_template;
    page.name = "sp_page".into();
    page.text_exp = string_dvar("ui_sp_page_label");
    page.vis_exp = "1".into();
    page.disabled_exp = "0".into();
    page.rect.x = 204.0;
    page.rect.y = 248.0;
    page.rect.w = 72.0;
    page.item_type = 0;
    page.static_flags = 0x100000;
    page.background.clear();
    page.handlers = Default::default();
    menu.items.push(page);
    catalog.menus.insert(menu.name.clone(), menu);
    Ok(())
}

pub fn sync(dvars: &mut UiMenuDvars, mode: usize, page: usize, hover: usize) {
    let modes = modes();
    let selected = &modes[mode];
    let entry = page * PAGE_SIZE + hover;
    dvars.set(
        "ui_sp_preview_title",
        selected
            .entries
            .get(entry)
            .copied()
            .unwrap_or("")
            .to_uppercase(),
    );
    dvars.set(
        "ui_sp_preview",
        selected.previews.get(entry).copied().unwrap_or(""),
    );
    dvars.set(
        "ui_sp_row_tint",
        if selected.available() { "1" } else { "0.55" },
    );
    dvars.set(
        "ui_sp_available",
        if selected.available() { "1" } else { "0" },
    );
    dvars.set("ui_sp_previous", if page > 0 { "1" } else { "0" });
    dvars.set(
        "ui_sp_next",
        if page + 1 < selected.pages(PAGE_SIZE) {
            "1"
        } else {
            "0"
        },
    );
    dvars.set(
        "ui_sp_page_label",
        format!("{} / {}", page + 1, selected.pages(PAGE_SIZE)),
    );
    dvars.set(
        "ui_sp_hint",
        if selected.available() {
            "CAMPAIGN (WIP)"
        } else {
            "UNAVAILABLE"
        },
    );
    for row in 0..PAGE_SIZE {
        dvars.set(
            &format!("ui_sp_mission_{row}"),
            selected
                .entries
                .get(page * PAGE_SIZE + row)
                .copied()
                .map(|label| {
                    let label = label.to_uppercase();
                    if label.chars().count() > 26 {
                        format!("{}...", label.chars().take(23).collect::<String>())
                    } else {
                        label
                    }
                })
                .unwrap_or_else(String::new),
        );
    }
    for index in 0..16 {
        let label = modes.get(index).map_or(String::new(), |mode| {
            let label = mode.button_label.unwrap_or(mode.label).to_uppercase();
            if mode.available() {
                format!("{label} (WIP)")
            } else {
                label
            }
        });
        dvars.set(&format!("ui_sp_mode_{index}"), label);
        dvars.set(
            &format!("ui_sp_tint_{index}"),
            if index == mode { "1" } else { "0.6" },
        );
    }
}
