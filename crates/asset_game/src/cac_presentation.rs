use asset_core::{AssetKey, AssetNamespace};

use crate::{CacWeaponPreview, CapturedStringTable};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacItemPresentation {
    fallback_label: String,
    fallback_name_key: String,
    archive_image: Option<String>,
}

impl CacItemPresentation {
    pub fn prepare_weapon(reference: &str) -> Self {
        Self {
            fallback_label: fallback_label(reference),
            fallback_name_key: fallback_name_key(reference),
            archive_image: weapon_image(reference).map(str::to_owned),
        }
    }

    pub fn prepare_attachment(reference: &str) -> Self {
        Self {
            fallback_label: fallback_label(
                reference
                    .rsplit_once('+')
                    .map_or(reference, |(_, attachment)| attachment),
            ),
            fallback_name_key: fallback_name_key(reference),
            archive_image: attachment_image(
                reference
                    .rsplit_once('+')
                    .map_or(reference, |(_, attachment)| attachment),
            )
            .map(str::to_owned),
        }
    }

    pub fn prepare_perk(reference: &str) -> Self {
        Self {
            fallback_label: fallback_label(reference),
            fallback_name_key: fallback_name_key(reference),
            archive_image: Some(material_iwd_stem(reference).to_owned()),
        }
    }

    pub fn fallback_label(&self) -> &str {
        &self.fallback_label
    }
    pub fn fallback_name_key(&self) -> &str {
        &self.fallback_name_key
    }
    pub fn archive_image(&self) -> Option<&str> {
        self.archive_image.as_deref()
    }
}

pub fn prepare_cac_preview(
    namespace: AssetNamespace,
    mut preview: CacWeaponPreview,
) -> CacWeaponPreview {
    preview.name_key = localized_key(namespace, &preview.name_key);
    preview.desc_key = localized_key(namespace, &preview.desc_key);
    if !preview.image.is_empty() && AssetKey::parse(&preview.image).is_err() {
        preview.image = format!("{}:material/{}", namespace.as_str(), preview.image);
    }
    preview
}

fn localized_key(namespace: AssetNamespace, reference: &str) -> String {
    let reference = reference.trim_start_matches('@');
    if reference.is_empty() {
        return String::new();
    }
    if namespace == AssetNamespace::Iw4 || AssetKey::parse(reference).is_ok() {
        format!("@{reference}")
    } else {
        format!("@{}:localize/{reference}", namespace.as_str())
    }
}

pub fn prepare_cac_equipment_preview(
    table: &CapturedStringTable,
    weapon: &str,
    preview: &CacWeaponPreview,
) -> Option<CacWeaponPreview> {
    let key = AssetKey::parse(weapon).ok()?;
    if key.namespace != AssetNamespace::Iw4 {
        return None;
    }
    let leaf = weapon.rsplit('/').next().unwrap_or(weapon);
    let reference = if leaf.ends_with("_mp") {
        leaf.to_owned()
    } else {
        format!("{leaf}_mp")
    };
    let image = table.lookup(1, &reference, 3);
    if image.is_empty() {
        return None;
    }
    let mut preview = preview.clone();
    preview.image = image.to_owned();
    preview.desc_key = format!("@{}", table.lookup(1, &reference, 4));
    Some(prepare_cac_preview(key.namespace, preview))
}

