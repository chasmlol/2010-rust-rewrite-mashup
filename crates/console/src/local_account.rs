use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use bevy::prelude::*;
use session::LocalAccount;
use sim::{AccountId, AccountSnapshot, PLAYER_DATA_BUFFER_BYTES};

const MAGIC: &[u8; 8] = b"IW4LACC3";
const KEYED_MAGIC: &[u8; 8] = b"IW4LACC2";
const LEGACY_MAGIC: &[u8; 8] = b"IW4LACC1";
const LEGACY_EMPTY_BYTES: usize = 25;
const LEGACY_SNAPSHOT_BYTES: usize = LEGACY_EMPTY_BYTES + 16 + PLAYER_DATA_BUFFER_BYTES;
const EMPTY_BYTES: usize = 57;
const KEYED_SNAPSHOT_BYTES: usize = EMPTY_BYTES + 16 + PLAYER_DATA_BUFFER_BYTES;
const SNAPSHOT_BYTES: usize = KEYED_SNAPSHOT_BYTES + sim::SKILL_RATING_BYTES;
const SAVE_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Resource, Default)]
pub(crate) struct AccountPersistence {
    path: Option<PathBuf>,
    saved: Option<AccountSnapshot>,
    retry_at: Option<Instant>,
    last_error: Option<(io::ErrorKind, String)>,
}

pub(crate) fn load(
    mut commands: Commands,
    identity: Res<ui::LaunchIdentity>,
    role: Res<frame::RuntimeRole>,
    mut persistence: ResMut<AccountPersistence>,
) {
    if !matches!(
        *role,
        frame::RuntimeRole::Listen | frame::RuntimeRole::Client
    ) {
        return;
    }
    let path = std::env::var_os("IW4L_ACCOUNT_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            crate::user_settings::settings_path(&identity.artifacts)
                .map(|path| path.with_file_name("account.dat"))
        });
    let Some(path) = path else {
        warn!("no HOME or XDG_CONFIG_HOME; no durable local account available");
        return;
    };
    match load_or_create(&path) {
        Ok(account) => {
            persistence.path = Some(path);
            persistence.saved = account.snapshot.clone();
            commands.insert_resource(net::AccountSaveReceipt(account.snapshot.clone()));
            commands.insert_resource(account);
        }
        Err(error) => warn!("could not load local account {}: {error}", path.display()),
    }
}

