pub mod adopt;
pub mod bullet;
mod shield;
pub use shield::{ShieldAttachment, ShieldCarrierCollision};
pub mod bullet_collision;
mod carrier;
pub mod collision_census;
pub mod combat;
pub mod content;
mod corpse;
mod damage;
mod entity_run;
mod equipment;
mod frame;
mod gentity;
pub mod hudelem;
pub mod identities;
pub mod input;
mod item;
mod local_profile;
pub use item::ITEM_USE_HOLD_MS;
pub use local_profile::LocalPlayerProfile;
mod mantle_xanim;
pub mod voxel;
pub mod match_state;
mod missile;
mod missile_guidance;
pub use missile_guidance::{MissileGuide, MissileTarget};
mod penetration;
mod persistent_data;
mod persistent_defaults;
pub use persistent_data::{
    AccountId, AccountSnapshot, PLAYER_DATA_BUFFER_BYTES, PersistentDataError, PersistentDataStore,
};
pub use persistent_defaults::PlayerDataDefaults;
mod presence;
mod remote_missile;
pub mod script;
mod weapon_lock;
pub use weapon_lock::WeaponLock;
pub mod player_anim_script;
pub mod rules;
mod score;
mod skill_rating;
pub use skill_rating::{SKILL_RATING_BYTES, SkillRating, SkillRatingError, SkillRatings};
pub mod script_gaps;
mod script_player;
mod smodel_grid;
mod snapshot;
mod sound_alias_cs;
pub mod spawn;
mod step;
pub mod t5_destructible;
mod world;
pub mod world_objects;

pub use adopt::{ADOPT_GAP_COUNT, ADOPT_GAPS, AdoptGap, AdoptReport};
pub use bullet_collision::{
    AuthorityDObjCollision, AuthorityDObjCollisionBone, AuthorityDObjState, AuthorityModelOwner,
    BulletHitKind, BulletPath, BulletTraceQuery, BulletTraceSegment, COLLISION_COVERAGE,
    COLLISION_HISTORY_TICKS, CONTENTS_SOLID, ColliderId, CollisionCoverageRow, CollisionHistory,
    CoverageSupport, CurrentAuthorityReason, EntityCollisionCapabilities, EntityCollisionEpoch,
    EntityCollisionFrame, EntityCollisionHistory, EntityCollisionSample, EntityCollisionTraceGeom,
    HistoryClampReason, HistoryFrame, HistoryPhase, HistoryRefusalReason, HistorySample,
    HistorySampleVerdict, HitVolumeKind, LAGCOMP_MAX_REWIND_TICKS, LagcompQuery,
    LinkedBrushCollisionBrush, MASK_BULLET_WORLD, MASK_PLAYER_SOLID, MASK_SHOT, PLAYER_MAXS,
    PLAYER_MINS, PlayerCollisionPose, ScriptModelPlayAnim, ShotSampleProvenance, ShotSampleQuality,
    TraceInvalidReason, TraceOutcome, bullet_trace, bullet_trace_segments,
    bullet_trace_segments_with_entity_models, bullet_trace_with_entity_models, contents_match_mask,
    glass_piece_from_hit, lagcomp_rewind_ticks,
};
pub use carrier::{SimWorld, StepReason, step, try_step};
pub use collision_census::{
    CollisionCensus, EntityClipCensus, ModelCollisionCensus, PlayerClipCensus, WorldClipCensus,
};
pub use combat::{
    AcceptedShot, Emission, EntityClipKind, PlayerCollisionRepresentation, ShotCollisionGeometry,
    ShotCollisionVerdict, TracePhaseOutput, spread_direction_on_plane, spread_pellet_direction,
};
pub use content::{CONTENT_DIGEST_SCHEME, ContentComponents, content_components, content_digest};
pub use corpse::{PlayerCorpsePool, PlayerCorpseSlot, level_time_ms};
pub use damage::{DamageAttempt, DamageOutcome, DamageRefusal, DeathCommit};
pub use equipment::{
    EquipmentRuntimeFacts, ProjectileHitGeometry, ProjectileImpact, ProjectileState,
    projectile_birth_ms,
};
pub use gentity::{
    EntityAllocError, EntityKernel, EntityKernelOccupiedSnapshot, EntityKernelSlotSnapshot,
    EntityKernelSnapshot, EntityKernelSnapshotError, EntityRef, EntityRefError, EntityRelations,
    EntityRunKind, EntitySlotView, GENTITY_RESERVED_COUNT, GENTITY_REUSE_QUARANTINE_MS,
    GENTITY_TEMP_EVENT_LIFETIME_MS, KERNEL_PHASE_ORDER, KernelPhase, ScriptMoverGentity,
    ThinkEnter, UsePress, apos_from_entity_state, begin_script_mover_rotate_velocity,
    collect_use_presses, gentity_spawn_base, init_item_state, init_missile_state,
    init_script_mover_state, pos_from_entity_state, rotate_velocity_apos,
};
pub use hudelem::{
    GameHudElemSlot, HUDELEM_UPDATE_ARCHIVAL, HUDELEM_UPDATE_BOTH, HUDELEM_UPDATE_CURRENT,
    hud_elem_update_client, rebase_hud_archival,
};
pub use identities::{
    ActionSequence, DamageSource, EventSequence, FireCause, LifeSequence, MatchPhase, MatchRng,
    PelletId, PendingBrass, ProjectileId, RNG_DOMAIN_SCHEME, RngDomain, ScriptModelId, ShotId,
};
pub use input::{
    ActionRequestId, ClassId, ClientAction, CommandSequence, MENU_RESPONSE_BYTES, PlayerCommand,
    SpawnPick, TickInput, action_request_id, menu_response_field, menu_response_text,
};
pub use mantle_xanim::MantleXAnimBind;
pub use match_state::{
    ClassDef, ClassRejectReason, ClientLifecycle, ClientSnapshotMeta,
    ConfigurationChangeRejectReason, DroppedItemAmmo, EntityEventPayload, EntityEventRecord,
    EventAudience, EventRecord, GiveRejectReason, HealthRegenCensus, InputReceipt,
    ItemPickupRecord, KillcamHud, LinkedWeaponView, LoadoutSpec, LocationSelection,
    MENU_COMMAND_TAIL, MatchEndReason, MenuCommand, MenuCommandKind, PelletFxRecord, PersonalClass,
    RadarMode, RemoteMissile, RngDebugMeta, SIM_EVENT_ROSTER, ScriptBlur, ScriptControls,
    ScriptDepthOfField, ScriptDvars, ScriptSeat, SimEvent, SimEventRow, SnapshotMeta,
    TargetBoxDvar, UNRELIABLE_SIM_EVENT_COUNT, ViewEffects, VisionChange, is_postfx_dvar,
    sim_event_is_reliable,
};
pub use player_anim_script::{
    AnimConditions, AnimScriptCommand, AnimScriptCondition, AnimScriptItem, PlayerAnimScript,
    anim_conditions_from_pmove, pmove_anim_weapon_ids,
};
pub use rules::{FFA, FreeForAllRules};
pub use score::{MATCH_TICK_MS, bootstrap_score_defaults};
pub use script_gaps::ScriptGaps;
pub use snapshot::{
    AreaEntityLinkSnapshot, AreaEntityWorldSnapshot, AreaEntityWorldSnapshotError,
    AreaSectorSnapshot, Snapshot,
};
pub use sound_alias_cs::{
    CS_LOCALIZED_STRINGS_SLOTS, CS_SOUNDALIASES_SLOTS, EffectNameCs, EffectNameCsOccupied,
    HUD_PRINT_ARG_SEPARATOR, HUD_STRING_PLAIN, HudMaterialCs, HudMaterialCsOccupied, HudStringCs,
    HudStringCsOccupied, REQUIRED_HUD_MATERIALS, SoundAliasCs, SoundAliasCsOccupied,
    hud_string_in_occupied, name_in_occupied,
};
pub use spawn::{
    AuthoredSpawnPoint, HostCheats, HostGameModeSelection, MatchBootstrap, SPAWN_BAD_DIST,
    SPAWN_IDEAL_DIST, SpawnAttemptReport, SpawnDecision, SpawnReject, host_game_mode_kind,
    pick_ffa_spawn, spawn_candidate_indices, spawn_candidate_indices_for,
};
pub use step::phase_materialize_entity_dobjs;

