use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::Resource;
use fastfile_iw4::{AssetLinkSink, AssetType, Ptr, Result as ZoneResult, ZoneStream, load_zone};

use crate::{FamilyId, ZoneMemory, open_zone};

#[derive(Clone, Debug, Default, Resource)]
pub struct LocalizeCatalog {
    entries: HashMap<String, String>,
    raw_entries: HashMap<String, Vec<u8>>,
    by_namespace: HashMap<(asset_core::FamilyId, String), String>,
    raw_by_namespace: HashMap<(asset_core::FamilyId, String), Vec<u8>>,
}

impl LocalizeCatalog {
    pub fn text(&self, key: &str) -> Option<&str> {
        if let Ok(asset) = asset_core::AssetKey::parse(key)
            && asset.kind == asset_core::AssetKind::Localize
        {
            return self.text_in(asset.namespace, &asset.name);
        }
        self.entries.get(&key.to_uppercase()).map(String::as_str)
    }

    pub fn text_in(&self, namespace: asset_core::FamilyId, key: &str) -> Option<&str> {
        let key = key.to_uppercase();
        self.by_namespace
            .get(&(namespace, key.clone()))
            .or_else(|| self.entries.get(&key))
            .map(String::as_str)
    }

    pub fn raw_text(&self, key: &str) -> Option<&[u8]> {
        self.raw_entries.get(&key.to_uppercase()).map(Vec::as_slice)
    }

    pub fn raw_text_asset(&self, key: &asset_core::AssetKey) -> Option<&[u8]> {
        if key.kind != asset_core::AssetKind::Localize {
            return None;
        }
        self.raw_by_namespace
            .get(&(key.namespace, key.name.to_uppercase()))
            .map(Vec::as_slice)
    }

    pub fn text_asset(&self, key: &asset_core::AssetKey) -> Option<&str> {
        if key.kind != asset_core::AssetKind::Localize {
            return None;
        }
        self.by_namespace
            .get(&(key.namespace, key.name.to_uppercase()))
            .map(String::as_str)
    }

    pub fn absorb_in_namespace(&mut self, namespace: asset_core::FamilyId, other: LocalizeCatalog) {
        for (name, text) in &other.entries {
            self.by_namespace
                .entry((namespace, name.clone()))
                .or_insert_with(|| text.clone());
        }
        for (name, bytes) in &other.raw_entries {
            self.raw_by_namespace
                .entry((namespace, name.clone()))
                .or_insert_with(|| bytes.clone());
        }
        self.absorb(other);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn absorb(&mut self, other: LocalizeCatalog) {
        self.by_namespace.extend(other.by_namespace);
        self.raw_by_namespace.extend(other.raw_by_namespace);
        for (key, bytes) in other.raw_entries {
            self.raw_entries.entry(key).or_insert(bytes);
        }
        for (k, v) in other.entries {
            self.entries.entry(k).or_insert(v);
        }
    }

    pub fn insert_bytes(&mut self, name: &str, value: &[u8]) {
        if name.is_empty() {
            return;
        }
        self.entries
            .entry(name.to_uppercase())
            .or_insert_with(|| decode_localized_text(value));
        self.raw_entries
            .entry(name.to_uppercase())
            .or_insert_with(|| value.to_vec());
    }
}

const CP1252_HIGH: [u16; 32] = [
    0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039,
    0x0152, 0, 0x017d, 0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x02dc,
    0x2122, 0x0161, 0x203a, 0x0153, 0, 0x017e, 0x0178,
];

pub fn decode_localized_text(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    for &b in raw {
        match b {
            0x00..=0x7f => out.push(b as char),
            0x80..=0x9f => match CP1252_HIGH[(b - 0x80) as usize] {
                0 => out.push(char::REPLACEMENT_CHARACTER),
                cp => {
                    out.push(char::from_u32(u32::from(cp)).unwrap_or(char::REPLACEMENT_CHARACTER))
                }
            },
            _ => out.push(char::from(b)),
        }
    }
    out
}

#[derive(Default)]
struct LocalizeSink {
    catalog: LocalizeCatalog,
    walked: usize,
    sound: Option<asset_audio::ZoneSoundCapture>,
}

impl AssetLinkSink for LocalizeSink {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> ZoneResult<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.iw4_loaded(s, ty, slot, insert_slot);
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> ZoneResult<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.iw4_alias(ty, slot, target);
        }
        Ok(())
    }

    fn capture_localize(&mut self, name: &str, value: &[u8]) -> ZoneResult<()> {
        self.catalog.insert_bytes(name, value);
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ZoneResult<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        Ok(())
    }

    asset_audio::forward_iw4_sound!();
}

