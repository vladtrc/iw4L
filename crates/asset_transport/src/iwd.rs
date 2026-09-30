use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

#[derive(Clone, Debug)]
pub struct IwdFile {
    archive: PathBuf,
    entry: String,
    crc32: u32,
    size: u64,
}

impl IwdFile {
    pub fn entry(&self) -> &str {
        &self.entry
    }

    pub fn crc32(&self) -> u32 {
        self.crc32
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn read(&self) -> Result<Vec<u8>, String> {
        ENTRY_PAYLOAD_READS.fetch_add(1, Ordering::Relaxed);
        read_indexed_entry(self, None)
    }

    /// Inflate at most `limit` bytes of the entry.
    ///
    /// A header is a fixed prefix, and deflate stops where the reader stops:
    /// asking whether an image is a cubemap costs the first block of it, not
    /// the megabyte behind it.
    pub fn read_header(&self, limit: usize) -> Result<Vec<u8>, String> {
        ENTRY_HEADER_READS.fetch_add(1, Ordering::Relaxed);
        read_indexed_entry(self, Some(limit))
    }
}

#[derive(Debug, Default)]
pub struct IwdIndex {
    images: HashMap<String, Vec<IwdFile>>,
    archives: usize,
}

fn index_cache() -> &'static RwLock<HashMap<PathBuf, Arc<IwdIndex>>> {
    static CACHE: OnceLock<RwLock<HashMap<PathBuf, Arc<IwdIndex>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

type IndexBuild = Arc<OnceLock<Result<Arc<IwdIndex>, String>>>;

fn index_builds() -> &'static Mutex<HashMap<PathBuf, IndexBuild>> {
    static BUILDS: OnceLock<Mutex<HashMap<PathBuf, IndexBuild>>> = OnceLock::new();
    BUILDS.get_or_init(|| Mutex::new(HashMap::new()))
}

impl IwdIndex {
    pub fn open(directory: &Path) -> Result<Arc<Self>, String> {
        let key = directory.to_path_buf();
        if let Some(hit) = index_cache()
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return Ok(Arc::clone(hit));
        }

        let slot = {
            let mut builds = index_builds()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Arc::clone(builds.entry(key.clone()).or_default())
        };

        let built = slot
            .get_or_init(|| Self::open_uncached(directory).map(Arc::new))
            .clone();
        {
            let mut builds = index_builds()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if builds
                .get(&key)
                .is_some_and(|live| Arc::ptr_eq(live, &slot))
            {
                builds.remove(&key);
            }
        }
        let built = built?;
        let mut cache = index_cache()
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(Arc::clone(
            cache.entry(key).or_insert_with(|| Arc::clone(&built)),
        ))
    }

    pub fn is_cached(directory: &Path) -> bool {
        index_cache()
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(directory)
    }

    pub fn archive_count(&self) -> usize {
        self.archives
    }

    pub fn image_candidates(&self, name: &str) -> Option<&[IwdFile]> {
        self.images
            .get(&name.to_ascii_lowercase())
            .map(Vec::as_slice)
    }

    fn open_uncached(directory: &Path) -> Result<Self, String> {
        let mut archives = std::fs::read_dir(directory)
            .map_err(|error| format!("cannot read IWD directory {}: {error}", directory.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("iwd"))
            })
            .collect::<Vec<_>>();
        archives.sort();

        // The archive scan itself is one-time-per-install work (thousands of
        // small seeky zip central-directory reads per archive) that a spinning
        // disk pays for on every single launch unless it is cached — the
        // decoded pixel data already gets this treatment (`load_or_decode_mips`),
        // this is the same idea for the name index that decoding depends on.
        let cache_key = index_cache_key(&archives);
        if let Some(key) = &cache_key
            && let Some(bytes) = crate::artifact_cache::cache_get(INDEX_CACHE_KIND, key)
            && let Some(images) = decode_index(&bytes, &archives)
        {
            return Ok(Self {
                images,
                archives: archives.len(),
            });
        }

        type IndexedArchive = (usize, Result<Vec<(String, IwdFile)>, String>);
        let next = std::sync::atomic::AtomicUsize::new(0);
        let done: Mutex<Vec<IndexedArchive>> = Mutex::new(Vec::with_capacity(archives.len()));
        let lanes = archives.len().min(index_lane_width());
        std::thread::scope(|scope| {
            for _ in 0..lanes {
                scope.spawn(|| {
                    loop {
                        let slot = next.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = archives.get(slot) else {
                            return;
                        };
                        let indexed = index_image_entries(path);
                        done.lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .push((slot, indexed));
                    }
                });
            }
        });
        let mut done = done
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        done.sort_by_key(|(slot, _)| *slot);
        let per_archive = done
            .into_iter()
            .map(|(_, indexed)| indexed)
            .collect::<Result<Vec<_>, String>>()?;

        let mut images: HashMap<String, Vec<IwdFile>> = HashMap::new();
        for entries in per_archive {
            for (name, file) in entries {
                images.entry(name).or_default().push(file);
            }
        }
        if let Some(key) = &cache_key {
            let blob = encode_index(&archives, &images);
            if let Err(error) = crate::artifact_cache::cache_put(INDEX_CACHE_KIND, key, &blob) {
                diag::warn!(Zone, "iwd index cache store {key}: {error}");
            }
        }
        Ok(Self {
            images,
            archives: archives.len(),
        })
    }
}

