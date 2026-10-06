use asset_game::WeaponRegistry;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoritativeClassProjection {
    pub def: sim::ClassDef,
    pub lock_reason: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassRow {
    pub weapons: [String; 4],
    pub attachments: [Vec<String>; 2],
    pub perks: [String; 3],
    pub deathstreak: String,
    pub camos: [String; 2],
}

pub fn resolve_class_weapon(
    weapons: &WeaponRegistry,
    name: &str,
    attachments: &[String],
    rules: asset_game::LoadoutRules,
) -> Result<u32, String> {
    if name.is_empty() {
        return if attachments.is_empty() {
            Ok(0)
        } else {
            Err("weapon.unknown_family".to_owned())
        };
    }
    let resolve = |selection: asset_game::WeaponSelection| {
        weapons
            .resolve_configuration(&selection, rules)
            .map(|resolved| {
                weapons
                    .bind(resolved.handle())
                    .expect("resolved in this registry")
                    .wire_id()
            })
            .map_err(|refusal| format!("{name}:{}", refusal.code()))
    };
    if let Some(family) = asset_game::FamilyKey::parse(name)
        && weapons.weapon_families().family(&family).is_some()
    {
        return resolve(asset_game::WeaponSelection::with(family, attachments));
    }
    let id = match weapons.resolve_index(name) {
        Ok(Some(id)) => id,
        Ok(None) | Err(_) => return Err(format!("{name}:catalog.unknown")),
    };
    match weapons.describe_configuration(id) {
        Some(described) if described.family.is_some() => {
            let mut selection = described.clone();
            for extra in attachments {
                if !selection.attachments.contains(extra) {
                    selection.attachments.push(extra.clone());
                }
            }
            resolve(selection)
        }
        _ if attachments.is_empty() => weapons
            .configuration_admission(id)
            .map(|()| id)
            .map_err(|refusal| format!("{name}:{}", refusal.code())),
        _ => Err(format!("{name}:weapon.unknown_family")),
    }
}

pub fn authoritative_class_lock_reason(
    names: [&str; 4],
    ids: [u32; 4],
    combat: &[weapon_iw4::WeaponCombatFacts],
    equipment: &[sim::EquipmentRuntimeFacts],
) -> Option<String> {
    let mut reason = None;
    for (name, id) in names[..2].iter().zip(&ids[..2]) {
        if *id == 0 {
            continue;
        }
        if !combat
            .get(*id as usize)
            .copied()
            .is_some_and(weapon_iw4::WeaponCombatFacts::is_usable)
        {
            append_lock_reason(&mut reason, format!("{name}:validated.missing_profile"));
        }
    }
    for (slot, (name, id)) in names[2..].iter().zip(&ids[2..]).enumerate() {
        if *id == 0 {
            continue;
        }
        if !equipment.get(*id as usize).copied().is_some_and(|facts| {
            facts.is_offhand() && matches!((slot, facts.offhand_class), (0, 1 | 4 | 5) | (1, 2 | 3))
        }) {
            append_lock_reason(&mut reason, format!("{name}:offhand.missing_runtime_facts"));
        }
    }
    reason
}

fn append_lock_reason(reason: &mut Option<String>, item: String) {
    match reason {
        Some(reason) => {
            reason.push_str("; ");
            reason.push_str(&item);
        }
        None => *reason = Some(item),
    }
}

pub fn perk_catalog_id(reference: &str) -> Option<u32> {
    sim::match_state::CLASS_CATALOG_PERKS
        .iter()
        .position(|perk| perk.eq_ignore_ascii_case(reference))
        .map(|index| index as u32 + 1)
}

pub fn project_class(
    class_id: u32,
    row: &ClassRow,
    weapons: &WeaponRegistry,
    combat: &[weapon_iw4::WeaponCombatFacts],
    equipment: &[sim::EquipmentRuntimeFacts],
) -> AuthoritativeClassProjection {
    let resolved = resolve_personal_class(row, weapons).and_then(|loadout| {
        loadout
            .definition(sim::ClassId(class_id), 1)
            .ok_or_else(|| "class.invalid_definition".to_owned())
    });
    let (mut def, mut lock_reason) = match resolved {
        Ok(def) => (def, None),
        Err(reason) => (
            sim::ClassDef::primary_secondary(sim::ClassId(class_id), 1, 0, 0),
            Some(reason),
        ),
    };
    let names = row.weapons.each_ref().map(String::as_str);
    if let Some(reason) =
        authoritative_class_lock_reason(names, def.weapon_slot_ids(), combat, equipment)
    {
        append_lock_reason(&mut lock_reason, reason);
    }
    def.locked = lock_reason.is_some();
    AuthoritativeClassProjection { def, lock_reason }
}

impl From<&frame::HostClassSlot> for ClassRow {
    fn from(slot: &frame::HostClassSlot) -> Self {
        Self {
            weapons: [
                slot.primary.clone(),
                slot.secondary.clone(),
                slot.lethal.clone(),
                slot.tactical.clone(),
            ],
            attachments: [
                slot.primary_attachments.clone(),
                slot.secondary_attachments.clone(),
            ],
            perks: slot.perks.clone(),
            deathstreak: slot.deathstreak.clone(),
            camos: slot.camos.clone(),
        }
    }
}

pub fn resolve_personal_class(
    row: &ClassRow,
    registry: &WeaponRegistry,
) -> Result<sim::PersonalClass, String> {
    let mut loadout = sim::PersonalClass::default();
    let rules = asset_game::LoadoutRules::for_class(&row.perks[0]);
    for (slot, weapon) in loadout.weapons.iter_mut().enumerate() {
        *weapon = resolve_class_weapon(
            registry,
            &row.weapons[slot],
            row.attachments.get(slot).map_or(&[], Vec::as_slice),
            rules,
        )?;
    }
    for (slot, camo) in row.camos.iter().enumerate() {
        loadout.camos[slot] = registry
            .camouflage_slot(loadout.weapons[slot], camo)
            .ok_or_else(|| format!("{camo}:camouflage.unavailable"))?;
    }
    for (slot, name) in row.perks.iter().enumerate() {
        if name.is_empty() || name == "specialty_null" {
            continue;
        }
        let id =
            perk_catalog_id(name).ok_or_else(|| format!("{name}:perk.unknown_catalog_entry"))?;
        if sim::match_state::perk_slot_from_class_catalog(id) != Some(slot) {
            return Err(format!("{name}:perk.invalid_slot"));
        }
        loadout.perks[slot] = id;
    }
    if !row.deathstreak.is_empty() && row.deathstreak != "specialty_null" {
        loadout.deathstreak = sim::match_state::CLASS_CATALOG_DEATHSTREAKS
            .iter()
            .position(|name| *name == row.deathstreak)
            .ok_or_else(|| format!("{}:deathstreak.unknown_catalog_entry", row.deathstreak))?
            as u8
            + 1;
    }
    Ok(loadout)
}
