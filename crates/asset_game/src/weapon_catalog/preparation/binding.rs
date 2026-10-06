use super::*;

impl WeaponPreparationRecipe {
    pub(in crate::weapon_catalog) fn declare_sound_hints(&mut self, sounds: &WeaponSoundAliases) {
        if self.closed_references {
            return;
        }
        for name in sounds.reachable_aliases() {
            self.record(
                WeaponComponent::Sound,
                name,
                PreparedComponentTarget::SoundPublication,
            );
        }
    }

    pub(in crate::weapon_catalog) fn declare_notetrack_sound(
        &mut self,
        clip: Option<&str>,
        alias: &str,
    ) {
        if self.reference(WeaponComponent::Sound, alias).is_some() {
            return;
        }
        if !self.closed_references {
            self.record(
                WeaponComponent::Sound,
                alias,
                PreparedComponentTarget::SoundPublication,
            );
            return;
        }
        let Some(reference) = clip
            .and_then(|name| self.reference(WeaponComponent::Animation, name))
            .cloned()
        else {
            return;
        };
        if !matches!(reference.target, PreparedComponentTarget::Animation(_)) {
            return;
        }
        self.references.push(PreparedComponentReference {
            component: WeaponComponent::Sound,
            source_namespace: reference.source_namespace,
            source_name: alias.to_owned(),
            name: alias.to_owned(),
            storage_namespace: reference.storage_namespace,
            policy: reference.policy,
            target: PreparedComponentTarget::SoundPublication,
        });
    }