const INDEX_CACHE_KIND: &str = "iwdidx";
const INDEX_CACHE_MAGIC: &[u8; 8] = b"IW4LIDX1";

/// A fingerprint of the archive set (name, size, mtime of every `.iwd` under
/// the directory) so a changed install invalidates the cache instead of
/// silently serving a stale name index. `archives` is already sorted, so the
/// same file set always produces the same fingerprint and the same
/// archive-index ordering the encoded entries reference.
fn index_cache_key(archives: &[PathBuf]) -> Option<String> {
    let mut hash = crate::artifact_cache::fnv1a64(b"iwd-index-v1");
    for archive in archives {
        let meta = std::fs::metadata(archive).ok()?;
        let modified = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        hash = crate::artifact_cache::fnv1a64_more(
            hash,
            archive.file_name()?.to_string_lossy().as_bytes(),
        );
        hash = crate::artifact_cache::fnv1a64_more(hash, &meta.len().to_le_bytes());
        hash = crate::artifact_cache::fnv1a64_more(hash, &modified.to_le_bytes());
    }
    Some(format!("{hash:016x}"))
}

fn encode_index(archives: &[PathBuf], images: &HashMap<String, Vec<IwdFile>>) -> Vec<u8> {
    let entry_count: usize = images.values().map(Vec::len).sum();
    let mut out = Vec::with_capacity(16 + entry_count * 32);
    out.extend_from_slice(INDEX_CACHE_MAGIC);
    out.extend_from_slice(&(archives.len() as u32).to_le_bytes());
    out.extend_from_slice(&(entry_count as u32).to_le_bytes());
    for (name, files) in images {
        for file in files {
            let Some(archive_index) = archives.iter().position(|path| *path == file.archive)
            else {
                continue;
            };
            out.extend_from_slice(&(archive_index as u16).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&(file.entry.len() as u16).to_le_bytes());
            out.extend_from_slice(file.entry.as_bytes());
            out.extend_from_slice(&file.crc32.to_le_bytes());
            out.extend_from_slice(&file.size.to_le_bytes());
        }
    }
    out
}

fn decode_index(bytes: &[u8], archives: &[PathBuf]) -> Option<HashMap<String, Vec<IwdFile>>> {
    if bytes.len() < 16 || &bytes[..8] != INDEX_CACHE_MAGIC {
        return None;
    }
    let archive_count = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
    if archive_count != archives.len() {
        return None;
    }
    let entry_count = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
    let mut at = 16;
    let mut images: HashMap<String, Vec<IwdFile>> = HashMap::new();
    for _ in 0..entry_count {
        let archive_index = u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as usize;
        at += 2;
        let name_len = u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as usize;
        at += 2;
        let name = std::str::from_utf8(bytes.get(at..at + name_len)?)
            .ok()?
            .to_owned();
        at += name_len;
        let entry_len = u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as usize;
        at += 2;
        let entry = std::str::from_utf8(bytes.get(at..at + entry_len)?)
            .ok()?
            .to_owned();
        at += entry_len;
        let crc32 = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
        at += 4;
        let size = u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?);
        at += 8;
        let archive = archives.get(archive_index)?.clone();
        images.entry(name).or_default().push(IwdFile {
            archive,
            entry,
            crc32,
            size,
        });
    }
    Some(images)
}

