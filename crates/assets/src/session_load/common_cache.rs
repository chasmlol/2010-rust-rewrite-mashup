use super::*;

static NEXT_COMMON_PROFILE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommonKey {
    pub(super) runtime: Option<PathBuf>,
    pub(super) foreign: Option<PathBuf>,
}

impl CommonKey {
    pub(super) fn for_match(
        zone_ff: Option<&Path>,
        runtime: Option<&Path>,
        report: &mut Vec<String>,
    ) -> Self {
        Self {
            runtime: runtime.map(Path::to_path_buf),
            foreign: resolve_foreign_material_donor(zone_ff, runtime, report),
        }
    }

    fn shell(games: &asset_transport::GamesRoot, report: &mut Vec<String>) -> Self {
        let runtime = match find_common_mp_for_envelope(games, fastfile_iw4::ZONE_VERSION_PC) {
            Ok(found) => Some(found.path),
            Err(error) => {
                report.push(format!("CAC iw4: {error}"));
                None
            }
        };
        Self {
            runtime,
            foreign: None,
        }
    }
}

impl std::fmt::Display for CommonKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.runtime {
            Some(path) => write!(f, "{}", path.display())?,
            None => f.write_str("<no runtime common_mp>")?,
        }
        if let Some(foreign) = &self.foreign {
            write!(f, " + {}", foreign.display())?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) struct CommonCounts {
    pub(super) startup_count: usize,
    pub(super) t5_mat_count: usize,
    pub(super) t5_reuse_mat: usize,
    pub(super) t5_xanim_n: usize,
    pub(super) foreign_count: usize,
    pub(super) seed_mat: usize,
    pub(super) seed_img: usize,
    pub(super) common_reuse_mat: usize,
    pub(super) common_reuse_img: usize,
}

#[derive(Clone)]
pub(super) struct CommonProducts {
    pub(super) scripts: crate::ScriptSources,
    pub(super) t5_scene_models: asset_world::MapXModelSceneCatalog,
    pub(super) material_seed: MaterialCatalog,
    pub(super) shared_surfaces: asset_model::SharedXModelSurfaces,
    pub(super) scene_models: asset_world::MapXModelSceneCatalog,
    pub(super) light_defs: Vec<asset_world::CapturedLightDef>,
    pub(super) pen_table: weapon_iw4::PenetrationDepthTable,
    pub(super) pen_table_loaded: bool,
    pub(super) lochit_table: Option<[f32; weapon_iw4::HITLOC_COUNT]>,
    pub(super) tracers: asset_game::TracerCatalog,
    pub(super) xmodel_walk: crate::PreparedXModelWalkCensus,
    pub(super) s1_common_bytes: usize,
    pub(super) teamsets: std::collections::HashMap<String, asset_game::MapTeamSettings>,
    pub(super) film_visions: std::collections::BTreeMap<
        String,
        Result<asset_world::FilmVision, asset_world::FilmVisionParseError>,
    >,
    pub(super) weapons: WeaponBuild,
    pub(super) fpv_meshes: FpvMeshBuild,
    pub(super) world_weapons: WorldWeaponBuild,
    pub(super) projectile_meshes: ProjectileMeshBuild,
    pub(super) xanims: XAnimBuild,
    pub(super) player_anim_sources: asset_anim::PlayerAnimSources,
    pub(super) fx: FxCatalog,
    pub(super) fx_models: asset_game::FxModelCatalog,
    pub(super) impact_fx: Option<asset_game::OwnedFxImpactTable>,
    pub(super) t5_xanims: XAnimBuild,
    pub(super) t5_fx: FxCatalog,
    pub(super) t5_impact_fx: Option<asset_game::OwnedFxImpactTable>,
    pub(super) iw5_materials: MaterialCatalog,
    pub(super) iw5_scene_models: asset_world::MapXModelSceneCatalog,
    pub(super) iw5_shared_surfaces: asset_model::SharedXModelSurfaces,
    pub(super) strings: LocalizeCatalog,
    pub(super) counts: CommonCounts,
    pub(super) report: Vec<String>,
    pub(super) localize_report: Vec<String>,
}

