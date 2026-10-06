use super::{WeaponRegistry, normalize_weapon_name};
use crate::{AssetNamespace, ConfigurationRefusal, FamilyKey, WeaponFamily, WeaponSelection};

pub(crate) trait WeaponConfigurationCompiler {
    fn compile(
        &self,
        family: &WeaponFamily,
        selection: &WeaponSelection,
    ) -> Result<u32, ConfigurationRefusal>;
}

mod common;
mod iw4;
mod iw5;
mod t5;
mod t6;

pub(super) fn authored_name(key: &FamilyKey, attachments: &[String]) -> String {
    match key.namespace {
        AssetNamespace::Iw4 => iw4::authored_name(&key.base, attachments),
        AssetNamespace::Iw5 => common::joined_name(&key.base, attachments),
        AssetNamespace::T5 => t5::authored_name(&key.base, attachments),
        AssetNamespace::T6 => t6::authored_name(&key.base, attachments),
    }
}

pub(crate) fn authored_attachments(key: &FamilyKey, name: &str) -> Option<Vec<String>> {
    let name = name.strip_suffix("_mp").unwrap_or(name);
    if name == key.base {
        return Some(Vec::new());
    }
    if key.namespace == AssetNamespace::T5 && name == t5::authored_name(&key.base, &["dw".into()]) {
        return Some(vec!["dw".into()]);
    }
    let rest = name.strip_prefix(&key.base)?.strip_prefix('_')?;
    Some(rest.split('_').map(str::to_owned).collect())
}

impl WeaponConfigurationCompiler for WeaponRegistry {
    fn compile(
        &self,
        family: &WeaponFamily,
        selection: &WeaponSelection,
    ) -> Result<u32, ConfigurationRefusal> {
        let compiler: &dyn WeaponConfigurationCompiler = match family.key.namespace {
            AssetNamespace::Iw4 => &iw4::Iw4Configuration(self),
            AssetNamespace::Iw5 => &iw5::Iw5Configuration(self),
            AssetNamespace::T5 => &t5::T5Configuration(self),
            AssetNamespace::T6 => &t6::T6Configuration(self),
        };
        compiler.compile(family, selection)
    }
}

impl crate::weapon_families::FamilyContent for WeaponRegistry {
    fn published_handle(&self, id: u32) -> Option<crate::WeaponHandle> {
        self.bind_published_row(id)
            .ok()
            .map(|weapon| weapon.handle())
    }
    fn lookup(&self, namespace: crate::AssetNamespace, name: &str) -> Option<u32> {
        self.by_namespaced
            .get(&(namespace, normalize_weapon_name(name)))
            .copied()
    }

    fn offhand_class(&self, id: u32) -> i32 {
        self.facts_of(id).map_or(0, |facts| facts.offhand_class)
    }

    fn admission(&self, id: u32) -> Result<(), crate::ConfigurationRefusal> {
        self.configuration_admission(id)
    }

    fn prepared_all(&self) -> Vec<(u32, crate::WeaponSelection)> {
        self.configurations
            .iter()
            .map(|(selection, &id)| (id, selection.clone()))
            .collect()
    }

    fn names_in(&self, namespace: crate::AssetNamespace) -> Vec<(u32, String)> {
        self.published_weapons()
            .map(|weapon| weapon.wire_id())
            .filter(|&id| id != 0)
            .filter(|&id| self.identity_namespace_of(id) == Some(namespace))
            .filter(|&id| self.iw5_configuration_of(id).is_none())
            .map(|id| (id, normalize_weapon_name(self.name_of(id))))
            .collect()
    }
}

pub(super) fn compile_completion_names(registry: &WeaponRegistry) -> Vec<String> {
    use crate::FamilySlot;
    let mut names: Vec<String> = registry
        .weapon_families()
        .offered()
        .filter(|family| matches!(family.slot, FamilySlot::Primary | FamilySlot::Secondary))
        .filter(|family| {
            family
                .base
                .is_some_and(|id| registry.gun_xmodel_of(id).is_some())
        })
        .map(|family| family.key.short())
        .collect();
    names.extend(
        registry
            .published_weapons()
            .filter(|weapon| weapon.wire_id() != 0)
            .filter_map(|weapon| {
                let id = weapon.wire_id();
                if registry.describe_configuration(id).is_some()
                    || registry.gun_xmodel_of(id).is_none()
                    || registry.configuration_admission(id).is_err()
                {
                    return None;
                }
                let facts = weapon.hud_facts()?;
                if !facts.is_primary() || facts.offhand_class != 0 {
                    return None;
                }
                let key =
                    crate::FamilyKey::new(registry.host_namespace_of(id)?, registry.name_of(id));
                if registry.weapon_families().families().iter().any(|family| {
                    family.key.namespace == key.namespace
                        && (key.base == family.key.base
                            || key
                                .base
                                .strip_prefix(&family.key.base)
                                .is_some_and(|suffix| suffix.starts_with('_') || suffix == "dw"))
                }) {
                    return None;
                }
                Some(key.short())
            }),
    );
    names.sort();
    names.dedup();
    names
}
