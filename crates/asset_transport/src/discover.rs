use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

pub(crate) const IW4_ZONE_VERSION: u32 = 0x114;

const T5_ZONE_VERSION: u32 = 0x1D9;

const IW5_ZONE_VERSION: u32 = 1;

const T6_ZONE_VERSION: u32 = fastfile_t6::ZONE_VERSION_PC;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamesRoot(pub PathBuf);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZoneFile {
    pub path: PathBuf,
    pub zone_name: String,

    pub alias_note: Option<String>,
}

pub fn load_dotenv() {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(".env");
        if candidate.is_file() {
            let _ = dotenvy::from_path(&candidate);
            return;
        }
    }
    let Ok(mut dir) = std::env::current_dir() else {
        return;
    };
    loop {
        let candidate = dir.join(".env");
        if candidate.is_file() {
            let _ = dotenvy::from_path(&candidate);
            return;
        }
        if !dir.pop() {
            return;
        }
    }
}

pub fn games_root_from_env() -> Result<GamesRoot, String> {
    load_dotenv();
    let path = match std::env::var_os("IW4L_GAMES") {
        Some(raw) => PathBuf::from(raw),
        None => default_games_root()?,
    };
    if !path.is_dir() {
        return Err(format!("IW4L_GAMES is not a directory: {}", path.display()));
    }
    Ok(GamesRoot(path))
}

#[cfg(windows)]
fn default_games_root() -> Result<PathBuf, String> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .ok_or_else(|| {
            "IW4L_GAMES is unset and the portable launcher directory is unavailable".to_owned()
        })
}

#[cfg(not(windows))]
fn default_games_root() -> Result<PathBuf, String> {
    Err("IW4L_GAMES is not set — copy .env.example to .env and set the games root".to_owned())
}

const SIBLING_TITLES: [&str; 3] = [
    "Call of Duty Black Ops",
    "Call of Duty Black Ops II",
    "Call of Duty Modern Warfare 3",
];

static GAME_FOLDERS: std::sync::RwLock<Vec<PathBuf>> = std::sync::RwLock::new(Vec::new());

pub fn set_game_folders(folders: Vec<PathBuf>) {
    let folders = folders
        .into_iter()
        .filter(|folder| folder.join("zone").is_dir())
        .collect();
    *GAME_FOLDERS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = folders;
}

pub fn game_install_root(picked: &Path) -> PathBuf {
    picked
        .ancestors()
        .take(3)
        .find(|dir| dir.join("zone").is_dir())
        .unwrap_or(picked)
        .to_path_buf()
}

pub fn folder_holds_game(folder: &Path, game: crate::ZoneGame) -> bool {
    let version = zone_version(game);
    let mut pending = vec![(folder.join("zone"), 0)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path.is_dir() {
                if depth < 2 {
                    pending.push((path, depth + 1));
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ff"))
                && peek_zone_version(&path) == Some(version)
            {
                return true;
            }
        }
    }
    false
}

pub fn find_game_install(root: &Path, game: crate::ZoneGame) -> Option<PathBuf> {
    search_roots(root)
        .into_iter()
        .find(|folder| folder_holds_game(folder, game))
}

pub fn search_roots(root: &Path) -> Vec<PathBuf> {
    let roots = configured_search_roots(root);
    #[cfg(not(windows))]
    let mut roots = roots;
    #[cfg(not(windows))]
    for installed in crate::steam::installed_game_roots(&roots) {
        if !roots.contains(&installed) {
            roots.push(installed);
        }
    }
    roots
}

pub(crate) fn configured_search_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![root.to_path_buf()];
    for folder in GAME_FOLDERS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
    {
        if !roots.contains(folder) {
            roots.push(folder.clone());
        }
    }
    if let Some(library) = root.parent() {
        for title in SIBLING_TITLES {
            let sibling = library.join(title);
            if sibling != root && sibling.join("zone").is_dir() && !roots.contains(&sibling) {
                roots.push(sibling);
            }
        }
    }
    #[cfg(windows)]
    {
        let Ok(entries) = std::fs::read_dir(root) else {
            return roots;
        };
        let mut links = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
            })
            .collect::<Vec<_>>();
        links.sort();
        for link_path in links {
            match shortcut_target_root(&link_path) {
                Ok(target) if !roots.contains(&target) => roots.push(target),
                Ok(_) => {}
                Err(error) => diag::warn!(
                    Zone,
                    "game shortcut {} ignored: {error}",
                    link_path.display()
                ),
            }
        }
    }
    roots
}

