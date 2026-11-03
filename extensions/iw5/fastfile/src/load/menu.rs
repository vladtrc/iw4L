use super::{AssetLinkSink, always_alloc, always_array, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{Ptr, Result, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

const EVENT_UNCONDITIONAL: u8 = 0;
const EVENT_IF: u8 = 1;
const EVENT_ELSE: u8 = 2;
const EVENT_SET_LOCAL_VAR_BOOL: u8 = 3;
const EVENT_SET_LOCAL_VAR_INT: u8 = 4;
const EVENT_SET_LOCAL_VAR_FLOAT: u8 = 5;
const EVENT_SET_LOCAL_VAR_STRING: u8 = 6;

const EET_OPERAND: i32 = 1;
const VAL_STRING: i32 = 2;
const VAL_FUNCTION: i32 = 3;

const ITEM_TYPE_TEXT: i32 = 0;
const ITEM_TYPE_EDITFIELD: i32 = 4;
const ITEM_TYPE_LISTBOX: i32 = 6;
const ITEM_TYPE_NUMERICFIELD: i32 = 9;
const ITEM_TYPE_SLIDER: i32 = 10;
const ITEM_TYPE_YESNO: i32 = 11;
const ITEM_TYPE_MULTI: i32 = 12;
const ITEM_TYPE_DVARENUM: i32 = 13;
const ITEM_TYPE_BIND: i32 = 14;
const ITEM_TYPE_VALIDFILEFIELD: i32 = 16;
const ITEM_TYPE_DECIMALFIELD: i32 = 17;
const ITEM_TYPE_UPREDITFIELD: i32 = 18;
const ITEM_TYPE_NEWS_TICKER: i32 = 20;
const ITEM_TYPE_TEXT_SCROLL: i32 = 21;
const ITEM_TYPE_EMAILFIELD: i32 = 22;
const ITEM_TYPE_PASSWORDFIELD: i32 = 23;

pub(super) fn load_menu_list(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let width = s.pointer_bytes();
    let p = s.alloc_load(4, s.layout(sz::MENU_LIST, 24))?;
    let count = s.i32_at(p, s.layout(4, 8))?.max(0) as usize;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    if let Some(menus) = always_array(s, p.at(s.layout(8, 16)), 4, width * count)? {
        for i in 0..count {
            asset_ptr_at(s, links, AssetType::Menu, menus.at(i * width))?;
        }
    }
    s.pop()
}

pub(super) fn load_menu_def_asset(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::MENU_DEF, 200))?;
    load_menu_def(s, links, p)
}

fn load_menu_def(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    s.push(XFILE_BLOCK_VIRTUAL)?;
    let width = s.pointer_bytes();
    if always_alloc(s, p.at(0))? {
        let data = s.alloc_load(4, s.layout(sz::MENU_DATA, 288))?;
        s.fixup_slot(p.at(0), data)?;
        load_menu_data(s, links, data)?;
    }
    load_window_def(s, links, p.at(s.layout(4, 8)))?;
    let item_count = s.i32_at(p, s.layout(168, 184))?.max(0) as usize;
    if let Some(items) = always_array(s, p.at(s.layout(172, 192)), 4, width * item_count)? {
        for i in 0..item_count {
            load_item_def_ptr(s, links, items.at(i * width))?;
        }
    }
    s.pop()
}

fn load_menu_data(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    let expression_data = s.layout(204, 272);
    if s.begin_body(p.at(expression_data))? {
        let expr = s.alloc_load(4, s.layout(sz::EXPRESSION_SUPPORTING_DATA, 48))?;
        s.fixup_slot(p.at(expression_data), expr)?;
        load_expression_supporting_data(s, expr)?;
    }
    for off in [
        24,
        s.layout(28, 32),
        s.layout(32, 40),
        s.layout(36, 48),
        s.layout(40, 56),
    ] {
        if always_alloc(s, p.at(off))? {
            let set = s.alloc_load(4, s.layout(sz::MENU_EVENT_HANDLER_SET, 16))?;
            s.fixup_slot(p.at(off), set)?;
            load_menu_event_handler_set(s, links, set)?;
        }
    }
    let on_key = s.layout(44, 64);
    if always_alloc(s, p.at(on_key))? {
        let key = s.alloc_load(4, s.layout(sz::ITEM_KEY_HANDLER, 24))?;
        s.fixup_slot(p.at(on_key), key)?;
        load_item_key_handler(s, links, key)?;
    }
    load_statement_ptr(s, links, p.at(s.layout(48, 72)))?;
    follow_name(s, p, s.layout(52, 80))?;
    follow_name(s, p, s.layout(56, 88))?;
    for off in [
        s.layout(76, 112),
        s.layout(80, 120),
        s.layout(84, 128),
        s.layout(88, 136),
        s.layout(92, 144),
        s.layout(96, 152),
        s.layout(100, 160),
    ] {
        load_statement_ptr(s, links, p.at(off))?;
    }
    Ok(())
}

