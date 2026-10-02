pub mod anim;
pub mod draw;
mod draw_build;
pub mod gaps;
pub mod lighting;
pub mod occupancy;
mod plugin;

pub use anim::dobj_pose::{
    HostDObjPoseFrame, HostDObjPoseRefuse, PosedPlayer, PosedPlayerFrame, PosedPlayerHead,
};
pub use anim::fpv_host::{
    FpvBoltTargets, FpvGenerateArgs, FpvHeldLife, FpvHeldSettled, FpvPoseKind, FpvPoseProduct,
    FpvPoseRefuse, FpvPosedFrame, FpvPresentCursor, PendingFpvNotetracks, PendingFpvSpawn,
    PendingFpvSpawnRequest, generate_fpv_pose,
};
pub use anim::fpv_prepared::{
    FpvPreparationCensus, FpvRigSet, FpvWeaponSlot, FpvWeaponTable, FpvWeaponView, PreparedFpv,
    prepare_fpv_compositions,
};
pub use anim::model_materials::{PreparedModelMaterials, prepare_model_materials};
pub use anim::viewmodel_controller::{
    AdvanceResult, EventResult, ViewmodelController, ViewmodelEvent, WeaponState,
};
pub use anim::*;
pub use draw::*;
pub use draw_build::*;
pub use lighting::{
    fpv_dobj_lighting_box_half, fpv_dobj_skel_radii, lighting_box_half,
    script_model_lighting_box_half, viewmodel_lighting_origin,
};
pub use occupancy::{
    DynEntCellBits, DynEntPhysClip, DynEntPhysWorld, FpvGeometrySet, FpvPlacementRoot,
    FpvPlacementSet, GunOffset, LocalSpawnArmed, PendingViewHurt, RemoteFxBolts, RemotePlayer,
    RenderFocus, ScriptModelDrawSet, ScriptModelSkinSet, SessionViewmodel, occupy_fpv_scene,
    spawn_pending_fpv, stamp_fpv_placement_matrix, sync_camera_from_presented, tick_fpv_viewmodel,
};
pub use plugin::RenderAnimPlugin;

pub mod clip_triangles;
pub mod jak;
mod jak_pose;
mod minecraft_entities;
mod minecraft_hand;
mod minecraft_inventory;
mod minecraft_mining;
mod minecraft_minimap;
mod minecraft_sounds;
pub mod minecraft_world;
pub mod skate;
