use std::collections::HashMap;

use asset_iw4::size as sz;
use asset_iw4::snd_alias::{
    SND_ALIAS_ALIAS_NAME, SND_ALIAS_CENTER_PERCENTAGE, SND_ALIAS_CHAIN, SND_ALIAS_DIST_MAX,
    SND_ALIAS_DIST_MIN, SND_ALIAS_ENVELOP_MAX, SND_ALIAS_ENVELOP_MIN, SND_ALIAS_ENVELOP_PERCENTAGE,
    SND_ALIAS_FLAGS, SND_ALIAS_LFE_PERCENTAGE, SND_ALIAS_MIXER_GROUP, SND_ALIAS_PITCH_MAX,
    SND_ALIAS_PITCH_MIN, SND_ALIAS_PROBABILITY, SND_ALIAS_SECONDARY, SND_ALIAS_SEQUENCE,
    SND_ALIAS_SLAVE_PERCENTAGE, SND_ALIAS_SOUND_FILE, SND_ALIAS_SPEAKER_MAP, SND_ALIAS_START_DELAY,
    SND_ALIAS_SUBTITLE, SND_ALIAS_VELOCITY_MIN, SND_ALIAS_VOL_MAX, SND_ALIAS_VOL_MIN,
    SND_ALIAS_VOLUME_FALLOFF_CURVE, SND_CURVE_DEFAULT_ASSET_NAME, SND_CURVE_KNOT_COUNT,
    SND_CURVE_KNOT_STRIDE, SND_CURVE_KNOTS, SND_CURVE_MAX_KNOTS, SND_ENTCHANNEL_FILE,
    SndAliasFlags,
};
use fastfile_iw4::{AssetLinkSink, AssetType, Ptr, Result, ZonePtr, ZoneStream};

use crate::asset_graph::{AssetEdge, AssetEdgeCensus, AssetEdgeReason, ZoneOwner};
use crate::ent_channel::{EntChannel, parse_ent_channel_file};
use crate::{AssetNamespace, ZoneGame};

pub use asset_iw4::{lerp_range, pick_weighted_variant_index, snd_advance_lcg, snd_unit_random};

pub type LoadedSoundEdge = crate::asset_graph::AssetEdge<crate::asset_graph::LoadedSoundSpace>;

pub type LoadedSoundEdgeReason = AssetEdgeReason;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SoundAliasKey {
    pub namespace: AssetNamespace,
    pub name: String,
}

impl SoundAliasKey {
    pub fn new(namespace: AssetNamespace, name: impl Into<String>) -> Self {
        Self {
            namespace,
            name: name.into(),
        }
    }

    pub fn host(name: impl Into<String>) -> Self {
        Self::new(AssetNamespace::Iw4, name)
    }
}

fn mint_revision() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn ns_of(game: ZoneGame) -> AssetNamespace {
    AssetNamespace::from_zone_game(game)
}

pub const MSS_PCM: i32 = 1;

#[derive(Clone, Debug, Default)]
pub struct LoadedSoundPcm {
    pub name: String,

    pub game: ZoneGame,
    pub(crate) format: i32,
    pub rate: u32,
    pub(crate) bits: i32,
    pub(crate) channels: i32,
    pub samples: u32,
    pub block_size: u32,
    pub(crate) pcm: std::sync::Arc<[u8]>,

    pub zone: ZoneOwner,

    pub seek_table: Vec<u32>,
}

impl LoadedSoundPcm {
    pub fn captured(
        name: impl Into<String>,
        format: i32,
        rate: u32,
        channels: i32,
        pcm: Vec<u8>,
        seek_table: Vec<u32>,
    ) -> Self {
        Self {
            name: name.into(),
            game: ZoneGame::Iw4,
            format,
            rate,
            bits: 16,
            channels,
            samples: 0,
            block_size: 0,
            pcm: pcm.into(),
            zone: ZoneOwner::default(),
            seek_table,
        }
    }

    pub fn t5_adpcm_bytes(&self) -> Option<&[u8]> {
        (self.format == 6).then_some(&self.pcm[..])
    }

    pub fn channels(&self) -> i32 {
        self.channels
    }

    /// Source bytes are immutable. Decoded samples and failures belong to the
    /// runtime ClipStore, never to this shared catalog entry.
    pub fn encoded_bytes(&self) -> &[u8] {
        &self.pcm
    }

    pub fn encoded_arc(&self) -> std::sync::Arc<[u8]> {
        self.pcm.clone()
    }

    pub fn format(&self) -> i32 {
        self.format
    }

    pub fn bits(&self) -> i32 {
        self.bits
    }