fn index_image_entries(path: &Path) -> Result<Vec<(String, IwdFile)>, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("cannot index {}: {error}", path.display()))?;
    let mut entries = Vec::new();
    for entry_index in 0..archive.len() {
        let entry = archive
            .by_index(entry_index)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let exact = entry.name().to_owned();
        let normalized = exact.replace('\\', "/");
        let Some(name) = normalized
            .strip_prefix("images/")
            .and_then(|name| name.strip_suffix(".iwi"))
        else {
            continue;
        };
        if name.is_empty() || name.contains('/') {
            continue;
        }
        entries.push((
            name.to_ascii_lowercase(),
            IwdFile {
                archive: path.to_path_buf(),
                entry: exact,
                crc32: entry.crc32(),
                size: entry.size(),
            },
        ));
    }
    Ok(entries)
}

type OpenArchive = zip::ZipArchive<std::io::BufReader<std::fs::File>>;

static PARKED: OnceLock<Mutex<HashMap<PathBuf, Vec<OpenArchive>>>> = OnceLock::new();

const PARKED_LIMIT: usize = 256;

static PARKED_N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn index_lane_width() -> usize {
    static WIDTH: OnceLock<usize> = OnceLock::new();
    *WIDTH.get_or_init(|| {
        std::env::var("IW4L_IWD_INDEX_LANES")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|lanes| *lanes > 0)
            .unwrap_or(4)
    })
}

fn parked() -> &'static Mutex<HashMap<PathBuf, Vec<OpenArchive>>> {
    PARKED.get_or_init(|| Mutex::new(HashMap::new()))
}

struct ArchiveLease {
    path: PathBuf,
    archive: Option<OpenArchive>,
}

impl ArchiveLease {
    fn take(path: &Path) -> Result<Self, String> {
        let parked = parked()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_mut(path)
            .and_then(Vec::pop);
        if let Some(archive) = parked {
            PARKED_N.fetch_sub(1, Ordering::Relaxed);
            return Ok(Self {
                path: path.to_path_buf(),
                archive: Some(archive),
            });
        }
        let opened_at = std::time::Instant::now();
        let disk = std::fs::File::open(path)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        let archive = zip::ZipArchive::new(std::io::BufReader::new(disk))
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        IWD_DIRECTORY_NS.fetch_add(opened_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
        IWD_DIRECTORY_OPENS.fetch_add(1, Ordering::Relaxed);
        Ok(Self {
            path: path.to_path_buf(),
            archive: Some(archive),
        })
    }
}

impl Drop for ArchiveLease {
    fn drop(&mut self) {
        let Some(archive) = self.archive.take() else {
            return;
        };

        if PARKED_N.fetch_add(1, Ordering::Relaxed) >= PARKED_LIMIT {
            PARKED_N.fetch_sub(1, Ordering::Relaxed);
            return;
        }
        parked()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(self.path.clone())
            .or_default()
            .push(archive);
    }
}

static ENTRY_PAYLOAD_READS: AtomicU64 = AtomicU64::new(0);
static ENTRY_HEADER_READS: AtomicU64 = AtomicU64::new(0);

static IWD_DIRECTORY_NS: AtomicU64 = AtomicU64::new(0);
static IWD_DIRECTORY_OPENS: AtomicU64 = AtomicU64::new(0);
static IWD_INFLATE_NS: AtomicU64 = AtomicU64::new(0);

fn read_indexed_entry(file: &IwdFile, limit: Option<usize>) -> Result<Vec<u8>, String> {
    read_pooled_entry(&file.archive, &file.entry, limit)
}

fn read_pooled_entry(
    archive_path: &Path,
    entry_name: &str,
    limit: Option<usize>,
) -> Result<Vec<u8>, String> {
    let mut lease = ArchiveLease::take(archive_path)?;
    let archive = lease
        .archive
        .as_mut()
        .expect("lease holds its reader until drop");
    let inflate_at = std::time::Instant::now();
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|error| format!("cannot read {entry_name}: {error}"))?;
    let want = limit.map_or(entry.size() as usize, |limit| {
        limit.min(entry.size() as usize)
    });
    let mut bytes = Vec::with_capacity(want);
    let read = match limit {
        Some(limit) => entry.take(limit as u64).read_to_end(&mut bytes),
        None => entry.read_to_end(&mut bytes),
    };
    read.map_err(|error| format!("cannot read {entry_name}: {error}"))?;
    IWD_INFLATE_NS.fetch_add(inflate_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
    Ok(bytes)
}

pub fn cached_iwd_dirs() -> Vec<PathBuf> {
    let mut dirs = index_cache()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    dirs.sort();
    dirs
}

/// Entries inflated whole, and entries inflated only far enough to read a
/// header.
pub fn iwd_entry_reads() -> (u64, u64) {
    (
        ENTRY_PAYLOAD_READS.load(Ordering::Relaxed),
        ENTRY_HEADER_READS.load(Ordering::Relaxed),
    )
}

pub fn iwd_read_cost() -> (f64, u64, f64) {
    (
        IWD_DIRECTORY_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
        IWD_DIRECTORY_OPENS.load(Ordering::Relaxed),
        IWD_INFLATE_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
    )
}

pub fn game_main_for_zone(zone_ff: &Path) -> Result<PathBuf, String> {
    for ancestor in zone_ff.ancestors() {
        let main = ancestor.join("main");
        if main.is_dir() {
            return Ok(main);
        }
    }
    Err(format!(
        "no game tree with a `main/` directory above {}",
        zone_ff.display()
    ))
}

pub fn game_mains_under(games_root: &Path) -> Vec<PathBuf> {
    let mut mains = Vec::new();
    let mut stack = crate::discover::search_roots(games_root);
    while let Some(dir) = stack.pop() {
        let main = dir.join("main");
        if main.is_dir() {
            mains.push(main);
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                }
            }
        }
    }
    mains.sort();
    mains
}