pub(super) struct KeptImages {
    pub(super) label: &'static str,
    pub(super) namespace: &'static str,
    pub(super) batch: asset_material::material_images::DecodedImageBatch,
    pub(super) job: load_jobs::Job,
}

pub struct CommonSet {
    pub(super) id: u64,
    pub(super) key: CommonKey,
    pub(super) products: CommonProducts,
    donor_images: async_lock::OnceCell<Vec<KeptImages>>,
    pub(super) fpv_plan: Option<ImageDemandPlan>,
    pub(super) retained: std::sync::Mutex<asset_material::material_images::PayloadRetention>,
    pub(super) cac_tables: Vec<(asset_core::AssetNamespace, asset_game::CapturedStringTable)>,
    pub(super) prepared_ms: f32,
    pub(super) ready_at: std::time::Instant,
}

impl CommonSet {
    pub(super) async fn donor_images(&self) -> &[KeptImages] {
        self.donor_images.wait().await
    }

    pub(super) fn donor_batches(&self) -> usize {
        self.donor_images.get().map_or(0, Vec::len)
    }

    pub(super) fn retain(&self, batch: &asset_material::material_images::DecodedImageBatch) -> u64 {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .keep(batch)
    }

    pub(super) fn retained_bytes(&self) -> u64 {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .bytes()
    }

    pub(super) fn retained_payloads(&self) -> usize {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .payloads()
    }
}

struct CommonFlight {
    pub(super) key: CommonKey,
    set: async_lock::OnceCell<Option<Arc<CommonSet>>>,
}

static COMMON: std::sync::Mutex<Option<Arc<CommonFlight>>> = std::sync::Mutex::new(None);

struct FlightGuard(Option<Arc<CommonFlight>>);

impl Drop for FlightGuard {
    fn drop(&mut self) {
        let Some(flight) = self.0.take() else {
            return;
        };
        let mut slot = COMMON.lock().unwrap_or_else(|poison| poison.into_inner());
        if slot
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &flight))
        {
            *slot = None;
        }
        drop(slot);
        let _ = flight.set.set_blocking(None);
    }
}

pub(super) fn landed_common(key: &CommonKey) -> Option<Arc<CommonSet>> {
    let slot = COMMON.lock().unwrap_or_else(|poison| poison.into_inner());
    let flight = slot.as_ref().filter(|flight| flight.key == *key)?;
    flight.set.get().cloned().flatten()
}

pub(super) async fn ensure_common(key: CommonKey) -> (Arc<CommonSet>, &'static str) {
    let (flight, reach) = {
        let mut slot = COMMON.lock().unwrap_or_else(|poison| poison.into_inner());
        match slot.as_ref() {
            Some(flight) if flight.key == key => {
                let reach = if flight.set.get().is_some() {
                    "reused"
                } else {
                    "joined in flight"
                };
                (Arc::clone(flight), reach)
            }
            _ => {
                let flight = Arc::new(CommonFlight {
                    key: key.clone(),
                    set: async_lock::OnceCell::new(),
                });
                if let Some(previous) = slot.replace(Arc::clone(&flight)) {
                    diag::info!(World, "common set: {} replaced by {key}", previous.key);
                }
                let running = Arc::clone(&flight);
                load_pool()
                    .spawn(async move {
                        let mut guard = FlightGuard(Some(running));
                        let set = prepare_common(key).await;
                        if let Some(flight) = guard.0.take() {
                            let _ = flight.set.set(Some(set)).await;
                        }
                    })
                    .detach();
                (flight, "prepared")
            }
        }
    };
    diag::info!(World, "common set: {reach} for {}", flight.key);
    match flight.set.wait().await {
        Some(set) => (Arc::clone(set), reach),
        None => panic!("common set for {} was not prepared", flight.key),
    }
}

