use super::{AssetLinkSink, always_array, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{Ptr, Result, XFILE_BLOCK_TEMP, XFILE_BLOCK_VIRTUAL, ZoneStream};

pub(super) fn load_menu_list(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::MENU_LIST)?;
    let count = s.i32_at(p, 4)?.max(0) as usize;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    if let Some(menus) = always_array(s, p.at(8), 4, 4 * count)? {
        for i in 0..count {
            load_menu_def_ptr(s, links, menus.at(i * 4))?;
        }
    }
    s.pop()
}

fn load_menu_def_ptr(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
) -> Result<()> {
    s.push(XFILE_BLOCK_TEMP)?;
    let (load, _insert) = s.begin_body_with_insert(slot)?;
    if !load {
        return s.pop();
    }
    let body = s.alloc_load(8, sz::MENU_DEF)?;
    s.fixup_slot(slot, body)?;
    let result = load_menu_def(s, links, body);
    s.pop()?;
    result
}

fn load_menu_def(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    s.push(XFILE_BLOCK_VIRTUAL)?;
    load_window(s, links, p)?;
    follow_name(s, p, 0xa4)?;
    if let Some(ev) = always_array(s, p.at(0x10c), 4, sz::GENERIC_EVENT_HANDLER)? {
        load_generic_event_handler(s, ev)?;
    }
    if let Some(key) = always_array(s, p.at(0x110), 4, sz::ITEM_KEY_HANDLER)? {
        load_item_key_handler(s, key)?;
    }
    load_expression_statement(s, p.at(0x114))?;
    follow_name(s, p, 0x138)?;
    follow_name(s, p, 0x13c)?;
    load_expression_statement(s, p.at(0x168))?;
    load_expression_statement(s, p.at(0x178))?;
    let item_count = s.i32_at(p, 0xb0)?.max(0) as usize;
    if let Some(items) = always_array(s, p.at(0x188), 4, 4 * item_count)? {
        for i in 0..item_count {
            load_item_def_ptr(s, links, items.at(i * 4))?;
        }
    }
    s.pop()
}

fn load_window(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    follow_name(s, p, 0)?;
    follow_name(s, p, 0x34)?;
    asset_ptr_at(s, links, AssetType::Material, p.at(0xa0))?;
    Ok(())
}

fn load_item_def_ptr(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let body = s.alloc_load(8, sz::ITEM_DEF)?;
    s.fixup_slot(slot, body)?;
    load_item_def(s, links, body)
}

fn load_item_def(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    load_window(s, links, p)?;
    let ty = s.i32_at(p, 0xa4)?;
    follow_name(s, p, 0xb0)?;
    follow_name(s, p, 0xb4)?;
    follow_name(s, p, 0xb8)?;
    load_item_def_data(s, links, p.at(0xc0), ty)?;
    if let Some(rect) = always_array(s, p.at(0xc8), 4, sz::RECT_DATA)? {
        load_rect_data(s, rect)?;
    }
    load_expression_statement(s, p.at(0xcc))?;
    load_expression_statement(s, p.at(0xf0))?;
    if let Some(ev) = always_array(s, p.at(0x104), 4, sz::GENERIC_EVENT_HANDLER)? {
        load_generic_event_handler(s, ev)?;
    }
    if let Some(anim) = always_array(s, p.at(0x108), 4, sz::UI_ANIM_INFO)? {
        load_ui_anim_info(s, anim)?;
    }
    Ok(())
}

fn load_item_def_data(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
    ty: i32,
) -> Result<()> {
    match ty {
        1 | 3 | 0xf | 0x12 | 0x14 | 4 | 0xa | 5 | 0xe | 7 | 0xd | 9 | 0xc | 0x10 | 8 | 0xb
        | 0x16 => load_text_def_ptr(s, links, slot, ty),
        2 => load_image_def_ptr(s, slot),
        0x15 | 0x13 => load_focus_item_def_ptr(s, links, slot, ty),
        6 => load_owner_draw_def_ptr(s, slot),
        _ => Ok(()),
    }
}

