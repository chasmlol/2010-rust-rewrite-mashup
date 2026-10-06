use crate::EntityState;

pub const ET_EVENTS: i32 = 0x12;
pub const EVENT_SEQUENCE_MASK: i32 = 0x7ff;
pub const EVENT_SEQUENCE_WRAP_WINDOW: i32 = 0x200;
pub const EVENT_RING_LEN: i32 = 4;

pub const LOCAL_SOUND_ENTITY: i32 = 0x7ff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct EntityEventKind(pub i32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntityEventFact {
    pub name: &'static str,
    pub event: EntityEventKind,
}

macro_rules! entity_events {
    ($($name:ident = $value:literal;)*) => {
        impl EntityEventKind {
            $(pub const $name: Self = Self($value);)*

            pub const TAXONOMY: &'static [EntityEventFact] = &[
                $(EntityEventFact {
                    name: concat!("EV_", stringify!($name)),
                    event: EntityEventKind($value),
                },)*
            ];
        }
    };
}

entity_events! {
    NONE = 0x00;
    FOLIAGE_SOUND = 0x01;
    STOP_WEAPON_SOUND = 0x02;
    SOUND_ALIAS = 0x03;
    SOUND_ALIAS_AS_MASTER = 0x04;
    STOPSOUNDS = 0x05;
    STANCE_FORCE_STAND = 0x06;
    STANCE_FORCE_CROUCH = 0x07;
    STANCE_FORCE_PRONE = 0x08;
    ITEM_PICKUP = 0x0a;
    AMMO_PICKUP = 0x0b;
    NOAMMO = 0x0c;
    RESET_ADS = 0x11;
    RELOAD = 0x12;
    RELOAD_FROM_EMPTY = 0x13;
    RELOAD_START = 0x14;
    RELOAD_END = 0x15;
    RELOAD_ADDAMMO = 0x17;
    RAISE_WEAPON = 0x18;
    FIRST_RAISE_WEAPON = 0x19;
    PUTAWAY_WEAPON = 0x1a;
    WEAPON_ALT = 0x1b;
    PULLBACK_WEAPON = 0x1d;
    FIRE_WEAPON = 0x1e;
    FIRE_WEAPON_LASTSHOT = 0x1f;
    RECHAMBER_WEAPON = 0x21;
    EJECT_BRASS = 0x22;
    FIRE_WEAPON_LEFT = 0x23;
    FIRE_WEAPON_LASTSHOT_LEFT = 0x24;
    EJECT_BRASS_LEFT = 0x25;
    SV_FIRE_WEAPON = 0x2a;
    MELEE_SWIPE = 0x2e;
    FIRE_MELEE = 0x2f;
    PREP_OFFHAND = 0x30;
    USE_OFFHAND = 0x31;
    MELEE_HIT = 0x33;
    MELEE_MISS = 0x34;
    MELEE_BLOOD = 0x35;
    BULLET_HIT = 0x3a;
    BULLET_HIT_SHIELD = 0x3b;
    BULLET_HIT_EXPLODE = 0x3c;
    BULLET_HIT_CLIENT_SMALL = 0x3d;
    BULLET_HIT_CLIENT_LARGE = 0x3e;
    BULLET_HIT_CLIENT_EXPLODE = 0x3f;
    BULLET_HIT_CLIENT_SHIELD = 0x40;
    EXPLOSIVE_IMPACT_ON_SHIELD = 0x41;
    EXPLOSIVE_SPLASH_ON_SHIELD = 0x42;
    GRENADE_BOUNCE = 0x43;
    GRENADE_STICK = 0x44;
    GRENADE_REST = 0x45;
    GRENADE_EXPLODE = 0x46;
    ROCKET_EXPLODE = 0x49;
    ROCKET_EXPLODE_NOMARKS = 0x4a;
    FLASHBANG_EXPLODE = 0x4b;
    PLAY_FX = 0x53;
    PLAY_FX_ON_TAG = 0x54;
    STOP_FX_ON_TAG = 0x55;
    PHYS_EXPLOSION_SPHERE = 0x56;
    DETONATE = 0x5e;
    OBITUARY = 0x62;
    FOOTSTEP_SPRINT = 0x6b;
    FOOTSTEP_RUN = 0x6c;
    FOOTSTEP_WALK = 0x6d;
    FOOTSTEP_PRONE = 0x6e;
    JUMP = 0x6f;
    LANDING_DEFAULT = 0x70;
    LANDING_BARK = 0x71;
    LANDING_BRICK = 0x72;
    LANDING_CARPET = 0x73;
    LANDING_CLOTH = 0x74;
    LANDING_CONCRETE = 0x75;
    LANDING_DIRT = 0x76;
    LANDING_FLESH = 0x77;
    LANDING_FOLIAGE = 0x78;
    LANDING_GLASS = 0x79;
    LANDING_GRASS = 0x7a;
    LANDING_GRAVEL = 0x7b;
    LANDING_ICE = 0x7c;
    LANDING_METAL = 0x7d;
    LANDING_MUD = 0x7e;
    LANDING_PAPER = 0x7f;
    LANDING_PLASTER = 0x80;
    LANDING_ROCK = 0x81;
    LANDING_SAND = 0x82;
    LANDING_SNOW = 0x83;
    LANDING_WATER = 0x84;
    LANDING_WOOD = 0x85;
    LANDING_ASPHALT = 0x86;
    LANDING_CERAMIC = 0x87;
    LANDING_PLASTIC = 0x88;
    LANDING_RUBBER = 0x89;
    LANDING_CUSHION = 0x8a;
    LANDING_FRUIT = 0x8b;
    LANDING_PAINTEDMETAL = 0x8c;
    LANDING_RIOTSHIELD = 0x8d;
    LANDING_SLUSH = 0x8e;
    LANDING_PAIN_DEFAULT = 0x8f;
    LANDING_PAIN_BARK = 0x90;
    LANDING_PAIN_BRICK = 0x91;
    LANDING_PAIN_CARPET = 0x92;
    LANDING_PAIN_CLOTH = 0x93;
    LANDING_PAIN_CONCRETE = 0x94;
    LANDING_PAIN_DIRT = 0x95;
    LANDING_PAIN_FLESH = 0x96;
    LANDING_PAIN_FOLIAGE = 0x97;
    LANDING_PAIN_GLASS = 0x98;
    LANDING_PAIN_GRASS = 0x99;
    LANDING_PAIN_GRAVEL = 0x9a;
    LANDING_PAIN_ICE = 0x9b;
    LANDING_PAIN_METAL = 0x9c;
    LANDING_PAIN_MUD = 0x9d;
    LANDING_PAIN_PAPER = 0x9e;
    LANDING_PAIN_PLASTER = 0x9f;
    LANDING_PAIN_ROCK = 0xa0;
    LANDING_PAIN_SAND = 0xa1;
    LANDING_PAIN_SNOW = 0xa2;
    LANDING_PAIN_WATER = 0xa3;
    LANDING_PAIN_WOOD = 0xa4;
    LANDING_PAIN_ASPHALT = 0xa5;
    LANDING_PAIN_CERAMIC = 0xa6;
    LANDING_PAIN_PLASTIC = 0xa7;
    LANDING_PAIN_RUBBER = 0xa8;
    LANDING_PAIN_CUSHION = 0xa9;
    LANDING_PAIN_FRUIT = 0xaa;
    LANDING_PAIN_PAINTEDMETAL = 0xab;
    LANDING_PAIN_RIOTSHIELD = 0xac;
    LANDING_PAIN_SLUSH = 0xad;
    MANTLE = 0xae;
    PLAY_RUMBLE_ON_ENT = 0xb1;
    PLAY_RUMBLE_ON_POS = 0xb2;
    STOP_RUMBLE = 0xb5;
}

