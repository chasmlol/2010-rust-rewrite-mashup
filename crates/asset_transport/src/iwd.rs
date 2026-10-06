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
            .filter(|path| is_iwd_archive(path))
            .collect::<Vec<_>>();
        archives.sort();

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
        Ok(Self {
            images,
            archives: archives.len(),
        })
    }
}

/// macOS writes an AppleDouble `._<name>` beside every file it copies onto
/// exFAT, FAT or a network share. `._iw_00.iwd` is not a zip, and one of them
/// in `main/` would fail the whole index.
fn is_iwd_archive(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("iwd"))
        && !path
            .file_name()
            .is_some_and(|name| name.as_encoded_bytes().starts_with(b"._"))
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
    let mut entry = archive.by_name(entry_name).map_err(|error| {
        format!("cannot open IWD entry {entry_name} in {archive_path:?}: {error}")
    })?;
    let bytes = read_entry_bytes(&mut entry, limit).map_err(|error| {
        format!("cannot read IWD entry {entry_name} in {archive_path:?}: {error}")
    })?;
    IWD_INFLATE_NS.fetch_add(inflate_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
    Ok(bytes)
}

fn read_entry_bytes<R: Read>(
    entry: &mut zip::read::ZipFile<'_, R>,
    limit: Option<usize>,
) -> Result<Vec<u8>, String> {
    let size = entry.size();
    // Bound a prefix before converting the archive's u64 size to usize.
    let want = limit.map_or(size, |limit| size.min(limit as u64));
    let capacity = usize::try_from(want)
        .map_err(|_| format!("IWD entry size {want} does not fit in memory"))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|error| format!("cannot allocate {want} bytes for IWD entry: {error}"))?;
    entry
        .by_ref()
        .take(want)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() != capacity {
        return Err(format!(
            "IWD entry length mismatch: expected {want} bytes, read {}",
            bytes.len()
        ));
    }
    if want == size {
        // Take stops at its limit without asking zip for EOF. Probe once to
        // validate the CRC and reject extra data without growing the buffer.
        let mut extra = [0u8; 1];
        loop {
            match entry.read(&mut extra) {
                Ok(0) => break,
                Ok(_) => {
                    return Err(format!(
                        "IWD entry length mismatch: expected {want} bytes, entry has additional data"
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
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

pub fn game_mains_by_root(games_root: &Path) -> Vec<Vec<PathBuf>> {
    crate::discover::search_roots(games_root)
        .into_iter()
        .map(|root| {
            let mut mains = Vec::new();
            let mut stack = vec![root];
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
            mains.dedup();
            mains
        })
        .collect()
}

pub fn game_mains_under(games_root: &Path) -> Vec<PathBuf> {
    let mut mains: Vec<PathBuf> = Vec::new();
    for main in game_mains_by_root(games_root).into_iter().flatten() {
        if !mains.contains(&main) {
            mains.push(main);
        }
    }
    mains
}

pub fn read_iwd_named(games_root: &Path, want: &str) -> Option<Vec<u8>> {
    let want = want.replace('\\', "/");
    let mut stack: Vec<_> = crate::discover::search_roots(games_root)
        .into_iter()
        .rev()
        .collect();
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
            if !is_iwd_archive(&path) {
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
                if entry.name().replace('\\', "/").eq_ignore_ascii_case(&want)
                    && let Ok(bytes) = read_entry_bytes(&mut entry, None)
                {
                    return Some(bytes);
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
            .filter(|p| is_iwd_archive(p))
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
