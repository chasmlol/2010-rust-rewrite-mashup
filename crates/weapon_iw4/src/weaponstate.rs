#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponDecodeError {
    UnknownWeaponState(i32),
    UnknownFireType(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum WeaponState {
    Ready = 0x0,
    Raising = 0x1,
    RaisingAltswitch = 0x2,
    Dropping = 0x3,
    DroppingQuick = 0x4,

    DroppingAltswitch = 0x5,
    Firing = 0x6,
    Rechambering = 0x7,
    Reloading = 0x8,
    ReloadingInterrupt = 0x9,
    ReloadStart = 0xA,
    ReloadStartInterrupt = 0xB,
    ReloadEnd = 0xC,
    MeleeInit = 0xD,
    MeleeFire = 0xE,
    MeleeEnd = 0xF,
    OffhandInit = 0x10,
    OffhandPrepare = 0x11,
    OffhandHold = 0x12,
    OffhandStart = 0x13,
    Offhand = 0x14,
    OffhandEnd = 0x15,

    Detonating = 0x16,
    SprintIn = 0x17,
    SprintLoop = 0x18,
    SprintOut = 0x19,
    StunnedStart = 0x1A,
    StunnedLoop = 0x1B,
    StunnedEnd = 0x1C,
    NightVisionWear = 0x1D,
    NightVisionRemove = 0x1E,
}

impl WeaponState {
    pub fn from_i32(v: i32) -> Result<Self, WeaponDecodeError> {
        Ok(match v {
            0x0 => Self::Ready,
            0x1 => Self::Raising,
            0x2 => Self::RaisingAltswitch,
            0x3 => Self::Dropping,
            0x4 => Self::DroppingQuick,
            0x5 => Self::DroppingAltswitch,
            0x6 => Self::Firing,
            0x7 => Self::Rechambering,
            0x8 => Self::Reloading,
            0x9 => Self::ReloadingInterrupt,
            0xA => Self::ReloadStart,
            0xB => Self::ReloadStartInterrupt,
            0xC => Self::ReloadEnd,
            0xD => Self::MeleeInit,
            0xE => Self::MeleeFire,
            0xF => Self::MeleeEnd,
            0x10 => Self::OffhandInit,
            0x11 => Self::OffhandPrepare,
            0x12 => Self::OffhandHold,
            0x13 => Self::OffhandStart,
            0x14 => Self::Offhand,
            0x15 => Self::OffhandEnd,
            0x16 => Self::Detonating,
            0x17 => Self::SprintIn,
            0x18 => Self::SprintLoop,
            0x19 => Self::SprintOut,
            0x1A => Self::StunnedStart,
            0x1B => Self::StunnedLoop,
            0x1C => Self::StunnedEnd,
            0x1D => Self::NightVisionWear,
            0x1E => Self::NightVisionRemove,
            other => return Err(WeaponDecodeError::UnknownWeaponState(other)),
        })
    }

    pub fn is_raise_or_drop(self) -> bool {
        matches!(
            self,
            Self::Raising
                | Self::RaisingAltswitch
                | Self::Dropping
                | Self::DroppingQuick
                | Self::DroppingAltswitch
        )
    }

    pub fn is_sprint(self) -> bool {
        matches!(self, Self::SprintIn | Self::SprintLoop | Self::SprintOut)
    }

    pub fn is_reload_family(self) -> bool {
        matches!(
            self,
            Self::Reloading
                | Self::ReloadingInterrupt
                | Self::ReloadStart
                | Self::ReloadStartInterrupt
                | Self::ReloadEnd
        )
    }
}

#[must_use]
pub fn viewmodel_rocket_should_be_attached(
    clip: i32,
    weaponstate: i32,
    weapon_time: i32,
    reload_time_ms: i32,
    reload_show_rocket_time_ms: i32,
) -> bool {
    if clip > 0 {
        return true;
    }
    matches!(WeaponState::from_i32(weaponstate), Ok(state) if state.is_reload_family())
        && reload_time_ms.wrapping_sub(weapon_time) > reload_show_rocket_time_ms
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum FireType {
    FullAuto = 0,
    SingleShot = 1,
    BurstFire2 = 2,
    BurstFire3 = 3,
    BurstFire4 = 4,
    DoubleBarrel = 5,
}

impl FireType {
    pub fn from_i32(v: i32) -> Result<Self, WeaponDecodeError> {
        Ok(match v {
            0 => Self::FullAuto,
            1 => Self::SingleShot,
            2 => Self::BurstFire2,
            3 => Self::BurstFire3,
            4 => Self::BurstFire4,
            5 => Self::DoubleBarrel,
            other => return Err(WeaponDecodeError::UnknownFireType(other)),
        })
    }

    pub fn is_full_auto(self) -> bool {
        matches!(self, Self::FullAuto)
    }

    pub fn is_single(self) -> bool {
        matches!(self, Self::SingleShot | Self::DoubleBarrel)
    }

    pub fn is_burst(self) -> bool {
        matches!(self, Self::BurstFire2 | Self::BurstFire3 | Self::BurstFire4)
    }

    pub fn burst_limit(self) -> Option<u8> {
        match self {
            Self::BurstFire2 => Some(2),
            Self::BurstFire3 => Some(3),
            Self::BurstFire4 => Some(4),
            _ => None,
        }
    }

    pub fn shot_limit_reached(self, shot_count: u8) -> bool {
        match self {
            Self::FullAuto => false,
            Self::SingleShot | Self::DoubleBarrel => shot_count != 0,
            Self::BurstFire2 => shot_count > 1,
            Self::BurstFire3 => shot_count > 2,
            Self::BurstFire4 => shot_count > 3,
        }
    }
}