pub struct ShellCommon {
    pub weapons: WeaponRegistry,
    pub strings: LocalizeCatalog,
    pub tables: Vec<(asset_core::AssetNamespace, asset_game::CapturedStringTable)>,
    pub report: Vec<String>,
}

pub async fn load_shell_common(games: asset_transport::GamesRoot) -> ShellCommon {
    let mut report = Vec::new();
    let key = CommonKey::shell(&games, &mut report);
    let (common, reach) = ensure_common(key).await;
    let weapons = common.products.weapons.clone().publish();
    report.push(format!(
        "CAC: {reach} common set {}; weapons={} (iw4={} iw5={} t5={} t6={}) tables={}",
        common.key,
        weapons.len(),
        weapons.namespace_count(asset_core::AssetNamespace::Iw4),
        weapons.namespace_count(asset_core::AssetNamespace::Iw5),
        weapons.namespace_count(asset_core::AssetNamespace::T5),
        weapons.namespace_count(asset_core::AssetNamespace::T6),
        common.cac_tables.len(),
    ));
    {
        let families = weapons.weapon_families();
        let t6 = |family: &&asset_game::WeaponFamily| {
            family.key.namespace == asset_core::AssetNamespace::T6
        };
        let offered: Vec<_> = families.offered().filter(t6).collect();
        let excluded: Vec<_> = families
            .excluded()
            .iter()
            .filter(|(key, _)| key.starts_with("t6:"))
            .collect();
        report.push(format!(
            "CAC t6 families: {} offered ({} primary, {} secondary, {} lethal, {} tactical); {} excluded: {:?}",
            offered.len(),
            offered.iter().filter(|f| f.slot == asset_game::FamilySlot::Primary).count(),
            offered.iter().filter(|f| f.slot == asset_game::FamilySlot::Secondary).count(),
            offered.iter().filter(|f| f.slot == asset_game::FamilySlot::Lethal).count(),
            offered.iter().filter(|f| f.slot == asset_game::FamilySlot::Tactical).count(),
            excluded.len(),
            excluded,
        ));
    }
    for (namespace, table) in &common.cac_tables {
        report.push(format!(
            "CAC {}: {} rows={}",
            namespace.as_str(),
            table.name,
            table.rows
        ));
    }
    ShellCommon {
        weapons,
        strings: common.products.strings.clone(),
        tables: common.cac_tables.clone(),
        report,
    }
}