    pub(in crate::weapon_catalog) fn bind_combat_effects(
        &mut self,
        combat: &mut WeaponCombatFx,
        slots: CombatFxSlots,
        catalog: &crate::FxCatalog,
        tracers: &crate::TracerCatalog,
    ) {
        for (slot, hint, edge) in [
            (
                slots.view_flash,
                &combat.view_flash_hint,
                &mut combat.view_flash,
            ),
            (
                slots.world_flash,
                &combat.world_flash_hint,
                &mut combat.world_flash,
            ),
            (
                slots.view_shell_eject,
                &combat.view_shell_eject_hint,
                &mut combat.view_shell_eject,
            ),
            (
                slots.world_shell_eject,
                &combat.world_shell_eject_hint,
                &mut combat.world_shell_eject,
            ),
            (
                slots.view_last_shot_eject,
                &combat.view_last_shot_eject_hint,
                &mut combat.view_last_shot_eject,
            ),
            (
                slots.world_last_shot_eject,
                &combat.world_last_shot_eject_hint,
                &mut combat.world_last_shot_eject,
            ),
            (
                slots.explosion,
                &combat.explosion_hint,
                &mut combat.explosion,
            ),
        ] {
            let Some(name) = hint.as_deref() else {
                continue;
            };
            if slot.is_some() {
                self.declare_native_slot(WeaponComponent::Effect, name);
            }
            *edge = self.bind_fx(catalog, slot.is_some(), Some(name));
        }
        if let Some(name) = combat.tracer_hint.as_deref() {
            if slots.tracer.is_some() {
                self.declare_native_slot(WeaponComponent::Tracer, name);
            }
            let valid = self
                .candidate_namespace(WeaponComponent::Tracer, name)
                .is_some()
                && combat
                    .tracer
                    .bound_index()
                    .and_then(|index| tracers.def_at(index))
                    .is_some_and(|def| def.name == name);
            if valid {
                self.record(
                    WeaponComponent::Tracer,
                    name,
                    PreparedComponentTarget::Tracer(combat.tracer),
                );
            } else {
                self.record(
                    WeaponComponent::Tracer,
                    name,
                    PreparedComponentTarget::Unsupported(ComponentPreparationRefusal::CatalogMiss),
                );
                combat.tracer = AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss);
            }
        }
    }

    fn declare_native_slot(&mut self, component: WeaponComponent, name: &str) {
        if !self.closed_references || self.reference(component, name).is_some() {
            return;
        }
        let Some(namespace) = self.namespace(component) else {
            return;
        };
        self.references.push(PreparedComponentReference {
            component,
            source_namespace: namespace,
            source_name: name.to_owned(),
            storage_namespace: namespace,
            name: name.to_owned(),
            policy: ComponentPreparationPolicy::Native,
            target: PreparedComponentTarget::Pending,
        });
    }
    fn candidate_namespace(
        &self,
        component: WeaponComponent,
        name: &str,
    ) -> Option<AssetNamespace> {
        self.namespace(component)?;
        if !self.closed_references {
            return self.namespace(component);
        }
        let reference = self
            .references
            .iter()
            .find(|r| r.component == component && r.name == name)?;
        if matches!(reference.policy, ComponentPreparationPolicy::Unsupported(_)) {
            return None;
        }
        Some(reference.storage_namespace)
    }

    pub fn reference(
        &self,
        component: WeaponComponent,
        name: &str,
    ) -> Option<&PreparedComponentReference> {
        self.references
            .iter()
            .find(|r| r.component == component && r.name == name)
    }

    pub(in crate::weapon_catalog) fn namespace_for(
        &self,
        component: WeaponComponent,
        name: &str,
    ) -> Option<AssetNamespace> {
        let namespace = self.candidate_namespace(component, name)?;
        match self.reference(component, name)?.target {
            PreparedComponentTarget::Pending | PreparedComponentTarget::Unsupported(_) => None,
            _ => Some(namespace),
        }
    }

    fn record(&mut self, component: WeaponComponent, name: &str, target: PreparedComponentTarget) {
        if let Some(reference) = self
            .references
            .iter_mut()
            .find(|r| r.component == component && r.name == name)
        {
            reference.target = target;
            return;
        }
        let native = !self.closed_references;
        self.references.push(PreparedComponentReference {
            component,
            source_namespace: self.namespace(component).unwrap_or(self.source.namespace),
            source_name: name.to_owned(),
            name: name.to_owned(),
            storage_namespace: self.namespace(component).unwrap_or(self.source.namespace),
            policy: if native {
                ComponentPreparationPolicy::Native
            } else {
                ComponentPreparationPolicy::Unsupported(
                    ComponentPreparationRefusal::UndeclaredReference,
                )
            },
            target: if native {
                target
            } else {
                PreparedComponentTarget::Unsupported(
                    ComponentPreparationRefusal::UndeclaredReference,
                )
            },
        });
    }

    pub(in crate::weapon_catalog) fn finish(&mut self) {
        self.closed_references = true;
        for reference in &mut self.references {
            if reference.target == PreparedComponentTarget::Pending {
                reference.target = PreparedComponentTarget::Unsupported(
                    ComponentPreparationRefusal::UnrequestedCapability,
                );
            }
        }
    }

    pub(in crate::weapon_catalog) fn absorb_references(&mut self, other: &Self) {
        for reference in &other.references {
            if matches!(
                reference.component,
                WeaponComponent::ViewModel | WeaponComponent::Animation
            ) && self
                .reference(reference.component, &reference.name)
                .is_none()
            {
                self.references.push(reference.clone());
            }
        }
    }

    pub(in crate::weapon_catalog) fn bind_fpv(
        &mut self,
        component: WeaponComponent,
        hint: Option<&str>,
        catalog: &crate::FpvMeshCatalog,
    ) -> AssetEdge<FpvMeshSpace> {
        let hint = hint.filter(|name| !name.is_empty());
        let edge = fpv_model_edge(
            hint,
            hint.and_then(|name| self.candidate_namespace(component, name)),
            catalog,
        );
        if let Some(name) = hint {
            self.record(
                component,
                name,
                if edge.is_bound() {
                    PreparedComponentTarget::Fpv(edge)
                } else {
                    PreparedComponentTarget::Unsupported(ComponentPreparationRefusal::CatalogMiss)
                },
            );
        }
        edge
    }

    pub(in crate::weapon_catalog) fn bind_world(
        &mut self,
        hint: Option<&str>,
        catalog: &crate::WorldWeaponCatalog,
    ) -> AssetEdge<WorldWeaponSpace> {
        let hint = hint.filter(|name| !name.is_empty());
        let edge = world_model_edge(
            hint,
            hint.and_then(|name| self.candidate_namespace(WeaponComponent::WorldModel, name)),
            catalog,
        );
        if let Some(name) = hint {
            self.record(
                WeaponComponent::WorldModel,
                name,
                if edge.is_bound() {
                    PreparedComponentTarget::World(edge)
                } else {
                    PreparedComponentTarget::Unsupported(ComponentPreparationRefusal::CatalogMiss)
                },
            );
        }
        edge
    }

    pub(in crate::weapon_catalog) fn bind_material(
        &mut self,
        hint: Option<&str>,
        authored: bool,
        catalog: &crate::MaterialDefinitions,
    ) -> AssetEdge<MaterialSpace> {
        let hint = hint.filter(|name| !name.is_empty());
        let edge =
            match hint.and_then(|name| self.candidate_namespace(WeaponComponent::Material, name)) {
                Some(namespace) => material_hint_edge(hint, namespace, authored, catalog),
                None if hint.is_none() && !authored => AssetEdge::Absent,
                None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
            };
        if let Some(name) = hint {
            self.record(
                WeaponComponent::Material,
                name,
                if edge.is_bound() {
                    PreparedComponentTarget::Material(edge)
                } else {
                    PreparedComponentTarget::Unsupported(ComponentPreparationRefusal::CatalogMiss)
                },
            );
        }
        edge
    }

    pub(in crate::weapon_catalog) fn bind_anim(
        &mut self,
        catalog: &crate::XAnimCatalog,
        hint: Option<&str>,
    ) -> AssetEdge<XAnimSpace> {
        let hint = hint.filter(|name| !name.is_empty());
        let edge = match hint
            .and_then(|name| self.candidate_namespace(WeaponComponent::Animation, name))
        {
            Some(namespace) => catalog.hint_edge(hint, namespace),
            None if hint.is_none() => AssetEdge::Absent,
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        };
        if let Some(name) = hint {
            self.record(
                WeaponComponent::Animation,
                name,
                if edge.is_bound() {
                    PreparedComponentTarget::Animation(edge)
                } else {
                    PreparedComponentTarget::Unsupported(ComponentPreparationRefusal::CatalogMiss)
                },
            );
        }
        edge
    }

    pub(in crate::weapon_catalog) fn bind_fx(
        &mut self,
        catalog: &crate::FxCatalog,
        authored: bool,
        hint: Option<&str>,
    ) -> AssetEdge<FxSpace> {
        let hint = hint.filter(|name| !name.is_empty());
        let edge =
            match hint.and_then(|name| self.candidate_namespace(WeaponComponent::Effect, name)) {
                Some(namespace) => catalog.hint_edge(authored, hint, namespace),
                None if hint.is_none() && !authored => AssetEdge::Absent,
                None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
            };
        if let Some(name) = hint {
            self.record(
                WeaponComponent::Effect,
                name,
                if edge.is_bound() {
                    PreparedComponentTarget::Effect(edge)
                } else {
                    PreparedComponentTarget::Unsupported(ComponentPreparationRefusal::CatalogMiss)
                },
            );
        }
        edge
    }

    pub(in crate::weapon_catalog) fn bind_projectile(
        &mut self,
        hint: Option<&str>,
        catalog: &crate::ProjectileMeshCatalog,
        zone: ZoneOwner,
    ) -> AssetEdge<ProjectileModelSpace> {
        let hint = hint.filter(|name| !name.is_empty());
        let edge = match hint {
            None => AssetEdge::Absent,
            Some(name) => match self
                .candidate_namespace(WeaponComponent::Projectile, name)
                .and_then(|ns| catalog.index_by_name(ns, name))
            {
                Some(order) => AssetEdge::bind_order(order, zone),
                None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
            },
        };
        if let Some(name) = hint {
            self.record(
                WeaponComponent::Projectile,
                name,
                if edge.is_bound() {
                    PreparedComponentTarget::Projectile(edge)
                } else {
                    PreparedComponentTarget::Unsupported(ComponentPreparationRefusal::CatalogMiss)
                },
            );
        }
        edge
    }
}

impl WeaponRow {
    pub(in crate::weapon_catalog) fn component_namespace(
        &self,
        component: WeaponComponent,
    ) -> Option<AssetNamespace> {
        self.preparation.namespace(component)
    }
}

impl WeaponCombatFx {
    pub(in crate::weapon_catalog) fn refuse_preparation(&mut self) {
        for edge in [
            &mut self.view_flash,
            &mut self.world_flash,
            &mut self.view_shell_eject,
            &mut self.world_shell_eject,
            &mut self.view_last_shot_eject,
            &mut self.world_last_shot_eject,
            &mut self.explosion,
        ] {
            if !edge.is_absent() {
                *edge = AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss);
            }
        }
        if !self.tracer.is_absent() {
            self.tracer = AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss);
        }
    }
}
