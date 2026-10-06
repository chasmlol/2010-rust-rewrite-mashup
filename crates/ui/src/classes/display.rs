use crate::{ClassLoadoutCatalog, ClassPickerFolder};

pub fn localized(loc: &asset_game::LocalizeCatalog, key: &str, fallback: &str) -> String {
    loc.text(key.trim_start_matches('@'))
        .unwrap_or(fallback)
        .to_owned()
}

pub fn category_label(folder: ClassPickerFolder, loc: &asset_game::LocalizeCatalog) -> String {
    folder.category.map_or_else(
        || localized(loc, "MENU_EQUIPMENT_CAPS", "Equipment"),
        |category| localized(loc, category.loc_key().unwrap_or(""), category.menu_label()),
    )
}

pub fn label(
    key: &str,
    catalog: &ClassLoadoutCatalog,
    loc: &asset_game::LocalizeCatalog,
) -> String {
    if key.is_empty() {
        return localized(loc, "MENU_NONE", "None");
    }
    if let Some(preview) = catalog.previews.get(key)
        && let Some(text) = loc.text(preview.name_key.trim_start_matches('@'))
    {
        return text.to_owned();
    }
    if let Some(presentation) = catalog.presentation.get(key) {
        return loc
            .text(presentation.fallback_name_key())
            .unwrap_or(presentation.fallback_label())
            .to_owned();
    }
    key.to_owned()
}

pub fn preview_image(key: &str, catalog: &ClassLoadoutCatalog) -> String {
    catalog
        .previews
        .get(key)
        .map_or_else(String::new, |preview| preview.image.clone())
}
