use crate::walk::{LoadedAsset, Ptr, ZoneLoad};

pub mod variant {
    pub const INTERNAL_NAME: u32 = 0;
    pub const WEAP_DEF: u32 = 8;
    pub const DISPLAY_NAME: u32 = 12;
    pub const ALT_WEAPON_NAME: u32 = 16;
    pub const XANIMS: u32 = 32;
    pub const XANIM_COUNT: u32 = 88;
    pub const ATTACH_VIEW_MODEL: u32 = 40;
    pub const ATTACH_WORLD_MODEL: u32 = 44;
    pub const ATTACH_VIEW_MODEL_OFFSETS: u32 = 56;
    pub const ATTACH_WORLD_MODEL_OFFSETS: u32 = 152;
    pub const ATTACH_VIEW_MODEL_ROTATIONS: u32 = 248;
    pub const ATTACH_WORLD_MODEL_ROTATIONS: u32 = 344;
    pub const ATTACH_MODEL_COUNT: u32 = 8;
    pub const ATTACHMENT_UNIQUES: u32 = 28;
    pub const ATTACHMENTS: u32 = 24;
    pub const ATTACHMENT_COUNT: u32 = 63;
    pub const ATTACHMENT_UNIQUE_COUNT: u32 = 95;
    pub const CLIP_SIZE: u32 = 476;
    pub const RELOAD_TIME: u32 = 480;
    pub const RELOAD_EMPTY_TIME: u32 = 484;
    pub const RELOAD_QUICK_TIME: u32 = 488;
    pub const RELOAD_QUICK_EMPTY_TIME: u32 = 492;
    pub const ADS_TRANS_IN_TIME: u32 = 496;
    pub const ADS_TRANS_OUT_TIME: u32 = 500;
    pub const ALT_RAISE_TIME: u32 = 504;
    pub const OVERLAY_MATERIAL: u32 = 596;
    pub const ADS_VIEW_KICK_CENTER_SPEED: u32 = 540;
    pub const HIP_VIEW_KICK_CENTER_SPEED: u32 = 544;
    pub const ADS_ZOOM_FOV1: u32 = 548;
    pub const ADS_ZOOM_IN_FRAC: u32 = 560;
    pub const ADS_ZOOM_OUT_FRAC: u32 = 564;
    pub const SILENCED: u32 = 580;
    pub const DUAL_MAG: u32 = 581;
    pub const SIZE: usize = 716;
}

