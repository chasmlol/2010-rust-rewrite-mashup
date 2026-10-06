mod catalog_linking;
mod model_linking;
mod publication;
use catalog_linking::{material_hint_edge, stamp_combat_fx};
mod capture_merge;
use capture_merge::{
    apply_leftover_default_anim_overrides, apply_leftover_default_sound_overrides,
    apply_leftover_hip_spread, idle_from_capture, leftover_hip_spread_block, merge_body_facts,
    merge_combat_fx, merge_combat_slots, merge_sound_aliases, merge_sz_xanims,
    movement_from_capture, read_hide_tags, read_name, read_script_string_map, read_sz_xanims,
    xanims_idle,
};
mod preparation;
pub use preparation::{
    ComponentPreparationPolicy, ComponentPreparationRefusal, PreparedComponentReference,
    PreparedComponentTarget, WeaponComponent, WeaponPreparationRecipe, WeaponPreparationRefusal,
};
mod iw5_parameters;
mod native_t6;
use iw5_parameters::*;
mod registry;
pub use capture_t6::{
    t6_attachment_ads_model, t6_attachment_models, t6_attachment_sound_names,
    t6_attachment_xanim_names, t6_model_name, t6_weapon_sound_names, t6_weapon_xanim_names,
};
mod capture_t6;
pub use capture_t5::t5_inline_note_alias;
use capture_t6::*;
mod capture_t5;
use capture_t5::*;
mod capture_iw5;
use capture_iw5::*;
mod binding;
mod capture_iw4;
pub use binding::{BoundWeapon, WeaponBindingRefusal, WeaponHandle};
mod combat;
pub use combat::WeaponCombatRefusal;
pub(crate) mod configuration;
mod equipment;
mod fpv;
mod iw5_configuration;
mod presentation;
pub use fpv::WeaponFpvFacts;
pub use presentation::{
    ProjectileCameraPolicy, WeaponEventFacts, WeaponHudFacts, WeaponWorldFacts,
};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn mint_weapon_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

use asset_core::{
    AssetEdge, AssetEdgeCensus, AssetEdgeReason, FpvMeshSpace, FxSpace, MaterialSpace,
    ProjectileModelSpace, TracerSpace, WorldWeaponSpace, XAnimSpace, ZoneOwner,
};
use asset_iw4::size::{WEAPON_ANIM_COUNT, weap_anim};

use fastfile_iw4::{
    Ptr, ScriptStrings, WeaponIdleCapture, WeaponMovementOfsCapture, ZonePtr, ZoneStream,
};
use weapon_iw4::{WEAPON_ANIM_SLOTS, weap_anim_extra};
use weapon_iw4::{WeaponIdleInputs, WeaponMovementOfsInputs};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct WeaponBodyFacts {
    pub body_resolved: bool,

    pub fire_time_ms: i32,

    pub impact_type: i32,

    pub raise_time_ms: i32,

    pub drop_time_ms: i32,
    pub alternate_raise_time_ms: i32,
    pub alternate_drop_time_ms: i32,
    pub first_raise_time_ms: i32,

    pub fire_delay_ms: i32,

    pub hold_fire_time_ms: i32,

    pub weap_type: i32,

    pub player_anim_type: i32,

    pub weap_class: i32,

    pub offhand_class: i32,

    pub shots_per_fire: i32,

    pub ammo_index: i32,

    pub clip_index: i32,

    pub ammo_counter_clip: i32,

    pub low_ammo_warning_threshold: f32,

    pub hip_spread_stand_min: f32,

    pub hip_spread_ducked_min: f32,

    pub hip_spread_prone_min: f32,

    pub hip_spread_stand_max: f32,

    pub hip_spread_ducked_max: f32,

    pub hip_spread_prone_max: f32,

    pub hip_spread_decay_rate: f32,

    pub hip_spread_fire_add: f32,

    pub hip_spread_turn_add: f32,

    pub hip_spread_move_add: f32,

    pub hip_spread_ducked_decay: f32,

    pub hip_spread_prone_decay: f32,

    pub i_reticle_side_size: i32,

    pub i_reticle_min_ofs: i32,

    pub hip_reticle_side_pos: f32,

    pub ads_aim_pitch: f32,

    pub ads_crosshair_in_frac: f32,

    pub ads_crosshair_out_frac: f32,

    pub ads_spread: f32,

    pub can_hold_breath: bool,

    pub aim_down_sight: bool,

    pub thermal_scope: bool,

    pub silenced: bool,

    pub ads_zoom_fov: f32,

    pub ads_dof: Option<[f32; 2]>,

    pub ads_zoom_in_frac: f32,

    pub ads_zoom_out_frac: f32,

    pub no_ads_when_mag_empty: bool,

    pub inherits_perks: bool,

    pub ads_reload_trans_time_ms: i32,
    pub ads_in_rate: f32,

    pub ads_out_rate: f32,

    pub rechamber_while_ads: bool,

    pub ads_fire_only: bool,

    pub melee_damage: i32,

    pub overlay_reticle: i32,

    pub overlay_interface: i32,

    pub ads_overlay_width: f32,

    pub ads_overlay_height: f32,

    pub melee_time_ms: i32,

    pub melee_delay_ms: i32,

    pub melee_charge_time_ms: i32,

    pub melee_charge_delay_ms: i32,

    pub knife_model: u32,

    pub use_as_melee: bool,

    pub quick_raise_time_ms: i32,

    pub quick_drop_time_ms: i32,

    pub select_requires_ammo: Option<bool>,

    pub offhand_hold_is_cancelable: Option<bool>,

    pub move_speed_scale: f32,

    pub ads_move_speed_scale: f32,

    pub sprint_duration_scale: f32,

    pub ducked_ofs: [f32; 3],

    pub prone_ofs: [f32; 3],

    pub night_vision_wear_time: i32,

    pub ads_bob_factor: f32,

    pub ads_view_bob_mult: f32,

    pub movement: WeaponMovementOfsInputs,

    pub idle: WeaponIdleInputs,

    pub clip_size: i32,
    pub penetrate_type: i32,

    pub penetrate_multiplier: f32,

    pub motion_tracker: bool,

    pub rifle_bullet: bool,
    pub ricochet_chance: f32,
    pub explosive_bullet: bool,
    pub inventory_type: i32,
    pub fire_type: i32,
    pub max_ammo: i32,
    pub damage: i32,
    pub rechamber_time_ms: i32,

    pub rechamber_bolt_time_ms: i32,

    pub rechamber_bolt_delay_ms: i32,
    pub reload_time_ms: i32,

    pub reload_show_rocket_time_ms: i32,
    pub reload_empty_time_ms: i32,
    pub reload_add_time_ms: i32,

    pub reload_empty_add_time_ms: i32,
    pub reload_start_time_ms: i32,

    pub reload_start_add_time_ms: i32,
    pub reload_end_time_ms: i32,

    pub dual_mag: Option<weapon_iw4::DualMagTimes>,

    pub kill_icon_ratio: i32,

    pub flip_kill_icon: bool,

    pub reload_ammo_add: i32,

    pub reload_start_add: i32,

    pub no_partial_reload: bool,

    pub bolt_action: bool,

    pub segmented_reload: bool,

    pub sprint_raise_time_ms: i32,

    pub sprint_loop_time_ms: i32,

    pub sprint_drop_time_ms: i32,
    pub stunned_start_time_ms: i32,
    pub stunned_end_time_ms: i32,
    pub fuse_time_ms: i32,

    pub auto_aim_range: f32,

    pub aim_assist_range: f32,

    pub aim_assist_range_ads: f32,

    pub cook_off_hold: bool,

    pub clip_only: bool,

    pub has_detonator: bool,
    pub detonate_delay_ms: i32,
    pub detonate_time_ms: i32,
    pub projectile_rotates: bool,
    pub timed_detonation: bool,

    pub proj_impact_explode: bool,

    pub stick_to_players: bool,
    pub explosion_radius: i32,
    pub explosion_radius_min: i32,
    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub damage_cone_angle: f32,
    pub missile_guidance: i32,
    pub ignition_delay_ms: i32,
    pub require_lock_to_fire: bool,
    pub stickiness: i32,
    pub projectile_speed: i32,
    pub projectile_speed_up: i32,
    pub projectile_speed_forward: i32,
    pub projectile_speed_relative_up: i32,
    pub refuses_pickup: bool,
    pub projectile_activate_dist: i32,
    pub projectile_explosion_type: i32,
    pub parallel_bounce: Option<[f32; 31]>,
    pub perpendicular_bounce: Option<[f32; 31]>,
    pub location_damage_mult: Option<[f32; 20]>,
    pub start_ammo: i32,

    pub ammo_count_clip_relative: bool,
    pub min_damage: i32,
    pub min_player_damage: i32,
    pub max_damage_range: f32,
    pub min_damage_range: f32,

    pub kick: WeaponKickFacts,

    pub sway: WeaponSwayFacts,

    pub dual_wield_view_model_offset: f32,

    pub no_dual_wield: bool,
    pub dual_wield: bool,
    pub fire_melees: bool,
}

impl WeaponBodyFacts {
    pub fn start_ammo_rounds(&self) -> i32 {
        leftover_clip_relative_rounds(
            self.start_ammo,
            self.clip_size,
            self.ammo_count_clip_relative,
        )
    }

    pub fn max_ammo_rounds(&self) -> i32 {
        leftover_clip_relative_rounds(self.max_ammo, self.clip_size, self.ammo_count_clip_relative)
    }
}

