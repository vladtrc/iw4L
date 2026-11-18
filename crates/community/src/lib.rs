use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
pub type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Community {
    pub schema: u32,
    pub name: String,
    pub master: Master,
    pub updates: Updates,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Master {
    pub address: String,
    pub server_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Updates {
    pub url: String,
    pub ca_pem: String,
}

impl Community {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let community: Community =
            toml::from_str(std::str::from_utf8(bytes).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if community.schema != 1
            || community.name.trim().is_empty()
            || community.master.server_name.trim().is_empty()
            || community.master.address.trim().is_empty()
        {
            return Err("unsupported or incomplete community descriptor".into());
        }
        trust_roots(&community.updates.ca_pem)?;
        update_url(&community)?;
        Ok(community)
    }
}

pub fn trust_roots(pem: &str) -> Result<rustls::RootCertStore> {
    let certs = rustls_pemfile::certs(&mut std::io::Cursor::new(pem.as_bytes()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    if certs.is_empty() {
        return Err("community CA contains no certificates".into());
    }
    let mut roots = rustls::RootCertStore::empty();
    for cert in certs {
        roots.add(cert).map_err(|e| e.to_string())?;
    }
    Ok(roots)
}

pub fn update_url(community: &Community) -> Result<url::Url> {
    let url = url::Url::parse(&community.updates.url).map_err(|e| e.to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().ends_with("/manifest.toml")
    {
        return Err(
            "update URL must be an HTTPS manifest.toml without credentials, query or fragment"
                .into(),
        );
    }
    Ok(url)
}

#[derive(Clone, Debug)]
pub struct CommunityFile {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct CommunityContext {
    pub entries: Vec<CommunityFile>,
    pub selected_path: Option<PathBuf>,
    pub selected: Option<Community>,
}
struct InstalledContext {
    context: CommunityContext,
    restart: fn(&Path) -> Result<()>,
}
static CONTEXT: OnceLock<InstalledContext> = OnceLock::new();
pub fn install_context(context: CommunityContext, restart: fn(&Path) -> Result<()>) -> Result<()> {
    CONTEXT
        .set(InstalledContext { context, restart })
        .map_err(|_| "community context already installed".to_owned())
}
pub fn communities() -> &'static [CommunityFile] {
    CONTEXT
        .get()
        .map_or(&[], |installed| &installed.context.entries)
}
pub fn selected_path() -> Option<&'static Path> {
    CONTEXT.get()?.context.selected_path.as_deref()
}
pub fn selected() -> Option<&'static Community> {
    CONTEXT.get()?.context.selected.as_ref()
}
pub fn request_restart(path: &Path) -> Result<()> {
    let installed = CONTEXT.get().ok_or("community launcher is not installed")?;
    if !installed
        .context
        .entries
        .iter()
        .any(|entry| entry.path == path)
    {
        return Err("community is not in the installed server list".to_owned());
    }
    (installed.restart)(path)
}
