use super::*;

impl WeaponBuild {
    pub fn resolve_fpv_mesh_edges(&mut self, fpv: &crate::FpvMeshCatalog) {
        for row in &mut self.registry.rows {
            row.gun_xmodel_edge = row.preparation.bind_fpv(
                WeaponComponent::ViewModel,
                row.gun_xmodel.as_deref(),
                fpv,
            );
            row.camo_view_edges = row
                .camo_models
                .view
                .iter()
                .map(|(slot, name)| {
                    (
                        *slot,
                        row.preparation
                            .bind_fpv(WeaponComponent::ViewModel, Some(name), fpv),
                    )
                })
                .collect();
            row.hand_xmodel_edge =
                row.preparation
                    .bind_fpv(WeaponComponent::Hands, row.hand_xmodel.as_deref(), fpv);
            row.rocket_model_edge = row.preparation.bind_fpv(
                WeaponComponent::ViewModel,
                row.rocket_model.as_deref(),
                fpv,
            );
            row.attachment_view_model_edges = row
                .attachment_view_models
                .iter()
                .map(|name| {
                    row.preparation
                        .bind_fpv(WeaponComponent::ViewModel, Some(name), fpv)
                })
                .collect();
            if !row.gun_xmodel_edge.is_bound() {
                row.fpv_hands = [None, None];
                row.fpv_assemblies = [None, None];
            }
            row.fpv_mount_plan = row.gun_xmodel_edge.bound_index().and_then(|gun| {
                let attachments: Option<Vec<_>> = row
                    .attachment_view_model_edges
                    .iter()
                    .map(|edge| edge.bound_index().map(crate::FpvMeshIndex::from_order))
                    .collect();
                let rocket = if row.rocket_model_edge.is_absent() {
                    Some(None)
                } else {
                    row.rocket_model_edge
                        .bound_index()
                        .map(|index| Some(crate::FpvMeshIndex::from_order(index)))
                }?;
                let mut plan = asset_model::plan_fpv_mounts(
                    fpv,
                    crate::FpvMeshIndex::from_order(gun),
                    &attachments?,
                    rocket,
                    row.preparation.attachment_mount_on_root(),
                );
                if let Ok(plan) = &mut plan {
                    plan.ads_swaps = row
                        .attachment_view_ads_models
                        .iter()
                        .filter_map(|(model, ads)| {
                            let at = row.attachment_view_models.iter().position(|m| m == model)?;
                            let ads = row
                                .preparation
                                .bind_fpv(WeaponComponent::ViewModel, Some(ads), fpv)
                                .bound_index()?;
                            Some((at, crate::FpvMeshIndex::from_order(ads)))
                        })
                        .collect();
                    if let Some(name) = row.secondary_gun_xmodel.as_deref() {
                        let model = row
                            .preparation
                            .bind_fpv(WeaponComponent::ViewModel, Some(name), fpv)
                            .bound_index();
                        match model {
                            Some(model) => {
                                plan.secondary_gun = Some(crate::FpvMeshIndex::from_order(model))
                            }
                            None => {
                                return Some(Err(asset_model::FpvMountError {
                                    model: name.to_owned(),
                                    detail: "secondary gun missing from FPV catalog",
                                }));
                            }
                        }
                    }
                }
                Some(plan)
            });
        }
    }

    pub fn resolve_fpv_hands(
        &mut self,
        fpv: &crate::FpvMeshCatalog,
        bodies: &asset_model::BodyMeshCatalog,
    ) {
        for row in &mut self.registry.rows {
            let Some(hand_ns) = row.component_namespace(WeaponComponent::Hands) else {
                row.fpv_hands = [None, None];
                continue;
            };
            let Some(map_ns) = fpv.map_namespace else {
                row.fpv_hands = [None, None];
                continue;
            };
            let hand_name = row
                .hand_xmodel_edge
                .bound_index()
                .and_then(|index| fpv.get_at(index))
                .map(|entry| entry.skel.name.as_str());
            row.fpv_hands = std::array::from_fn(|side| {
                let kit = bodies.kits().kit(side == 1)?;
                let body = bodies.get(&kit.body)?;
                if body.namespace != map_ns {
                    return None;
                }
                let kit = Some(kit);
                let mut choice =
                    asset_model::FpvHands::resolve(fpv, map_ns, kit, hand_name, hand_ns);
                if row.secondary_gun_xmodel.is_some()
                    && fpv.get_hands(&choice).is_some_and(|entry| {
                        !entry
                            .skel
                            .bone_names
                            .iter()
                            .any(|bone| bone == "tag_weapon1")
                    })
                {
                    choice = asset_model::FpvHands::game_default(map_ns);
                }
                let (ns, name) = choice.key()?;
                if ns != body.namespace {
                    return None;
                }
                let index = fpv.index_by_name(ns, name)?;
                let skel = &fpv.get_at(index)?.skel;
                if skel.pose.is_none() || !skel.bone_names.iter().any(|bone| bone == "tag_weapon") {
                    return None;
                }
                Some((choice, crate::FpvMeshIndex::from_order(index)))
            });
        }
    }