pub(crate) fn save(
    role: Res<frame::RuntimeRole>,
    authority: Option<ResMut<net::AuthorityWorld>>,
    account: Option<ResMut<LocalAccount>>,
    mut persistence: ResMut<AccountPersistence>,
    mut receipt: Option<ResMut<net::AccountSaveReceipt>>,
) {
    if !matches!(
        *role,
        frame::RuntimeRole::Listen | frame::RuntimeRole::Client
    ) {
        return;
    }
    let (Some(mut account), Some(path)) = (account, persistence.path.clone()) else {
        return;
    };
    let mut authority = authority;
    let snapshot = if *role == frame::RuntimeRole::Listen {
        authority
            .as_ref()
            .and_then(|authority| authority.0.persistent_data().snapshot(account.id))
    } else {
        account.snapshot.clone()
    };
    let Some(snapshot) = snapshot else {
        return;
    };
    if persistence.saved.as_ref() == Some(&snapshot) {
        persistence.retry_at = None;
        persistence.last_error = None;
        if let Some(receipt) = receipt.as_mut() {
            receipt.0 = Some(snapshot.clone());
        }
        if *role == frame::RuntimeRole::Listen
            && let Some(authority) = authority.as_mut()
        {
            authority
                .0
                .persistent_data_mut()
                .acknowledge_saved(account.id, snapshot.revision);
        }
        return;
    };
    if persistence
        .retry_at
        .is_some_and(|retry_at| Instant::now() < retry_at)
    {
        return;
    }

    let next = LocalAccount {
        id: account.id,
        key: account.key.clone(),
        snapshot: Some(snapshot.clone()),
    };
    match save_current(&path, &next, persistence.saved.as_ref()) {
        Ok(()) => {
            persistence.retry_at = None;
            persistence.last_error = None;
            if *role == frame::RuntimeRole::Listen
                && let Some(authority) = authority.as_mut()
            {
                authority
                    .0
                    .persistent_data_mut()
                    .acknowledge_saved(account.id, snapshot.revision);
            }
            if let Some(receipt) = receipt.as_mut() {
                receipt.0 = Some(snapshot.clone());
            }
            persistence.saved = Some(snapshot.clone());
            account.snapshot = Some(snapshot);
        }
        Err(error) => {
            let error = (error.kind(), error.to_string());
            if persistence.last_error.as_ref() != Some(&error) {
                warn!(
                    "could not save local account {}: {}",
                    path.display(),
                    error.1
                );
                persistence.last_error = Some(error);
            }
            persistence.retry_at = Some(Instant::now() + SAVE_RETRY_DELAY);
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn encode(account: &LocalAccount) -> io::Result<Vec<u8>> {
    if account.id.0 == [0; 16] {
        return Err(invalid("zero account identity"));
    }
    if account.key.account() != account.id {
        return Err(invalid("account identity does not match signing key"));
    }
    let mut bytes = Vec::with_capacity(SNAPSHOT_BYTES);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&account.id.0);
    bytes.extend_from_slice(&account.key.export_seed());
    bytes.push(u8::from(account.snapshot.is_some()));
    if let Some(snapshot) = &account.snapshot {
        if snapshot.account != account.id || snapshot.bytes.len() != PLAYER_DATA_BUFFER_BYTES {
            return Err(invalid("account buffer identity or size mismatch"));
        }
        if snapshot.bytes[..4] != snapshot.version.to_le_bytes()
            || snapshot.bytes[4..8] != snapshot.checksum.to_le_bytes()
        {
            return Err(invalid("account buffer schema stamp mismatch"));
        }
        bytes.extend_from_slice(&snapshot.revision.to_le_bytes());
        bytes.extend_from_slice(&snapshot.version.to_le_bytes());
        bytes.extend_from_slice(&snapshot.checksum.to_le_bytes());
        bytes.extend_from_slice(&snapshot.bytes);
        bytes.extend_from_slice(&snapshot.skills.encode());
    }
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> io::Result<(LocalAccount, bool)> {
    let legacy = bytes.starts_with(LEGACY_MAGIC);
    let keyed = bytes.starts_with(KEYED_MAGIC);
    let (empty, snapshot_length, tag) = if legacy {
        (LEGACY_EMPTY_BYTES, LEGACY_SNAPSHOT_BYTES, 24)
    } else if keyed {
        (EMPTY_BYTES, KEYED_SNAPSHOT_BYTES, 56)
    } else {
        (EMPTY_BYTES, SNAPSHOT_BYTES, 56)
    };
    if !matches!(bytes.len(), n if n == empty || n == snapshot_length)
        || (!legacy && !keyed && !bytes.starts_with(MAGIC))
    {
        return Err(invalid("invalid account file version or length"));
    }
    let previous_id = AccountId(bytes[8..24].try_into().unwrap());
    if previous_id.0 == [0; 16] {
        return Err(invalid("zero account identity"));
    }
    let key = if legacy {
        net::AccountKey::generate()
    } else {
        net::AccountKey::from_seed(bytes[24..56].try_into().unwrap())
    }
    .map_err(|_| invalid("invalid account signing key"))?;
    let id = key.account();
    if !legacy && previous_id != id {
        return Err(invalid("account identity does not match signing key"));
    }
    let snapshot = match bytes[tag] {
        0 if bytes.len() == empty => None,
        1 if bytes.len() == snapshot_length => {
            let header = tag + 1;
            let version = i32::from_le_bytes(bytes[header + 8..header + 12].try_into().unwrap());
            let checksum = u32::from_le_bytes(bytes[header + 12..header + 16].try_into().unwrap());
            let data_end = header + 16 + PLAYER_DATA_BUFFER_BYTES;
            let data = &bytes[header + 16..data_end];
            let skills = if legacy || keyed {
                sim::SkillRatings::default()
            } else {
                sim::SkillRatings::decode(&bytes[data_end..])
                    .map_err(|_| invalid("invalid account skill ratings"))?
            };
            if data[..4] != version.to_le_bytes() || data[4..8] != checksum.to_le_bytes() {
                return Err(invalid("account buffer schema stamp mismatch"));
            }
            Some(AccountSnapshot {
                account: id,
                revision: u64::from_le_bytes(bytes[header..header + 8].try_into().unwrap()),
                version,
                checksum,
                bytes: data.to_vec(),
                skills,
            })
        }
        _ => return Err(invalid("invalid account payload tag")),
    };
    Ok((LocalAccount { id, key, snapshot }, legacy || keyed))
}

fn read(path: &Path) -> io::Result<(LocalAccount, bool)> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((SNAPSHOT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    decode(&bytes)
}

fn random_id() -> io::Result<[u8; 16]> {
    let mut id = [0; 16];
    while id == [0; 16] {
        getrandom::fill(&mut id).map_err(|error| io::Error::other(error.to_string()))?;
    }
    Ok(id)
}

fn lock_file(path: &Path) -> io::Result<File> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut name = std::ffi::OsString::from(".");
    name.push(
        path.file_name()
            .ok_or_else(|| invalid("account path has no filename"))?,
    );
    name.push(".lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(parent.join(name))?;
    file.lock()?;
    Ok(file)
}

fn save_current(
    path: &Path,
    next: &LocalAccount,
    expected: Option<&AccountSnapshot>,
) -> io::Result<()> {
    let _lock = lock_file(path)?;
    let (current, legacy) = read(path)?;
    if legacy || current.id != next.id || current.key.public_key() != next.key.public_key() {
        return Err(invalid("account file owner changed since load"));
    }
    if current.snapshot == next.snapshot {
        return Ok(());
    }
    if current.snapshot.as_ref() != expected {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "account file changed since load",
        ));
    }
    write_atomic(path, next, false)
}

fn load_or_create(path: &Path) -> io::Result<LocalAccount> {
    let _lock = lock_file(path)?;
    load_or_create_locked(path)
}

fn load_or_create_locked(path: &Path) -> io::Result<LocalAccount> {
    match read(path) {
        Ok((account, migrate)) => {
            if migrate {
                write_atomic(path, &account, false)?;
            }
            return Ok(account);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let key = net::AccountKey::generate().map_err(|_| invalid("account key generation failed"))?;
    let account = LocalAccount {
        id: key.account(),
        key,
        snapshot: None,
    };
    match write_atomic(path, &account, true) {
        Ok(()) => Ok(account),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => load_or_create_locked(path),
        Err(error) => Err(error),
    }
}

fn write_atomic(path: &Path, account: &LocalAccount, create: bool) -> io::Result<()> {
    let bytes = encode(account)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let nonce = random_id()?;
    let suffix: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
    let temporary = parent.join(format!(".account-{suffix}.tmp"));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        if create {
            // Publish the first identity without replacing a concurrent creator's identity.
            fs::hard_link(&temporary, path)?;
            fs::remove_file(&temporary)?;
        } else {
            fs::rename(&temporary, path)?;
        }
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
