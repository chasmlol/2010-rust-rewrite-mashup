mod admission;
pub mod readiness;
pub use net::LocalAccount;
pub use readiness::SessionReadinessPolicy;
pub mod combat_table;
pub mod content_manifest;
pub mod life_front;
pub mod lifecycle;
pub mod loadout;
pub mod local_arm;
pub mod match_apply;
pub mod plugin;
pub mod startup;
pub mod view_subject;

pub use content_manifest::{
    AuthorityWeaponProfile, ManifestFact, ManifestGap, RuntimeRuleset, SessionContentManifest,
    SessionManifestError, SessionWeaponId, SessionWeaponManifestRow,
};

pub use life_front::{LifeFrontCensus, LifeNotifyCensus};
pub use lifecycle::{
    LiveWorldIdentity, SessionSwapCompletion, SessionSwapRequest, SessionSwapResult,
    SessionSwapTarget, TeardownGaps, TeardownRequest,
};
pub use loadout::{
    AuthoritativeClassProjection, ClassRow, authoritative_class_lock_reason, perk_catalog_id,
    project_class, resolve_class_weapon,
};
pub use local_arm::{
    arm_local_from_presented, join_local_on_class_select, sync_prediction_metrics_to_probe,
};
pub use match_apply::{apply_prepared_match, install_script_model_id};
pub use plugin::SessionPlugin;
pub use startup::{PendingConsoleLines, StartupCommands};