impl EntityEventKind {
    pub const LANDING_FIRST: Self = Self::LANDING_DEFAULT;

    pub const LANDING_LAST: Self = Self::LANDING_SLUSH;

    pub const LANDING_PAIN_FIRST: Self = Self::LANDING_PAIN_DEFAULT;

    pub const LANDING_PAIN_LAST: Self = Self::LANDING_PAIN_SLUSH;

    pub const fn from_event_entity_type(e_type: i32) -> Option<Self> {
        if e_type >= ET_EVENTS && e_type <= ET_EVENTS + Self::STOP_RUMBLE.0 {
            Some(Self(e_type - ET_EVENTS))
        } else {
            None
        }
    }

    pub const fn event_entity_type(self) -> Option<i32> {
        if self.0 > Self::NONE.0 && self.0 <= Self::STOP_RUMBLE.0 {
            Some(ET_EVENTS + self.0)
        } else {
            None
        }
    }

    pub const fn landing_surface_index(self) -> Option<i32> {
        if self.0 >= Self::LANDING_FIRST.0 && self.0 <= Self::LANDING_LAST.0 {
            Some(self.0 - Self::LANDING_FIRST.0)
        } else if self.0 >= Self::LANDING_PAIN_FIRST.0 && self.0 <= Self::LANDING_PAIN_LAST.0 {
            Some(self.0 - Self::LANDING_PAIN_FIRST.0)
        } else {
            None
        }
    }