impl fastfile_iw4::AssetSink for LocalizeSink {
    fn set_script_strings(&mut self, _strings: fastfile_iw4::ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        _index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> ZoneResult<()> {
        fastfile_iw4::load_asset_at_observed(s, ty, slot, self)?;
        self.walked += 1;
        Ok(())
    }
}

pub fn load_localize_catalog(path: &Path) -> Result<LocalizeCatalog, String> {
    let image = open_zone(path).map_err(|e| e.to_string())?;
    let header = image.header().map_err(|e| e.to_string())?;
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = LocalizeSink {
        sound: asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::FamilyId::IW4,
            "localize",
        ),
        ..Default::default()
    };
    let walk = load_zone(&mut stream, &mut sink);
    if let Some(sound) = sink.sound.take() {
        sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
    }
    if let Err(e) = walk {
        diag::info!(
            Zone,
            "localize: {} stopped after {} assets ({} strings): {e}",
            path.display(),
            sink.walked,
            sink.catalog.len()
        );
    }
    Ok(sink.catalog)
}

const LOCALIZE_CACHE_FORMAT: u32 = 3;
const LOCALIZE_CACHE_MAGIC: u32 = 0x4c_4f_43_31;

fn localize_cache_key(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos() as u64;
    let mut hash = asset_transport::fnv1a64(path.to_string_lossy().as_bytes());
    hash = asset_transport::fnv1a64_more(hash, &meta.len().to_le_bytes());
    hash = asset_transport::fnv1a64_more(hash, &modified.to_le_bytes());
    Some(format!("{LOCALIZE_CACHE_FORMAT:08x}-{hash:016x}"))
}

impl LocalizeCatalog {
    fn cache_encode(&self) -> Vec<u8> {
        let mut rows = self.entries.iter().collect::<Vec<_>>();
        rows.sort_unstable();
        let mut out = Vec::with_capacity(16 + rows.len() * 48);
        out.extend_from_slice(&LOCALIZE_CACHE_MAGIC.to_le_bytes());
        out.extend_from_slice(&LOCALIZE_CACHE_FORMAT.to_le_bytes());
        out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        for (key, text) in rows {
            out.extend_from_slice(&(key.len() as u32).to_le_bytes());
            out.extend_from_slice(key.as_bytes());
            out.extend_from_slice(&(text.len() as u32).to_le_bytes());
            out.extend_from_slice(text.as_bytes());
            let raw = self.raw_entries.get(key).map(Vec::as_slice).unwrap_or(&[]);
            out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            out.extend_from_slice(raw);
        }
        out
    }

    fn cache_decode(blob: &[u8]) -> Option<Self> {
        let mut at = 0usize;
        let word = |at: &mut usize| -> Option<u32> {
            let end = at.checked_add(4)?;
            let value = u32::from_le_bytes(blob.get(*at..end)?.try_into().ok()?);
            *at = end;
            Some(value)
        };
        if word(&mut at)? != LOCALIZE_CACHE_MAGIC || word(&mut at)? != LOCALIZE_CACHE_FORMAT {
            return None;
        }
        let count = word(&mut at)? as usize;
        if count > blob.len().saturating_sub(at) / 12 {
            return None;
        }
        let mut entries = HashMap::with_capacity(count);
        let mut raw_entries = HashMap::with_capacity(count);
        for _ in 0..count {
            let key_len = word(&mut at)? as usize;
            let key_end = at.checked_add(key_len)?;
            let key = std::str::from_utf8(blob.get(at..key_end)?).ok()?.to_owned();
            at = key_end;
            let text_len = word(&mut at)? as usize;
            let text_end = at.checked_add(text_len)?;
            let text = std::str::from_utf8(blob.get(at..text_end)?)
                .ok()?
                .to_owned();
            at = text_end;
            let raw_len = word(&mut at)? as usize;
            let raw_end = at.checked_add(raw_len)?;
            let raw = blob.get(at..raw_end)?.to_vec();
            at = raw_end;
            if decode_localized_text(&raw) != text {
                return None;
            }
            raw_entries.insert(key.clone(), raw);
            entries.insert(key, text);
        }
        (at == blob.len() && entries.len() == count).then_some(Self {
            entries,
            raw_entries,
            ..Default::default()
        })
    }
}

pub fn load_localize_catalog_in_lane(path: &Path) -> Result<LocalizeCatalog, String> {
    let family = asset_transport::zone_game_for_path(path).ok_or_else(|| {
        format!(
            "localization path has no installed family: {}",
            path.display()
        )
    })?;
    let policy = localization_family(family)
        .ok_or_else(|| format!("no localization loader for {}", family.as_str()))?;
    let key = localize_cache_key(path);
    if let Some(hit) = cached_localize_catalog(key.as_deref()) {
        return Ok(hit);
    }

    let _flight = key
        .as_deref()
        .map(|key| asset_transport::cache_flight("localize", key));
    if let Some(hit) = cached_localize_catalog(key.as_deref()) {
        return Ok(hit);
    }
    let catalog = (policy.load)(path)?;
    if let Some(key) = &key
        && let Err(error) = asset_transport::cache_put("localize", key, &catalog.cache_encode())
    {
        diag::warn!(Zone, "localize cache store {key}: {error}");
    }
    Ok(catalog)
}