fn load_window_def(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    follow_name(s, p, 0)?;
    follow_name(s, p, s.layout(44, 48))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(s.layout(160, 168)))
}

fn load_item_def_ptr(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
) -> Result<()> {
    if !always_alloc(s, slot)? {
        return Ok(());
    }
    let body = s.alloc_load(4, s.layout(sz::ITEM_DEF, 504))?;
    s.fixup_slot(slot, body)?;
    load_item_def(s, links, body)
}

fn load_item_def(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    load_window_def(s, links, p)?;
    follow_name(s, p, s.layout(228, 240))?;
    for off in [
        s.layout(240, 264),
        s.layout(244, 272),
        s.layout(248, 280),
        s.layout(252, 288),
        s.layout(256, 296),
        s.layout(260, 304),
        s.layout(264, 312),
        s.layout(268, 320),
        s.layout(272, 328),
    ] {
        if always_alloc(s, p.at(off))? {
            let set = s.alloc_load(4, s.layout(sz::MENU_EVENT_HANDLER_SET, 16))?;
            s.fixup_slot(p.at(off), set)?;
            load_menu_event_handler_set(s, links, set)?;
        }
    }
    follow_name(s, p, s.layout(276, 336))?;
    follow_name(s, p, s.layout(280, 344))?;
    let on_key = s.layout(284, 352);
    if always_alloc(s, p.at(on_key))? {
        let key = s.alloc_load(4, s.layout(sz::ITEM_KEY_HANDLER, 24))?;
        s.fixup_slot(p.at(on_key), key)?;
        load_item_key_handler(s, links, key)?;
    }
    follow_name(s, p, s.layout(288, 360))?;
    follow_name(s, p, s.layout(292, 368))?;
    asset_ptr_at(s, links, AssetType::Sound, p.at(s.layout(300, 384)))?;
    load_item_def_data(s, links, p)?;
    let float_expression = s.layout(sz::ITEM_FLOAT_EXPRESSION, 16);
    let float_count = s.i32_at(p, s.layout(316, 408))?.max(0) as usize;
    if let Some(arr) = always_array(
        s,
        p.at(s.layout(320, 416)),
        4,
        float_expression * float_count,
    )? {
        for i in 0..float_count {
            load_item_float_expression(s, links, arr.at(i * float_expression))?;
        }
    }
    for off in [
        s.layout(324, 424),
        s.layout(328, 432),
        s.layout(332, 440),
        s.layout(336, 448),
        s.layout(380, 496),
    ] {
        load_statement_ptr(s, links, p.at(off))?;
    }
    Ok(())
}

