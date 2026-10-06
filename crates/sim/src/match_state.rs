use crate::identities::LifeSequence;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClientLifecycle {
    #[default]
    Connecting,
    ChoosingClass,

    SpawnPending,
    Alive,
    Dead,

    RespawnPending,
    Spectating,

    Intermission,
}

mod client_view;
mod events;
mod loadout;
mod snapshot_meta;

pub use client_view::{
    KillcamHud, LinkedWeaponView, LocationSelection, MENU_COMMAND_TAIL, MenuCommand,
    MenuCommandKind, RadarMode, RemoteMissile, ScriptBlur, ScriptDepthOfField, ScriptSeat,
    ViewEffects, VisionChange, is_postfx_dvar,
};
pub use events::{
    EntityEventPayload, EntityEventRecord, EventAudience, EventRecord, PelletFxRecord,
    SIM_EVENT_ROSTER, SimEvent, SimEventRow, UNRELIABLE_SIM_EVENT_COUNT, sim_event_is_reliable,
};
pub use loadout::{
    CLASS_CATALOG_DEATHSTREAKS, CLASS_CATALOG_PERKS, ClassDef, ClassRejectReason,
    ConfigurationChangeRejectReason, GiveRejectReason, IW4_CAMOS, LoadoutSpec,
    PERSONAL_CLASS_SLOTS, PersonalClass, iw4_camo_index,
};
pub fn class_catalog_perk_name(id: u32) -> Option<&'static str> {
    CLASS_CATALOG_PERKS
        .get(id.checked_sub(1)? as usize)
        .copied()
}

