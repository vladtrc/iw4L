use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub struct ScriptSources {
    sources: BTreeMap<String, ScriptSource>,
    tables: BTreeMap<String, ScriptTable>,
    schemas: BTreeMap<String, std::sync::Arc<structured_data_iw4::DefinitionSet>>,
    entities: Option<String>,
    path_nodes: Vec<asset_world::PathNode>,
    configs: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptSourceOrigin {
    Packaged,
    BuiltIn,
}

#[derive(Clone, Debug)]
struct ScriptSource {
    bytes: Result<Vec<u8>, String>,
    origin: ScriptSourceOrigin,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptTable {
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<String>,
}

fn normalize(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

impl ScriptSources {
    pub fn read(&self, module: &str) -> Result<Vec<u8>, String> {
        self.sources
            .get(module)
            .map(|source| source.bytes.clone())
            .unwrap_or_else(|| Err(format!("missing script asset {module}.gsc")))
    }

    pub fn origin(&self, module: &str) -> Option<ScriptSourceOrigin> {
        self.sources.get(module).map(|source| source.origin)
    }

    pub fn modules(&self) -> impl Iterator<Item = &str> {
        self.sources.keys().map(String::as_str)
    }

    pub fn walk_iw4_zone(path: &std::path::Path) -> Result<Self, String> {
        struct Walk(ScriptSources);
        impl fastfile_iw4::AssetSink for Walk {
            fn set_script_strings(&mut self, _: fastfile_iw4::ScriptStrings) {}
            fn load_asset(
                &mut self,
                s: &mut fastfile_iw4::ZoneStream<'_>,
                _: usize,
                ty: fastfile_iw4::AssetType,
                slot: fastfile_iw4::Ptr,
            ) -> fastfile_iw4::Result<()> {
                fastfile_iw4::load_asset_at_observed(s, ty, slot, self).map(|_| ())
            }
        }
        impl fastfile_iw4::AssetLinkSink for Walk {
            fn loaded(
                &mut self,
                _: &fastfile_iw4::ZoneStream<'_>,
                _: fastfile_iw4::AssetType,
                _: fastfile_iw4::Ptr,
                _: Option<fastfile_iw4::Ptr>,
            ) -> fastfile_iw4::Result<()> {
                Ok(())
            }
            fn alias(
                &mut self,
                _: fastfile_iw4::AssetType,
                _: fastfile_iw4::Ptr,
                _: fastfile_iw4::Ptr,
            ) -> fastfile_iw4::Result<()> {
                Ok(())
            }
            fn capture_raw_file(
                &mut self,
                name: &str,
                data: &[u8],
                zlib_compressed: bool,
            ) -> fastfile_iw4::Result<()> {
                self.0.capture(name, data, zlib_compressed);
                Ok(())
            }
        }
        let image = asset_transport::open_zone(path).map_err(|e| e.to_string())?;
        let header = image.header().map_err(|e| e.to_string())?;
        let mut memory = asset_transport::ZoneMemory::for_header(&header);
        let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
        let mut walk = Walk(Self::default());
        fastfile_iw4::load_zone(&mut stream, &mut walk).map_err(|e| e.to_string())?;
        Ok(walk.0)
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub(crate) fn asset_names(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for source in self.sources.values().filter_map(|s| s.bytes.as_ref().ok()) {
            quoted_names(source, &mut names);
        }
        if let Some(entities) = &self.entities {
            quoted_names(entities.as_bytes(), &mut names);
        }
        for table in self.tables.values() {
            names.extend(table.cells.iter().cloned());
        }
        names
    }

    pub(crate) fn fx_references(&self) -> BTreeMap<String, String> {
        let mut references = BTreeMap::new();
        for (module, source) in &self.sources {
            let Ok(bytes) = &source.bytes else {
                continue;
            };
            let mut at = 0;
            while at < bytes.len() {
                if bytes[at..].starts_with(b"//") {
                    while at < bytes.len() && bytes[at] != b'\n' {
                        at += 1;
                    }
                } else if bytes[at..].starts_with(b"/*") {
                    at += 2;
                    while at + 1 < bytes.len() && &bytes[at..at + 2] != b"*/" {
                        at += 1;
                    }
                    at = (at + 2).min(bytes.len());
                } else if bytes[at] == b'"' {
                    at += 1;
                    while at < bytes.len() && bytes[at] != b'"' {
                        at += if bytes[at] == b'\\' { 2 } else { 1 };
                    }
                    at = (at + 1).min(bytes.len());
                } else if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
                    let start = at;
                    while at < bytes.len()
                        && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_')
                    {
                        at += 1;
                    }
                    if !bytes[start..at].eq_ignore_ascii_case(b"loadfx") {
                        continue;
                    }
                    let mut value = at;
                    while bytes.get(value).is_some_and(u8::is_ascii_whitespace) {
                        value += 1;
                    }
                    if bytes.get(value) != Some(&b'(') {
                        continue;
                    }
                    value += 1;
                    while bytes.get(value).is_some_and(u8::is_ascii_whitespace) {
                        value += 1;
                    }
                    if bytes.get(value) != Some(&b'"') {
                        continue;
                    }
                    value += 1;
                    let start = value;
                    while value < bytes.len() && bytes[value] != b'"' {
                        value += 1;
                    }
                    if let Ok(name) = std::str::from_utf8(&bytes[start..value]) {
                        references.insert(normalize(name), module.clone());
                    }
                } else {
                    at += 1;
                }
            }
        }
        references
    }

    pub fn schemas(&self) -> &BTreeMap<String, std::sync::Arc<structured_data_iw4::DefinitionSet>> {
        &self.schemas
    }

    pub(crate) fn capture_schema(
        &mut self,
        name: String,
        schema: structured_data_iw4::DefinitionSet,
    ) {
        self.schemas
            .insert(normalize(&name), std::sync::Arc::new(schema));
    }

    pub fn tables(&self) -> &BTreeMap<String, ScriptTable> {
        &self.tables
    }

    pub fn entities(&self) -> Option<&str> {
        self.entities.as_deref()
    }

    pub fn path_nodes(&self) -> &[asset_world::PathNode] {
        &self.path_nodes
    }

    pub(crate) fn set_path_nodes(&mut self, nodes: Vec<asset_world::PathNode>) {
        self.path_nodes = nodes;
    }

    pub fn config(&self, name: &str) -> Option<&str> {
        self.configs.get(&normalize(name)).map(String::as_str)
    }

    pub fn shocks(&self) -> impl Iterator<Item = (&str, &str)> {
        self.configs.iter().filter_map(|(path, text)| {
            let name = path.strip_prefix("shock/")?.strip_suffix(".shock")?;
            Some((name, text.as_str()))
        })
    }

    pub(crate) fn capture(&mut self, name: &str, data: &[u8], compressed: bool) {
        let name = normalize(name);
        if name.ends_with(".cfg")
            || name.ends_with(".shock")
            || name.ends_with(".atr")
            || name == "radiant/keys.txt"
        {
            if let Some(text) = asset_world::decode_rawfile_text(data, compressed) {
                self.configs.insert(name, text);
            }
            return;
        }
        let Some(module) = name.strip_suffix(".gsc") else {
            return;
        };
        let source = if compressed {
            asset_transport::inflate_zlib(data).map_err(|e| e.to_string())
        } else {
            Ok(asset_world::decode_packed_rawfile(data).unwrap_or_else(|| data.to_vec()))
        }
        .map(|mut bytes| {
            while bytes.last() == Some(&0) {
                bytes.pop();
            }
            bytes
        });
        self.sources.insert(
            module.to_owned(),
            ScriptSource {
                bytes: source,
                origin: ScriptSourceOrigin::Packaged,
            },
        );
    }

    pub(crate) fn capture_table(&mut self, table: &asset_game::CapturedStringTable) {
        self.tables.insert(
            normalize(&table.name),
            ScriptTable {
                columns: table.columns,
                rows: table.rows,
                cells: table.cells.clone(),
            },
        );
    }

    pub(crate) fn set_table_cells(
        &mut self,
        table: &str,
        key: &str,
        cells: &[(usize, String)],
    ) -> bool {
        let Some(table) = self.tables.get_mut(&normalize(table)) else {
            return false;
        };
        let Some(row) = table
            .cells
            .chunks_exact_mut(table.columns.max(1))
            .take(table.rows)
            .find(|row| row[0].eq_ignore_ascii_case(key))
        else {
            return false;
        };
        for (column, value) in cells {
            if let Some(cell) = row.get_mut(*column) {
                cell.clone_from(value);
            }
        }
        true
    }

    pub(crate) fn insert_source(&mut self, module: &str, source: String) {
        self.sources.insert(
            normalize(module),
            ScriptSource {
                bytes: Ok(source.into_bytes()),
                origin: ScriptSourceOrigin::BuiltIn,
            },
        );
    }

    pub(crate) fn set_entities(&mut self, entities: String) {
        self.entities = Some(entities);
    }

    pub fn overlay(&mut self, other: Self) {
        self.sources.extend(other.sources);
        self.tables.extend(other.tables);
        self.schemas.extend(other.schemas);
        self.configs.extend(other.configs);
        if other.entities.is_some() {
            self.entities = other.entities;
        }
        if !other.path_nodes.is_empty() {
            self.path_nodes = other.path_nodes;
        }
    }
}

fn quoted_names(bytes: &[u8], names: &mut BTreeSet<String>) {
    let mut at = 0;
    while at < bytes.len() {
        match (bytes[at], bytes.get(at + 1).copied()) {
            (b'/', Some(b'/')) => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            (b'/', Some(b'*')) => {
                at += 2;
                while at + 1 < bytes.len() && &bytes[at..at + 2] != b"*/" {
                    at += 1;
                }
                at = (at + 2).min(bytes.len());
            }
            (b'"', _) => {
                at += 1;
                let start = at;
                while at < bytes.len() && bytes[at] != b'"' {
                    if bytes[at] == b'\\' && at + 1 < bytes.len() {
                        at += 1;
                    }
                    at += 1;
                }
                if let Ok(name) = std::str::from_utf8(&bytes[start..at]) {
                    names.insert(name.to_owned());
                }
                at += 1;
            }
            _ => at += 1,
        }
    }
}