fn cached_localize_catalog(key: Option<&str>) -> Option<LocalizeCatalog> {
    LocalizeCatalog::cache_decode(&asset_transport::cache_get("localize", key?)?)
}

pub const MP_LOCALIZED_ZONES: &[&str] = &[
    "localized_code_post_gfx_mp",
    "localized_common_mp",
    "localized_ui_mp",
];

pub type LocalizationLoader = fn(&Path) -> Result<LocalizeCatalog, String>;
pub type LocalizationPlanner =
    fn(&asset_transport::GamesRoot, &Path, Option<&str>) -> Result<LocalizationSources, String>;
#[derive(Clone, Copy)]
pub struct LocalizationFamily {
    pub load: LocalizationLoader,
    pub plan: LocalizationPlanner,
    /// Lower values claim unqualified keys first; ties use family token order.
    pub priority: u16,
}
pub const REFERENCE_LOCALIZATION_FAMILY: LocalizationFamily = LocalizationFamily {
    load: load_localize_catalog,
    plan: reference_localization_plan,
    priority: 0,
};
fn reference_localization_plan(
    root: &asset_transport::GamesRoot,
    zone_ff: &Path,
    _language: Option<&str>,
) -> Result<LocalizationSources, String> {
    let mut plan = LocalizationSources::default();
    for name in MP_LOCALIZED_ZONES {
        match asset_transport::find_runtime_zone(root, zone_ff, name) {
            Ok(found) => plan.files.push(found),
            Err(error) => plan.gaps.push(format!(
                "localize: {name} missing ({error}) — its keys will report as gaps"
            )),
        }
    }
    Ok(plan)
}
#[derive(Default, Debug)]
pub struct LocalizationSources {
    pub files: Vec<asset_transport::ZoneFile>,
    /// Missing optional filenames, retained for consumers such as UI preparation.
    pub gaps: Vec<String>,
}
#[derive(Default)]
pub struct LocalizationFamilies(std::collections::BTreeMap<FamilyId, LocalizationFamily>);
impl LocalizationFamilies {
    pub fn register(&mut self, id: FamilyId, policy: LocalizationFamily) {
        assert!(
            self.0.insert(id, policy).is_none(),
            "duplicate localization family"
        );
    }
}
static FAMILIES: std::sync::OnceLock<LocalizationFamilies> = std::sync::OnceLock::new();
pub fn install_localization_families(families: LocalizationFamilies) -> Result<(), &'static str> {
    FAMILIES
        .set(families)
        .map_err(|_| "localization families already installed")
}
pub fn localization_family(id: FamilyId) -> Option<&'static LocalizationFamily> {
    FAMILIES.get()?.0.get(&id)
}
pub fn localization_families() -> impl Iterator<Item = (FamilyId, &'static LocalizationFamily)> {
    let mut policies: Vec<_> = FAMILIES
        .get()
        .into_iter()
        .flat_map(|f| f.0.iter())
        .map(|(id, policy)| (*id, policy))
        .collect();
    policies.sort_by_key(|(id, policy)| (policy.priority, *id));
    policies.into_iter()
}

#[derive(Default, Debug)]
pub struct LocalizationPlan {
    pub sources: Vec<(FamilyId, asset_transport::ZoneFile)>,
    pub report: Vec<String>,
}
pub fn plan_localization(
    root: &asset_transport::GamesRoot,
    zone_ff: &Path,
    language: Option<&str>,
) -> LocalizationPlan {
    let mut plan = LocalizationPlan::default();
    for (namespace, family) in localization_families() {
        match (family.plan)(root, zone_ff, language) {
            // The world pass counts discovered sources; per-file gaps remain for UI callers.
            Ok(found) => plan
                .sources
                .extend(found.files.into_iter().map(|found| (namespace, found))),
            Err(error) => plan.report.push(format!("localize gap: {error}")),
        }
    }
    plan
}

pub fn load_localized_strings_for_zone(
    games: &asset_transport::GamesRoot,
    zone: &str,
) -> Result<LocalizeCatalog, String> {
    let anchor = asset_transport::find_zone_file(games, zone)?;
    let namespace = asset_transport::zone_game_for_path(&anchor.path).ok_or_else(|| {
        format!(
            "localization path has no installed family: {}",
            anchor.path.display()
        )
    })?;
    let policy = localization_family(namespace)
        .ok_or_else(|| format!("no localization loader for {}", namespace.as_str()))?;
    let mut catalog = LocalizeCatalog::default();
    let plan = (policy.plan)(games, &anchor.path, None)?;
    for gap in plan.gaps {
        diag::warn!(Zone, "{gap}");
    }
    for found in plan.files {
        match load_localize_catalog_in_lane(&found.path) {
            Ok(part) => catalog.absorb_in_namespace(namespace, part),
            Err(error) => diag::warn!(
                Zone,
                "localize: {} failed to load ({error})",
                found.zone_name
            ),
        }
    }
    Ok(catalog)
}
