use std::collections::HashMap;
use std::sync::Arc;

use anim_iw4::set_hide_part_bit;
use asset_model::{FpvMeshCatalog, FpvMountPlan};
use xmodel_runtime::{AnimClip, Attach, DObj, DObjError, ModelPoseSrc};

use crate::FpvMeshIndex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FpvPartRole {
    Hands,
    Gun,
    Attachment,
    Rocket,
    Knife,
}

#[derive(Clone, Debug)]
pub struct FpvAssemblyPart {
    pub model: FpvMeshIndex,
    pub role: FpvPartRole,
    pub bone_base: usize,
    pub hide: Option<[u32; 6]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FpvAssemblyTags {
    pub flash: Option<u16>,
    pub flash_silenced: Option<u16>,
    pub brass: Option<u16>,
    pub knife: Option<u16>,
    pub laser: Option<u16>,
    pub tracker_screen: [Option<u16>; 3],
    pub tracker_light: Option<u16>,
}

#[derive(Debug)]
pub struct FpvAssembly {
    pub family: crate::FpvFamilyConnection,
    mesh_identity: u64,
    pub dobj: Arc<DObj>,
    pub parts: Vec<FpvAssemblyPart>,
    pub view_bone: usize,
    pub camera_bone: Option<usize>,
    pub paired_bones: usize,
    pub combined_hands: bool,
    pub tags: FpvAssemblyTags,
    pub collapsed_bones: Vec<usize>,
}

#[derive(Default)]
pub struct FpvSkeletons(HashMap<SkeletonModels, Arc<DObj>>);

type SkeletonModels = Vec<(usize, Option<(usize, String)>)>;

impl FpvSkeletons {
    fn build(
        &mut self,
        specs: &[(&ModelPoseSrc, Option<Attach>)],
    ) -> Result<Arc<DObj>, FpvAssemblyError> {
        let key = specs
            .iter()
            .map(|(model, attach)| {
                (
                    std::ptr::from_ref(*model) as usize,
                    attach.as_ref().map(|a| (a.parent_model, a.tag.clone())),
                )
            })
            .collect();
        if let Some(dobj) = self.0.get(&key) {
            return Ok(Arc::clone(dobj));
        }
        let dobj = Arc::new(DObj::build(specs).map_err(FpvAssemblyError::Skeleton)?);
        self.0.insert(key, Arc::clone(&dobj));
        Ok(dobj)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FpvHideMode {
    #[default]
    Surfaces,
    Bones,
}

#[derive(Debug)]
pub enum FpvAssemblyError {
    Catalog(&'static str),
    Skeleton(DObjError),
    NoTagView,
}

impl core::fmt::Display for FpvAssemblyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Catalog(what) => write!(f, "{what} missing from the first-person catalog"),
            Self::Skeleton(error) => write!(f, "combined skeleton: {error}"),
            Self::NoTagView => write!(f, "combined skeleton has no tag_view"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FpvAssemblyKey {
    pub hands: FpvMeshIndex,
    pub gun: FpvMeshIndex,
    pub secondary_gun: Option<FpvMeshIndex>,
    pub attachments: Vec<FpvMeshIndex>,
    pub rocket: Option<FpvMeshIndex>,
    pub knife: Option<FpvMeshIndex>,
    pub hide_tags: Vec<String>,
    pub hide_mode: FpvHideMode,
    pub jammed: bool,
}

#[derive(Clone, Debug)]
pub struct FpvSideAssemblies {
    pub bare: Arc<FpvAssembly>,
    pub rocket: Option<Arc<FpvAssembly>>,
    pub melee: Option<Arc<FpvAssembly>>,
    pub ads: Option<Arc<FpvAssembly>>,
    pub jammed: Option<Arc<FpvAssembly>>,
}

pub const EMP_RETICLE_TAGS: [&str; 5] = [
    "tag_reticle_acog",
    "tag_reticle_red_dot",
    "tag_eotech_reticle",
    "tag_reticle_tavor_scope",
    "tag_reticle_thermal_scope",
];

impl FpvSideAssemblies {
    pub fn pick(&self, rocket: bool) -> &Arc<FpvAssembly> {
        match (&self.rocket, rocket) {
            (Some(with), true) => with,
            _ => &self.bare,
        }
    }
}

fn hide_words(
    catalog: &FpvMeshCatalog,
    model: FpvMeshIndex,
    hide_tags: &[String],
) -> Option<[u32; 6]> {
    let skel = &catalog.get_at(model.order())?.skel;
    if hide_tags.is_empty() || skel.surface_part_bits.len() != skel.surface_vertex_ranges.len() {
        return None;
    }
    let mut words = [0u32; 6];
    for bone in 0..skel.bone_names.len() {
        if crate::bone_has_hidden_ancestor(&skel.bone_names, |b| skel.parent_of(b), bone, hide_tags)
        {
            set_hide_part_bit(&mut words, bone);
        }
    }
    Some(words)
}

impl FpvAssembly {
    pub fn build(
        catalog: &FpvMeshCatalog,
        hands: FpvMeshIndex,
        mounts: &FpvMountPlan,
        rocket: bool,
        knife: Option<FpvMeshIndex>,
        ads: bool,
        hide_tags: &[String],
        hide_mode: FpvHideMode,
        jammed: bool,
        skeletons: &mut FpvSkeletons,
    ) -> Result<Self, FpvAssemblyError> {
        if catalog.identity() == 0 {
            return Err(FpvAssemblyError::Catalog("published mesh owner"));
        }
        let family = crate::FpvFamilyConnection::bind(catalog, mounts.gun, hands).ok_or(
            FpvAssemblyError::Catalog("compatible gun and hands families"),
        )?;
        let hands = family.hands();
        let reticle_tags: Vec<String> = if jammed {
            EMP_RETICLE_TAGS
                .iter()
                .map(|tag| (*tag).to_owned())
                .collect()
        } else {
            Vec::new()
        };
        let gun_tags: Vec<String> = hide_tags.iter().chain(&reticle_tags).cloned().collect();
        let pose_of = |model: FpvMeshIndex| -> Result<&ModelPoseSrc, FpvAssemblyError> {
            if model != hands
                && catalog
                    .get_at(model.order())
                    .is_none_or(|entry| entry.namespace != family.family())
            {
                return Err(FpvAssemblyError::Catalog("native FPV component family"));
            }
            catalog
                .get_at(model.order())
                .and_then(|entry| entry.skel.pose.as_ref())
                .ok_or(FpvAssemblyError::Catalog("model pose"))
        };
        let mut parts = vec![
            (hands, FpvPartRole::Hands, None),
            (
                family.gun(),
                FpvPartRole::Gun,
                Some(Attach {
                    parent_model: 0,
                    tag: "tag_weapon".into(),
                }),
            ),
        ];
        if let Some(model) = mounts.secondary_gun {
            parts.push((
                model,
                FpvPartRole::Gun,
                Some(Attach {
                    parent_model: 0,
                    tag: "tag_weapon1".into(),
                }),
            ));
        }
        for (mount, model) in mounts.attachments.iter().zip(mounts.attachment_models(ads)) {
            parts.push((
                model,
                FpvPartRole::Attachment,
                Some(Attach {
                    parent_model: mount.parent_model
                        + usize::from(mounts.secondary_gun.is_some() && mount.parent_model >= 2),
                    tag: mount.tag.clone(),
                }),
            ));
        }
        if rocket {
            let mount = mounts
                .rocket
                .as_ref()
                .ok_or(FpvAssemblyError::Catalog("rocket mount"))?;
            parts.push((
                mount.model,
                FpvPartRole::Rocket,
                Some(Attach {
                    parent_model: mount.parent_model,
                    tag: mount.tag.clone(),
                }),
            ));
        }
        if let Some(model) = knife {
            parts.push((
                model,
                FpvPartRole::Knife,
                Some(Attach {
                    parent_model: 0,
                    tag: "tag_knife_attach".into(),
                }),
            ));
        }
        let mut specs = Vec::with_capacity(parts.len());
        for (model, _, attach) in &parts {
            specs.push((pose_of(*model)?, attach.clone()));
        }
        let dobj = skeletons.build(&specs)?;

        let view_bone = dobj.find("tag_view").ok_or(FpvAssemblyError::NoTagView)?;
        let camera_bone = dobj.find("tag_camera");
        let tag = |name: &str| dobj.find(name).and_then(|index| u16::try_from(index).ok());
        let tags = FpvAssemblyTags {
            flash: tag("tag_flash"),
            flash_silenced: tag("tag_flash_silenced"),
            brass: tag("tag_brass"),
            knife: tag("tag_knife_fx"),
            laser: tag(fx_iw4::FX_LASER_TAG),
            tracker_screen: [
                tag("tag_screen_tl"),
                tag("tag_screen_bl"),
                tag("tag_screen_br"),
            ],
            tracker_light: tag("tag_motion_tracker_fx"),
        };
        let paired_bones = specs[0].0.num_bones + specs[1].0.num_bones;
        let mut collapsed_bones = Vec::new();
        let parts = parts
            .into_iter()
            .zip(&dobj.models)
            .map(|((model, role, _), slot)| {
                let hide = match role {
                    FpvPartRole::Gun => hide_words(catalog, model, &gun_tags),
                    FpvPartRole::Attachment => {
                        hide_words(catalog, model, &reticle_tags).filter(|words| *words != [0; 6])
                    }
                    _ => None,
                };
                let hide = match (hide_mode, hide) {
                    (FpvHideMode::Bones, Some(words)) => {
                        collapsed_bones.extend(
                            (0..slot.bone_count)
                                .filter(|&bone| {
                                    words[bone >> 5] & (0x8000_0000u32 >> (bone & 31)) != 0
                                })
                                .map(|bone| slot.base + bone),
                        );
                        None
                    }
                    (_, hide) => hide,
                };
                FpvAssemblyPart {
                    model,
                    role,
                    bone_base: slot.base,
                    hide,
                }
            })
            .collect();
        Ok(Self {
            family,
            mesh_identity: catalog.identity(),
            dobj,
            parts,
            view_bone,
            camera_bone,
            paired_bones,
            combined_hands: mounts.secondary_gun.is_some(),
            tags,
            collapsed_bones,
        })
    }

    pub fn mesh_identity(&self) -> u64 {
        self.mesh_identity
    }

    pub fn compose_tracks(
        &self,
        clip_index: usize,
        clip: &AnimClip,
        tracks: &FpvClipTracks,
    ) -> Option<Vec<Option<usize>>> {
        let tables: Vec<Option<&[u16]>> = self
            .parts
            .iter()
            .map(|part| tracks.table(clip_index, clip, self.mesh_identity, part.model))
            .collect();
        let track_n = tables.iter().flatten().next()?.len();
        Some(
            (0..track_n)
                .map(|track| {
                    self.parts.iter().zip(&tables).find_map(|(part, table)| {
                        let local = *table.as_ref()?.get(track)?;
                        (local != FpvClipTracks::NONE).then(|| part.bone_base + usize::from(local))
                    })
                })
                .collect(),
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct FpvClipTracks {
    tables: HashMap<(usize, FpvMeshIndex), Arc<[u16]>>,
    sources: HashMap<usize, FpvClipSource>,
}

#[derive(Clone, Debug)]
struct FpvClipSource {
    mesh_identity: u64,
    clip: Arc<AnimClip>,
}

impl FpvClipTracks {
    pub const NONE: u16 = u16::MAX;

    pub fn table(
        &self,
        clip_index: usize,
        clip: &AnimClip,
        mesh_identity: u64,
        model: FpvMeshIndex,
    ) -> Option<&[u16]> {
        self.owns_clip(mesh_identity, clip_index, clip)
            .then_some(())?;
        self.tables
            .get(&(clip_index, model))
            .map(|tracks| &tracks[..])
    }

    pub fn owns_clip(&self, mesh_identity: u64, clip_index: usize, clip: &AnimClip) -> bool {
        self.matches_clip(mesh_identity, clip_index, Some(clip))
    }

    pub fn matches_clip(
        &self,
        mesh_identity: u64,
        clip_index: usize,
        clip: Option<&AnimClip>,
    ) -> bool {
        match self.sources.get(&clip_index) {
            Some(source) => {
                source.mesh_identity == mesh_identity
                    && clip.is_some_and(|clip| std::ptr::eq(source.clip.as_ref(), clip))
            }
            None => clip.is_none(),
        }
    }

    pub fn len(&self) -> usize {
        self.tables.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    pub fn bind(
        &mut self,
        catalog: &FpvMeshCatalog,
        clip_index: usize,
        clip: Arc<AnimClip>,
        model: FpvMeshIndex,
    ) {
        if catalog.identity() == 0 || self.tables.contains_key(&(clip_index, model)) {
            return;
        }
        let Some(pose) = catalog
            .get_at(model.order())
            .and_then(|entry| entry.skel.pose.as_ref())
        else {
            return;
        };
        let mut first_by_name: HashMap<&str, u16> = HashMap::with_capacity(pose.num_bones);
        for (index, name) in pose.bone_names.iter().take(pose.num_bones).enumerate() {
            if let Ok(index) = u16::try_from(index) {
                first_by_name.entry(name.as_str()).or_insert(index);
            }
        }
        let table: Arc<[u16]> = clip
            .tracks
            .iter()
            .map(|track| {
                first_by_name
                    .get(track.name.as_str())
                    .copied()
                    .unwrap_or(Self::NONE)
            })
            .collect();
        self.sources.entry(clip_index).or_insert(FpvClipSource {
            mesh_identity: catalog.identity(),
            clip,
        });
        self.tables.insert((clip_index, model), table);
    }
}
