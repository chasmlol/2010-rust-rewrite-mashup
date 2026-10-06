use asset_core::AssetNamespace;
use bevy::prelude::*;
use frame::{UiMenuDvars, UiMenuRequest};
use ui::classes::display::{category_label, label, localized, preview_image};
use ui::{ClassEditRow, ClassLoadoutCatalog, ClassPickerFolder, SessionClassStore};

use crate::{CommandSpec, ConsoleCommand, ConsoleRegistry};

const PAGE_SIZE: usize = 10;

#[derive(Default)]
pub(crate) struct ClassMenuState {
    row: Option<ClassEditRow>,
    game: Option<AssetNamespace>,
    folder: Option<ClassPickerFolder>,
    page: usize,
    attachments: bool,
    camo: bool,
    hover: usize,
}

fn camo_names(catalog: &ClassLoadoutCatalog, weapon: &str) -> Vec<String> {
    let Some(registry) = catalog.resolver.0.as_deref() else {
        return Vec::new();
    };
    let Ok(id) =
        session::resolve_class_weapon(registry, weapon, &[], asset_game::LoadoutRules::default())
    else {
        return Vec::new();
    };
    registry
        .camouflage_choices(id)
        .into_iter()
        .map(|(_, name)| name.to_owned())
        .collect()
}

fn camo_preview(catalog: &ClassLoadoutCatalog, weapon: &str, camo: &str) -> String {
    let Some(registry) = catalog.resolver.0.as_deref() else {
        return String::new();
    };
    session::resolve_class_weapon(registry, weapon, &[], asset_game::LoadoutRules::default())
        .map_or_else(
            |_| String::new(),
            |id| registry.camouflage_preview(id, camo),
        )
}

