use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct ScriptSources {
    sources: BTreeMap<String, Result<Vec<u8>, String>>,
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

    pub(crate) fn capture(&mut self, name: &str, data: &[u8], compressed: bool) {
        let name = name.replace('\\', "/").to_ascii_lowercase();
        let Some(module) = name.strip_suffix(".gsc") else {
            return;
        };
        let source = if compressed {
            asset_transport::inflate_zlib(data).map_err(|e| e.to_string())
        } else {
            Ok(data.to_vec())
        }
        .map(|mut bytes| {
            while bytes.last() == Some(&0) {
                bytes.pop();
            }
            bytes
        });
        self.sources.insert(module.to_owned(), source);
    }

    pub(crate) fn overlay(&mut self, other: Self) {
        self.sources.extend(other.sources);
    }
}
