use crate::input::ClassId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassRejectReason {
    UnknownOrStaleClass,

    NoSpawnAvailable,

    LockedContent,

    UnknownWeaponId,
}

impl ClassRejectReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownOrStaleClass => "unknown_or_stale_class",
            Self::NoSpawnAvailable => "no_spawn_available",
            Self::LockedContent => "locked_content",
            Self::UnknownWeaponId => "unknown_weapon_id",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiveRejectReason {
    NotAlive,

    InvalidWeapon,

    UnknownWeaponId,

    UnsupportedWeapon,

    EmptyCombatProfile,
}

impl GiveRejectReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAlive => "not_alive",
            Self::InvalidWeapon => "invalid_weapon",
            Self::UnknownWeaponId => "unknown_weapon_id",
            Self::UnsupportedWeapon => "unsupported_weapon",
            Self::EmptyCombatProfile => "empty_combat_profile",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigurationChangeRejectReason {
    NotAlive,
    StaleSource,
    InvalidTarget,
    DifferentFamily,
    Busy,
    NoInventorySlot,
    AmmoTableFull,
    SharedAmmoConflict,
}

impl ConfigurationChangeRejectReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAlive => "not_alive",
            Self::StaleSource => "stale_source",
            Self::InvalidTarget => "invalid_target",
            Self::DifferentFamily => "different_family",
            Self::Busy => "busy",
            Self::NoInventorySlot => "no_inventory_slot",
            Self::AmmoTableFull => "ammo_table_full",
            Self::SharedAmmoConflict => "shared_ammo_conflict",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadoutSpec {
    pub class_id: ClassId,
    pub revision: u32,
    pub primary: u32,
    pub secondary: u32,
    pub primary_attachments: [u32; 4],
    pub secondary_attachments: [u32; 4],
    pub lethal: u32,
    pub tactical: u32,

    pub perks: [u32; 3],
}

pub const PERSONAL_CLASS_SLOTS: usize = 5;

pub const CLASS_CATALOG_DEATHSTREAKS: [&str; 4] = [
    "specialty_copycat",
    "specialty_combathigh",
    "specialty_grenadepulldeath",
    "specialty_finalstand",
];

pub use weapon_iw4::IW4_CAMOS;

pub fn iw4_camo_index(name: &str) -> u8 {
    IW4_CAMOS
        .iter()
        .position(|camo| camo.eq_ignore_ascii_case(name))
        .map_or(0, |index| index as u8)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PersonalClass {
    pub weapons: [u32; 4],
    pub perks: [u32; 3],
    pub deathstreak: u8,
    pub camos: [u8; 2],
}

impl PersonalClass {
    pub fn definition(self, id: ClassId, revision: u32) -> Option<ClassDef> {
        if revision == 0 {
            return None;
        }
        let mut def = ClassDef::primary_secondary(id, revision, self.weapons[0], self.weapons[1]);
        def.lethal = self.weapons[2];
        def.tactical = self.weapons[3];
        def.camos = self.camos;
        def.perks = self.perks;
        def.deathstreak = if self.deathstreak == 0 {
            String::new()
        } else {
            CLASS_CATALOG_DEATHSTREAKS
                .get(self.deathstreak as usize - 1)?
                .to_string()
        };
        Some(def)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDef {
    pub id: ClassId,
    pub revision: u32,
    pub primary: u32,
    pub secondary: u32,

    pub primary_attachments: [u32; 4],
    pub secondary_attachments: [u32; 4],
    pub lethal: u32,
    pub tactical: u32,
    pub perks: [u32; 3],

    pub deathstreak: String,
    pub camos: [u8; 2],
    pub locked: bool,
}

impl ClassDef {
    pub fn primary_secondary(id: ClassId, revision: u32, primary: u32, secondary: u32) -> Self {
        Self {
            id,
            revision,
            primary,
            secondary,
            primary_attachments: [0; 4],
            secondary_attachments: [0; 4],
            lethal: 0,
            tactical: 0,
            perks: [0; 3],
            deathstreak: String::new(),
            camos: [0; 2],
            locked: false,
        }
    }

    pub fn weapon_slot_ids(&self) -> [u32; 4] {
        [self.primary, self.secondary, self.lethal, self.tactical]
    }
}

pub const CLASS_CATALOG_PERKS: [&str; 16] = [
    "specialty_bulletdamage",
    "specialty_fastreload",
    "specialty_coldblooded",
    "specialty_lightweight",
    "specialty_scavenger",
    "specialty_hardline",
    "specialty_heartbreaker",
    "specialty_marathon",
    "specialty_explosivedamage",
    "specialty_extendedmelee",
    "specialty_bulletaccuracy",
    "specialty_bling",
    "specialty_onemanarmy",
    "specialty_localjammer",
    "specialty_detectexplosive",
    "specialty_pistoldeath",
];
