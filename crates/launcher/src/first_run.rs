//! First-launch setup: find the player's MW2 install, offer to take their
//! Skate 3 default.xex, convert the few Skate 3 files skating needs with the
//! bundled converter, and record both in `.env` beside the executable. Skate
//! 3 is optional: without it the game plays with skating off. Every later
//! launch goes straight in.
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

const TITLE: &str = "2010 Rust Rewrite Mashup";
/// `.env` key recording that the player chose to play without Skate 3.
const SKATE_SWITCH: &str = "IW4L_SKATE";
const MW2_FOLDER: &str = "Call of Duty Modern Warfare 2";

pub fn prepare() -> Result<(), String> {
    let root = std::env::current_dir().map_err(|error| error.to_string())?;
    let env_path = root.join(".env");
    let mut env = EnvFile::read(&env_path);

    if !env
        .get("IW4L_GAMES")
        .is_some_and(|path| mw2_ready(Path::new(path)))
    {
        let games = locate_mw2()?;
        env.set("IW4L_GAMES", &games);
        env.write(&env_path)?;
    }

    if std::env::var(SKATE_SWITCH).as_deref() == Ok("off") || env.get(SKATE_SWITCH) == Some("off") {
        return Ok(());
    }
    let assets = match env
        .get("IW4L_SKATE_ASSETS")
        .map(PathBuf::from)
        .filter(|path| skate_ready(path))
    {
        Some(assets) => assets,
        None => match convert_skate(&root)? {
            Some(assets) => {
                env.set("IW4L_SKATE_ASSETS", &assets);
                env.write(&env_path)?;
                assets
            }
            None => {
                env.set_value(SKATE_SWITCH, "off");
                env.write(&env_path)?;
                return Ok(());
            }
        },
    };
    assets::skate_board::ensure(&assets)
        .map_err(|error| format!("Could not prepare the skateboard: {error}"))
}

pub fn fail(message: &str) -> ! {
    MessageDialog::new()
        .set_title(TITLE)
        .set_level(MessageLevel::Error)
        .set_description(message)
        .show();
    std::process::exit(2);
}

fn mw2_ready(path: &Path) -> bool {
    path.is_dir()
        && assets::find_zone_file(&asset_transport::GamesRoot(path.to_owned()), "iw4:mp_rust")
            .is_ok()
}

fn locate_mw2() -> Result<PathBuf, String> {
    if let Some(found) = steam_libraries()
        .into_iter()
        .map(|library| library.join("steamapps").join("common").join(MW2_FOLDER))
        .find(|path| mw2_ready(path))
    {
        let use_it = MessageDialog::new()
            .set_title(TITLE)
            .set_description(format!(
                "Found Modern Warfare 2 here:\n\n{}\n\nUse this copy?",
                found.display()
            ))
            .set_buttons(MessageButtons::YesNo)
            .show();
        if use_it == MessageDialogResult::Yes {
            return Ok(found);
        }
    } else {
        inform(
            "Welcome! First, select your Call of Duty: Modern Warfare 2 folder \
             (the one containing iw4mp.exe and the zone folder).",
        );
    }
    loop {
        let picked = rfd::FileDialog::new()
            .set_title("Select your Modern Warfare 2 folder")
            .pick_folder()
            .ok_or("Setup cancelled: no Modern Warfare 2 folder was selected.")?;
        if mw2_ready(&picked) {
            return Ok(picked);
        }
        inform(&format!(
            "{}\n\ndoesn't look like Modern Warfare 2 with its multiplayer maps. \
             Select the folder that contains iw4mp.exe and the zone folder.",
            picked.display()
        ));
    }
}

