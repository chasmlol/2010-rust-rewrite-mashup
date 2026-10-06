use super::*;

impl WeaponRegistry {
    pub(super) fn iw5_attachment_slots_of(
        &self,
        id: u32,
    ) -> Option<&[Option<String>; fastfile_iw5::size::WEAPON_ATTACHMENT_SLOT_COUNT]> {
        let row = self.rows.get(id as usize)?;
        (row.namespace == crate::AssetNamespace::Iw5).then_some(&row.iw5_attachment_slots)
    }

    pub(super) fn iw5_attachment_asset(&self, native_name: &str) -> Option<&Iw5ScopeRow> {
        self.iw5_attachments.get(native_name)
    }

    pub(super) fn iw5_reload_overrides_of(
        &self,
        id: u32,
    ) -> Option<&[fastfile_iw5::ReloadOverride]> {
        let row = self.rows.get(id as usize)?;
        (row.namespace == crate::AssetNamespace::Iw5).then_some(row.iw5_reload_overrides.as_slice())
    }

    pub(super) fn iw5_anim_overrides_of(&self, id: u32) -> Option<&[LeftoverAnimOverride]> {
        let row = self.rows.get(id as usize)?;
        (row.namespace == crate::AssetNamespace::Iw5).then_some(row.iw5_anim_overrides.as_slice())
    }

    pub(super) fn iw5_fx_overrides_of(&self, id: u32) -> Option<&[Iw5FxOverride]> {
        let row = self.rows.get(id as usize)?;
        (row.namespace == crate::AssetNamespace::Iw5).then_some(row.iw5_fx_overrides.as_slice())
    }

    pub(super) fn iw5_notetrack_overrides_of(&self, id: u32) -> Option<&[Iw5NotetrackOverride]> {
        let row = self.rows.get(id as usize)?;
        (row.namespace == crate::AssetNamespace::Iw5)
            .then_some(row.iw5_notetrack_overrides.as_slice())
    }

    pub(super) fn select_iw5_anim_override(
        &self,
        id: u32,
        selection: Iw5AttachmentSelection,
        anim_tree_type: u32,
    ) -> Option<&LeftoverAnimOverride> {
        iw5_best_pair_override(
            self.iw5_anim_overrides_of(id)?,
            selection,
            anim_tree_type,
            |row| (row.attachment1, row.attachment2, row.anim_tree_type),
        )
    }

    pub(super) fn select_iw5_sound_override(
        &self,
        id: u32,
        selection: Iw5AttachmentSelection,
        sound_type: u32,
    ) -> Option<&LeftoverSoundOverride> {
        let row = self.rows.get(id as usize)?;
        if row.namespace != crate::AssetNamespace::Iw5 {
            return None;
        }
        iw5_best_pair_override(
            &row.sounds.leftover_sound_overrides,
            selection,
            sound_type,
            |row| (row.attachment1, row.attachment2, row.sound_type),
        )
    }

    pub(super) fn select_iw5_fx_override(
        &self,
        id: u32,
        selection: Iw5AttachmentSelection,
        fx_type: u32,
    ) -> Option<&Iw5FxOverride> {
        iw5_best_pair_override(self.iw5_fx_overrides_of(id)?, selection, fx_type, |row| {
            (row.attachment1, row.attachment2, row.fx_type)
        })
    }

    pub(super) fn select_iw5_reload_override(
        &self,
        id: u32,
        selection: Iw5AttachmentSelection,
    ) -> Option<&fastfile_iw5::ReloadOverride> {
        self.iw5_reload_overrides_of(id)?
            .iter()
            .find(|row| row.attachment != 0 && selection.contains_condition(row.attachment))
    }

    pub(super) fn select_iw5_notetrack_override(
        &self,
        id: u32,
        selection: Iw5AttachmentSelection,
    ) -> Option<&Iw5NotetrackOverride> {
        self.iw5_notetrack_overrides_of(id)?
            .iter()
            .find(|row| row.attachment != 0 && selection.contains_condition(row.attachment))
    }

