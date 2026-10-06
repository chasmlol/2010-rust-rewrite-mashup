use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PosedPlayerHead {
    Exact(Vec3),

    NoDObjOrHead,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PosedPlayer {
    pub entnum: u16,
    pub head: PosedPlayerHead,
}

#[derive(Resource, Default)]
pub struct PosedPlayerFrame {
    players: Vec<PosedPlayer>,
}

impl PosedPlayerFrame {
    pub fn clear(&mut self) {
        self.players.clear();
    }

    pub fn publish(&mut self, player: PosedPlayer) {
        self.players.push(player);
    }

    pub fn iter(&self) -> impl Iterator<Item = &PosedPlayer> {
        self.players.iter()
    }
}

#[derive(Clone, Debug)]
struct HostDObjPose {
    current_valid: bool,
    centity_teleport: bool,
    entity: xmodel_runtime::DObjBoneOrientation,
    bones: Vec<xmodel_runtime::DObjBoneOrientation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostDObjPoseRefuse {
    MissingDObj,
    BoneCountOverflow,
    Bone(fx_iw4::BoneOrientationRefuse),
    Frame(xmodel_runtime::DObjBoneOrientationError),
}

#[derive(Resource, Default)]
pub struct HostDObjPoseFrame {
    by_dobj: HashMap<u32, HostDObjPose>,
}

impl HostDObjPoseFrame {
    pub fn clear(&mut self) {
        self.by_dobj.clear();
    }

    pub fn publish(
        &mut self,
        dobj: u32,
        current_valid: bool,
        next_state_eflags: u32,
        entity_world: Mat4,
        bone_world: &[Mat4],
    ) -> Result<(), HostDObjPoseRefuse> {
        let entity = xmodel_runtime::bone_orientation(&[entity_world], 0)
            .map_err(HostDObjPoseRefuse::Frame)?;
        let mut bones = Vec::with_capacity(bone_world.len());
        for matrix in bone_world {
            let world = entity_world * *matrix;
            bones.push(
                xmodel_runtime::bone_orientation(&[world], 0).map_err(HostDObjPoseRefuse::Frame)?,
            );
        }
        self.by_dobj.insert(
            dobj,
            HostDObjPose {
                current_valid,
                centity_teleport: fx_iw4::bolt_spawn_teleport_bit(dobj, next_state_eflags),
                entity,
                bones,
            },
        );
        Ok(())
    }

    pub fn resolve(
        &self,
        dobj: u32,
        bone: i32,
    ) -> Result<fx::FxBoltOrientation, HostDObjPoseRefuse> {
        let pose = self
            .by_dobj
            .get(&dobj)
            .ok_or(HostDObjPoseRefuse::MissingDObj)?;
        let count =
            u8::try_from(pose.bones.len()).map_err(|_| HostDObjPoseRefuse::BoneCountOverflow)?;
        let route = fx_iw4::get_bone_orientation_route(dobj, pose.current_valid, bone, Some(count))
            .map_err(HostDObjPoseRefuse::Bone)?;
        let orientation = match route {
            fx_iw4::FxGetBoneOrientationRoute::EntityPose => pose.entity,
            fx_iw4::FxGetBoneOrientationRoute::DObjBone(index) => pose.bones[index as usize],
        };
        Ok(fx::FxBoltOrientation {
            origin: orientation.origin,
            axis: orientation.axis,
        })
    }

    pub fn resolve_live_bolt(&self, dobj: u32, bone: u16) -> Option<fx::FxResolvedBoltPose> {
        let pose = self.by_dobj.get(&dobj)?;
        Some(fx::FxResolvedBoltPose {
            centity_teleport: pose.centity_teleport,
            orientation: self.resolve(dobj, i32::from(bone)).ok(),
        })
    }
}

pub fn begin_dobj_pose_frame(
    mut dobj_poses: ResMut<HostDObjPoseFrame>,
    mut posed_players: ResMut<PosedPlayerFrame>,
) {
    dobj_poses.clear();
    posed_players.clear();
}

#[derive(Resource, Default)]
pub struct ScriptModelBoltDemand {
    entnums: HashSet<u16>,
}

impl ScriptModelBoltDemand {
    pub fn insert(&mut self, entnum: u16) {
        self.entnums.insert(entnum);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entnums.is_empty()
    }

    pub(crate) fn contains(&self, entnum: u16) -> bool {
        self.entnums.contains(&entnum)
    }

    pub(crate) fn retain(&mut self, live: impl Fn(u16) -> bool) {
        self.entnums.retain(|entnum| live(*entnum));
    }
}

#[derive(Resource, Default)]
pub struct ScriptModelDObjFrame {
    models: HashMap<(sim::ScriptModelId, u16), std::sync::Arc<xmodel_runtime::DObj>>,
}
impl ScriptModelDObjFrame {
    pub(crate) fn clear(&mut self) {
        self.models.clear();
    }
    pub(crate) fn publish(
        &mut self,
        id: sim::ScriptModelId,
        entity: u16,
        dobj: std::sync::Arc<xmodel_runtime::DObj>,
    ) {
        self.models.insert((id, entity), dobj);
    }
    pub fn iter(&self) -> impl Iterator<Item = (sim::ScriptModelId, u16)> + '_ {
        self.models.keys().copied()
    }
}