#[cfg(windows)]
fn shortcut_target_root(link_path: &Path) -> Result<PathBuf, String> {
    let link = lnk::ShellLink::open(link_path, lnk::encoding::WINDOWS_1252)
        .map_err(|error| format!("cannot parse .lnk: {error}"))?;
    let info = link
        .link_info()
        .as_ref()
        .ok_or_else(|| ".lnk has no LinkInfo filesystem target".to_owned())?;
    if !info.link_info_flags().has_volume_id_and_local_base_path() {
        return Err(".lnk target is not on a local filesystem".to_owned());
    }
    let mut target = info
        .local_base_path_unicode()
        .as_deref()
        .or_else(|| info.local_base_path())
        .map(PathBuf::from)
        .ok_or_else(|| ".lnk has no local base path".to_owned())?;
    let suffix = info
        .common_path_suffix_unicode()
        .as_deref()
        .unwrap_or_else(|| info.common_path_suffix());
    if !suffix.is_empty() {
        target.push(suffix);
    }
    game_root_for_shortcut_target(&target)
}

#[cfg(windows)]
fn game_root_for_shortcut_target(target: &Path) -> Result<PathBuf, String> {
    if target.is_dir() {
        return Ok(target.to_path_buf());
    }
    if target.is_file()
        && let Some(parent) = target.parent()
    {
        return Ok(parent.to_path_buf());
    }
    Err(format!("target does not exist: {}", target.display()))
}

pub fn peek_zone_version(path: &Path) -> Option<u32> {
    read_zone_version(path).ok()
}

fn read_zone_version(path: &Path) -> Result<u32, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut header = [0u8; 12];
    use std::io::Read;
    file.read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    if &header[0..4] != b"IWff" && &header[0..8] != fastfile_t6::MAGIC_SIGNED {
        return Err(format!("not an IWff envelope: {:02x?}", &header[..8]));
    }
    Ok(u32::from_le_bytes(header[8..12].try_into().unwrap()))
}

fn game_files(root: &Path) -> impl Iterator<Item = Result<PathBuf, String>> {
    files_under(search_roots(root))
}

fn files_under(roots: Vec<PathBuf>) -> impl Iterator<Item = Result<PathBuf, String>> {
    let mut pending: VecDeque<_> = roots.into_iter().map(Ok).collect();
    let mut visited = HashSet::new();
    std::iter::from_fn(move || {
        loop {
            let path = match pending.pop_front()? {
                Ok(path) => path,
                Err(error) => return Some(Err(error)),
            };
            let canonical = match std::fs::canonicalize(&path) {
                Ok(path) => path,
                Err(error) => {
                    return Some(Err(format!("cannot access {}: {error}", path.display())));
                }
            };
            if !visited.insert(canonical) {
                continue;
            }
            let metadata = match std::fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => return Some(Err(format!("cannot stat {}: {error}", path.display()))),
            };
            if metadata.is_file() {
                return Some(Ok(path));
            }
            if !metadata.is_dir() {
                continue;
            }
            let entries = match std::fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(error) => return Some(Err(format!("cannot read {}: {error}", path.display()))),
            };
            let mut children = entries
                .map(|entry| {
                    entry.map(|entry| entry.path()).map_err(|error| {
                        format!("cannot read entry in {}: {error}", path.display())
                    })
                })
                .collect::<Vec<_>>();
            children.sort();
            pending.extend(children);
        }
    })
}

pub fn zone_game_for_path(path: &Path) -> Option<crate::ZoneGame> {
    match peek_zone_version(path)? {
        IW4_ZONE_VERSION => Some(crate::ZoneGame::Iw4),
        T5_ZONE_VERSION => Some(crate::ZoneGame::T5),
        IW5_ZONE_VERSION => Some(crate::ZoneGame::Iw5),
        T6_ZONE_VERSION => Some(crate::ZoneGame::T6),
        _ => None,
    }
}