    pub fn is_t5_xwma(&self) -> bool {
        self.format == crate::sound_wma_t5::T5_WMA
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CapturedSndCurve {
    pub name: String,
    pub knots: Vec<(f32, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct CapturedAlias {
    pub alias_name: String,

    pub subtitle: Option<String>,

    pub secondary: Option<String>,

    pub chain: Option<String>,

    pub mixer_group: Option<String>,

    pub loaded_name: Option<String>,

    pub loaded: LoadedSoundEdge,

    pub streamed: Option<(String, String)>,

    pub file_type: Option<u8>,

    pub file_exists: Option<u8>,

    pub file_name: Option<String>,

    pub file_u: Option<&'static str>,

    pub file_u_ptr: Option<String>,

    pub file_u_deref: Option<&'static str>,

    pub sequence: i32,

    pub vol_min: f32,
    pub vol_max: f32,
    pub pitch_min: f32,
    pub pitch_max: f32,

    pub dist_min: f32,
    pub dist_max: f32,

    pub velocity_min: f32,

    pub flags: Option<u32>,

    pub slave_percentage: f32,

    pub probability: f32,

    pub lfe_percentage: f32,

    pub center_percentage: f32,

    pub start_delay: i32,

    pub volume_falloff: Option<CapturedSndCurve>,

    pub t5_distance_curves: Option<[u8; 2]>,

    pub envelop_min: f32,
    pub envelop_max: f32,
    pub envelop_percentage: f32,

    pub speaker_map: Option<String>,

    pub limit_count: Option<u8>,

    pub entity_limit_count: Option<u8>,
}

impl CapturedAlias {
    pub fn file_kind(&self) -> &'static str {
        match self.file_type {
            None => "none",
            Some(1) => "loaded",
            Some(2) => "streamed",
            Some(3) => "primed",
            Some(_) => "unknown",
        }
    }

    pub fn is_null_file(&self) -> bool {
        is_null_sound_name(self.file_name.as_deref())
            || self
                .streamed
                .as_ref()
                .is_some_and(|(dir, name)| dir.is_empty() && is_null_sound_name(Some(name)))
            || is_null_sound_name(self.loaded_name.as_deref())
    }

    pub fn loaded_present_name(&self) -> Option<&str> {
        self.loaded
            .is_bound()
            .then(|| self.loaded_name.as_deref())
            .flatten()
            .filter(|name| !name.is_empty())
    }

    pub fn decoded_flags(&self) -> Option<SndAliasFlags> {
        self.flags.map(SndAliasFlags::from_word)
    }
}

fn is_null_sound_name(name: Option<&str>) -> bool {
    let Some(name) = name else {
        return false;
    };
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    crate::AssetRef::bare_name(file).eq_ignore_ascii_case("null.wav")
}

fn capture_loaded_edge(
    sound_file: Option<ZonePtr>,
    file_type: Option<u8>,
    file_u: Option<&str>,
    file_u_deref: Option<&str>,
    loaded_name: Option<&str>,
) -> AssetEdge<crate::LoadedSoundSpace> {
    match sound_file {
        None | Some(ZonePtr::Null) => AssetEdge::Absent,
        Some(ZonePtr::Following) | Some(ZonePtr::Insert) => {
            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
        }
        Some(ZonePtr::Offset(_)) => match file_type {
            Some(2) | Some(3) => AssetEdge::Absent,
            Some(1) => {
                if is_null_sound_name(loaded_name) {
                    return AssetEdge::Absent;
                }
                match file_u {
                    Some("null") | None => AssetEdge::Absent,
                    Some("following") | Some("insert") => {
                        if loaded_name.is_some_and(|name| !name.is_empty()) {
                            AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
                        } else {
                            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
                        }
                    }
                    Some("offset") => {
                        let stub =
                            file_u_deref == Some("following") || file_u_deref == Some("insert");
                        if stub && !loaded_name.is_some_and(|name| !name.is_empty()) {
                            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
                        } else {
                            AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
                        }
                    }
                    _ => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                }
            }
            _ => AssetEdge::Absent,
        },
    }
}

#[derive(Clone, Debug, Default)]
pub struct CapturedSound {
    pub name: String,
    pub aliases: Vec<CapturedAlias>,

    pub game: ZoneGame,

    pub zone: ZoneOwner,
}

impl CapturedSound {
    pub fn ent_channel(&self, variant: usize) -> Option<u32> {
        match self.game {
            ZoneGame::T5 => None,
            ZoneGame::Iw4 | ZoneGame::Iw5 => self
                .aliases
                .get(variant)
                .and_then(|a| a.decoded_flags().map(|f| f.channel())),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PickLoadedOutcome<'a> {
    pub variant_index: usize,
    pub picked: Option<PickedSound<'a>>,
}

#[derive(Clone, Debug)]
pub struct PickedSound<'a> {
    pub sound: &'a LoadedSoundPcm,
    pub variant_index: usize,
    pub volume: f32,
    pub pitch: f32,

    pub layer: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SoundCatalog {
    pub sounds: Vec<CapturedSound>,
    pub loaded: Vec<LoadedSoundPcm>,

    pub curves: HashMap<String, CapturedSndCurve>,

    pub rawfiles: HashMap<(AssetNamespace, String), Vec<u8>>,

    pub ent_channels: Vec<EntChannel>,
    by_alias: HashMap<(AssetNamespace, String), usize>,

    by_alias_ci: HashMap<(AssetNamespace, String), usize>,
    by_loaded: HashMap<(AssetNamespace, String), usize>,

    file_to_loaded: HashMap<(u8, u32), String>,

    file_to_streamed: HashMap<(u8, u32), (String, String)>,

    loaded_by_insert: HashMap<(u8, u32), String>,

    curve_by_ptr: HashMap<(u8, u32), String>,
    last_loaded_name: Option<String>,
    last_curve_name: Option<String>,
    capture_game: ZoneGame,
    pub capture_gaps: usize,

    pub alias_flags_missing: usize,

    pub curve_capture_gaps: usize,
    capture_zone: ZoneOwner,

    revision: u64,
}

impl SoundCatalog {
    pub fn absorb(&mut self, other: SoundCatalog) {
        self.absorb_unresolved(other);
        self.finalize();
    }

    pub fn absorb_unresolved(&mut self, other: SoundCatalog) {
        for s in other.sounds {
            self.register_sound(s);
        }
        for l in other.loaded {
            self.register_loaded(l);
        }
        for (key, data) in other.rawfiles {
            self.rawfiles.entry(key).or_insert(data);
        }
        if self.ent_channels.is_empty() && !other.ent_channels.is_empty() {
            self.ent_channels = other.ent_channels;
        }
        for (name, curve) in other.curves {
            match self.curves.get_mut(&name) {
                Some(existing) if existing.knots.is_empty() && !curve.knots.is_empty() => {
                    *existing = curve;
                }
                None => {
                    self.curves.insert(name, curve);
                }
                Some(_) => {}
            }
        }
        for (k, v) in other.loaded_by_insert {
            self.loaded_by_insert.entry(k).or_insert(v);
        }
        for (k, v) in other.curve_by_ptr {
            self.curve_by_ptr.entry(k).or_insert(v);
        }
        self.capture_gaps += other.capture_gaps;
        self.alias_flags_missing += other.alias_flags_missing;
        self.curve_capture_gaps += other.curve_capture_gaps;

        self.resolve_ent_channels();
    }

    pub fn finalize(&mut self) {
        self.resolve_loaded_edges();
        self.resolve_curve_knots();
        self.resolve_ent_channels();
        self.publish();
    }

    pub fn absorb_missing_aliases(&mut self, other: SoundCatalog) {
        self.absorb_missing_aliases_unresolved(other);
        self.resolve_loaded_edges();
        self.resolve_curve_knots();
        self.publish();
    }

    pub fn absorb_missing_aliases_unresolved(&mut self, other: SoundCatalog) {
        for sound in other.sounds {
            if self.index_in(ns_of(sound.game), &sound.name).is_none() {
                self.register_sound(sound);
            }
        }
        for loaded in other.loaded {
            if self
                .loaded_index_in(ns_of(loaded.game), &loaded.name)
                .is_none()
            {
                self.register_loaded(loaded);
            }
        }
        for (key, data) in other.rawfiles {
            self.rawfiles.entry(key).or_insert(data);
        }
        if self.ent_channels.is_empty() && !other.ent_channels.is_empty() {
            self.ent_channels = other.ent_channels;
        }
        for (name, curve) in other.curves {
            match self.curves.get_mut(&name) {
                Some(existing) if existing.knots.is_empty() && !curve.knots.is_empty() => {
                    *existing = curve;
                }
                None => {
                    self.curves.insert(name, curve);
                }
                Some(_) => {}
            }
        }
        self.capture_gaps += other.capture_gaps;
    }

    pub fn publish(&mut self) {
        self.revision = mint_revision();
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn ingest_rawfile(&mut self, name: &str, data: &[u8], zlib_compressed: bool) {
        let bytes = if zlib_compressed {
            asset_transport::inflate_zlib(data).unwrap_or_else(|_| data.to_vec())
        } else {
            let mut bytes = data.to_vec();
            if bytes.last() == Some(&0) {
                bytes.pop();
            }
            bytes
        };
        if !name.is_empty() {
            self.rawfiles
                .insert((ns_of(self.capture_game), name.to_owned()), bytes);
        }
    }

    pub(crate) fn capture_zone_for_ingest(&self) -> ZoneOwner {
        self.capture_zone
    }

    pub fn ingest_loaded(&mut self, loaded: LoadedSoundPcm) {
        self.register_loaded(loaded);
    }

    pub fn ingest_sound(&mut self, mut sound: CapturedSound) {
        sound.game = self.capture_game;
        sound.zone = self.capture_zone;
        self.register_sound(sound);
    }

    pub(crate) fn ingest_curve(&mut self, curve: CapturedSndCurve) {
        self.insert_curve(curve);
    }

    pub fn resolve_loaded_edges(&mut self) {
        for i in 0..self.sounds.len() {
            let ns = ns_of(self.sounds[i].game);
            let alias_name = self.sounds[i].name.clone();
            for j in 0..self.sounds[i].aliases.len() {
                let null_file = self.sounds[i].aliases[j].is_null_file();
                let kind = self.sounds[i].aliases[j].file_kind();
                let hint = self.sounds[i].aliases[j].loaded_name.clone();
                let edge = self.sounds[i].aliases[j].loaded;
                if null_file {
                    if let Some(idx) = self.conventional_loaded_index(ns, &alias_name) {
                        self.sounds[i].aliases[j].loaded =
                            AssetEdge::bind_order(idx, self.zone_of_loaded(idx));
                    } else {
                        self.sounds[i].aliases[j].loaded = AssetEdge::Absent;
                    }
                    continue;
                }

                if matches!(
                    edge,
                    AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
                ) {
                    continue;
                }
                if let Some(idx) = hint
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .and_then(|name| self.loaded_index_in(ns, name))
                {
                    self.sounds[i].aliases[j].loaded =
                        AssetEdge::bind_order(idx, self.zone_of_loaded(idx));
                    continue;
                }
                if kind == "streamed" || kind == "primed" {
                    self.sounds[i].aliases[j].loaded = AssetEdge::Absent;
                    continue;
                }
                if self.sounds[i].aliases[j].loaded.is_bound() {
                    self.sounds[i].aliases[j].loaded =
                        AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss);
                }
            }
        }
    }

    pub fn loaded_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for sound in &self.sounds {
            for row in &sound.aliases {
                census.push(row.loaded);
            }
        }
        census
    }

    pub fn loaded_unresolved_hints(&self) -> Vec<&str> {
        self.sounds
            .iter()
            .filter(|sound| sound.aliases.iter().any(|row| row.loaded.is_unresolved()))
            .map(|sound| sound.name.as_str())
            .collect()
    }

    pub fn loaded_unresolved_reason_counts(&self) -> (usize, usize) {
        let mut temp = 0;
        let mut miss = 0;
        for sound in &self.sounds {
            for row in &sound.aliases {
                match row.loaded {
                    AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable) => temp += 1,
                    AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss) => miss += 1,
                    _ => {}
                }
            }
        }
        (temp, miss)
    }

    pub fn pcm_at(&self, index: usize) -> Option<&LoadedSoundPcm> {
        self.loaded.get(index)
    }

    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn set_capture_game(&mut self, game: ZoneGame) {
        self.capture_game = game;
    }

    pub fn zone_of_loaded(&self, index: usize) -> ZoneOwner {
        self.loaded
            .get(index)
            .map(|pcm| pcm.zone)
            .unwrap_or_default()
    }

    fn register_sound(&mut self, sound: CapturedSound) {
        if sound.name.is_empty() {
            return;
        }
        let ns = ns_of(sound.game);
        let idx = self.sounds.len();
        self.by_alias.insert((ns, sound.name.clone()), idx);
        self.by_alias_ci
            .insert((ns, sound.name.to_ascii_lowercase()), idx);
        self.sounds.push(sound);
    }

    fn register_loaded(&mut self, loaded: LoadedSoundPcm) {
        if loaded.name.is_empty() {
            return;
        }
        let ns = ns_of(loaded.game);
        let idx = self.loaded.len();
        self.by_loaded.insert((ns, loaded.name.clone()), idx);
        self.loaded.push(loaded);
    }

    pub fn index_in(&self, ns: AssetNamespace, alias: &str) -> Option<usize> {
        if let Some(&i) = self.by_alias.get(&(ns, alias.to_owned())) {
            return Some(i);
        }
        self.by_alias_ci
            .get(&(ns, alias.to_ascii_lowercase()))
            .copied()
    }

    pub fn index_by_name(&self, alias: &str) -> Option<usize> {
        self.index_in(AssetNamespace::Iw4, alias)
    }

    pub fn index_unique(&self, alias: &str) -> Option<usize> {
        let mut found = None;
        for ns in [AssetNamespace::Iw4, AssetNamespace::T5, AssetNamespace::Iw5] {
            if let Some(i) = self.index_in(ns, alias) {
                if found.is_some() {
                    return None;
                }
                found = Some(i);
            }
        }
        found
    }

    pub fn sound_in(&self, ns: AssetNamespace, alias: &str) -> Option<&CapturedSound> {
        self.index_in(ns, alias).map(|i| &self.sounds[i])
    }

    pub fn sound(&self, alias: &str) -> Option<&CapturedSound> {
        self.sound_in(AssetNamespace::Iw4, alias)
    }

    pub fn loaded_index_in(&self, ns: AssetNamespace, name: &str) -> Option<usize> {
        let bare = crate::AssetRef::bare_name(name);
        self.by_loaded.get(&(ns, bare.to_owned())).copied()
    }

    fn conventional_loaded_index(&self, ns: AssetNamespace, alias: &str) -> Option<usize> {
        if let Some(idx) = self.loaded_index_in(ns, alias) {
            return Some(idx);
        }
        let wav = format!("/{alias}.wav");
        self.loaded.iter().enumerate().find_map(|(idx, loaded)| {
            (ns_of(loaded.game) == ns && loaded.name.ends_with(&wav)).then_some(idx)
        })
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.sounds.get(index).map(|s| s.name.as_str())
    }

    pub fn namespace_of_alias(&self, index: usize) -> AssetNamespace {
        self.sounds
            .get(index)
            .map(|s| ns_of(s.game))
            .unwrap_or_default()
    }

    pub fn zone_of_alias(&self, index: usize) -> ZoneOwner {
        self.sounds.get(index).map(|s| s.zone).unwrap_or_default()
    }

    fn loaded_name_for_offset(&self, s: &ZoneStream<'_>, p: Ptr) -> Option<String> {
        self.loaded_by_insert
            .get(&file_key(p))
            .cloned()
            .or_else(|| {
                self.loaded_by_insert
                    .get(&file_key(s.resolve_alias(p)))
                    .cloned()
            })
    }

    fn curve_with_knots(&self, key: &str) -> Option<&CapturedSndCurve> {
        self.curves.get(key).filter(|c| !c.knots.is_empty())
    }

    fn curve_lookup(&self, name: &str) -> Option<CapturedSndCurve> {
        if let Some(curve) = self.curve_with_knots(name) {
            return Some(curve.clone());
        }
        let bare = crate::AssetRef::bare_name(name);
        if let Some(curve) = self.curve_with_knots(bare) {
            let mut out = curve.clone();
            out.name = name.to_owned();
            return Some(out);
        }

        if bare == "$default" {
            if let Some(curve) = self.curve_with_knots(SND_CURVE_DEFAULT_ASSET_NAME) {
                let mut out = curve.clone();
                out.name = name.to_owned();
                return Some(out);
            }
        }
        None
    }

    pub fn ent_channel(&self, channel: u32) -> Option<&EntChannel> {
        self.ent_channels.get(channel as usize)
    }

    pub fn resolve_ent_channels(&mut self) {
        if !self.ent_channels.is_empty() {
            return;
        }
        let Some(bytes) = self.rawfile_named(SND_ENTCHANNEL_FILE) else {
            return;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            return;
        };
        match parse_ent_channel_file(text) {
            Ok(rows) => self.ent_channels = rows,
            Err(e) => diag::warn!(Zone, "entchannel parse: {e}"),
        }
    }

    fn rawfile_named(&self, want: &str) -> Option<&[u8]> {
        for ns in [AssetNamespace::Iw4, AssetNamespace::T5, AssetNamespace::Iw5] {
            if let Some(data) = self.rawfiles.get(&(ns, want.to_owned())) {
                return Some(data.as_slice());
            }
        }
        self.rawfiles.iter().find_map(|((_, name), data)| {
            let n = name.replace('\\', "/");
            n.eq_ignore_ascii_case(want)
                .then_some(data.as_slice())
                .or_else(|| {
                    n.rsplit('/')
                        .next()
                        .is_some_and(|leaf| leaf.eq_ignore_ascii_case("channels.def"))
                        .then_some(data.as_slice())
                })
        })
    }

    fn insert_curve(&mut self, curve: CapturedSndCurve) {
        if curve.knots.is_empty() {
            let bare = crate::AssetRef::bare_name(&curve.name);
            if self.curves.get(bare).is_some_and(|c| !c.knots.is_empty())
                || self
                    .curves
                    .get(&curve.name)
                    .is_some_and(|c| !c.knots.is_empty())
            {
                return;
            }
        }
        self.curves.insert(curve.name.clone(), curve);
    }

    fn curve_name_for_offset(&self, s: &ZoneStream<'_>, p: Ptr) -> Option<String> {
        self.curve_by_ptr.get(&file_key(p)).cloned().or_else(|| {
            self.curve_by_ptr
                .get(&file_key(s.resolve_alias(p)))
                .cloned()
        })
    }

    fn curve_for_row(&mut self, s: &ZoneStream<'_>, row: Ptr) -> Option<CapturedSndCurve> {
        let field = row.at(s.layout(SND_ALIAS_VOLUME_FALLOFF_CURVE, 104));
        let named = self.curve_by_ptr.get(&file_key(field)).cloned();
        if let Some(name) = named.as_deref()
            && let Some(curve) = self.curve_lookup(name)
        {
            return Some(curve);
        }
        match s
            .ptr_at(row, s.layout(SND_ALIAS_VOLUME_FALLOFF_CURVE, 104))
            .ok()
        {
            None | Some(ZonePtr::Null) => None,
            Some(ZonePtr::Offset(p)) => {
                let header = s.resolve_alias(p);
                if let Some(curve) = read_curve_header(s, header) {
                    if let Some(filled) = self.curve_lookup(&curve.name) {
                        return Some(filled);
                    }
                    self.insert_curve(curve.clone());
                    return Some(curve);
                }
                let name = self.curve_name_for_offset(s, p).or(named);
                match name.as_deref().and_then(|n| self.curve_lookup(n)) {
                    Some(curve) => Some(curve),
                    None => name.map(|name| CapturedSndCurve {
                        name,
                        knots: Vec::new(),
                    }),
                }
            }
            Some(_) => named.map(|name| CapturedSndCurve {
                name,
                knots: Vec::new(),
            }),
        }
    }

    pub fn resolve_curve_knots(&mut self) {
        let mut remaining = 0usize;
        for i in 0..self.sounds.len() {
            for j in 0..self.sounds[i].aliases.len() {
                if self.sounds[i].aliases[j].volume_falloff.is_none()
                    && let Some([dry, near]) = self.sounds[i].aliases[j].t5_distance_curves
                {
                    let dry = self.curves.get(&format!("t5/curve/{dry}"));
                    let near = self.curves.get(&format!("t5/curve/{near}"));

                    if let (Some(dry), Some(near)) = (dry, near)
                        && near
                            .knots
                            .iter()
                            .position(|&(x, _)| x >= 1.0)
                            .is_some_and(|end| near.knots[..=end].iter().all(|&(_, y)| y == 1.0))
                        && dry.knots.first() == Some(&(0.0, 1.0))
                        && dry.knots.last() == Some(&(1.0, 0.0))
                    {
                        self.sounds[i].aliases[j].volume_falloff = Some(dry.clone());
                    } else {
                        remaining += 1;
                    }
                }
                let Some(cur) = self.sounds[i].aliases[j].volume_falloff.clone() else {
                    continue;
                };
                if !cur.knots.is_empty() {
                    continue;
                }
                match self.curve_lookup(&cur.name) {
                    Some(filled) => {
                        self.sounds[i].aliases[j].volume_falloff = Some(filled);
                    }
                    None => remaining += 1,
                }
            }
        }
        self.curve_capture_gaps = remaining;
    }

    fn offset_deref_kind(s: &ZoneStream<'_>, p: Ptr) -> Option<&'static str> {
        let v = s.u32_at(p, 0).ok()?;
        zone_ptr_kind(Some(ZonePtr::decode(v)))
    }

    pub fn loaded_for_alias(&self, ns: AssetNamespace, alias: &str) -> Option<&LoadedSoundPcm> {
        self.loaded_for_alias_depth(ns, alias, 0)
    }

    fn loaded_for_alias_depth(
        &self,
        ns: AssetNamespace,
        alias: &str,
        depth: u8,
    ) -> Option<&LoadedSoundPcm> {
        let sound = self.sound_in(ns, alias)?;
        for row in &sound.aliases {
            if let Some(pcm) = row
                .loaded
                .bound_index()
                .and_then(|index| self.pcm_at(index))
            {
                return Some(pcm);
            }
        }
        if depth < 2 {
            for row in &sound.aliases {
                if let Some(sec) = &row.secondary
                    && !sec.is_empty()
                    && sec != alias
                {
                    if let Some(pcm) = self.loaded_for_alias_depth(ns, sec, depth + 1) {
                        return Some(pcm);
                    }
                }
            }
        }

        None
    }

    pub fn pcm_for_variant(
        &self,
        ns: AssetNamespace,
        alias: &str,
        variant: usize,
    ) -> Option<&LoadedSoundPcm> {
        let sound = self.sound_in(ns, alias)?;
        let row = sound.aliases.get(variant)?;
        row.loaded
            .bound_index()
            .and_then(|index| self.pcm_at(index))
    }

    pub fn streamed_for_variant(
        &self,
        ns: AssetNamespace,
        alias: &str,
        variant: usize,
    ) -> Option<(AssetNamespace, String, String)> {
        let sound = self.sound_in(ns, alias)?;
        let row = sound.aliases.get(variant)?;
        self.streamed_from_row(ns, row)
    }

    fn streamed_from_row(
        &self,
        ns: AssetNamespace,
        row: &CapturedAlias,
    ) -> Option<(AssetNamespace, String, String)> {
        let (dir, name) = row.streamed.as_ref()?;
        if name.is_empty() || name == ",null.wav" {
            return None;
        }
        if ns == AssetNamespace::T5 && dir.is_empty() {
            let path = name.replace('\\', "/");
            if let Some(relative) = path.strip_prefix("sound/")
                && let Some((directory, file)) = relative.rsplit_once('/')
            {
                return Some((ns, directory.to_owned(), file.to_owned()));
            }
            return None;
        }
        if dir.is_empty() {
            return None;
        }
        Some((ns, dir.clone(), name.clone()))
    }

    pub fn streamed_sound_for_alias(
        &self,
        ns: AssetNamespace,
        alias: &str,
    ) -> Option<(AssetNamespace, String, String)> {
        self.streamed_sound_for_alias_depth(ns, alias, 0)
    }

    fn streamed_sound_for_alias_depth(
        &self,
        ns: AssetNamespace,
        alias: &str,
        depth: u8,
    ) -> Option<(AssetNamespace, String, String)> {
        let sound = self.sound_in(ns, alias)?;
        for row in &sound.aliases {
            if let Some(path) = self.streamed_from_row(ns, row) {
                return Some(path);
            }
        }
        if depth < 2 {
            for row in &sound.aliases {
                if let Some(sec) = &row.secondary
                    && !sec.is_empty()
                    && sec != alias
                {
                    if let Some(path) = self.streamed_sound_for_alias_depth(ns, sec, depth + 1) {
                        return Some(path);
                    }
                }
            }
        }
        None
    }

    fn rawfile_text_in(&self, ns: AssetNamespace, name: &str) -> Option<&str> {
        let data = self.rawfiles.get(&(ns, name.to_owned()))?;
        std::str::from_utf8(data).ok()
    }

    fn rawfile_text_unique(&self, name: &str) -> Option<&str> {
        let mut found = None;
        for ns in [AssetNamespace::Iw4, AssetNamespace::T5, AssetNamespace::Iw5] {
            if let Some(text) = self.rawfile_text_in(ns, name) {
                if found.is_some() {
                    return None;
                }
                found = Some(text);
            }
        }
        found
    }

    pub fn rawfile_text(&self, name: &str) -> Option<&str> {
        std::str::from_utf8(self.rawfile_bytes(name)?).ok()
    }

    pub fn rawfile_bytes(&self, name: &str) -> Option<&[u8]> {
        self.rawfiles
            .get(&(AssetNamespace::Iw4, name.to_owned()))
            .map(Vec::as_slice)
            .or_else(|| {
                let mut found = None;
                for ns in [AssetNamespace::T5, AssetNamespace::Iw5] {
                    if let Some(data) = self.rawfiles.get(&(ns, name.to_owned())) {
                        if found.is_some() {
                            return None;
                        }
                        found = Some(data.as_slice());
                    }
                }
                found
            })
    }

    fn rawfile_text_for_map(&self, ns: AssetNamespace, name: &str) -> Option<&str> {
        self.rawfile_text_in(ns, name)
            .or_else(|| self.rawfile_text_unique(name))
    }

    pub fn createfx_loop_sounds(
        &self,
        ns: AssetNamespace,
        map: &str,
    ) -> Vec<crate::createfx::CreateFxLoopSound> {
        let stem = map.strip_prefix("maps/mp/").unwrap_or(map);
        let stem = stem.strip_suffix(".d3dbsp").unwrap_or(stem);
        let path = format!("maps/createfx/{stem}_fx.gsc");
        let Some(src) = self.rawfile_text_for_map(ns, &path) else {
            let alt = format!("maps/mp/{stem}_fx.gsc");
            return self
                .rawfile_text_for_map(ns, &alt)
                .map(crate::createfx::parse_createfx_loop_sounds)
                .unwrap_or_default();
        };
        crate::createfx::parse_createfx_loop_sounds(src)
    }

    pub fn createfx_oneshots(
        &self,
        ns: AssetNamespace,
        map: &str,
    ) -> Vec<crate::createfx::CreateFxOneshot> {
        let stem = map.strip_prefix("maps/mp/").unwrap_or(map);
        let stem = stem.strip_suffix(".d3dbsp").unwrap_or(stem);
        let createfx_path = format!("maps/createfx/{stem}_fx.gsc");
        let mp_fx_path = format!("maps/mp/{stem}_fx.gsc");
        let src = self
            .rawfile_text_for_map(ns, &createfx_path)
            .or_else(|| self.rawfile_text_for_map(ns, &mp_fx_path));
        let Some(src) = src else {
            return Vec::new();
        };
        let mut shots = crate::createfx::parse_createfx_oneshots(src);
        if let Some(alias_src) = self.rawfile_text_for_map(ns, &mp_fx_path) {
            let aliases = crate::createfx::parse_createfx_effect_aliases(alias_src);
            crate::createfx::apply_createfx_effect_aliases(&mut shots, &aliases);
        }
        shots
    }

    pub fn pick_loaded_outcome<'a>(
        &'a self,
        ns: AssetNamespace,
        alias: &str,
        rng: &mut u32,
        avoid: Option<usize>,
    ) -> Option<PickLoadedOutcome<'a>> {
        self.pick_loaded_outcome_depth(ns, alias, rng, avoid, 0)
    }

    fn pick_loaded_outcome_depth<'a>(
        &'a self,
        ns: AssetNamespace,
        alias: &str,
        rng: &mut u32,
        avoid: Option<usize>,
        depth: u8,
    ) -> Option<PickLoadedOutcome<'a>> {
        let sound = self.sound_in(ns, alias)?;
        if sound.aliases.is_empty() {
            return Some(PickLoadedOutcome {
                variant_index: 0,
                picked: None,
            });
        }
        let weights: Vec<f32> = sound
            .aliases
            .iter()
            .map(|a| {
                if a.probability > 0.0 {
                    a.probability
                } else {
                    1.0
                }
            })
            .collect();
        let index = pick_weighted_variant_index(&weights, rng, avoid);
        let row = &sound.aliases[index];
        let own_pcm = row
            .loaded
            .bound_index()
            .and_then(|index| self.pcm_at(index));
        let layer = own_pcm.is_some().then(|| row.secondary.clone()).flatten();
        let pcm = own_pcm.or_else(|| {
            if depth >= 10 {
                return None;
            }
            row.secondary.as_deref().and_then(|sec| {
                self.pick_loaded_outcome_depth(ns, sec, rng, None, depth + 1)
                    .and_then(|outcome| outcome.picked)
                    .map(|picked| picked.sound)
            })
        });
        let Some(pcm) = pcm else {
            return Some(PickLoadedOutcome {
                variant_index: index,
                picked: None,
            });
        };
        let t_vol = snd_unit_random(rng);
        let t_pitch = snd_unit_random(rng);
        let volume = if row.vol_min == 0.0 && row.vol_max == 0.0 {
            1.0
        } else {
            lerp_range(row.vol_min, row.vol_max, t_vol)
        };
        let pitch = if row.pitch_min == 0.0 && row.pitch_max == 0.0 {
            1.0
        } else {
            lerp_range(row.pitch_min, row.pitch_max, t_pitch)
        };
        Some(PickLoadedOutcome {
            variant_index: index,
            picked: Some(PickedSound {
                sound: pcm,
                variant_index: index,
                volume,
                pitch,
                layer,
            }),
        })
    }
}

