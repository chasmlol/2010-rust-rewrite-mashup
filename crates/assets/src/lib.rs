mod artifact_cache;
mod asset_graph;
pub mod bot_model;
mod glb;
pub mod jak_model;
pub mod minecraft_map;
pub mod minecraft_setup;
pub mod skate_board;
mod gltf_export;
mod iwd;
mod lane;
mod lane_capability;
pub mod loading_screen;
mod map_load_process;
mod map_scripts;
mod match_load;
pub mod plugin;
pub mod prepared;
pub mod session_load;
mod teardown;

pub use artifact_cache::{cache_flight, cache_get, cache_put, fnv1a64, fnv1a64_more};
pub(crate) use asset_graph::stamp_match_destructible_death;
pub use asset_graph::{
    AssetEdge, AssetEdgeCensus, AssetEdgeReason, AssetGraphCensus, DESTRUCTIBLE_DEATH_HINTS,
    DeathClipEdge, DeathHuskEdge, DestructibleDeathHint, DestructibleDeathRow, FpvMeshIndex,
    FpvMeshSpace, FxIndex, FxModelIndex, FxModelSpace, FxSpace, LoadedSoundIndex, LoadedSoundSpace,
    MapXModelIndex, MapXModelSpace, MaterialIndex, MaterialSpace, ProjectileModelIndex,
    ProjectileModelSpace, SoundAliasIndex, SoundAliasSpace, TechniqueSetIndex, TechniqueSetSpace,
    TracerIndex, TracerSpace, WorldWeaponIndex, WorldWeaponSpace, XAnimIndex, XAnimSpace,
    resolve_after_absorb,
};
pub use fastfile_iw4::GlyphCapture;
pub use gltf_export::{GltfExportSummary, export_prepared_world_gltf};
pub use iwd::{IwdSoundIndex, NamespaceSoundIwd, NamespaceTree, NamespaceTrees};
pub use lane::{CommonCensus, LANE_GAPS, LaneGap, LoadedWorld, ZoneLane, lane};
pub use lane_capability::{LaneStatus, PreparedCapability, lane_status};
pub use lighting_iw4::MODEL_LIGHTING_TILE_BYTES;
pub use loading_screen::{LoadingPreviewSource, LoadingScreen};
pub use map_load_process::MapLoadProcess;
pub use match_load::{
    MapLoadApproval, MatchLoadAbort, MatchLoadAccepted, MatchLoadBusy, MatchLoadDispatch,
    MatchLoadRequest, PreparedMatchReady, PreparedMatchSound,
};
pub use minecraft_map::{find_zone_file, list_mp_map_packs, list_mp_maps};
pub use plugin::AssetPlugin;
pub use prepared::{
    MapFacts, MatchMaterials, MatchType10SoundHints, PreparedBodies, PreparedBodyClips,
    PreparedDestructibleDeath, PreparedFpvMeshes, PreparedGaps, PreparedLocalizedStrings,
    PreparedMap, PreparedProjectileMeshes, PreparedWeapons, PreparedWorldWeapons, PreparedXAnims,
    PreparedXModelWalkCensus, SessionCompass,
};
pub use session_load::{
    MatchLoadOutcome, MatchMaterialSeed, PreparedMatch, PreparedWorld, ShellCommon,
    apply_match_material_map, load_match_material_catalog, load_match_material_seed, load_pool,
    load_prepared_match, load_shell_common, load_workers, publish_process_cpus,
};

pub mod image_handles;

mod script_sources;
pub use script_sources::{ScriptSources, ScriptTable};