pub fn find_zone_file_under(search_root: &Path, zone: &str) -> Result<ZoneFile, String> {
    let file_name = format!("{zone}.ff");
    let mut iw4: Option<PathBuf> = None;
    let mut t5: Option<PathBuf> = None;
    let mut iw5: Option<PathBuf> = None;
    let mut other: Option<(PathBuf, u32)> = None;
    let mut errors = Vec::new();
    for entry in game_files(search_root) {
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };
        if !path
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case(&file_name))
        {
            continue;
        }
        match peek_zone_version(&path) {
            Some(IW4_ZONE_VERSION) if iw4.is_none() => iw4 = Some(path),
            Some(T5_ZONE_VERSION) if t5.is_none() => t5 = Some(path),
            Some(IW5_ZONE_VERSION) if iw5.is_none() => iw5 = Some(path),
            Some(v) if other.is_none() => other = Some((path, v)),
            None if other.is_none() => other = Some((path, 0)),
            _ => {}
        }
    }
    if let Some(path) = iw4 {
        return Ok(ZoneFile {
            path,
            zone_name: zone.to_owned(),
            alias_note: None,
        });
    }
    if let Some(path) = t5 {
        return Ok(ZoneFile {
            path,
            zone_name: zone.to_owned(),
            alias_note: None,
        });
    }
    if let Some(path) = iw5 {
        return Ok(ZoneFile {
            path,
            zone_name: zone.to_owned(),
            alias_note: None,
        });
    }
    if let Some((path, version)) = other {
        if version == 0 {
            return Err(format!(
                "zone `{file_name}` at {} is not a readable IWff envelope \
                 (need IW4 0x114, T5 0x1d9, or IW5 0x1)",
                path.display()
            ));
        }
        return Err(format!(
            "zone `{file_name}` found at {} but version is {version:#x} \
             (want IW4 0x114, T5 0x1d9, or IW5 0x1)",
            path.display()
        ));
    }
    Err(format!(
        "zone `{file_name}` not found under {}; scan errors: {}",
        search_root.display(),
        if errors.is_empty() {
            "none".to_owned()
        } else {
            errors.join("; ")
        }
    ))
}

pub fn find_zone_file_version(
    root: &GamesRoot,
    zone: &str,
    version: u32,
) -> Result<ZoneFile, String> {
    let file_name = format!("{zone}.ff");
    let mut found: Option<PathBuf> = None;
    let mut rejected = Vec::new();
    for entry in game_files(&root.0) {
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                rejected.push(error);
                continue;
            }
        };
        if !path
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case(&file_name))
        {
            continue;
        }
        let actual = read_zone_version(&path);
        if actual == Ok(version) {
            found = Some(path);
            break;
        }
        rejected.push(format!(
            "{} ({})",
            path.display(),
            actual.map_or_else(|error| error, |v| format!("version {v:#x}"))
        ));
    }
    let path = found.ok_or_else(|| {
        format!(
            "zone `{file_name}` version {version:#x} not found; {}; other candidates: {}",
            games_root_report(root),
            if rejected.is_empty() {
                "none".to_owned()
            } else {
                rejected.join("; ")
            }
        )
    })?;
    Ok(ZoneFile {
        path,
        zone_name: zone.to_owned(),
        alias_note: None,
    })
}

pub fn split_zone_key(zone: &str) -> (Option<crate::ZoneGame>, &str) {
    let Some((prefix, rest)) = zone.split_once(':') else {
        return (None, zone);
    };
    match crate::ZoneGame::from_prefix(prefix) {
        Some(game) if !rest.is_empty() => (Some(game), rest),
        _ => (None, zone),
    }
}

pub fn map_load_title(key: &str, game: Option<crate::ZoneGame>) -> String {
    let (parsed, stem) = split_zone_key(key);
    match parsed.or(game) {
        Some(game) => format!("{}:{stem}", game.prefix()),
        None => stem.to_owned(),
    }
}

pub fn group_mp_maps(maps: &[String]) -> [Vec<&str>; 4] {
    let mut cols = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for map in maps {
        let col = match split_zone_key(map).0 {
            Some(crate::ZoneGame::Iw4) | None => 0,
            Some(crate::ZoneGame::Iw5) => 1,
            Some(crate::ZoneGame::T5) => 2,
            Some(crate::ZoneGame::T6) => 3,
        };
        cols[col].push(map.as_str());
    }
    cols
}