pub mod def {
    pub const GUN_XMODEL: u32 = 4;
    pub const HAND_XMODEL: u32 = 8;
    pub const PLAYER_ANIM_TYPE: u32 = 24;
    pub const WEAP_TYPE: u32 = 28;
    pub const WEAP_CLASS: u32 = 32;
    pub const PENETRATE_TYPE: u32 = 36;
    pub const IMPACT_TYPE: u32 = 40;
    pub const INVENTORY_TYPE: u32 = 44;
    pub const FIRE_TYPE: u32 = 48;
    pub const OFFHAND_CLASS: u32 = 96;
    pub const OFFHAND_SLOT: u32 = 100;
    pub const PULLBACK_SOUND: u32 = 168;
    pub const PULLBACK_SOUND_PLAYER: u32 = 172;
    pub const FIRE_SOUND: u32 = 176;
    pub const FIRE_SOUND_PLAYER: u32 = 180;
    pub const FIRE_LAST_SOUND: u32 = 224;
    pub const FIRE_LAST_SOUND_PLAYER: u32 = 228;
    pub const EMPTY_FIRE_SOUND: u32 = 232;
    pub const EMPTY_FIRE_SOUND_PLAYER: u32 = 236;
    pub const MELEE_SWIPE_SOUND: u32 = 248;
    pub const MELEE_SWIPE_SOUND_PLAYER: u32 = 252;
    pub const MELEE_HIT_SOUND: u32 = 256;
    pub const MELEE_MISS_SOUND: u32 = 260;
    pub const RAISE_SOUND: u32 = 384;
    pub const RAISE_SOUND_PLAYER: u32 = 388;
    pub const FIRST_RAISE_SOUND: u32 = 392;
    pub const FIRST_RAISE_SOUND_PLAYER: u32 = 396;
    pub const PUTAWAY_SOUND: u32 = 408;
    pub const PUTAWAY_SOUND_PLAYER: u32 = 412;
    pub const PROJ_EXPLOSION_SOUND: u32 = 1820;
    pub const WORLD_MODEL: u32 = 916;
    pub const ROCKET_MODEL: u32 = 924;
    pub const HUD_ICON: u32 = 940;
    pub const AMMO_COUNTER_CLIP: u32 = 964;
    pub const START_AMMO: u32 = 968;
    pub const MAX_AMMO: u32 = 972;
    pub const SHOT_COUNT: u32 = 976;
    pub const AMMO_COUNT_CLIP_RELATIVE: u32 = 993;
    pub const DAMAGE: u32 = 996;
    pub const DAMAGE_RANGE: u32 = 1020;
    pub const DAMAGE_STEPS: usize = 6;
    pub const MIN_PLAYER_DAMAGE: u32 = 1044;
    pub const PLAYER_DAMAGE: u32 = 1056;
    pub const MELEE_DAMAGE: u32 = 1060;
    pub const FIRE_DELAY: u32 = 1072;
    pub const MELEE_DELAY: u32 = 1076;
    pub const MELEE_CHARGE_DELAY: u32 = 1080;
    pub const DETONATE_DELAY: u32 = 1084;
    pub const FIRE_TIME: u32 = 1128;
    pub const LAST_FIRE_TIME: u32 = 1132;
    pub const RECHAMBER_TIME: u32 = 1136;
    pub const RECHAMBER_BOLT_TIME: u32 = 1140;
    pub const HOLD_FIRE_TIME: u32 = 1144;
    pub const DETONATE_TIME: u32 = 1148;
    pub const MELEE_TIME: u32 = 1152;
    pub const BURST_DELAY_TIME: u32 = 1156;
    pub const MELEE_CHARGE_TIME: u32 = 1160;
    pub const RELOAD_SHOW_ROCKET_TIME: u32 = 1172;
    pub const RELOAD_ADD_TIME: u32 = 1180;
    pub const RELOAD_EMPTY_ADD_TIME: u32 = 1184;
    pub const RELOAD_QUICK_ADD_TIME: u32 = 1188;
    pub const RELOAD_QUICK_EMPTY_ADD_TIME: u32 = 1192;
    pub const RELOAD_START_TIME: u32 = 1196;
    pub const RELOAD_START_ADD_TIME: u32 = 1200;
    pub const RELOAD_END_TIME: u32 = 1204;
    pub const DROP_TIME: u32 = 1208;
    pub const RAISE_TIME: u32 = 1212;
    pub const ALT_DROP_TIME: u32 = 1216;
    pub const QUICK_DROP_TIME: u32 = 1220;
    pub const QUICK_RAISE_TIME: u32 = 1224;
    pub const FIRST_RAISE_TIME: u32 = 1228;
    pub const FUSE_TIME: u32 = 1356;
    pub const NO_ADS_WHEN_MAG_EMPTY: u32 = 1373;
    pub const MOVE_SPEED_SCALE: u32 = 1416;
    pub const ADS_MOVE_SPEED_SCALE: u32 = 1420;
    pub const OVERLAY_RETICLE: u32 = 1428;
    pub const OVERLAY_INTERFACE: u32 = 1432;
    pub const OVERLAY_WIDTH: u32 = 1436;
    pub const OVERLAY_HEIGHT: u32 = 1440;
    pub const HIP_SPREAD_STAND_MIN: u32 = 1456;
    pub const HIP_RETICLE_SIDE_POS: u32 = 1504;
    pub const ADS_IDLE_AMOUNT: u32 = 1508;
    pub const HIP_IDLE_AMOUNT: u32 = 1512;
    pub const ADS_IDLE_SPEED: u32 = 1516;
    pub const HIP_IDLE_SPEED: u32 = 1520;
    pub const IDLE_CROUCH_FACTOR: u32 = 1524;
    pub const IDLE_PRONE_FACTOR: u32 = 1528;
    pub const GUN_MAX_PITCH: u32 = 1532;
    pub const GUN_MAX_YAW: u32 = 1536;
    pub const BOLT_ACTION: u32 = 1588;
    pub const AIM_DOWN_SIGHT: u32 = 1592;
    pub const RECHAMBER_WHILE_ADS: u32 = 1593;
    pub const COOK_OFF_HOLD: u32 = 1604;
    pub const ADS_FIRE_ONLY: u32 = 1608;
    pub const DUAL_WIELD: u32 = 1615;
    pub const RETRIEVABLE: u32 = 1618;
    pub const KILL_ICON: u32 = 1632;
    pub const KILL_ICON_RATIO: u32 = 1636;
    pub const FLIP_KILL_ICON: u32 = 1640;
    pub const NO_PARTIAL_RELOAD: u32 = 1641;
    pub const SEGMENTED_RELOAD: u32 = 1642;
    pub const RELOAD_AMMO_ADD: u32 = 1644;
    pub const RELOAD_START_ADD: u32 = 1648;
    pub const IS_TACTICAL_INSERTION: u32 = 1700;
    pub const EXPLOSION_RADIUS: u32 = 1708;
    pub const EXPLOSION_RADIUS_MIN: u32 = 1712;
    pub const EXPLOSION_INNER_DAMAGE: u32 = 1720;
    pub const EXPLOSION_OUTER_DAMAGE: u32 = 1724;
    pub const DAMAGE_CONE_ANGLE: u32 = 1728;
    pub const PROJECTILE_SPEED: u32 = 1732;
    pub const PROJECTILE_SPEED_UP: u32 = 1736;
    pub const PROJECTILE_SPEED_RELATIVE_UP: u32 = 1740;
    pub const PROJECTILE_SPEED_FORWARD: u32 = 1744;
    pub const PROJECTILE_ACTIVATE_DIST: u32 = 1752;
    pub const PROJECTILE_MODEL: u32 = 1768;
    pub const PROJ_EXPLOSION: u32 = 1772;
    pub const PROJ_EXPLOSION_EFFECT: u32 = 1776;
    pub const PROJ_IMPACT_EXPLODE: u32 = 1836;
    pub const STICKINESS: u32 = 1840;
    pub const PLANTABLE: u32 = 1848;
    pub const HAS_DETONATOR: u32 = 1849;
    pub const TIMED_DETONATION: u32 = 1850;
    pub const HOLD_BUTTON_TO_THROW: u32 = 1854;
    pub const OFFHAND_HOLD_IS_CANCELABLE: u32 = 1855;
    pub const USE_AS_MELEE: u32 = 1872;
    pub const PARALLEL_BOUNCE: u32 = 1880;
    pub const PERPENDICULAR_BOUNCE: u32 = 1884;
    pub const SURF_TYPE_COUNT: usize = 32;
    pub const ADS_GUN_KICK_REDUCED_KICK_BULLETS: u32 = 1936;
    pub const ADS_GUN_KICK_REDUCED_KICK_PERCENT: u32 = 1940;
    pub const ADS_GUN_KICK_PITCH_MIN: u32 = 1944;
    pub const ADS_GUN_KICK_PITCH_MAX: u32 = 1948;
    pub const ADS_GUN_KICK_YAW_MIN: u32 = 1952;
    pub const ADS_GUN_KICK_YAW_MAX: u32 = 1956;
    pub const ADS_GUN_KICK_ACCEL: u32 = 1960;
    pub const ADS_GUN_KICK_SPEED_MAX: u32 = 1964;
    pub const ADS_GUN_KICK_SPEED_DECAY: u32 = 1968;
    pub const ADS_GUN_KICK_STATIC_DECAY: u32 = 1972;
    pub const ADS_VIEW_KICK_PITCH_MIN: u32 = 1976;
    pub const ADS_VIEW_KICK_PITCH_MAX: u32 = 1980;
    pub const ADS_VIEW_KICK_YAW_MIN: u32 = 1988;
    pub const ADS_VIEW_KICK_YAW_MAX: u32 = 1992;
    pub const ADS_VIEW_SCATTER_MIN: u32 = 2008;
    pub const ADS_VIEW_SCATTER_MAX: u32 = 2012;
    pub const ADS_SPREAD: u32 = 2016;
    pub const HIP_GUN_KICK_REDUCED_KICK_BULLETS: u32 = 2020;
    pub const HIP_GUN_KICK_REDUCED_KICK_PERCENT: u32 = 2024;
    pub const HIP_GUN_KICK_PITCH_MIN: u32 = 2028;
    pub const HIP_GUN_KICK_PITCH_MAX: u32 = 2032;
    pub const HIP_GUN_KICK_YAW_MIN: u32 = 2036;
    pub const HIP_GUN_KICK_YAW_MAX: u32 = 2040;
    pub const HIP_GUN_KICK_ACCEL: u32 = 2044;
    pub const HIP_GUN_KICK_SPEED_MAX: u32 = 2048;
    pub const HIP_GUN_KICK_SPEED_DECAY: u32 = 2052;
    pub const HIP_GUN_KICK_STATIC_DECAY: u32 = 2056;
    pub const HIP_VIEW_KICK_PITCH_MIN: u32 = 2060;
    pub const HIP_VIEW_KICK_PITCH_MAX: u32 = 2064;
    pub const HIP_VIEW_KICK_YAW_MIN: u32 = 2072;
    pub const HIP_VIEW_KICK_YAW_MAX: u32 = 2076;
    pub const HIP_VIEW_SCATTER_MIN: u32 = 2080;
    pub const HIP_VIEW_SCATTER_MAX: u32 = 2084;
    pub const LOCATION_DAMAGE_MULTIPLIERS: u32 = 2300;
    pub const TRACER_TYPE: u32 = 2320;
    pub const SIZE: usize = 2448;
}

