use assets::{BoundWeapons, PreparedLocalizedStrings};

use crate::gaps::{GapCause, HudGap, HudPresentationGaps};

pub(crate) fn localized_weapon_name(
    viewmodel_index: u32,
    weapons: Option<&BoundWeapons<'_>>,
    strings: Option<&PreparedLocalizedStrings>,
    gaps: &mut HudPresentationGaps,
) -> Option<String> {
    let Some(weapons) = weapons else {
        gaps.raise(GapCause::NameNoWeaponCatalog);
        return None;
    };
    let Some(key) = weapons.registry().display_name_key_of(viewmodel_index) else {
        gaps.raise(GapCause::NameNoDisplayNameKey { viewmodel_index });
        return None;
    };
    gaps.clear(HudGap::WeaponDisplayName);
    let Some(strings) = strings else {
        gaps.raise(GapCause::NoStringTable);
        return None;
    };
    let text = match weapons.registry().identity_namespace_of(viewmodel_index) {
        Some(namespace) => strings.0.text_in(namespace, key),
        None => strings.0.text(key),
    };
    match text {
        Some(text) => {
            gaps.clear(HudGap::LocalizedText);
            let mut name = text.to_owned();
            for key in weapons
                .registry()
                .attachment_caption_keys_of(viewmodel_index)
            {
                let caption = match weapons.registry().identity_namespace_of(viewmodel_index) {
                    Some(namespace) => strings.0.text_in(namespace, key),
                    None => strings.0.text(key),
                };
                if let Some(caption) = caption {
                    name.push(' ');
                    name.push_str(caption);
                }
            }
            Some(name)
        }
        None => {
            gaps.raise(GapCause::LocalizedRowMissing {
                key: key.to_owned(),
            });
            None
        }
    }
}
