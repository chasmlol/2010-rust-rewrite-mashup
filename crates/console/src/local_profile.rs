use std::{fs, path::PathBuf};

use bevy::prelude::*;
use sim::LocalPlayerProfile;

#[derive(Resource, Default)]
pub(crate) struct ProfilePersistence {
    path: Option<PathBuf>,
    saved: LocalPlayerProfile,
}

pub(crate) fn load(
    identity: Res<ui::LaunchIdentity>,
    role: Res<frame::RuntimeRole>,
    mut profile: ResMut<LocalPlayerProfile>,
    authority: Option<ResMut<net::AuthorityWorld>>,
    mut persistence: ResMut<ProfilePersistence>,
) {
    if *role != frame::RuntimeRole::Listen {
        return;
    }
    let path = std::env::var_os("IW4L_PROFILE_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            crate::user_settings::settings_path(&identity.artifacts)
                .map(|path| path.with_file_name("profile.cfg"))
        });
    if let Some(path) = path {
        match fs::read_to_string(&path) {
            Ok(source) => match parse(&source) {
                Ok(loaded) => *profile = loaded,
                Err(error) => warn!("could not parse {}: {error}", path.display()),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => warn!("could not read {}: {error}", path.display()),
        }
        persistence.path = Some(path);
    } else {
        warn!("no HOME or XDG_CONFIG_HOME; local player profile is session-only");
    }
    persistence.saved = *profile;
    if let Some(mut authority) = authority {
        authority.0.set_local_player_profile(*profile);
    }
}

pub(crate) fn save(
    role: Res<frame::RuntimeRole>,
    authority: Option<Res<net::AuthorityWorld>>,
    mut profile: ResMut<LocalPlayerProfile>,
    mut persistence: ResMut<ProfilePersistence>,
) {
    if *role != frame::RuntimeRole::Listen {
        return;
    }
    if let Some(authority) = authority {
        let current = authority.0.local_player_profile();
        if *profile != current {
            *profile = current;
        }
    }
    if *profile == persistence.saved {
        return;
    }
    let Some(path) = persistence.path.as_ref() else {
        return;
    };
    let result = write_profile(path, &profile);
    match result {
        Ok(()) => persistence.saved = *profile,
        Err(error) => warn!("could not save {}: {error}", path.display()),
    }
}

fn parse(source: &str) -> Result<LocalPlayerProfile, String> {
    let mut profile = LocalPlayerProfile::default();
    for line in source.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| "expected a profile field=value line".to_owned())?;
        let value = value
            .trim()
            .parse::<u8>()
            .map_err(|_| format!("invalid byte for {}", name.trim()))?;
        if !profile.set(name.trim(), value) {
            return Err(format!("unknown profile field {}", name.trim()));
        }
    }
    Ok(profile)
}

fn serialize(profile: &LocalPlayerProfile) -> String {
    let mut source = String::from("# iw4l local player profile v1\n");
    for name in LocalPlayerProfile::FIELDS {
        source.push_str(&format!("{name}={}\n", profile.get(name).unwrap()));
    }
    source
}

fn write_profile(path: &std::path::Path, profile: &LocalPlayerProfile) -> std::io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("cfg.tmp");
    fs::write(&temporary, serialize(profile))?;
    fs::rename(temporary, path)
}