    pub fn resolve_fpv_assemblies(
        &mut self,
        fpv: &crate::FpvMeshCatalog,
        xanims: &crate::XAnimCatalog,
    ) -> FpvAssemblyCensus {
        let mut shared: HashMap<crate::FpvAssemblyKey, Result<Arc<crate::FpvAssembly>, String>> =
            HashMap::new();
        let mut skeletons = crate::FpvSkeletons::default();
        let mut tracks = crate::FpvClipTracks::default();
        let mut census = FpvAssemblyCensus::default();
        self.registry.alternate_fpv.clear();
        let mut variants = Vec::new();
        for parent in 1..self.registry.rows.len() as u32 {
            let alternate = self.registry.alternate_of(parent);
            if alternate != 0
                && self
                    .registry
                    .facts_of(alternate)
                    .is_some_and(|f| f.inventory_type == 3)
            {
                variants.push((alternate, parent));
            }
        }
        let preparations: Vec<_> = (0..self.registry.rows.len() as u32)
            .map(|id| (id, 0))
            .chain(variants)
            .map(|(id, parent)| {
                (
                    id,
                    parent,
                    crate::effective_hide_tags(
                        &self.registry,
                        if parent == 0 { id } else { parent },
                    ),
                )
            })
            .collect();
        for (id, parent, hide_tags) in preparations {
            let row = &mut self.registry.rows[id as usize];
            if parent == 0 {
                row.fpv_assemblies = [None, None];
            }
            let Some(Ok(mounts)) = &row.fpv_mount_plan else {
                continue;
            };
            let clips: std::collections::BTreeSet<usize> = row
                .sz_xanim_edges
                .iter()
                .chain(&row.sz_xanim_right_edges)
                .chain(&row.sz_xanim_left_edges)
                .filter_map(|edge| edge.bound_index())
                .collect();
            let hide_mode = row.preparation.hide_mode();
            let knife_model = row.knife_xmodel.as_deref().map(|name| {
                row.preparation
                    .bind_fpv(WeaponComponent::ViewModel, Some(name), fpv)
                    .bound_index()
                    .map(crate::FpvMeshIndex::from_order)
                    .ok_or_else(|| format!("knife model `{name}` missing from FPV catalog"))
            });
            let mut assemble = |hands: crate::FpvMeshIndex,
                                mounts: &asset_model::FpvMountPlan,
                                rocket: bool,
                                knife: Option<crate::FpvMeshIndex>,
                                ads: bool,
                                jammed: bool| {
                let key = crate::FpvAssemblyKey {
                    hands,
                    gun: mounts.gun,
                    secondary_gun: mounts.secondary_gun,
                    attachments: mounts.attachment_models(ads).collect(),
                    rocket: rocket
                        .then(|| mounts.rocket.as_ref().map(|mount| mount.model))
                        .flatten(),
                    knife,
                    hide_tags: hide_tags.clone(),
                    hide_mode,
                    jammed,
                };
                shared
                    .entry(key)
                    .or_insert_with(|| {
                        census.built += 1;
                        crate::FpvAssembly::build(
                            fpv,
                            hands,
                            mounts,
                            rocket,
                            knife,
                            ads,
                            &hide_tags,
                            hide_mode,
                            jammed,
                            &mut skeletons,
                        )
                        .map(Arc::new)
                        .map_err(|error| error.to_string())
                    })
                    .clone()
            };
            let sides: [Option<Result<crate::FpvSideAssemblies, String>>; 2] =
                std::array::from_fn(|side| {
                    let (_, hands) = row.fpv_hands[side].as_ref()?;
                    let bare = match assemble(*hands, mounts, false, None, false, false) {
                        Ok(bare) => bare,
                        Err(error) => return Some(Err(error)),
                    };
                    let rocket = match mounts
                        .rocket
                        .is_some()
                        .then(|| assemble(*hands, mounts, true, None, false, false))
                    {
                        None => None,
                        Some(Ok(rocket)) => Some(rocket),
                        Some(Err(error)) => return Some(Err(error)),
                    };
                    let melee = match &knife_model {
                        None => None,
                        Some(Err(error)) => return Some(Err(error.clone())),
                        Some(Ok(knife)) => {
                            match assemble(*hands, mounts, false, Some(*knife), false, false) {
                                Ok(melee) => Some(melee),
                                Err(error) => return Some(Err(error)),
                            }
                        }
                    };
                    let ads = match (!mounts.ads_swaps.is_empty())
                        .then(|| assemble(*hands, mounts, false, None, true, false))
                    {
                        None => None,
                        Some(Ok(ads)) => Some(ads),
                        Some(Err(error)) => return Some(Err(error)),
                    };
                    let jammed = match assemble(*hands, mounts, false, None, false, true) {
                        Ok(jammed) => jammed,
                        Err(error) => return Some(Err(error)),
                    };
                    let jammed = jammed
                        .parts
                        .iter()
                        .zip(&bare.parts)
                        .any(|(jammed, bare)| jammed.hide != bare.hide)
                        .then_some(jammed);
                    Some(Ok(crate::FpvSideAssemblies {
                        bare,
                        rocket,
                        melee,
                        ads,
                        jammed,
                    }))
                });
            for side in sides.iter().flatten().flatten() {
                for assembly in std::iter::once(&side.bare)
                    .chain(&side.rocket)
                    .chain(&side.melee)
                    .chain(&side.ads)
                    .chain(&side.jammed)
                {
                    for &clip_index in &clips {
                        let Some(clip) = xanims.clip_at(clip_index) else {
                            continue;
                        };
                        for part in &assembly.parts {
                            tracks.bind(fpv, clip_index, Arc::clone(&clip), part.model);
                        }
                    }
                }
            }
            for side in sides.iter().flatten() {
                match side {
                    Ok(_) => census.linked += 1,
                    Err(_) => census.refused += 1,
                }
            }
            if parent == 0 {
                row.fpv_assemblies = sides;
            } else {
                self.registry.alternate_fpv.insert((id, parent), sides);
            }
        }
        census.clip_tables = tracks.len();
        self.registry.fpv_clip_tracks = Arc::new(tracks);
        census
    }

