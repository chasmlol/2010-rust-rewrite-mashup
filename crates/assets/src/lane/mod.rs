mod helpers;
mod iw4;
mod iw5;
mod sink;
mod t5;
mod t6;

use std::path::Path;

pub(crate) use helpers::{
    build_iw5_static_model_draw, build_static_model_draw, build_t5_static_model_draw,
};
pub(crate) use sink::{CommonWalkSink, MaterialPopulationSink, ZoneWalkSink};

use crate::{
    lane_capability::{LaneStatus, PreparedCapability},
    session_load::{PreparedWorld, WorldDrawPolicy},
};
use asset_anim::XAnimBuild;
use asset_core::ZoneGame;
use asset_game::WeaponBuild;
use asset_model::{BodyMeshBuild, FpvMeshBuild, WorldWeaponBuild};
use asset_transport::{LoadProgress, ZoneImage};

pub type FilmVisionCatalog = std::collections::BTreeMap<
    String,
    Result<asset_world::FilmVision, asset_world::FilmVisionParseError>,
>;

#[derive(Clone, Debug)]
pub struct LaneGap {
    pub capability: PreparedCapability,
    pub reason: String,
    pub addr: Option<&'static str>,
}

pub struct LoadedWorld {
    pub scripts: crate::ScriptSources,
    pub world: PreparedWorld,
    /// What the map zone itself captured. The local material indices in
    /// `world` are indices into this pool until the match finalizes one.
    pub materials: asset_material::MaterialCatalog,
    pub collision: Option<asset_world::ClipCollision>,
    pub spawns: Vec<asset_world::SpawnPoint>,
    pub bodies: BodyMeshBuild,
    pub fpv_meshes: FpvMeshBuild,
    pub xanims: XAnimBuild,
    pub facts: crate::MapFacts,
    /// Bytes the zone arenas held while the walk read them. The arenas
    /// themselves die with the walk; only their size travels.
    pub arena_bytes: usize,
    pub sound: Option<Result<asset_audio::SoundCatalog, String>>,
    pub report: Vec<String>,
    pub gaps: Vec<LaneGap>,
}

impl LoadedWorld {
    pub fn with_gap(
        policy: WorldDrawPolicy,
        capability: PreparedCapability,
        reason: impl Into<String>,
        addr: Option<&'static str>,
    ) -> Self {
        let reason = reason.into();
        Self {
            world: PreparedWorld {
                policy,
                ..PreparedWorld::empty(policy)
            },
            report: vec![reason.clone()],
            gaps: vec![LaneGap {
                capability,
                reason,
                addr,
            }],
            ..LoadedWorld::empty(policy)
        }
    }

    pub fn push_gap(
        &mut self,
        capability: PreparedCapability,
        reason: impl Into<String>,
        addr: Option<&'static str>,
    ) {
        self.gaps.push(LaneGap {
            capability,
            reason: reason.into(),
            addr,
        });
    }
}

#[derive(Default)]
pub struct CommonCensus {
    pub scripts: crate::ScriptSources,
    pub scene_models: asset_world::MapXModelSceneCatalog,
    pub shared_surfaces: asset_model::SharedXModelSurfaces,
    pub weapons: WeaponBuild,

    pub cac_tables: Vec<asset_game::CapturedStringTable>,
    pub fpv: FpvMeshBuild,
    pub world_weapons: WorldWeaponBuild,

    pub projectile_meshes: asset_model::ProjectileMeshBuild,
    pub xanims: XAnimBuild,
    pub player_anim_sources: asset_anim::PlayerAnimSources,
    pub fx: asset_game::FxCatalog,
    pub fx_models: asset_game::FxModelCatalog,

    pub tracers: asset_game::TracerCatalog,
    pub impact_fx: Option<asset_game::OwnedFxImpactTable>,

    pub material_population: asset_material::MaterialCatalog,

    pub light_defs: Vec<asset_world::CapturedLightDef>,
    pub report: Vec<String>,

    pub pen_table: weapon_iw4::PenetrationDepthTable,
    pub pen_table_loaded: bool,
    pub lochit_table: Option<[f32; weapon_iw4::HITLOC_COUNT]>,

    pub xmodel_walk: crate::PreparedXModelWalkCensus,

    /// Bytes the common_mp arenas held while the walk read them; the arenas
    /// themselves do not outlive it.
    pub s1_common_bytes: usize,