fn weapon_image(weapon_name: &str) -> Option<&'static str> {
    let weapon_name = weapon_name.rsplit('/').next().unwrap_or(weapon_name);
    let stem = weapon_name
        .strip_suffix("_mp")
        .unwrap_or(weapon_name)
        .to_ascii_lowercase();
    Some(match stem.as_str() {
        "ak47" => "weapon_ak47",
        "m4" => "weapon_m4carbine",
        "famas" => "weapon_famas",
        "scar" => "weapon_scar_h",
        "tar21" => "weapon_tavor",
        "fal" => "weapon_fnfal",
        "m16" => "weapon_m16a4",
        "masada" => "weapon_masada",
        "fn2000" => "weapon_fn2000",
        "ump45" => "weapon_ump45_iron",
        "mp5k" => "weapon_mp5k",
        "uzi" => "weapon_mini_uzi",
        "p90" => "weapon_p90",
        "kriss" => "weapon_kriss",
        "rpd" => "weapon_rpd",
        "sa80" => "weapon_sa80",
        "mg4" => "weapon_mg4",
        "m240" => "weapon_m240",
        "aug" => "weapon_steyraug",
        "barrett" => "weapon_barrett50cal",
        "cheytac" => "weapon_cheytac_scope",
        "wa2000" => "weapon_wa2000",
        "m21" => "weapon_m14ebr",
        "ranger" => "weapon_ranger",
        "model1887" => "weapon_model1887",
        "striker" => "weapon_striker",
        "aa12" => "weapon_aa12",
        "m1014" => "weapon_benelli_m4",
        "spas12" => "weapon_spas12",
        "usp" => "weapon_usp_45",
        "beretta" => "weapon_m9beretta",
        "deserteagle" => "weapon_desert_eagle",
        "coltanaconda" => "weapon_colt_anaconda",
        "glock" => "weapon_glock",
        "beretta393" => "weapon_beretta393",
        "pp2000" => "weapon_pp2000",
        "tmp" => "weapon_tmp",
        "at4" => "weapon_at4",
        "rpg" => "weapon_rpg7",
        "stinger" => "weapon_stinger",
        "javelin" => "weapon_javelin",
        "riotshield" => "weapon_riot_shield",
        "semtex" => "cardicon_semtex",
        "frag_grenade" => "weapon_fraggrenade",
        "throwingknife" => "cardicon_throwing_knive",
        "claymore" => "weapon_claymore",
        "c4" => "weapon_c4",
        "flash_grenade" => "weapon_flashbang",
        "smoke_grenade" => "weapon_smokegrenade",
        "concussion_grenade" => "weapon_concgrenade",
        "flare" => "specialty_tactical_insert",
        _ => return None,
    })
}

fn material_iwd_stem(material: &str) -> &str {
    match material {
        "specialty_onemanarmy" => "specialty_one_man_army",
        "specialty_coldblooded" => "specialty_cold_blooded",
        "specialty_dangerclose" => "specialty_danger_close",
        "specialty_localjammer" => "specialty_scrambler",
        "equipment_frag" => "weapon_fraggrenade",
        "equipment_semtex" => "cardicon_semtex",
        "equipment_c4" => "weapon_c4",
        "equipment_claymore" => "weapon_claymore",
        "equipment_throwing_knife" => "cardicon_throwing_knive",
        "equipment_flare" => "specialty_tactical_insert",
        "weapon_semtex" => "cardicon_semtex",
        "weapon_cheytac" => "weapon_cheytac_scope",
        "killiconmelee" => "cardicon_throwing_knive",
        other => other,
    }
}

fn attachment_image(token: &str) -> Option<&'static str> {
    let token = token.rsplit('/').next().unwrap_or(token);
    let normalized = token
        .trim()
        .strip_suffix("_mp")
        .unwrap_or(token.trim())
        .to_ascii_lowercase();
    let token = normalized
        .split('_')
        .rev()
        .find(|part| attachment_material(part).is_some())
        .unwrap_or(normalized.as_str());
    attachment_material(token)
}

fn attachment_material(token: &str) -> Option<&'static str> {
    Some(match token {
        "acog" => "weapon_attachment_acog",
        "eotech" => "weapon_attachment_eotech",
        "reflex" => "weapon_attachment_reflex",
        "thermal" => "weapon_attachment_thermal",
        "silencer" | "suppressor" => "weapon_attachment_suppressor",
        "grip" => "weapon_attachment_grip",
        "fmj" => "weapon_attachment_fmj",
        "xmags" | "mags" => "weapon_attachment_mags",
        "gl" | "m203" | "gp25" => "weapon_attachment_m203",
        "shotgun" => "weapon_attachment_shotgun",
        "heartbeat" => "weapon_attachment_heartbeat",
        "akimbo" => "weapon_attachment_akimbo",
        "tactical" => "weapon_attachment_tactical",
        _ => return None,
    })
}

fn fallback_label(weapon: &str) -> String {
    let weapon = weapon.rsplit('/').next().unwrap_or(weapon);
    let weapon = weapon.strip_prefix("iw5_").unwrap_or(weapon);
    weapon
        .strip_suffix("_mp")
        .unwrap_or(weapon)
        .replace('_', " ")
        .to_ascii_uppercase()
}

fn fallback_name_key(reference: &str) -> String {
    format!(
        "PERKS_{}",
        reference.trim_start_matches("specialty_").to_uppercase()
    )
}