fn zone_ptr_kind(p: Option<ZonePtr>) -> Option<&'static str> {
    Some(match p? {
        ZonePtr::Null => "null",
        ZonePtr::Offset(_) => "offset",
        ZonePtr::Following => "following",
        ZonePtr::Insert => "insert",
    })
}

fn zone_ptr_label(p: Option<ZonePtr>) -> Option<String> {
    match p? {
        ZonePtr::Offset(q) => Some(format!("{}:{}", q.block, q.offset)),
        _ => None,
    }
}

fn file_key(p: Ptr) -> (u8, u32) {
    (p.block, p.offset)
}

struct InspectedSoundFile {
    file_type: Option<u8>,
    file_exists: Option<u8>,
    loaded_name: Option<String>,
    streamed: Option<(String, String)>,
    file_name: Option<String>,
}

fn inspect_sound_file(s: &ZoneStream<'_>, file: Ptr) -> InspectedSoundFile {
    let file_type = s.u8_at(file, 0).ok();
    let file_exists = s.u8_at(file, 1).ok();
    match file_type {
        Some(1) => {
            let loaded_name = match s.ptr_at(file, s.layout(4, 8)).ok() {
                Some(ZonePtr::Offset(p)) => {
                    let target = s.resolve_alias(p);

                    match s.u32_at(target, 0).ok().map(ZonePtr::decode) {
                        Some(ZonePtr::Following) | Some(ZonePtr::Insert) => None,
                        _ => name_at(s, target, 0),
                    }
                }
                _ => None,
            };
            InspectedSoundFile {
                file_type,
                file_exists,
                file_name: loaded_name.clone(),
                loaded_name,
                streamed: None,
            }
        }
        Some(2) | Some(3) => {
            let dir = name_at(s, file, s.layout(4, 8)).unwrap_or_default();
            let name = name_at(s, file, s.layout(8, 16)).unwrap_or_default();
            let file_name = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            let streamed = (!name.is_empty()).then_some((dir, name));
            InspectedSoundFile {
                file_type,
                file_exists,
                loaded_name: None,
                streamed,
                file_name: (!file_name.is_empty()).then_some(file_name),
            }
        }
        _ => InspectedSoundFile {
            file_type,
            file_exists,
            loaded_name: None,
            streamed: None,
            file_name: None,
        },
    }
}

