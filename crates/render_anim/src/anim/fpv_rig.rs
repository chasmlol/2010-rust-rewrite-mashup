//! The prepared first-person rig.
//!

use std::collections::HashMap;
use std::sync::Arc;

use bevy::math::{Mat4, Vec3};
use render_scene::SmodelPassMaterial;

use crate::anim::fpv_pose::{
    FpvBoltFrame, FpvBoltTags, PosedClip, tag_camera_lens_local, tag_view_to_bevy_camera,
};
use crate::anim::xmodel_pose::{FpvSurfOwner, SkinLayout, build_skin_layout, skin_packed_into};
use crate::draw::FpvSurfaceDraw;
use anim_iw4::PartBits;
use anim_iw4::surface_hidden;
use asset_game::{FpvAssembly, FpvClipTracks, FpvPartRole};
use asset_model::FpvMeshCatalog;
use xmodel_runtime::AnimInstance;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FpvSurfaceVerdict {
    Admitted(u32),
    Inapplicable(&'static str),
    Refused {
        material: String,
        cause: &'static str,
    },
}

#[derive(Default)]
pub struct FpvMaterialAdmission {
    pub materials: Vec<SmodelPassMaterial>,
    pub by_authored: HashMap<usize, u32>,
    verdicts: HashMap<usize, Vec<(usize, FpvSurfaceVerdict)>>,
}

impl FpvMaterialAdmission {
    pub fn record(&mut self, catalog_entry: usize, verdicts: Vec<(usize, FpvSurfaceVerdict)>) {
        self.verdicts.insert(catalog_entry, verdicts);
    }

    pub fn verdict(&self, catalog_entry: usize, surface: usize) -> Option<&FpvSurfaceVerdict> {
        self.verdicts
            .get(&catalog_entry)?
            .iter()
            .find(|(index, _)| *index == surface)
            .map(|(_, verdict)| verdict)
    }

    pub fn verdicts_of(&self, catalog_entry: usize) -> &[(usize, FpvSurfaceVerdict)] {
        self.verdicts
            .get(&catalog_entry)
            .map_or(&[], |verdicts| verdicts.as_slice())
    }
}

#[derive(Clone, Copy, Debug)]
struct PreparedFpvSurface {
    index_start: u32,
    index_count: u32,
    material: u32,
    authored: usize,
    lens_named: bool,
}

pub struct PreparedFpvModel {
    hide: Option<[u32; 6]>,
    mesh_identity: u64,
    catalog_entry: usize,
    posed_surface_n: usize,
    packed_ok: bool,
    layout: SkinLayout,
    surfaces: Vec<PreparedFpvSurface>,
    refusal: Option<FpvMandatoryRefusal>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FpvMandatoryRefusal {
    pub model: String,
    pub surface: usize,
    pub material: String,
    pub cause: &'static str,
}

impl core::fmt::Display for FpvMandatoryRefusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} surface {} `{}`: {}",
            self.model, self.surface, self.material, self.cause
        )
    }
}

#[derive(Debug)]
pub enum FpvRigError {
    Catalog(&'static str),
    Layout(&'static str),
}

impl core::fmt::Display for FpvRigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Catalog(what) => write!(f, "{what} missing from the first-person catalog"),
            Self::Layout(what) => write!(f, "{what}"),
        }
    }
}

pub fn leftover_scope_surf_is_lens(name: &str) -> bool {
    name.contains("lens")
}

