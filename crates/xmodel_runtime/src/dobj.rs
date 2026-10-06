use anim_iw4::{
    Local, PartBits, QUAT_IDENTITY, VEC3_ZERO, compose_translation, hide_part_bit, normalize,
    quat_add_weighted, quat16, set_hide_part_bit, surface_hidden, xmodel_no_scale_bit,
};
use glam::{Mat4, Quat, Vec3};

use crate::AnimClip;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HidePartBits([u32; PartBits::WORDS]);

impl HidePartBits {
    pub const fn from_words(words: [u32; PartBits::WORDS]) -> Self {
        Self(words)
    }

    pub const fn words(&self) -> &[u32; PartBits::WORDS] {
        &self.0
    }

    pub fn set(&mut self, bone: usize) {
        set_hide_part_bit(&mut self.0, bone);
    }

    pub fn get(&self, bone: usize) -> bool {
        hide_part_bit(&self.0, bone)
    }
}

#[derive(Debug, Clone)]
pub struct Attach {
    pub parent_model: usize,
    pub tag: String,
}

#[derive(Debug, Clone)]
pub struct ModelSlot {
    pub name: String,
    pub base: usize,
    pub bone_count: usize,
    pub scale: f32,
}

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub model: usize,
    pub parent: Option<usize>,
    pub bind_rotation: Quat,
    pub bind_translation: Vec3,

    pub bind_world: Mat4,
    pub no_scale: bool,
}

