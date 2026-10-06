pub const AMMO_TABLE_BYTES: usize = 0x78;

pub const AMMOCLIP_TABLE_BYTES: usize = 0xb4;

pub const WEAPON_DATA_BYTES: usize = 15 * 5;

pub fn get_ammo_not_in_clip(ammo: &[u8; AMMO_TABLE_BYTES], ammo_index: i32) -> i32 {
    for i in 0..15 {
        let off = i * 8;
        let idx = i32::from_le_bytes(ammo[off..off + 4].try_into().expect("4"));
        if idx == ammo_index {
            return i32::from_le_bytes(ammo[off + 4..off + 8].try_into().expect("4"));
        }
    }
    0
}

pub fn set_ammo_not_in_clip(
    ammo: &mut [u8; AMMO_TABLE_BYTES],
    ammo_index: i32,
    count: i32,
) -> bool {
    let mut free = None;
    for i in 0..15 {
        let off = i * 8;
        let idx = i32::from_le_bytes(ammo[off..off + 4].try_into().expect("4"));
        if idx == ammo_index {
            ammo[off + 4..off + 8].copy_from_slice(&count.to_le_bytes());
            return true;
        }
        if idx == 0 && free.is_none() {
            free = Some(off);
        }
    }
    if let Some(off) = free {
        ammo[off..off + 4].copy_from_slice(&ammo_index.to_le_bytes());
        ammo[off + 4..off + 8].copy_from_slice(&count.to_le_bytes());
        return true;
    }
    false
}

pub fn get_total_ammo_in_clips(ammoclip: &[u8; AMMOCLIP_TABLE_BYTES], clip_index: i32) -> i32 {
    for i in 0..15 {
        let off = i * 12;
        let idx = i32::from_le_bytes(ammoclip[off..off + 4].try_into().expect("4"));
        if idx == clip_index {
            let clip = i32::from_le_bytes(ammoclip[off + 4..off + 8].try_into().expect("4"));
            let alt = i32::from_le_bytes(ammoclip[off + 8..off + 12].try_into().expect("4"));
            return clip.saturating_add(alt);
        }
    }
    0
}

pub fn clip_row_present(ammoclip: &[u8; AMMOCLIP_TABLE_BYTES], clip_index: i32) -> bool {
    if clip_index == 0 {
        return false;
    }
    for i in 0..15 {
        let off = i * 12;
        let idx = i32::from_le_bytes(ammoclip[off..off + 4].try_into().expect("4"));
        if idx == clip_index {
            return true;
        }
    }
    false
}

pub fn ammo_row_present(ammo: &[u8; AMMO_TABLE_BYTES], ammo_index: i32) -> bool {
    if ammo_index == 0 {
        return false;
    }
    for i in 0..15 {
        let off = i * 8;
        let idx = i32::from_le_bytes(ammo[off..off + 4].try_into().expect("4"));
        if idx == ammo_index {
            return true;
        }
    }
    false
}

pub fn get_clip_for_hand(ammoclip: &[u8; AMMOCLIP_TABLE_BYTES], clip_index: i32, hand: u8) -> i32 {
    let hand = usize::from(hand.min(1));
    for i in 0..15 {
        let off = i * 12;
        let idx = i32::from_le_bytes(ammoclip[off..off + 4].try_into().expect("4"));
        if idx == clip_index {
            let field = off + 4 + hand * 4;
            return i32::from_le_bytes(ammoclip[field..field + 4].try_into().expect("4"));
        }
    }
    0
}

pub fn set_clip_for_hand(
    ammoclip: &mut [u8; AMMOCLIP_TABLE_BYTES],
    clip_index: i32,
    hand: u8,
    count: i32,
) -> bool {
    let hand = usize::from(hand.min(1));
    let mut free = None;
    for i in 0..15 {
        let off = i * 12;
        let idx = i32::from_le_bytes(ammoclip[off..off + 4].try_into().expect("4"));
        if idx == clip_index {
            let field = off + 4 + hand * 4;
            ammoclip[field..field + 4].copy_from_slice(&count.to_le_bytes());
            return true;
        }
        if idx == 0 && free.is_none() {
            free = Some(off);
        }
    }
    if let Some(off) = free {
        ammoclip[off..off + 4].copy_from_slice(&clip_index.to_le_bytes());
        let field = off + 4 + hand * 4;
        ammoclip[field..field + 4].copy_from_slice(&count.to_le_bytes());
        return true;
    }
    false
}

pub fn ensure_clip_row(ammoclip: &mut [u8; AMMOCLIP_TABLE_BYTES], clip_index: i32) -> usize {
    for i in 0..15 {
        let off = i * 12;
        let idx = i32::from_le_bytes(ammoclip[off..off + 4].try_into().expect("4"));
        if idx == clip_index {
            return off;
        }
    }
    for i in 0..15 {
        let off = i * 12;
        let idx = i32::from_le_bytes(ammoclip[off..off + 4].try_into().expect("4"));
        if idx == 0 {
            ammoclip[off..off + 12].fill(0);
            ammoclip[off..off + 4].copy_from_slice(&clip_index.to_le_bytes());
            return off;
        }
    }

    ammoclip[0..12].fill(0);
    ammoclip[0..4].copy_from_slice(&clip_index.to_le_bytes());
    0
}

