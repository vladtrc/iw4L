use std::collections::{BTreeMap, HashMap};

use crate::{
    AttachmentChoice, CapturedStringTable, ConfigurationRefusal, FamilyId, FamilyKey, FamilySlot,
    LoadoutRules, WeaponFamily, WeaponSelection,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedConfiguration {
    handle: crate::WeaponHandle,
    selection: WeaponSelection,
}

impl ResolvedConfiguration {
    pub(crate) fn new(handle: crate::WeaponHandle, selection: WeaponSelection) -> Self {
        Self { handle, selection }
    }

    pub fn handle(&self) -> crate::WeaponHandle {
        self.handle
    }

    pub fn selection(&self) -> &WeaponSelection {
        &self.selection
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentOption {
    pub choice: AttachmentChoice,
    pub selected: bool,
    pub toggle: Result<ResolvedConfiguration, ConfigurationRefusal>,
}

#[derive(Clone, Debug, Default)]
struct NamespaceTables {
    attachments: BTreeMap<String, AttachmentChoice>,
    forbidden: std::collections::HashSet<(String, String)>,
    partners: HashMap<String, Vec<String>>,
    cosmetics: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct WeaponFamilies {
    families: Vec<WeaponFamily>,
    by_key: HashMap<FamilyKey, usize>,
    tables: HashMap<FamilyId, NamespaceTables>,
    described: HashMap<u32, WeaponSelection>,
    excluded: Vec<(String, String)>,
}

fn is_attachment_table(name: &str) -> bool {
    name.eq_ignore_ascii_case("mp/attachmentTable.csv")
}

fn is_combos_table(name: &str) -> bool {
    name.eq_ignore_ascii_case("mp/attachmentCombos.csv")
}

fn read_attachments(namespace: FamilyId, table: &CapturedStringTable) -> NamespaceTables {
    let policy = &crate::weapon_policy(namespace)
        .expect("registered table owner")
        .tables;
    let mut out = NamespaceTables::default();
    for row in 1..table.rows as i32 {
        let Some(row) = (policy.attachment)(table, row) else {
            continue;
        };
        match row.role {
            crate::AttachmentTableRole::Cosmetic => {
                out.cosmetics.push(row.choice.name);
                continue;
            }
            crate::AttachmentTableRole::Service => continue,
            crate::AttachmentTableRole::Offered => {}
        }
        if let Some(partners) = row.partners {
            out.partners.insert(row.choice.name.clone(), partners);
        }
        out.attachments
            .entry(row.choice.name.clone())
            .or_insert(row.choice);
    }
    if policy.order == crate::AttachmentOrder::ByName {
        for (order, choice) in out.attachments.values_mut().enumerate() {
            choice.order = order as u32;
        }
    }
    out
}

fn read_combos(table: &CapturedStringTable, into: &mut NamespaceTables) {
    for row in 1..table.rows as i32 {
        let a = table.cell(row, 0).to_ascii_lowercase();
        for col in 1..table.columns as i32 {
            if !table.cell(row, col).eq_ignore_ascii_case("no") {
                continue;
            }
            let b = table.cell(col, 0).to_ascii_lowercase();
            if a.is_empty() || b.is_empty() || a == b {
                continue;
            }
            into.forbidden.insert(ordered_pair(&a, &b));
        }
    }
}

fn ordered_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

pub(crate) trait FamilyContent:
    crate::weapon_catalog::configuration::WeaponConfigurationCompiler
{
    fn published_handle(&self, id: u32) -> Option<crate::WeaponHandle>;
    fn lookup(&self, namespace: FamilyId, name: &str) -> Option<u32>;
    fn offhand_class(&self, id: u32) -> i32;
    fn admission(&self, id: u32) -> Result<(), ConfigurationRefusal>;
    fn names_in(&self, namespace: FamilyId) -> Vec<(u32, String)>;
    fn prepared_all(&self) -> Vec<(u32, WeaponSelection)> {
        Vec::new()
    }
}

impl WeaponFamilies {
    pub(crate) fn build(
        tables: &[(FamilyId, CapturedStringTable)],
        content: &impl FamilyContent,
    ) -> Self {
        let mut out = Self::default();
        for (namespace, table) in tables {
            if crate::weapon_policy(*namespace).is_none() {
                continue;
            }
            if is_attachment_table(&table.name) && !out.tables.contains_key(namespace) {
                out.tables
                    .insert(*namespace, read_attachments(*namespace, table));
            }
        }
        for (namespace, table) in tables {
            if crate::weapon_policy(*namespace).is_none() {
                continue;
            }
            if is_combos_table(&table.name) {
                let entry = out.tables.entry(*namespace).or_default();
                if entry.forbidden.is_empty() {
                    read_combos(table, entry);
                }
            }
        }
        for (namespace, table) in tables {
            if crate::weapon_policy(*namespace).is_none() {
                continue;
            }
            if crate::is_stats_table_name(&table.name) {
                out.read_stats(*namespace, table, content);
            }
        }
        out.describe_registry(content);
        out
    }

    fn read_stats(
        &mut self,
        namespace: FamilyId,
        table: &CapturedStringTable,
        content: &impl FamilyContent,
    ) {
        let known = self
            .tables
            .get(&namespace)
            .cloned()
            .unwrap_or_else(NamespaceTables::default);
        let Some(policy) = crate::weapon_policy(namespace).map(|policy| &policy.tables) else {
            return;
        };
        for row in 1..table.rows as i32 {
            let Some(row) = (policy.stats)(table, row, &known.attachments) else {
                continue;
            };
            let group = row.item_group.as_str();
            let key = FamilyKey::new(namespace, &row.base);
            if known.attachments.contains_key(&key.base) || self.by_key.contains_key(&key) {
                continue;
            }
            let base_id = content.lookup(namespace, &key.base);
            let category = crate::cac_category_from_item_group(group);
            let offhand = base_id
                .map(|id| content.offhand_class(id))
                .and_then(crate::cac_offhand_bucket);
            let slot = (policy.slot)(&row, category, offhand);
            let offered = &row.offered;
            let mut attachments: Vec<AttachmentChoice> = offered
                .iter()
                .filter_map(|name| known.attachments.get(name).cloned())
                .collect();
            attachments.sort_by_key(|choice| choice.order);
            attachments.dedup_by(|a, b| a.name == b.name);
            match base_id {
                None => self.excluded.push((
                    key.asset_key(),
                    format!("authored base `{}` is not in the loaded content", key.base),
                )),
                Some(id) if let Err(refusal) = content.admission(id) => {
                    self.excluded.push((key.asset_key(), refusal.to_string()))
                }
                Some(_) if slot == FamilySlot::Other => self
                    .excluded
                    .push((key.asset_key(), format!("`{group}` is not a class slot"))),
                Some(_) => {}
            }
            self.by_key.insert(key.clone(), self.families.len());
            self.families.push(WeaponFamily {
                key,
                item_group: group.to_owned(),
                category,
                slot,
                display_key: row.caption_key,
                image: row.image,
                base: base_id,
                attachments,
            });
        }
    }

    fn describe_registry(&mut self, content: &impl FamilyContent) {
        let namespaces: Vec<FamilyId> = {
            let mut seen: Vec<FamilyId> = self.families.iter().map(|f| f.key.namespace).collect();
            seen.sort_by_key(|ns| ns.as_str());
            seen.dedup();
            seen
        };
        for namespace in namespaces {
            let mut bases: Vec<&WeaponFamily> = self
                .families
                .iter()
                .filter(|family| family.key.namespace == namespace)
                .collect();
            bases.sort_by_key(|family| std::cmp::Reverse(family.key.base.len()));
            let mut described = Vec::new();
            for (id, name) in content.names_in(namespace) {
                for family in &bases {
                    if let Some(attachments) = self.parse_configuration(family, &name) {
                        described
                            .push((id, WeaponSelection::with(family.key.clone(), &attachments)));
                        break;
                    }
                }
            }
            described.extend(content.prepared_all().into_iter().filter(|(_, selection)| {
                selection
                    .family
                    .as_ref()
                    .is_some_and(|key| key.namespace == namespace)
            }));
            for (id, selection) in described {
                if self
                    .resolve(&selection, LoadoutRules::default(), content)
                    .is_ok_and(|resolved| Some(resolved.handle()) == content.published_handle(id))
                {
                    self.described.insert(id, selection);
                }
            }
        }
    }

    fn parse_configuration(&self, family: &WeaponFamily, name: &str) -> Option<Vec<String>> {
        let parts = crate::weapon_catalog::configuration::authored_attachments(&family.key, name)?;
        if parts.is_empty() {
            return Some(parts);
        }
        let tables = self.tables.get(&family.key.namespace)?;
        if parts.is_empty()
            || !parts
                .iter()
                .all(|part| tables.attachments.contains_key(part))
        {
            return None;
        }
        let normalized = self.normalize(family.key.namespace, &parts);
        (normalized == parts).then_some(parts)
    }

    pub(crate) fn normalize(&self, namespace: FamilyId, attachments: &[String]) -> Vec<String> {
        let mut out: Vec<String> = attachments
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .filter(|name| !name.is_empty() && name != "none")
            .collect();
        out.sort();
        out.dedup();
        let Some(policy) = crate::weapon_policy(namespace).map(|policy| &policy.tables) else {
            return Vec::new();
        };
        let tables = self.tables.get(&namespace);
        out.sort_by_key(|name| {
            tables
                .and_then(|t| t.attachments.get(name))
                .map(|choice| (policy.point_rank)(&choice.point))
                .unwrap_or(0)
        });
        out
    }

    fn compatible(&self, namespace: FamilyId, a: &str, b: &str) -> bool {
        let Some(policy) = crate::weapon_policy(namespace).map(|policy| &policy.tables) else {
            return false;
        };
        let Some(tables) = self.tables.get(&namespace) else {
            return true;
        };
        match policy.compatibility {
            crate::AttachmentCompatibility::ForbiddenPairs => {
                !tables.forbidden.contains(&ordered_pair(a, b))
            }
            crate::AttachmentCompatibility::MutualPartners => {
                let lists = |x: &str, y: &str| {
                    tables
                        .partners
                        .get(x)
                        .is_some_and(|partners| partners.iter().any(|p| p == y))
                };
                lists(a, b) && lists(b, a)
            }
        }
    }

    pub fn families(&self) -> &[WeaponFamily] {
        &self.families
    }

    pub fn family(&self, key: &FamilyKey) -> Option<&WeaponFamily> {
        self.by_key.get(key).map(|&i| &self.families[i])
    }

    pub fn find(&self, key: &FamilyKey) -> Option<&WeaponFamily> {
        self.family(key).or_else(|| {
            let prefixed = FamilyKey {
                namespace: key.namespace,
                base: format!("{}_{}", key.namespace.as_str(), key.base),
            };
            self.family(&prefixed)
        })
    }

    pub(crate) fn candidate_selections(&self, namespace: FamilyId) -> Vec<(u32, WeaponSelection)> {
        let mut families: Vec<&WeaponFamily> = self
            .families
            .iter()
            .filter(|family| family.key.namespace == namespace && family.base.is_some())
            .collect();
        families.sort_by_key(|family| family.key.asset_key());
        let mut out = Vec::new();
        for family in families {
            let base = family.base.expect("filtered loaded base");
            out.push((base, WeaponSelection::bare(family.key.clone())));
            let mut names: Vec<String> = family
                .attachments
                .iter()
                .map(|choice| choice.name.clone())
                .collect();
            names.sort();
            names.dedup();
            for (i, name) in names.iter().enumerate() {
                out.push((
                    base,
                    WeaponSelection::with(family.key.clone(), std::slice::from_ref(name)),
                ));
                for other in &names[i + 1..] {
                    if self.compatible(namespace, name, other) {
                        out.push((
                            base,
                            WeaponSelection::with(
                                family.key.clone(),
                                &[name.clone(), other.clone()],
                            ),
                        ));
                    }
                }
            }
        }
        out
    }

    pub fn offered(&self) -> impl Iterator<Item = &WeaponFamily> {
        self.families.iter().filter(|family| {
            family.slot != FamilySlot::Other
                && !self
                    .excluded
                    .iter()
                    .any(|(key, _)| *key == family.key.asset_key())
        })
    }

    pub fn excluded(&self) -> &[(String, String)] {
        &self.excluded
    }

    pub fn cosmetic_choices(&self, namespace: FamilyId) -> &[String] {
        self.tables
            .get(&namespace)
            .map(|t| t.cosmetics.as_slice())
            .unwrap_or(&[])
    }

    pub fn describe(&self, id: u32) -> Option<&WeaponSelection> {
        self.described.get(&id)
    }

    pub(crate) fn resolve(
        &self,
        selection: &WeaponSelection,
        rules: LoadoutRules,
        content: &impl FamilyContent,
    ) -> Result<ResolvedConfiguration, ConfigurationRefusal> {
        let Some(key) = selection.family.as_ref() else {
            return Err(ConfigurationRefusal::UnknownFamily(String::new()));
        };
        let family = self
            .family(key)
            .ok_or_else(|| ConfigurationRefusal::UnknownFamily(key.asset_key()))?;
        let namespace = key.namespace;
        let attachments = self.normalize(namespace, &selection.attachments);
        let known = self.tables.get(&namespace);
        for name in &attachments {
            if !known.is_some_and(|t| t.attachments.contains_key(name)) {
                return Err(ConfigurationRefusal::UnknownAttachment(name.clone()));
            }
            if !family.attachments.iter().any(|choice| choice.name == *name) {
                return Err(ConfigurationRefusal::NotOffered(name.clone()));
            }
        }
        if attachments.len() > rules.max_attachments {
            return Err(ConfigurationRefusal::RuleRestricted(format!(
                "at most {} attachments per weapon",
                rules.max_attachments
            )));
        }
        for (i, a) in attachments.iter().enumerate() {
            for b in &attachments[i + 1..] {
                if !self.compatible(namespace, a, b) {
                    return Err(ConfigurationRefusal::Incompatible {
                        a: a.clone(),
                        b: b.clone(),
                    });
                }
            }
        }
        let selection = WeaponSelection::with(key.clone(), &attachments);
        let id = content.compile(family, &selection)?;
        Ok(ResolvedConfiguration::new(
            content
                .published_handle(id)
                .ok_or_else(|| ConfigurationRefusal::MissingContent(id.to_string()))?,
            selection,
        ))
    }

    pub(crate) fn attachment_options(
        &self,
        selection: &WeaponSelection,
        rules: LoadoutRules,
        content: &impl FamilyContent,
    ) -> Result<Vec<AttachmentOption>, ConfigurationRefusal> {
        let key = selection
            .family
            .as_ref()
            .ok_or_else(|| ConfigurationRefusal::UnknownFamily(String::new()))?;
        let family = self
            .family(key)
            .ok_or_else(|| ConfigurationRefusal::UnknownFamily(key.asset_key()))?;
        let current = self.normalize(key.namespace, &selection.attachments);
        Ok(family
            .attachments
            .iter()
            .map(|choice| {
                let selected = current.contains(&choice.name);
                let mut next = current.clone();
                if selected {
                    next.retain(|name| *name != choice.name);
                } else {
                    next.push(choice.name.clone());
                }
                let toggle =
                    self.resolve(&WeaponSelection::with(key.clone(), &next), rules, content);
                AttachmentOption {
                    choice: choice.clone(),
                    selected,
                    toggle,
                }
            })
            .collect())
    }
}
