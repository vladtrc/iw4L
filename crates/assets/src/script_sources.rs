use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct ScriptSources {
    sources: BTreeMap<String, Result<Vec<u8>, String>>,
    tables: BTreeMap<String, ScriptTable>,
    entities: Option<String>,
    configs: BTreeMap<String, String>,
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
            .cloned()
            .unwrap_or_else(|| Err(format!("missing script asset {module}.gsc")))
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn tables(&self) -> &BTreeMap<String, ScriptTable> {
        &self.tables
    }

    pub fn entities(&self) -> Option<&str> {
        self.entities.as_deref()
    }

    pub fn config(&self, name: &str) -> Option<&str> {
        self.configs.get(&normalize(name)).map(String::as_str)
    }

    pub(crate) fn capture(&mut self, name: &str, data: &[u8], compressed: bool) {
        let name = normalize(name);
        if name.ends_with(".cfg") {
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
        self.sources.insert(module.to_owned(), source);
    }

    pub(crate) fn capture_table(&mut self, table: &crate::CapturedStringTable) {
        self.tables.insert(
            normalize(&table.name),
            ScriptTable {
                columns: table.columns,
                rows: table.rows,
                cells: table.cells.clone(),
            },
        );
    }

    pub(crate) fn insert_source(&mut self, module: &str, source: String) {
        self.sources
            .insert(normalize(module), Ok(source.into_bytes()));
    }

    pub(crate) fn set_entities(&mut self, entities: String) {
        self.entities = Some(entities);
    }

    pub(crate) fn overlay(&mut self, other: Self) {
        self.sources.extend(other.sources);
        self.tables.extend(other.tables);
        self.configs.extend(other.configs);
        if other.entities.is_some() {
            self.entities = other.entities;
        }
    }
}
