use super::*;

pub(super) fn joined_name(base: &str, attachments: &[String]) -> String {
    if attachments.is_empty() {
        base.to_owned()
    } else {
        format!("{base}_{}", attachments.join("_"))
    }
}

pub(super) fn compile_authored(
    registry: &WeaponRegistry,
    family: &WeaponFamily,
    _selection: &WeaponSelection,
    name: String,
) -> Result<u32, ConfigurationRefusal> {
    let id = registry
        .by_namespaced
        .get(&(family.key.namespace, name.clone()))
        .copied()
        .ok_or_else(|| ConfigurationRefusal::MissingContent(format!("{name}_mp")))?;
    registry.configuration_admission(id)?;
    Ok(id)
}
