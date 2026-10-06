use entity_iw4::{EntityEventAction, EntityEventKind};

macro_rules! land_ev {
    ($ident:ident) => {
        EntityEventRow {
            name: concat!("EV_", stringify!($ident)),
            event: EntityEventKind::$ident,
            dispatch: EntityEventDispatch::Observer(EntityEventAction::MovementSound),
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityEventDispatch {
    Observer(EntityEventAction),
    OwnerInput,

    Unsupported(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct EntityEventRow {
    pub name: &'static str,
    pub event: EntityEventKind,
    pub dispatch: EntityEventDispatch,
}

pub const EV_DISPATCH_REGISTRY: &[EntityEventRow] = &[
    EntityEventRow {
        name: "EV_NONE",
        event: EntityEventKind::NONE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::None),
    },
    EntityEventRow {
        name: "EV_FOLIAGE_SOUND",
        event: EntityEventKind::FOLIAGE_SOUND,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_STOP_WEAPON_SOUND",
        event: EntityEventKind::STOP_WEAPON_SOUND,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_SOUND_ALIAS",
        event: EntityEventKind::SOUND_ALIAS,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_SOUND_ALIAS_AS_MASTER",
        event: EntityEventKind::SOUND_ALIAS_AS_MASTER,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_STOPSOUNDS",
        event: EntityEventKind::STOPSOUNDS,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_STANCE_FORCE_STAND",
        event: EntityEventKind::STANCE_FORCE_STAND,
        dispatch: EntityEventDispatch::OwnerInput,
    },
    EntityEventRow {
        name: "EV_STANCE_FORCE_CROUCH",
        event: EntityEventKind::STANCE_FORCE_CROUCH,
        dispatch: EntityEventDispatch::OwnerInput,
    },
    EntityEventRow {
        name: "EV_STANCE_FORCE_PRONE",
        event: EntityEventKind::STANCE_FORCE_PRONE,
        dispatch: EntityEventDispatch::OwnerInput,
    },
    EntityEventRow {
        name: "EV_ITEM_PICKUP",
        event: EntityEventKind::ITEM_PICKUP,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_AMMO_PICKUP",
        event: EntityEventKind::AMMO_PICKUP,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_NOAMMO",
        event: EntityEventKind::NOAMMO,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_RESET_ADS",
        event: EntityEventKind::RESET_ADS,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::ResetAds),
    },
    EntityEventRow {
        name: "EV_RELOAD",
        event: EntityEventKind::RELOAD,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_RELOAD_FROM_EMPTY",
        event: EntityEventKind::RELOAD_FROM_EMPTY,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_RELOAD_START",
        event: EntityEventKind::RELOAD_START,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_RELOAD_END",
        event: EntityEventKind::RELOAD_END,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_RELOAD_ADDAMMO",
        event: EntityEventKind::RELOAD_ADDAMMO,
        dispatch: EntityEventDispatch::Unsupported(
            "applying the reload top-up here would double-count: clip and stock already arrive authoritatively in \
             ClientSnapshotMeta.ammo_clip / ammo_stock, so a client that also applied this event \
             would add the magazine twice",
        ),
    },
    EntityEventRow {
        name: "EV_RAISE_WEAPON",
        event: EntityEventKind::RAISE_WEAPON,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_FIRST_RAISE_WEAPON",
        event: EntityEventKind::FIRST_RAISE_WEAPON,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_PUTAWAY_WEAPON",
        event: EntityEventKind::PUTAWAY_WEAPON,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_WEAPON_ALT",
        event: EntityEventKind::WEAPON_ALT,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_PULLBACK_WEAPON",
        event: EntityEventKind::PULLBACK_WEAPON,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_FIRE_WEAPON",
        event: EntityEventKind::FIRE_WEAPON,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::WeaponFire),
    },
    EntityEventRow {
        name: "EV_FIRE_WEAPON_LASTSHOT",
        event: EntityEventKind::FIRE_WEAPON_LASTSHOT,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::WeaponFire),
    },
    EntityEventRow {
        name: "EV_RECHAMBER_WEAPON",
        event: EntityEventKind::RECHAMBER_WEAPON,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_EJECT_BRASS",
        event: EntityEventKind::EJECT_BRASS,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::EjectBrass),
    },
    EntityEventRow {
        name: "EV_FIRE_WEAPON_LEFT",
        event: EntityEventKind::FIRE_WEAPON_LEFT,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::WeaponFire),
    },
    EntityEventRow {
        name: "EV_FIRE_WEAPON_LASTSHOT_LEFT",
        event: EntityEventKind::FIRE_WEAPON_LASTSHOT_LEFT,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::WeaponFire),
    },
    EntityEventRow {
        name: "EV_EJECT_BRASS_LEFT",
        event: EntityEventKind::EJECT_BRASS_LEFT,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::EjectBrass),
    },
    EntityEventRow {
        name: "EV_SV_FIRE_WEAPON",
        event: EntityEventKind::SV_FIRE_WEAPON,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::WeaponFire),
    },
    EntityEventRow {
        name: "EV_MELEE_SWIPE",
        event: EntityEventKind::MELEE_SWIPE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_FIRE_MELEE",
        event: EntityEventKind::FIRE_MELEE,
        dispatch: EntityEventDispatch::Unsupported(
            "melee hit is the server centre-ray; client dyn-ent and glass \
             melee FX are not played",
        ),
    },
    EntityEventRow {
        name: "EV_PREP_OFFHAND",
        event: EntityEventKind::PREP_OFFHAND,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_USE_OFFHAND",
        event: EntityEventKind::USE_OFFHAND,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_MELEE_HIT",
        event: EntityEventKind::MELEE_HIT,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_MELEE_MISS",
        event: EntityEventKind::MELEE_MISS,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Sound),
    },
    EntityEventRow {
        name: "EV_MELEE_BLOOD",
        event: EntityEventKind::MELEE_BLOOD,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::MeleeBlood),
    },
    EntityEventRow {
        name: "EV_BULLET_HIT",
        event: EntityEventKind::BULLET_HIT,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::BulletHit),
    },
    EntityEventRow {
        name: "EV_BULLET_HIT_SHIELD",
        event: EntityEventKind::BULLET_HIT_SHIELD,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::BulletHit),
    },
    EntityEventRow {
        name: "EV_BULLET_HIT_EXPLODE",
        event: EntityEventKind::BULLET_HIT_EXPLODE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::BulletHit),
    },
    EntityEventRow {
        name: "EV_BULLET_HIT_CLIENT_SMALL",
        event: EntityEventKind::BULLET_HIT_CLIENT_SMALL,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::BulletHit),
    },
    EntityEventRow {
        name: "EV_BULLET_HIT_CLIENT_LARGE",
        event: EntityEventKind::BULLET_HIT_CLIENT_LARGE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::BulletHit),
    },
    EntityEventRow {
        name: "EV_BULLET_HIT_CLIENT_EXPLODE",
        event: EntityEventKind::BULLET_HIT_CLIENT_EXPLODE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::BulletHit),
    },
    EntityEventRow {
        name: "EV_BULLET_HIT_CLIENT_SHIELD",
        event: EntityEventKind::BULLET_HIT_CLIENT_SHIELD,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::BulletHit),
    },
    EntityEventRow {
        name: "EV_EXPLOSIVE_IMPACT_ON_SHIELD",
        event: EntityEventKind::EXPLOSIVE_IMPACT_ON_SHIELD,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::BulletHit),
    },
    EntityEventRow {
        name: "EV_EXPLOSIVE_SPLASH_ON_SHIELD",
        event: EntityEventKind::EXPLOSIVE_SPLASH_ON_SHIELD,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::BulletHit),
    },
    EntityEventRow {
        name: "EV_GRENADE_BOUNCE",
        event: EntityEventKind::GRENADE_BOUNCE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::GrenadeContact),
    },
    EntityEventRow {
        name: "EV_GRENADE_STICK",
        event: EntityEventKind::GRENADE_STICK,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::GrenadeContact),
    },
    EntityEventRow {
        name: "EV_GRENADE_REST",
        event: EntityEventKind::GRENADE_REST,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::GrenadeContact),
    },
    EntityEventRow {
        name: "EV_GRENADE_EXPLODE",
        event: EntityEventKind::GRENADE_EXPLODE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Explosion),
    },
    EntityEventRow {
        name: "EV_ROCKET_EXPLODE",
        event: EntityEventKind::ROCKET_EXPLODE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Explosion),
    },
    EntityEventRow {
        name: "EV_ROCKET_EXPLODE_NOMARKS",
        event: EntityEventKind::ROCKET_EXPLODE_NOMARKS,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Explosion),
    },
    EntityEventRow {
        name: "EV_FLASHBANG_EXPLODE",
        event: EntityEventKind::FLASHBANG_EXPLODE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Explosion),
    },
    EntityEventRow {
        name: "EV_PLAY_FX",
        event: EntityEventKind::PLAY_FX,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::PlayFx),
    },
    EntityEventRow {
        name: "EV_PLAY_FX_ON_TAG",
        event: EntityEventKind::PLAY_FX_ON_TAG,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::PlayFx),
    },
    EntityEventRow {
        name: "EV_STOP_FX_ON_TAG",
        event: EntityEventKind::STOP_FX_ON_TAG,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::PlayFx),
    },
    EntityEventRow {
        name: "EV_PHYS_EXPLOSION_SPHERE",
        event: EntityEventKind::PHYS_EXPLOSION_SPHERE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::PhysicsSphere),
    },
    EntityEventRow {
        name: "EV_PLAY_RUMBLE_ON_ENT",
        event: EntityEventKind::PLAY_RUMBLE_ON_ENT,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Rumble),
    },
    EntityEventRow {
        name: "EV_PLAY_RUMBLE_ON_POS",
        event: EntityEventKind::PLAY_RUMBLE_ON_POS,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Rumble),
    },
    EntityEventRow {
        name: "EV_STOP_RUMBLE",
        event: EntityEventKind::STOP_RUMBLE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Rumble),
    },
    EntityEventRow {
        name: "EV_OBITUARY",
        event: EntityEventKind::OBITUARY,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::Obituary),
    },
    EntityEventRow {
        name: "EV_FOOTSTEP_SPRINT",
        event: EntityEventKind::FOOTSTEP_SPRINT,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::MovementSound),
    },
    EntityEventRow {
        name: "EV_FOOTSTEP_RUN",
        event: EntityEventKind::FOOTSTEP_RUN,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::MovementSound),
    },
    EntityEventRow {
        name: "EV_FOOTSTEP_WALK",
        event: EntityEventKind::FOOTSTEP_WALK,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::MovementSound),
    },
    EntityEventRow {
        name: "EV_FOOTSTEP_PRONE",
        event: EntityEventKind::FOOTSTEP_PRONE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::MovementSound),
    },
    EntityEventRow {
        name: "EV_JUMP",
        event: EntityEventKind::JUMP,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::MovementSound),
    },
    land_ev!(LANDING_DEFAULT),
    land_ev!(LANDING_BARK),
    land_ev!(LANDING_BRICK),
    land_ev!(LANDING_CARPET),
    land_ev!(LANDING_CLOTH),
    land_ev!(LANDING_CONCRETE),
    land_ev!(LANDING_DIRT),
    land_ev!(LANDING_FLESH),
    land_ev!(LANDING_FOLIAGE),
    land_ev!(LANDING_GLASS),
    land_ev!(LANDING_GRASS),
    land_ev!(LANDING_GRAVEL),
    land_ev!(LANDING_ICE),
    land_ev!(LANDING_METAL),
    land_ev!(LANDING_MUD),
    land_ev!(LANDING_PAPER),
    land_ev!(LANDING_PLASTER),
    land_ev!(LANDING_ROCK),
    land_ev!(LANDING_SAND),
    land_ev!(LANDING_SNOW),
    land_ev!(LANDING_WATER),
    land_ev!(LANDING_WOOD),
    land_ev!(LANDING_ASPHALT),
    land_ev!(LANDING_CERAMIC),
    land_ev!(LANDING_PLASTIC),
    land_ev!(LANDING_RUBBER),
    land_ev!(LANDING_CUSHION),
    land_ev!(LANDING_FRUIT),
    land_ev!(LANDING_PAINTEDMETAL),
    land_ev!(LANDING_RIOTSHIELD),
    land_ev!(LANDING_SLUSH),
    land_ev!(LANDING_PAIN_DEFAULT),
    land_ev!(LANDING_PAIN_BARK),
    land_ev!(LANDING_PAIN_BRICK),
    land_ev!(LANDING_PAIN_CARPET),
    land_ev!(LANDING_PAIN_CLOTH),
    land_ev!(LANDING_PAIN_CONCRETE),
    land_ev!(LANDING_PAIN_DIRT),
    land_ev!(LANDING_PAIN_FLESH),
    land_ev!(LANDING_PAIN_FOLIAGE),
    land_ev!(LANDING_PAIN_GLASS),
    land_ev!(LANDING_PAIN_GRASS),
    land_ev!(LANDING_PAIN_GRAVEL),
    land_ev!(LANDING_PAIN_ICE),
    land_ev!(LANDING_PAIN_METAL),
    land_ev!(LANDING_PAIN_MUD),
    land_ev!(LANDING_PAIN_PAPER),
    land_ev!(LANDING_PAIN_PLASTER),
    land_ev!(LANDING_PAIN_ROCK),
    land_ev!(LANDING_PAIN_SAND),
    land_ev!(LANDING_PAIN_SNOW),
    land_ev!(LANDING_PAIN_WATER),
    land_ev!(LANDING_PAIN_WOOD),
    land_ev!(LANDING_PAIN_ASPHALT),
    land_ev!(LANDING_PAIN_CERAMIC),
    land_ev!(LANDING_PAIN_PLASTIC),
    land_ev!(LANDING_PAIN_RUBBER),
    land_ev!(LANDING_PAIN_CUSHION),
    land_ev!(LANDING_PAIN_FRUIT),
    land_ev!(LANDING_PAIN_PAINTEDMETAL),
    land_ev!(LANDING_PAIN_RIOTSHIELD),
    land_ev!(LANDING_PAIN_SLUSH),
    EntityEventRow {
        name: "EV_MANTLE",
        event: EntityEventKind::MANTLE,
        dispatch: EntityEventDispatch::Observer(EntityEventAction::MovementSound),
    },
];

pub fn ev_dispatch_row(event: EntityEventKind) -> Option<&'static EntityEventRow> {
    EV_DISPATCH_REGISTRY.iter().find(|row| row.event == event)
}