pub fn spend_clip_for_hand(
    ammoclip: &mut [u8; AMMOCLIP_TABLE_BYTES],
    clip_index: i32,
    hand: u8,
    amount: i32,
) {
    let off = ensure_clip_row(ammoclip, clip_index);
    let hand = usize::from(hand.min(1));
    let field = off + 4 + hand * 4;
    let cur = i32::from_le_bytes(ammoclip[field..field + 4].try_into().expect("4"));
    let next = cur - amount;
    ammoclip[field..field + 4].copy_from_slice(&next.to_le_bytes());
}

#[inline]
pub fn get_ammo_index(weapon_def_ammo_index: i32) -> i32 {
    weapon_def_ammo_index
}

#[inline]
pub fn get_clip_index(weapon_def_clip_index: i32) -> i32 {
    weapon_def_clip_index
}

#[must_use]
pub fn clip_table_key(clip_index: i32, weapon: u32) -> i32 {
    if clip_index != 0 {
        clip_index
    } else {
        weapon as i32
    }
}

#[must_use]
pub fn ammo_table_key(ammo_index: i32, weapon: u32) -> i32 {
    if ammo_index != 0 {
        ammo_index
    } else {
        weapon as i32
    }
}

pub fn get_weapon_dual_wield_byte(weapon_data: &[u8; WEAPON_DATA_BYTES], slot: i32) -> u8 {
    if !(0..15).contains(&slot) {
        return 0;
    }
    weapon_data[slot as usize * 5 + 1]
}

pub fn set_weapon_dual_wield_byte(
    weapon_data: &mut [u8; WEAPON_DATA_BYTES],
    slot: i32,
    dual: bool,
) {
    if !(0..15).contains(&slot) {
        return;
    }
    weapon_data[slot as usize * 5 + 1] = u8::from(dual);
}

pub fn player_weapons_find_slot(weapons: &[i32; 15], weapon: i32) -> i32 {
    if weapon == 0 {
        return -1;
    }
    for (i, &slot) in weapons.iter().enumerate() {
        if slot == weapon {
            return i as i32;
        }
    }
    -1
}

pub fn latch_weapon_dual_wield(
    weapons: &[i32; 15],
    weapon_data: &mut [u8],
    weapon: u32,
    dual: bool,
) {
    let slot = player_weapons_find_slot(weapons, weapon as i32);
    if slot < 0 {
        return;
    }
    let Some(data) = weapon_data.get_mut(..WEAPON_DATA_BYTES) else {
        return;
    };
    let Ok(data) = <&mut [u8; WEAPON_DATA_BYTES]>::try_from(data) else {
        return;
    };
    set_weapon_dual_wield_byte(data, slot, dual);
}

pub fn weapon_model_for_held(weapons: &[i32; 15], weapon_data: &[u8], weapon: u32) -> u8 {
    let slot = player_weapons_find_slot(weapons, weapon as i32);
    if slot < 0 {
        return 0;
    }
    weapon_data.get(slot as usize * 5 + 2).copied().unwrap_or(0)
}

pub fn set_weapon_model_for_held(
    weapons: &[i32; 15],
    weapon_data: &mut [u8],
    weapon: u32,
    model: u8,
) {
    let slot = player_weapons_find_slot(weapons, weapon as i32);
    if slot < 0 {
        return;
    }
    if let Some(byte) = weapon_data.get_mut(slot as usize * 5 + 2) {
        *byte = model;
    }
}

pub fn num_hands(dual_wield_byte: u8) -> i32 {
    if dual_wield_byte != 0 { 1 } else { 0 }
}

pub fn num_hands_for_held(weapons: &[i32; 15], weapon_data: &[u8], held_weapon: u32) -> i32 {
    if held_weapon == 0 {
        return 0;
    }
    let slot = player_weapons_find_slot(weapons, held_weapon as i32);
    if slot < 0 {
        return 0;
    }
    let Some(data) = weapon_data.get(..WEAPON_DATA_BYTES) else {
        return 0;
    };
    let data: &[u8; WEAPON_DATA_BYTES] = data.try_into().expect("WEAPON_DATA_BYTES");
    num_hands(get_weapon_dual_wield_byte(data, slot))
}

pub fn has_akimbo_viewmodel_anims(right_idle: Option<&str>) -> bool {
    right_idle
        .and_then(|name| name.as_bytes().first().copied())
        .is_some_and(|b| b != 0)
}

#[must_use]
pub fn create_akimbo_viewmodel_trees(no_dual_wield: u8, right_idle: Option<&str>) -> bool {
    no_dual_wield == 0 && has_akimbo_viewmodel_anims(right_idle)
}

pub fn get_ammo_player_both_clips(
    ps: &playerstate_iw4::PlayerState,
    weapon: u32,
    ammo_index: i32,
    clip_index: i32,
) -> i32 {
    let stock = get_ammo_not_in_clip(&ps.ammo, ammo_index);
    let right = get_clip_for_hand(&ps.ammoclip, clip_index, 0);
    let left = if crate::num_hands_for_held(&ps.weapons, &ps.weapon_data, weapon) == 1 {
        get_clip_for_hand(&ps.ammoclip, clip_index, 1)
    } else {
        0
    };
    stock.wrapping_add(right).wrapping_add(left)
}