fn leftover_clip_relative_rounds(count: i32, clip_size: i32, clip_relative: bool) -> i32 {
    if clip_relative && clip_size > 0 && count > 0 {
        count.saturating_mul(clip_size)
    } else {
        count
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponKickFacts {
    pub f_ads_view_kick_center_speed: f32,

    pub f_hip_view_kick_center_speed: f32,
    pub gun_max_pitch: f32,
    pub gun_max_yaw: f32,
    pub ads_gun_kick_reduced_kick_bullets: i32,
    pub ads_gun_kick_reduced_kick_percent: f32,
    pub ads_gun_kick_pitch_min: f32,
    pub ads_gun_kick_pitch_max: f32,
    pub ads_gun_kick_yaw_min: f32,
    pub ads_gun_kick_yaw_max: f32,
    pub ads_gun_kick_accel: f32,
    pub ads_gun_kick_speed_max: f32,
    pub ads_gun_kick_speed_decay: f32,
    pub ads_gun_kick_static_decay: f32,
    pub ads_view_kick_pitch_min: f32,
    pub ads_view_kick_pitch_max: f32,
    pub ads_view_kick_yaw_min: f32,
    pub ads_view_kick_yaw_max: f32,
    pub hip_gun_kick_reduced_kick_bullets: i32,
    pub hip_gun_kick_reduced_kick_percent: f32,
    pub hip_gun_kick_pitch_min: f32,
    pub hip_gun_kick_pitch_max: f32,
    pub hip_gun_kick_yaw_min: f32,
    pub hip_gun_kick_yaw_max: f32,
    pub hip_gun_kick_accel: f32,
    pub hip_gun_kick_speed_max: f32,
    pub hip_gun_kick_speed_decay: f32,
    pub hip_gun_kick_static_decay: f32,
    pub hip_view_kick_pitch_min: f32,
    pub hip_view_kick_pitch_max: f32,
    pub hip_view_kick_yaw_min: f32,
    pub hip_view_kick_yaw_max: f32,
}

impl WeaponKickFacts {
    fn from_capture(c: fastfile_iw4::WeaponKickCapture) -> Self {
        Self {
            f_ads_view_kick_center_speed: c.f_ads_view_kick_center_speed,
            f_hip_view_kick_center_speed: c.f_hip_view_kick_center_speed,
            gun_max_pitch: c.gun_max_pitch,
            gun_max_yaw: c.gun_max_yaw,
            ads_gun_kick_reduced_kick_bullets: c.ads_gun_kick_reduced_kick_bullets,
            ads_gun_kick_reduced_kick_percent: c.ads_gun_kick_reduced_kick_percent,
            ads_gun_kick_pitch_min: c.ads_gun_kick_pitch_min,
            ads_gun_kick_pitch_max: c.ads_gun_kick_pitch_max,
            ads_gun_kick_yaw_min: c.ads_gun_kick_yaw_min,
            ads_gun_kick_yaw_max: c.ads_gun_kick_yaw_max,
            ads_gun_kick_accel: c.ads_gun_kick_accel,
            ads_gun_kick_speed_max: c.ads_gun_kick_speed_max,
            ads_gun_kick_speed_decay: c.ads_gun_kick_speed_decay,
            ads_gun_kick_static_decay: c.ads_gun_kick_static_decay,
            ads_view_kick_pitch_min: c.ads_view_kick_pitch_min,
            ads_view_kick_pitch_max: c.ads_view_kick_pitch_max,
            ads_view_kick_yaw_min: c.ads_view_kick_yaw_min,
            ads_view_kick_yaw_max: c.ads_view_kick_yaw_max,
            hip_gun_kick_reduced_kick_bullets: c.hip_gun_kick_reduced_kick_bullets,
            hip_gun_kick_reduced_kick_percent: c.hip_gun_kick_reduced_kick_percent,
            hip_gun_kick_pitch_min: c.hip_gun_kick_pitch_min,
            hip_gun_kick_pitch_max: c.hip_gun_kick_pitch_max,
            hip_gun_kick_yaw_min: c.hip_gun_kick_yaw_min,
            hip_gun_kick_yaw_max: c.hip_gun_kick_yaw_max,
            hip_gun_kick_accel: c.hip_gun_kick_accel,
            hip_gun_kick_speed_max: c.hip_gun_kick_speed_max,
            hip_gun_kick_speed_decay: c.hip_gun_kick_speed_decay,
            hip_gun_kick_static_decay: c.hip_gun_kick_static_decay,
            hip_view_kick_pitch_min: c.hip_view_kick_pitch_min,
            hip_view_kick_pitch_max: c.hip_view_kick_pitch_max,
            hip_view_kick_yaw_min: c.hip_view_kick_yaw_min,
            hip_view_kick_yaw_max: c.hip_view_kick_yaw_max,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponSwayFacts {
    pub sway_max_angle: f32,
    pub sway_lerp_speed: f32,
    pub sway_pitch_scale: f32,
    pub sway_yaw_scale: f32,
    pub sway_horiz_scale: f32,
    pub sway_vert_scale: f32,

    pub sway_shell_shock_scale: f32,
    pub ads_sway_max_angle: f32,
    pub ads_sway_lerp_speed: f32,
    pub ads_sway_pitch_scale: f32,
    pub ads_sway_yaw_scale: f32,
    pub ads_sway_horiz_scale: f32,
    pub ads_sway_vert_scale: f32,
}

impl WeaponSwayFacts {
    fn from_capture(c: fastfile_iw4::WeaponSwayCapture) -> Self {
        Self {
            sway_max_angle: c.sway_max_angle,
            sway_lerp_speed: c.sway_lerp_speed,
            sway_pitch_scale: c.sway_pitch_scale,
            sway_yaw_scale: c.sway_yaw_scale,
            sway_horiz_scale: c.sway_horiz_scale,
            sway_vert_scale: c.sway_vert_scale,
            sway_shell_shock_scale: c.sway_shell_shock_scale,
            ads_sway_max_angle: c.ads_sway_max_angle,
            ads_sway_lerp_speed: c.ads_sway_lerp_speed,
            ads_sway_pitch_scale: c.ads_sway_pitch_scale,
            ads_sway_yaw_scale: c.ads_sway_yaw_scale,
            ads_sway_horiz_scale: c.ads_sway_horiz_scale,
            ads_sway_vert_scale: c.ads_sway_vert_scale,
        }
    }

    pub fn hip_params(&self) -> weapon_iw4::WeaponSwayParams {
        weapon_iw4::WeaponSwayParams {
            max_angle: self.sway_max_angle,
            pitch_scale: self.sway_pitch_scale,
            yaw_scale: self.sway_yaw_scale,
            horiz_scale: self.sway_horiz_scale,
            vert_scale: self.sway_vert_scale,
        }
    }

    pub fn ads_params(&self) -> weapon_iw4::WeaponSwayParams {
        weapon_iw4::WeaponSwayParams {
            max_angle: self.ads_sway_max_angle,
            pitch_scale: self.ads_sway_pitch_scale,
            yaw_scale: self.ads_sway_yaw_scale,
            horiz_scale: self.ads_sway_horiz_scale,
            vert_scale: self.ads_sway_vert_scale,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacOffhandBucket {
    Lethal,
    Tactical,
}

pub fn cac_offhand_bucket(offhand_class: i32) -> Option<CacOffhandBucket> {
    match offhand_class {
        1 | 4 | 5 => Some(CacOffhandBucket::Lethal),
        2 | 3 => Some(CacOffhandBucket::Tactical),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponCamoModels {
    pub view: Vec<(u8, String)>,
    pub world: Vec<(u8, String)>,
}

impl WeaponCamoModels {
    pub fn is_empty(&self) -> bool {
        self.view.is_empty() && self.world.is_empty()
    }
}

#[derive(Clone, Debug)]
struct CatalogWeapon {
    pub namespace: crate::AssetNamespace,
    pub name: String,
    pub alternate_weapon: Option<String>,
    pub impact_payload: Option<String>,

    pub weap_def: Option<(u8, u32)>,

    pub display_name_key: Option<String>,

    pub reticle: WeaponReticleAssets,

    pub hud_material_edges: WeaponHudMaterialEdges,

    pub overlay_material: Option<String>,

    pub overlay_image: Option<String>,
    pub reticle_center_slot: Option<Ptr>,
    pub reticle_side_slot: Option<Ptr>,

    pub overlay_material_slot: Option<Ptr>,

    pub iw5_attachment_slots: [Option<String>; fastfile_iw5::size::WEAPON_ATTACHMENT_SLOT_COUNT],
    pub attached_models: [Vec<String>; 2],
    pub t6_clip_models: [Option<String>; 2],
    pub t6_attachments: Vec<T6Attachment>,
    pub t6_attachment_stats: Vec<T6AttachmentStats>,
    pub iw5_reload_overrides: Vec<fastfile_iw5::ReloadOverride>,
    pub iw5_anim_overrides: Vec<LeftoverAnimOverride>,
    pub iw5_fx_overrides: Vec<Iw5FxOverride>,
    pub iw5_notetrack_overrides: Vec<Iw5NotetrackOverride>,

    pub hud_icon: Option<String>,
    pub hud_icon_slot: Option<Ptr>,
    pub pickup_icon: Option<String>,
    pub pickup_icon_slot: Option<Ptr>,
    pub pickup_icon_image: Option<String>,
    pub pickup_icon_ratio: i32,
    pub hud_icon_ratio: i32,

    pub hud_icon_image: Option<String>,

    pub dpad_icon: Option<String>,
    pub dpad_icon_image: Option<String>,
    pub dpad_icon_atlas: Option<[u8; 2]>,
    pub dpad_icon_ratio: i32,
    pub kill_icon: Option<String>,
    pub kill_icon_slot: Option<Ptr>,

    pub kill_icon_image: Option<String>,

    pub proj_trail: Option<String>,
    pub proj_trail_slot: Option<Ptr>,

    pub proj_beacon: Option<String>,
    pub proj_beacon_slot: Option<Ptr>,

    pub proj_ignition: Option<String>,
    pub proj_ignition_slot: Option<Ptr>,

    pub projectile_fx: WeaponProjectileFx,

    pub gun_xmodel: Option<String>,

    pub hand_xmodel: Option<String>,
    pub dual_wield_weapon: Option<String>,

    pub world_model: Option<String>,

    pub camo_models: WeaponCamoModels,
    pub skin_parent: Option<String>,

    pub projectile_model: Option<String>,

    pub rocket_model: Option<String>,

    pub knife_xmodel: Option<String>,

    pub sz_xanims: [Option<String>; WEAPON_ANIM_SLOTS],

    pub sz_xanims_right: [Option<String>; WEAPON_ANIM_SLOTS],

    pub sz_xanims_left: [Option<String>; WEAPON_ANIM_SLOTS],

    pub hide_tags: Vec<String>,

    pub sounds: WeaponSoundAliases,

    pub combat_fx: WeaponCombatFx,

    pub(crate) combat_slots: CombatFxSlots,
    pub facts: WeaponBodyFacts,
}

#[derive(Clone, Debug, Default)]
struct Iw5ScopeRow {
    pub display_name: Option<String>,
    pub weapon_type: i32,
    pub weapon_class: i32,
    pub overlay: Option<String>,
    pub view_models: [Option<String>; fastfile_iw5::size::ATTACH_MODEL_COUNT],
    pub world_models: [Option<String>; fastfile_iw5::size::ATTACH_MODEL_COUNT],
    pub reticle_models: [Option<String>; fastfile_iw5::size::ATTACH_RETICLE_COUNT],
    pub overlay_reticle: i32,
    pub thermal: bool,
    pub width: f32,
    pub height: f32,
    pub sight: Option<fastfile_iw5::AttachmentSight>,
    pub ammo_general: Option<fastfile_iw5::AttachmentAmmoGeneral>,
    pub reload: Option<fastfile_iw5::AttachmentReload>,
    pub add_ons: Option<fastfile_iw5::AttachmentAddOns>,
    pub general: Option<fastfile_iw5::AttachmentGeneral>,
    pub ammunition: Option<fastfile_iw5::AttachmentAmmunition>,
    pub damage: Option<fastfile_iw5::AttachmentDamage>,
    pub projectile: Option<fastfile_iw5::AttachmentProjectile>,
    pub projectile_model: Option<String>,
    pub reticle: Option<WeaponReticleAssets>,
    pub projectile_explosion_fx: Option<String>,
    pub projectile_trail_fx: Option<String>,
    pub projectile_ignition_fx: Option<String>,
    pub projectile_explosion_sound: Option<String>,
    pub projectile_ignition_sound: Option<String>,
    pub location_damage: Option<[f32; 19]>,
    pub idle_settings: Option<fastfile_iw5::AttachmentIdleSettings>,
    pub ads_settings: Option<fastfile_iw5::AttachmentAdsSettings>,
    pub ads_settings_main: Option<fastfile_iw5::AttachmentAdsSettings>,
    pub hip_spread: Option<fastfile_iw5::AttachmentHipSpread>,
    pub gun_kick: Option<fastfile_iw5::AttachmentGunKick>,
    pub view_kick: Option<[f32; 10]>,
    pub scales: fastfile_iw5::AttachmentScales,
    pub share_ammo_with_alt: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Iw5AttachmentSelection {
    pub scope: u8,
    pub underbarrel: u8,
    pub others: u8,
}

impl Iw5AttachmentSelection {
    fn override_candidates(self) -> [u16; 3] {
        let mut candidates = [0; 3];
        let mut count = 0;
        let mut push = |condition| {
            candidates[count.min(2)] = condition;
            count += 1;
        };
        if (1..=6).contains(&self.scope) {
            push(u16::from(self.scope));
        }
        if (1..=3).contains(&self.underbarrel) {
            push(u16::from(self.underbarrel) << 3);
        }
        for bit in 0..4 {
            if self.others & (1 << bit) != 0 {
                push(1 << (5 + bit));
            }
        }
        candidates
    }

    pub fn contains_condition(self, condition: u16) -> bool {
        if condition == 0 {
            return true;
        }
        self.override_candidates().contains(&condition)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponReticleAssets {
    pub center_material: Option<String>,

    pub side_material: Option<String>,

    pub center_edge: AssetEdge<MaterialSpace>,

    pub side_edge: AssetEdge<MaterialSpace>,

    pub center_image: Option<String>,

    pub side_image: Option<String>,

    pub center_size: i32,

    pub side_size: i32,

    /// Whether the zone authored a reticle material at all. The slot pointers
    /// that answered this during the walk stay on the build row: the HUD, and
    /// the edge stamping beside it, only ever asked whether there was one.
    pub center_authored: bool,

    pub side_authored: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponHudMaterialEdges {
    pub overlay: AssetEdge<MaterialSpace>,
    pub hud_icon: AssetEdge<MaterialSpace>,
    pub pickup_icon: AssetEdge<MaterialSpace>,
    pub kill_icon: AssetEdge<MaterialSpace>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponProjectileFx {
    pub trail: AssetEdge<FxSpace>,
    pub beacon: AssetEdge<FxSpace>,
    pub ignition: AssetEdge<FxSpace>,
}

impl WeaponProjectileFx {
    pub fn edges(self) -> [AssetEdge<FxSpace>; 3] {
        [self.trail, self.beacon, self.ignition]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotetrackConvention {
    #[default]
    SoundMap,

    InlinePrefix,
}

impl NotetrackConvention {
    pub const fn dump_token(self) -> &'static str {
        match self {
            Self::SoundMap => "sound_map",
            Self::InlinePrefix => "inline_prefix",
        }
    }
}

pub const T5_NOTE_SOUND_PREFIX: &str = "sndnt#";

pub const T5_NOTE_RUMBLE_PREFIX: &str = "rmbnt#";

#[derive(Clone, Debug, Default)]
pub struct LinkedNotetrackAction {
    pub sound_alias: Option<String>,
    pub rumble_alias: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponDependencyGap {
    pub id: u32,
    pub kind: &'static str,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponSoundAliases {
    pub fire: Option<String>,
    pub fire_player: Option<String>,
    pub empty_fire: Option<String>,
    pub empty_fire_player: Option<String>,
    pub melee_swipe: Option<String>,
    pub melee_swipe_player: Option<String>,
    pub melee_hit: Option<String>,
    pub melee_miss: Option<String>,
    pub pickup: Option<String>,
    pub pickup_player: Option<String>,
    pub ammo_pickup: Option<String>,
    pub ammo_pickup_player: Option<String>,
    pub pullback: Option<String>,
    pub pullback_player: Option<String>,
    pub reload: Option<String>,
    pub reload_player: Option<String>,
    pub reload_empty: Option<String>,
    pub reload_empty_player: Option<String>,
    pub reload_start: Option<String>,
    pub reload_start_player: Option<String>,
    pub reload_end: Option<String>,
    pub reload_end_player: Option<String>,
    pub rechamber: Option<String>,
    pub rechamber_player: Option<String>,
    pub alt_switch: Option<String>,
    pub alt_switch_player: Option<String>,
    pub raise: Option<String>,
    pub raise_player: Option<String>,
    pub first_raise: Option<String>,
    pub first_raise_player: Option<String>,
    pub putaway: Option<String>,
    pub putaway_player: Option<String>,
    pub proj_explosion: Option<String>,

    pub projectile: Option<String>,

    pub proj_ignition_sound: Option<String>,

    pub bounce: [Option<String>; asset_iw4::size::SURF_TYPE_NUM],

    pub notetrack_sound_map: Vec<(String, String)>,

    pub notetrack_rumble_map: Vec<(String, String)>,

    pub fire_player_akimbo: Option<String>,

    pub fire_loop: Option<String>,
    pub fire_loop_player: Option<String>,

    pub fire_stop: Option<String>,
    pub fire_stop_player: Option<String>,

    pub fire_last: Option<String>,

    pub fire_last_player: Option<String>,

    leftover_sound_overrides: Vec<LeftoverSoundOverride>,

    pub fire_ptr_kind: Option<&'static str>,
    pub fire_player_ptr_kind: Option<&'static str>,
    pub reload_player_ptr_kind: Option<&'static str>,

    pub notetrack_convention: NotetrackConvention,
}

macro_rules! weapon_sound_slots {
    ($($variant:ident => $field:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(usize)]
        pub enum WeaponSoundSlot {
            $($variant),+
        }

        impl WeaponSoundSlot {
            pub const ALL: [Self; weapon_sound_slots!(@count $($variant),+)] = [
                $(Self::$variant),+
            ];
        }

        impl WeaponSoundAliases {
            fn hint(&self, slot: WeaponSoundSlot) -> Option<&str> {
                match slot {
                    $(WeaponSoundSlot::$variant => self.$field.as_deref()),+
                }
            }
        }
    };
    (@count $($variant:ident),+) => {
        <[()]>::len(&[$(weapon_sound_slots!(@unit $variant)),+])
    };
    (@unit $variant:ident) => { () };
}

weapon_sound_slots! {
    Fire => fire,
    FirePlayer => fire_player,
    EmptyFire => empty_fire,
    EmptyFirePlayer => empty_fire_player,
    MeleeSwipe => melee_swipe,
    MeleeSwipePlayer => melee_swipe_player,
    MeleeHit => melee_hit,
    MeleeMiss => melee_miss,
    Pickup => pickup,
    PickupPlayer => pickup_player,
    AmmoPickup => ammo_pickup,
    AmmoPickupPlayer => ammo_pickup_player,
    Pullback => pullback,
    PullbackPlayer => pullback_player,
    Reload => reload,
    ReloadPlayer => reload_player,
    ReloadEmpty => reload_empty,
    ReloadEmptyPlayer => reload_empty_player,
    ReloadStart => reload_start,
    ReloadStartPlayer => reload_start_player,
    ReloadEnd => reload_end,
    ReloadEndPlayer => reload_end_player,
    Rechamber => rechamber,
    RechamberPlayer => rechamber_player,
    AltSwitch => alt_switch,
    AltSwitchPlayer => alt_switch_player,
    Raise => raise,
    RaisePlayer => raise_player,
    FirstRaise => first_raise,
    FirstRaisePlayer => first_raise_player,
    Putaway => putaway,
    PutawayPlayer => putaway_player,
    ProjectileExplosion => proj_explosion,
    Projectile => projectile,
    ProjIgnition => proj_ignition_sound,
    FireLast => fire_last,
    FireLastPlayer => fire_last_player,
}

impl WeaponSoundAliases {
    pub fn reachable_aliases(&self) -> Vec<&str> {
        let mut out = Vec::new();
        let slots = [
            self.fire.as_deref(),
            self.fire_player.as_deref(),
            self.empty_fire.as_deref(),
            self.empty_fire_player.as_deref(),
            self.melee_swipe.as_deref(),
            self.melee_swipe_player.as_deref(),
            self.melee_hit.as_deref(),
            self.melee_miss.as_deref(),
            self.pickup.as_deref(),
            self.pickup_player.as_deref(),
            self.ammo_pickup.as_deref(),
            self.ammo_pickup_player.as_deref(),
            self.pullback.as_deref(),
            self.pullback_player.as_deref(),
            self.reload.as_deref(),
            self.reload_player.as_deref(),
            self.reload_empty.as_deref(),
            self.reload_empty_player.as_deref(),
            self.reload_start.as_deref(),
            self.reload_start_player.as_deref(),
            self.reload_end.as_deref(),
            self.reload_end_player.as_deref(),
            self.rechamber.as_deref(),
            self.rechamber_player.as_deref(),
            self.alt_switch.as_deref(),
            self.alt_switch_player.as_deref(),
            self.raise.as_deref(),
            self.raise_player.as_deref(),
            self.first_raise.as_deref(),
            self.first_raise_player.as_deref(),
            self.putaway.as_deref(),
            self.putaway_player.as_deref(),
            self.proj_explosion.as_deref(),
            self.projectile.as_deref(),
            self.proj_ignition_sound.as_deref(),
            self.fire_player_akimbo.as_deref(),
            self.fire_loop.as_deref(),
            self.fire_loop_player.as_deref(),
            self.fire_stop.as_deref(),
            self.fire_stop_player.as_deref(),
            self.fire_last.as_deref(),
            self.fire_last_player.as_deref(),
        ];
        for slot in slots.into_iter().flatten() {
            if !slot.is_empty() {
                out.push(slot);
            }
        }
        for name in &self.bounce {
            if let Some(s) = name.as_deref().filter(|s| !s.is_empty()) {
                out.push(s);
            }
        }
        for (_, alias) in &self.notetrack_sound_map {
            if !alias.is_empty() {
                out.push(alias.as_str());
            }
        }
        for ov in &self.leftover_sound_overrides {
            if let Some(s) = ov.override_sound.as_deref().filter(|s| !s.is_empty()) {
                out.push(s);
            }
            if let Some(s) = ov.altmode_sound.as_deref().filter(|s| !s.is_empty()) {
                out.push(s);
            }
        }
        out
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LeftoverAnimOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub anim_tree_type: u32,
    pub override_anim: Option<String>,
    pub altmode_anim: Option<String>,
    pub anim_time_ms: i32,
    pub alt_time_ms: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LeftoverSoundOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub sound_type: u32,
    pub override_sound: Option<String>,
    pub altmode_sound: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Iw5FxOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub fx_type: u32,
    pub override_fx: Option<String>,
    pub altmode_fx: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Iw5NotetrackOverride {
    pub attachment: u16,
    pub sound_map: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CombatFxSlots {
    view_flash: Option<Ptr>,
    world_flash: Option<Ptr>,
    view_shell_eject: Option<Ptr>,
    world_shell_eject: Option<Ptr>,
    view_last_shot_eject: Option<Ptr>,
    world_last_shot_eject: Option<Ptr>,
    explosion: Option<Ptr>,
    tracer: Option<Ptr>,
}

impl CombatFxSlots {
    fn last_shot_pair_authored(self) -> bool {
        self.view_last_shot_eject.is_some() && self.world_last_shot_eject.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponCombatFx {
    pub view_flash: AssetEdge<FxSpace>,
    pub view_flash_hint: Option<String>,
    pub world_flash: AssetEdge<FxSpace>,
    pub world_flash_hint: Option<String>,
    pub view_shell_eject: AssetEdge<FxSpace>,
    pub view_shell_eject_hint: Option<String>,
    pub world_shell_eject: AssetEdge<FxSpace>,
    pub world_shell_eject_hint: Option<String>,
    pub view_last_shot_eject: AssetEdge<FxSpace>,
    pub view_last_shot_eject_hint: Option<String>,
    pub world_last_shot_eject: AssetEdge<FxSpace>,
    pub world_last_shot_eject_hint: Option<String>,
    pub explosion: AssetEdge<FxSpace>,
    pub explosion_hint: Option<String>,

    pub tracer: AssetEdge<TracerSpace>,

    pub tracer_hint: Option<String>,

    pub namespace: crate::AssetNamespace,

    last_shot_eject_pair_authored: bool,
}

impl WeaponCombatFx {
    pub fn empty(namespace: crate::AssetNamespace) -> Self {
        Self {
            namespace,
            view_flash: AssetEdge::Absent,
            view_flash_hint: None,
            world_flash: AssetEdge::Absent,
            world_flash_hint: None,
            view_shell_eject: AssetEdge::Absent,
            view_shell_eject_hint: None,
            world_shell_eject: AssetEdge::Absent,
            world_shell_eject_hint: None,
            view_last_shot_eject: AssetEdge::Absent,
            view_last_shot_eject_hint: None,
            world_last_shot_eject: AssetEdge::Absent,
            world_last_shot_eject_hint: None,
            explosion: AssetEdge::Absent,
            explosion_hint: None,
            tracer: AssetEdge::Absent,
            tracer_hint: None,
            last_shot_eject_pair_authored: false,
        }
    }
    fn present_bound<'a>(
        &self,
        edge: AssetEdge<FxSpace>,
        hint: &'a Option<String>,
    ) -> Option<crate::FxName<'a>> {
        edge.is_bound()
            .then(|| hint.as_deref())
            .flatten()
            .filter(|name| !name.is_empty())
            .map(|name| crate::FxName::new(self.namespace, name))
    }

    pub fn view_flash_present(&self) -> Option<crate::FxName<'_>> {
        self.present_bound(self.view_flash, &self.view_flash_hint)
    }

    pub fn world_flash_present(&self) -> Option<crate::FxName<'_>> {
        self.present_bound(self.world_flash, &self.world_flash_hint)
    }

    pub fn flash_present(&self, player_view: bool) -> Option<crate::FxName<'_>> {
        if player_view {
            self.view_flash_present()
        } else {
            self.world_flash_present()
        }
    }

    pub fn flash_edge(&self, player_view: bool) -> AssetEdge<FxSpace> {
        if player_view {
            self.view_flash
        } else {
            self.world_flash
        }
    }

    pub fn view_shell_eject_present(&self) -> Option<crate::FxName<'_>> {
        self.present_bound(self.view_shell_eject, &self.view_shell_eject_hint)
    }

    pub fn world_shell_eject_present(&self) -> Option<crate::FxName<'_>> {
        self.present_bound(self.world_shell_eject, &self.world_shell_eject_hint)
    }

    pub fn brass_present(&self, player_view: bool) -> Option<crate::FxName<'_>> {
        if player_view {
            self.view_shell_eject_present()
        } else {
            self.world_shell_eject_present()
        }
    }

    pub fn last_shot_eject_pair_authored(&self) -> bool {
        self.last_shot_eject_pair_authored
    }

    pub fn last_shot_eject_present(&self, player_view: bool) -> Option<crate::FxName<'_>> {
        if player_view {
            self.present_bound(self.view_last_shot_eject, &self.view_last_shot_eject_hint)
        } else {
            self.present_bound(self.world_last_shot_eject, &self.world_last_shot_eject_hint)
        }
    }

    pub fn brass_present_for_event(
        &self,
        player_view: bool,
        last_shot: bool,
    ) -> Option<crate::FxName<'_>> {
        if last_shot && self.last_shot_eject_pair_authored() {
            self.last_shot_eject_present(player_view)
        } else {
            self.brass_present(player_view)
        }
    }

    pub fn brass_edge(&self, player_view: bool) -> AssetEdge<FxSpace> {
        if player_view {
            self.view_shell_eject
        } else {
            self.world_shell_eject
        }
    }

    pub fn brass_edge_for_event(&self, player_view: bool, last_shot: bool) -> AssetEdge<FxSpace> {
        if last_shot && self.last_shot_eject_pair_authored() {
            if player_view {
                self.view_last_shot_eject
            } else {
                self.world_last_shot_eject
            }
        } else {
            self.brass_edge(player_view)
        }
    }

    pub fn explosion_present(&self) -> Option<crate::FxName<'_>> {
        self.present_bound(self.explosion, &self.explosion_hint)
    }

    pub fn edges(&self) -> [AssetEdge<FxSpace>; 7] {
        [
            self.view_flash,
            self.world_flash,
            self.view_shell_eject,
            self.world_shell_eject,
            self.view_last_shot_eject,
            self.world_last_shot_eject,
            self.explosion,
        ]
    }
}

#[derive(Clone, Debug, Default)]
pub struct WeaponCatalog {
    entries: Vec<CatalogWeapon>,
    strings: ScriptStrings,
    iw5_attachments: HashMap<String, Iw5ScopeRow>,
    capture_ns: Option<crate::AssetNamespace>,
    vehicle_turrets: HashMap<String, String>,
    vehicle_compass: HashMap<String, ([String; 2], [i32; 2])>,
    vehicle_accel: HashMap<String, f32>,
}

fn fpv_model_edge(
    hint: Option<&str>,
    ns: Option<crate::AssetNamespace>,
    fpv: &crate::FpvMeshCatalog,
) -> AssetEdge<FpvMeshSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    match hint {
        None => AssetEdge::Absent,
        Some(name) => match ns.and_then(|ns| fpv.index_by_name(ns, name)) {
            Some(index) => AssetEdge::bind_order(index, fpv.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn world_model_edge(
    hint: Option<&str>,
    namespace: Option<crate::AssetNamespace>,
    catalog: &crate::WorldWeaponCatalog,
) -> AssetEdge<WorldWeaponSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    match hint {
        None => AssetEdge::Absent,
        Some(name) => match namespace.and_then(|ns| catalog.index_by_name(ns, name)) {
            Some(index) => AssetEdge::bind_order(index, catalog.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn sound_alias_in_bank<'a>(
    hint: Option<&str>,
    ns: crate::AssetNamespace,
    catalog: &'a crate::SoundCatalog,
) -> Option<(crate::AssetNamespace, &'a str)> {
    let name = hint.filter(|name| !name.is_empty())?;
    let order = catalog.index_in(ns, name)?;
    let bound = catalog.bind_published_alias(order, 0).ok()?;
    (bound.policy().namespace == ns).then_some((ns, catalog.name_at(order)?))
}

fn ptr_key(p: Ptr) -> (u8, u32) {
    (p.block, p.offset)
}

pub const T6_XANIM_PREFIX: &str = "t6_";

const T6_CLIP_SLOT: u32 = 6;

#[derive(Clone, Debug)]
struct T6Attachment {
    pub kind: u32,
    pub mask: u32,
    pub alt_weapon: Option<String>,
    pub models: [Vec<String>; 2],
    pub view_ads_model: Option<(String, String)>,
    pub overlay: Option<String>,
    pub hide_tags: Vec<String>,
    pub xanims: [Option<String>; WEAPON_ANIM_SLOTS],
    pub fire_sound: Option<String>,
    pub fire_sound_player: Option<String>,
    pub disable_base_attachment: bool,
    pub disable_base_clip: bool,
}

#[derive(Clone, Debug)]
struct T6AttachmentStats {
    pub kind: u32,
    pub clip_size_scale: f32,
    pub fire_time_scale: f32,
    pub reload_time_scales: [f32; 5],
    pub ads_in_time_scale: f32,
    pub ads_out_time_scale: f32,
    pub ads_zoom_fov: Option<f32>,
    pub ads_zoom_in_frac: Option<f32>,
    pub ads_zoom_out_frac: Option<f32>,
    pub damage_range_scale: f32,
    pub hip_spread_min_scale: f32,
    pub hip_spread_max_scale: f32,
    pub ads_move_speed_scale: f32,
    pub ads_view_kick_center_speed_scale: f32,
    pub ads_idle_amount_scale: f32,
    pub penetrating: bool,
    pub dual_mag: bool,
    pub shared_ammo: bool,
}

const ADS_WALK_SPEED_SCALE: f32 = 0.4;

pub struct T6AttachmentModel {
    pub copy: String,
    pub model: String,
    pub tag: Option<String>,
    pub offset: [f32; 3],
    pub angles: [f32; 3],
}

#[derive(Clone, Debug)]
struct WeaponRow {
    preparation: WeaponPreparationRecipe,
    name: String,
    alternate_weapon: Option<String>,
    impact_payload: Option<String>,
    alternate_index: u32,

    namespace: crate::AssetNamespace,

    facts: WeaponBodyFacts,
    semantics: Option<crate::WeaponSemanticPolicy>,
    combat: Option<combat::WeaponCombatProjection>,
    fpv: Option<WeaponFpvFacts>,
    hud: Option<WeaponHudFacts>,
    events: Option<WeaponEventFacts>,
    world: Option<WeaponWorldFacts>,
    equipment: Option<weapon_iw4::EquipmentRuntimeFacts>,
    penetration: Option<weapon_iw4::BulletPenFacts>,

    gun_xmodel: Option<String>,

    hand_xmodel: Option<String>,
    dual_wield_weapon: Option<String>,
    secondary_gun_xmodel: Option<String>,

    gun_xmodel_edge: AssetEdge<FpvMeshSpace>,

    hand_xmodel_edge: AssetEdge<FpvMeshSpace>,

    rocket_model_edge: AssetEdge<FpvMeshSpace>,

    attachment_view_model_edges: Vec<AssetEdge<FpvMeshSpace>>,

    fpv_hands: [Option<(asset_model::FpvHands, crate::FpvMeshIndex)>; 2],

    fpv_mount_plan: Option<Result<asset_model::FpvMountPlan, asset_model::FpvMountError>>,

    fpv_assemblies: [Option<Result<crate::FpvSideAssemblies, String>>; 2],

    world_model: Option<String>,

    world_model_edge: AssetEdge<WorldWeaponSpace>,

    camo_models: WeaponCamoModels,
    skin_parent: Option<String>,
    material_camos: Arc<[crate::WeaponCamouflage]>,

    camo_view_edges: Vec<(u8, AssetEdge<FpvMeshSpace>)>,
    camo_world_edges: Vec<(u8, AssetEdge<WorldWeaponSpace>)>,

    attachment_world_model_edges: Vec<AssetEdge<WorldWeaponSpace>>,

    attachment_world_mounts: Vec<Option<String>>,

    projectile_model: Option<String>,

    projectile_model_edge: AssetEdge<ProjectileModelSpace>,

    rocket_model: Option<String>,

    knife_xmodel: Option<String>,

    sz_xanims: [Option<String>; WEAPON_ANIM_SLOTS],

    sz_xanim_edges: [AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS],

    sz_xanim_right_edges: [AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS],

    sz_xanim_left_edges: [AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS],

    notetrack_actions: HashMap<String, LinkedNotetrackAction>,

    sz_xanims_right: [Option<String>; WEAPON_ANIM_SLOTS],

    sz_xanims_left: [Option<String>; WEAPON_ANIM_SLOTS],

    hide_tags: Vec<String>,

    attachment_view_models: Vec<String>,
    attachment_world_models: Vec<String>,
    attachment_view_ads_models: Vec<(String, String)>,

    t6_clip_models: [Option<String>; 2],
    t6_attachments: Vec<T6Attachment>,
    t6_attachment_stats: Vec<T6AttachmentStats>,

    iw5_configuration: Option<(u32, Iw5AttachmentSelection)>,
    prepared_attachments: Vec<String>,
    attachment_caption_keys: Vec<String>,

    iw5_attachment_slots: [Option<String>; fastfile_iw5::size::WEAPON_ATTACHMENT_SLOT_COUNT],
    iw5_reload_overrides: Vec<fastfile_iw5::ReloadOverride>,
    iw5_anim_overrides: Vec<LeftoverAnimOverride>,
    iw5_fx_overrides: Vec<Iw5FxOverride>,
    iw5_notetrack_overrides: Vec<Iw5NotetrackOverride>,

    sounds: WeaponSoundAliases,

    combat_fx: WeaponCombatFx,

    reticle: WeaponReticleAssets,

    hud_material_edges: WeaponHudMaterialEdges,

    overlay_material: Option<String>,
    overlay_image: Option<String>,
    overlay_material_from_slot: bool,

    hud_icon: Option<String>,
    hud_icon_from_slot: bool,
    pickup_icon: Option<String>,
    pickup_icon_image: Option<String>,
    pickup_icon_authored: bool,
    pickup_icon_ratio: i32,
    hud_icon_ratio: i32,

    hud_icon_image: Option<String>,

    dpad_icon_image: Option<String>,
    dpad_icon_atlas: Option<[u8; 2]>,
    dpad_icon_ratio: i32,
    kill_icon: Option<String>,
    kill_icon_from_slot: bool,
    kill_icon_image: Option<String>,

    projectile_fx: WeaponProjectileFx,

    proj_trail: Option<String>,
    proj_trail_from_slot: bool,
    proj_beacon: Option<String>,
    proj_beacon_from_slot: bool,
    proj_ignition: Option<String>,
    proj_ignition_from_slot: bool,

    display_name_key: Option<String>,
}

impl Default for WeaponRow {
    fn default() -> Self {
        Self {
            preparation: WeaponPreparationRecipe::native(crate::AssetNamespace::Iw4),
            name: String::new(),
            alternate_weapon: None,
            impact_payload: None,
            alternate_index: 0,
            namespace: crate::AssetNamespace::Iw4,
            facts: WeaponBodyFacts::default(),
            semantics: None,
            combat: None,
            fpv: None,
            hud: None,
            events: None,
            world: None,
            equipment: None,
            penetration: None,
            gun_xmodel: None,
            hand_xmodel: None,
            dual_wield_weapon: None,
            secondary_gun_xmodel: None,
            gun_xmodel_edge: AssetEdge::Absent,
            hand_xmodel_edge: AssetEdge::Absent,
            rocket_model_edge: AssetEdge::Absent,
            attachment_view_model_edges: Vec::new(),
            fpv_hands: [None, None],
            fpv_mount_plan: None,
            fpv_assemblies: [None, None],
            world_model: None,
            world_model_edge: AssetEdge::Absent,
            camo_models: WeaponCamoModels::default(),
            skin_parent: None,
            material_camos: Arc::default(),
            camo_view_edges: Vec::new(),
            camo_world_edges: Vec::new(),
            attachment_world_model_edges: Vec::new(),
            attachment_world_mounts: Vec::new(),
            projectile_model: None,
            projectile_model_edge: AssetEdge::Absent,
            rocket_model: None,
            knife_xmodel: None,
            sz_xanims: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanim_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
            sz_xanim_right_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
            sz_xanim_left_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
            notetrack_actions: HashMap::new(),
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: [const { None }; WEAPON_ANIM_SLOTS],
            hide_tags: Vec::new(),
            attachment_view_models: Vec::new(),
            attachment_world_models: Vec::new(),
            attachment_view_ads_models: Vec::new(),
            t6_clip_models: Default::default(),
            t6_attachments: Vec::new(),
            t6_attachment_stats: Vec::new(),
            iw5_configuration: None,
            prepared_attachments: Vec::new(),
            attachment_caption_keys: Vec::new(),
            iw5_attachment_slots: std::array::from_fn(|_| None),
            iw5_reload_overrides: Vec::new(),
            iw5_anim_overrides: Vec::new(),
            iw5_fx_overrides: Vec::new(),
            iw5_notetrack_overrides: Vec::new(),
            sounds: WeaponSoundAliases::default(),
            combat_fx: WeaponCombatFx::empty(crate::AssetNamespace::Iw4),
            reticle: WeaponReticleAssets::default(),
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: None,
            overlay_image: None,
            overlay_material_from_slot: false,
            hud_icon: None,
            hud_icon_from_slot: false,
            pickup_icon: None,
            pickup_icon_image: None,
            pickup_icon_authored: false,
            pickup_icon_ratio: 0,
            hud_icon_ratio: 0,
            hud_icon_image: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: None,
            kill_icon_from_slot: false,
            kill_icon_image: None,
            projectile_fx: WeaponProjectileFx::default(),
            proj_trail: None,
            proj_trail_from_slot: false,
            proj_beacon: None,
            proj_beacon_from_slot: false,
            proj_ignition: None,
            proj_ignition_from_slot: false,
            display_name_key: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WeaponRegistry {
    rows: Vec<WeaponRow>,

    world_catalog_identity: u64,

    iw5_attachments: HashMap<String, Iw5ScopeRow>,

    configurations: HashMap<crate::WeaponSelection, u32>,

    by_name: HashMap<String, u32>,

    by_namespaced: HashMap<(crate::AssetNamespace, String), u32>,

    item_groups: HashMap<(crate::AssetNamespace, String), String>,

    families: crate::WeaponFamilies,
    completion_names: Vec<String>,

    alternate_fpv: HashMap<(u32, u32), [Option<Result<crate::FpvSideAssemblies, String>>; 2]>,
    fpv_clip_tracks: Arc<crate::FpvClipTracks>,

    vehicle_turrets: HashMap<String, String>,
    vehicle_compass: HashMap<String, ([String; 2], [i32; 2])>,
    vehicle_accel: HashMap<String, f32>,

    revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownWeaponName {
    pub name: String,
}

impl core::fmt::Display for UnknownWeaponName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown weapon name `{}`", self.name)
    }
}

impl std::error::Error for UnknownWeaponName {}

#[derive(Clone, Debug, Default)]
pub struct Iw5PreparationCensus {
    pub prepared: usize,
    pub refused: Vec<crate::WeaponSelection>,
}

fn compose_t6_configuration(base: &WeaponRow, kinds: &[u32], name: String) -> Option<WeaponRow> {
    if base.t6_attachments.is_empty() {
        return None;
    }
    let singles: Vec<&T6Attachment> = kinds
        .iter()
        .filter_map(|&kind| {
            base.t6_attachments
                .iter()
                .find(|attachment| attachment.kind == kind && attachment.mask == 0)
        })
        .collect();
    let mask = kinds.iter().fold(0u32, |mask, &kind| {
        mask | 1u32.checked_shl(kind).unwrap_or(0)
    });
    let pair = (kinds.len() > 1)
        .then(|| {
            base.t6_attachments
                .iter()
                .find(|attachment| attachment.mask == mask)
        })
        .flatten();
    let mut row = base.clone();
    row.name = name;
    row.t6_attachments = Vec::new();
    row.hide_tags = Vec::new();
    for (side, models) in [
        &mut row.attachment_view_models,
        &mut row.attachment_world_models,
    ]
    .into_iter()
    .enumerate()
    {
        let clip = base.t6_clip_models[side].as_ref();
        let is_clip = |model: &String| clip == Some(model);
        if singles.iter().any(|a| a.disable_base_attachment) {
            models.retain(|model| is_clip(model));
        }
        if singles.iter().any(|a| a.disable_base_clip) {
            models.retain(|model| !is_clip(model));
        }
        for attachment in &singles {
            models.extend(attachment.models[side].iter().cloned());
        }
    }
    if !singles.is_empty() {
        let overlay = match pair {
            Some(pair) => pair.overlay.clone(),
            None if singles.iter().any(|a| a.overlay.is_none()) => None,
            None => singles
                .iter()
                .filter_map(|a| a.overlay.clone())
                .find(|overlay| base.overlay_material.as_ref() != Some(overlay))
                .or_else(|| base.overlay_material.clone()),
        };
        match &overlay {
            None => row.facts.overlay_reticle = 0,
            Some(_) if row.facts.overlay_reticle == 0 => row.facts.overlay_reticle = 1,
            Some(_) => {}
        }
        row.facts.can_hold_breath = overlay.is_some() && row.facts.weap_class != 11;
        row.overlay_image = overlay.clone();
        row.overlay_material = overlay;
    }
    row.attachment_view_ads_models = singles
        .iter()
        .filter_map(|attachment| attachment.view_ads_model.clone())
        .collect();
    for attachment in singles.iter().copied().chain(pair) {
        for (slot, clip) in attachment.xanims.iter().enumerate() {
            if let Some(clip) = clip
                && base.sz_xanims[slot].as_ref() != Some(clip)
            {
                row.sz_xanims[slot] = Some(clip.clone());
            }
        }
    }
    for stats in kinds.iter().filter_map(|&kind| {
        base.t6_attachment_stats
            .iter()
            .find(|stats| stats.kind == kind)
    }) {
        apply_t6_attachment_stats(&mut row.facts, stats);
    }
    if singles.is_empty()
        && let Some(bare) = base
            .t6_attachments
            .iter()
            .find(|a| a.kind == 0 && a.mask == 0)
    {
        row.hide_tags = bare.hide_tags.clone();
    }
    for attachment in &singles {
        row.hide_tags.extend(attachment.hide_tags.iter().cloned());
        if let Some(sound) = &attachment.fire_sound {
            row.sounds.fire = Some(sound.clone());
        }
        if let Some(sound) = &attachment.fire_sound_player {
            row.sounds.fire_player = Some(sound.clone());
        }
        if let Some(alt) = &attachment.alt_weapon {
            row.alternate_weapon = Some(alt.clone());
        }
    }
    Some(row)
}

#[derive(Clone, Debug, Default)]
pub struct T6PreparationCensus {
    pub prepared: usize,
    pub refused: usize,
    pub alternates: usize,
}

fn t6_attachment_kinds(
    tables: &[(crate::AssetNamespace, crate::CapturedStringTable)],
) -> HashMap<String, u32> {
    tables
        .iter()
        .filter(|(namespace, table)| {
            *namespace == crate::AssetNamespace::T6
                && table.name.eq_ignore_ascii_case("mp/attachmentTable.csv")
        })
        .flat_map(|(_, table)| {
            (1..table.rows as i32)
                .filter(|&row| table.cell(row, 2) == "attachment")
                .filter_map(|row| {
                    Some((
                        table.cell(row, 4).to_ascii_lowercase(),
                        table.cell(row, 0).parse().ok()?,
                    ))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct T6Melee {
    pub knife: String,
    pub melee: String,
    pub charge: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct WeaponBuild {
    registry: WeaponRegistry,
    combat_slots: Vec<CombatFxSlots>,
    family_tables: Vec<(crate::AssetNamespace, crate::CapturedStringTable)>,
}

impl std::ops::Deref for WeaponBuild {
    type Target = WeaponRegistry;

    fn deref(&self) -> &Self::Target {
        &self.registry
    }
}

impl WeaponBuild {
    pub fn set_family_tables(
        &mut self,
        tables: Vec<(crate::AssetNamespace, crate::CapturedStringTable)>,
    ) {
        self.family_tables = tables;
    }

    pub fn prepare_t6_configurations(&mut self) -> T6PreparationCensus {
        let families = crate::WeaponFamilies::build(&self.family_tables, &self.registry);
        let kinds = t6_attachment_kinds(&self.family_tables);
        let mut census = T6PreparationCensus::default();
        let mut prepared = Vec::new();
        for (base_id, selection) in families.candidate_selections(crate::AssetNamespace::T6) {
            if selection.attachments.is_empty() {
                continue;
            }
            let Some(family) = selection
                .family
                .as_ref()
                .and_then(|key| families.family(key))
            else {
                continue;
            };
            let attachments = families.normalize(crate::AssetNamespace::T6, &selection.attachments);
            let name = configuration::authored_name(&family.key, &attachments);
            if self
                .registry
                .by_namespaced
                .contains_key(&(crate::AssetNamespace::T6, name.clone()))
            {
                continue;
            }
            let kinds: Option<Vec<u32>> =
                attachments.iter().map(|a| kinds.get(a).copied()).collect();
            match kinds.and_then(|kinds| {
                compose_t6_configuration(&self.registry.rows[base_id as usize], &kinds, name)
            }) {
                Some(mut row) => {
                    row.attachment_caption_keys = attachments
                        .iter()
                        .filter_map(|name| family.attachments.iter().find(|a| &a.name == name))
                        .map(|a| a.caption_key.trim_start_matches('@').to_owned())
                        .filter(|key| !key.is_empty())
                        .collect();
                    row.prepared_attachments = attachments.clone();
                    prepared.push((
                        base_id,
                        crate::WeaponSelection {
                            attachments,
                            ..selection
                        },
                        row,
                    ));
                }
                None => census.refused += 1,
            }
        }
        census.prepared = prepared.len();
        for (base_id, selection, row) in prepared {
            self.registry
                .configurations
                .insert(selection, self.registry.rows.len() as u32);
            let slots = self
                .combat_slots
                .get(base_id as usize)
                .copied()
                .unwrap_or_else(CombatFxSlots::default);
            self.combat_slots
                .resize(self.registry.rows.len(), CombatFxSlots::default());
            self.combat_slots.push(slots);
            self.registry.rows.push(row);
            if let Some((alternate, raise)) =
                self.compose_t6_alternate(base_id, self.registry.rows.len() - 1)
            {
                let parent = &mut self.registry.rows.last_mut().expect("just pushed");
                parent.alternate_weapon = Some(alternate.name.clone());
                parent.sz_xanims[weap_anim::ALT_RAISE] = raise;
                parent.sz_xanims[weap_anim::ALT_DROP] = None;
                parent.facts.alternate_drop_time_ms = 0;
                self.combat_slots.push(slots);
                self.registry.rows.push(alternate);
                census.alternates += 1;
            }
        }
        self.registry.rebuild_name_maps();
        self.registry.revision = mint_weapon_revision();
        census
    }

    fn compose_t6_alternate(
        &self,
        base_id: u32,
        parent: usize,
    ) -> Option<(WeaponRow, Option<String>)> {
        let rows = &self.registry.rows;
        let base = &rows[base_id as usize];
        let config = &rows[parent];
        let alt_name = config.alternate_weapon.as_deref()?;
        let attachment = base.t6_attachments.iter().find(|a| {
            a.mask == 0
                && a.alt_weapon
                    .as_deref()
                    .is_some_and(|alt| alt.eq_ignore_ascii_case(alt_name))
        })?;
        let shared = base
            .t6_attachment_stats
            .iter()
            .find(|stats| stats.kind == attachment.kind)
            .is_some_and(|stats| stats.shared_ammo);
        let alt_id = *self
            .registry
            .by_namespaced
            .get(&(crate::AssetNamespace::T6, normalize_weapon_name(alt_name)))?;
        let mut alt = rows[alt_id as usize].clone();
        alt.name = format!("{}+{}", alt.name, config.name);
        alt.alternate_weapon = Some(config.name.clone());
        alt.attachment_view_models = config.attachment_view_models.clone();
        alt.attachment_world_models = config.attachment_world_models.clone();
        alt.attachment_view_ads_models = config.attachment_view_ads_models.clone();
        alt.hide_tags = config.hide_tags.clone();
        alt.t6_attachments = Vec::new();
        for slot in 0..WEAPON_ANIM_SLOTS {
            if alt.sz_xanims[slot] == base.sz_xanims[slot] {
                alt.sz_xanims[slot] = config.sz_xanims[slot].clone();
            }
        }
        let authored = |slot: usize| {
            attachment.xanims[slot]
                .clone()
                .filter(|clip| base.sz_xanims[slot].as_ref() != Some(clip))
        };
        let mut config_raise = config.sz_xanims[weap_anim::ALT_RAISE].clone();
        if alt.sz_xanims[weap_anim::ALT_RAISE].is_none()
            || alt.sz_xanims[weap_anim::ALT_RAISE] == base.sz_xanims[weap_anim::ALT_RAISE]
        {
            alt.sz_xanims[weap_anim::ALT_RAISE] = config_raise.clone();
            if let Some(out) = authored(weap_anim::ALT_DROP) {
                config_raise = Some(out);
            }
        }
        alt.sz_xanims[weap_anim::ALT_DROP] = None;
        alt.facts.alternate_drop_time_ms = 0;
        if alt.facts.alternate_raise_time_ms <= 0 {
            alt.facts.alternate_raise_time_ms = config.facts.alternate_raise_time_ms;
        }
        if shared {
            alt.facts.ammo_index = parent as i32;
            alt.facts.clip_index = parent as i32;
            alt.facts.clip_size = config.facts.clip_size;
            alt.facts.start_ammo = config.facts.start_ammo;
            alt.facts.max_ammo = config.facts.max_ammo;
        }
        Some((alt, config_raise))
    }

    pub fn prepare_iw5_configurations(&mut self) -> Iw5PreparationCensus {
        let families = crate::WeaponFamilies::build(&self.family_tables, &self.registry);
        let mut census = Iw5PreparationCensus::default();
        let mut prepared = Vec::new();
        for (base_id, selection) in families.iw5_candidate_selections() {
            if selection.attachments.is_empty() {
                continue;
            }
            let Some(namespace) = selection.family.as_ref().map(|key| key.namespace) else {
                continue;
            };
            let attachments = families.normalize(namespace, &selection.attachments);
            let selection = crate::WeaponSelection {
                attachments,
                ..selection
            };
            let row = self
                .registry
                .resolve_iw5_attachment_slots(base_id, &selection.attachments)
                .ok()
                .and_then(|native| {
                    self.registry
                        .compose_iw5_configuration(base_id, native, false)
                });
            match row {
                Some(row) => prepared.push((base_id, selection, row)),
                None => census.refused.push(selection),
            }
        }
        census.prepared = prepared.len();
        for (base_id, selection, mut row) in prepared {
            row.prepared_attachments = selection.attachments.clone();
            self.registry
                .configurations
                .insert(selection, self.registry.rows.len() as u32);
            let slots = self
                .combat_slots
                .get(base_id as usize)
                .copied()
                .unwrap_or_else(CombatFxSlots::default);
            self.combat_slots
                .resize(self.registry.rows.len(), CombatFxSlots::default());
            self.combat_slots.push(slots);
            self.registry.rows.push(row);
        }
        let primary_count = self.registry.rows.len();
        for parent in 1..primary_count {
            let Some((base, native)) = self.registry.rows[parent].iw5_configuration else {
                continue;
            };
            let Some(mut alternate) = self.registry.compose_iw5_configuration(base, native, true)
            else {
                continue;
            };
            let index = self.registry.rows.len() as u32;
            let primary = &self.registry.rows[parent];
            let underbarrel = native.underbarrel as usize + 5;
            let shared = self.registry.rows[base as usize].iw5_attachment_slots[underbarrel]
                .as_deref()
                .and_then(|name| self.registry.iw5_attachments.get(name))
                .is_some_and(|a| a.share_ammo_with_alt);
            alternate.facts.ammo_index = if shared {
                if primary.facts.ammo_index != 0 {
                    primary.facts.ammo_index
                } else {
                    parent as i32
                }
            } else {
                (parent as i32) | 0x1000000
            };
            alternate.facts.clip_index = if shared {
                if primary.facts.clip_index != 0 {
                    primary.facts.clip_index
                } else {
                    parent as i32
                }
            } else {
                (parent as i32) | 0x1000000
            };
            alternate.prepared_attachments = primary.prepared_attachments.clone();
            alternate.alternate_weapon = None;
            alternate.alternate_index = 0;
            self.registry.rows[parent].alternate_index = index;
            self.combat_slots.push(
                self.combat_slots
                    .get(base as usize)
                    .copied()
                    .unwrap_or_default(),
            );
            self.registry.rows.push(alternate);
        }
        self.registry.rebuild_name_maps();
        self.registry.revision = mint_weapon_revision();
        census
    }

    pub fn absorb(&mut self, mut other: Self) {
        self.registry
            .vehicle_compass
            .extend(std::mem::take(&mut other.registry.vehicle_compass));
        self.registry
            .vehicle_accel
            .extend(std::mem::take(&mut other.registry.vehicle_accel));
        self.registry
            .vehicle_turrets
            .extend(std::mem::take(&mut other.registry.vehicle_turrets));
        if other.registry.is_empty() {
            self.registry
                .iw5_attachments
                .extend(other.registry.iw5_attachments);
            self.family_tables.extend(other.family_tables);
            return;
        }
        if self.registry.rows.is_empty() {
            other.registry.vehicle_turrets = std::mem::take(&mut self.registry.vehicle_turrets);
            other.registry.vehicle_compass = std::mem::take(&mut self.registry.vehicle_compass);
            other.registry.vehicle_accel = std::mem::take(&mut self.registry.vehicle_accel);
            *self = other;
            return;
        }
        self.registry
            .rows
            .extend(other.registry.rows.into_iter().skip(1));
        self.registry
            .iw5_attachments
            .extend(other.registry.iw5_attachments);
        self.combat_slots
            .extend(other.combat_slots.into_iter().skip(1));
        self.family_tables.extend(other.family_tables);
        self.registry.item_groups.extend(other.registry.item_groups);
        self.registry.rebuild_name_maps();
        self.registry.revision = mint_weapon_revision();
    }

    pub fn resolve_combat_fx(&mut self, fx: &crate::FxCatalog, tracers: &crate::TracerCatalog) {
        let n = self.registry.rows.len().min(self.combat_slots.len());
        for i in 1..n {
            let row = &mut self.registry.rows[i];
            let Some(namespace) = row.component_namespace(WeaponComponent::Effect) else {
                row.combat_fx.refuse_preparation();
                continue;
            };
            stamp_combat_fx(
                &mut row.combat_fx,
                self.combat_slots[i],
                namespace,
                fx,
                tracers,
            );
            row.preparation.bind_combat_effects(
                &mut row.combat_fx,
                self.combat_slots[i],
                fx,
                tracers,
            );
        }
    }

    pub fn resolve_hud_material_edges(&mut self, materials: &crate::MaterialDefinitions) {
        for row in &mut self.registry.rows {
            let overlay = row.preparation.bind_material(
                row.overlay_material.as_deref(),
                row.overlay_material_from_slot,
                materials,
            );
            if row.overlay_image.is_none()
                && let Some(index) = overlay.bound()
                && let Some(material) = materials.materials.get(index.order())
            {
                row.overlay_image = materials.hud_image_name(material).map(str::to_owned);
            }
            row.reticle.center_edge = row.preparation.bind_material(
                row.reticle.center_material.as_deref(),
                row.reticle.center_authored,
                materials,
            );
            row.reticle.side_edge = row.preparation.bind_material(
                row.reticle.side_material.as_deref(),
                row.reticle.side_authored,
                materials,
            );
            row.hud_material_edges = WeaponHudMaterialEdges {
                overlay,
                hud_icon: row.preparation.bind_material(
                    row.hud_icon.as_deref(),
                    row.hud_icon_from_slot,
                    materials,
                ),
                pickup_icon: row.preparation.bind_material(
                    row.pickup_icon.as_deref(),
                    row.pickup_icon_authored,
                    materials,
                ),
                kill_icon: row.preparation.bind_material(
                    row.kill_icon.as_deref(),
                    row.kill_icon_from_slot,
                    materials,
                ),
            };
        }
    }

    pub fn resolve_projectile_fx_edges(&mut self, fx: &crate::FxCatalog) {
        for row in &mut self.registry.rows {
            row.projectile_fx = WeaponProjectileFx {
                trail: row.preparation.bind_fx(
                    fx,
                    row.proj_trail_from_slot,
                    row.proj_trail.as_deref(),
                ),
                beacon: row.preparation.bind_fx(
                    fx,
                    row.proj_beacon_from_slot,
                    row.proj_beacon.as_deref(),
                ),
                ignition: row.preparation.bind_fx(
                    fx,
                    row.proj_ignition_from_slot,
                    row.proj_ignition.as_deref(),
                ),
            };
        }
    }

    pub fn resolve_sz_xanim_edges(&mut self, xanims: &crate::XAnimCatalog) {
        let companions: Vec<_> = self
            .registry
            .rows
            .iter()
            .map(|row| {
                if !row.facts.dual_wield || xanims_idle(&row.sz_xanims).is_none() {
                    return None;
                }
                let name = row.dual_wield_weapon.as_deref()?;
                let id = self
                    .registry
                    .by_namespaced
                    .get(&(row.namespace, normalize_weapon_name(name)))?;
                let companion = &self.registry.rows[*id as usize];
                Some((
                    companion.gun_xmodel.clone(),
                    companion.sz_xanims_left.clone(),
                    companion.preparation.clone(),
                ))
            })
            .collect();
        for (row, companion) in self.registry.rows.iter_mut().zip(companions) {
            if let Some((gun, anims, preparation)) = companion {
                row.preparation.absorb_references(&preparation);
                row.secondary_gun_xmodel = gun;
                row.sz_xanims_right = row.sz_xanims.clone();
                row.sz_xanims_left = anims;
            }
        }
        for row in &mut self.registry.rows {
            let mut edges = [AssetEdge::Absent; WEAPON_ANIM_SLOTS];
            for (edge, hint) in edges.iter_mut().zip(row.sz_xanims.iter()) {
                *edge = row.preparation.bind_anim(xanims, hint.as_deref());
            }
            row.sz_xanim_edges = edges;
            row.sz_xanim_right_edges = std::array::from_fn(|slot| {
                row.preparation
                    .bind_anim(xanims, row.sz_xanims_right[slot].as_deref())
            });
            row.sz_xanim_left_edges = std::array::from_fn(|slot| {
                row.preparation
                    .bind_anim(xanims, row.sz_xanims_left[slot].as_deref())
            });
        }
    }

    pub fn time_t6_alternate_raises(&mut self, xanims: &crate::XAnimCatalog) -> usize {
        let mut timed = 0;
        for row in &mut self.registry.rows {
            if row.namespace != crate::AssetNamespace::T6 || row.alternate_weapon.is_none() {
                continue;
            }
            let Some(clip) = row.sz_xanim_edges[weap_anim::ALT_RAISE]
                .bound_index()
                .and_then(|index| xanims.clip_at(index))
            else {
                continue;
            };
            let clip_ms = (clip.duration() * 1000.0).round() as i32;
            if row.facts.alternate_raise_time_ms < clip_ms / 2 {
                row.facts.alternate_raise_time_ms = clip_ms;
                timed += 1;
            }
        }
        timed
    }

    pub fn resolve_notetrack_actions(&mut self, xanims: &crate::XAnimCatalog) -> (usize, usize) {
        let mut linked = 0;
        let mut inline = 0;
        for row in &mut self.registry.rows {
            let mut actions: HashMap<String, LinkedNotetrackAction> = HashMap::new();
            match row.sounds.notetrack_convention {
                NotetrackConvention::SoundMap => {
                    for (note, alias) in &row.sounds.notetrack_sound_map {
                        if !alias.is_empty() {
                            let action = actions.entry(note.to_ascii_lowercase()).or_default();
                            if action.sound_alias.is_none() {
                                action.sound_alias = Some(alias.clone());
                            }
                        }
                    }
                    for (note, alias) in &row.sounds.notetrack_rumble_map {
                        if !alias.is_empty() {
                            let action = actions.entry(note.to_ascii_lowercase()).or_default();
                            if action.rumble_alias.is_none() {
                                action.rumble_alias = Some(alias.clone());
                            }
                        }
                    }
                }
                NotetrackConvention::InlinePrefix => {
                    let indices: std::collections::BTreeSet<_> = row
                        .sz_xanim_edges
                        .iter()
                        .chain(&row.sz_xanim_right_edges)
                        .chain(&row.sz_xanim_left_edges)
                        .filter_map(|edge| edge.bound_index())
                        .collect();
                    for index in indices {
                        let Some(clip) = xanims.clip_at(index) else {
                            continue;
                        };
                        for notify in &clip.notifies {
                            let sound = t5_inline_note_alias(&notify.name, T5_NOTE_SOUND_PREFIX);
                            let rumble = t5_inline_note_alias(&notify.name, T5_NOTE_RUMBLE_PREFIX);
                            if sound.is_none() && rumble.is_none() {
                                continue;
                            }
                            let action =
                                actions.entry(notify.name.to_ascii_lowercase()).or_default();
                            if let Some(alias) = sound {
                                row.preparation
                                    .declare_notetrack_sound(xanims.name_at(index), alias);
                            }
                            action.sound_alias = sound.map(str::to_owned);
                            action.rumble_alias = rumble.map(str::to_owned);
                        }
                    }
                }
            }
            linked += actions.len();
            if row.sounds.notetrack_convention == NotetrackConvention::InlinePrefix {
                inline += actions.len();
            }
            row.notetrack_actions = actions;
        }
        (linked, inline)
    }

    pub fn apply_stats_item_groups(&mut self, table: &crate::CapturedStringTable) {
        for id in 1..=self.len() as u32 {
            let Some(ns) = self.identity_namespace_of(id) else {
                continue;
            };
            let name = self.name_of(id).to_owned();
            if name.is_empty() {
                continue;
            }
            if let Some(group) = crate::item_group_for_weapon(table, &name) {
                self.registry
                    .item_groups
                    .insert((ns, name), group.to_owned());
            }
        }
    }

    pub fn prepare_t5_camouflages(
        &mut self,
        table: &crate::CapturedStringTable,
        choices: &crate::CapturedStringTable,
        materials: &mut asset_material::MaterialCatalog,
        fpv: &asset_model::FpvMeshBuild,
        world: &asset_model::WorldWeaponBuild,
    ) -> usize {
        let mut prepared = HashMap::new();
        let mut variants = HashMap::new();
        let mut count = 0;
        for row in &mut self.registry.rows {
            if row.namespace != crate::AssetNamespace::T5 {
                continue;
            }
            let Some(parent) = row.skin_parent.as_deref() else {
                continue;
            };
            let key = (
                parent.to_owned(),
                row.gun_xmodel.clone(),
                row.world_model.clone(),
            );
            let camos = prepared.entry(key).or_insert_with(|| {
                let mut keys = Vec::new();
                if let Some(entry) = row
                    .gun_xmodel
                    .as_deref()
                    .and_then(|name| fpv.get(crate::AssetNamespace::T5, name))
                {
                    keys.extend(entry.material_keys.iter().flatten().cloned());
                }
                if let Some(entry) = row
                    .world_model
                    .as_deref()
                    .and_then(|name| world.get(crate::AssetNamespace::T5, name))
                {
                    keys.extend(entry.material_keys.iter().flatten().cloned());
                }
                keys.sort_by(|a, b| a.name.cmp(&b.name));
                keys.dedup();
                Arc::<[crate::WeaponCamouflage]>::from(crate::weapon_camo::prepare_t5(
                    table,
                    choices,
                    parent,
                    &keys,
                    materials,
                    &mut variants,
                ))
            });
            row.material_camos = Arc::clone(camos);
            count += usize::from(!camos.is_empty());
        }
        count
    }

    pub fn apply_stats_tables<'a>(
        &mut self,
        tables: impl IntoIterator<Item = &'a crate::CapturedStringTable>,
    ) {
        for table in tables {
            if crate::is_stats_table_name(&table.name) {
                self.apply_stats_item_groups(table);
            }
        }
    }

    pub fn stamp_projectile_model_edges(
        &mut self,
        catalog: &crate::ProjectileMeshCatalog,
        zone: ZoneOwner,
    ) {
        for row in &mut self.registry.rows {
            row.projectile_model_edge =
                row.preparation
                    .bind_projectile(row.projectile_model.as_deref(), catalog, zone);
        }
    }
}

pub fn overlay_name_is_hud_iris(name: &str) -> bool {
    !name.starts_with("mc/")
}

fn normalize_weapon_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_suffix("_mp").unwrap_or(&lower).to_owned()
}

pub fn gsc_weapon_script_name(catalog_bare: &str) -> String {
    if catalog_bare.is_empty() {
        return String::new();
    }
    if catalog_bare.ends_with("_mp") {
        catalog_bare.to_owned()
    } else {
        format!("{catalog_bare}_mp")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FpvAssemblyCensus {
    pub built: usize,
    pub linked: usize,
    pub refused: usize,
    pub clip_tables: usize,
}