pub fn find_zone_file(root: &GamesRoot, zone: &str) -> Result<ZoneFile, String> {
    let zone = zone.trim().to_ascii_lowercase();
    if zone.is_empty() {
        return Err("zone name is empty".into());
    }
    let (game, stem) = split_zone_key(&zone);
    find_zone_stem(root, game, stem)
}

pub fn zone_version(game: crate::ZoneGame) -> u32 {
    match game {
        crate::ZoneGame::Iw4 => IW4_ZONE_VERSION,
        crate::ZoneGame::T5 => T5_ZONE_VERSION,
        crate::ZoneGame::Iw5 => IW5_ZONE_VERSION,
        crate::ZoneGame::T6 => T6_ZONE_VERSION,
    }
}

fn find_stem_file(
    root: &GamesRoot,
    game: Option<crate::ZoneGame>,
    stem: &str,
) -> Result<ZoneFile, String> {
    match game {
        Some(game) => find_zone_file_version(root, stem, zone_version(game)),
        None => find_zone_file_under(&root.0, stem),
    }
}

fn find_zone_stem(
    root: &GamesRoot,
    game: Option<crate::ZoneGame>,
    stem: &str,
) -> Result<ZoneFile, String> {
    if stem.starts_with("mp_") {
        return find_stem_file(root, game, stem);
    }
    let Some(prefixed) = resolve_mp_zone_alias(stem) else {
        return find_stem_file(root, game, stem);
    };
    match find_stem_file(root, game, &prefixed) {
        Ok(mut found) => {
            let note = format!(
                "zone alias: `{stem}` -> `{prefixed}` (MP stem preferred over bare `{stem}.ff`)"
            );
            found.alias_note = Some(note.clone());
            match std::fs::metadata(&found.path) {
                Ok(meta) => diag::info!(Zone, "{note} ({} bytes)", meta.len()),
                Err(_) => diag::info!(Zone, "{note}"),
            }
            Ok(found)
        }
        Err(mp_err) => match find_stem_file(root, game, stem) {
            Ok(found) => Ok(found),
            Err(bare_err) => Err(format!("{mp_err}; {bare_err}")),
        },
    }
}

pub fn resolve_mp_zone_alias(zone: &str) -> Option<String> {
    if zone.is_empty() || zone.starts_with("mp_") || zone.ends_with("_mp") {
        return None;
    }
    Some(format!("mp_{zone}"))
}

pub fn find_common_mp_for_zone(zone_ff: &Path) -> Result<ZoneFile, String> {
    find_named_zone_for_tree(zone_ff, "common_mp")
}

pub fn find_runtime_zone(root: &GamesRoot, zone_ff: &Path, zone: &str) -> Result<ZoneFile, String> {
    match zone_game_for_path(zone_ff) {
        Some(crate::ZoneGame::Iw4) | None => find_zone_for_tree(zone_ff, zone),
        Some(_) => {
            let found = find_zone_file_under(&root.0, zone)?;
            match peek_zone_version(&found.path) {
                Some(IW4_ZONE_VERSION) => Ok(found),
                Some(version) => Err(format!(
                    "runtime profile is IW4 FFA; {zone} at {} is version {version:#x} \
                     (want IW4 {IW4_ZONE_VERSION:#x})",
                    found.path.display()
                )),
                None => Err(format!(
                    "runtime profile is IW4 FFA; {zone} at {} is not a readable IWff envelope",
                    found.path.display()
                )),
            }
        }
    }
}

pub fn find_runtime_common_mp(root: &GamesRoot, zone_ff: &Path) -> Result<ZoneFile, String> {
    find_runtime_zone(root, zone_ff, "common_mp")
}

pub fn find_common_mp_for_envelope(root: &GamesRoot, version: u32) -> Result<ZoneFile, String> {
    find_zone_file_version(root, "common_mp", version)
}

pub fn find_localized_common_mp_for_zone(zone_ff: &Path) -> Result<ZoneFile, String> {
    find_named_zone_for_tree(zone_ff, "localized_common_mp")
}

pub fn find_zone_for_tree(zone_ff: &Path, zone: &str) -> Result<ZoneFile, String> {
    find_named_zone_for_tree(zone_ff, zone)
}

