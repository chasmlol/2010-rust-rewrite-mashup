pub mod combat;
pub mod drawsurf;
pub mod entity_marks;
mod fire_occurrence;
pub mod fire_weapon_fx;
pub mod host;
pub mod model_append;
pub mod model_draw;
mod plugin;
pub mod present;
pub mod product;
pub mod system;
pub mod tracer;

pub use drawsurf::*;
pub use entity_marks::{EntityMarkAttachment, EntityMarkRequest, EntityMarkStore, EntityMarks};
pub use fire_occurrence::{FireFxOccurrence, PresentedFireFx};
pub use fire_weapon_fx::fire_weapon_fx_should_client_trace;
pub use host::{
    CombatFxDump, FxCameraOrigin, FxDumpRequest, FxJournalCursor, FxMarkDvars, FxWorldColorImages,
    HostFxDlights, HostFxPostLights, HostFxSystem, LaserDvars, PreparedFxCatalog,
    PreparedFxElemInfos, PreparedFxModels, PreparedImpactFx, PresentedVehicleFx,
    PresentedVehicleFxRow,
};
pub use model_append::append_fx_model_asset;
pub use model_draw::{FxModelAssetDraw, FxModelDrawPlan, XMODEL_OBJECT_ID_FX_BASE};
pub use plugin::RenderFxPlugin;
pub use product::{
    FxFrameOutcome, FxGeneratedFrame, FxUnavailableCause, clear_fx_owned_plans,
    publish_empty_fx_owned_plans,
};
pub use tracer::{
    PreparedTracers, QueuedBeam, TracerDrawGate, TracerSpawnSkip, TracerWorld, tick_tracer_beams,
    try_spawn_tracer,
};

pub use model_draw::{FxModelStaging, PreparedFxModelGeometry};