pub mod weap_anim {
    pub const IDLE: usize = 1;
    pub const EMPTY_IDLE: usize = 2;
    pub const FIRE_INTRO: usize = 3;
    pub const FIRE: usize = 4;
    pub const HOLD_FIRE: usize = 5;
    pub const LASTSHOT: usize = 6;
    pub const RECHAMBER: usize = 8;
    pub const MELEE: usize = 9;
    pub const MELEE_CHARGE: usize = 14;
    pub const RELOAD: usize = 16;
    pub const RELOAD_EMPTY: usize = 18;
    pub const RELOAD_START: usize = 19;
    pub const RELOAD_END: usize = 20;
    pub const RELOAD_QUICK: usize = 21;
    pub const RELOAD_QUICK_EMPTY: usize = 22;
    pub const RAISE: usize = 23;
    pub const FIRST_RAISE: usize = 24;
    pub const DROP: usize = 25;
    pub const ALT_RAISE: usize = 26;
    pub const ALT_DROP: usize = 27;
    pub const QUICK_RAISE: usize = 28;
    pub const QUICK_DROP: usize = 29;
    pub const EMPTY_RAISE: usize = 30;
    pub const EMPTY_DROP: usize = 31;
    pub const SPRINT_IN: usize = 32;
    pub const SPRINT_LOOP: usize = 33;
    pub const SPRINT_OUT: usize = 34;
    pub const DETONATE: usize = 58;
    pub const ADS_FIRE: usize = 61;
    pub const ADS_LASTSHOT: usize = 62;
    pub const ADS_RECHAMBER: usize = 64;
    pub const DW_LEFT_FIRE: usize = 78;
    pub const DW_LEFT_LASTSHOT: usize = 79;
    pub const DW_LEFT_IDLE: usize = 81;
    pub const DW_LEFT_EMPTY_IDLE: usize = 82;
    pub const DW_LEFT_RELOAD_EMPTY: usize = 83;
    pub const DW_LEFT_RELOAD: usize = 84;
    pub const ADS_UP: usize = 85;
    pub const ADS_DOWN: usize = 86;
}