    pub(super) fn resolve_iw5_attachment_slots(
        &self,
        base_id: u32,
        names: &[String],
    ) -> Result<Iw5AttachmentSelection, crate::ConfigurationRefusal> {
        let slots = self.iw5_attachment_slots_of(base_id).ok_or_else(|| {
            crate::ConfigurationRefusal::UnknownFamily(self.name_of(base_id).to_owned())
        })?;
        let mut selected = Iw5AttachmentSelection::default();
        for name in names {
            let mut matches = slots.iter().enumerate().filter(|(_, slot)| {
                slot.as_deref()
                    .is_some_and(|native| native.eq_ignore_ascii_case(name))
            });
            let direct = matches.next();
            if matches.next().is_some() {
                return Err(crate::ConfigurationRefusal::Unsupported(format!(
                    "`{name}` has more than one native IW5 slot in `{}`",
                    self.name_of(base_id)
                )));
            }
            let index = if let Some((index, _)) = direct {
                index
            } else {
                let display_key = format!("WEAPON_{}_ATTACHMENT", name.to_ascii_uppercase());
                let mut by_display = slots.iter().enumerate().filter(|(_, slot)| {
                    slot.as_deref()
                        .and_then(|native| self.iw5_attachments.get(native))
                        .and_then(|asset| asset.display_name.as_deref())
                        .is_some_and(|key| key.eq_ignore_ascii_case(&display_key))
                });
                let Some((index, _)) = by_display.next() else {
                    return Err(crate::ConfigurationRefusal::NotOffered(name.clone()));
                };
                if by_display.next().is_some() {
                    return Err(crate::ConfigurationRefusal::Unsupported(format!(
                        "`{name}` has an ambiguous IW5 display key in `{}`",
                        self.name_of(base_id)
                    )));
                }
                index
            };
            let native = slots[index].as_deref().expect("matched native slot");
            if !self.iw5_attachments.contains_key(native) {
                return Err(crate::ConfigurationRefusal::MissingContent(
                    native.to_owned(),
                ));
            }
            match index {
                0..=5 => {
                    if selected.scope != 0 && selected.scope != (index + 1) as u8 {
                        return Err(crate::ConfigurationRefusal::Incompatible {
                            a: slots[selected.scope as usize - 1]
                                .clone()
                                .unwrap_or_default(),
                            b: name.clone(),
                        });
                    }
                    selected.scope = (index + 1) as u8;
                }
                6..=8 => {
                    if selected.underbarrel != 0 && selected.underbarrel != (index - 5) as u8 {
                        return Err(crate::ConfigurationRefusal::Incompatible {
                            a: slots[selected.underbarrel as usize + 5]
                                .clone()
                                .unwrap_or_default(),
                            b: name.clone(),
                        });
                    }
                    selected.underbarrel = (index - 5) as u8;
                }
                9..=12 => selected.others |= 1 << (index - 9),
                _ => unreachable!(),
            }
        }
        Ok(selected)
    }

    pub(super) fn iw5_primary_attachment_assets(
        &self,
        base_id: u32,
        selection: Iw5AttachmentSelection,
    ) -> Option<Vec<&Iw5ScopeRow>> {
        let slots = self.iw5_attachment_slots_of(base_id)?;
        let slot_asset = |index: usize| {
            slots
                .get(index)?
                .as_deref()
                .and_then(|name| self.iw5_attachment_asset(name))
        };
        let mut assets = Vec::with_capacity(3);
        let mut push = |asset| {
            if assets.len() < 3 {
                assets.push(asset);
            } else {
                assets[2] = asset;
            }
        };
        if selection.scope != 0 {
            push(slot_asset(usize::from(selection.scope - 1))?);
        }
        for bit in 0..4 {
            if selection.others & (1 << bit) != 0 {
                push(slot_asset(9 + bit)?);
            }
        }
        if selection.underbarrel != 0 {
            let asset = slot_asset(usize::from(selection.underbarrel) + 5)?;
            if asset.weapon_class == 0
                || asset.ads_settings_main.is_some()
                || (asset.scales.ads_settings_main != 0.0 && asset.scales.ads_settings_main != 1.0)
            {
                push(asset);
            }
        }
        Some(assets)
    }

