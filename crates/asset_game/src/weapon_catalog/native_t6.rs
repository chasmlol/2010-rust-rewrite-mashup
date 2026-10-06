use super::*;

impl WeaponBuild {
    pub fn prepare_native_t6(
        &mut self,
        fpv: &crate::FpvMeshCatalog,
        hands: Option<&str>,
    ) -> Vec<WeaponPreparationRefusal> {
        let mut refusals = Vec::new();
        for row in &mut self.registry.rows {
            if row.namespace != crate::AssetNamespace::T6 {
                continue;
            }
            row.preparation = WeaponPreparationRecipe::for_capture(row.namespace, &row.name);
            row.hand_xmodel = hands
                .filter(|name| fpv.get(crate::AssetNamespace::T6, name).is_some())
                .map(str::to_owned);
            row.fpv_hands = [None, None];
            row.fpv_mount_plan = None;
            row.fpv_assemblies = [None, None];
            if row
                .gun_xmodel
                .as_deref()
                .is_some_and(|name| fpv.get(row.namespace, name).is_none())
            {
                let refusal = WeaponPreparationRefusal::MissingNativeComponent {
                    weapon: row.name.clone(),
                    component: WeaponComponent::ViewModel,
                };
                row.preparation.refusal = Some(refusal.clone());
                refusals.push(refusal);
            }
            row.sounds.notetrack_convention = NotetrackConvention::InlinePrefix;
        }
        self.registry.revision = mint_weapon_revision();
        refusals
    }
}
