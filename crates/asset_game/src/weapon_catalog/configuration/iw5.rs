use super::*;

pub(super) struct Iw5Configuration<'a>(pub(super) &'a WeaponRegistry);

impl WeaponConfigurationCompiler for Iw5Configuration<'_> {
    fn compile(
        &self,
        family: &WeaponFamily,
        selection: &WeaponSelection,
    ) -> Result<u32, ConfigurationRefusal> {
        if selection.attachments.is_empty() {
            return common::compile_authored(
                self.0,
                family,
                selection,
                common::joined_name(&family.key.base, &selection.attachments),
            );
        }
        let base = family.base.ok_or_else(|| {
            ConfigurationRefusal::MissingContent(format!("{}_mp", family.key.base))
        })?;
        let slots = self
            .0
            .resolve_iw5_attachment_slots(base, &selection.attachments)?;
        self.0
            .iw5_primary_attachment_assets(base, slots)
            .ok_or_else(|| ConfigurationRefusal::MissingContent(self.0.name_of(base).into()))?;
        let id = self
            .0
            .configurations
            .get(selection)
            .copied()
            .ok_or_else(|| {
                ConfigurationRefusal::MissingContent(format!(
                    "{} {}",
                    family.key,
                    selection.attachments.join(" ")
                ))
            })?;
        self.0.configuration_admission(id)?;
        Ok(id)
    }
}