pub use snapshot_meta::{
    ClientSnapshotMeta, DroppedItemAmmo, ItemPickupRecord, RngDebugMeta, ScriptDvars, SnapshotMeta,
    TargetBoxDvar,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchEndReason {
    ScoreLimit,
    TimeLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthRegenCensus {
    pub old_health: i32,
    pub hurt_time_ms: i32,
    pub very_hurt: bool,

    pub named_sound: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputReceipt {
    pub applied_cmds: u32,
    pub moving_cmds: u32,
    pub path_units: f32,
}

impl InputReceipt {
    pub(crate) fn record(&mut self, moving: bool, from: [f32; 3], to: [f32; 3]) {
        self.applied_cmds = self.applied_cmds.wrapping_add(1);
        if moving {
            self.moving_cmds = self.moving_cmds.wrapping_add(1);
        }
        let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let step = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if step.is_finite() {
            self.path_units += step;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientMatchState {
    pub shield: Option<crate::ShieldAttachment>,
    pub weapon_lock: crate::WeaponLock,
    pub lifecycle: ClientLifecycle,
    pub god_mode: bool,
    pub input_receipt: InputReceipt,
    pub loadout: Option<LoadoutSpec>,
    pub life_sequence: LifeSequence,

    pub item_use_spawn_ms: i32,
    pub item_use_entity: Option<crate::EntityRef>,
    pub item_use_press_ms: i32,

    pub(crate) ammo_clip: i32,

    pub(crate) ammo_stock: i32,

    pub(crate) ammo_by_weapon: Vec<(u32, i32, i32)>,

    pub(crate) taped_mag_spent: Vec<u32>,

    pub(crate) weapon_shot_count: u8,
    pub(crate) burst_latch: bool,
    pub(crate) burst_latch_secondary: bool,

    pub(crate) rechamber_pending: bool,
    pub(crate) rechamber_pending_secondary: bool,
    pub(crate) pending_brass: [Option<crate::PendingBrass>; 2],

    pub(crate) dead_since_tick: Option<u32>,

    pub(crate) forced_spawn: Option<crate::SpawnPick>,

    pub(crate) look_at_killer_yaw: i32,

    pub(crate) name: [u8; 16],

    pub kills: i32,
    pub deaths: i32,
    pub score: i32,
    pub kill_streak: i32,
    pub radar: RadarMode,
    pub radar_blocked: bool,
    pub remote_missile: Option<RemoteMissile>,
    pub linked_weapon_view: Option<LinkedWeaponView>,

    pub(crate) ffa_team: Option<u8>,

    pub(crate) client_state_team: i32,

    pub(crate) rank: i32,
    pub(crate) prestige: i32,
    pub(crate) player_card_icon: u32,
    pub(crate) player_card_title: u32,
    pub(crate) player_card_nameplate: u32,
    pub(crate) client_dvars: Vec<(String, String)>,
    pub(crate) shellshock: Option<hud_iw4::ShockParams>,
    pub(crate) view_effects: ViewEffects,
    pub(crate) menu_commands: Vec<MenuCommand>,
    pub(crate) location_selection: Option<LocationSelection>,

    pub(crate) controls: ScriptControls,

    pub(crate) max_health: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScriptControls {
    pub frozen: bool,
    pub weapons_disabled: bool,
    pub offhands_disabled: bool,
    pub switch_disabled: bool,
    pub jump_disabled: bool,
    pub usability_disabled: bool,
    pub linked: bool,
    pub switch_to: u32,
}

impl ClientMatchState {
    pub(crate) fn ammo_for(&self, weapon: u32) -> (i32, i32) {
        self.ammo_by_weapon
            .iter()
            .find(|(w, _, _)| *w == weapon)
            .map(|(_, c, s)| (*c, *s))
            .unwrap_or((0, 0))
    }

    pub(crate) fn set_ammo(&mut self, weapon: u32, clip: i32, stock: i32) {
        if weapon == 0 {
            return;
        }
        if let Some(row) = self
            .ammo_by_weapon
            .iter_mut()
            .find(|(w, _, _)| *w == weapon)
        {
            row.1 = clip;
            row.2 = stock;
            return;
        }
        self.ammo_by_weapon.push((weapon, clip, stock));
    }

    pub(crate) fn quick_reload_ready(&self, weapon: u32) -> bool {
        !self.taped_mag_spent.contains(&weapon)
    }

    pub(crate) fn set_quick_reload_ready(&mut self, weapon: u32, ready: bool) {
        self.taped_mag_spent.retain(|&w| w != weapon);
        if !ready && weapon != 0 {
            self.taped_mag_spent.push(weapon);
        }
    }

    pub(crate) fn clear_ammo_inventory(&mut self) {
        self.ammo_by_weapon.clear();
        self.taped_mag_spent.clear();
        self.ammo_clip = 0;
        self.ammo_stock = 0;
    }

    pub(crate) fn mirror_held_ammo(&mut self, weapon: u32) {
        let (clip, stock) = self.ammo_for(weapon);
        self.ammo_clip = clip;
        self.ammo_stock = stock;
    }

    pub(crate) fn to_snapshot_meta(&self) -> ClientSnapshotMeta {
        ClientSnapshotMeta {
            shield: self.shield,
            shield_collision: None,
            controls: self.controls,
            weapon_lock: self.weapon_lock,
            killcam_hud: None,
            lifecycle: self.lifecycle,
            god_mode: self.god_mode,
            loadout: self.loadout.clone(),
            life_sequence: self.life_sequence,
            item_use_spawn_ms: self.item_use_spawn_ms,
            item_use_entity: self.item_use_entity,
            item_use_press_ms: self.item_use_press_ms,
            ammo_clip: self.ammo_clip,
            ammo_stock: self.ammo_stock,
            score: self.score,
            kills: self.kills,
            deaths: self.deaths,
            kill_streak: self.kill_streak,
            radar: self.radar,
            radar_blocked: self.radar_blocked,
            remote_missile: self.remote_missile,
            linked_weapon_view: self.linked_weapon_view,
            ammo_by_weapon: self.ammo_by_weapon.clone(),
            taped_mag_spent: self.taped_mag_spent.clone(),
            weapon_shot_count: self.weapon_shot_count,
            burst_latch: self.burst_latch,
            burst_latch_secondary: self.burst_latch_secondary,
            rechamber_pending: self.rechamber_pending,
            rechamber_pending_secondary: self.rechamber_pending_secondary,
            pending_brass: self.pending_brass,
            dead_since_tick: self.dead_since_tick,
            look_at_killer_yaw: self.look_at_killer_yaw,
            name: self.name,
            hud_archival: Vec::new(),
            hud_current: Vec::new(),
            ffa_team: self.ffa_team,
            client_state_team: self.client_state_team,
            rank: self.rank,
            prestige: self.prestige,
            player_card_icon: self.player_card_icon,
            player_card_title: self.player_card_title,
            player_card_nameplate: self.player_card_nameplate,
            client_dvars: self.client_dvars.clone(),
            shellshock: self.shellshock.clone(),
            view_effects: self.view_effects.clone(),
            menu_commands: self.menu_commands.clone(),
            location_selection: self.location_selection.clone(),
        }
    }

    pub(crate) fn push_menu_command(&mut self, kind: MenuCommandKind) {
        let serial = self
            .menu_commands
            .last()
            .map_or(1, |c| c.serial.wrapping_add(1));
        self.menu_commands.push(MenuCommand { serial, kind });
        let excess = self.menu_commands.len().saturating_sub(MENU_COMMAND_TAIL);
        self.menu_commands.drain(..excess);
    }

    pub(crate) fn adopt_snapshot_meta(&mut self, meta: &ClientSnapshotMeta) {
        self.controls = meta.controls;
        self.shield = meta.shield;
        self.weapon_lock = meta.weapon_lock;
        self.lifecycle = meta.lifecycle;
        self.god_mode = meta.god_mode;
        self.loadout = meta.loadout.clone();
        self.life_sequence = meta.life_sequence;
        self.item_use_spawn_ms = meta.item_use_spawn_ms;
        self.item_use_entity = meta.item_use_entity;
        self.item_use_press_ms = meta.item_use_press_ms;
        self.ammo_clip = meta.ammo_clip;
        self.ammo_stock = meta.ammo_stock;
        self.score = meta.score;
        self.kills = meta.kills;
        self.deaths = meta.deaths;
        self.kill_streak = meta.kill_streak;
        self.radar = meta.radar;
        self.radar_blocked = meta.radar_blocked;
        self.remote_missile = meta.remote_missile;
        self.linked_weapon_view = meta.linked_weapon_view;
        self.ammo_by_weapon = meta.ammo_by_weapon.clone();
        self.taped_mag_spent = meta.taped_mag_spent.clone();
        self.weapon_shot_count = meta.weapon_shot_count;
        self.burst_latch = meta.burst_latch;
        self.burst_latch_secondary = meta.burst_latch_secondary;
        self.rechamber_pending = meta.rechamber_pending;
        self.rechamber_pending_secondary = meta.rechamber_pending_secondary;
        self.pending_brass = meta.pending_brass;
        self.dead_since_tick = meta.dead_since_tick;
        self.look_at_killer_yaw = meta.look_at_killer_yaw;
        self.name = meta.name;
        self.ffa_team = meta.ffa_team;
        self.client_state_team = meta.client_state_team;
        self.rank = meta.rank;
        self.prestige = meta.prestige;
        self.player_card_icon = meta.player_card_icon;
        self.player_card_title = meta.player_card_title;
        self.player_card_nameplate = meta.player_card_nameplate;
        self.client_dvars = meta.client_dvars.clone();
        self.shellshock = meta.shellshock.clone();
        self.view_effects = meta.view_effects.clone();
        self.menu_commands = meta.menu_commands.clone();
        self.location_selection = meta.location_selection.clone();
    }
}

pub fn perk_table_code_from_class_catalog(id: u32) -> u32 {
    match id {
        1 => 6,
        2 => 2,
        3 => 9,
        4 => 7,
        5 => 3,
        6 => 8,
        7 => 14,
        8 => 1,
        9 => 10,
        10 => 11,
        11 => 12,
        12 => 5,
        13 => 4,
        14 => 13,
        15 => 15,
        16 => 16,
        _ => 0,
    }
}

pub fn perk_slot_from_class_catalog(id: u32) -> Option<usize> {
    match id {
        2 | 5 | 8 | 12 | 13 => Some(0),
        1 | 3 | 4 | 6 | 9 => Some(1),
        7 | 10 | 11 | 14 | 15 | 16 => Some(2),
        _ => None,
    }
}
