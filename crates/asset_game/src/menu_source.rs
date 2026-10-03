use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::{MenuCatalog, MenuDef, MenuEvent, MenuItem};

#[derive(Deserialize)]
#[serde(untagged)]
enum Definition {
    Variant(Variant),
    Complete(MenuDef),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    name: String,
    base: String,
    #[serde(default)]
    fields: BTreeMap<String, Value>,
    #[serde(default)]
    item_overrides: Vec<ItemPatch>,
    #[serde(default)]
    append_items: Vec<MenuItem>,
    #[serde(default)]
    item_copies: Vec<ItemPatch>,
    #[serde(default)]
    open_prefix: Vec<MenuEvent>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemPatch {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    dvar: Option<String>,
    #[serde(default)]
    text_key: Option<String>,
    #[serde(default)]
    item_type: Option<i32>,
    #[serde(default)]
    optional: bool,
    fields: BTreeMap<String, Value>,
}

impl ItemPatch {
    fn resolve(&self, menu: &MenuDef) -> Result<Option<usize>, String> {
        let selectors = [
            self.name.is_some(),
            self.index.is_some(),
            self.dvar.is_some(),
            self.text_key.is_some(),
        ];
        if selectors.into_iter().filter(|set| *set).count() != 1 {
            return Err(format!(
                "Menu `{}` item patch requires one selector",
                menu.name
            ));
        }
        // Localized/modified fastfiles do not share item indices. A text key
        // can also label both a navigation button and a heading, so allow a
        // type discriminator while still requiring exactly one matching item.
        let mut matches = menu.items.iter().enumerate().filter(|(index, item)| {
            self.index.is_none_or(|selected| selected == *index)
                && self.name.as_ref().is_none_or(|name| name == &item.name)
                && self.dvar.as_ref().is_none_or(|dvar| dvar == &item.dvar)
                && self
                    .text_key
                    .as_ref()
                    .is_none_or(|key| key == &item.text_key)
                && self.item_type.is_none_or(|kind| kind == item.item_type)
        });
        match (matches.next(), matches.next()) {
            (Some((index, _)), None) => Ok(Some(index)),
            (None, None) if self.optional => Ok(None),
            _ => Err(format!(
                "Menu `{}` has no unique item matching name={:?}, index={:?}, dvar={:?}, text_key={:?}, item_type={:?}",
                menu.name, self.name, self.index, self.dvar, self.text_key, self.item_type
            )),
        }
    }
}

fn merge(target: &mut Value, fields: BTreeMap<String, Value>) -> Result<(), String> {
    let object = target
        .as_object_mut()
        .ok_or("Menu override requires an object")?;
    for (key, value) in fields {
        if let Value::Object(child) = value {
            if let Some(existing) = object.get_mut(&key).filter(|value| value.is_object()) {
                merge(existing, child.into_iter().collect())?;
            } else {
                object.insert(key, Value::Object(child));
            }
        } else {
            object.insert(key, value);
        }
    }
    Ok(())
}

fn resolve_definition(
    definition: Definition,
    resolved: &BTreeMap<String, MenuDef>,
    catalog: &MenuCatalog,
) -> Result<MenuDef, String> {
    let def = match definition {
        Definition::Complete(def) => def,
        Definition::Variant(variant) => {
            let base = resolved
                .get(&variant.base)
                .or_else(|| catalog.get(&variant.base))
                .ok_or_else(|| {
                    format!(
                        "Menu `{}` requires missing base `{}`",
                        variant.name, variant.base
                    )
                })?;
            let mut value = serde_json::to_value(base).map_err(|error| error.to_string())?;
            merge(&mut value, variant.fields)?;
            let mut def: MenuDef = serde_json::from_value(value)
                .map_err(|error| format!("Menu `{}`: {error}", variant.name))?;
            def.name = variant.name;
            for item in variant.item_overrides {
                let Some(index) = item.resolve(&def)? else {
                    continue;
                };
                let target = &mut def.items[index];
                let mut value =
                    serde_json::to_value(&*target).map_err(|error| error.to_string())?;
                merge(&mut value, item.fields)?;
                *target = serde_json::from_value(value)
                    .map_err(|error| format!("Menu `{}` item {index}: {error}", def.name))?;
            }
            for copy in variant.item_copies {
                let Some(index) = copy.resolve(base)? else {
                    continue;
                };
                let source = &base.items[index];
                let mut value = serde_json::to_value(source).map_err(|error| error.to_string())?;
                merge(&mut value, copy.fields)?;
                def.items.push(
                    serde_json::from_value(value)
                        .map_err(|error| format!("Menu `{}` copied item: {error}", def.name))?,
                );
            }
            def.items.extend(variant.append_items);
            def.handlers.open.splice(0..0, variant.open_prefix);
            def
        }
    };
    if def.name.is_empty() || resolved.contains_key(&def.name) {
        return Err(format!("Empty or duplicate menu name `{}`", def.name));
    }
    Ok(def)
}

fn parse(source: &str) -> Result<Vec<Definition>, String> {
    serde_json::from_str(source).map_err(|error| format!("Menu definitions: {error}"))
}

pub(crate) fn load(source: &str, catalog: &MenuCatalog) -> Result<Vec<MenuDef>, String> {
    let mut resolved = BTreeMap::<String, MenuDef>::new();
    for definition in parse(source)? {
        let def = resolve_definition(definition, &resolved, catalog)?;
        resolved.insert(def.name.clone(), def);
    }
    Ok(resolved.into_values().collect())
}

/// Like [`load`], but a variant whose base menu is absent — a base defined
/// only by game menu content, for example — is skipped with a warning
/// instead of failing the whole source. Every other failure stays fatal.
pub(crate) fn load_lenient(
    source: &str,
    catalog: &MenuCatalog,
) -> Result<(Vec<MenuDef>, Vec<String>), String> {
    let mut resolved = BTreeMap::<String, MenuDef>::new();
    let mut warnings = Vec::new();
    for definition in parse(source)? {
        if let Definition::Variant(variant) = &definition
            && resolved.get(&variant.base).is_none()
            && catalog.get(&variant.base).is_none()
        {
            warnings.push(format!(
                "Menu `{}` requires missing base `{}`",
                variant.name, variant.base
            ));
            continue;
        }
        let def = resolve_definition(definition, &resolved, catalog)?;
        resolved.insert(def.name.clone(), def);
    }
    Ok((resolved.into_values().collect(), warnings))
}