pub fn read_iwd_named(games_root: &Path, want: &str) -> Option<Vec<u8>> {
    let want = want.replace('\\', "/");
    let mut stack = crate::discover::search_roots(games_root);
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths = entries
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("iwd"))
            {
                continue;
            }
            let Ok(file) = std::fs::File::open(&path) else {
                continue;
            };
            let Ok(mut archive) = zip::ZipArchive::new(file) else {
                continue;
            };
            for index in 0..archive.len() {
                let Ok(mut entry) = archive.by_index(index) else {
                    continue;
                };
                if entry.name().replace('\\', "/").eq_ignore_ascii_case(&want) {
                    let mut bytes = Vec::new();
                    if entry.read_to_end(&mut bytes).is_ok() {
                        return Some(bytes);
                    }
                }
            }
        }
    }
    None
}

pub fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))
}

pub fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, String> {
    miniz_oxide::inflate::decompress_to_vec_zlib(data)
        .map_err(|error| format!("zlib inflate: {error:?}"))
}

#[derive(Debug, Default, Clone)]
pub struct IwdSoundIndex {
    dir: PathBuf,

    sounds: HashMap<String, (PathBuf, String)>,
}

fn sound_key(relative: &str) -> String {
    relative.replace('\\', "/").to_ascii_lowercase()
}

fn sound_rel_from_zip_name(name: &str, language: Option<&str>) -> Option<String> {
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    let lower = language
        .and_then(|language| lower.strip_prefix(language)?.strip_prefix('/'))
        .unwrap_or(&lower);
    let rel = lower.strip_prefix("sound/")?;
    if rel.is_empty() {
        return None;
    }
    Some(rel.to_owned())
}

fn archive_language(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    let (language, _) = stem.strip_prefix("localized_")?.rsplit_once('_')?;
    Some(language.to_owned())
}

impl IwdSoundIndex {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, String> {
        let directory = directory.as_ref();
        let mut archives = std::fs::read_dir(directory)
            .map_err(|e| format!("cannot read {}: {e}", directory.display()))?
            .filter_map(std::result::Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("iwd"))
            })
            .collect::<Vec<_>>();
        archives.sort();

        let mut sounds = HashMap::new();
        for path in archives {
            let file = std::fs::File::open(&path)
                .map_err(|e| format!("cannot open {}: {e}", path.display()))?;

            let archive = zip::ZipArchive::new(std::io::BufReader::new(file))
                .map_err(|e| format!("zip {}: {e}", path.display()))?;
            let language = archive_language(&path);
            for name in archive.file_names() {
                let Some(rel) = sound_rel_from_zip_name(name, language.as_deref()) else {
                    continue;
                };
                sounds
                    .entry(rel)
                    .or_insert_with(|| (path.clone(), name.to_owned()));
            }
        }
        Ok(Self {
            dir: directory.to_path_buf(),
            sounds,
        })
    }

    pub fn indexed_dir(&self) -> &Path {
        &self.dir
    }

    pub fn sound_count(&self) -> usize {
        self.sounds.len()
    }

    pub fn read_sound(&self, relative: &str) -> Option<Result<Vec<u8>, String>> {
        let (archive, entry) = self.sounds.get(&sound_key(relative))?;
        Some(read_pooled_entry(archive, entry, None))
    }
}
