use super::*;

impl WeaponCatalog {
    pub fn into_build(self) -> WeaponBuild {
        let mut build = WeaponBuild::from_catalog(self.entries, self.iw5_attachments);
        build.registry.vehicle_turrets = self.vehicle_turrets;
        build.registry.vehicle_compass = self.vehicle_compass;
        build.registry.vehicle_accel = self.vehicle_accel;
        build
    }
}

impl WeaponBuild {
    pub fn publish(self) -> WeaponRegistry {
        let mut registry = self.registry;
        registry.revision = mint_weapon_revision();
        registry.families = crate::WeaponFamilies::build(&self.family_tables, &registry);
        let t5_knife = registry
            .rows
            .iter()
            .position(|row| {
                row.namespace == crate::AssetNamespace::T5
                    && row.name.strip_suffix("_mp").unwrap_or(&row.name) == "knife"
            })
            .map(|index| index as u32);
        for row in &mut registry.rows {
            row.preparation.set_source(row.namespace, &row.name);
            row.preparation.declare_sound_hints(&row.sounds);
            row.preparation.finish();
            let melee_weapon =
                if row.namespace == crate::AssetNamespace::T5 && !row.facts.use_as_melee {
                    t5_knife.map_or(crate::MeleeWeaponPolicy::Own, |weapon| {
                        crate::MeleeWeaponPolicy::T5KnifeCompatibility { weapon }
                    })
                } else {
                    crate::MeleeWeaponPolicy::Own
                };
            let policy = crate::WeaponSemanticPolicy::compile(
                row.namespace,
                row.facts.thermal_scope,
                row.overlay_material.as_deref(),
                melee_weapon,
            );
            row.facts.thermal_scope = policy.thermal_scope.enabled();
            row.semantics = Some(policy);
        }
        for id in 0..registry.rows.len() as u32 {
            let projection = combat::WeaponCombatProjection::prepare(&registry, id);
            let equipment = equipment::equipment(&registry, id);
            let penetration = equipment::penetration(&registry, id);
            let row = &mut registry.rows[id as usize];
            row.combat = projection;
            row.equipment = equipment;
            row.penetration = penetration;
            row.hud = Some(WeaponHudFacts::prepare(row.facts));
            row.events = Some(WeaponEventFacts::prepare(row.facts));
            row.world = Some(WeaponWorldFacts::prepare(row.facts));
            row.fpv = Some(WeaponFpvFacts::prepare(
                row.facts,
                row.sz_xanim_right_edges[weap_anim::IDLE].is_bound(),
                row.preparation.ads_overlay_convention(),
            ));
        }
        registry.completion_names = configuration::compile_completion_names(&registry);
        registry
    }