pub mod unique {
    pub const NAME: u32 = 0;
    pub const TYPE: u32 = 4;
    pub const COMBINED_MASK: u32 = 16;
    pub const ALT_WEAPON_NAME: u32 = 20;
    pub const HIDE_TAGS: u32 = 36;
    pub const HIDE_TAG_COUNT: u32 = 32;
    pub const VIEW_MODEL: u32 = 40;
    pub const VIEW_MODEL_ADDITIONAL: u32 = 44;
    pub const VIEW_MODEL_ADS: u32 = 48;
    pub const WORLD_MODEL: u32 = 52;
    pub const WORLD_MODEL_ADDITIONAL: u32 = 56;
    pub const VIEW_MODEL_TAG: u32 = 60;
    pub const WORLD_MODEL_TAG: u32 = 64;
    pub const VIEW_MODEL_OFFSETS: u32 = 68;
    pub const WORLD_MODEL_OFFSETS: u32 = 80;
    pub const VIEW_MODEL_ROTATIONS: u32 = 92;
    pub const WORLD_MODEL_ROTATIONS: u32 = 104;
    pub const VIEW_MODEL_ADD_OFFSETS: u32 = 116;
    pub const WORLD_MODEL_ADD_OFFSETS: u32 = 128;
    pub const VIEW_MODEL_ADD_ROTATIONS: u32 = 140;
    pub const WORLD_MODEL_ADD_ROTATIONS: u32 = 152;
    pub const DISABLE_BASE_ATTACHMENT: u32 = 168;
    pub const DISABLE_BASE_CLIP: u32 = 169;
    pub const OVERLAY_MATERIAL: u32 = 196;
    pub const XANIMS: u32 = 232;
    pub const FIRE_SOUND: u32 = 256;
    pub const FIRE_SOUND_PLAYER: u32 = 260;
}