impl PreparedFpvModel {
    pub fn build(
        catalog: &FpvMeshCatalog,
        catalog_entry: usize,
        hide: Option<&[u32; 6]>,
        admission: &FpvMaterialAdmission,
    ) -> Result<Self, FpvRigError> {
        if catalog.identity() == 0 {
            return Err(FpvRigError::Catalog("published mesh owner"));
        }
        let entry = catalog
            .get_at(catalog_entry)
            .ok_or(FpvRigError::Catalog("model"))?;
        let skel = &entry.skel;
        let lod_range = skel.surfaces_for_lod(0);
        let first_surface = lod_range.start;
        let hidden = |surface: usize| {
            hide.is_some_and(|words| {
                skel.surface_part_bits
                    .get(surface)
                    .is_some_and(|bits| surface_hidden(bits, words, 0))
            })
        };
        let admitted = |surface: usize| match admission.verdict(catalog_entry, surface) {
            Some(FpvSurfaceVerdict::Admitted(row)) => Some(*row),
            _ => None,
        };
        let layout = build_skin_layout(skel, 0, |lod_local| {
            let surface = first_surface + lod_local;
            !hidden(surface) && admitted(surface).is_some()
        })
        .ok_or(FpvRigError::Layout("skin layout refused the model"))?;

        let mut refusal = None;
        for surface in lod_range.clone() {
            if hidden(surface) {
                continue;
            }
            if let Some(FpvSurfaceVerdict::Refused { material, cause }) =
                admission.verdict(catalog_entry, surface)
            {
                refusal = Some(FpvMandatoryRefusal {
                    model: skel.name.clone(),
                    surface,
                    material: material.clone(),
                    cause: *cause,
                });
                break;
            }
        }

        let mut surfaces = Vec::new();
        for surface in &layout.surfaces {
            if !surface.visible || surface.index_count == 0 {
                continue;
            }
            let Some(authored) = entry
                .material_edges
                .get(surface.surface_index)
                .and_then(|edge| edge.bound_index())
            else {
                continue;
            };
            let Some(material) = admitted(surface.surface_index) else {
                continue;
            };
            surfaces.push(PreparedFpvSurface {
                index_start: surface.index_start,
                index_count: surface.index_count,
                material,
                authored,
                lens_named: entry
                    .material_keys
                    .get(surface.surface_index)
                    .and_then(|key| Some(key.as_ref()?.name.as_str()))
                    .is_some_and(leftover_scope_surf_is_lens),
            });
        }
        Ok(Self {
            hide: hide.copied(),
            mesh_identity: catalog.identity(),
            catalog_entry,
            posed_surface_n: lod_range.len(),
            packed_ok: skel.packed_vertices.len() == skel.positions.len(),
            layout,
            surfaces,
            refusal,
        })
    }

    pub fn refusal(&self) -> Option<&FpvMandatoryRefusal> {
        self.refusal.as_ref()
    }
}

struct ComposedPart {
    model: Arc<PreparedFpvModel>,
    owner: FpvSurfOwner,
    bone_base: usize,
}

pub struct PreparedFpvComposition {
    assembly: Arc<FpvAssembly>,
    parts: Vec<ComposedPart>,
    refusal: Option<FpvMandatoryRefusal>,
}

fn surf_owner(role: FpvPartRole) -> FpvSurfOwner {
    match role {
        FpvPartRole::Hands => FpvSurfOwner::Hands,
        FpvPartRole::Gun | FpvPartRole::Knife => FpvSurfOwner::Gun,
        FpvPartRole::Attachment => FpvSurfOwner::Scope,
        FpvPartRole::Rocket => FpvSurfOwner::Rocket,
    }
}

impl PreparedFpvComposition {
    pub fn compose(
        assembly: Arc<FpvAssembly>,
        mut model_of: impl FnMut(usize, Option<[u32; 6]>) -> Result<Arc<PreparedFpvModel>, String>,
    ) -> Result<Self, String> {
        let mut parts = Vec::with_capacity(assembly.parts.len());
        let mut refusal = None;
        for part in &assembly.parts {
            let model = model_of(part.model.order(), part.hide)?;
            if model.mesh_identity != assembly.mesh_identity()
                || model.catalog_entry != part.model.order()
                || model.hide != part.hide
            {
                return Err("FPV model or hide layout differs from the prepared assembly".into());
            }
            if refusal.is_none() {
                refusal = model.refusal().cloned();
            }
            parts.push(ComposedPart {
                model,
                owner: surf_owner(part.role),
                bone_base: part.bone_base,
            });
        }
        Ok(Self {
            assembly,
            parts,
            refusal,
        })
    }