fn find_named_zone_for_tree(zone_ff: &Path, zone: &str) -> Result<ZoneFile, String> {
    let tree = game_root_for_zone(zone_ff)?;
    if let Some(version) = peek_zone_version(zone_ff)
        && let Ok(found) = find_zone_file_version(&GamesRoot(tree.clone()), zone, version)
    {
        return Ok(found);
    }
    find_zone_file_under(&tree, zone)
}

pub fn game_root_for_zone(zone_ff: &Path) -> Result<PathBuf, String> {
    let mut cursor = zone_ff.parent();
    while let Some(dir) = cursor {
        if dir.join("zone").is_dir() {
            return Ok(dir.to_path_buf());
        }
        cursor = dir.parent();
    }
    Err(format!(
        "no game tree with a `zone/` directory above {}",
        zone_ff.display()
    ))
}

pub fn games_root_report(root: &GamesRoot) -> String {
    let roots = search_roots(&root.0);
    let mut line = format!("games root: {}", root.0.display());
    for extra in roots.iter().skip(1) {
        line.push_str(&format!("; shortcut root: {}", extra.display()));
    }
    line
}

pub fn games_content_report(root: &GamesRoot) -> Vec<String> {
    #[derive(Default)]
    struct Content {
        versions: BTreeMap<u32, usize>,
        files: usize,
        dlc: usize,
        iw4_common: bool,
    }
    let mut trees: BTreeMap<PathBuf, Content> = BTreeMap::new();
    let mut report = Vec::new();
    for entry in game_files(&root.0) {
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                report.push(format!("asset scan: {error}"));
                continue;
            }
        };
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ff"))
        {
            continue;
        }
        let tree = game_root_for_zone(&path).unwrap_or_else(|_| root.0.clone());
        let is_dlc = path.strip_prefix(&tree).ok().is_some_and(|relative| {
            relative.parent().is_some_and(|parent| {
                parent
                    .components()
                    .any(|part| part.as_os_str().eq_ignore_ascii_case("dlc"))
            })
        });
        let content = trees.entry(tree).or_default();
        content.files += 1;
        if is_dlc {
            content.dlc += 1;
        }
        match read_zone_version(&path) {
            Ok(version) => {
                *content.versions.entry(version).or_default() += 1;
                if version == IW4_ZONE_VERSION
                    && path
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("common_mp.ff"))
                {
                    content.iw4_common = true;
                }
            }
            Err(error) => report.push(format!("asset header {}: {error}", path.display())),
        }
    }
    if trees.is_empty() {
        report.push(
            "No .ff files found in the configured game folders and shortcut targets.".to_owned(),
        );
    }
    for (tree, content) in trees {
        let versions = content
            .versions
            .iter()
            .map(|(version, count)| format!("{version:#x}: {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        report.push(format!(
            "assets {}: {} FastFiles, {} in DLC folders; envelope versions {{{versions}}}",
            tree.display(),
            content.files,
            content.dlc
        ));
        if content.files == content.dlc && !content.iw4_common {
            report.push("Only DLC FastFiles were found in this tree; DLC maps do not supply the base multiplayer menu assets.".to_owned());
        }
        if content.versions.contains_key(&IW4_ZONE_VERSION) {
            report.push(format!(
                "IW4 common_mp.ff: {}",
                if content.iw4_common {
                    "found"
                } else {
                    "missing from this tree"
                }
            ));
        }
    }
    report
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapPack {
    pub label: String,
    pub maps: Vec<String>,
}

pub fn list_mp_maps(root: &GamesRoot) -> Vec<String> {
    let mut maps: Vec<String> = list_mp_map_packs(root)
        .into_iter()
        .flat_map(|pack| pack.maps)
        .collect();
    maps.sort();
    maps
}