    pub fn taxonomy_name(self) -> Option<&'static str> {
        Self::TAXONOMY
            .iter()
            .find(|fact| fact.event.0 == self.0)
            .map(|fact| fact.name)
    }

    pub const fn is_landing_pain(self) -> bool {
        self.0 >= Self::LANDING_PAIN_FIRST.0 && self.0 <= Self::LANDING_PAIN_LAST.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityEventAction {
    None,
    OwnerStance,
    Sound,
    WeaponFire,
    EjectBrass,
    BulletHit,
    GrenadeContact,
    Explosion,
    PhysicsSphere,
    Rumble,
    PlayFx,
    Obituary,
    MovementSound,

    ResetAds,

    MeleeBlood,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedEntityEvent(pub EntityEventKind);

pub fn entity_event_action(
    event: EntityEventKind,
) -> Result<EntityEventAction, UnsupportedEntityEvent> {
    Ok(match event {
        EntityEventKind::NONE => EntityEventAction::None,
        EntityEventKind::STANCE_FORCE_STAND
        | EntityEventKind::STANCE_FORCE_CROUCH
        | EntityEventKind::STANCE_FORCE_PRONE => EntityEventAction::OwnerStance,
        EntityEventKind::RESET_ADS => EntityEventAction::ResetAds,
        EntityEventKind::FOLIAGE_SOUND
        | EntityEventKind::STOP_WEAPON_SOUND
        | EntityEventKind::SOUND_ALIAS
        | EntityEventKind::SOUND_ALIAS_AS_MASTER
        | EntityEventKind::STOPSOUNDS
        | EntityEventKind::ITEM_PICKUP
        | EntityEventKind::AMMO_PICKUP
        | EntityEventKind::NOAMMO
        | EntityEventKind::RELOAD
        | EntityEventKind::RELOAD_FROM_EMPTY
        | EntityEventKind::RELOAD_START
        | EntityEventKind::RELOAD_END
        | EntityEventKind::RAISE_WEAPON
        | EntityEventKind::FIRST_RAISE_WEAPON
        | EntityEventKind::PUTAWAY_WEAPON
        | EntityEventKind::WEAPON_ALT
        | EntityEventKind::PULLBACK_WEAPON
        | EntityEventKind::RECHAMBER_WEAPON
        | EntityEventKind::PREP_OFFHAND
        | EntityEventKind::USE_OFFHAND
        | EntityEventKind::DETONATE
        | EntityEventKind::MELEE_SWIPE
        | EntityEventKind::MELEE_HIT
        | EntityEventKind::MELEE_MISS => EntityEventAction::Sound,
        EntityEventKind::MELEE_BLOOD => EntityEventAction::MeleeBlood,
        EntityEventKind::FIRE_WEAPON
        | EntityEventKind::FIRE_WEAPON_LASTSHOT
        | EntityEventKind::FIRE_WEAPON_LEFT
        | EntityEventKind::FIRE_WEAPON_LASTSHOT_LEFT
        | EntityEventKind::SV_FIRE_WEAPON => EntityEventAction::WeaponFire,
        EntityEventKind::EJECT_BRASS | EntityEventKind::EJECT_BRASS_LEFT => {
            EntityEventAction::EjectBrass
        }
        EntityEventKind::BULLET_HIT
        | EntityEventKind::BULLET_HIT_SHIELD
        | EntityEventKind::BULLET_HIT_EXPLODE
        | EntityEventKind::BULLET_HIT_CLIENT_SMALL
        | EntityEventKind::BULLET_HIT_CLIENT_LARGE
        | EntityEventKind::BULLET_HIT_CLIENT_EXPLODE
        | EntityEventKind::BULLET_HIT_CLIENT_SHIELD
        | EntityEventKind::EXPLOSIVE_IMPACT_ON_SHIELD
        | EntityEventKind::EXPLOSIVE_SPLASH_ON_SHIELD => EntityEventAction::BulletHit,
        EntityEventKind::GRENADE_BOUNCE
        | EntityEventKind::GRENADE_STICK
        | EntityEventKind::GRENADE_REST => EntityEventAction::GrenadeContact,
        EntityEventKind::GRENADE_EXPLODE
        | EntityEventKind::ROCKET_EXPLODE
        | EntityEventKind::ROCKET_EXPLODE_NOMARKS
        | EntityEventKind::FLASHBANG_EXPLODE => EntityEventAction::Explosion,
        EntityEventKind::PLAY_RUMBLE_ON_ENT
        | EntityEventKind::PLAY_RUMBLE_ON_POS
        | EntityEventKind::STOP_RUMBLE => EntityEventAction::Rumble,
        EntityEventKind::PHYS_EXPLOSION_SPHERE => EntityEventAction::PhysicsSphere,
        EntityEventKind::PLAY_FX
        | EntityEventKind::PLAY_FX_ON_TAG
        | EntityEventKind::STOP_FX_ON_TAG => EntityEventAction::PlayFx,
        EntityEventKind::OBITUARY => EntityEventAction::Obituary,
        EntityEventKind::FOOTSTEP_SPRINT
        | EntityEventKind::FOOTSTEP_RUN
        | EntityEventKind::FOOTSTEP_WALK
        | EntityEventKind::FOOTSTEP_PRONE
        | EntityEventKind::JUMP
        | EntityEventKind::MANTLE => EntityEventAction::MovementSound,
        other => {
            if other.landing_surface_index().is_some() {
                EntityEventAction::MovementSound
            } else {
                return Err(UnsupportedEntityEvent(other));
            }
        }
    })
}

#[must_use]
pub const fn is_left_hand_fire_event(event: EntityEventKind) -> bool {
    matches!(event.0, 0x23 | 0x24 | 0x25 | 0x28 | 0x29 | 0x2c | 0x2d)
}

#[must_use]
pub const fn is_weapon_fire_last_shot_event(event: EntityEventKind) -> bool {
    matches!(event.0, 0x1f | 0x24 | 0x27 | 0x29 | 0x2b | 0x2d)
}

#[must_use]
pub const fn predicted_weapon_fire_event(hand: i32, last_shot: bool) -> EntityEventKind {
    match (hand != 0, last_shot) {
        (false, false) => EntityEventKind::FIRE_WEAPON,
        (false, true) => EntityEventKind::FIRE_WEAPON_LASTSHOT,
        (true, false) => EntityEventKind::FIRE_WEAPON_LEFT,
        (true, true) => EntityEventKind::FIRE_WEAPON_LASTSHOT_LEFT,
    }
}

pub const fn bullet_hit_event(impact_type: i32, local_client: bool) -> Option<EntityEventKind> {
    if local_client {
        match impact_type {
            1 | 9 => Some(EntityEventKind::BULLET_HIT_CLIENT_SMALL),
            2 => Some(EntityEventKind::BULLET_HIT_CLIENT_LARGE),
            3 => Some(EntityEventKind::BULLET_HIT_CLIENT_SHIELD),
            _ => None,
        }
    } else {
        match impact_type {
            1 | 2 | 9 => Some(EntityEventKind::BULLET_HIT),
            3 => Some(EntityEventKind::BULLET_HIT_SHIELD),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequencedEntityEvent {
    pub sequence: i32,
    pub event: EntityEventKind,
    pub event_parm: i32,
}

pub fn add_entity_event(state: &mut EntityState, event: EntityEventKind, event_parm: i32) {
    if event == EntityEventKind::NONE {
        return;
    }
    let slot = (state.event_sequence & 3) as usize;
    state.events[slot] = event.0;
    state.event_parms[slot] = event_parm;
    state.event_sequence = state.event_sequence.wrapping_add(1) & EVENT_SEQUENCE_MASK;
}

pub fn packet_entity_uses_event_ring(e_type: i32) -> bool {
    e_type != 8 && e_type != 9 && e_type < ET_EVENTS
}

pub fn consume_entity_events(
    state: &EntityState,
    consumed_sequence: &mut i32,
    mut dispatch: impl FnMut(SequencedEntityEvent),
) {
    let next = state.event_sequence;
    if *consumed_sequence == next {
        return;
    }
    if next == 0 {
        *consumed_sequence = 0;
        return;
    }

    let mut consumed = *consumed_sequence;
    if next + EVENT_SEQUENCE_WRAP_WINDOW < consumed {
        consumed -= EVENT_SEQUENCE_MASK + 1;
    }
    if next - consumed > EVENT_RING_LEN {
        consumed = next - EVENT_RING_LEN;
    }

    while consumed < next {
        let slot = (consumed as u32 & 3) as usize;
        dispatch(SequencedEntityEvent {
            sequence: consumed & EVENT_SEQUENCE_MASK,
            event: EntityEventKind(state.events[slot]),
            event_parm: state.event_parms[slot],
        });
        consumed += 1;
    }
    *consumed_sequence = next;
}
