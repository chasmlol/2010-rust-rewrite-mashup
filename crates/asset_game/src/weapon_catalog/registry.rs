use super::*;

impl WeaponRegistry {
    pub fn vehicle_compass(&self) -> impl Iterator<Item = (&str, &[String; 2], [i32; 2])> {
        self.vehicle_compass
            .iter()
            .map(|(name, (icons, size))| (name.as_str(), icons, *size))
    }

    pub fn vehicle_accel(&self) -> impl Iterator<Item = (&str, f32)> {
        self.vehicle_accel
            .iter()
            .map(|(name, accel)| (name.as_str(), *accel))
    }

    pub fn vehicle_turrets(&self) -> Vec<(String, String)> {
        self.vehicle_turrets
            .iter()
            .map(|(vehicle, weapon)| (vehicle.clone(), weapon.clone()))
            .collect()
    }

    pub fn world_catalog_identity(&self) -> u64 {
        self.world_catalog_identity
    }

    pub(crate) fn iw5_configuration_of(&self, id: u32) -> Option<(u32, Iw5AttachmentSelection)> {
        self.rows.get(id as usize)?.iw5_configuration
    }

    pub(super) fn rebuild_name_maps(&mut self) {
        self.by_name.clear();
        self.by_namespaced.clear();
        for (id, row) in self.rows.iter().enumerate().skip(1) {
            if row.iw5_configuration.is_some() {
                continue;
            }
            self.by_namespaced
                .insert((row.namespace, row.name.clone()), id as u32);
            self.by_name.entry(row.name.clone()).or_insert(id as u32);
        }
        for row in &mut self.rows {
            if let Some(name) = row.alternate_weapon.as_deref() {
                row.alternate_index = self
                    .by_namespaced
                    .get(&(row.namespace, normalize_weapon_name(name)))
                    .copied()
                    .unwrap_or(0);
            }
        }
    }

    pub fn reticle_of(&self, index: u32) -> Option<&WeaponReticleAssets> {
        self.rows.get(index as usize).map(|row| &row.reticle)
    }