    pub(super) fn compose_iw5_configuration(
        &self,
        base_id: u32,
        native: Iw5AttachmentSelection,
        alternate: bool,
    ) -> Option<WeaponRow> {
        use fastfile_iw5::size as sz;
        let base = self.rows.get(base_id as usize)?;
        let slot_asset = |index: usize| {
            base.iw5_attachment_slots
                .get(index)?
                .as_deref()
                .and_then(|native| self.iw5_attachments.get(native))
        };
        let scope = match native.scope {
            0 => None,
            index => Some(slot_asset(usize::from(index) - 1)?),
        };
        let underbarrel = match native.underbarrel {
            0 => None,
            index => Some(slot_asset(usize::from(index) + 5)?),
        };
        let others = (0..4)
            .filter(|bit| native.others & (1 << bit) != 0)
            .map(|bit| slot_asset(9 + bit))
            .collect::<Option<Vec<_>>>()?;

        let mut row = base.clone();
        row.iw5_configuration = Some((base_id, native));

        let first = |models: &[Option<String>]| models.first().cloned().flatten();
        let mut view = if scope.is_none() {
            base.attachment_view_models.clone()
        } else {
            Vec::new()
        };
        let mut world = if scope.is_none() {
            base.attachment_world_models.clone()
        } else {
            Vec::new()
        };
        if let Some(scope) = scope {
            view.extend(first(&scope.view_models));
            view.extend(first(&scope.reticle_models));
            world.extend(first(&scope.world_models));
        }
        for asset in underbarrel.into_iter().chain(others.iter().copied()) {
            view.extend(first(&asset.view_models));
            world.extend(first(&asset.world_models));
        }
        row.attachment_view_models = view;
        row.attachment_world_models = world;

        let primary_assets = self.iw5_primary_attachment_assets(base_id, native)?;
        let alternate_assets;
        let assets = if alternate {
            let underbarrel = underbarrel.filter(|asset| asset.weapon_class != 0)?;
            row.facts.weap_type = remap_iw5_weap_type(underbarrel.weapon_type);
            row.facts.weap_class = underbarrel.weapon_class;
            row.facts.inventory_type = 3;
            alternate_assets = if underbarrel.share_ammo_with_alt {
                primary_assets
            } else {
                vec![underbarrel]
            };
            &alternate_assets
        } else {
            &primary_assets
        };
        apply_iw5_parameter_blocks(&mut row.facts, assets);
        if let Some(reticle) = assets.iter().find_map(|asset| asset.reticle.as_ref()) {
            row.reticle = reticle.clone();
        }
        if alternate {
            if let Some(projectile) = iw5_first_block(assets, |asset| asset.projectile) {
                row.facts.explosion_radius = projectile.explosion_radius;
                row.facts.explosion_inner_damage = projectile.explosion_inner_damage;
                row.facts.explosion_outer_damage = projectile.explosion_outer_damage;
                row.facts.projectile_speed = projectile.speed;
                row.facts.projectile_speed_up = projectile.speed_up;
                row.facts.projectile_activate_dist = projectile.activate_distance;
                row.facts.projectile_explosion_type = projectile.explosion_type;
                row.facts.proj_impact_explode = projectile.impact_explode;
                let asset = assets.iter().find(|asset| asset.projectile.is_some())?;
                row.projectile_model = asset.projectile_model.clone();
                row.combat_fx.explosion_hint = asset.projectile_explosion_fx.clone();
                row.proj_trail = asset.projectile_trail_fx.clone();
                row.proj_trail_from_slot = row.proj_trail.is_some();
                row.proj_ignition = asset.projectile_ignition_fx.clone();
                row.proj_ignition_from_slot = row.proj_ignition.is_some();
                row.sounds.proj_explosion = asset.projectile_explosion_sound.clone();
                row.sounds.proj_ignition_sound = asset.projectile_ignition_sound.clone();
            }
        }

        if scope.is_some() || alternate {
            row.overlay_material = None;
            row.overlay_image = None;
            row.overlay_material_from_slot = false;
            row.facts.thermal_scope = false;
            row.facts.overlay_reticle = 0;
            row.facts.overlay_interface = 0;
            row.facts.ads_overlay_width = 0.0;
            row.facts.ads_overlay_height = 0.0;
        }
        if let Some(scope) = assets.iter().find(|asset| asset.overlay.is_some()) {
            if let Some(overlay) = scope
                .overlay
                .as_deref()
                .filter(|name| !name.is_empty() && overlay_name_is_hud_iris(name))
            {
                row.overlay_material = Some(overlay.to_owned());
                row.overlay_image = None;
                row.overlay_material_from_slot = true;
                row.facts.overlay_reticle = scope.overlay_reticle;
                row.facts.thermal_scope = scope.thermal;
                row.facts.ads_overlay_width = scope.width;
                row.facts.ads_overlay_height = scope.height;
            }
        }

        let mut anim_types: Vec<u32> = base
            .iw5_anim_overrides
            .iter()
            .map(|row| row.anim_tree_type)
            .collect();
        anim_types.sort_unstable();
        anim_types.dedup();
        for anim_type in anim_types {
            let Some(chosen) = self.select_iw5_anim_override(base_id, native, anim_type) else {
                continue;
            };
            let Some(slot) = iw5_anim_tree_type_to_iw4_slot(anim_type) else {
                continue;
            };
            if let Some(anim) = if alternate {
                chosen.altmode_anim.clone()
            } else {
                chosen.override_anim.clone()
            } {
                row.sz_xanims[slot] = Some(anim);
            }
            let time = if alternate {
                chosen.alt_time_ms
            } else {
                chosen.anim_time_ms
            };
            if time > 0 {
                if let Some(timer) = iw5_anim_timer(&mut row.facts, slot) {
                    *timer = time;
                }
            }
        }

        for (sound_type, target) in [
            (sz::SND_OVERRIDE_TYPE_FIRE, &mut row.sounds.fire),
            (
                sz::SND_OVERRIDE_TYPE_PLAYER_FIRE,
                &mut row.sounds.fire_player,
            ),
            (
                sz::SND_OVERRIDE_TYPE_PLAYER_AKIMBO,
                &mut row.sounds.fire_player_akimbo,
            ),
            (
                sz::SND_OVERRIDE_TYPE_PLAYER_LASTSHOT,
                &mut row.sounds.fire_last_player,
            ),
        ] {
            if let Some(sound) = self
                .select_iw5_sound_override(base_id, native, sound_type)
                .and_then(|chosen| {
                    if alternate {
                        chosen.altmode_sound.clone()
                    } else {
                        chosen.override_sound.clone()
                    }
                })
            {
                *target = Some(sound);
            }
        }

        for (fx_type, hint) in [
            (1, &mut row.combat_fx.view_flash_hint),
            (2, &mut row.combat_fx.world_flash_hint),
            (3, &mut row.combat_fx.view_shell_eject_hint),
            (4, &mut row.combat_fx.world_shell_eject_hint),
        ] {
            if let Some(fx) = self
                .select_iw5_fx_override(base_id, native, fx_type)
                .and_then(|chosen| {
                    if alternate {
                        chosen.altmode_fx.clone()
                    } else {
                        chosen.override_fx.clone()
                    }
                })
            {
                *hint = Some(fx);
            }
        }

        if let Some(reload) = self.select_iw5_reload_override(base_id, native) {
            row.facts.reload_add_time_ms = reload.reload_add_time_ms;
            row.facts.reload_start_add_time_ms = reload.reload_start_add_time_ms;
        }
        if let Some(notetracks) = self.select_iw5_notetrack_override(base_id, native) {
            row.sounds.notetrack_sound_map = notetracks.sound_map.clone();
        }
        Some(row)
    }
}