    pub teamsets: std::collections::HashMap<String, asset_game::MapTeamSettings>,
    pub film_visions: FilmVisionCatalog,

    pub pending_images: Option<asset_material::material_images::ImageDemandPlan>,

    pub(crate) preparation: Option<Box<dyn CommonFamilyCompiler>>,
}

pub(crate) struct CommonPreparationProducts {
    pub weapons: WeaponBuild,
    pub materials: asset_material::MaterialCatalog,
    pub fpv: FpvMeshBuild,
    pub world: WorldWeaponBuild,
    pub projectiles: asset_model::ProjectileMeshBuild,
    pub xanims: XAnimBuild,
    pub fx: asset_game::FxCatalog,
    pub report: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CommonDependencyRefusal {
    NativeMaterial {
        family: asset_core::FamilyId,
        name: String,
        reason: String,
    },
    WeaponPreparation(asset_game::WeaponPreparationRefusal),
}

pub(crate) struct CommonPreparationResult {
    pub products: CommonPreparationProducts,
    pub refusals: Vec<CommonDependencyRefusal>,
}

pub(crate) trait CommonFamilyCompiler: Send {
    fn compile(self: Box<Self>, products: CommonPreparationProducts) -> CommonPreparationResult;
}

pub struct MaterialPopulation {
    pub materials: asset_material::MaterialCatalog,
    pub light_defs: Vec<asset_world::CapturedLightDef>,
    pub walked: usize,
    pub report: Vec<String>,
    pub cac_tables: Vec<asset_game::CapturedStringTable>,
    pub scripts: crate::ScriptSources,
}

impl Default for MaterialPopulation {
    fn default() -> Self {
        Self {
            materials: asset_material::MaterialCatalog::default(),
            light_defs: Vec::new(),
            walked: 0,
            report: Vec::new(),
            cac_tables: Vec::new(),
            scripts: crate::ScriptSources::default(),
        }
    }
}

pub trait ZoneLane: Send + Sync {
    fn game(&self) -> ZoneGame;
    fn capabilities(&self) -> &'static [(PreparedCapability, LaneStatus)];

    fn load_world(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,

        shared_surfaces: asset_model::SharedXModelSurfaces,

        material_seed: asset_material::MaterialCatalog,
        common_film_visions: &FilmVisionCatalog,
    ) -> LoadedWorld;

    fn load_common_mp(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        decode_color_maps: bool,
        material_seed: asset_material::MaterialCatalog,
    ) -> CommonCensus;

    fn load_material_population(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        material_seed: asset_material::MaterialCatalog,
    ) -> MaterialPopulation;
}

pub fn lane(game: ZoneGame) -> &'static dyn ZoneLane {
    match game {
        ZoneGame::Iw4 => &iw4::Iw4Lane,
        ZoneGame::T5 => &t5::T5Lane,
        ZoneGame::Iw5 => &iw5::Iw5Lane,
        ZoneGame::T6 => &t6::T6Lane,
    }
}

pub const LANE_GAPS: &[&str] = &[
    "assets::lane::iw4::load_world/zone_header",
    "assets::lane::iw4::load_world/zone_arenas",
    "assets::lane::iw4::load_world/no_gfx_world",
    "assets::lane::iw4::load_world/world_mesh",
    "assets::lane::t5::load_world/zone_header",
    "assets::lane::t5::load_world/zone_arenas",
    "assets::lane::t5::load_world/no_gfx_world",
    "assets::lane::t5::load_world/world_mesh",
    "assets::lane::iw5::load_world/zone_header",
    "assets::lane::iw5::load_world/zone_arenas",
    "assets::lane::iw5::load_world/no_gfx_world",
    "assets::lane::iw5::load_world/world_mesh",
    "assets::lane::t6::load_world/no_decoder",
    "assets::session_load::load_prepared_match/zone_open",
];

impl LoadedWorld {
    pub fn empty(policy: WorldDrawPolicy) -> Self {
        Self {
            scripts: Default::default(),
            world: PreparedWorld::empty(policy),
            materials: Default::default(),
            collision: Default::default(),
            spawns: Default::default(),
            bodies: Default::default(),
            fpv_meshes: Default::default(),
            xanims: Default::default(),
            facts: Default::default(),
            arena_bytes: Default::default(),
            sound: Default::default(),
            report: Default::default(),
            gaps: Default::default(),
        }
    }
}