#[derive(Debug, Clone)]
pub struct DObj {
    pub bones: Vec<Bone>,
    pub models: Vec<ModelSlot>,
    pub duplicates: Vec<(usize, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DObjBoneOrientation {
    pub origin: [f32; 3],

    pub axis: [[f32; 3]; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DObjBoneOrientationError {
    BoneOutOfRange { bone: usize, count: usize },
    NonFinite { bone: usize },
    DegenerateAxis { bone: usize, axis: usize },
}

pub fn bone_orientation(
    bone_world: &[Mat4],
    bone: usize,
) -> Result<DObjBoneOrientation, DObjBoneOrientationError> {
    let Some(world) = bone_world.get(bone).copied() else {
        return Err(DObjBoneOrientationError::BoneOutOfRange {
            bone,
            count: bone_world.len(),
        });
    };
    let origin = world.transform_point3(Vec3::ZERO);
    if !origin.is_finite() {
        return Err(DObjBoneOrientationError::NonFinite { bone });
    }
    let mut axis = [[0.0; 3]; 3];
    for (index, source) in Vec3::AXES.into_iter().enumerate() {
        let direction = world.transform_vector3(source);
        let length = direction.length();
        if !length.is_finite() {
            return Err(DObjBoneOrientationError::NonFinite { bone });
        }
        if length <= f32::EPSILON {
            return Err(DObjBoneOrientationError::DegenerateAxis { bone, axis: index });
        }
        axis[index] = (direction / length).to_array();
    }
    Ok(DObjBoneOrientation {
        origin: origin.to_array(),
        axis,
    })
}

#[derive(Debug, Clone)]
pub enum DObjError {
    Model {
        model: String,
        detail: String,
    },
    AttachTag {
        model: String,
        tag: String,
        parent: usize,
    },
    TooManyBones {
        count: usize,
        limit: usize,
    },
}

impl core::fmt::Display for DObjError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Model { model, detail } => write!(f, "model {model:?}: {detail}"),
            Self::AttachTag { model, tag, parent } => {
                write!(
                    f,
                    "attachment {model:?}: no bone named {tag:?} in model {parent}"
                )
            }
            Self::TooManyBones { count, limit } => {
                write!(f, "a DObj holds at most {limit} bones, got {count}")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelPoseSrc {
    pub name: String,
    pub num_bones: usize,
    pub num_root_bones: usize,
    pub scale: f32,
    pub no_scale_part_bits: [u32; 6],
    pub bone_names: Vec<String>,

    pub parent_list: Vec<u8>,

    pub quats: Vec<[i16; 4]>,

    pub trans: Vec<[f32; 3]>,

    pub base_mat: Vec<(Quat, Vec3)>,

    pub root_rest: Option<(Quat, Vec3)>,
}

impl DObj {
    pub fn build(models: &[(&ModelPoseSrc, Option<Attach>)]) -> Result<Self, DObjError> {
        let mut bones: Vec<Bone> = Vec::new();
        let mut slots: Vec<ModelSlot> = Vec::new();

        for (index, (model, attach)) in models.iter().enumerate() {
            let count = model.num_bones;
            let roots = model.num_root_bones;
            if model.base_mat.len() < count {
                return Err(DObjError::Model {
                    model: model.name.clone(),
                    detail: format!("{} bind matrices for {count} bones", model.base_mat.len()),
                });
            }

            let attach_to = match attach {
                None => None,
                Some(a) => {
                    let slot: &ModelSlot =
                        slots.get(a.parent_model).ok_or_else(|| DObjError::Model {
                            model: model.name.clone(),
                            detail: format!(
                                "attaches to model {}, which is not built yet",
                                a.parent_model
                            ),
                        })?;
                    let range = slot.base..slot.base + slot.bone_count;
                    let found = bones[range.clone()]
                        .iter()
                        .position(|b| b.name == a.tag)
                        .map(|i| slot.base + i);
                    Some(found.ok_or_else(|| DObjError::AttachTag {
                        model: model.name.clone(),
                        tag: a.tag.clone(),
                        parent: a.parent_model,
                    })?)
                }
            };

            let base = bones.len();
            slots.push(ModelSlot {
                name: model.name.clone(),
                base,
                bone_count: count,
                scale: model.scale,
            });

            for bone in 0..count {
                let (bq, bt) = model.base_mat[bone];
                let bind_world = Mat4::from_rotation_translation(bq.normalize(), bt);

                let (bind_rotation, bind_translation, parent) = if bone < roots {
                    let (rotation, translation) = model
                        .root_rest
                        .filter(|_| bone == 0)
                        .unwrap_or((Quat::IDENTITY, Vec3::ZERO));
                    (rotation, translation, attach_to)
                } else {
                    let child = bone - roots;
                    let step = *model
                        .parent_list
                        .get(child)
                        .ok_or_else(|| DObjError::Model {
                            model: model.name.clone(),
                            detail: format!("bone {bone} has no parent entry"),
                        })? as usize;
                    let parent = bone
                        .checked_sub(step)
                        .filter(|p| *p < bone && step > 0)
                        .ok_or_else(|| DObjError::Model {
                            model: model.name.clone(),
                            detail: format!("bone {bone} parent step {step} escapes the skeleton"),
                        })?;
                    let quat = *model.quats.get(child).ok_or_else(|| DObjError::Model {
                        model: model.name.clone(),
                        detail: format!("bone {bone} has no bind rotation"),
                    })?;
                    let trans = *model.trans.get(child).ok_or_else(|| DObjError::Model {
                        model: model.name.clone(),
                        detail: format!("bone {bone} has no bind translation"),
                    })?;
                    let rq = quat16(quat);
                    (
                        Quat::from_xyzw(rq[0], rq[1], rq[2], rq[3]),
                        Vec3::from_array(trans),
                        Some(base + parent),
                    )
                };

                bones.push(Bone {
                    name: model.bone_names.get(bone).cloned().unwrap_or_default(),
                    model: index,
                    parent,
                    bind_rotation,
                    bind_translation,
                    bind_world,
                    no_scale: xmodel_no_scale_bit(&model.no_scale_part_bits, bone),
                });
            }
        }

        if bones.len() > PartBits::CAPACITY {
            return Err(DObjError::TooManyBones {
                count: bones.len(),
                limit: PartBits::CAPACITY,
            });
        }

        let mut duplicates = Vec::new();
        let mut first_by_name = std::collections::HashMap::<&str, usize>::new();
        for (dest, bone) in bones.iter().enumerate() {
            if bone.name.is_empty() {
                continue;
            }
            let source = *first_by_name.entry(&bone.name).or_insert(dest);
            // Models are appended in contiguous blocks. The first occurrence
            // is therefore also the first occurrence in any earlier model.
            if bones[source].model != bone.model && bone.parent != Some(source) {
                duplicates.push((dest, source));
            }
        }

        Ok(DObj {
            bones,
            models: slots,
            duplicates,
        })
    }

    pub fn bone_count(&self) -> usize {
        self.bones.len()
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    pub fn all_parts(&self) -> PartBits {
        let mut bits = PartBits::default();
        for i in 0..self.bones.len() {
            bits.set(i);
        }
        bits
    }

    pub fn tracks_for(&self, clip: &AnimClip) -> Vec<Option<usize>> {
        let mut first_by_name = std::collections::HashMap::with_capacity(self.bones.len());
        for (index, bone) in self.bones.iter().enumerate() {
            first_by_name.entry(bone.name.as_str()).or_insert(index);
        }
        clip.tracks
            .iter()
            .map(|track| first_by_name.get(track.name.as_str()).copied())
            .collect()
    }

    pub fn calc_anim(&self, anims: &[AnimInstance<'_>], requested: &PartBits) -> Vec<Local> {
        self.calc_anim_masked(anims, requested).0
    }

    pub fn calc_anim_masked(
        &self,
        anims: &[AnimInstance<'_>],
        requested: &PartBits,
    ) -> (Vec<Local>, Vec<bool>) {
        let mut rotation = vec![[0.0f32; 4]; self.bones.len()];
        let mut translation = vec![VEC3_ZERO; self.bones.len()];
        let mut trans_weight = vec![0.0f32; self.bones.len()];
        let mut controlled = vec![false; self.bones.len()];

        for anim in anims {
            if anim.weight <= 0.0 {
                continue;
            }
            for (track, bone) in anim.tracks.iter().enumerate() {
                let Some(bone) = *bone else { continue };
                if !requested.get(bone) {
                    continue;
                }
                if let Some(parts) = anim.parts
                    && !parts.get(bone)
                {
                    continue;
                }
                let Some(sample) = anim.clip.sample_track(track, anim.time) else {
                    continue;
                };
                controlled[bone] = true;
                trans_weight[bone] += anim.weight;

                let q = sample.rotation.unwrap_or(QUAT_IDENTITY);
                rotation[bone] = quat_add_weighted(rotation[bone], q, anim.weight);

                if let Some(t) = sample.translation {
                    translation[bone][0] += t[0] * anim.weight;
                    translation[bone][1] += t[1] * anim.weight;
                    translation[bone][2] += t[2] * anim.weight;
                }
            }
        }

        let locals = (0..self.bones.len())
            .map(|bone| {
                if controlled[bone] {
                    let trans = if trans_weight[bone] > 0.0 {
                        let r = 1.0 / trans_weight[bone];
                        [
                            translation[bone][0] * r,
                            translation[bone][1] * r,
                            translation[bone][2] * r,
                        ]
                    } else {
                        translation[bone]
                    };
                    Local {
                        control: false,
                        rotation: normalize(rotation[bone]),
                        translation: trans,
                    }
                } else {
                    let br = self.bones[bone].bind_rotation;
                    Local {
                        control: false,
                        rotation: [br.x, br.y, br.z, br.w],
                        translation: VEC3_ZERO,
                    }
                }
            })
            .collect();
        (locals, controlled)
    }

    pub fn apply_duplicates(&self, locals: &mut [Local]) {
        for &(dest, source) in &self.duplicates {
            locals[dest] = locals[source];
        }
    }

    pub fn compose(&self, locals: &[Local], placement: Mat4) -> Vec<Mat4> {
        let mut world: Vec<Mat4> = Vec::with_capacity(self.bones.len());
        for (index, bone) in self.bones.iter().enumerate() {
            let scale = if bone.no_scale {
                1.0
            } else {
                self.models[bone.model].scale
            };
            let bind = [
                bone.bind_translation.x,
                bone.bind_translation.y,
                bone.bind_translation.z,
            ];
            let local_t = compose_translation(bind, scale, locals[index].translation);
            let rq = locals[index].rotation;
            let local = Mat4::from_rotation_translation(
                Quat::from_xyzw(rq[0], rq[1], rq[2], rq[3]),
                Vec3::from_array(local_t),
            );
            world.push(match bone.parent {
                Some(parent) if locals[index].control => {
                    let root = world[0].to_scale_rotation_translation().1;
                    let parent_rotation = world[parent].to_scale_rotation_translation().1;
                    let control = Quat::from_xyzw(rq[0], rq[1], rq[2], rq[3]);
                    Mat4::from_rotation_translation(
                        root * control * root.conjugate() * parent_rotation,
                        world[parent].transform_point3(Vec3::from_array(local_t)),
                    )
                }
                Some(parent) => world[parent] * local,
                None => placement * local,
            });
        }
        world
    }

    pub fn pose(
        &self,
        anims: &[AnimInstance<'_>],
        requested: &PartBits,
        placement: Mat4,
    ) -> Vec<Mat4> {
        self.pose_with_controller(anims, requested, placement, |_, _, _| {})
    }

    pub fn pose_with_controller(
        &self,
        anims: &[AnimInstance<'_>],
        requested: &PartBits,
        placement: Mat4,
        controller: impl FnOnce(&DObj, &PartBits, &mut [Local]),
    ) -> Vec<Mat4> {
        let mut locals = self.calc_anim(anims, requested);
        controller(self, requested, &mut locals);
        self.apply_duplicates(&mut locals);
        self.compose(&locals, placement)
    }

    pub fn surface_visible(
        &self,
        model: usize,
        surface_part_bits: &[u32; PartBits::WORDS],
        hidden: &HidePartBits,
    ) -> bool {
        let Some(slot) = self.models.get(model) else {
            return false;
        };
        let base = u32::try_from(slot.base).unwrap_or(u32::MAX);
        !surface_hidden(surface_part_bits, hidden.words(), base)
    }

    pub fn skin_matrices(&self, world: &[Mat4]) -> Vec<Mat4> {
        self.bones
            .iter()
            .zip(world)
            .map(|(bone, world)| *world * bone.bind_world.inverse())
            .collect()
    }

    pub fn bind_locals(&self) -> Vec<Local> {
        self.bones
            .iter()
            .map(|b| Local {
                control: false,
                rotation: [
                    b.bind_rotation.x,
                    b.bind_rotation.y,
                    b.bind_rotation.z,
                    b.bind_rotation.w,
                ],
                translation: VEC3_ZERO,
            })
            .collect()
    }
}

pub const AIM_PITCH_CLAMP_RAD: f32 = 85.0 * core::f32::consts::PI / 180.0;

pub fn apply_legs_yaw(dobj: &DObj, locals: &mut [Local], legs_offset: f32) {
    if legs_offset.abs() < 1e-5 {
        return;
    }
    yaw_bone(dobj, locals, "pelvis", legs_offset);
    yaw_bone(dobj, locals, "torso_stabilizer", -legs_offset);
}

pub fn apply_aim_pitches(dobj: &DObj, locals: &mut [Local], torso_pitch: f32, waist_pitch: f32) {
    let waist = waist_pitch.clamp(-AIM_PITCH_CLAMP_RAD, AIM_PITCH_CLAMP_RAD);
    let torso = torso_pitch.clamp(-AIM_PITCH_CLAMP_RAD, AIM_PITCH_CLAMP_RAD);
    let upper = torso - waist;
    pitch_spine_bone(dobj, locals, "j_spinelower", waist);
    pitch_spine_bone(dobj, locals, "j_spineupper", upper * 0.5);
    pitch_spine_bone(dobj, locals, "j_spine4", upper * 0.5);
}

pub const PLAYER_CONTROLLER_TAGS: [&str; 4] = ["back_low", "back_mid", "back_up", "pelvis"];

pub fn set_angles(angles_deg: [f32; 3]) -> [f32; 4] {
    const HALF: f32 = 0.008_726_646;
    let (yaw_s, yaw_c) = (angles_deg[1] * HALF).sin_cos();
    let (pitch_s, pitch_c) = (angles_deg[0] * HALF).sin_cos();
    let (roll_s, roll_c) = (angles_deg[2] * HALF).sin_cos();
    let t0 = -pitch_s * yaw_s;
    let t1 = pitch_s * yaw_c;
    let t2 = pitch_c * yaw_s;
    let t3 = pitch_c * yaw_c;
    [
        roll_s * t3 + roll_c * t0,
        roll_c * t1 + roll_s * t2,
        -roll_s * t1 + roll_c * t2,
        roll_c * t3 - roll_s * t0,
    ]
}

pub fn set_control_tag_angles(locals: &mut [Local], bone_index: usize, angles_deg: [f32; 3]) {
    let Some(local) = locals.get_mut(bone_index) else {
        return;
    };
    local.rotation = set_angles(angles_deg);
    local.control = true;
    local.translation = VEC3_ZERO;
}

pub fn set_local_tag(
    locals: &mut [Local],
    bone_index: usize,
    trans: [f32; 3],
    angles_deg: [f32; 3],
) {
    let Some(local) = locals.get_mut(bone_index) else {
        return;
    };
    local.rotation = set_angles(angles_deg);
    local.translation = trans;
    local.control = false;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerControllerInput {
    pub view_pitch_deg: f32,
    pub prone: bool,
    pub crouch: bool,
    pub lean_frac: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerControllerResult {
    pub tags: u8,

    pub tag_origin: bool,
    pub tag_origin_offset: [f32; 3],
    pub tag_origin_angles: [f32; 3],
}

const LEAN_SHIFT_LEFT: f32 = 5.0;
const LEAN_SHIFT_RIGHT: f32 = 2.5;
const LEAN_SHIFT_CROUCH_LEFT: f32 = 12.5;
const LEAN_SHIFT_CROUCH_RIGHT: f32 = 13.0;
const LEAN_ROTATE_LEFT: f32 = 1.25;
const LEAN_ROTATE_RIGHT: f32 = 1.25;
const LEAN_ROTATE_CROUCH_LEFT: f32 = 1.25;
const LEAN_ROTATE_CROUCH_RIGHT: f32 = 1.0;
const LEAN_TORSO_ROLL: f32 = 46.25;
const LEAN_TAG_ORIGIN_ROLL: f32 = 3.75;
const PRONE_TAG_ORIGIN_X: f32 = -24.0;
const PRONE_TAG_ORIGIN_Y: f32 = -12.0;

pub fn apply_player_controller(
    dobj: &DObj,
    locals: &mut [Local],
    input: PlayerControllerInput,
) -> PlayerControllerResult {
    let plan = plan_player_controller(input);
    let mut tags = 0u8;
    for (i, name) in PLAYER_CONTROLLER_TAGS.iter().enumerate() {
        if let Some(bone) = dobj.find(name) {
            set_control_tag_angles(locals, bone, plan.angles[i]);
            tags += 1;
        }
    }
    let tag_origin = !locals.is_empty();
    if tag_origin {
        set_local_tag(locals, 0, plan.tag_origin_offset, plan.tag_origin_angles);
    }
    PlayerControllerResult {
        tags,
        tag_origin,
        tag_origin_offset: plan.tag_origin_offset,
        tag_origin_angles: plan.tag_origin_angles,
    }
}

pub fn player_controller_tag_origin(input: PlayerControllerInput) -> ([f32; 3], [f32; 3]) {
    let plan = plan_player_controller(input);
    (plan.tag_origin_offset, plan.tag_origin_angles)
}

struct ControllerPlan {
    angles: [[f32; 3]; 4],
    tag_origin_offset: [f32; 3],
    tag_origin_angles: [f32; 3],
}

fn plan_player_controller(input: PlayerControllerInput) -> ControllerPlan {
    let mut torso = [0.0_f32, 0.0, 0.0];
    let mut tag_origin_angles = [0.0_f32, 0.0, 0.0];
    let mut tag_origin_offset = [0.0_f32, 0.0, 0.0];

    torso[0] = 2.0 * angle_normalize_180(input.view_pitch_deg);
    if input.prone {
        torso[0] = angle_normalize_180(torso[0]);
        torso[0] = if torso[0] <= 0.0 {
            torso[0] * 0.25
        } else {
            torso[0] * 0.5
        };
    }
    torso[1] -= tag_origin_angles[1];

    let lean = input.lean_frac;
    torso[2] = lean * LEAN_TORSO_ROLL;
    if lean != 0.0 {
        let shift = if input.crouch {
            if lean <= 0.0 {
                LEAN_SHIFT_CROUCH_LEFT
            } else {
                LEAN_SHIFT_CROUCH_RIGHT
            }
        } else if lean <= 0.0 {
            LEAN_SHIFT_LEFT
        } else {
            LEAN_SHIFT_RIGHT
        };
        tag_origin_offset[1] = -lean * shift;
    }

    let mut angles = [[0.0_f32; 3]; 4];
    if input.prone {
        let yaw = torso[1].to_radians();
        let (s, c) = yaw.sin_cos();
        tag_origin_offset[0] += (1.0 - c) * PRONE_TAG_ORIGIN_X;
        tag_origin_offset[1] += s * PRONE_TAG_ORIGIN_Y;
        if lean * s > 0.0 {
            tag_origin_offset[1] += -lean * (1.0 - c) * 16.0;
        }
        angles[0] = [0.0, torso[2] * -1.2, torso[2] * 0.3];
        angles[1] = [0.0, torso[1] * 0.1 - torso[2] * 0.2, torso[2] * 0.2];
        angles[2] = [torso[0], torso[1] * 0.8 + torso[2], torso[2] * -0.2];
    } else {
        if lean != 0.0 {
            let rot = if input.crouch {
                if lean <= 0.0 {
                    LEAN_ROTATE_CROUCH_LEFT
                } else {
                    LEAN_ROTATE_CROUCH_RIGHT
                }
            } else if lean <= 0.0 {
                LEAN_ROTATE_LEFT
            } else {
                LEAN_ROTATE_RIGHT
            };
            torso[2] *= rot;
        }
        tag_origin_angles[2] = lean * LEAN_TAG_ORIGIN_ROLL;
        angles[0] = [torso[0] * 0.2, torso[1] * 0.4, torso[2] * 0.5];
        angles[1] = [torso[0] * 0.3, torso[1] * 0.4, torso[2] * 0.5];
        angles[2] = [torso[0] * 0.5, torso[1] * 0.2, torso[2] * -0.6];
    }
    angles[3] = [0.0, 0.0, 0.0];
    ControllerPlan {
        angles,
        tag_origin_offset,
        tag_origin_angles,
    }
}

pub fn apply_standing_player_controller(
    dobj: &DObj,
    locals: &mut [Local],
    view_pitch_deg: f32,
) -> u8 {
    apply_player_controller(
        dobj,
        locals,
        PlayerControllerInput {
            view_pitch_deg,
            prone: false,
            crouch: false,
            lean_frac: 0.0,
        },
    )
    .tags
}

fn angle_normalize_180(deg: f32) -> f32 {
    let turns = deg * 0.002_777_777_845_039_964;
    (turns - (turns + 0.5).floor()) * 360.0
}

pub fn yaw_bone(dobj: &DObj, locals: &mut [Local], name: &str, angle: f32) {
    let Some(i) = dobj.find(name) else {
        return;
    };
    let (_, bind_rot, _) = dobj.bones[i].bind_world.to_scale_rotation_translation();
    let axis = (bind_rot.inverse() * Vec3::Z).normalize_or_zero();
    if axis == Vec3::ZERO {
        return;
    }
    let q = Quat::from_axis_angle(axis, angle);
    let r = Quat::from_xyzw(
        locals[i].rotation[0],
        locals[i].rotation[1],
        locals[i].rotation[2],
        locals[i].rotation[3],
    );
    let out = (r * q).normalize();
    locals[i].rotation = [out.x, out.y, out.z, out.w];
}

pub fn pitch_spine_bone(dobj: &DObj, locals: &mut [Local], name: &str, model_pitch: f32) {
    if model_pitch.abs() < 1e-6 {
        return;
    }
    let Some(i) = dobj.find(name) else {
        return;
    };
    let (_, bind_rot, _) = dobj.bones[i].bind_world.to_scale_rotation_translation();

    let axis = (bind_rot.inverse() * Vec3::Y).normalize_or_zero();
    if axis == Vec3::ZERO {
        return;
    }
    let q = Quat::from_axis_angle(axis, -model_pitch);
    let r = Quat::from_xyzw(
        locals[i].rotation[0],
        locals[i].rotation[1],
        locals[i].rotation[2],
        locals[i].rotation[3],
    );
    let out = (r * q).normalize();
    locals[i].rotation = [out.x, out.y, out.z, out.w];
}

pub const TP_WEAPON_ATTACH_TAGS: &[&str] = &["tag_weapon_right", "tag_inhand", "tag_weapon_left"];

pub const TP_HEAD_ATTACH_TAG: &str = "j_spine4";

pub fn tp_weapon_attach_tag(body_bone_names: &[String]) -> Option<&'static str> {
    TP_WEAPON_ATTACH_TAGS
        .iter()
        .copied()
        .find(|tag| body_bone_names.iter().any(|n| n == tag))
}

pub fn tp_head_attach_tag(body_bone_names: &[String]) -> Option<&'static str> {
    body_bone_names
        .iter()
        .any(|n| n == TP_HEAD_ATTACH_TAG)
        .then_some(TP_HEAD_ATTACH_TAG)
}

pub fn build_body_head_weapon_dobj(
    body: &ModelPoseSrc,
    head: Option<&ModelPoseSrc>,
    weapon: &ModelPoseSrc,
) -> Option<(DObj, &'static str, Option<usize>)> {
    let gun_tag = tp_weapon_attach_tag(&body.bone_names)?;
    let mut models: Vec<(&ModelPoseSrc, Option<Attach>)> = Vec::with_capacity(3);
    models.push((body, None));
    if let Some(head) = head {
        let head_tag = tp_head_attach_tag(&body.bone_names)?;
        models.push((
            head,
            Some(Attach {
                parent_model: 0,
                tag: head_tag.into(),
            }),
        ));
    }
    models.push((
        weapon,
        Some(Attach {
            parent_model: 0,
            tag: gun_tag.into(),
        }),
    ));
    let dobj = DObj::build(&models).ok()?;
    let flash = dobj.find("tag_flash");
    Some((dobj, gun_tag, flash))
}

pub fn build_body_weapon_dobj(
    body: &ModelPoseSrc,
    weapon: &ModelPoseSrc,
) -> Option<(DObj, &'static str, Option<usize>)> {
    build_body_head_weapon_dobj(body, None, weapon)
}

pub struct AnimInstance<'a> {
    pub clip: &'a AnimClip,
    pub tracks: &'a [Option<usize>],
    pub time: f32,
    pub weight: f32,
    pub parts: Option<&'a PartBits>,
}