    pub fn assembly(&self) -> &Arc<FpvAssembly> {
        &self.assembly
    }

    pub fn refusal(&self) -> Option<&FpvMandatoryRefusal> {
        self.refusal.as_ref()
    }
}

pub fn compose_clip_tracks(
    assembly: &FpvAssembly,
    clip_index: usize,
    clip: &asset_anim::AnimClip,
    tracks: &FpvClipTracks,
) -> Option<Arc<[u16]>> {
    let composed = assembly.compose_tracks(clip_index, clip, tracks)?;
    Some(
        composed
            .into_iter()
            .map(|bone| {
                bone.and_then(|bone| u16::try_from(bone).ok())
                    .unwrap_or(FpvClipTracks::NONE)
            })
            .collect(),
    )
}

/// One model drawn for one hand. The same prepared model appears twice in a
/// dual-wield rig — once per hand — with a destination range of its own.
struct PlanSlot {
    hand: usize,
    part: usize,
    dest_base: usize,
}

pub struct PreparedFpvGeometry {
    segments: Vec<(Arc<PreparedFpvModel>, u32)>,
    pub index_n: usize,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub material_authored: Vec<usize>,
    pub draws: Vec<FpvSurfaceDraw>,
    pub dest_n: usize,
    pub packed_ok: bool,

    pub hands_plan_n: u32,
    pub gun_plan_n: u32,
    pub scope_plan_n: u32,
    pub scope_house_plan_n: u32,
    pub scope_lens_plan_n: u32,
    pub plan_draw_n: u32,
    pub plan_skip_n: u32,
}

impl PreparedFpvGeometry {
    pub fn write_indices(&self, out: &mut Vec<u32>) {
        out.clear();
        out.reserve(self.index_n);
        for (model, rebase) in &self.segments {
            out.extend(model.layout.indices.iter().map(|index| index + rebase));
        }
    }
}

pub struct FpvHandPose {
    generation: u64,
    skin: Vec<Mat4>,
    eye_from_world: Mat4,
    offset: Vec3,
    pub lens: Mat4,
    pub bolt: FpvBoltFrame,
}

pub struct PreparedFpvRig {
    meshes: Arc<FpvMeshCatalog>,
    clips: [Vec<Option<Arc<asset_anim::AnimClip>>>; 2],
    composition: Arc<PreparedFpvComposition>,
    dual: bool,
    generation: u64,
    parts: PartBits,
    tags: FpvBoltTags,
    secondary_tags: Option<FpvBoltTags>,
    slots: Vec<PlanSlot>,
    tracks: [Vec<Option<Arc<[u16]>>>; 2],
    hand_parts: Option<[PartBits; 2]>,
    pub geometry: PreparedFpvGeometry,
}

