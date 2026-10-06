use asset_game::MenuCatalog;
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct BarracksProfile {
    pub selection: sim::PlayerProfile,
    pub loaded: bool,
    path: Option<std::path::PathBuf>,
    written: Option<sim::PlayerProfile>,
    retry_at: Option<std::time::Instant>,
}

pub fn reward_rows(catalog: &MenuCatalog) -> Vec<u32> {
    let Some(table) = catalog.string_table("mp/killstreakTable.csv") else {
        return Vec::new();
    };
    let mut rows: Vec<_> = (0..table.rows as u32)
        .filter(|&row| {
            table
                .cell(row as i32, 4)
                .parse::<u32>()
                .is_ok_and(|cost| (1..=25).contains(&cost))
                && !matches!(table.cell(row as i32, 1), "none" | "sentry")
        })
        .collect();
    rows.sort_by_key(|&row| (table.cell(row as i32, 4).parse::<u32>().unwrap_or(0), row));
    rows
}

pub fn valid_profile(catalog: &MenuCatalog, profile: sim::PlayerProfile) -> bool {
    let valid_card = |table: &str, row: u32| {
        catalog
            .string_table(table)
            .is_some_and(|table| row < table.rows as u32 && !table.cell(row as i32, 0).is_empty())
    };
    if !valid_card("mp/cardTitleTable.csv", profile.title)
        || !valid_card("mp/cardIconTable.csv", profile.emblem)
    {
        return false;
    }
    let rows = reward_rows(catalog);
    let Some(table) = catalog.string_table("mp/killstreakTable.csv") else {
        return false;
    };
    let mut costs = Vec::new();
    for row in profile.killstreaks {
        if !rows.contains(&row) {
            return false;
        }
        let cost = table.cell(row as i32, 4);
        if costs.contains(&cost) {
            return false;
        }
        costs.push(cost);
    }
    true
}

pub(crate) fn load_profile(
    identity: Option<Res<frame::LaunchIdentity>>,
    catalog: Res<MenuCatalog>,
    mut profile: ResMut<BarracksProfile>,
) {
    if profile.loaded {
        return;
    }
    let Some(identity) = identity else {
        return;
    };
    let Some(table) = catalog.string_table("mp/killstreakTable.csv") else {
        return;
    };
    let mut selection = sim::PlayerProfile::default();
    for (index, name) in ["uav", "airdrop", "predator_missile"]
        .into_iter()
        .enumerate()
    {
        let Some(row) = (0..table.rows).find(|&row| table.cell(row as i32, 1) == name) else {
            return;
        };
        selection.killstreaks[index] = row as u32;
    }
    if !valid_profile(&catalog, selection) || identity.artifacts.as_os_str().is_empty() {
        return;
    }
    profile.loaded = true;
    profile.selection = selection;
    let path = identity.artifacts.join("profile/barracks.txt");
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let mut fields = text.split_whitespace();
            let header = fields.next();
            let values: Option<Vec<u32>> = fields.map(|field| field.parse().ok()).collect();
            let loaded = values
                .and_then(|values| match values.as_slice() {
                    &[title, emblem, a, b, c] => Some(sim::PlayerProfile {
                        title,
                        emblem,
                        killstreaks: [a, b, c],
                    }),
                    _ => None,
                })
                .filter(|&selection| {
                    header == Some("iw4l-barracks-1") && valid_profile(&catalog, selection)
                });
            let Some(loaded) = loaded else {
                diag::warn!(
                    Ui,
                    "barracks: invalid profile at {}; file preserved",
                    path.display()
                );
                return;
            };
            profile.selection = loaded;
            profile.written = Some(loaded);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            diag::warn!(
                Ui,
                "barracks: cannot read {}: {error}; file preserved",
                path.display()
            );
            return;
        }
    }
    profile.path = Some(path);
}

pub(crate) fn save_profile(catalog: Res<MenuCatalog>, mut profile: ResMut<BarracksProfile>) {
    if !profile.loaded
        || profile.written == Some(profile.selection)
        || profile
            .retry_at
            .is_some_and(|at| std::time::Instant::now() < at)
        || !valid_profile(&catalog, profile.selection)
    {
        return;
    }
    let Some(path) = profile.path.clone() else {
        return;
    };
    let sim::PlayerProfile {
        title,
        emblem,
        killstreaks: [a, b, c],
    } = profile.selection;
    let contents = format!("iw4l-barracks-1\n{title} {emblem} {a} {b} {c}\n");
    let pending = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        use std::io::Write;
        std::fs::create_dir_all(path.parent().unwrap())?;
        let mut file = std::fs::File::create(&pending)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&pending, &path)
    })();
    match result {
        Ok(()) => {
            profile.written = Some(profile.selection);
            profile.retry_at = None;
        }
        Err(error) => {
            let _ = std::fs::remove_file(pending);
            profile.retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
            diag::warn!(Ui, "barracks: cannot write {}: {error}", path.display());
        }
    }
}
