use std::sync::Arc;

use bevy::math::{Mat4, Vec3};
use bevy::prelude::*;

use crate::anim::fpv::{
    EquippedFpv, FpvAuthoritySample, FpvPresentState, tick_equipped_fpv_with_predicted_fire,
};
use crate::anim::fpv_pose::{FpvBoltFrame, PosedClip};
use crate::anim::fpv_prepared::FpvRigSet;
use crate::anim::fpv_rig::{FpvHandPose, PreparedFpvRig};
use assets::FpvMeshIndex;

#[derive(Resource, Default)]
pub struct FpvPresentCursor(pub FpvPresentState);

#[derive(Resource, Default)]
pub struct PendingFpvSpawn(pub Option<PendingFpvSpawnRequest>);

#[derive(Clone, Debug)]
pub struct PendingFpvSpawnRequest {
    pub weapon_handle: asset_game::WeaponHandle,
    pub parent_handle: Option<asset_game::WeaponHandle>,
    pub gun_index: FpvMeshIndex,
    pub catalog_id: u64,
    pub weapon_id: u32,
    pub parent_weapon: u32,
}

#[derive(Resource, Default)]
pub struct PendingFpvNotetracks {
    pub batch: Option<audio::ViewmodelNotetracks>,
}

#[derive(Resource, Default)]
pub struct FpvHeldSettled(pub Option<u32>);

#[derive(Resource, Default)]
pub struct FpvHeldLife(pub Option<u32>);

#[derive(Resource, Default)]
pub struct FpvBoltTargets {
    pub pose: [Option<FpvBoltFrame>; 2],
    pub tracker_screen: Option<[Vec3; 3]>,
    pub tracker_light: Option<fx::FxBoltTarget>,
    pub flash: [Option<fx::FxBoltTarget>; 2],
    pub brass: [Option<fx::FxBoltTarget>; 2],
    pub knife: [Option<fx::FxBoltTarget>; 2],
    pub laser: [Option<fx::FxBoltTarget>; 2],
}

impl FpvBoltTargets {
    pub fn clear(&mut self) {
        self.pose = [None, None];
        self.tracker_screen = None;
        self.tracker_light = None;
        self.flash = [None, None];
        self.brass = [None, None];
        self.knife = [None, None];
        self.laser = [None, None];
    }