fn load_text_def_ptr(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
    item_ty: i32,
) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let p = s.alloc_load(4, sz::TEXT_DEF)?;
    s.fixup_slot(slot, p)?;
    follow_name(s, p, 0x38)?;
    if let Some(te) = always_array(s, p.at(0x3c), 4, sz::TEXT_EXP)? {
        load_expression_statement(s, te)?;
    }
    load_text_def_data(s, links, p.at(0x40), item_ty)
}

fn load_text_def_data(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
    item_ty: i32,
) -> Result<()> {
    match item_ty {
        3 | 4 | 21 | 20 | 10 | 5 | 13 | 7 | 14 | 30 | 9 | 12 | 16 | 8 | 11 | 22 => {
            load_focus_item_def_ptr(s, links, slot, item_ty)
        }
        15 => {
            if !super::always_alloc(s, slot)? {
                return Ok(());
            }
            let p = s.alloc_load(4, sz::GAME_MSG_DEF)?;
            s.fixup_slot(slot, p)?;
            Ok(())
        }
        _ => Ok(()),
    }
}

fn load_image_def_ptr(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let p = s.alloc_load(4, sz::IMAGE_DEF)?;
    s.fixup_slot(slot, p)?;
    load_expression_statement(s, p)
}

fn load_owner_draw_def_ptr(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let p = s.alloc_load(4, sz::OWNER_DRAW_DEF)?;
    s.fixup_slot(slot, p)?;
    load_expression_statement(s, p)
}

fn load_focus_item_def_ptr(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
    item_ty: i32,
) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let p = s.alloc_load(4, sz::FOCUS_ITEM_DEF)?;
    s.fixup_slot(slot, p)?;
    for off in [0, 4, 8, 12] {
        follow_name(s, p, off)?;
    }
    if let Some(key) = always_array(s, p.at(16), 4, sz::ITEM_KEY_HANDLER)? {
        load_item_key_handler(s, key)?;
    }
    load_focus_def_data(s, links, p.at(20), item_ty)
}

fn load_focus_def_data(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
    item_ty: i32,
) -> Result<()> {
    match item_ty {
        4 => load_list_box_def_ptr(s, links, slot),
        0xa => load_multi_def_ptr(s, slot),
        5 | 0xd | 7 | 0xe | 0x1e | 9 | 0xc | 0x10 | 8 | 0x16 => load_edit_field_def_ptr(s, slot),
        0xb => load_enum_dvar_def_ptr(s, slot),
        _ => Ok(()),
    }
}

fn load_list_box_def_ptr(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let p = s.alloc_load(4, sz::LIST_BOX_DEF)?;
    s.fixup_slot(slot, p)?;
    asset_ptr_at(s, links, AssetType::Material, p.at(0x280))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(0x284))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(0x288))?;
    let max_rows = s.i32_at(p, 0x294)?.max(0) as usize;
    if let Some(rows) = always_array(s, p.at(0x290), 4, sz::MENU_ROW * max_rows)? {
        let num_cols = s.i32_at(p, 0x1c)?.max(0) as usize;
        for i in 0..max_rows {
            load_menu_row(s, rows.at(i * sz::MENU_ROW), num_cols)?;
        }
    }
    Ok(())
}

fn load_menu_row(s: &mut ZoneStream<'_>, p: Ptr, num_cols: usize) -> Result<()> {
    if let Some(cells) = always_array(s, p.at(0), 4, sz::MENU_CELL * num_cols)? {
        for i in 0..num_cols {
            let c = cells.at(i * sz::MENU_CELL);
            let max_chars = s.i32_at(c, 4)?.max(0) as usize;
            always_array(s, c.at(8), 1, max_chars)?;
        }
    }
    always_array(s, p.at(4), 1, 32)?;
    always_array(s, p.at(8), 1, 32)?;
    Ok(())
}