pub mod attachment {
    pub const NAME: u32 = 0;
    pub const TYPE: u32 = 8;
    pub const SILENCED: u32 = 44;
    pub const DUAL_MAG: u32 = 45;
    pub const LASER_SIGHT: u32 = 46;
    pub const SHARED_AMMO: u32 = 50;
    pub const DAMAGE_RANGE_SCALE: u32 = 52;
    pub const ADS_ZOOM_FOV: u32 = 56;
    pub const ADS_ZOOM_IN_FRAC: u32 = 68;
    pub const ADS_ZOOM_OUT_FRAC: u32 = 72;
    pub const ADS_TRANS_IN_TIME_SCALE: u32 = 76;
    pub const ADS_TRANS_OUT_TIME_SCALE: u32 = 80;
    pub const ADS_VIEW_KICK_CENTER_SPEED_SCALE: u32 = 92;
    pub const ADS_IDLE_AMOUNT_SCALE: u32 = 96;
    pub const ADS_MOVE_SPEED_SCALE: u32 = 156;
    pub const HIP_SPREAD_MIN_SCALE: u32 = 160;
    pub const HIP_SPREAD_MAX_SCALE: u32 = 164;
    pub const FIRE_TIME_SCALE: u32 = 188;
    pub const RELOAD_TIME_SCALES: u32 = 192;
    pub const CLIP_SIZE_SCALE: u32 = 232;
    pub const PERKS: u32 = 252;
}

#[derive(Clone, Copy)]
pub struct AttachmentView<'z> {
    load: &'z ZoneLoad,
    asset: &'z LoadedAsset,
}

impl<'z> AttachmentView<'z> {
    pub fn u32_at(&self, off: u32) -> u32 {
        let o = off as usize;
        self.asset
            .header
            .get(o..o + 4)
            .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn f32_at(&self, off: u32) -> f32 {
        f32::from_bits(self.u32_at(off))
    }

    pub fn flag(&self, off: u32) -> bool {
        self.asset.header.get(off as usize).is_some_and(|&b| b != 0)
    }

    pub fn name(&self) -> Option<&'z str> {
        let p = crate::walk::decode_ptr(self.u32_at(attachment::NAME))?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    pub fn attachment_type(&self) -> u32 {
        self.u32_at(attachment::TYPE)
    }
}

pub type UniqueModel<'z> = (&'z str, Option<&'z str>, [f32; 3], [f32; 3]);

#[derive(Clone, Copy)]
pub struct AttachmentUniqueView<'z> {
    load: &'z ZoneLoad,
    asset: &'z LoadedAsset,
}

impl<'z> AttachmentUniqueView<'z> {
    fn u32_at(&self, off: u32) -> u32 {
        let o = off as usize;
        self.asset
            .header
            .get(o..o + 4)
            .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    fn vec3(&self, off: u32) -> [f32; 3] {
        core::array::from_fn(|i| f32::from_bits(self.u32_at(off + 4 * i as u32)))
    }

    fn str_at(&self, off: u32) -> Option<&'z str> {
        let p = crate::walk::decode_ptr(self.u32_at(off))?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
            .filter(|name| !name.is_empty())
    }