pub const WEAPON_MODEL_PREFIX: &str = "#weapon:";

pub fn weapon_model_attachment(model: &str) -> Option<u32> {
    model.strip_prefix(WEAPON_MODEL_PREFIX)?.parse().ok()
}
pub use world::{
    ClientId, HitvolDumpRow, PendingLocalSound, PendingPlayerCardEvent, PendingPlayerCardKind,
    PendingPrint, PlayerKitCollision, SimBrush, SimClipBsp, SimClipCmodels, SimClipMesh,
    SimStaticModel, SimTriggerHull, Tick, WeaponScriptSounds, blank_player_state,
};
pub use world_objects::{
    DestructibleLoopSound, GLASS_BLAST_DAMAGE_SCALE, GLASS_BLAST_RADIUS_CAP,
    GLASS_DAMAGE_TO_DESTROY, GLASS_DAMAGE_TO_WEAKEN, GLASS_FRACTURE_PROFILE_VERSION,
    GLASS_MELEE_DAMAGE, GLASS_PROJECTILE_PANE_HOPS, GlassBreakRecord, GlassCause, GlassPaneBasis,
    GlassPieceId, GlassPieceSnapshot, GlassPieceState, GlassShatterSeed, MISSILE_GLASS_SHATTER_VEL,
    WorldObjectSnapshot, WorldObjectState, glass_blast_integer_damage,
};

mod scene_effects;
mod time_scale;
pub use scene_effects::{
    MAX_SCRIPT_EARTHQUAKES, ScriptEarthquake, ScriptFog, ScriptFogParams, ScriptSunFog,
};
pub use time_scale::ScriptSlowMotion;
mod objectives;
pub use objectives::{
    CompassObjective, CompassVehicle, ObjectiveMatch, ObjectiveState, ScriptEffect,
    VehicleHudTarget,
};

pub use world::{SimContent, SimContentBuilder, WeaponSetup};

mod script_audio;
pub use script_audio::{ScriptAmbient, ScriptAudioCommand};

pub use input::PlayerProfile;

mod tick_result;
pub use tick_result::{
    ActionOutcome, ActionResult, FireCommandOutcome, FireCommandRefusal, FireCommandResult,
    TickEffects, TickResult,
};
