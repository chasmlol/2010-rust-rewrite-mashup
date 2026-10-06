use super::*;

impl WeaponCatalog {
    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn set_capture_ns(&mut self, ns: crate::AssetNamespace) {
        self.capture_ns = Some(ns);
    }

    pub fn capture_vehicle(&mut self, stream: &ZoneStream<'_>) {
        let Some((Some(name), turret)) = stream.vehicle() else {
            return;
        };
        let Some(name) = read_name(stream, name) else {
            return;
        };
        if let Some(turret) = turret.and_then(|ptr| read_name(stream, ptr)) {
            self.vehicle_turrets.insert(name.clone(), turret);
        }
        self.vehicle_accel
            .insert(name.clone(), stream.vehicle_accel());
        let (icons, size) = stream.vehicle_compass();
        self.vehicle_compass
            .insert(name, (icons.map(str::to_owned), size));
    }

    pub fn resolve_reticles(&mut self, materials: &crate::MaterialCatalog) {
        let ns = self
            .capture_ns
            .expect("asset capture requires an explicit family");
        let names_of = |slot: Ptr| {
            let material = materials
                .material_index(slot)
                .and_then(|i| materials.materials.get(i.get()))?;
            let name = Some(material.name.to_string()).filter(|n| !n.is_empty())?;
            let image = materials.hud_image_name(material).map(str::to_owned);
            Some((name, image))
        };
        for entry in &mut self.entries {
            if let Some(slot) = entry.reticle_center_slot {
                if let Some((name, image)) = names_of(slot) {
                    entry.reticle.center_material = Some(name);
                    entry.reticle.center_image = image;
                }
            }
            if entry.reticle.center_image.is_none() {
                if let Some(name) = entry.reticle.center_material.as_deref() {
                    if let Some(index) = materials.material_index_by_ns(ns, name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.reticle.center_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(slot) = entry.reticle_side_slot {
                if let Some((name, image)) = names_of(slot) {
                    entry.reticle.side_material = Some(name);
                    entry.reticle.side_image = image;
                }
            }
            if entry.reticle.side_image.is_none() {
                if let Some(name) = entry.reticle.side_material.as_deref() {
                    if let Some(index) = materials.material_index_by_ns(ns, name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.reticle.side_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(slot) = entry.overlay_material_slot {
                if let Some((name, image)) = names_of(slot) {
                    entry.overlay_material = Some(name);
                    entry.overlay_image = image;
                }
            }
            if entry.overlay_image.is_none() {
                if let Some(name) = entry.overlay_material.as_deref() {
                    if let Some(index) = materials.material_index_by_ns(ns, name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.overlay_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(slot) = entry.pickup_icon_slot {
                if let Some((name, image)) = names_of(slot) {
                    entry.pickup_icon = Some(name);
                    entry.pickup_icon_image = image;
                }
            }
            if let Some(slot) = entry.hud_icon_slot {
                if let Some((name, image)) = names_of(slot) {
                    entry.hud_icon = Some(name);
                    if entry.hud_icon_image.is_none() {
                        entry.hud_icon_image = image;
                    }
                }
            }
            if entry.hud_icon_image.is_none() {
                if let Some(name) = entry.hud_icon.as_deref() {
                    if let Some(index) = materials.material_index_by_ns(ns, name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.hud_icon_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(slot) = entry.kill_icon_slot {
                if let Some((name, image)) = names_of(slot) {
                    if entry.kill_icon.is_none() {
                        entry.kill_icon = Some(name);
                    }
                    if entry.kill_icon_image.is_none() {
                        entry.kill_icon_image = image;
                    }
                }
            }
            if entry.kill_icon_image.is_none() {
                if let Some(name) = entry.kill_icon.as_deref() {
                    if let Some(index) = materials.material_index_by_ns(ns, name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.kill_icon_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(name) = entry.dpad_icon.as_deref()
                && let Some(index) = materials.material_index_by_ns(ns, name)
                && let Some(material) = materials.materials.get(index.order())
            {
                entry.dpad_icon_image = materials.hud_image_name(material).map(str::to_owned);
                entry.dpad_icon_atlas = material.texture_atlas;
            }
            entry.reticle.center_edge = material_hint_edge(
                entry.reticle.center_material.as_deref(),
                ns,
                entry.reticle.center_authored,
                materials,
            );
            entry.reticle.side_edge = material_hint_edge(
                entry.reticle.side_material.as_deref(),
                ns,
                entry.reticle.side_authored,
                materials,
            );
            entry.hud_material_edges.overlay = material_hint_edge(
                entry.overlay_material.as_deref(),
                ns,
                entry.overlay_material_slot.is_some(),
                materials,
            );
            entry.hud_material_edges.hud_icon = material_hint_edge(
                entry.hud_icon.as_deref(),
                ns,
                entry.hud_icon_slot.is_some(),
                materials,
            );
            entry.hud_material_edges.pickup_icon = material_hint_edge(
                entry.pickup_icon.as_deref(),
                ns,
                entry.pickup_icon_slot.is_some(),
                materials,
            );
            entry.hud_material_edges.kill_icon = material_hint_edge(
                entry.kill_icon.as_deref(),
                ns,
                entry.kill_icon_slot.is_some(),
                materials,
            );
        }
    }

    pub fn resolve_reticle_images(&mut self, materials: &crate::MaterialDefinitions) {
        let ns = self
            .capture_ns
            .expect("asset capture requires an explicit family");
        for entry in &mut self.entries {
            if entry.reticle.center_image.is_none() {
                if let Some(name) = entry.reticle.center_material.as_deref() {
                    if let Some(index) = materials.material_index_by_ns(ns, name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.reticle.center_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if entry.reticle.side_image.is_none() {
                if let Some(name) = entry.reticle.side_material.as_deref() {
                    if let Some(index) = materials.material_index_by_ns(ns, name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.reticle.side_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if entry.overlay_image.is_none() {
                if let Some(name) = entry.overlay_material.as_deref() {
                    if let Some(index) = materials.material_index_by_ns(ns, name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.overlay_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if entry.hud_icon_image.is_none() {
                if let Some(name) = entry.hud_icon.as_deref() {
                    if let Some(index) = materials.material_index_by_ns(ns, name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.hud_icon_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if entry.kill_icon_image.is_none() {
                if let Some(name) = entry.kill_icon.as_deref() {
                    if let Some(index) = materials.material_index_by_ns(ns, name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.kill_icon_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(name) = entry.dpad_icon.as_deref()
                && let Some(index) = materials.material_index_by_ns(ns, name)
                && let Some(material) = materials.materials.get(index.order())
            {
                entry.dpad_icon_image = materials.hud_image_name(material).map(str::to_owned);
                entry.dpad_icon_atlas = material.texture_atlas;
            }
            entry.reticle.center_edge = material_hint_edge(
                entry.reticle.center_material.as_deref(),
                ns,
                entry.reticle.center_authored,
                materials,
            );
            entry.reticle.side_edge = material_hint_edge(
                entry.reticle.side_material.as_deref(),
                ns,
                entry.reticle.side_authored,
                materials,
            );
            entry.hud_material_edges.overlay = material_hint_edge(
                entry.overlay_material.as_deref(),
                ns,
                entry.overlay_material_slot.is_some(),
                materials,
            );
            entry.hud_material_edges.hud_icon = material_hint_edge(
                entry.hud_icon.as_deref(),
                ns,
                entry.hud_icon_slot.is_some(),
                materials,
            );
            entry.hud_material_edges.pickup_icon = material_hint_edge(
                entry.pickup_icon.as_deref(),
                ns,
                entry.pickup_icon_slot.is_some(),
                materials,
            );
            entry.hud_material_edges.kill_icon = material_hint_edge(
                entry.kill_icon.as_deref(),
                ns,
                entry.kill_icon_slot.is_some(),
                materials,
            );
        }
    }

    pub fn hud_material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in &self.entries {
            census.push(entry.reticle.center_edge);
            census.push(entry.reticle.side_edge);
            census.push(entry.hud_material_edges.overlay);
            census.push(entry.hud_material_edges.hud_icon);
            census.push(entry.hud_material_edges.pickup_icon);
            census.push(entry.hud_material_edges.kill_icon);
        }
        census
    }

    pub fn resolve_projectile_fx_edges(&mut self, fx: &crate::FxCatalog) {
        let ns = self
            .capture_ns
            .expect("asset capture requires an explicit family");
        for entry in &mut self.entries {
            stamp_fx_edge(
                entry.proj_trail_slot,
                ns,
                fx,
                &mut entry.projectile_fx.trail,
                &mut entry.proj_trail,
            );
            stamp_fx_edge(
                entry.proj_beacon_slot,
                ns,
                fx,
                &mut entry.projectile_fx.beacon,
                &mut entry.proj_beacon,
            );
            stamp_fx_edge(
                entry.proj_ignition_slot,
                ns,
                fx,
                &mut entry.projectile_fx.ignition,
                &mut entry.proj_ignition,
            );
        }
    }

    pub fn projectile_fx_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in &self.entries {
            for edge in entry.projectile_fx.edges() {
                census.push(edge);
            }
        }
        census
    }

    pub fn resolve_projectile_impact_fx(&mut self, table: &crate::OwnedFxImpactTable) {
        for entry in &mut self.entries {
            if entry.facts.weap_type == weapon_iw4::WEAPTYPE_BULLET
                || entry.combat_slots.explosion.is_some()
            {
                continue;
            }
            entry.combat_fx.explosion_hint = table
                .impact_row(entry.facts.impact_type, false)
                .and_then(|row| table.effect_name(row, 0, None))
                .map(|fx| fx.name.to_owned());
        }
    }

    pub fn resolve_combat_fx(&mut self, fx: &crate::FxCatalog, tracers: &crate::TracerCatalog) {
        let ns = self
            .capture_ns
            .expect("asset capture requires an explicit family");
        for entry in &mut self.entries {
            stamp_combat_fx(&mut entry.combat_fx, entry.combat_slots, ns, fx, tracers);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn tracer_type_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in &self.entries {
            census.push(entry.combat_fx.tracer);
        }
        census
    }

    pub fn combat_fx_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in &self.entries {
            for edge in entry.combat_fx.edges() {
                census.push(edge);
            }
        }
        census
    }

    pub fn projectile_model_hints(&self) -> HashSet<asset_model::ProjectileMeshKey> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let name = entry.projectile_model.as_deref()?;
                (!name.is_empty())
                    .then(|| asset_model::ProjectileMeshKey::new(entry.namespace, name))
            })
            .collect()
    }
}

pub(super) fn material_hint_edge(
    hint: Option<&str>,
    namespace: crate::AssetNamespace,
    authored_slot: bool,
    materials: &crate::MaterialDefinitions,
) -> AssetEdge<MaterialSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    if !authored_slot && hint.is_none() {
        return AssetEdge::Absent;
    }
    match hint.and_then(|name| materials.material_index_by_ns(namespace, name)) {
        Some(index) => AssetEdge::bind(index, materials.zone_of(index.order())),
        None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
    }
}

pub(super) fn stamp_fx_edge(
    slot: Option<Ptr>,
    ns: crate::AssetNamespace,
    fx: &crate::FxCatalog,
    edge: &mut AssetEdge<FxSpace>,
    hint: &mut Option<String>,
) {
    let leftover = hint.as_deref().filter(|s| !s.is_empty()).map(str::to_owned);
    let slot_name = slot.and_then(|s| fx.name_at_slot(s)).map(str::to_owned);
    let name = leftover.or(slot_name);
    *hint = name.clone();
    *edge = match name.as_deref() {
        None if slot.is_none() => AssetEdge::Absent,
        Some(name) => match fx.index_in(ns, name) {
            Some(index) => AssetEdge::bind_order(index, fx.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
        None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
    };
}

pub(super) fn stamp_combat_fx(
    combat: &mut WeaponCombatFx,
    slots: CombatFxSlots,
    ns: crate::AssetNamespace,
    fx: &crate::FxCatalog,
    tracers: &crate::TracerCatalog,
) {
    combat.namespace = ns;
    stamp_fx_edge(
        slots.view_flash,
        ns,
        fx,
        &mut combat.view_flash,
        &mut combat.view_flash_hint,
    );
    stamp_fx_edge(
        slots.world_flash,
        ns,
        fx,
        &mut combat.world_flash,
        &mut combat.world_flash_hint,
    );
    stamp_fx_edge(
        slots.view_shell_eject,
        ns,
        fx,
        &mut combat.view_shell_eject,
        &mut combat.view_shell_eject_hint,
    );
    stamp_fx_edge(
        slots.world_shell_eject,
        ns,
        fx,
        &mut combat.world_shell_eject,
        &mut combat.world_shell_eject_hint,
    );
    stamp_fx_edge(
        slots.view_last_shot_eject,
        ns,
        fx,
        &mut combat.view_last_shot_eject,
        &mut combat.view_last_shot_eject_hint,
    );
    stamp_fx_edge(
        slots.world_last_shot_eject,
        ns,
        fx,
        &mut combat.world_last_shot_eject,
        &mut combat.world_last_shot_eject_hint,
    );
    stamp_fx_edge(
        slots.explosion,
        ns,
        fx,
        &mut combat.explosion,
        &mut combat.explosion_hint,
    );
    let tracer = slots
        .tracer
        .and_then(|s| tracers.index_at_slot(s))
        .and_then(|index| Some((index, tracers.def_at(index)?)));
    combat.tracer_hint = tracer.map(|(_, def)| def.name.clone());
    combat.tracer = match (slots.tracer, tracer) {
        (None, _) => AssetEdge::Absent,
        (_, Some((index, _))) => AssetEdge::bind_order(index, tracers.zone_of(index)),
        (Some(_), None) => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
    };
    combat.last_shot_eject_pair_authored = slots.last_shot_pair_authored();
}
