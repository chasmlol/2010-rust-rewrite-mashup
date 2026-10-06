use super::*;

pub(super) struct Iw4Configuration<'a>(pub(super) &'a WeaponRegistry);

pub(super) fn authored_name(base: &str, attachments: &[String]) -> String {
    common::joined_name(base, attachments)
}

impl WeaponConfigurationCompiler for Iw4Configuration<'_> {
    fn compile(
        &self,
        family: &WeaponFamily,
        selection: &WeaponSelection,
    ) -> Result<u32, ConfigurationRefusal> {
        common::compile_authored(
            self.0,
            family,
            selection,
            authored_name(&family.key.base, &selection.attachments),
        )
    }
}