fn load_multi_def_ptr(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let p = s.alloc_load(4, sz::MULTI_DEF)?;
    s.fixup_slot(slot, p)?;
    for i in 0..32 {
        follow_name(s, p, i * 4)?;
    }
    for i in 0..32 {
        follow_name(s, p, 0x80 + i * 4)?;
    }
    Ok(())
}

fn load_edit_field_def_ptr(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let p = s.alloc_load(4, sz::EDIT_FIELD_DEF)?;
    s.fixup_slot(slot, p)?;
    Ok(())
}

fn load_enum_dvar_def_ptr(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let p = s.alloc_load(4, sz::ENUM_DVAR_DEF)?;
    s.fixup_slot(slot, p)?;
    follow_name(s, p, 0)
}

fn load_rect_data(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    for off in [0, 16, 32, 48] {
        load_expression_statement(s, p.at(off))?;
    }
    Ok(())
}

fn load_ui_anim_info(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let count = s.i32_at(p, 0)?.max(0) as usize;
    if let Some(states) = always_array(s, p.at(4), 4, 4 * count)? {
        for i in 0..count {
            load_anim_params_ptr(s, states.at(i * 4))?;
        }
    }
    Ok(())
}

fn load_anim_params_ptr(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let p = s.alloc_load(4, sz::ANIM_PARAMS_DEF)?;
    s.fixup_slot(slot, p)?;
    follow_name(s, p, 0)?;
    if let Some(ev) = always_array(s, p.at(0x68), 4, sz::GENERIC_EVENT_HANDLER)? {
        load_generic_event_handler(s, ev)?;
    }
    Ok(())
}

fn load_generic_event_handler(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    follow_name(s, p, 0)?;
    if let Some(script) = always_array(s, p.at(4), 4, sz::GENERIC_EVENT_SCRIPT)? {
        load_generic_event_script(s, script)?;
    }
    if let Some(next) = always_array(s, p.at(8), 4, sz::GENERIC_EVENT_HANDLER)? {
        load_generic_event_handler(s, next)?;
    }
    Ok(())
}

fn load_item_key_handler(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    if let Some(script) = always_array(s, p.at(4), 4, sz::GENERIC_EVENT_SCRIPT)? {
        load_generic_event_script(s, script)?;
    }
    if let Some(next) = always_array(s, p.at(8), 4, sz::ITEM_KEY_HANDLER)? {
        load_item_key_handler(s, next)?;
    }
    Ok(())
}

fn load_generic_event_script(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    if let Some(pre) = always_array(s, p.at(0), 4, sz::SCRIPT_CONDITION)? {
        load_script_condition(s, pre)?;
    }
    load_expression_statement(s, p.at(4))?;
    follow_name(s, p, 0x1c)?;
    if let Some(next) = always_array(s, p.at(0x28), 4, sz::GENERIC_EVENT_SCRIPT)? {
        load_generic_event_script(s, next)?;
    }
    Ok(())
}

fn load_script_condition(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    if let Some(next) = always_array(s, p.at(0xc), 4, sz::SCRIPT_CONDITION)? {
        load_script_condition(s, next)?;
    }
    Ok(())
}

fn load_expression_statement(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    follow_name(s, p, 0)?;
    let num = s.i32_at(p, 8)?.max(0) as usize;
    if s.begin_body(p.at(12))? {
        let arr = s.alloc_load(4, sz::EXPRESSION_RPN * num)?;
        for i in 0..num {
            load_expression_rpn(s, arr.at(i * sz::EXPRESSION_RPN))?;
        }
    }
    Ok(())
}

fn load_expression_rpn(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let ty = s.i32_at(p, 0)?;
    if ty == 0 {
        let data_ty = s.i32_at(p, 4)?;
        if data_ty == 2 {
            follow_name(s, p, 8)?;
        }
    }
    Ok(())
}