fn camo_label(camo: &str) -> String {
    if camo.is_empty() {
        return "None".into();
    }
    camo.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn camo_slot(row: ClassEditRow) -> Option<usize> {
    match row {
        ClassEditRow::Primary => Some(0),
        ClassEditRow::Secondary => Some(1),
        _ => None,
    }
}

impl ClassMenuState {
    fn games(&self, catalog: &ClassLoadoutCatalog) -> Vec<AssetNamespace> {
        let mut games = Vec::new();
        if let Some(row) = self.row {
            for folder in catalog.categories_for(row) {
                if !games.contains(&folder.namespace) {
                    games.push(folder.namespace);
                }
            }
        }
        games
    }

    fn folders(&self, catalog: &ClassLoadoutCatalog) -> Vec<ClassPickerFolder> {
        self.row
            .map(|row| {
                catalog
                    .categories_for(row)
                    .into_iter()
                    .filter(|folder| Some(folder.namespace) == self.game)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn choices(&self, catalog: &ClassLoadoutCatalog, store: &SessionClassStore) -> Vec<String> {
        let Some(row) = self.row else {
            return Vec::new();
        };
        if self.camo {
            let Some(slot) = store.slots.get(store.selected) else {
                return Vec::new();
            };
            std::iter::once(String::new())
                .chain(camo_names(catalog, slot.row_value(row)))
                .collect()
        } else if self.attachments {
            let Some(slot) = store.slots.get(store.selected) else {
                return Vec::new();
            };
            std::iter::once(String::new())
                .chain(
                    catalog
                        .attachments(row, slot.row_value(row))
                        .iter()
                        .cloned(),
                )
                .collect()
        } else if let Some(folder) = self.folder {
            catalog.keys_in_category(row, folder)
        } else {
            catalog.options(row)
        }
    }
}

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    for name in [
        "ui_class_slot",
        "ui_class_preview_slot",
        "ui_class_edit",
        "ui_class_game",
        "ui_class_category",
        "ui_class_pick",
        "ui_class_page",
        "ui_class_attachments",
        "ui_class_camo",
        "ui_class_reset",
        "ui_class_rename",
        "ui_class_hover",
    ] {
        registry.register(CommandSpec::new(name));
    }
}

fn index(command: &ConsoleCommand) -> Result<usize, String> {
    command
        .args
        .first()
        .and_then(|arg| arg.parse().ok())
        .ok_or_else(|| "Invalid class selection".into())
}

pub(crate) fn route(
    mut events: MessageReader<ConsoleCommand>,
    mut state: Local<ClassMenuState>,
    mut store: ResMut<SessionClassStore>,
    catalog: Res<ClassLoadoutCatalog>,
    loc: Res<asset_game::LocalizeCatalog>,
    mut dvars: ResMut<UiMenuDvars>,
    mut menus: MessageWriter<UiMenuRequest>,
    mut echo: crate::feature_dispatch::ConsoleEcho,
) {
    for command in events
        .read()
        .filter(|command| command.name.starts_with("ui_class_"))
    {
        let result = (|| -> Result<(), String> {
            match command.name.as_str() {
                "ui_class_hover" => state.hover = index(command)?.min(PAGE_SIZE - 1),
                "ui_class_preview_slot" => {
                    let selected = index(command)?;
                    store.slots.get(selected).ok_or("Class is unavailable")?;
                    store.selected = selected;
                }
                "ui_class_slot" => {
                    let selected = index(command)?;
                    let slot = store.slots.get(selected).ok_or("Class is unavailable")?;
                    dvars.set("ui_class_name", &slot.name);
                    store.selected = selected;
                    *state = ClassMenuState::default();
                    menus.write(UiMenuRequest::Open("class_editor".into()));
                }
                "ui_class_edit" | "ui_class_attachments" => {
                    let row = ClassEditRow::ALL
                        .get(index(command)?)
                        .copied()
                        .ok_or("Unknown loadout slot")?;
                    state.row = Some(row);
                    state.game = None;
                    state.folder = None;
                    state.page = 0;
                    state.hover = 0;
                    state.attachments = command.name == "ui_class_attachments";
                    state.camo = false;
                    if state.attachments
                        && !matches!(row, ClassEditRow::Primary | ClassEditRow::Secondary)
                    {
                        return Err("This slot does not accept attachments".into());
                    }
                    let menu = if !state.attachments && ClassLoadoutCatalog::uses_categories(row) {
                        "class_games"
                    } else {
                        "class_picker"
                    };
                    menus.write(UiMenuRequest::Open(menu.into()));
                }
                "ui_class_camo" => {
                    let row = match index(command)? {
                        0 => ClassEditRow::Primary,
                        1 => ClassEditRow::Secondary,
                        _ => return Err("Unknown weapon slot".into()),
                    };
                    let slot = store
                        .slots
                        .get(store.selected)
                        .ok_or("Class is unavailable")?;
                    if camo_names(&catalog, slot.row_value(row)).is_empty() {
                        return Err("This weapon has no camouflage".into());
                    }
                    *state = ClassMenuState {
                        row: Some(row),
                        camo: true,
                        ..Default::default()
                    };
                    menus.write(UiMenuRequest::Open("class_picker".into()));
                }
                "ui_class_game" => {
                    let games = state.games(&catalog);
                    state.game = Some(*games.get(index(command)?).ok_or("Game is unavailable")?);
                    state.folder = None;
                    state.page = 0;
                    state.hover = 0;
                    let folders = state.folders(&catalog);
                    if folders.len() == 1 && folders[0].category.is_none() {
                        state.folder = Some(folders[0]);
                        menus.write(UiMenuRequest::Open("class_picker".into()));
                    } else {
                        menus.write(UiMenuRequest::Open("class_categories".into()));
                    }
                }
                "ui_class_category" => {
                    state.folder = Some(
                        *state
                            .folders(&catalog)
                            .get(index(command)?)
                            .ok_or("Category is unavailable")?,
                    );
                    state.page = 0;
                    state.hover = 0;
                    menus.write(UiMenuRequest::Open("class_picker".into()));
                }
                "ui_class_page" => {
                    let len = state.choices(&catalog, &store).len();
                    let delta = command
                        .args
                        .first()
                        .and_then(|arg| arg.parse::<isize>().ok())
                        .ok_or("Invalid page")?;
                    state.hover = 0;
                    state.page = state
                        .page
                        .saturating_add_signed(delta)
                        .min(len.saturating_sub(1) / PAGE_SIZE);
                }
                "ui_class_pick" => {
                    let row = state.row.ok_or("No loadout slot selected")?;
                    let at = index(command)?;
                    if at >= PAGE_SIZE {
                        return Err("Invalid item row".into());
                    }
                    let choices = state.choices(&catalog, &store);
                    let value = choices
                        .get(state.page * PAGE_SIZE + at)
                        .ok_or("Item is unavailable")?
                        .clone();
                    let selected = store.selected;
                    let slot = store
                        .slots
                        .get_mut(selected)
                        .ok_or("Class is unavailable")?;
                    let mut candidate = slot.clone();
                    if state.camo {
                        let at = camo_slot(row).ok_or("This slot takes no camouflage")?;
                        candidate.camos[at] = value;
                    } else if state.attachments {
                        let mut chosen = if row == ClassEditRow::Primary {
                            candidate.primary_attachments.clone()
                        } else {
                            candidate.secondary_attachments.clone()
                        };
                        if value.is_empty() {
                            chosen.clear();
                        } else if let Some(at) = chosen.iter().position(|name| name == &value) {
                            chosen.remove(at);
                        } else {
                            if candidate.loadout_rules().max_attachments == 1 {
                                chosen.clear();
                            }
                            chosen.push(value);
                        }
                        catalog
                            .check_attachments(
                                candidate.row_value(row),
                                &chosen,
                                candidate.loadout_rules(),
                            )
                            .map_err(|error| format!("{error:?}"))?;
                        if row == ClassEditRow::Primary {
                            candidate.primary_attachments = chosen;
                        } else {
                            candidate.secondary_attachments = chosen;
                        }
                    } else {
                        candidate.set_row(row, value);
                        let rules = candidate.loadout_rules();
                        for (weapon, chosen) in [
                            (&candidate.primary, &mut candidate.primary_attachments),
                            (&candidate.secondary, &mut candidate.secondary_attachments),
                        ] {
                            while !chosen.is_empty()
                                && catalog.check_attachments(weapon, chosen, rules).is_err()
                            {
                                chosen.pop();
                            }
                        }
                        let weapons = [candidate.primary.clone(), candidate.secondary.clone()];
                        for (weapon, camo) in weapons.iter().zip(&mut candidate.camos) {
                            if !camo.is_empty() && !camo_names(&catalog, weapon).contains(camo) {
                                camo.clear();
                            }
                        }
                    }
                    catalog.validate_edit(&candidate, row)?;
                    candidate.lock_reason = catalog.validate_class(&candidate).err();
                    if slot.lock_reason.is_none()
                        && let Some(reason) = &candidate.lock_reason
                    {
                        return Err(reason.clone());
                    }
                    *slot = candidate;
                    if state.camo {
                        let camo = camo_slot(row).map_or("", |at| slot.camos[at].as_str());
                        echo.write(format!(
                            "menu: class {} {} camouflage = {}",
                            selected + 1,
                            row.label(),
                            camo_label(camo)
                        ));
                    } else {
                        echo.write(format!(
                            "menu: class {} {} = {}",
                            selected + 1,
                            row.label(),
                            slot.row_value(row)
                        ));
                    }
                    for menu in ["class_picker", "class_categories", "class_games"] {
                        menus.write(UiMenuRequest::Close(menu.into()));
                    }
                    let has_camo = !camo_names(&catalog, slot.row_value(row)).is_empty();
                    if !state.attachments
                        && !state.camo
                        && matches!(row, ClassEditRow::Primary | ClassEditRow::Secondary)
                    {
                        state.attachments = true;
                        state.page = 0;
                        state.hover = 0;
                        menus.write(UiMenuRequest::Open("class_picker".into()));
                    } else if state.attachments && has_camo {
                        state.attachments = false;
                        state.camo = true;
                        state.page = 0;
                        state.hover = 0;
                        menus.write(UiMenuRequest::Open("class_picker".into()));
                    }
                }
                "ui_class_reset" => {
                    let selected = store.selected;
                    let slot = store
                        .slots
                        .get_mut(selected)
                        .ok_or("Class is unavailable")?;
                    let candidate = if let Some(preset) = ui::showcase_classes()
                        .iter()
                        .find(|preset| preset.name == slot.name)
                    {
                        ui::ClassSlotState::from_preset(preset)
                    } else {
                        let registry = catalog
                            .resolver
                            .0
                            .as_deref()
                            .ok_or("Weapon catalog is not ready")?;
                        let mut default = ui::SessionClassStore::from_showcase(0, registry)
                            .slots
                            .into_iter()
                            .nth(selected)
                            .ok_or("No available default class")?;
                        default.name.clone_from(&slot.name);
                        default
                    };
                    catalog.validate_class(&candidate)?;
                    *slot = candidate;
                    dvars.set("ui_class_name", &slot.name);
                }
                "ui_class_rename" => {
                    let name: String = dvars
                        .get("ui_class_name")
                        .unwrap_or_default()
                        .trim()
                        .chars()
                        .take(20)
                        .collect();
                    if name.is_empty() {
                        return Err(localized(
                            &loc,
                            "MENU_IWNET_CREATE_BADNAME",
                            "Enter a class name",
                        ));
                    }
                    let selected = store.selected;
                    store
                        .slots
                        .get_mut(selected)
                        .ok_or("Class is unavailable")?
                        .name = name;
                    menus.write(UiMenuRequest::Close("class_rename".into()));
                }
                _ => {}
            }
            if !matches!(
                command.name.as_str(),
                "ui_class_hover" | "ui_class_preview_slot"
            ) {
                dvars.set("ui_class_status", "");
            }
            Ok(())
        })();
        if let Err(error) = result {
            dvars.set("ui_class_status", &error);
            echo.write(format!("menu: {error}"));
        }
    }
    for at in 0..10 {
        dvars.set(
            &format!("ui_class_slot_{at}"),
            store
                .slots
                .get(at)
                .map(|slot| slot.name.clone())
                .unwrap_or_default(),
        );
    }
    if let Some(slot) = store.slots.get(store.selected) {
        if dvars.get("ui_class_status").is_none_or(str::is_empty)
            && let Some(reason) = &slot.lock_reason
        {
            dvars.set("ui_class_status", format!("Class unavailable: {reason}"));
        }
        dvars.set("ui_class_title", &slot.name);
        dvars.set("ui_class_saved_name", &slot.name);
        dvars.set("ui_class_index", store.selected.to_string());
        for row in ClassEditRow::ALL {
            dvars.set(
                &format!("ui_class_value_{}", row.as_u8()),
                label(slot.row_value(row), &catalog, &loc),
            );
            dvars.set(
                &format!("ui_class_image_{}", row.as_u8()),
                preview_image(slot.row_value(row), &catalog),
            );
            dvars.set(
                &format!("ui_class_desc_{}", row.as_u8()),
                catalog
                    .previews
                    .get(slot.row_value(row))
                    .and_then(|preview| loc.text(preview.desc_key.trim_start_matches('@')))
                    .unwrap_or_default(),
            );
        }
        for (at, camo) in slot.camos.iter().enumerate() {
            dvars.set(
                &format!("ui_class_camo_image_{at}"),
                camo_preview(
                    &catalog,
                    if at == 0 {
                        &slot.primary
                    } else {
                        &slot.secondary
                    },
                    camo,
                ),
            );
        }
        for (name, selected) in [
            ("primary", &slot.primary_attachments),
            ("secondary", &slot.secondary_attachments),
        ] {
            for at in 0..2 {
                let weapon = if name == "primary" {
                    &slot.primary
                } else {
                    &slot.secondary
                };
                let key = selected
                    .get(at)
                    .map(|attachment| format!("{weapon}+{attachment}"))
                    .unwrap_or_default();
                dvars.set(
                    &format!("ui_class_{name}_attachment_image_{at}"),
                    preview_image(&key, &catalog),
                );
                dvars.set(
                    &format!("ui_class_{name}_attachment_name_{at}"),
                    if key.is_empty() {
                        String::new()
                    } else {
                        label(&key, &catalog, &loc)
                    },
                );
            }
            dvars.set(
                &format!("ui_class_{name}_attachments"),
                if selected.is_empty() {
                    label("", &catalog, &loc)
                } else {
                    selected
                        .iter()
                        .map(|attachment| {
                            label(
                                &format!(
                                    "{}+{attachment}",
                                    if name == "primary" {
                                        &slot.primary
                                    } else {
                                        &slot.secondary
                                    }
                                ),
                                &catalog,
                                &loc,
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" + ")
                },
            );
        }
    }
    let games = state.games(&catalog);
    let folders = state.folders(&catalog);
    let choices = state.choices(&catalog, &store);
    dvars.set("ui_class_game_count", games.len().to_string());
    dvars.set("ui_class_category_count", folders.len().to_string());
    dvars.set(
        "ui_class_choice_count",
        choices
            .len()
            .saturating_sub(state.page * PAGE_SIZE)
            .min(PAGE_SIZE)
            .to_string(),
    );
    dvars.set(
        "ui_class_more_pages",
        if choices.len() > PAGE_SIZE { "1" } else { "0" },
    );
    let picker_depth = if state.attachments
        || state.camo
        || state
            .row
            .is_none_or(|row| !ClassLoadoutCatalog::uses_categories(row))
    {
        1
    } else if state.folder.is_some_and(|folder| folder.category.is_none()) {
        2
    } else {
        3
    };
    dvars.set("ui_class_picker_depth", picker_depth.to_string());
    let mut caption = state.row.map_or_else(
        || localized(&loc, "MENU_CLASSES", "Classes"),
        |row| localized(&loc, row.loc_key(), row.label()),
    );
    if state.attachments {
        caption.push_str(" / ");
        caption.push_str(&localized(&loc, "MENU_ATTACHMENTS_CAPS", "Attachments"));
    } else if state.camo {
        caption.push_str(" / Camouflage");
    }
    dvars.set("ui_class_caption", caption);
    dvars.set(
        "ui_class_game_label",
        state
            .game
            .map(|game| game.as_str().to_uppercase())
            .unwrap_or_default(),
    );
    dvars.set(
        "ui_class_category_label",
        state
            .folder
            .map(|folder| category_label(folder, &loc))
            .unwrap_or_default(),
    );
    state.page = state.page.min(choices.len().saturating_sub(1) / PAGE_SIZE);
    for at in 0..PAGE_SIZE {
        dvars.set(
            &format!("ui_class_game_{at}"),
            games
                .get(at)
                .map(|game| game.as_str().to_uppercase())
                .unwrap_or_default(),
        );
        dvars.set(
            &format!("ui_class_category_{at}"),
            folders
                .get(at)
                .map(|folder| category_label(*folder, &loc))
                .unwrap_or_default(),
        );
        dvars.set(
            &format!("ui_class_choice_{at}"),
            choices
                .get(state.page * PAGE_SIZE + at)
                .map(|value| {
                    let slot = store.slots.get(store.selected);
                    if state.camo {
                        let selected = slot
                            .zip(state.row.and_then(camo_slot))
                            .is_some_and(|(slot, at)| slot.camos[at] == *value);
                        return format!(
                            "{}{}",
                            if selected { "* " } else { "" },
                            camo_label(value)
                        );
                    }
                    let key = if state.attachments && !value.is_empty() {
                        slot.zip(state.row)
                            .map(|(slot, row)| format!("{}+{value}", slot.row_value(row)))
                            .unwrap_or_default()
                    } else {
                        value.clone()
                    };
                    let selected = slot.zip(state.row).is_some_and(|(slot, row)| {
                        if state.attachments {
                            let chosen = if row == ClassEditRow::Primary {
                                &slot.primary_attachments
                            } else {
                                &slot.secondary_attachments
                            };
                            if value.is_empty() {
                                chosen.is_empty()
                            } else {
                                chosen.contains(value)
                            }
                        } else {
                            slot.row_value(row) == value
                        }
                    });
                    format!(
                        "{}{}",
                        if selected { "* " } else { "" },
                        label(&key, &catalog, &loc)
                    )
                })
                .unwrap_or_default(),
        );
    }
    let title = if state.camo {
        "Camouflage".to_owned()
    } else if state.attachments {
        localized(&loc, "MENU_ATTACHMENTS_CAPS", "Attachments")
    } else if let Some(folder) = state.folder {
        format!(
            "{} / {}",
            folder.namespace.as_str().to_uppercase(),
            category_label(folder, &loc)
        )
    } else {
        state.row.map_or_else(
            || localized(&loc, "MENU_WEAPON_CLASSES_CAPS", "Classes"),
            |row| localized(&loc, row.loc_key(), row.label()),
        )
    };
    dvars.set("ui_class_picker_title", title);
    let preview = choices
        .get(state.page * PAGE_SIZE + state.hover)
        .filter(|_| !state.camo)
        .and_then(|key| {
            let lookup = if state.attachments {
                let slot = store.slots.get(store.selected)?;
                format!("{}+{key}", slot.row_value(state.row?))
            } else {
                key.clone()
            };
            let preview = catalog.previews.get(&lookup)?;
            let ns = asset_core::AssetKey::parse(&lookup).ok()?.namespace;
            Some((ns, preview))
        });
    let image = preview
        .filter(|(_, preview)| !preview.image.is_empty())
        .map(|(ns, preview)| {
            if asset_core::AssetKey::parse(&preview.image).is_ok() {
                preview.image.clone()
            } else {
                format!("{}:material/{}", ns.as_str(), preview.image)
            }
        })
        .unwrap_or_default();
    let image = if state.camo {
        choices
            .get(state.page * PAGE_SIZE + state.hover)
            .filter(|camo| !camo.is_empty())
            .map_or_else(String::new, |camo| {
                camo_preview(
                    &catalog,
                    store
                        .slots
                        .get(store.selected)
                        .zip(state.row)
                        .map_or("", |(slot, row)| slot.row_value(row)),
                    camo,
                )
            })
    } else {
        image
    };
    dvars.set("ui_class_preview", image);
    let square_preview = state.attachments
        || matches!(
            state.row,
            Some(
                ClassEditRow::Perk1
                    | ClassEditRow::Perk2
                    | ClassEditRow::Perk3
                    | ClassEditRow::Deathstreak
            )
        );
    dvars.set(
        "ui_class_preview_width",
        if square_preview { "100" } else { "200" },
    );
    dvars.set(
        "ui_class_preview_title",
        preview
            .map(|(_, p)| localized(&loc, &p.name_key, &p.reference))
            .unwrap_or_default(),
    );
    for (at, bar) in asset_game::CacStatBar::ALL.into_iter().enumerate() {
        let value = preview.and_then(|(_, p)| {
            p.bars
                .iter()
                .find(|(kind, _)| *kind == bar)
                .map(|(_, v)| *v)
        });
        dvars.set(
            &format!("ui_class_stat_{at}"),
            value.unwrap_or(0).clamp(0, 100).to_string(),
        );
        dvars.set(
            &format!("ui_class_stat_visible_{at}"),
            if value.is_some() { "1" } else { "0" },
        );
    }
    dvars.set(
        "ui_class_description",
        preview
            .and_then(|(_, preview)| loc.text(preview.desc_key.trim_start_matches('@')))
            .unwrap_or_default(),
    );
    dvars.set(
        "ui_class_page",
        format!(
            "{} / {}",
            state.page + 1,
            choices.len().max(1).div_ceil(PAGE_SIZE)
        ),
    );
}
