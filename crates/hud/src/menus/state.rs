use bevy::prelude::*;
use frame::AppScreen;

#[derive(Clone, Debug, Default)]
pub(crate) struct ItemState {
    pub(crate) hidden: bool,
    pub(crate) fore: Option<[f32; 4]>,
    pub(crate) back: Option<[f32; 4]>,
}

#[derive(Clone, Debug)]
pub(crate) struct OpenMenu {
    pub(crate) name: String,
    pub(crate) captures_input: bool,
    pub(crate) focus: Option<usize>,
    pub(crate) hover: Option<usize>,
    pub(crate) items: Vec<ItemState>,
    pub(crate) item_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct EditState {
    pub(crate) menu: String,
    pub(crate) item: usize,
    pub(crate) buffer: Vec<char>,
    pub(crate) cursor: usize,
    pub(crate) max_chars: usize,
}

#[derive(Resource, Default)]
pub struct ScriptMenus {
    pub(crate) stack: Vec<OpenMenu>,
    pub(crate) applied_serial: u32,
    pub(crate) responses: Vec<(String, String)>,
    pub(crate) exec: Vec<String>,
    pub(crate) sounds: Vec<String>,
    pub(crate) reported: std::collections::HashSet<String>,
    pub(crate) screen: Option<AppScreen>,
    pub(crate) cursor: Option<Vec2>,
    pub(crate) editing: Option<EditState>,
    pub(crate) slider_drag: Option<(String, usize)>,
    pub(crate) binding_menu: Option<String>,
    pub(crate) bind_requests: Vec<String>,
}

pub(crate) fn item_ids(def: &asset_game::MenuDef) -> Vec<String> {
    let mut occurrences = std::collections::HashMap::<String, usize>::new();
    def.items
        .iter()
        .map(|item| {
            let name = if !item.authored_id.is_empty() {
                format!("authored:{}", item.authored_id.to_ascii_lowercase())
            } else if item.name.is_empty() {
                format!(
                    "anonymous:{}:{}:{}:{}",
                    item.item_type, item.owner_draw, item.dvar, item.text_key
                )
            } else {
                item.name.to_ascii_lowercase()
            };
            let occurrence = occurrences.entry(name.clone()).or_default();
            let id = format!("{name}#{}", *occurrence);
            *occurrence += 1;
            id
        })
        .collect()
}

impl ScriptMenus {
    pub(crate) fn reconcile(&mut self, catalog: &asset_game::MenuCatalog) {
        self.stack.retain(|open| catalog.get(&open.name).is_some());
        for open in &mut self.stack {
            let Some(def) = catalog.get(&open.name) else {
                continue;
            };
            let ids = item_ids(def);
            if open.item_ids == ids {
                continue;
            }
            let remap = |index: Option<usize>| {
                index
                    .and_then(|index| open.item_ids.get(index))
                    .and_then(|id| ids.iter().position(|next| next == id))
            };
            open.focus = remap(open.focus);
            open.hover = remap(open.hover);
            if let Some(editing) = &mut self.editing
                && editing.menu.eq_ignore_ascii_case(&open.name)
            {
                if let Some(next) = remap(Some(editing.item)) {
                    editing.item = next;
                } else {
                    self.editing = None;
                }
            }
            if let Some((menu, index)) = &mut self.slider_drag
                && menu.eq_ignore_ascii_case(&open.name)
            {
                if let Some(next) = remap(Some(*index)) {
                    *index = next;
                } else {
                    self.slider_drag = None;
                }
            }
            let mut states: std::collections::HashMap<_, _> = open
                .item_ids
                .drain(..)
                .zip(std::mem::take(&mut open.items))
                .collect();
            open.items = ids
                .iter()
                .map(|id| states.remove(id).unwrap_or_else(ItemState::default))
                .collect();
            open.item_ids = ids;
        }
        if self
            .editing
            .as_ref()
            .is_some_and(|editing| !self.is_open(&editing.menu))
        {
            self.editing = None;
        }
        if self
            .slider_drag
            .as_ref()
            .is_some_and(|(menu, _)| !self.is_open(menu))
        {
            self.slider_drag = None;
        }
    }

    pub fn captures_input(&self) -> bool {
        self.input_menu().is_some()
    }

    pub(crate) fn input_menu(&self) -> Option<&OpenMenu> {
        self.stack.iter().rev().find(|menu| menu.captures_input)
    }

    pub fn open_names(&self) -> Vec<String> {
        self.stack.iter().map(|m| m.name.clone()).collect()
    }

    pub fn is_open(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    pub fn focused_item(&self) -> Option<(&str, usize)> {
        let menu = self.input_menu()?;
        Some((&menu.name, menu.focus?))
    }

    pub fn focused_item_in(&self, name: &str) -> Option<usize> {
        self.stack.get(self.position(name)?)?.focus
    }

    pub(crate) fn position(&self, name: &str) -> Option<usize> {
        self.stack
            .iter()
            .position(|m| m.name.eq_ignore_ascii_case(name))
    }

    pub(crate) fn get_mut(&mut self, name: &str) -> Option<&mut OpenMenu> {
        let pos = self.position(name)?;
        self.stack.get_mut(pos)
    }
}