    pub fn resolve_world_model_edges(&mut self, catalog: &crate::WorldWeaponCatalog) {
        self.registry.world_catalog_identity = catalog.identity();
        for row in &mut self.registry.rows {
            row.world_model_edge = row
                .preparation
                .bind_world(row.world_model.as_deref(), catalog);
            row.camo_world_edges = row
                .camo_models
                .world
                .iter()
                .map(|(slot, name)| (*slot, row.preparation.bind_world(Some(name), catalog)))
                .collect();
            row.attachment_world_model_edges = row
                .attachment_world_models
                .iter()
                .map(|name| row.preparation.bind_world(Some(name), catalog))
                .collect();
            let gun = row
                .world_model_edge
                .bound_index()
                .and_then(|index| catalog.get_at(index));
            let on_gun_root = row.preparation.attachment_mount_on_root();
            row.attachment_world_mounts = row
                .attachment_world_model_edges
                .iter()
                .map(|edge| {
                    if on_gun_root {
                        let attachment = catalog.get_at(edge.bound_index()?)?;
                        let bones = &gun?.skel.bone_names;
                        let tag = attachment.skel.mount_tag.as_deref().and_then(|tag| {
                            bones.iter().find(|bone| bone.eq_ignore_ascii_case(tag))
                        });
                        return tag.or(bones.first()).cloned();
                    }
                    let root = catalog
                        .get_at(edge.bound_index()?)?
                        .skel
                        .bone_names
                        .first()?;
                    gun?.skel
                        .bone_names
                        .iter()
                        .find(|bone| bone.eq_ignore_ascii_case(root))
                        .cloned()
                })
                .collect();
        }
    }
}
