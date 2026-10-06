use bevy::prelude::*;
use movement_iw4::bob_cycle_wrapped;

use crate::{
    aliases::{StepGait, footstep_aliases, gear_rattle_alias, select_fire_alias},
    messages::{Footstep, WeaponSound},
};

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct BobCycleTracker {
    pub last: u8,
    primed: bool,
}

impl BobCycleTracker {
    pub fn observe(&mut self, bob: u8) -> bool {
        if !self.primed {
            self.last = bob;
            self.primed = true;
            return false;
        }
        let wrapped = bob_cycle_wrapped(self.last, bob);
        self.last = bob;
        wrapped
    }
}

pub fn emit_footstep_on_bob_wrap(
    tracker: &mut BobCycleTracker,
    bob_cycle: u8,
    gait: StepGait,
    surface_flags: u32,
    local_player: bool,
    quieter: bool,
    origin_inches: Option<[f32; 3]>,
    snd_ent: Option<u32>,
    footsteps: &mut MessageWriter<Footstep>,
    weapon_sounds: &mut MessageWriter<WeaponSound>,
) -> bool {
    if !tracker.observe(bob_cycle) {
        return false;
    }
    let (alias, fallback) = footstep_aliases(gait, surface_flags, local_player, quieter);
    footsteps.write(Footstep {
        event: None,
        alias,
        fallback,
        origin_inches,
        snd_ent,
    });
    weapon_sounds.write(WeaponSound {
        event: None,
        namespace: asset_core::AssetNamespace::Iw4,
        alias: gear_rattle_alias(gait, local_player).to_owned(),
        origin_inches,
        snd_ent,
    });
    true
}

pub fn emit_weapon_fire(
    player_view: bool,
    fire: Option<&str>,
    fire_player: Option<&str>,
    origin_inches: Option<[f32; 3]>,
    snd_ent: Option<u32>,
    namespace: asset_core::AssetNamespace,
    weapon_sounds: &mut MessageWriter<WeaponSound>,
) -> bool {
    let Some(alias) = select_fire_alias(player_view, fire, fire_player) else {
        return false;
    };
    weapon_sounds.write(WeaponSound {
        event: None,
        namespace,
        alias: alias.to_owned(),
        origin_inches,
        snd_ent,
    });
    true
}