fn load_item_def_data(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    let ty = s.i32_at(p, s.layout(184, 196))?;
    let type_data = p.at(s.layout(312, 400));
    match ty {
        ITEM_TYPE_LISTBOX => {
            if always_alloc(s, type_data)? {
                let body = s.alloc_load(4, s.layout(sz::LIST_BOX_DEF, 472))?;
                s.fixup_slot(type_data, body)?;
                if always_alloc(s, body.at(416))? {
                    let set = s.alloc_load(4, s.layout(sz::MENU_EVENT_HANDLER_SET, 16))?;
                    s.fixup_slot(body.at(416), set)?;
                    load_menu_event_handler_set(s, links, set)?;
                }
                asset_ptr_at(s, links, AssetType::Material, body.at(s.layout(448, 456)))?;
                load_statement_ptr(s, links, body.at(s.layout(452, 464)))?;
            }
        }
        ITEM_TYPE_TEXT
        | ITEM_TYPE_EDITFIELD
        | ITEM_TYPE_NUMERICFIELD
        | ITEM_TYPE_SLIDER
        | ITEM_TYPE_YESNO
        | ITEM_TYPE_BIND
        | ITEM_TYPE_VALIDFILEFIELD
        | ITEM_TYPE_DECIMALFIELD
        | ITEM_TYPE_UPREDITFIELD
        | ITEM_TYPE_EMAILFIELD
        | ITEM_TYPE_PASSWORDFIELD => {
            if always_alloc(s, type_data)? {
                let body = s.alloc_load(4, sz::EDIT_FIELD_DEF)?;
                s.fixup_slot(type_data, body)?;
            }
        }
        ITEM_TYPE_MULTI => {
            if always_alloc(s, type_data)? {
                let width = s.pointer_bytes();
                let body = s.alloc_load(4, s.layout(sz::MULTI_DEF, 648))?;
                s.fixup_slot(type_data, body)?;
                for i in 0..32 {
                    follow_name(s, body, i * width)?;
                }
                for i in 0..32 {
                    follow_name(s, body, s.layout(128, 256) + i * width)?;
                }
            }
        }
        ITEM_TYPE_DVARENUM => follow_name(s, type_data, 0)?,
        ITEM_TYPE_NEWS_TICKER => {
            if always_alloc(s, type_data)? {
                let body = s.alloc_load(4, sz::NEWS_TICKER_DEF)?;
                s.fixup_slot(type_data, body)?;
            }
        }
        ITEM_TYPE_TEXT_SCROLL => {
            if always_alloc(s, type_data)? {
                let body = s.alloc_load(4, sz::TEXT_SCROLL_DEF)?;
                s.fixup_slot(type_data, body)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn load_item_float_expression(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
) -> Result<()> {
    load_statement_ptr(s, links, p.at(s.layout(4, 8)))
}

fn load_expression_supporting_data(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    load_ui_function_list(s, p.at(0))?;
    load_static_dvar_list(s, p.at(s.layout(8, 16)))?;
    load_string_list(s, p.at(s.layout(16, 32)))
}

fn load_ui_function_list(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let width = s.pointer_bytes();
    let functions = s.layout(4, 8);
    let count = s.i32_at(p, 0)?.max(0) as usize;

    if s.begin_body(p.at(functions))? {
        let arr = s.alloc_load(4, width * count)?;
        s.fixup_slot(p.at(functions), arr)?;
        for i in 0..count {
            load_statement_ptr(s, &mut IgnoreLinks, arr.at(i * width))?;
        }
    }
    Ok(())
}

fn load_static_dvar_list(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let width = s.pointer_bytes();
    let count = s.i32_at(p, 0)?.max(0) as usize;
    if let Some(arr) = always_array(s, p.at(s.layout(4, 8)), 4, width * count)? {
        for i in 0..count {
            load_static_dvar_ptr(s, arr.at(i * width))?;
        }
    }
    Ok(())
}

fn load_static_dvar_ptr(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
    if !always_alloc(s, slot)? {
        return Ok(());
    }
    let body = s.alloc_load(4, s.layout(sz::STATIC_DVAR, 16))?;
    s.fixup_slot(slot, body)?;

    follow_name(s, body, s.layout(4, 8))
}

fn load_string_list(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let width = s.pointer_bytes();
    let count = s.i32_at(p, 0)?.max(0) as usize;
    if let Some(arr) = always_array(s, p.at(s.layout(4, 8)), 4, width * count)? {
        for i in 0..count {
            s.follow_string(arr, i * width)?;
        }
    }
    Ok(())
}

fn load_menu_event_handler_set(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
) -> Result<()> {
    let width = s.pointer_bytes();
    let count = s.i32_at(p, 0)?.max(0) as usize;

    if let Some(arr) = always_array(s, p.at(s.layout(4, 8)), 4, width * count)? {
        for i in 0..count {
            load_menu_event_handler_ptr(s, links, arr.at(i * width))?;
        }
    }
    Ok(())
}

fn load_menu_event_handler_ptr(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
) -> Result<()> {
    if !always_alloc(s, slot)? {
        return Ok(());
    }
    let body = s.alloc_load(4, s.layout(sz::MENU_EVENT_HANDLER, 16))?;
    s.fixup_slot(slot, body)?;
    load_menu_event_handler(s, links, body)
}

fn load_menu_event_handler(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
) -> Result<()> {
    let event_type = s.u8_at(p, s.layout(4, 8))?;
    match event_type {
        EVENT_UNCONDITIONAL => follow_name(s, p, 0)?,
        EVENT_IF => {
            if always_alloc(s, p.at(0))? {
                let script = s.alloc_load(4, s.layout(sz::CONDITIONAL_SCRIPT, 16))?;
                s.fixup_slot(p.at(0), script)?;

                load_statement_ptr(s, links, script.at(s.layout(4, 8)))?;
                if always_alloc(s, script.at(0))? {
                    let set = s.alloc_load(4, s.layout(sz::MENU_EVENT_HANDLER_SET, 16))?;
                    s.fixup_slot(script.at(0), set)?;
                    load_menu_event_handler_set(s, links, set)?;
                }
            }
        }
        EVENT_ELSE => {
            if always_alloc(s, p.at(0))? {
                let set = s.alloc_load(4, s.layout(sz::MENU_EVENT_HANDLER_SET, 16))?;
                s.fixup_slot(p.at(0), set)?;
                load_menu_event_handler_set(s, links, set)?;
            }
        }
        EVENT_SET_LOCAL_VAR_BOOL
        | EVENT_SET_LOCAL_VAR_INT
        | EVENT_SET_LOCAL_VAR_FLOAT
        | EVENT_SET_LOCAL_VAR_STRING => {
            if always_alloc(s, p.at(0))? {
                let data = s.alloc_load(4, s.layout(sz::SET_LOCAL_VAR_DATA, 16))?;
                s.fixup_slot(p.at(0), data)?;
                follow_name(s, data, 0)?;
                load_statement_ptr(s, links, data.at(s.layout(4, 8)))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn load_item_key_handler(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
) -> Result<()> {
    let action = s.layout(4, 8);
    let next_off = s.layout(8, 16);
    if always_alloc(s, p.at(action))? {
        let set = s.alloc_load(4, s.layout(sz::MENU_EVENT_HANDLER_SET, 16))?;
        s.fixup_slot(p.at(action), set)?;
        load_menu_event_handler_set(s, links, set)?;
    }
    if always_alloc(s, p.at(next_off))? {
        let next = s.alloc_load(4, s.layout(sz::ITEM_KEY_HANDLER, 24))?;
        s.fixup_slot(p.at(next_off), next)?;
        load_item_key_handler(s, links, next)?;
    }
    Ok(())
}

fn load_statement_ptr(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
) -> Result<()> {
    let _ = links;
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => Ok(()),
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            Ok(())
        }
        _ => {
            if !s.begin_body(slot)? {
                return Ok(());
            }
            let body = s.alloc_load(4, s.layout(sz::STATEMENT, 128))?;
            s.fixup_slot(slot, body)?;
            load_statement(s, body)
        }
    }
}

fn load_statement(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let entry = s.layout(sz::EXPRESSION_ENTRY, 24);
    let supporting = s.layout(8, 16);
    let count = s.i32_at(p, 0)?.max(0) as usize;
    if let Some(arr) = always_array(s, p.at(s.layout(4, 8)), 4, entry * count)? {
        for i in 0..count {
            load_expression_entry(s, arr.at(i * entry))?;
        }
    }
    if s.begin_body(p.at(supporting))? {
        let data = s.alloc_load(4, s.layout(sz::EXPRESSION_SUPPORTING_DATA, 48))?;
        s.fixup_slot(p.at(supporting), data)?;
        load_expression_supporting_data(s, data)?;
    }
    Ok(())
}

fn load_expression_entry(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let ty = s.i32_at(p, 0)?;
    if ty == EET_OPERAND {
        let data_ty = s.i32_at(p, s.layout(4, 8))?;

        let internals = s.layout(8, 16);
        if data_ty == VAL_STRING {
            follow_name(s, p, internals)?;
        } else if data_ty == VAL_FUNCTION {
            load_statement_ptr(s, &mut IgnoreLinks, p.at(internals))?;
        }
    }
    Ok(())
}

struct IgnoreLinks;

impl AssetLinkSink for IgnoreLinks {
    fn loaded(
        &mut self,
        _s: &ZoneStream<'_>,
        _ty: AssetType,
        _slot: Ptr,
        _insert_slot: Option<Ptr>,
    ) -> Result<()> {
        Ok(())
    }

    fn alias(&mut self, _ty: AssetType, _slot: Ptr, _target: Ptr) -> Result<()> {
        Ok(())
    }
}