impl PreparedFpvRig {
    /// A plan holding this generation is a plan whose indices, ranges,
    /// materials and draws are this rig's.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_dual(&self) -> bool {
        self.dual
    }

    pub(super) fn build(
        composition: Arc<PreparedFpvComposition>,
        dual: bool,
        admission: &FpvMaterialAdmission,
        tracks: [Vec<Option<Arc<[u16]>>>; 2],
        meshes: Arc<FpvMeshCatalog>,
        clips: [Vec<Option<Arc<asset_anim::AnimClip>>>; 2],
    ) -> Self {
        assert_eq!(composition.assembly.mesh_identity(), meshes.identity());
        let parts_n = composition.parts.len();

        // Every hand's view hands first, then every hand's gun and whatever
        // hangs off it.
        let hand_n = if dual && !composition.assembly.combined_hands {
            2
        } else {
            1
        };
        let mut order: Vec<(usize, usize)> = Vec::new();
        for hand in 0..hand_n {
            order.push((hand, 0));
        }
        for hand in 0..hand_n {
            for part in 1..parts_n {
                // The scope and the rocket hang off the right gun only.
                if hand == 1 && part != 1 {
                    continue;
                }
                order.push((hand, part));
            }
        }

        let mut geometry = PreparedFpvGeometry {
            segments: Vec::with_capacity(order.len()),
            index_n: 0,
            surface_ranges: Vec::new(),
            materials: Vec::new(),
            material_authored: Vec::new(),
            draws: Vec::new(),
            dest_n: 0,
            packed_ok: true,
            hands_plan_n: 0,
            gun_plan_n: 0,
            scope_plan_n: 0,
            scope_house_plan_n: 0,
            scope_lens_plan_n: 0,
            plan_draw_n: 0,
            plan_skip_n: 0,
        };
        let mut slots: Vec<PlanSlot> = Vec::with_capacity(order.len());
        let mut material_key: HashMap<usize, u32> = HashMap::new();
        let mut dest_base = 0usize;
        let mut posed_surface_n = 0usize;
        for (hand, part_index) in order {
            let part = &composition.parts[part_index];
            let model = &part.model;
            posed_surface_n += model.posed_surface_n;
            geometry.packed_ok &= model.packed_ok;
            let index_base = geometry.index_n as u32;
            for surface in &model.surfaces {
                let Some(material) = admission.materials.get(surface.material as usize) else {
                    continue;
                };
                let is_scope = part.owner == FpvSurfOwner::Scope;
                let is_lens = is_scope && surface.lens_named;
                let range_index = geometry.surface_ranges.len() as u32;
                geometry
                    .surface_ranges
                    .push((index_base + surface.index_start, surface.index_count));
                let material_index = *material_key.entry(surface.authored).or_insert_with(|| {
                    let index = geometry.materials.len() as u32;
                    geometry.materials.push(material.clone());
                    geometry.material_authored.push(surface.authored);
                    index
                });
                geometry.draws.push(FpvSurfaceDraw {
                    surface: range_index,
                    material: material_index,
                    is_scope: is_scope && !is_lens,
                    hands: part.owner == FpvSurfOwner::Hands,
                });
                if hand == 0 {
                    match part.owner {
                        FpvSurfOwner::Hands => geometry.hands_plan_n += 1,
                        _ => geometry.gun_plan_n += 1,
                    }
                }
                if is_scope {
                    geometry.scope_plan_n += 1;
                    if is_lens {
                        geometry.scope_lens_plan_n += 1;
                    } else {
                        geometry.scope_house_plan_n += 1;
                    }
                }
            }
            geometry
                .segments
                .push((Arc::clone(model), dest_base as u32));
            geometry.index_n += model.layout.indices.len();
            slots.push(PlanSlot {
                hand,
                part: part_index,
                dest_base,
            });
            dest_base += model.layout.dest_vertex_n;
        }
        geometry.dest_n = dest_base;
        geometry.plan_draw_n = geometry.draws.len() as u32;
        geometry.plan_skip_n = posed_surface_n.saturating_sub(geometry.draws.len()) as u32;

        let assembly = &composition.assembly;
        let tags = FpvBoltTags {
            flash: assembly.tags.flash,
            flash_silenced: assembly.tags.flash_silenced,
            brass: assembly.tags.brass,
            knife: assembly.tags.knife,
            laser: assembly.tags.laser,
            tracker_screen: assembly.tags.tracker_screen,
            tracker_light: assembly.tags.tracker_light,
        };
        let secondary_tags = assembly.combined_hands.then(|| {
            let tag = |name: &str| {
                assembly
                    .dobj
                    .find(name)
                    .and_then(|index| u16::try_from(index).ok())
            };
            FpvBoltTags {
                flash: tag("tag_flash1"),
                flash_silenced: tag("tag_flash_silenced1"),
                brass: tag("tag_brass1"),
                ..FpvBoltTags::default()
            }
        });
        let parts = assembly.dobj.all_parts();

        static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let hand_parts = composition.assembly.combined_hands.then(|| {
            let mut masks = [PartBits::default(); 2];
            let bones = &composition.assembly.dobj.bones;
            for (index, bone) in bones.iter().enumerate() {
                let mut ancestor = Some(index);
                let mut left = bone.model == 2;
                while let Some(parent) = ancestor {
                    let name = &bones[parent].name;
                    if name == "j_shoulder_le" || name == "tag_weapon1" {
                        left = true;
                        break;
                    }
                    ancestor = bones[parent].parent;
                }
                masks[usize::from(left)].set(index);
            }
            masks
        });
        Self {
            hand_parts,
            meshes,
            clips,
            composition,
            dual,
            generation: GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            parts,
            tags,
            secondary_tags,
            slots,
            tracks,
            geometry,
        }
    }

    /// One hand's bones for this frame. No skeleton is built here.
    pub fn pose_hand(
        &self,
        hand: usize,
        anims: &[PosedClip<'_>],
        offset: Vec3,
    ) -> Option<FpvHandPose> {
        self.pose_clips(hand, anims, &[], offset)
    }

    pub fn combines_hands(&self) -> bool {
        self.composition.assembly.combined_hands
    }

    pub fn secondary_bolt(&self, pose: &FpvHandPose) -> Option<FpvBoltFrame> {
        if pose.generation != self.generation {
            return None;
        }
        Some(FpvBoltFrame {
            bones: pose.bolt.bones.clone(),
            tags: self.secondary_tags?,
        })
    }

    pub fn pose_combined(
        &self,
        right: &[PosedClip<'_>],
        left: &[PosedClip<'_>],
    ) -> Option<FpvHandPose> {
        self.pose_clips(0, right, left, Vec3::ZERO)
    }

    fn pose_clips(
        &self,
        hand: usize,
        anims: &[PosedClip<'_>],
        left: &[PosedClip<'_>],
        offset: Vec3,
    ) -> Option<FpvHandPose> {
        if hand >= 2 || anims.is_empty() {
            return None;
        }
        if !anims.iter().all(|anim| self.owns_clip(hand, anim))
            || !left.iter().all(|anim| self.owns_clip(1, anim))
        {
            return None;
        }
        let bound: Vec<(&PosedClip<'_>, Vec<Option<usize>>, usize)> = anims
            .iter()
            .map(|anim| (anim, hand))
            .chain(left.iter().map(|anim| (anim, 1)))
            .filter_map(|(anim, source)| {
                let tracks = self.tracks[source].get(anim.node)?.as_ref()?;
                let bones = tracks
                    .iter()
                    .map(|&bone| (bone != FpvClipTracks::NONE).then_some(usize::from(bone)))
                    .collect();
                Some((anim, bones, source))
            })
            .collect();
        if !bound
            .iter()
            .any(|(_, bones, _)| bones.iter().any(Option::is_some))
        {
            return None;
        }
        let full_body = |anim: &PosedClip<'_>| {
            anim.node != asset_game::WeaponAnimSlot::AdsUp.index()
                && anim.node != asset_game::WeaponAnimSlot::AdsDown.index()
                && anim
                    .clip
                    .tracks
                    .iter()
                    .any(|track| track.name == "j_shoulder_le")
        };
        let full_weight = if self.combines_hands() {
            bound
                .iter()
                .filter(|(anim, _, source)| *source == 0 && full_body(anim))
                .map(|(anim, _, _)| anim.weight)
                .sum::<f32>()
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let instances: Vec<AnimInstance<'_>> = bound
            .iter()
            .map(|(anim, bones, source)| AnimInstance {
                clip: anim.clip,
                tracks: bones,
                time: anim.time,
                weight: anim.weight * if *source == 1 { 1.0 - full_weight } else { 1.0 },
                parts: self.hand_parts.as_ref().and_then(|masks| {
                    if *source == 0 && full_body(anim) {
                        None
                    } else {
                        Some(&masks[*source])
                    }
                }),
            })
            .collect();
        let assembly = &self.composition.assembly;
        let world = assembly.dobj.pose(&instances, &self.parts, Mat4::IDENTITY);
        let mut skin = assembly.dobj.skin_matrices(&world);
        for &bone in &assembly.collapsed_bones {
            if let (Some(skin), Some(world)) = (skin.get_mut(bone), world.get(bone)) {
                *skin = Mat4::from_cols(
                    bevy::math::Vec4::ZERO,
                    bevy::math::Vec4::ZERO,
                    bevy::math::Vec4::ZERO,
                    world.w_axis,
                );
            }
        }
        let eye_from_world = tag_view_to_bevy_camera() * world[assembly.view_bone].inverse();
        let lens = assembly
            .camera_bone
            .map(|camera| tag_camera_lens_local(world[assembly.view_bone], world[camera]))
            .unwrap_or(Mat4::IDENTITY);
        let published = if hand == 0 {
            world.len()
        } else {
            assembly.paired_bones.min(world.len())
        };
        let shift = (offset != Vec3::ZERO).then(|| Mat4::from_translation(offset));
        let bones = world[..published]
            .iter()
            .map(|bone| {
                let bone = eye_from_world * *bone;
                match shift {
                    Some(shift) => shift * bone,
                    None => bone,
                }
            })
            .collect();
        Some(FpvHandPose {
            generation: self.generation,
            skin,
            eye_from_world,
            offset,
            lens,
            bolt: FpvBoltFrame {
                bones,
                tags: self.tags,
            },
        })
    }

    fn owns_clip(&self, hand: usize, anim: &PosedClip<'_>) -> bool {
        self.clips
            .get(hand)
            .and_then(|clips| clips.get(anim.node))
            .and_then(Option::as_ref)
            .is_some_and(|clip| std::ptr::eq(clip.as_ref(), anim.clip))
    }

    /// Skin every slot into the destination buffer the plan published. A vertex
    /// update only: indices, surface ranges and materials are untouched.
    pub fn skin_into(
        &self,
        poses: &[Option<FpvHandPose>; 2],
        dest: &mut [[u8; asset_iw4::size::GFX_PACKED_VERTEX]],
    ) -> bool {
        if poses
            .iter()
            .flatten()
            .any(|pose| pose.generation != self.generation)
        {
            return false;
        }
        for slot in &self.slots {
            let Some(pose) = poses[slot.hand].as_ref() else {
                continue;
            };
            let part = &self.composition.parts[slot.part];
            let model = &part.model;
            let Some(skel) = self
                .meshes
                .get_at(model.catalog_entry)
                .map(|entry| &entry.skel)
            else {
                continue;
            };
            let end = slot.dest_base.saturating_add(model.layout.dest_vertex_n);
            if end > dest.len() {
                continue;
            }
            let rows = &mut dest[slot.dest_base..end];
            skin_packed_into(
                skel,
                |bone| pose.eye_from_world * pose.skin[part.bone_base + bone],
                |_| false,
                0,
                &model.layout,
                rows,
            );
            if pose.offset != Vec3::ZERO {
                translate_packed_rows(rows, pose.offset);
            }
        }
        true
    }
}

fn translate_packed_rows(rows: &mut [[u8; asset_iw4::size::GFX_PACKED_VERTEX]], delta: Vec3) {
    for row in rows {
        let x = f32::from_le_bytes([row[0], row[1], row[2], row[3]]) + delta.x;
        let y = f32::from_le_bytes([row[4], row[5], row[6], row[7]]) + delta.y;
        let z = f32::from_le_bytes([row[8], row[9], row[10], row[11]]) + delta.z;
        row[0..4].copy_from_slice(&x.to_le_bytes());
        row[4..8].copy_from_slice(&y.to_le_bytes());
        row[8..12].copy_from_slice(&z.to_le_bytes());
    }
}