fn name_at(s: &ZoneStream<'_>, parent: Ptr, field: usize) -> Option<String> {
    match s.ptr_at(parent, field).ok()? {
        ZonePtr::Offset(p) => s.cstr(s.resolve_alias(p)).ok().map(str::to_owned),
        ZonePtr::Null => Some(String::new()),
        _ => None,
    }
}

fn optional_name(s: &ZoneStream<'_>, parent: Ptr, field: usize) -> Option<String> {
    name_at(s, parent, field).filter(|n| !n.is_empty())
}

fn speaker_map_name(s: &ZoneStream<'_>, row: Ptr) -> Option<String> {
    match s.ptr_at(row, s.layout(SND_ALIAS_SPEAKER_MAP, 128)).ok()? {
        ZonePtr::Offset(p) => optional_name(s, s.resolve_alias(p), s.layout(4, 8)),
        _ => None,
    }
}

fn read_curve_header(s: &ZoneStream<'_>, header: Ptr) -> Option<CapturedSndCurve> {
    let name = name_at(s, header, 0).filter(|n| !n.is_empty())?;
    let count = s.u16_at(header, s.layout(SND_CURVE_KNOT_COUNT, 8)).ok()? as usize;
    let n = count.min(SND_CURVE_MAX_KNOTS);
    let mut knots = Vec::with_capacity(n);
    for i in 0..n {
        let off = s.layout(SND_CURVE_KNOTS, 12) + i * SND_CURVE_KNOT_STRIDE;
        let x = s.f32_at(header, off).ok()?;
        let y = s.f32_at(header, off + 4).ok()?;
        knots.push((x, y));
    }
    Some(CapturedSndCurve { name, knots })
}