    pub fn name(&self) -> Option<&'z str> {
        self.str_at(unique::NAME)
    }

    pub fn attachment_type(&self) -> u32 {
        self.u32_at(unique::TYPE)
    }

    pub fn combined_mask(&self) -> u32 {
        self.u32_at(unique::COMBINED_MASK)
    }

    pub fn alt_weapon(&self) -> Option<&'z str> {
        self.str_at(unique::ALT_WEAPON_NAME)
    }

    pub fn flag(&self, off: u32) -> bool {
        self.asset.header.get(off as usize).is_some_and(|&b| b != 0)
    }

    pub fn sound(&self, off: u32) -> Option<&'z str> {
        self.str_at(off)
    }

    fn model_name(&self, off: u32) -> Option<&'z str> {
        let model = &self.load.assets[self.asset.field(off)?];
        let p =
            crate::walk::decode_ptr(u32::from_le_bytes(model.header.get(0..4)?.try_into().ok()?))?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    pub fn models(&self, view: bool) -> [Option<UniqueModel<'z>>; 2] {
        use unique as u;
        let (model, add, tag, offsets, rotations, add_offsets, add_rotations) = if view {
            (
                u::VIEW_MODEL,
                u::VIEW_MODEL_ADDITIONAL,
                u::VIEW_MODEL_TAG,
                u::VIEW_MODEL_OFFSETS,
                u::VIEW_MODEL_ROTATIONS,
                u::VIEW_MODEL_ADD_OFFSETS,
                u::VIEW_MODEL_ADD_ROTATIONS,
            )
        } else {
            (
                u::WORLD_MODEL,
                u::WORLD_MODEL_ADDITIONAL,
                u::WORLD_MODEL_TAG,
                u::WORLD_MODEL_OFFSETS,
                u::WORLD_MODEL_ROTATIONS,
                u::WORLD_MODEL_ADD_OFFSETS,
                u::WORLD_MODEL_ADD_ROTATIONS,
            )
        };
        let tag = self.str_at(tag);
        [
            (model, offsets, rotations),
            (add, add_offsets, add_rotations),
        ]
        .map(|(model, offsets, rotations)| {
            Some((
                self.model_name(model)?,
                tag,
                self.vec3(offsets),
                self.vec3(rotations),
            ))
        })
    }

    pub fn asset_field_name(&self, off: u32) -> Option<&'z str> {
        self.model_name(off).filter(|name| !name.is_empty())
    }

    pub fn ads_model(&self) -> Option<UniqueModel<'z>> {
        use unique as u;
        Some((
            self.model_name(u::VIEW_MODEL_ADS)?,
            self.str_at(u::VIEW_MODEL_TAG),
            self.vec3(u::VIEW_MODEL_OFFSETS),
            self.vec3(u::VIEW_MODEL_ROTATIONS),
        ))
    }

    pub fn xanim(&self, slot: u32) -> Option<&'z str> {
        let arr = crate::walk::decode_ptr(self.u32_at(unique::XANIMS))?;
        let p = self.load.blocks.ptr_at(arr.at(4 * slot)).ok()??;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
            .filter(|name| !name.is_empty())
    }

    pub fn hide_tags(&self) -> impl Iterator<Item = &'z str> + 'z {
        let load = self.load;
        let arr = crate::walk::decode_ptr(self.u32_at(unique::HIDE_TAGS));
        (0..unique::HIDE_TAG_COUNT).filter_map(move |i| {
            let b = load.blocks.bytes(arr?.at(2 * i), 2).ok()?;
            let id = u16::from_le_bytes([b[0], b[1]]);
            (id != 0).then(|| load.script_string(id)).flatten()
        })
    }
}

pub mod weap_type {
    pub const BULLET: i32 = 0;
    pub const GRENADE: i32 = 1;
    pub const PROJECTILE: i32 = 2;
    pub const BINOCULARS: i32 = 3;
    pub const GAS: i32 = 4;
    pub const BOMB: i32 = 5;
    pub const MINE: i32 = 6;
    pub const MELEE: i32 = 7;
    pub const RIOTSHIELD: i32 = 8;
}

