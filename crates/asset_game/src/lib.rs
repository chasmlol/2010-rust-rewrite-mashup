use asset_anim::{ClipScheduler, ClipSchedulerError, XAnimCatalog};
use asset_audio::{SoundCatalog, game_nested_string_assignment};
use asset_core::*;
use asset_material::{MaterialCatalog, MaterialDefinitions, decode_ui_image};
use asset_model::{
    FpvMeshCatalog, ModelSkel, ProjectileMeshCatalog, WorldWeaponCatalog, WorldWeaponEntry,
    capture_xmodel_skel,
};
use asset_transport::{
    GamesRoot, Iw5ZoneMemory, T5ZoneMemory, ZoneMemory, find_zone_file, find_zone_for_tree,
    open_zone,
};
use asset_world::decode_rawfile_text;
pub mod arena;
mod attachment_hide;
mod cac_host;
mod cac_presentation;
mod cac_stats;
mod fpv_assembly;
mod fx_catalog;
mod fx_model_catalog;
mod graph_support;
mod impact_fx_catalog;
mod localize;
mod lochit;
mod menu_catalog;
mod menu_source;
mod penetration;
pub mod structured_data;
mod tracer_catalog;
mod weapon_anim_dispatch;
mod weapon_animations;
mod weapon_camo;
mod weapon_catalog;
pub use weapon_camo::WeaponCamouflage;
mod weapon_families;
mod weapon_semantics;
mod weapon_t6;
pub use weapon_t6::{
    MELEE_WEAPON as T6_MELEE_WEAPON, T6_EFFECTS, T6_EQUIPMENT_SOUNDS, capture_t6_string_table,
    planted_model as t6_planted_model,
};

pub use arena::*;
pub use attachment_hide::*;
pub use cac_host::*;
pub use cac_presentation::*;
pub use cac_stats::*;
pub use fpv_assembly::*;
pub use fx_catalog::*;
pub use fx_model_catalog::*;
pub use graph_support::AuthoredRef;
pub use impact_fx_catalog::*;
pub use localize::*;
pub use lochit::*;
pub use menu_catalog::*;
pub use penetration::*;
pub use tracer_catalog::*;
pub use weapon_anim_dispatch::*;
pub use weapon_animations::*;
pub use weapon_catalog::*;
pub use weapon_families::*;
pub use weapon_semantics::*;

pub mod asset_graph {
    pub(crate) use crate::graph_support::*;
}

mod team_t6;
pub use team_t6::t6_team_properties;

mod fpv_family;
pub use fpv_family::{FpvFamilyConnection, NativeFpvConnection, T6WithIw4Hands};