    pub(super) fn from_catalog(
        mut entries: Vec<CatalogWeapon>,
        iw5_attachments: HashMap<String, Iw5ScopeRow>,
    ) -> Self {
        let mut gun_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut hand_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut world_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut camo_by_def: HashMap<(u8, u32), WeaponCamoModels> = HashMap::new();
        let mut projectile_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut knife_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut rocket_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut sounds_by_def: HashMap<(u8, u32), WeaponSoundAliases> = HashMap::new();
        let mut combat_fx_by_def: HashMap<(u8, u32), WeaponCombatFx> = HashMap::new();
        let mut combat_slots_by_def: HashMap<(u8, u32), CombatFxSlots> = HashMap::new();
        let mut right_by_def: HashMap<(u8, u32), [Option<String>; WEAPON_ANIM_SLOTS]> =
            HashMap::new();
        let mut left_by_def: HashMap<(u8, u32), [Option<String>; WEAPON_ANIM_SLOTS]> =
            HashMap::new();
        for entry in &entries {
            if let (Some(key), Some(gun)) = (entry.weap_def, entry.gun_xmodel.as_ref()) {
                gun_by_def.entry(key).or_insert_with(|| gun.clone());
            }
            if let (Some(key), Some(hand)) = (entry.weap_def, entry.hand_xmodel.as_ref()) {
                hand_by_def.entry(key).or_insert_with(|| hand.clone());
            }
            if let (Some(key), Some(world)) = (entry.weap_def, entry.world_model.as_ref()) {
                world_by_def.entry(key).or_insert_with(|| world.clone());
            }
            if let Some(key) = entry.weap_def.filter(|_| !entry.camo_models.is_empty()) {
                camo_by_def
                    .entry(key)
                    .or_insert_with(|| entry.camo_models.clone());
            }
            if let (Some(key), Some(proj)) = (entry.weap_def, entry.projectile_model.as_ref()) {
                projectile_by_def.entry(key).or_insert_with(|| proj.clone());
            }
            if let (Some(key), Some(knife)) = (entry.weap_def, entry.knife_xmodel.as_ref()) {
                knife_by_def.entry(key).or_insert_with(|| knife.clone());
            }
            if let (Some(key), Some(rocket)) = (entry.weap_def, entry.rocket_model.as_ref()) {
                rocket_by_def.entry(key).or_insert_with(|| rocket.clone());
            }
            if let Some(key) = entry.weap_def {
                merge_sound_aliases(sounds_by_def.entry(key).or_default(), &entry.sounds);
                merge_combat_fx(
                    combat_fx_by_def
                        .entry(key)
                        .or_insert_with(|| WeaponCombatFx::empty(entry.namespace)),
                    &entry.combat_fx,
                );
                merge_combat_slots(
                    combat_slots_by_def.entry(key).or_default(),
                    &entry.combat_slots,
                );
                if xanims_idle(&entry.sz_xanims_right).is_some() {
                    right_by_def
                        .entry(key)
                        .or_insert_with(|| entry.sz_xanims_right.clone());
                }
                if xanims_idle(&entry.sz_xanims_left).is_some() {
                    left_by_def
                        .entry(key)
                        .or_insert_with(|| entry.sz_xanims_left.clone());
                }
            }
        }
        for entry in &mut entries {
            if entry.gun_xmodel.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.gun_xmodel = gun_by_def.get(&key).cloned();
                }
            }
            if entry.hand_xmodel.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.hand_xmodel = hand_by_def.get(&key).cloned();
                }
            }
            if entry.world_model.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.world_model = world_by_def.get(&key).cloned();
                }
            }
            if entry.camo_models.is_empty()
                && let Some(shared) = entry.weap_def.and_then(|key| camo_by_def.get(&key))
            {
                entry.camo_models = shared.clone();
            }
            if entry.projectile_model.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.projectile_model = projectile_by_def.get(&key).cloned();
                }
            }
            if entry.knife_xmodel.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.knife_xmodel = knife_by_def.get(&key).cloned();
                }
            }
            if entry.rocket_model.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.rocket_model = rocket_by_def.get(&key).cloned();
                }
            }
            if let Some(key) = entry.weap_def {
                if let Some(shared) = sounds_by_def.get(&key) {
                    merge_sound_aliases(&mut entry.sounds, shared);
                }
                if let Some(shared) = combat_fx_by_def.get(&key) {
                    merge_combat_fx(&mut entry.combat_fx, shared);
                }
                if let Some(&shared) = combat_slots_by_def.get(&key) {
                    merge_combat_slots(&mut entry.combat_slots, &shared);
                }
                if xanims_idle(&entry.sz_xanims_right).is_none() {
                    if let Some(shared) = right_by_def.get(&key) {
                        entry.sz_xanims_right = shared.clone();
                    }
                }
                if xanims_idle(&entry.sz_xanims_left).is_none() {
                    if let Some(shared) = left_by_def.get(&key) {
                        entry.sz_xanims_left = shared.clone();
                    }
                }
            }
        }

        let mut by_name: HashMap<String, CatalogWeapon> = HashMap::new();
        for entry in entries {
            let key = normalize_weapon_name(&entry.name);
            match by_name.entry(key) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(entry);
                }
                std::collections::hash_map::Entry::Occupied(mut slot) => {
                    let existing = slot.get_mut();
                    if existing.alternate_weapon.is_none() {
                        existing.alternate_weapon = entry.alternate_weapon;
                    }
                    if existing.gun_xmodel.is_none() {
                        existing.gun_xmodel = entry.gun_xmodel;
                    }
                    if existing.impact_payload.is_none() {
                        existing.impact_payload = entry.impact_payload;
                    }
                    if existing.dual_wield_weapon.is_none() {
                        existing.dual_wield_weapon = entry.dual_wield_weapon;
                    }
                    if existing.hand_xmodel.is_none() {
                        existing.hand_xmodel = entry.hand_xmodel;
                    }
                    if existing.world_model.is_none() {
                        existing.world_model = entry.world_model;
                    }
                    if existing.camo_models.is_empty() {
                        existing.camo_models = entry.camo_models;
                    }
                    if existing.projectile_model.is_none() {
                        existing.projectile_model = entry.projectile_model;
                    }
                    if existing.knife_xmodel.is_none() {
                        existing.knife_xmodel = entry.knife_xmodel;
                    }
                    if existing.rocket_model.is_none() {
                        existing.rocket_model = entry.rocket_model;
                    }
                    if existing.overlay_material.is_none() {
                        existing.overlay_material = entry.overlay_material;
                        existing.overlay_image = entry.overlay_image;
                        existing.overlay_material_slot = entry.overlay_material_slot;
                    }
                    if existing.hud_icon.is_none() {
                        existing.hud_icon = entry.hud_icon;
                        existing.hud_icon_slot = entry.hud_icon_slot;
                        existing.hud_icon_image = entry.hud_icon_image;
                    }
                    if !existing.reticle.center_authored
                        && !existing.reticle.side_authored
                        && (entry.reticle.center_authored || entry.reticle.side_authored)
                    {
                        existing.reticle = entry.reticle;
                    }
                    if existing.kill_icon.is_none() {
                        existing.kill_icon = entry.kill_icon;
                        existing.kill_icon_slot = entry.kill_icon_slot;
                        existing.kill_icon_image = entry.kill_icon_image;
                    }
                    if existing.proj_trail.is_none() {
                        existing.proj_trail = entry.proj_trail;
                        existing.proj_trail_slot = entry.proj_trail_slot;
                        existing.projectile_fx.trail = entry.projectile_fx.trail;
                    }
                    if existing.proj_beacon.is_none() {
                        existing.proj_beacon = entry.proj_beacon;
                        existing.proj_beacon_slot = entry.proj_beacon_slot;
                        existing.projectile_fx.beacon = entry.projectile_fx.beacon;
                    }
                    if existing.proj_ignition.is_none() {
                        existing.proj_ignition = entry.proj_ignition;
                        existing.proj_ignition_slot = entry.proj_ignition_slot;
                        existing.projectile_fx.ignition = entry.projectile_fx.ignition;
                    }
                    merge_sz_xanims(&mut existing.sz_xanims, entry.sz_xanims);
                    merge_sz_xanims(&mut existing.sz_xanims_right, entry.sz_xanims_right);
                    merge_sz_xanims(&mut existing.sz_xanims_left, entry.sz_xanims_left);
                    if existing.hide_tags.is_empty() && !entry.hide_tags.is_empty() {
                        existing.hide_tags = entry.hide_tags;
                    }
                    for (existing_slot, new_slot) in existing
                        .iw5_attachment_slots
                        .iter_mut()
                        .zip(entry.iw5_attachment_slots)
                    {
                        if existing_slot.is_none() {
                            *existing_slot = new_slot;
                        }
                    }
                    if existing.iw5_reload_overrides.is_empty() {
                        existing.iw5_reload_overrides = entry.iw5_reload_overrides;
                    }
                    if existing.iw5_anim_overrides.is_empty() {
                        existing.iw5_anim_overrides = entry.iw5_anim_overrides;
                    }
                    if existing.iw5_fx_overrides.is_empty() {
                        existing.iw5_fx_overrides = entry.iw5_fx_overrides;
                    }
                    if existing.iw5_notetrack_overrides.is_empty() {
                        existing.iw5_notetrack_overrides = entry.iw5_notetrack_overrides;
                    }
                    merge_sound_aliases(&mut existing.sounds, &entry.sounds);
                    if existing.sounds.leftover_sound_overrides.is_empty() {
                        existing.sounds.leftover_sound_overrides =
                            entry.sounds.leftover_sound_overrides.clone();
                    }
                    merge_combat_fx(&mut existing.combat_fx, &entry.combat_fx);
                    merge_combat_slots(&mut existing.combat_slots, &entry.combat_slots);
                    merge_body_facts(&mut existing.facts, entry.facts);
                }
            }
        }
        let mut names: Vec<String> = by_name.keys().cloned().collect();
        names.sort_unstable();

        let mut rows = Vec::with_capacity(names.len() + 1);
        let mut combat_slots = Vec::with_capacity(names.len() + 1);
        let mut index_of = HashMap::with_capacity(names.len());
        rows.push(WeaponRow::default());
        combat_slots.push(CombatFxSlots::default());
        for name in names {
            let entry = by_name.remove(&name).expect("key from map");
            let id = rows.len() as u32;
            index_of.insert(name.clone(), id);
            combat_slots.push(entry.combat_slots);
            rows.push(WeaponRow {
                preparation: WeaponPreparationRecipe::for_capture(entry.namespace, &name),
                name,
                alternate_weapon: entry.alternate_weapon,
                impact_payload: entry.impact_payload,
                alternate_index: 0,
                namespace: entry.namespace,
                facts: entry.facts,
                semantics: None,
                combat: None,
                fpv: None,
                hud: None,
                events: None,
                world: None,
                equipment: None,
                penetration: None,
                gun_xmodel: entry.gun_xmodel,
                hand_xmodel: entry.hand_xmodel,
                dual_wield_weapon: entry.dual_wield_weapon,
                secondary_gun_xmodel: None,
                gun_xmodel_edge: AssetEdge::Absent,
                hand_xmodel_edge: AssetEdge::Absent,
                rocket_model_edge: AssetEdge::Absent,
                attachment_view_model_edges: Vec::new(),
                fpv_hands: [None, None],
                fpv_mount_plan: None,
                fpv_assemblies: [None, None],
                world_model: entry.world_model,
                world_model_edge: AssetEdge::Absent,
                camo_models: entry.camo_models,
                skin_parent: entry.skin_parent,
                material_camos: Arc::default(),
                camo_view_edges: Vec::new(),
                camo_world_edges: Vec::new(),
                attachment_world_model_edges: Vec::new(),
                attachment_world_mounts: Vec::new(),
                projectile_model: entry.projectile_model,
                projectile_model_edge: AssetEdge::Absent,
                rocket_model: entry.rocket_model,
                knife_xmodel: entry.knife_xmodel,
                sz_xanims: entry.sz_xanims,
                sz_xanim_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
                sz_xanim_right_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
                sz_xanim_left_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
                notetrack_actions: HashMap::new(),
                sz_xanims_right: entry.sz_xanims_right,
                sz_xanims_left: entry.sz_xanims_left,
                hide_tags: entry.hide_tags,
                attachment_view_models: {
                    let mut models = entry.attached_models[0].clone();
                    if entry.namespace == crate::AssetNamespace::Iw5
                        && let Some((view, _)) =
                            iw5_default_scope_models(&entry.iw5_attachment_slots, &iw5_attachments)
                    {
                        models.extend(view);
                    }
                    models
                },
                attachment_world_models: {
                    let mut models = entry.attached_models[1].clone();
                    if entry.namespace == crate::AssetNamespace::Iw5
                        && let Some((_, world)) =
                            iw5_default_scope_models(&entry.iw5_attachment_slots, &iw5_attachments)
                    {
                        models.extend(world);
                    }
                    models
                },
                attachment_view_ads_models: Vec::new(),
                t6_clip_models: entry.t6_clip_models,
                t6_attachments: entry.t6_attachments,
                t6_attachment_stats: entry.t6_attachment_stats,
                iw5_configuration: None,
                prepared_attachments: Vec::new(),
                attachment_caption_keys: Vec::new(),
                iw5_attachment_slots: entry.iw5_attachment_slots,
                iw5_reload_overrides: entry.iw5_reload_overrides,
                iw5_anim_overrides: entry.iw5_anim_overrides,
                iw5_fx_overrides: entry.iw5_fx_overrides,
                iw5_notetrack_overrides: entry.iw5_notetrack_overrides,
                sounds: entry.sounds,
                combat_fx: entry.combat_fx,
                reticle: entry.reticle,
                hud_material_edges: entry.hud_material_edges,
                overlay_material_from_slot: entry.overlay_material_slot.is_some(),
                overlay_material: entry.overlay_material,
                overlay_image: entry.overlay_image,
                hud_icon_from_slot: entry.hud_icon_slot.is_some(),
                hud_icon: entry.hud_icon,
                hud_icon_image: entry.hud_icon_image,
                pickup_icon: entry.pickup_icon,
                pickup_icon_image: entry.pickup_icon_image,
                pickup_icon_authored: entry.pickup_icon_slot.is_some(),
                pickup_icon_ratio: entry.pickup_icon_ratio,
                hud_icon_ratio: entry.hud_icon_ratio,
                kill_icon_from_slot: entry.kill_icon_slot.is_some(),
                dpad_icon_image: entry.dpad_icon_image,
                dpad_icon_atlas: entry.dpad_icon_atlas,
                dpad_icon_ratio: entry.dpad_icon_ratio,
                kill_icon: entry.kill_icon,
                kill_icon_image: entry.kill_icon_image,
                projectile_fx: entry.projectile_fx,
                proj_trail_from_slot: entry.proj_trail_slot.is_some(),
                proj_trail: entry.proj_trail,
                proj_beacon_from_slot: entry.proj_beacon_slot.is_some(),
                proj_beacon: entry.proj_beacon,
                proj_ignition_from_slot: entry.proj_ignition_slot.is_some(),
                proj_ignition: entry.proj_ignition,
                display_name_key: entry.display_name_key,
            });
        }
        let mut registry = WeaponRegistry {
            rows,
            world_catalog_identity: 0,
            iw5_attachments,
            configurations: HashMap::new(),
            by_name: index_of,
            by_namespaced: HashMap::new(),
            item_groups: HashMap::new(),
            families: crate::WeaponFamilies::default(),
            completion_names: Vec::new(),
            alternate_fpv: HashMap::new(),
            fpv_clip_tracks: Arc::default(),
            vehicle_turrets: HashMap::new(),
            vehicle_compass: HashMap::new(),
            vehicle_accel: HashMap::new(),
            revision: mint_weapon_revision(),
        };
        registry.rebuild_name_maps();
        Self {
            registry,
            combat_slots,
            family_tables: Vec::new(),
        }
    }
}