/// Steam's own install plus every library it lists, and the conventional
/// `SteamLibrary` folder on each drive.
fn steam_libraries() -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    for steam in [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"] {
        let steam = PathBuf::from(steam);
        let listing = steam.join("steamapps").join("libraryfolders.vdf");
        if let Ok(text) = std::fs::read_to_string(listing) {
            for line in text.lines() {
                let mut fields = line.split('"').filter(|field| !field.trim().is_empty());
                if fields.next() == Some("path")
                    && let Some(path) = fields.next()
                {
                    libraries.push(PathBuf::from(path.replace(r"\\", r"\")));
                }
            }
        }
        libraries.push(steam);
    }
    for drive in 'C'..='Z' {
        libraries.push(PathBuf::from(format!(r"{drive}:\SteamLibrary")));
    }
    libraries
}

/// Every file the skate host and the board export read.
fn skate_ready(assets: &Path) -> bool {
    [
        "private/skater.glb",
        "private/game.json",
        "private/stock/physics-skeletons.json",
        "private/stock/skater-collections.json",
    ]
    .iter()
    .all(|file| assets.join(file).is_file())
        && {
            let catalog = assets.join("private/park-props/catalog.json");
            park_prop_catalog_ready(&catalog)
                || (!catalog.exists()
                    && assets
                        .join("private/park-props/availability.json")
                        .is_file())
        }
}

fn park_prop_catalog_ready(path: &Path) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|catalog| {
            if catalog.get("schema")?.as_u64() != Some(3) {
                return None;
            }
            let props = catalog.get("props")?.as_array()?;
            Some(props.iter().any(|prop| {
                prop.get("model")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
                    && prop
                        .get("collision")
                        .and_then(serde_json::Value::as_str)
                        .is_some()
            }))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::park_prop_catalog_ready;

    #[test]
    fn requires_the_runtime_catalog_schema() {
        let path = std::env::temp_dir().join(format!(
            "iw4l-prop-readiness-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(
            &path,
            br#"{"schema":2,"props":[{"model":"model.npz","collision":"collision.npz"}]}"#,
        )
        .unwrap();
        assert!(!park_prop_catalog_ready(&path));
        std::fs::write(
            &path,
            br#"{"schema":3,"props":[{"model":"model.bin","collision":"collision.bin"}]}"#,
        )
        .unwrap();
        assert!(park_prop_catalog_ready(&path));
        let _ = std::fs::remove_file(path);
    }
}

/// The converted Skate 3 data, or none when the player plays without it.
fn convert_skate(root: &Path) -> Result<Option<PathBuf>, String> {
    let converter = root.join("skate").join("iw4l-skate-convert.exe");
    if !converter.is_file() {
        return Err(format!(
            "{} is missing.\nRe-extract the release zip.",
            converter.display()
        ));
    }
    let answer = MessageDialog::new()
        .set_title(TITLE)
        .set_description(
            "Do you have Skate 3 (Xbox 360)?\n\n\
             Skate mode needs your extracted Skate 3 default.xex, with the game's \
             data folder beside it. ISO files do not work.\n\n\
             Choose No to play MW2 and the Minecraft world without skating. \
             Delete the .env file beside iw4l.exe to be asked again.",
        )
        .set_buttons(MessageButtons::YesNo)
        .show();
    if answer != MessageDialogResult::Yes {
        return Ok(None);
    }
    let out = root.join("skate-data");
    loop {
        let Some(xex) = rfd::FileDialog::new()
            .set_title("Select your Skate 3 default.xex")
            .add_filter("Skate 3 default.xex", &["xex"])
            .pick_file()
        else {
            return Ok(None);
        };
        println!("Converting Skate 3 data from {}", xex.display());
        match run_converter(&converter, &xex, &out) {
            Ok(()) => break,
            Err(error) => inform(&format!("{error}\n\nSelect default.xex again.")),
        }
    }
    let assets = out.join("assets");
    if skate_ready(&assets) {
        Ok(Some(assets))
    } else {
        Err(format!(
            "The Skate 3 conversion finished but {} is incomplete.",
            assets.display()
        ))
    }
}

/// Runs the converter with its progress echoed to this console, returning the
/// converter's own error line when it fails.
fn run_converter(converter: &Path, xex: &Path, out: &Path) -> Result<(), String> {
    let mut child = Command::new(converter)
        .arg("--xex")
        .arg(xex)
        .arg("--out")
        .arg(out)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start the Skate 3 converter: {error}"))?;
    let mut failure = None;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            println!("  {line}");
            if let Some(message) = line.strip_prefix("ERROR: ") {
                failure = Some(message.to_owned());
            }
        }
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    if status.success() {
        return Ok(());
    }
    Err(failure.unwrap_or_else(|| format!("The Skate 3 converter stopped ({status}).")))
}

fn inform(message: &str) {
    MessageDialog::new()
        .set_title(TITLE)
        .set_description(message)
        .show();
}

/// `.env` lines, rewritten in place: keys this setup owns are replaced and
/// every other line is kept as the player left it.
struct EnvFile {
    lines: Vec<String>,
}

impl EnvFile {
    fn read(path: &Path) -> Self {
        let lines = std::fs::read_to_string(path)
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_else(|_| Vec::new());
        Self { lines }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().rev().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().trim_matches('"'))
        })
    }

    fn set(&mut self, key: &str, value: &Path) {
        let value = value.display().to_string().replace('\\', "/");
        self.lines.retain(|line| {
            line.split_once('=')
                .is_none_or(|(name, _)| name.trim() != key)
        });
        self.lines.push(format!("{key}=\"{value}\""));
    }

    fn set_value(&mut self, key: &str, value: &str) {
        self.lines.retain(|line| {
            line.split_once('=')
                .is_none_or(|(name, _)| name.trim() != key)
        });
        self.lines.push(format!("{key}={value}"));
    }

    fn write(&self, path: &Path) -> Result<(), String> {
        let mut text = self.lines.join("\n");
        text.push('\n');
        std::fs::write(path, text)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))
    }
}
