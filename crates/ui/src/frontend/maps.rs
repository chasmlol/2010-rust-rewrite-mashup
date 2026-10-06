use crate::MenuMapList;

pub fn map_label(map: &str) -> String {
    map.split_once(':')
        .map_or(map, |(_, name)| name)
        .trim_start_matches("mp_")
        .replace('_', " ")
        .to_uppercase()
}

pub fn map_preview(map: &str) -> String {
    match map.split_once(':').unwrap_or(("iw4", map)) {
        (_, "") => String::new(),
        ("t5", stem) => format!(
            "t5:material/menu_mp_map_select_{}_big",
            stem.trim_start_matches("mp_")
        ),
        ("t6", stem) => format!("t6:material/menu_{stem}_map_select_final"),
        (namespace, stem) => format!("{namespace}:material/preview_{stem}"),
    }
}

pub fn pack_maps(maps: &MenuMapList, pack: usize) -> &[String] {
    maps.0.get(pack).map_or(&[], |pack| pack.maps.as_slice())
}

#[derive(bevy::prelude::Resource, Default)]
pub struct MapPresentation {
    entries: std::collections::BTreeMap<String, (asset_core::AssetKey, String)>,
}

impl MapPresentation {
    pub fn from_tables(
        tables: &[(asset_core::AssetNamespace, asset_game::CapturedStringTable)],
    ) -> Self {
        let mut entries = std::collections::BTreeMap::new();
        for (namespace, table) in tables {
            if *namespace != asset_core::AssetNamespace::T6
                || !table.name.eq_ignore_ascii_case("mp/mapstable.csv")
            {
                continue;
            }
            for row in 0..table.rows as i32 {
                let map = table.cell(row, 0);
                if !map.starts_with("mp_") {
                    continue;
                }
                let Ok(title) = asset_core::AssetKey::new(
                    *namespace,
                    asset_core::AssetKind::Localize,
                    table.cell(row, 3),
                ) else {
                    continue;
                };
                let Ok(image) = asset_core::AssetKey::new(
                    *namespace,
                    asset_core::AssetKind::Material,
                    table.cell(row, 4),
                ) else {
                    continue;
                };
                entries.insert(
                    format!("{}:{map}", namespace.as_str()),
                    (title, image.to_string()),
                );
            }
        }
        Self { entries }
    }

    pub fn label(&self, map: &str, strings: Option<&asset_game::LocalizeCatalog>) -> String {
        self.entries
            .get(map)
            .and_then(|(title, _)| strings?.text_asset(title))
            .map(str::to_owned)
            .unwrap_or_else(|| map_label(map))
    }

    pub fn preview(&self, map: &str) -> String {
        self.entries
            .get(map)
            .map(|(_, image)| {
                let loaded = asset_core::AssetKey::parse(image)
                    .is_ok_and(|key| asset_material::has_zone_ui_image(key.namespace, &key.name));
                if loaded {
                    image.clone()
                } else {
                    map_preview(map)
                }
            })
            .unwrap_or_else(|| map_preview(map))
    }
}
