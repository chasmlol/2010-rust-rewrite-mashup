use std::path::PathBuf;

use render::diag::acceptance::{ACCEPTANCE_ENV, ACCEPTANCE_FLAG, ACCEPTANCE_MAPS};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptanceLaunch {
    pub dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    Help,
    Menu,
    Map(String),
    Serve(String),
    ExportGltf(String),
    Play {
        name: String,
        zone_override: Option<String>,
    },
}

pub fn parse_cli(
    args: impl Iterator<Item = String>,
) -> Result<(LaunchMode, Option<AcceptanceLaunch>), String> {
    let (args, acceptance) = parse_acceptance_flag(args)?;
    let args = console::strip_cmds_flag(args.into_iter());
    let mode = parse_launch_args(args.into_iter())?;
    Ok((mode, acceptance))
}

fn parse_acceptance_flag(
    args: impl Iterator<Item = String>,
) -> Result<(Vec<String>, Option<AcceptanceLaunch>), String> {
    let mut out = Vec::new();
    let mut acceptance = None;
    let mut iter = args.peekable();
    while let Some(arg) = iter.next() {
        if arg == ACCEPTANCE_FLAG {
            match iter.next() {
                Some(dir) if !dir.is_empty() && !dir.starts_with('-') => {
                    acceptance = Some(AcceptanceLaunch {
                        dir: PathBuf::from(dir),
                    });
                }
                _ => {
                    return Err(format!(
                        "usage: iw4l --render-acceptance <artifact-dir> map <zone>\n\
                         env alternative: {env}=<artifact-dir>\n\
                         expected maps later: {maps}",
                        env = ACCEPTANCE_ENV,
                        maps = ACCEPTANCE_MAPS.join(", ")
                    ));
                }
            }
        } else if let Some(dir) = arg.strip_prefix(&(ACCEPTANCE_FLAG.to_owned() + "=")) {
            if dir.is_empty() {
                return Err("empty --render-acceptance path".into());
            }
            acceptance = Some(AcceptanceLaunch {
                dir: PathBuf::from(dir),
            });
        } else {
            out.push(arg);
        }
    }
    if acceptance.is_none()
        && let Some(dir) = std::env::var_os(ACCEPTANCE_ENV)
    {
        acceptance = Some(AcceptanceLaunch {
            dir: PathBuf::from(dir),
        });
    }
    Ok((out, acceptance))
}

pub const USAGE: &str = "usage: iw4l [--cmds '<script>'] map <zone> | serve <zone> | menu | play <demo>\n       iw4l export-gltf <zone>\n       iw4l --help";

pub fn parse_launch_args(mut args: impl Iterator<Item = String>) -> Result<LaunchMode, String> {
    match args.next().as_deref() {
        Some("--help") | Some("-h") => Ok(LaunchMode::Help),
        Some("map") => {
            let Some(zone) = args.next().filter(|z| !z.is_empty()) else {
                return Err("usage: iw4l map <zone> [--cmds '<script>']".into());
            };
            Ok(LaunchMode::Map(zone))
        }
        Some("serve") => {
            let Some(zone) = args.next().filter(|z| !z.is_empty()) else {
                return Err("usage: iw4l serve <zone> [--cmds '<script>']".into());
            };
            Ok(LaunchMode::Serve(zone))
        }
        Some("export-gltf") => {
            let Some(zone) = args.next().filter(|zone| !zone.is_empty()) else {
                return Err("usage: iw4l export-gltf <zone>".into());
            };
            if args.next().is_some() {
                return Err("usage: iw4l export-gltf <zone>".into());
            }
            Ok(LaunchMode::ExportGltf(zone))
        }
        Some("menu") => {
            if args.next().is_some() {
                return Err("usage: iw4l menu [--cmds '<script>']".into());
            }
            Ok(LaunchMode::Menu)
        }
        Some("play") => parse_play_args(args),
        Some(other) => Err(format!(
            "unknown launch args starting with `{other}` — expected: map <zone> | serve <zone> | menu | play <demo> [--cmds '<script>'] | export-gltf <zone>"
        )),
        None => Ok(LaunchMode::Menu),
    }
}

fn parse_play_args(args: impl Iterator<Item = String>) -> Result<LaunchMode, String> {
    let usage = "usage: iw4l play <demo> [--zone <zone>] [--cmds '<script>']";
    let mut name = None;
    let mut zone_override = None;
    let mut iter = args.peekable();
    while let Some(arg) = iter.next() {
        if arg == "--zone" {
            match iter.next() {
                Some(zone) if !zone.is_empty() && !zone.starts_with('-') => {
                    zone_override = Some(zone);
                }
                _ => return Err(usage.into()),
            }
            continue;
        }
        if let Some(zone) = arg.strip_prefix("--zone=") {
            if zone.is_empty() {
                return Err("empty --zone".into());
            }
            zone_override = Some(zone.to_owned());
            continue;
        }
        if arg.starts_with('-') {
            return Err(format!("unknown play flag `{arg}` — expected --zone"));
        }
        if name.is_some() {
            return Err(format!("{usage} (extra `{arg}`)"));
        }
        name = Some(replay::demo_stem(&arg).to_owned());
    }
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        return Err(usage.into());
    };
    Ok(LaunchMode::Play {
        name,
        zone_override,
    })
}