pub mod weap_class {
    pub const RIFLE: i32 = 0;
    pub const MG: i32 = 1;
    pub const SMG: i32 = 2;
    pub const SPREAD: i32 = 3;
    pub const PISTOL: i32 = 4;
    pub const GRENADE: i32 = 5;
    pub const ROCKETLAUNCHER: i32 = 6;
    pub const TURRET: i32 = 7;
    pub const NON_PLAYER: i32 = 8;
    pub const GAS: i32 = 9;
    pub const ITEM: i32 = 10;
    pub const MELEE: i32 = 11;
    pub const KILLSTREAK_ALT_STORED_WEAPON: i32 = 12;
    pub const PISTOL_SPREAD: i32 = 13;
}

pub mod inventory_type {
    pub const PRIMARY: i32 = 0;
    pub const OFFHAND: i32 = 1;
    pub const ITEM: i32 = 2;
    pub const ALTMODE: i32 = 3;
    pub const MELEE: i32 = 4;
    pub const DWLEFTHAND: i32 = 5;
}

pub mod offhand_slot {
    pub const NONE: i32 = 0;
    pub const LETHAL_GRENADE: i32 = 1;
    pub const TACTICAL_GRENADE: i32 = 2;
    pub const EQUIPMENT: i32 = 3;
    pub const SPECIFIC_USE: i32 = 4;
}

#[derive(Clone, Copy)]
pub struct WeaponView<'z> {
    load: &'z ZoneLoad,
    asset: &'z LoadedAsset,
    variant: &'z [u8],
    def: Option<Ptr>,
}

impl<'z> WeaponView<'z> {
    pub fn new(load: &'z ZoneLoad, asset: &'z LoadedAsset) -> Option<Self> {
        if asset.header.len() < variant::SIZE {
            return None;
        }
        let mut view = Self {
            load,
            asset,
            variant: &asset.header,
            def: None,
        };
        view.def = view.variant_ptr(variant::WEAP_DEF);
        Some(view)
    }

    fn variant_u32(&self, off: u32) -> u32 {
        let o = off as usize;
        u32::from_le_bytes(self.variant[o..o + 4].try_into().unwrap())
    }

    fn variant_ptr(&self, off: u32) -> Option<Ptr> {
        crate::walk::decode_ptr(self.variant_u32(off))
    }

    pub fn attachments(&self) -> impl Iterator<Item = AttachmentView<'z>> + 'z {
        let load = self.load;
        let arr = self.variant_ptr(variant::ATTACHMENTS);
        (0..variant::ATTACHMENT_COUNT).filter_map(move |i| {
            let asset = load.asset_at(arr?.at(4 * i))?;
            (asset.ty == crate::AssetType::Attachment).then_some(AttachmentView { load, asset })
        })
    }

    pub fn attachment_uniques(&self) -> impl Iterator<Item = AttachmentUniqueView<'z>> + 'z {
        let load = self.load;
        let arr = self.variant_ptr(variant::ATTACHMENT_UNIQUES);
        (0..variant::ATTACHMENT_UNIQUE_COUNT).filter_map(move |i| {
            let asset = load.asset_at(arr?.at(4 * i))?;
            (asset.ty == crate::AssetType::AttachmentUnique)
                .then_some(AttachmentUniqueView { load, asset })
        })
    }