pub fn list_mp_map_packs(root: &GamesRoot) -> Vec<MapPack> {
    let mut zones = Vec::new();
    for entry in game_files(&root.0) {
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                diag::warn!(Zone, "map discovery: {error}");
                continue;
            }
        };
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let stem = stem.to_ascii_lowercase();
        let is_ff = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ff"));
        if !(is_ff && stem.starts_with("mp_")) {
            continue;
        }
        if let Some(game) = zone_game_for_path(&path) {
            zones.push((game.prefix(), stem, map_pack_folder(&path)));
        }
    }
    let loads: HashSet<(&str, &str)> = zones
        .iter()
        .filter_map(|(game, stem, _)| stem.strip_suffix("_load").map(|map| (*game, map)))
        .collect();
    let mut packs: BTreeMap<(&str, &str), Vec<String>> = BTreeMap::new();
    let mut seen = HashSet::new();
    for (game, stem, folder) in &zones {
        if stem.ends_with("_load")
            || !(loads.contains(&(*game, stem.as_str())) || !loads.iter().any(|(g, _)| g == game))
        {
            continue;
        }
        let map = format!("{game}:{stem}");
        if seen.insert(map.clone()) {
            packs.entry((game, folder)).or_default().push(map);
        }
    }
    packs
        .into_iter()
        .map(|((game, folder), mut maps)| {
            maps.sort();
            MapPack {
                label: format!("{} {folder}", game.to_ascii_uppercase()),
                maps,
            }
        })
        .collect()
}

fn map_pack_folder(zone_ff: &Path) -> String {
    game_root_for_zone(zone_ff)
        .ok()
        .and_then(|tree| {
            zone_ff
                .strip_prefix(tree.join("zone"))
                .ok()?
                .components()
                .next()?
                .as_os_str()
                .to_str()
                .map(str::to_ascii_uppercase)
        })
        .filter(|folder| folder.starts_with("DLC"))
        .unwrap_or_else(|| "BASE".to_owned())
}

pub fn ensure_artifacts_dir() -> Result<PathBuf, String> {
    let dir = std::env::var_os("IW4L_ARTIFACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("iw4l-artifacts"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// T5 language archives carry a language prefix rather than `localized_`.
pub fn find_t5_localized_zones(
    anchor: &Path,
    language: Option<&str>,
) -> Result<Vec<ZoneFile>, String> {
    let (dir, prefix) = language_archive(anchor, language, crate::ZoneGame::T5)?;
    Ok(["code_post_gfx_mp", "common_mp", "ui_mp"]
        .into_iter()
        .filter_map(|stem| t5_localized_zone(&dir, &prefix, stem))
        .collect())
}

pub fn find_t5_localized_zone(
    anchor: &Path,
    language: Option<&str>,
    stem: &str,
) -> Result<Option<ZoneFile>, String> {
    let (dir, prefix) = language_archive(anchor, language, crate::ZoneGame::T5)?;
    Ok(t5_localized_zone(&dir, &prefix, stem))
}

pub fn find_t6_localized_zones(
    anchor: &Path,
    language: Option<&str>,
) -> Result<Vec<ZoneFile>, String> {
    let (dir, prefix) = language_archive(anchor, language, crate::ZoneGame::T6)?;
    Ok(["patch_mp", "ui_mp", "code_post_gfx_mp"]
        .into_iter()
        .filter_map(|stem| t5_localized_zone(&dir, &prefix, stem))
        .collect())
}

fn t5_localized_zone(dir: &Path, prefix: &str, stem: &str) -> Option<ZoneFile> {
    let zone_name = format!("{prefix}{stem}");
    let path = dir.join(format!("{zone_name}.ff"));
    path.is_file().then_some(ZoneFile {
        path,
        zone_name,
        alias_note: None,
    })
}

fn language_archive(
    anchor: &Path,
    language: Option<&str>,
    game: crate::ZoneGame,
) -> Result<(PathBuf, String), String> {
    let root = game_root_for_zone(anchor)?.join("zone");
    let mut choices: Vec<_> = files_under(vec![root])
        .filter_map(Result::ok)
        .filter(|path| {
            path.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|name| name.ends_with("_code_post_gfx_mp.ff"))
                && zone_game_for_path(path) == Some(game)
        })
        .collect();
    choices.sort();
    let preferred = choices.iter().position(|path| {
        path.parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .zip(language)
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
    });
    let chosen = match preferred {
        Some(index) => &choices[index],
        None if choices.len() == 1 => &choices[0],
        _ => {
            return Err(format!(
                "{} language archive selection is ambiguous or missing: {} candidates",
                game.prefix().to_uppercase(),
                choices.len()
            ));
        }
    };
    let name = chosen
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("language archive name")?;
    let prefix = name
        .strip_suffix("code_post_gfx_mp.ff")
        .ok_or("language prefix")?;
    let dir = chosen.parent().ok_or("language directory")?;
    Ok((dir.to_path_buf(), prefix.to_owned()))
}