impl AssetLinkSink for SoundCatalog {
    fn loaded(
        &mut self,
        _s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> Result<()> {
        match ty {
            AssetType::LoadedSound => {
                let Some(name) = self.last_loaded_name.clone() else {
                    return Ok(());
                };
                if let Some(ins) = insert_slot {
                    self.loaded_by_insert.insert(file_key(ins), name);
                }
            }
            AssetType::SoundCurve => {
                let Some(name) = self.last_curve_name.clone() else {
                    return Ok(());
                };
                self.curve_by_ptr.insert(file_key(slot), name.clone());
                if let Some(ins) = insert_slot {
                    self.curve_by_ptr.insert(file_key(ins), name);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> Result<()> {
        if ty == AssetType::SoundCurve
            && let Some(name) = self.curve_by_ptr.get(&file_key(target)).cloned()
        {
            self.curve_by_ptr.insert(file_key(slot), name);
        }
        Ok(())
    }

    fn capture_loaded_sound(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
        pcm: Ptr,
        data_len: usize,
    ) -> Result<()> {
        let name = name_at(s, header, 0).unwrap_or_default();

        let (format, rate, bits, channels, samples, block_size) =
            if s.wire_format() == fastfile_iw4::Iw4WireFormat::X64 {
                let format = i32::from(s.u16_at(header, 8)?);
                let channels = i32::from(s.u16_at(header, 10)?);
                let rate = s.u32_at(header, 12)?;
                let block_size = u32::from(s.u16_at(header, 20)?);
                let bits = i32::from(s.u16_at(header, 22)?);
                let samples = if format == MSS_PCM && block_size != 0 {
                    (data_len / block_size as usize) as u32
                } else {
                    0
                };
                (format, rate, bits, channels, samples, block_size)
            } else {
                (
                    s.i32_at(header, 4)?,
                    s.u32_at(header, 16)?,
                    s.i32_at(header, 20)?,
                    s.i32_at(header, 24)?,
                    s.u32_at(header, 28)?,
                    s.u32_at(header, 32)?,
                )
            };
        let pcm_bytes = s.slice_at(pcm, 0, data_len)?.to_vec();

        if name.is_empty() {
            self.capture_gaps += 1;
            self.last_loaded_name = None;
            return Ok(());
        }
        self.last_loaded_name = Some(name.clone());
        if pcm_bytes.is_empty() {
            if !is_null_sound_name(Some(&name)) {
                self.capture_gaps += 1;
            }
            return Ok(());
        }
        self.register_loaded(LoadedSoundPcm {
            name,
            game: self.capture_game,
            format,
            rate,
            bits,
            channels,
            samples,
            block_size,
            pcm: pcm_bytes.into(),
            zone: self.capture_zone,
            ..Default::default()
        });
        Ok(())
    }

    fn bind_last_loaded_to_sound_file(&mut self, file: Ptr) -> Result<()> {
        if let Some(name) = self.last_loaded_name.clone() {
            self.file_to_loaded.insert(file_key(file), name);
        } else {
            self.capture_gaps += 1;
        }
        Ok(())
    }

    fn bind_streamed_sound_file(&mut self, file: Ptr, dir: &str, name: &str) -> Result<()> {
        self.file_to_streamed
            .insert(file_key(file), (dir.to_owned(), name.to_owned()));
        Ok(())
    }

    fn capture_snd_curve(&mut self, s: &ZoneStream<'_>, header: Ptr) -> Result<()> {
        let name = name_at(s, header, 0).unwrap_or_default();
        if name.is_empty() {
            self.capture_gaps += 1;
        }
        let count = s
            .u16_at(header, s.layout(SND_CURVE_KNOT_COUNT, 8))
            .unwrap_or(0) as usize;
        if count > SND_CURVE_MAX_KNOTS {
            self.capture_gaps += 1;
        }
        let n = count.min(SND_CURVE_MAX_KNOTS);
        let mut knots = Vec::with_capacity(n);
        for i in 0..n {
            let off = s.layout(SND_CURVE_KNOTS, 12) + i * SND_CURVE_KNOT_STRIDE;
            let x = s.f32_at(header, off).unwrap_or(0.0);
            let y = s.f32_at(header, off + 4).unwrap_or(0.0);
            knots.push((x, y));
        }
        self.last_curve_name = Some(name.clone());
        self.curve_by_ptr.insert(file_key(header), name.clone());
        self.insert_curve(CapturedSndCurve { name, knots });
        Ok(())
    }

    fn capture_raw_file(&mut self, name: &str, data: &[u8], zlib_compressed: bool) -> Result<()> {
        self.ingest_rawfile(name, data, zlib_compressed);
        Ok(())
    }

    fn capture_sound(
        &mut self,
        s: &ZoneStream<'_>,
        list: Ptr,
        count: usize,
        head: Option<Ptr>,
    ) -> Result<()> {
        let name = name_at(s, list, 0).unwrap_or_default();
        let Some(arr) = head else {
            if count > 0 {
                self.capture_gaps += 1;
            }
            return Ok(());
        };

        let mut aliases = Vec::with_capacity(count);
        for i in 0..count {
            let row = arr.at(i * s.layout(sz::SND_ALIAS, 136));
            let alias_name = name_at(s, row, s.layout(SND_ALIAS_ALIAS_NAME, 0)).unwrap_or_default();
            let subtitle = optional_name(s, row, s.layout(SND_ALIAS_SUBTITLE, 8));
            let secondary = optional_name(s, row, s.layout(SND_ALIAS_SECONDARY, 16));
            let chain = optional_name(s, row, s.layout(SND_ALIAS_CHAIN, 24));
            let mixer_group = optional_name(s, row, s.layout(SND_ALIAS_MIXER_GROUP, 32));
            let (
                loaded_name,
                streamed,
                file_type,
                file_exists,
                file_name,
                file_u,
                file_u_ptr,
                file_u_deref,
            ) = match s.ptr_at(row, s.layout(SND_ALIAS_SOUND_FILE, 40)).ok() {
                Some(ZonePtr::Offset(p)) => {
                    let file = s.resolve_alias(p);
                    let key = file_key(file);
                    let loaded = self.file_to_loaded.get(&key).cloned();
                    let streamed = self.file_to_streamed.get(&key).cloned();
                    let inspected = inspect_sound_file(s, file);
                    let u = s.ptr_at(file, s.layout(4, 8)).ok();
                    let loaded = loaded.or_else(|| match u {
                        Some(ZonePtr::Offset(q)) => self.loaded_name_for_offset(s, q),
                        _ => None,
                    });
                    let file_u_deref = match u {
                        Some(ZonePtr::Offset(q)) => Self::offset_deref_kind(s, q),
                        _ => None,
                    };

                    let loaded = if file_u_deref == Some("following")
                        || file_u_deref == Some("insert")
                        || matches!(u, Some(ZonePtr::Following) | Some(ZonePtr::Insert))
                    {
                        loaded
                    } else {
                        loaded.or(inspected.loaded_name)
                    };
                    let streamed = streamed.or(inspected.streamed);
                    let file_name = loaded.clone().or(inspected.file_name.clone()).or_else(|| {
                        streamed.as_ref().map(|(dir, name)| {
                            if dir.is_empty() {
                                name.clone()
                            } else {
                                format!("{dir}/{name}")
                            }
                        })
                    });
                    (
                        loaded,
                        streamed,
                        inspected.file_type,
                        inspected.file_exists,
                        file_name,
                        zone_ptr_kind(u),
                        zone_ptr_label(u),
                        file_u_deref,
                    )
                }
                Some(ZonePtr::Null) => (None, None, None, None, None, Some("null"), None, None),
                _ => (
                    None,
                    None,
                    Some(0),
                    None,
                    None,
                    zone_ptr_kind(s.ptr_at(row, s.layout(SND_ALIAS_SOUND_FILE, 40)).ok()),
                    zone_ptr_label(s.ptr_at(row, s.layout(SND_ALIAS_SOUND_FILE, 40)).ok()),
                    None,
                ),
            };
            let flags = s.u32_at(row, s.layout(SND_ALIAS_FLAGS, 80)).ok();
            if flags.is_none() {
                self.alias_flags_missing += 1;
            }
            let volume_falloff = self.curve_for_row(s, row);
            let sound_file = s.ptr_at(row, s.layout(SND_ALIAS_SOUND_FILE, 40)).ok();
            let loaded = capture_loaded_edge(
                sound_file,
                file_type,
                file_u,
                file_u_deref,
                loaded_name.as_deref(),
            );
            aliases.push(CapturedAlias {
                alias_name,
                subtitle,
                secondary,
                chain,
                mixer_group,
                loaded_name,
                loaded,
                streamed,
                file_type,
                file_exists,
                file_name,
                file_u,
                file_u_ptr,
                file_u_deref,
                sequence: s.i32_at(row, s.layout(SND_ALIAS_SEQUENCE, 48)).unwrap_or(0),
                vol_min: s
                    .f32_at(row, s.layout(SND_ALIAS_VOL_MIN, 52))
                    .unwrap_or(0.0),
                vol_max: s
                    .f32_at(row, s.layout(SND_ALIAS_VOL_MAX, 56))
                    .unwrap_or(0.0),
                pitch_min: s
                    .f32_at(row, s.layout(SND_ALIAS_PITCH_MIN, 60))
                    .unwrap_or(0.0),
                pitch_max: s
                    .f32_at(row, s.layout(SND_ALIAS_PITCH_MAX, 64))
                    .unwrap_or(0.0),
                dist_min: s
                    .f32_at(row, s.layout(SND_ALIAS_DIST_MIN, 68))
                    .unwrap_or(0.0),
                dist_max: s
                    .f32_at(row, s.layout(SND_ALIAS_DIST_MAX, 72))
                    .unwrap_or(0.0),
                velocity_min: s
                    .f32_at(row, s.layout(SND_ALIAS_VELOCITY_MIN, 76))
                    .unwrap_or(0.0),
                flags,
                slave_percentage: s
                    .f32_at(row, s.layout(SND_ALIAS_SLAVE_PERCENTAGE, 84))
                    .unwrap_or(0.0),
                probability: s
                    .f32_at(row, s.layout(SND_ALIAS_PROBABILITY, 88))
                    .unwrap_or(1.0),
                lfe_percentage: s
                    .f32_at(row, s.layout(SND_ALIAS_LFE_PERCENTAGE, 92))
                    .unwrap_or(0.0),
                center_percentage: s
                    .f32_at(row, s.layout(SND_ALIAS_CENTER_PERCENTAGE, 96))
                    .unwrap_or(0.0),
                start_delay: s
                    .i32_at(row, s.layout(SND_ALIAS_START_DELAY, 100))
                    .unwrap_or(0),
                volume_falloff,
                t5_distance_curves: None,
                envelop_min: s
                    .f32_at(row, s.layout(SND_ALIAS_ENVELOP_MIN, 112))
                    .unwrap_or(0.0),
                envelop_max: s
                    .f32_at(row, s.layout(SND_ALIAS_ENVELOP_MAX, 116))
                    .unwrap_or(0.0),
                envelop_percentage: s
                    .f32_at(row, s.layout(SND_ALIAS_ENVELOP_PERCENTAGE, 120))
                    .unwrap_or(0.0),
                speaker_map: speaker_map_name(s, row),
                limit_count: None,
                entity_limit_count: None,
            });
        }

        if name.is_empty() {
            self.capture_gaps += 1;
            return Ok(());
        }
        self.ingest_sound(CapturedSound {
            name,
            aliases,
            game: self.capture_game,
            zone: self.capture_zone,
        });
        Ok(())
    }
}