    pub fn set_pose(&mut self, hand: usize, bolt: FpvBoltFrame) {
        self.pose[hand] = Some(bolt);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FpvPoseRefuse {
    CatalogMissing,
    NoActiveClips,
    EyePoseFailed {
        gun_xmodel: String,
    },
    DependencyUnresolved {
        weapon_id: u32,
        role: &'static str,
        name: String,
    },
}

pub enum FpvPoseKind {
    Hide,
    Refuse(FpvPoseRefuse),
    Posed(FpvPosedFrame),
}

impl Default for FpvPoseKind {
    fn default() -> Self {
        Self::Hide
    }
}

/// What one frame asked of the rig: bones per hand, and the lens the right hand
/// carries. No geometry — the surfaces this rig draws were settled when it was
/// prepared, and the vertices are written straight into the published plan.
pub struct FpvPosedFrame {
    pub secondary_bolt: Option<FpvBoltFrame>,
    pub poses: [Option<FpvHandPose>; 2],
    pub lens: Mat4,
    pub idle_sampled: bool,
}

#[derive(Resource, Default)]
pub struct FpvPoseProduct {
    pub drawgun: Option<i32>,
    pub kind: FpvPoseKind,
    pub camo: u8,
}

pub struct FpvGenerateArgs<'a> {
    pub dt: f32,
    pub equipped: &'a mut EquippedFpv,
    pub rigs: &'a FpvRigSet,
    pub active: &'a mut Option<Arc<PreparedFpvRig>>,
    pub cursor: &'a mut FpvPresentState,
    pub rocket: bool,
    pub melee: bool,
    pub ads: bool,
    pub jammed: bool,
    pub sample: Option<FpvAuthoritySample>,
    pub predicted_fire: bool,
    pub dual: bool,
    pub dual_offset: Option<f32>,
}

pub fn generate_fpv_pose(
    args: FpvGenerateArgs<'_>,
) -> (FpvPoseKind, crate::anim::fpv::FpvNotetracks) {
    let FpvGenerateArgs {
        dt,
        equipped,
        rigs,
        active,
        cursor,
        rocket,
        melee,
        ads,
        jammed,
        sample,
        predicted_fire,
        dual,
        dual_offset,
    } = args;
    let (_pose, notifies) =
        tick_equipped_fpv_with_predicted_fire(equipped, cursor, sample, predicted_fire, dt);

    let kind = pose_equipped_fpv(
        equipped,
        rigs,
        active,
        rocket,
        melee,
        ads,
        jammed,
        dual,
        dual_offset,
    );
    (kind, notifies)
}

fn pose_equipped_fpv(
    equipped: &EquippedFpv,
    rigs: &FpvRigSet,
    active: &mut Option<Arc<PreparedFpvRig>>,
    rocket: bool,
    melee: bool,
    ads: bool,
    jammed: bool,
    dual: bool,
    dual_offset: Option<f32>,
) -> FpvPoseKind {
    let right: Vec<PosedClip<'_>> = equipped
        .controller
        .active_anims()
        .map(|a| PosedClip {
            node: a.node,
            clip: a.clip,
            time: a.time,
            weight: a.weight,
        })
        .collect();
    if right.is_empty() {
        return FpvPoseKind::Refuse(FpvPoseRefuse::NoActiveClips);
    }
    let left: Vec<PosedClip<'_>> = match (dual, equipped.left.as_ref()) {
        (true, Some(left)) => left
            .active_anims()
            .map(|a| PosedClip {
                node: a.node,
                clip: a.clip,
                time: a.time,
                weight: a.weight,
            })
            .collect(),
        _ => Vec::new(),
    };
    let dual_drawn = !left.is_empty();

    let Some(prepared) = rigs.pick(rocket, dual_drawn, melee, ads, jammed) else {
        *active = None;
        return FpvPoseKind::Refuse(FpvPoseRefuse::EyePoseFailed {
            gun_xmodel: equipped.gun_xmodel.clone(),
        });
    };
    if active
        .as_ref()
        .is_none_or(|current| current.generation() != prepared.generation())
    {
        *active = Some(Arc::clone(prepared));
    }

    let combined = prepared.combines_hands();
    let pose = if combined {
        prepared.pose_combined(&right, &left)
    } else {
        prepared.pose_hand(0, &right, Vec3::ZERO)
    };
    let Some(right_pose) = pose else {
        return FpvPoseKind::Refuse(FpvPoseRefuse::EyePoseFailed {
            gun_xmodel: equipped.gun_xmodel.clone(),
        });
    };
    let lens = right_pose.lens;
    let left_pose = if dual_drawn && !combined {
        let offset = dual_offset
            .filter(|offset| *offset != 0.0)
            .map(|offset| {
                Vec3::from_array(weapon_iw4::dual_wield_view_model_origin_add(
                    1,
                    [-1.0, 0.0, 0.0],
                    offset,
                ))
            })
            .unwrap_or(Vec3::ZERO);
        let Some(pose) = prepared.pose_hand(1, &left, offset) else {
            return FpvPoseKind::Refuse(FpvPoseRefuse::EyePoseFailed {
                gun_xmodel: equipped.gun_xmodel.clone(),
            });
        };
        Some(pose)
    } else {
        None
    };

    let secondary_bolt = prepared.secondary_bolt(&right_pose);
    let poses = [Some(right_pose), left_pose];
    FpvPoseKind::Posed(FpvPosedFrame {
        poses,
        secondary_bolt,
        lens,
        idle_sampled: true,
    })
}