    pub fn projectile_fx_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            for edge in row.projectile_fx.edges() {
                census.push(edge);
            }
        }
        census
    }

    pub fn sz_xanim_edges_of(
        &self,
        index: u32,
    ) -> Option<&[AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS]> {
        self.rows.get(index as usize).map(|row| &row.sz_xanim_edges)
    }

    pub fn notetrack_actions_of(
        &self,
        index: u32,
    ) -> impl Iterator<Item = (&str, &LinkedNotetrackAction)> {
        self.rows
            .get(index as usize)
            .into_iter()
            .flat_map(|row| row.notetrack_actions.iter())
            .map(|(note, action)| (note.as_str(), action))
    }

    pub fn notetrack_sound_aliases_of(&self, index: u32) -> impl Iterator<Item = &str> {
        self.rows
            .get(index as usize)
            .into_iter()
            .flat_map(|row| row.notetrack_actions.values())
            .filter_map(|action| action.sound_alias.as_deref())
    }

    pub fn sz_xanim_right_edges_of(
        &self,
        index: u32,
    ) -> Option<&[AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS]> {
        self.rows
            .get(index as usize)
            .map(|row| &row.sz_xanim_right_edges)
    }

    pub fn sz_xanim_left_edges_of(
        &self,
        index: u32,
    ) -> Option<&[AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS]> {
        self.rows
            .get(index as usize)
            .map(|row| &row.sz_xanim_left_edges)
    }

    pub fn sz_xanim_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            for edge in &row.sz_xanim_edges {
                census.push(*edge);
            }
        }
        census
    }

    pub fn bound_weapon_xanim_indices(&self) -> Vec<usize> {
        let mut indices = std::collections::BTreeSet::new();
        for row in self.rows.iter().skip(1) {
            for edge in row
                .sz_xanim_edges
                .iter()
                .chain(&row.sz_xanim_right_edges)
                .chain(&row.sz_xanim_left_edges)
            {
                if let Some(index) = edge.bound_index() {
                    indices.insert(index);
                }
            }
        }
        indices.into_iter().collect()
    }

    pub fn gun_xmodel_edge_of(&self, index: u32) -> Option<AssetEdge<FpvMeshSpace>> {
        self.rows.get(index as usize).map(|row| row.gun_xmodel_edge)
    }

    pub fn material_camouflages_of(&self, weapon: u32) -> &[crate::WeaponCamouflage] {
        self.rows
            .get(weapon as usize)
            .map_or(&[], |row| &row.material_camos)
    }

    pub fn camouflage_choices(&self, weapon: u32) -> Vec<(u8, &str)> {
        let camos = self.material_camouflages_of(weapon);
        if !camos.is_empty() {
            return camos
                .iter()
                .map(|camo| (camo.slot, camo.name.as_str()))
                .collect();
        }
        self.camo_models_of(weapon).map_or_else(Vec::new, |models| {
            models
                .view
                .iter()
                .filter_map(|(slot, _)| {
                    weapon_iw4::IW4_CAMOS
                        .get(usize::from(*slot))
                        .filter(|_| *slot != 0)
                        .map(|name| (*slot, *name))
                })
                .collect()
        })
    }

    pub fn camouflage_slot(&self, weapon: u32, name: &str) -> Option<u8> {
        if name.is_empty() || name.eq_ignore_ascii_case("none") {
            return Some(0);
        }
        self.camouflage_choices(weapon)
            .into_iter()
            .find(|(_, own)| own.eq_ignore_ascii_case(name))
            .map(|(slot, _)| slot)
    }

    pub fn camouflage_preview(&self, weapon: u32, name: &str) -> String {
        if name.is_empty() || name.eq_ignore_ascii_case("none") {
            return String::new();
        }
        if let Some(camo) = self
            .material_camouflages_of(weapon)
            .iter()
            .find(|camo| camo.name.eq_ignore_ascii_case(name))
        {
            return camo.preview.clone();
        }
        format!("iw4:material/weapon_camo_menu_{name}")
    }

    pub fn camo_view_edges_of(&self, index: u32) -> &[(u8, AssetEdge<FpvMeshSpace>)] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.camo_view_edges.as_slice())
    }

    pub fn camo_world_edge_of(&self, index: u32, slot: u8) -> Option<AssetEdge<WorldWeaponSpace>> {
        let row = self.rows.get(index as usize)?;
        row.camo_world_edges
            .iter()
            .find(|(own, _)| *own == slot)
            .map(|(_, edge)| *edge)
    }

    pub fn camo_models_of(&self, index: u32) -> Option<&WeaponCamoModels> {
        self.rows.get(index as usize).map(|row| &row.camo_models)
    }

    pub fn fpv_hands_of(
        &self,
        index: u32,
        axis: bool,
    ) -> Option<(&asset_model::FpvHands, crate::FpvMeshIndex)> {
        self.rows.get(index as usize)?.fpv_hands[usize::from(axis)]
            .as_ref()
            .map(|(choice, index)| (choice, *index))
    }

    pub fn alternate_fpv_pairs(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.alternate_fpv.keys().copied()
    }

    pub fn fpv_assemblies_for(
        &self,
        index: u32,
        parent: u32,
        axis: bool,
    ) -> Option<&crate::FpvSideAssemblies> {
        if parent != 0 && self.facts_of(index).is_some_and(|f| f.inventory_type == 3) {
            return self.alternate_fpv.get(&(index, parent))?[usize::from(axis)]
                .as_ref()?
                .as_ref()
                .ok();
        }
        self.fpv_assemblies_of(index, axis)
    }

    pub fn fpv_assemblies_of(&self, index: u32, axis: bool) -> Option<&crate::FpvSideAssemblies> {
        self.rows.get(index as usize)?.fpv_assemblies[usize::from(axis)]
            .as_ref()?
            .as_ref()
            .ok()
    }

    pub fn fpv_assembly_gap_of(&self, index: u32, axis: bool) -> String {
        let Some(row) = self.rows.get(index as usize) else {
            return "no such weapon".to_owned();
        };
        match (&row.fpv_mount_plan, &row.fpv_assemblies[usize::from(axis)]) {
            (None, _) => "unlinked selected models".to_owned(),
            (Some(Err(error)), _) => error.to_string(),
            (_, None) => "effective kit / weapon hands".to_owned(),
            (_, Some(Err(error))) => error.clone(),
            (_, Some(Ok(_))) => "linked".to_owned(),
        }
    }

    pub fn fpv_clip_tracks(&self) -> &crate::FpvClipTracks {
        &self.fpv_clip_tracks
    }

    pub fn world_model_edge_of(&self, index: u32) -> Option<AssetEdge<WorldWeaponSpace>> {
        self.rows
            .get(index as usize)
            .map(|row| row.world_model_edge)
    }

    pub fn attachment_world_model_edges_of(&self, index: u32) -> &[AssetEdge<WorldWeaponSpace>] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_world_model_edges.as_slice())
    }

    pub fn attachment_world_mounts_of(&self, index: u32) -> &[Option<String>] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_world_mounts.as_slice())
    }

    pub fn world_model_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            census.push(row.world_model_edge);
        }
        census
    }

    pub fn dependency_gaps(&self) -> Vec<WeaponDependencyGap> {
        (1..self.rows.len() as u32)
            .flat_map(|id| self.dependency_gaps_of(id))
            .collect()
    }

    pub fn dependency_gaps_of(&self, id: u32) -> Vec<WeaponDependencyGap> {
        let Some(row) = self.rows.get(id as usize).filter(|_| id != 0) else {
            return Vec::new();
        };
        let named = [
            (
                row.gun_xmodel.is_some() && !row.gun_xmodel_edge.is_bound(),
                "view model",
                &row.gun_xmodel,
            ),
            (
                row.hand_xmodel.is_some() && !row.hand_xmodel_edge.is_bound(),
                "hands",
                &row.hand_xmodel,
            ),
            (
                row.rocket_model.is_some() && !row.rocket_model_edge.is_bound(),
                "FPV rocket model",
                &row.rocket_model,
            ),
            (
                row.world_model_edge.is_unresolved(),
                "world model",
                &row.world_model,
            ),
        ];
        let anims = row
            .sz_xanim_edges
            .iter()
            .zip(&row.sz_xanims)
            .map(|(edge, name)| (edge.is_unresolved(), "anim", name));
        let dual_anims = !row.facts.no_dual_wield
            && row.sz_xanims_right[weap_anim::IDLE]
                .as_deref()
                .is_some_and(|name| !name.is_empty());
        let right_anims = row
            .sz_xanim_right_edges
            .iter()
            .zip(&row.sz_xanims_right)
            .map(|(edge, name)| (dual_anims && edge.is_unresolved(), "right anim", name));
        let left_anims = row
            .sz_xanim_left_edges
            .iter()
            .zip(&row.sz_xanims_left)
            .map(|(edge, name)| (dual_anims && edge.is_unresolved(), "left anim", name));
        let mut gaps: Vec<WeaponDependencyGap> = named
            .into_iter()
            .chain(anims)
            .chain(right_anims)
            .chain(left_anims)
            .filter(|(unresolved, ..)| *unresolved)
            .map(|(_, kind, name)| WeaponDependencyGap {
                id,
                kind,
                name: name.clone().unwrap_or_default(),
            })
            .collect();
        if let Some(Err(error)) = &row.fpv_mount_plan {
            gaps.push(WeaponDependencyGap {
                id,
                kind: "FPV mount",
                name: error.to_string(),
            });
        }
        for (side, assembly) in row.fpv_assemblies.iter().enumerate() {
            if let Some(Err(error)) = assembly {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "FPV skeleton",
                    name: format!("{}: {error}", if side == 0 { "allies" } else { "axis" }),
                });
            }
        }
        if row.gun_xmodel_edge.is_bound() {
            for (side, hands) in row.fpv_hands.iter().enumerate() {
                if hands.is_none() {
                    gaps.push(WeaponDependencyGap {
                        id,
                        kind: "FPV hands layout",
                        name: if side == 0 { "allies" } else { "axis" }.to_owned(),
                    });
                }
            }
        }
        for (name, edge) in row
            .attachment_view_models
            .iter()
            .zip(&row.attachment_view_model_edges)
        {
            if !edge.is_bound() {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "FPV attachment model",
                    name: name.clone(),
                });
            }
        }
        for (name, edge) in row
            .attachment_world_models
            .iter()
            .zip(&row.attachment_world_model_edges)
        {
            if !edge.is_bound() {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "world attachment model",
                    name: name.clone(),
                });
            }
        }
        for ((name, edge), mount) in row
            .attachment_world_models
            .iter()
            .zip(&row.attachment_world_model_edges)
            .zip(&row.attachment_world_mounts)
        {
            if edge.is_bound() && mount.is_none() {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "world attachment mount",
                    name: name.clone(),
                });
            }
        }
        if row.attachment_world_models.len() != row.attachment_world_model_edges.len() {
            gaps.push(WeaponDependencyGap {
                id,
                kind: "world attachment bindings",
                name: format!(
                    "{} names, {} links",
                    row.attachment_world_models.len(),
                    row.attachment_world_model_edges.len()
                ),
            });
        }
        if row.attachment_view_models.len() != row.attachment_view_model_edges.len() {
            gaps.push(WeaponDependencyGap {
                id,
                kind: "FPV attachment bindings",
                name: format!(
                    "{} names, {} links",
                    row.attachment_view_models.len(),
                    row.attachment_view_model_edges.len()
                ),
            });
        }
        gaps
    }

    pub fn configuration_admission(&self, id: u32) -> Result<(), crate::ConfigurationRefusal> {
        if !self.configuration_supported(id) {
            return Err(crate::ConfigurationRefusal::Unsupported(format!(
                "`{}` needs a runtime mechanism IW4L lacks",
                self.name_of(id)
            )));
        }
        match self.dependency_gaps_of(id).first() {
            Some(gap) => Err(crate::ConfigurationRefusal::MissingDependency {
                weapon: self.name_of(id).to_owned(),
                kind: gap.kind,
                name: gap.name.clone(),
            }),
            None => Ok(()),
        }
    }

    pub fn world_model_entry<'a>(
        &self,
        index: u32,
        catalog: &'a crate::WorldWeaponCatalog,
    ) -> Option<&'a crate::WorldWeaponEntry> {
        if self.world_catalog_identity != catalog.identity() {
            return None;
        }
        catalog.get_at(self.world_model_edge_of(index)?.bound_index()?)
    }

    pub fn world_model_entry_for<'a>(
        &self,
        index: u32,
        camo: u8,
        catalog: &'a crate::WorldWeaponCatalog,
    ) -> Option<&'a crate::WorldWeaponEntry> {
        if camo != 0
            && self.world_catalog_identity == catalog.identity()
            && let Some(entry) = self
                .camo_world_edge_of(index, camo)
                .and_then(|edge| edge.bound_index())
                .and_then(|order| catalog.get_at(order))
        {
            return Some(entry);
        }
        self.world_model_entry(index, catalog)
    }

    pub fn authored_weapon_sound(&self, index: u32, slot: WeaponSoundSlot) -> Option<&str> {
        self.sounds_of(index)?
            .hint(slot)
            .filter(|name| !name.is_empty())
    }

    pub fn weapon_sound_alias<'a>(
        &self,
        index: u32,
        slot: WeaponSoundSlot,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<&'a str> {
        self.weapon_sound_key(index, slot, catalog)
            .map(|(_, alias)| alias)
    }

    pub fn weapon_sound_key<'a>(
        &self,
        index: u32,
        slot: WeaponSoundSlot,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<(crate::AssetNamespace, &'a str)> {
        let alias = self.authored_weapon_sound(index, slot)?;
        let ns = self.sound_namespace_for(index, alias)?;
        sound_alias_in_bank(Some(alias), ns, catalog)
    }

    pub fn bounce_sound_alias<'a>(
        &self,
        index: u32,
        surf: usize,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<&'a str> {
        let alias = self.bounce_sound_of(index, surf)?;
        let ns = self.sound_namespace_for(index, alias)?;
        sound_alias_in_bank(Some(alias), ns, catalog).map(|(_, alias)| alias)
    }

    pub fn hud_material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in &self.rows {
            census.push(row.reticle.center_edge);
            census.push(row.reticle.side_edge);
        }
        for row in &self.rows {
            census.push(row.hud_material_edges.overlay);
            census.push(row.hud_material_edges.hud_icon);
            census.push(row.hud_material_edges.pickup_icon);
            census.push(row.hud_material_edges.kill_icon);
        }
        census
    }

    pub fn display_name_key_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.display_name_key.as_deref())
    }

    pub(crate) fn facts_of(&self, index: u32) -> Option<WeaponBodyFacts> {
        self.rows.get(index as usize).map(|row| row.facts)
    }

    pub fn semantic_policy_of(&self, index: u32) -> Option<&crate::WeaponSemanticPolicy> {
        self.rows.get(index as usize)?.semantics.as_ref()
    }

    pub fn melee_weapon_of(&self, index: u32) -> u32 {
        self.semantic_policy_of(index)
            .map_or(index, |policy| policy.melee_weapon.weapon(index))
    }

    pub fn melee_impact_sound_key<'a>(
        &self,
        index: u32,
        knife: bool,
        impact: crate::MeleeImpact,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<(crate::AssetNamespace, &'a str)> {
        let row = self.rows.get(index as usize)?;
        let semantics = row.semantics.as_ref()?;
        let policy = semantics.melee_cues;
        let namespace = row.component_namespace(WeaponComponent::Sound)?;
        let slot = match impact {
            crate::MeleeImpact::Hit => WeaponSoundSlot::MeleeHit,
            crate::MeleeImpact::Miss => WeaponSoundSlot::MeleeMiss,
        };
        let own = || self.weapon_sound_key(index, slot, catalog);
        let generic =
            |knife| sound_alias_in_bank(Some(policy.generic(knife, impact)), namespace, catalog);
        let knife_alias = || knife.then(|| generic(true)).flatten();
        match policy.precedence {
            crate::MeleeCuePrecedence::T5AuthoredFirstCompatibility => own().or_else(knife_alias),
            crate::MeleeCuePrecedence::KnifeFirstCompatibility => knife_alias().or_else(own),
        }
        .or_else(|| generic(false))
    }

    pub fn alternate_of(&self, index: u32) -> u32 {
        self.rows
            .get(index as usize)
            .map_or(0, |row| row.alternate_index)
    }

    pub fn resolve_index(&self, name: &str) -> Result<Option<u32>, UnknownWeaponName> {
        if name.is_empty() {
            return Ok(None);
        }
        if name.contains(':') {
            match crate::AssetKey::parse(name) {
                Ok(key) => return self.resolve_key(&key),
                Err(_) => {
                    return Err(UnknownWeaponName {
                        name: name.to_owned(),
                    });
                }
            }
        }
        let norm = normalize_weapon_name(name);
        match self.by_name.get(&norm).copied() {
            Some(id) => Ok(Some(id)),
            None => Err(UnknownWeaponName { name: norm }),
        }
    }

    pub fn resolve_key(&self, key: &crate::AssetKey) -> Result<Option<u32>, UnknownWeaponName> {
        if key.kind != crate::AssetKind::Weapon {
            return Err(UnknownWeaponName {
                name: key.display(),
            });
        }
        let norm = normalize_weapon_name(key.logical_name());
        match self.by_namespaced.get(&(key.namespace, norm)).copied() {
            Some(id) => Ok(Some(id)),
            None => Err(UnknownWeaponName {
                name: key.display(),
            }),
        }
    }

    pub fn identity_namespace_of(&self, index: u32) -> Option<crate::AssetNamespace> {
        if index == 0 {
            return None;
        }
        self.rows.get(index as usize).map(|row| row.namespace)
    }

    pub fn sound_namespace_for(&self, index: u32, alias: &str) -> Option<crate::AssetNamespace> {
        self.rows
            .get(index as usize)
            .filter(|_| index != 0)?
            .preparation
            .namespace_for(WeaponComponent::Sound, alias)
    }

    pub fn projectile_model_reference_of(
        &self,
        index: u32,
    ) -> Option<(crate::AssetNamespace, &str)> {
        let row = self.rows.get(index as usize)?;
        let name = row.projectile_model.as_deref()?;
        row.projectile_model_edge.bound_index()?;
        Some((
            row.preparation
                .namespace_for(WeaponComponent::Projectile, name)?,
            name,
        ))
    }

    pub fn component_namespace_of(
        &self,
        index: u32,
        component: WeaponComponent,
    ) -> Option<crate::AssetNamespace> {
        self.rows
            .get(index as usize)
            .filter(|_| index != 0)?
            .component_namespace(component)
    }

    pub fn host_namespace_of(&self, index: u32) -> Option<crate::AssetNamespace> {
        self.rows
            .get(index as usize)
            .filter(|_| index != 0)
            .map(|row| row.preparation.host_namespace())
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn item_group_count(&self) -> usize {
        self.item_groups.len()
    }

    pub fn namespaced_key_of(&self, index: u32) -> Option<String> {
        self.key_of(index).map(|key| key.to_string())
    }

    pub fn key_of(&self, index: u32) -> Option<crate::AssetKey> {
        let ns = self.identity_namespace_of(index)?;
        let name = self.name_of(index);
        if name.is_empty() {
            return None;
        }
        crate::AssetKey::new(ns, crate::AssetKind::Weapon, gsc_weapon_script_name(name)).ok()
    }

    pub fn namespace_count(&self, ns: crate::AssetNamespace) -> usize {
        self.rows
            .iter()
            .skip(1)
            .filter(|row| row.namespace == ns)
            .count()
    }

    #[deprecated(note = "use resolve_index — unknown must not become 0")]
    pub fn index_of(&self, name: &str) -> u32 {
        match self.resolve_index(name) {
            Ok(Some(id)) => id,
            Ok(None) | Err(_) => 0,
        }
    }

    pub fn name_of(&self, index: u32) -> &str {
        self.rows
            .get(index as usize)
            .map(|row| row.name.as_str())
            .unwrap_or("")
    }

    pub fn script_name_of(&self, index: u32) -> String {
        gsc_weapon_script_name(self.name_of(index))
    }

    pub fn world_models_table(&self) -> Vec<(String, Vec<String>)> {
        (0..self.rows.len() as u32)
            .map(|i| {
                let model = self.world_model_of(i).unwrap_or_default().to_owned();
                (model, self.hide_tags_of(i).to_vec())
            })
            .collect()
    }

    pub fn projectile_models_table(&self) -> Vec<String> {
        (0..self.rows.len() as u32)
            .map(|i| self.projectile_model_of(i).unwrap_or_default().to_owned())
            .collect()
    }

    pub fn script_names_table(&self) -> Vec<String> {
        (0..self.rows.len())
            .map(|i| self.script_name_of(i as u32))
            .collect()
    }

    pub fn configuration_supported(&self, id: u32) -> bool {
        let Some(row) = self.rows.get(id as usize) else {
            return false;
        };
        if row.namespace != crate::AssetNamespace::T5 {
            return true;
        }
        if row.facts.dual_wield {
            return row.secondary_gun_xmodel.is_some()
                && xanims_idle(&row.sz_xanims_left).is_some();
        }
        !row.gun_xmodel.as_deref().is_some_and(|name| {
            name.ends_with("_dw_rh") || name.ends_with("_dw_lh") || name.ends_with("_lh_viewmodel")
        })
    }

    pub fn runnable_table(&self) -> Vec<bool> {
        (0..self.rows.len() as u32)
            .map(|id| id != 0 && self.configuration_admission(id).is_ok())
            .collect()
    }

    pub fn weapon_families(&self) -> &crate::WeaponFamilies {
        &self.families
    }

    pub fn resolve_configuration(
        &self,
        selection: &crate::WeaponSelection,
        rules: crate::LoadoutRules,
    ) -> Result<crate::ResolvedConfiguration, crate::ConfigurationRefusal> {
        self.families.resolve(selection, rules, self)
    }

    pub fn describe_configuration(&self, id: u32) -> Option<&crate::WeaponSelection> {
        self.families.describe(id)
    }

    pub fn attachment_caption_keys_of(&self, index: u32) -> &[String] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_caption_keys.as_slice())
    }

    pub fn prepared_attachments_of(&self, id: u32) -> &[String] {
        self.rows
            .get(id as usize)
            .map_or(&[], |row| row.prepared_attachments.as_slice())
    }

    pub fn weapon_completion_names(&self) -> &[String] {
        &self.completion_names
    }

    pub fn configuration_label(&self, id: u32) -> String {
        match self
            .describe_configuration(id)
            .and_then(|selection| Some((selection.family.as_ref()?, &selection.attachments)))
        {
            Some((family, attachments)) => {
                let mut label = family.short();
                for name in attachments {
                    label.push_str(" +");
                    label.push_str(name);
                }
                label
            }
            None => self.namespaced_key_of(id).unwrap_or_default(),
        }
    }

    pub fn configuration_transition_groups(&self) -> Vec<u32> {
        let mut keys: Vec<_> = self
            .families
            .families()
            .iter()
            .map(|family| family.key.clone())
            .collect();
        keys.sort_by_key(|key| key.asset_key());
        keys.dedup();
        let groups: HashMap<_, _> = keys
            .into_iter()
            .enumerate()
            .map(|(index, key)| (key, index as u32 + 1))
            .collect();
        let mut result = vec![0; self.rows.len()];
        for id in 1..self.rows.len() as u32 {
            let Some(selection) = self.describe_configuration(id) else {
                continue;
            };
            let Some(key) = selection.family.as_ref() else {
                continue;
            };
            if self
                .resolve_configuration(selection, crate::LoadoutRules::default())
                .is_ok_and(|resolved| {
                    self.bind(resolved.handle())
                        .is_ok_and(|weapon| weapon.wire_id() == id)
                })
            {
                result[id as usize] = groups.get(key).copied().unwrap_or(0);
            }
        }
        result
    }

    pub fn list_attachment_choices(
        &self,
        selection: &crate::WeaponSelection,
        rules: crate::LoadoutRules,
    ) -> Result<Vec<crate::AttachmentOption>, crate::ConfigurationRefusal> {
        self.families.attachment_options(selection, rules, self)
    }

    pub fn gun_xmodel_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.gun_xmodel.as_deref())
    }

    pub fn hand_xmodel_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.hand_xmodel.as_deref())
    }

    pub fn world_model_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.world_model.as_deref())
    }

    pub fn impact_payload_of(&self, index: u32) -> Option<u32> {
        let row = self.rows.get(index as usize)?;
        let name = normalize_weapon_name(row.impact_payload.as_deref()?);
        self.by_namespaced.get(&(row.namespace, name)).copied()
    }

    pub fn projectile_model_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.projectile_model.as_deref())
    }

    pub fn rocket_model_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.rocket_model.as_deref())
    }

    pub fn anim_of(&self, index: u32, slot: usize) -> Option<&str> {
        self.rows
            .get(index as usize)?
            .sz_xanims
            .get(slot)?
            .as_deref()
    }

    pub fn idle_anim_of(&self, index: u32) -> Option<&str> {
        self.anim_of(index, weap_anim::IDLE)
    }

    pub fn hide_tags_of(&self, index: u32) -> &[String] {
        self.rows
            .get(index as usize)
            .map(|row| row.hide_tags.as_slice())
            .unwrap_or(&[])
    }

    pub fn sounds_of(&self, index: u32) -> Option<&WeaponSoundAliases> {
        self.rows.get(index as usize).map(|row| &row.sounds)
    }

    pub fn bounce_sound_of(&self, index: u32, surf: usize) -> Option<&str> {
        self.sounds_of(index)?.bounce.get(surf)?.as_deref()
    }

    pub fn combat_fx_of(&self, index: u32) -> Option<&WeaponCombatFx> {
        self.rows.get(index as usize).map(|row| &row.combat_fx)
    }

    pub fn tracer_type_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            census.push(row.combat_fx.tracer);
        }
        census
    }

    pub fn combat_fx_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            for edge in row.combat_fx.edges() {
                census.push(edge);
            }
        }
        census
    }

    pub fn sz_xanims_of(&self, index: u32) -> Option<&[Option<String>; WEAPON_ANIM_SLOTS]> {
        self.rows.get(index as usize).map(|row| &row.sz_xanims)
    }

    pub fn timers_of(&self, index: u32) -> (i32, i32) {
        self.facts_of(index)
            .map(|f| (f.fire_time_ms, f.raise_time_ms))
            .unwrap_or((0, 0))
    }

    pub fn switch_timers_of(&self, index: u32) -> (i32, i32, i32) {
        self.facts_of(index)
            .map(|f| (f.drop_time_ms, f.quick_drop_time_ms, f.quick_raise_time_ms))
            .unwrap_or((0, 0, 0))
    }

    pub fn sprint_timers_of(&self, index: u32) -> (i32, i32, i32) {
        self.facts_of(index)
            .map(|f| {
                (
                    f.sprint_raise_time_ms,
                    f.sprint_loop_time_ms,
                    f.sprint_drop_time_ms,
                )
            })
            .unwrap_or((0, 0, 0))
    }

    pub fn quick_reload_timers_of(&self, index: u32) -> Option<(i32, i32)> {
        let dual_mag = self.facts_of(index)?.dual_mag?;
        Some((dual_mag.reload_ms, dual_mag.reload_empty_ms))
    }

    pub fn reload_timers_of(&self, index: u32) -> (i32, i32, i32, i32) {
        self.facts_of(index)
            .map(|f| {
                (
                    f.reload_time_ms,
                    f.reload_empty_time_ms,
                    f.reload_start_time_ms,
                    f.reload_end_time_ms,
                )
            })
            .unwrap_or((0, 0, 0, 0))
    }

    pub fn len(&self) -> usize {
        self.rows.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn scales_table(&self) -> Vec<(f32, f32, f32)> {
        self.rows
            .iter()
            .map(|row| {
                (
                    row.facts.move_speed_scale,
                    row.facts.ads_move_speed_scale,
                    row.facts.sprint_duration_scale,
                )
            })
            .collect()
    }

    pub fn camo_census(&self) -> (usize, usize, usize) {
        let with = self
            .rows
            .iter()
            .filter(|row| !row.camo_models.view.is_empty())
            .count();
        let view_bound = self
            .rows
            .iter()
            .flat_map(|row| &row.camo_view_edges)
            .filter(|(_, edge)| edge.is_bound())
            .count();
        let world_bound = self
            .rows
            .iter()
            .flat_map(|row| &row.camo_world_edges)
            .filter(|(_, edge)| edge.is_bound())
            .count();
        (with, view_bound, world_bound)
    }

    pub fn gun_xmodel_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.gun_xmodel.is_some())
            .count()
    }

    pub fn overlay_material_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.overlay_material.as_deref())
    }

    pub fn overlay_image_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.overlay_image.as_deref())
    }

    pub fn overlay_is_hud_iris(&self, index: u32) -> bool {
        self.overlay_material_of(index)
            .is_some_and(overlay_name_is_hud_iris)
    }

    pub fn pickup_icon_of(&self, index: u32) -> Option<(&str, i32)> {
        let row = self.rows.get(index as usize)?;
        if row.pickup_icon_authored {
            Some((row.pickup_icon_image.as_deref()?, row.pickup_icon_ratio))
        } else {
            Some((row.hud_icon_image.as_deref()?, row.hud_icon_ratio))
        }
    }

    pub fn hud_icon_namespace_of(&self, index: u32) -> Option<crate::AssetNamespace> {
        self.component_namespace_of(index, WeaponComponent::HudIconImage)
    }

    pub fn hud_icon_image_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.hud_icon_image.as_deref())
    }

    pub fn dpad_icon_of(&self, index: u32) -> Option<(&str, i32)> {
        let row = self.rows.get(index as usize)?;
        Some((row.dpad_icon_image.as_deref()?, row.dpad_icon_ratio))
    }

    pub fn dpad_icon_atlas_of(&self, index: u32) -> Option<[u8; 2]> {
        self.rows.get(index as usize)?.dpad_icon_atlas
    }

    pub fn kill_icon_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.kill_icon.as_deref())
    }

    pub fn kill_icon_image_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.kill_icon_image.as_deref())
    }

    pub fn proj_trail_of(&self, index: u32) -> Option<crate::FxName<'_>> {
        let row = self.rows.get(index as usize)?;
        row.projectile_fx.trail.is_bound().then_some(())?;
        Some(crate::FxName::new(
            row.component_namespace(WeaponComponent::Effect)?,
            row.proj_trail.as_deref()?,
        ))
    }

    pub fn proj_beacon_of(&self, index: u32) -> Option<crate::FxName<'_>> {
        let row = self.rows.get(index as usize)?;
        row.projectile_fx.beacon.is_bound().then_some(())?;
        Some(crate::FxName::new(
            row.component_namespace(WeaponComponent::Effect)?,
            row.proj_beacon.as_deref()?,
        ))
    }

    pub fn proj_ignition_of(&self, index: u32) -> Option<crate::FxName<'_>> {
        let row = self.rows.get(index as usize)?;
        row.projectile_fx.ignition.is_bound().then_some(())?;
        Some(crate::FxName::new(
            row.component_namespace(WeaponComponent::Effect)?,
            row.proj_ignition.as_deref()?,
        ))
    }

    pub fn world_model_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.world_model.is_some())
            .count()
    }

    pub fn projectile_model_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.projectile_model.is_some())
            .count()
    }

    pub fn projectile_model_bound_n(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.projectile_model_edge.is_bound())
            .count()
    }

    pub fn projectile_model_name_hint_n(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.projectile_model.is_some() && !row.projectile_model_edge.is_bound())
            .count()
    }

    pub fn idle_anim_count(&self) -> usize {
        (1..=self.len() as u32)
            .filter(|&id| self.idle_anim_of(id).is_some())
            .count()
    }

    pub fn sz_xanims_count(&self) -> usize {
        self.rows
            .iter()
            .skip(1)
            .filter(|row| row.sz_xanims.iter().any(|n| n.is_some()))
            .count()
    }
}