    pub fn variant_asset_name(&self, off: u32) -> Option<&'z str> {
        let asset = &self.load.assets[self.asset.field(off)?];
        let p =
            crate::walk::decode_ptr(u32::from_le_bytes(asset.header.get(0..4)?.try_into().ok()?))?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
            .filter(|name| !name.is_empty())
    }

    pub fn has_def(&self) -> bool {
        self.def.is_some()
    }

    pub fn variant_i32(&self, off: u32) -> i32 {
        self.variant_u32(off) as i32
    }

    pub fn variant_f32(&self, off: u32) -> f32 {
        f32::from_bits(self.variant_u32(off))
    }

    pub fn variant_bool(&self, off: u32) -> bool {
        self.variant[off as usize] != 0
    }

    pub fn variant_str(&self, off: u32) -> Option<&'z str> {
        let p = self.variant_ptr(off)?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    fn def_bytes(&self, off: u32, len: usize) -> Option<&'z [u8]> {
        self.load.blocks.bytes(self.def?.at(off), len).ok()
    }

    pub fn def_i32(&self, off: u32) -> i32 {
        self.def_bytes(off, 4)
            .map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn def_f32(&self, off: u32) -> f32 {
        self.def_bytes(off, 4)
            .map_or(0.0, |b| f32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn def_bool(&self, off: u32) -> bool {
        self.def_bytes(off, 1).is_some_and(|b| b[0] != 0)
    }

    pub fn def_str(&self, off: u32) -> Option<&'z str> {
        let p = self.load.blocks.ptr_at(self.def?.at(off)).ok()??;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    pub fn def_f32_array<const N: usize>(&self, off: u32) -> Option<[f32; N]> {
        let p = self.load.blocks.ptr_at(self.def?.at(off)).ok()??;
        let bytes = self.load.blocks.bytes(p, 4 * N).ok()?;
        Some(core::array::from_fn(|i| {
            f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap())
        }))
    }

    pub fn name(&self) -> Option<&'z str> {
        self.variant_str(variant::INTERNAL_NAME)
    }

    pub fn def_loaded_asset_name(&self, off: u32) -> Option<&'z str> {
        let asset = self.load.asset_in(self.asset, self.def?.at(off))?;
        let p =
            crate::walk::decode_ptr(u32::from_le_bytes(asset.header.get(0..4)?.try_into().ok()?))?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
            .filter(|name| !name.is_empty())
    }

    pub fn def_asset_name(&self, off: u32) -> Option<&'z str> {
        asset_name(self.load, self.def?.at(off))
    }

    pub fn def_asset_array_name(&self, off: u32, index: u32) -> Option<&'z str> {
        let arr = self.load.blocks.ptr_at(self.def?.at(off)).ok()??;
        asset_name(self.load, arr.at(4 * index))
    }

    pub fn attached_model(&self, slot: u32, view: bool) -> Option<(&'z str, [f32; 3], [f32; 3])> {
        use variant as v;
        let (models, offsets, rotations) = if view {
            (
                v::ATTACH_VIEW_MODEL,
                v::ATTACH_VIEW_MODEL_OFFSETS,
                v::ATTACH_VIEW_MODEL_ROTATIONS,
            )
        } else {
            (
                v::ATTACH_WORLD_MODEL,
                v::ATTACH_WORLD_MODEL_OFFSETS,
                v::ATTACH_WORLD_MODEL_ROTATIONS,
            )
        };
        if slot >= v::ATTACH_MODEL_COUNT {
            return None;
        }
        let name = asset_name(self.load, self.variant_ptr(models)?.at(4 * slot))?;
        let vec3 =
            |base: u32| core::array::from_fn(|k| self.variant_f32(base + 12 * slot + 4 * k as u32));
        Some((name, vec3(offsets), vec3(rotations)))
    }

    pub fn xanim(&self, index: u32) -> Option<&'z str> {
        let arr = self.variant_ptr(variant::XANIMS)?;
        let p = self.load.blocks.ptr_at(arr.at(4 * index)).ok()??;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    pub fn damage_curve(&self) -> [(i32, f32); def::DAMAGE_STEPS] {
        core::array::from_fn(|i| {
            (
                self.def_i32(def::DAMAGE + 4 * i as u32),
                self.def_f32(def::DAMAGE_RANGE + 4 * i as u32),
            )
        })
    }
}

pub fn asset_name(load: &ZoneLoad, slot: Ptr) -> Option<&str> {
    let asset = load.asset_at(slot)?;
    let off = if asset.ty == crate::AssetType::Image {
        72
    } else {
        0
    };
    let raw = u32::from_le_bytes(asset.header.get(off..off + 4)?.try_into().ok()?);
    let p = crate::walk::decode_ptr(raw)?;
    load.blocks
        .cstr(p)
        .ok()
        .and_then(|b| core::str::from_utf8(b).ok())
}