async fn prepare_common(key: CommonKey) -> Arc<CommonSet> {
    let started = std::time::Instant::now();
    let progress = LoadProgress::default();
    let pool = load_pool();
    let anchor = key.runtime.clone();

    let startup_walk = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        pool.spawn(async move { walk_startup_material_zones(anchor.as_ref(), &progress).await })
    };

    let mut donor_report = Vec::new();
    let material_donor = key.foreign.clone();
    let weapon_donor = resolve_iw5_weapon_donor(anchor.as_deref(), &mut donor_report);
    let iw5_startup_walk = weapon_donor.clone().map(|donor| {
        let progress = progress.clone();
        pool.spawn(async move { walk_iw5_startup(&donor, &progress) })
    });
    let shared_donor = match (&material_donor, &weapon_donor) {
        (Some(material), Some(weapons)) if material == weapons => Some(material.clone()),
        _ => None,
    };
    let (data_common_walk, mut iw5_weapon_walk) = match shared_donor {
        Some(donor) => {
            let progress = progress.clone();
            let job = load_jobs::open(JobKind::ImageDecode).namespace("iw5");
            (
                ForeignCommonWork::Shared(
                    pool.spawn(async move { walk_shared_iw5_common(&donor, &progress, job) }),
                ),
                None,
            )
        }
        None => {
            let material = material_donor.map(|donor| {
                let progress = progress.clone();
                let job = load_jobs::open(JobKind::ImageDecode).namespace("iw5");
                pool.spawn(async move { walk_foreign_material_common(&donor, &progress, job) })
            });
            let weapons = weapon_donor.map(|donor| {
                let progress = progress.clone();
                let job = load_jobs::open(JobKind::ImageDecode).namespace("iw5");
                pool.spawn(async move { walk_iw5_weapon_bundle(&donor, &progress, job) })
            });
            (ForeignCommonWork::Split(material), weapons)
        }
    };

    let common_open = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        pool.spawn(async move {
            let Some(path) = anchor else {
                progress.record_skipped_scoped(StageId::CommonAssets, "common_mp");
                return None;
            };
            let stage = progress.begin_scoped(StageId::CommonAssets, "common_mp", None);
            let opened = open_zone_shared(&path);
            finish_zone_open(stage, &opened);
            Some((path, opened))
        })
    };

    let t5_common_prep = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        pool.spawn(async move { t5_weapon_common_prep(anchor.as_deref(), &progress) })
    };
    let t6_weapon_walk = {
        let progress = progress.clone();
        pool.spawn(async move { walk_t6_weapon_bundle(&progress) })
    };
    let localize_walk = {
        let anchor = anchor.clone();
        let progress = progress.clone();
        pool.spawn(async move {
            let mut report = Vec::new();
            let strings = match &anchor {
                Some(path) => {
                    let stage = progress.begin(StageId::Localization, None);
                    let catalog = load_localized_strings_beside(path, &mut report, &stage);
                    stage.done();
                    catalog
                }
                None => {
                    progress.record_skipped(StageId::Localization);
                    report.push(
                        "localize: no runtime common_mp — every on-screen string is a gap".into(),
                    );
                    LocalizeCatalog::default()
                }
            };
            (strings, report)
        })
    };

    let (material_seed, mut common_report, iw4_stats, startup_light_defs, startup_scripts) =
        startup_walk.await;
    let startup_count = material_seed.materials.len();

    let t5_weapon_walk = {
        let progress = progress.clone();
        let job = load_jobs::open(JobKind::ImageDecode).namespace("t5");
        pool.spawn(async move {
            walk_t5_weapon_common(t5_common_prep.await, &progress, material_seed, job)
        })
    };

    let T5WeaponCommon {
        weapons: t5_weapons,
        fpv: t5_fpv,
        world_guns: t5_world_guns,
        mut material_seed,
        xanims: t5_xanims,
        fx: t5_fx,
        impact_fx: t5_impact_fx,
        projectiles: t5_projectiles,
        teamsets: t5_teamsets,
        scene_models: t5_scene_models,
        images: t5_images,
        stats_tables: t5_stats,
        report: t5_report,
    } = t5_weapon_walk.await;
    let t5_ids = t5_weapons.len();
    let t5_fpv_n = t5_fpv.len();
    let t5_xanim_n = t5_xanims.len();
    let t5_reuse_mat = material_seed.link_reused_materials;
    let t5_mat_count = material_seed
        .materials
        .len()
        .saturating_sub(startup_count)
        .saturating_add(t5_reuse_mat);
    common_report.extend(t5_report);
    let (foreign_materials, mut foreign_report, shared_bundle, foreign_images) =
        match data_common_walk {
            ForeignCommonWork::Shared(task) => {
                let (materials, bundle, images, report) = task.await;
                (materials, report, Some(bundle), images)
            }
            ForeignCommonWork::Split(Some(task)) => {
                let (materials, images, report) = task.await;
                (materials, report, None, images)
            }
            ForeignCommonWork::Split(None) => (MaterialCatalog::default(), Vec::new(), None, None),
        };

    common_report.append(&mut donor_report);
    common_report.append(&mut foreign_report);

    let foreign_count = foreign_materials.materials.len();
    let foreign_linked = material_seed.absorb_asset_population(foreign_materials);
    let mut shared_bundle = shared_bundle;
    if let Some(bundle) = &mut shared_bundle {
        bundle.scene_models.remap_walk_materials(&foreign_linked);
    }

    let mut shared_surfaces = asset_model::SharedXModelSurfaces::default();
    let mut common_scene_models = asset_world::MapXModelSceneCatalog::default();
    let mut common_light_defs = startup_light_defs;
    let mut common_pen_table = weapon_iw4::PenetrationDepthTable::empty();
    let mut common_pen_loaded = false;
    let mut common_lochit_table = None;

    let mut common_tracers = asset_game::TracerCatalog::default();
    let mut xmodel_walk = crate::PreparedXModelWalkCensus::default();
    let mut s1_common_bytes = 0;
    let mut teamsets = t5_teamsets;
    let mut scripts = startup_scripts;
    let mut common_film_visions = std::collections::BTreeMap::new();
    let mut fpv_plan = None;
    let mut iw4_census_stats = Vec::new();

    let common_opened = common_open.await;
    let runtime_namespace = common_opened
        .as_ref()
        .and_then(|(_, image)| image.as_ref().ok())
        .map(|image| image.game);

    let (
        mut weapons,
        mut fpv_meshes,
        mut world_weapons,
        mut projectile_meshes,
        mut xanims,
        mut player_anim_sources,
        mut common_fx,
        common_fx_models,
        common_impact,
        material_seed,
        mut common_walk_report,
    ) = match common_opened {
        Some((path, Ok(image))) => {
            let mut census =
                lane(image.game).load_common_mp(&path, &image, &progress, true, material_seed);
            fpv_plan = census.pending_images.take().filter(|plan| !plan.is_empty());
            shared_surfaces = census.shared_surfaces;
            common_scene_models = census.scene_models;
            common_light_defs.extend(census.light_defs);
            common_pen_table = census.pen_table;
            common_pen_loaded = census.pen_table_loaded;
            common_lochit_table = census.lochit_table;
            common_tracers = census.tracers;
            xmodel_walk = census.xmodel_walk;
            s1_common_bytes = census.s1_common_bytes;
            teamsets.extend(census.teamsets);
            common_film_visions = census.film_visions;
            // patch_mp loads after common_mp, so its scripts win.
            scripts = {
                let mut ordered = census.scripts;
                ordered.overlay(std::mem::take(&mut scripts));
                ordered
            };
            iw4_census_stats = census.cac_tables;
            (
                census.weapons,
                census.fpv,
                census.world_weapons,
                census.projectile_meshes,
                census.xanims,
                census.player_anim_sources,
                census.fx,
                census.fx_models,
                census.impact_fx,
                census.material_population,
                census.report,
            )
        }
        Some((_, Err(error))) => (
            asset_game::WeaponBuild::default(),
            FpvMeshBuild::default(),
            WorldWeaponBuild::default(),
            ProjectileMeshBuild::default(),
            XAnimBuild::default(),
            asset_anim::PlayerAnimSources::default(),
            asset_game::FxCatalog::default(),
            asset_game::FxModelCatalog::default(),
            None,
            material_seed,
            vec![format!("common_mp models: open zone: {error}")],
        ),
        None => (
            asset_game::WeaponBuild::default(),
            FpvMeshBuild::default(),
            WorldWeaponBuild::default(),
            ProjectileMeshBuild::default(),
            XAnimBuild::default(),
            asset_anim::PlayerAnimSources::default(),
            asset_game::FxCatalog::default(),
            asset_game::FxModelCatalog::default(),
            None,
            material_seed,
            Vec::new(),
        ),
    };
    common_report.append(&mut common_walk_report);
    weapons.apply_stats_tables(&iw4_stats);

    let common_reuse_mat = material_seed.link_reused_materials;
    let common_reuse_img = material_seed.link_reused_images;
    let seed_mat = material_seed.materials.len();
    let seed_img = material_seed.images.len();

    player_anim_sources.compile();
    common_report.push(player_anim_sources.compile_report_line());
    common_report.push(player_anim_sources.parse_report_line());

    let (bundle, bundle_images, mut iw5_report) = match (iw5_weapon_walk.take(), shared_bundle) {
        (Some(task), _) => task.await,
        (None, Some(bundle)) => (bundle, None, Vec::new()),
        (None, None) => (Iw5WeaponBundle::default(), None, Vec::new()),
    };
    let Iw5WeaponBundle {
        weapons: mut iw5_weapons,
        fpv: iw5_fpv,
        world_guns: iw5_world_guns,
        xanims: iw5_xanims,
        materials: iw5_materials,
        stats_tables: iw5_census_stats,
        scene_models: iw5_scene_models,
        shared_surfaces: iw5_shared_surfaces,
    } = bundle;
    let mut material_seed = material_seed;
    let iw5_stats = match iw5_startup_walk {
        Some(task) => {
            let population = task.await;
            common_report.extend(population.report);
            common_light_defs.extend(population.light_defs);
            material_seed.absorb_asset_population(population.materials);
            population.cac_tables
        }
        None => Vec::new(),
    };
    iw5_weapons.apply_stats_tables(&iw5_stats);
    let iw4_weapon_n = weapons.len();
    let fpv_common_n = fpv_meshes.len();
    let absorbed = iw5_weapons.len();
    weapons.absorb(iw5_weapons);
    let iw5_fpv_added = fpv_meshes.absorb(iw5_fpv);
    let world_gun_common_n = world_weapons.len();
    let iw5_world_added = world_weapons.absorb(iw5_world_guns);
    let xanim_before_iw5 = xanims.len();
    let iw5_xanim_added = xanims.absorb(iw5_xanims);
    let iw5_mat_n = iw5_materials.materials.len();
    common_report.append(&mut iw5_report);
    common_report.push(format!(
        "iw5 weapon bundle: donor={} absorbed={absorbed} catalog {iw4_weapon_n}→{} fpv {fpv_common_n}→{} (+{iw5_fpv_added} iw5 keys) world guns {world_gun_common_n}→{} (+{iw5_world_added} iw5 keys) xanims {xanim_before_iw5}→{} (+{iw5_xanim_added} iw5 keys) materials={iw5_mat_n} (absorbed after IW4 pool)",
        absorbed,
        weapons.len(),
        fpv_meshes.len(),
        world_weapons.len(),
        xanims.len(),
    ));
    let mut t5_weapons = t5_weapons;
    let (t5_code_stats, t5_census_stats) = t5_stats;
    t5_weapons.apply_stats_tables(&t5_code_stats);
    let t5_absorbed = t5_weapons.len();
    weapons.absorb(t5_weapons);
    projectile_meshes.absorb(t5_projectiles);
    let t5_fpv_added = fpv_meshes.absorb(t5_fpv);
    let t5_world_added = world_weapons.absorb(t5_world_guns);
    common_report.push(format!(
        "t5 weapon absorb: donor={t5_ids} unique={t5_absorbed} fpv=+{t5_fpv_added}/{t5_fpv_n} world_guns=+{t5_world_added}; registry now {} (t5={})",
        weapons.len(),
        weapons.namespace_count(asset_core::AssetNamespace::T5)
    ));
    let (t6_weapons, preparation, t6_tables, t6_report) = t6_weapon_walk.await;
    common_report.extend(t6_report);
    weapons.absorb(t6_weapons);
    if let Some(compiler) = preparation {
        let prepared = compiler.compile(crate::lane::CommonPreparationProducts {
            weapons,
            materials: material_seed,
            fpv: fpv_meshes,
            world: world_weapons,
            projectiles: projectile_meshes,
            xanims,
            fx: common_fx,
            report: Vec::new(),
        });
        common_report.extend(
            prepared
                .refusals
                .iter()
                .map(|cause| format!("common family preparation refused: {cause:?}")),
        );
        let products = prepared.products;
        common_report.extend(products.report);
        (
            weapons,
            material_seed,
            fpv_meshes,
            world_weapons,
            projectile_meshes,
            xanims,
            common_fx,
        ) = (
            products.weapons,
            products.materials,
            products.fpv,
            products.world,
            products.projectiles,
            products.xanims,
            products.fx,
        );
    }
    common_report.push(format!(
        "FPV generation: common={fpv_common_n} iw5_keys={iw5_fpv_added} t5={t5_fpv_n} t5_keys={t5_fpv_added} collide={} merged={}",
        fpv_meshes.collide_name_count(),
        fpv_meshes.len()
    ));

    let cac_tables: Vec<(asset_core::AssetNamespace, asset_game::CapturedStringTable)> = [
        (runtime_namespace, iw4_stats, iw4_census_stats),
        (
            Some(asset_core::AssetNamespace::Iw5),
            iw5_stats,
            iw5_census_stats,
        ),
        (
            Some(asset_core::AssetNamespace::T5),
            t5_code_stats,
            t5_census_stats,
        ),
        (Some(asset_core::AssetNamespace::T6), t6_tables, Vec::new()),
    ]
    .into_iter()
    .flat_map(|(namespace, code, common)| {
        code.into_iter()
            .chain(common)
            .filter_map(move |table| Some((namespace?, table)))
    })
    .collect();

    let mut camouflage_images = None;
    if let (Some((_, options)), Some((_, choices))) = (
        cac_tables.iter().find(|(ns, t)| {
            *ns == asset_core::AssetNamespace::T5
                && t.name.eq_ignore_ascii_case("mp/weaponoptions.csv")
        }),
        cac_tables.iter().find(|(ns, t)| {
            *ns == asset_core::AssetNamespace::T5
                && t.name.eq_ignore_ascii_case("mp/attachmentTable.csv")
        }),
    ) {
        let dressed = weapons.prepare_t5_camouflages(
            options,
            choices,
            &mut material_seed,
            &fpv_meshes,
            &world_weapons,
        );
        common_report.push(format!(
            "T5 camouflage: {dressed} weapon configurations prepared"
        ));
        let keys: Vec<_> = (1..=weapons.len() as u32)
            .flat_map(|id| {
                weapons
                    .material_camouflages_of(id)
                    .iter()
                    .flat_map(|camo| camo.materials.iter().map(|(_, to)| to.clone()))
            })
            .collect();
        if let Ok(zone) = games_root_from_env()
            .and_then(|root| find_common_mp_for_envelope(&root, fastfile_t5::ZONE_VERSION_PC))
        {
            let stage = progress.begin_scoped(StageId::Images, "T5 camouflage", None);
            let job = load_jobs::open(JobKind::ImageDecode).namespace("t5");
            let plan = asset_material::material_images::plan_material_images_for_keys(
                &zone.path,
                &mut material_seed,
                keys,
                &stage,
                load_pool(),
            );
            stage.done();
            camouflage_images = hold_image_plan("T5 camouflage", Some(plan), job)
                .map(|held| held.enqueue(&progress));
        }
    }

    weapons.set_family_tables(cac_tables.clone());
    let iw5_prepared = weapons.prepare_iw5_configurations();
    let t6_prepared = weapons.prepare_t6_configurations();
    common_report.push(format!(
        "T6 configurations: prepared={} refused={}",
        t6_prepared.prepared, t6_prepared.refused
    ));
    weapons.resolve_fpv_mesh_edges(&fpv_meshes);
    weapons.resolve_fpv_hands(&fpv_meshes, &asset_model::BodyMeshCatalog::default());
    weapons.resolve_world_model_edges(&world_weapons);

    common_report.push(format!(
        "IW5 configurations: prepared={} refused={} {:?}",
        iw5_prepared.prepared,
        iw5_prepared.refused.len(),
        iw5_prepared
            .refused
            .iter()
            .map(|selection| format!(
                "{} [{}]",
                selection
                    .family
                    .as_ref()
                    .map(|key| key.to_string())
                    .unwrap_or_default(),
                selection.attachments.join(" ")
            ))
            .collect::<Vec<_>>()
    ));
    let (strings, localize_report) = localize_walk.await;
    common_report.extend(progress.timing_report());
    let prepared_ms = started.elapsed().as_secs_f32() * 1000.0;
    diag::info!(
        World,
        "common set: {key} prepared in {prepared_ms:.0}ms; donor images still decoding"
    );

    material_seed.mark_images_common_owned();
    let mut iw5_materials = iw5_materials;
    iw5_materials.mark_images_common_owned();
    let set = Arc::new(CommonSet {
        id: NEXT_COMMON_PROFILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        key,
        products: CommonProducts {
            scripts,
            t5_scene_models,
            material_seed,
            shared_surfaces,
            scene_models: common_scene_models,
            light_defs: common_light_defs,
            pen_table: common_pen_table,
            pen_table_loaded: common_pen_loaded,
            lochit_table: common_lochit_table,
            tracers: common_tracers,
            xmodel_walk,
            s1_common_bytes,
            teamsets,
            film_visions: common_film_visions,
            weapons,
            fpv_meshes,
            world_weapons,
            projectile_meshes,
            xanims,
            player_anim_sources,
            fx: common_fx,
            fx_models: common_fx_models,
            impact_fx: common_impact,
            t5_xanims,
            t5_fx,
            t5_impact_fx,
            iw5_materials,
            iw5_scene_models,
            iw5_shared_surfaces,
            strings,
            counts: CommonCounts {
                startup_count,
                t5_mat_count,
                t5_reuse_mat,
                t5_xanim_n,
                foreign_count,
                seed_mat,
                seed_img,
                common_reuse_mat,
                common_reuse_img,
            },
            report: common_report,
            localize_report,
        },
        donor_images: async_lock::OnceCell::new(),
        fpv_plan,
        retained: std::sync::Mutex::new(Default::default()),
        cac_tables,
        prepared_ms,
        ready_at: std::time::Instant::now(),
    });

    let keeping = Arc::clone(&set);
    load_pool()
        .spawn(async move {
            let mut kept = Vec::new();
            for (namespace, pending) in [
                ("t5", t5_images),
                ("t5", camouflage_images),
                ("iw5", foreign_images),
                ("iw5", bundle_images),
            ] {
                let Some(pending) = pending else {
                    continue;
                };
                let job = pending.job;
                let (label, batch) = pending.join().await;
                let batch = batch.into_kept();
                keeping.retain(&batch);
                kept.push(KeptImages {
                    label,
                    namespace,
                    batch,
                    job,
                });
            }
            diag::info!(
                World,
                "common set: {} donor image batches kept, {} payloads ({:.1}MiB)",
                kept.len(),
                keeping.retained_payloads(),
                keeping.retained_bytes() as f64 / (1024.0 * 1024.0),
            );
            let _ = keeping.donor_images.set(kept).await;
        })
        .detach();
    set
}

fn walk_iw5_startup(donor: &Path, progress: &LoadProgress) -> crate::lane::MaterialPopulation {
    let found = match find_zone_for_tree(donor, "code_post_gfx_mp") {
        Ok(found) => found,
        Err(error) => {
            return crate::lane::MaterialPopulation {
                report: vec![format!("IW5 startup code_post_gfx_mp: {error}")],
                ..Default::default()
            };
        }
    };
    let mut population = walk_population_file(&found.path, progress, MaterialCatalog::default());
    population.report.push(format!(
        "IW5 startup: {} tables={} materials={} images={} light_defs={}",
        found.path.display(),
        population.cac_tables.len(),
        population.materials.materials.len(),
        population.materials.images.len(),
        population.light_defs.len()
    ));
    population
}
