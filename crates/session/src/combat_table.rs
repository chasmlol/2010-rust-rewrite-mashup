use asset_game::WeaponRegistry;
use weapon_iw4::{HITLOC_COUNT, WeaponCombatFacts, WeaponHostRules};

pub fn from_registry(
    weapons: &WeaponRegistry,
    global_location: Option<[f32; HITLOC_COUNT]>,
) -> Vec<WeaponCombatFacts> {
    let rules = WeaponHostRules::default();
    let mut refused = Vec::new();
    let rows = weapons
        .published_weapons()
        .map(|weapon| {
            let id = weapon.wire_id();
            if id == 0 {
                return WeaponCombatFacts::none();
            }
            match weapon.combat_facts(rules, global_location) {
                Ok(facts) => facts,
                Err(reason) => {
                    refused.push(format!("{}({reason:?})", weapons.name_of(id)));
                    WeaponCombatFacts::none()
                }
            }
        })
        .collect();
    if !refused.is_empty() {
        diag::info!(
            Sim,
            "combat facts refused for {} weapons: {}",
            refused.len(),
            refused.join(" ")
        );
    }
    rows
}

pub fn melee_only_from_registry(weapons: &WeaponRegistry) -> Vec<bool> {
    (0..=weapons.len())
        .map(|index| weapons.melee_only_of(index as u32))
        .collect()
}

pub fn script_sounds_from_registry(weapons: &WeaponRegistry) -> Vec<sim::WeaponScriptSounds> {
    (0..=weapons.len())
        .map(|i| {
            weapons
                .sounds_of(i as u32)
                .map(|s| sim::WeaponScriptSounds {
                    fire: s.fire.clone(),
                    fire_player: s.fire_player.clone(),
                    pickup: s.pickup.clone(),
                    pickup_player: s.pickup_player.clone(),
                    proj_explosion: s.proj_explosion.clone(),
                })
                .unwrap_or_default()
        })
        .collect()
}

pub fn pen_from_registry(weapons: &WeaponRegistry) -> Vec<weapon_iw4::BulletPenFacts> {
    (0..=weapons.len())
        .map(|index| {
            weapons
                .penetration_facts_of(index as u32)
                .unwrap_or_default()
        })
        .collect()
}

pub fn equipment_from_registry(weapons: &WeaponRegistry) -> Vec<sim::EquipmentRuntimeFacts> {
    (0..=weapons.len())
        .map(|index| weapons.equipment_facts_of(index as u32).unwrap_or_default())
        .collect()
}
