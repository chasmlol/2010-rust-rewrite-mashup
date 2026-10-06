use super::*;
use crate::anim::world_weapon::{ItemComposition, PreparedItemCompositions};

pub fn kit_dobj_radius(radii: impl IntoIterator<Item = f32>) -> Option<f32> {
    radii
        .into_iter()
        .fold(None::<f32>, |acc, r| Some(acc.map_or(r, |a| a.max(r))))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KitSource<'a> {
    Body(&'a str),
    World(usize),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum PreparedKitSource {
    Body(String),
    World(usize),
}

pub struct PreparedRemoteKit {
    bodies: Arc<asset_model::BodyMeshCatalog>,
    world: Arc<asset_model::WorldWeaponCatalog>,
    sources: Vec<PreparedKitSource>,
    pub radius: Option<f32>,
    pub hide_part_bits: [u32; 6],
    pub dobj: Option<std::sync::Arc<xmodel_runtime::DObj>>,
    pub bolt_bones: [Option<u16>; 4],
    pub head_bone: Option<usize>,
}

impl PreparedRemoteKit {
    pub fn models(&self) -> Option<Vec<KitModel<'_>>> {
        self.sources
            .iter()
            .map(|source| match source {
                PreparedKitSource::Body(key) => {
                    let entry = self.bodies.get(key)?;
                    Some(KitModel {
                        name: entry.skel.name.as_str(),
                        skel: &entry.skel,
                        hide_tags: Vec::new(),
                        source: KitSource::Body(key.as_str()),
                    })
                }
                PreparedKitSource::World(index) => {
                    let entry = self.world.get_at(*index)?;
                    Some(KitModel {
                        name: entry.skel.name.as_str(),
                        skel: &entry.skel,
                        hide_tags: Vec::new(),
                        source: KitSource::World(*index),
                    })
                }
            })
            .collect()
    }
}

#[derive(bevy::prelude::Resource, Default)]
pub(crate) struct PreparedRemoteKits {
    owner: Option<(
        Arc<asset_model::BodyMeshCatalog>,
        Arc<asset_game::WeaponRegistry>,
        u64,
    )>,
    kits: HashMap<(bool, u32, u8), Option<PreparedRemoteKit>>,
    built: HashMap<KitModels, Option<Arc<xmodel_runtime::DObj>>>,
}

type KitModels = Vec<(PreparedKitSource, Option<(usize, String)>)>;

impl PreparedRemoteKits {
    pub(crate) fn clear(&mut self) {
        self.kits.clear();
        self.built.clear();
        self.owner = None;
    }

    pub(crate) fn owned_by(
        &self,
        bodies: &assets::PreparedBodies,
        weapons: &assets::PreparedWeapons,
        world_weapons: &assets::PreparedWorldWeapons,
    ) -> bool {
        self.owner.as_ref().is_some_and(|(b, w, catalog)| {
            Arc::ptr_eq(b, &bodies.0)
                && w.revision() == weapons.registry().revision()
                && *catalog == world_weapons.0.identity()
        })
    }

    pub(crate) fn reset_for(
        &mut self,
        bodies: &assets::PreparedBodies,
        weapons: &assets::PreparedWeapons,
        world_weapons: &assets::PreparedWorldWeapons,
    ) {
        if !self.owned_by(bodies, weapons, world_weapons) {
            self.clear();
            self.owner = Some((
                Arc::clone(&bodies.0),
                Arc::clone(&weapons.registry()),
                world_weapons.0.identity(),
            ));
        }
    }

    pub(crate) fn get(&self, axis: bool, weapon: u32, camo: u8) -> Option<&PreparedRemoteKit> {
        self.kits
            .get(&(axis, weapon, camo))
            .and_then(Option::as_ref)
    }

    pub(crate) fn prepare(
        &mut self,
        bodies: &assets::PreparedBodies,
        weapons: &assets::PreparedWeapons,
        world_weapons: &assets::PreparedWorldWeapons,
        compositions: &mut PreparedItemCompositions,
        axis: bool,
        handle: asset_game::WeaponHandle,
        camo: u8,
    ) {
        self.reset_for(bodies, weapons, world_weapons);
        let Ok(bound) = weapons.registry().bind(handle) else {
            return;
        };
        let weapon = bound.wire_id();
        if self.kits.contains_key(&(axis, weapon, camo)) {
            return;
        }
        let shared = if weapon == 0 || bound.world_facts().is_some_and(|facts| facts.is_shield()) {
            None
        } else {
            match compositions.prepare(weapons, world_weapons, handle, camo) {
                Ok(shared) => Some(shared),
                Err(_) => {
                    self.kits.insert((axis, weapon, camo), None);
                    return;
                }
            }
        };
        let kit = self.compile(
            bodies,
            weapons,
            world_weapons,
            axis,
            weapon,
            camo,
            shared.as_deref(),
        );
        self.kits.insert((axis, weapon, camo), kit);
    }

    fn compile(
        &mut self,
        bodies: &assets::PreparedBodies,
        weapons: &assets::PreparedWeapons,
        world_weapons: &assets::PreparedWorldWeapons,
        axis: bool,
        weapon: u32,
        camo: u8,
        shared: Option<&ItemComposition>,
    ) -> Option<PreparedRemoteKit> {
        let (models, radius) = occupy_remote_kit_dobj(
            bodies,
            Some(weapons),
            Some(world_weapons),
            axis,
            weapon,
            true,
            None,
            shared,
        )?;
        let sources = models
            .iter()
            .map(|model| match model.source {
                KitSource::Body(key) => PreparedKitSource::Body(key.to_owned()),
                KitSource::World(index) => PreparedKitSource::World(index),
            })
            .collect();
        let dobj = select_remote_models(
            bodies,
            Some(weapons),
            Some(world_weapons),
            axis,
            weapon,
            camo,
            None,
            shared,
        )
        .ok()
        .and_then(|set| {
            let key = set
                .dobj_models
                .iter()
                .map(|(model, attach)| {
                    let source = models
                        .iter()
                        .find(|candidate| {
                            candidate
                                .skel
                                .pose
                                .as_ref()
                                .is_some_and(|pose| std::ptr::eq(pose, *model))
                        })?
                        .source;
                    let source = match source {
                        KitSource::Body(key) => PreparedKitSource::Body(key.to_owned()),
                        KitSource::World(index) => PreparedKitSource::World(index),
                    };
                    Some((
                        source,
                        attach.as_ref().map(|a| (a.parent_model, a.tag.clone())),
                    ))
                })
                .collect::<Option<KitModels>>()?;
            self.built
                .entry(key)
                .or_insert_with(|| {
                    xmodel_runtime::DObj::build(&set.dobj_models)
                        .ok()
                        .map(std::sync::Arc::new)
                })
                .clone()
        });
        Some(PreparedRemoteKit {
            bodies: bodies.0.clone(),
            world: world_weapons.0.clone(),
            sources,
            radius,
            hide_part_bits: kit_hide_part_bits(&models),
            bolt_bones: [
                "tag_flash",
                "tag_brass",
                "tag_knife_fx",
                fx_iw4::FX_LASER_TAG,
            ]
            .map(|tag| {
                dobj.as_ref()
                    .and_then(|dobj| dobj.find(tag))
                    .and_then(|bone| u16::try_from(bone).ok())
            }),
            head_bone: dobj.as_ref().and_then(|dobj| dobj.find("j_head")),
            dobj,
        })
    }
}

pub fn kit_hide_part_bits(models: &[KitModel<'_>]) -> [u32; 6] {
    let mut bits = [0u32; 6];
    let mut base = 0usize;
    for model in models {
        render_scene::hide_part_bits_from_tags(&mut bits, model.skel, base, &model.hide_tags);
        base += model.skel.bones.len();
    }
    bits
}

pub struct KitModel<'a> {
    pub name: &'a str,
    pub skel: &'a asset_model::ModelSkel,
    pub hide_tags: Vec<String>,
    pub source: KitSource<'a>,
}

pub(crate) fn occupy_remote_kit_dobj<'a>(
    bodies: &'a assets::PreparedBodies,
    weapons: Option<&'a assets::PreparedWeapons>,
    world_weapons: Option<&'a assets::PreparedWorldWeapons>,
    axis: bool,
    weapon: u32,
    with_hide_tags: bool,
    shield: Option<sim::ShieldAttachment>,
    shared: Option<&'a ItemComposition>,
) -> Option<(Vec<KitModel<'a>>, Option<f32>)> {
    let kits = bodies.0.kits();
    let kit = kits.kit(axis)?;
    let body = bodies.0.get(&kit.body)?;
    if body.skel.positions.is_empty() || body.skel.bones.is_empty() {
        return None;
    }
    let _pose_src = body.skel.pose.as_ref()?;
    let mut skels: Vec<KitModel<'a>> = vec![KitModel {
        name: body.skel.name.as_str(),
        skel: &body.skel,
        hide_tags: Vec::new(),
        source: KitSource::Body(kit.body.as_str()),
    }];
    if let Some(name) = kit.head.as_deref() {
        if let Some(entry) = bodies.0.get(name) {
            if let (Some(_head_pose), Some(_tag)) = (
                entry.skel.pose.as_ref(),
                xmodel_runtime::tp_head_attach_tag(&body.skel.bone_names),
            ) {
                skels.push(KitModel {
                    name: entry.skel.name.as_str(),
                    skel: &entry.skel,
                    hide_tags: Vec::new(),
                    source: KitSource::Body(name),
                });
            }
        }
    }
    if weapon != 0
        && !weapons
            .and_then(|w| {
                w.registry()
                    .bind_published_row(weapon)
                    .ok()
                    .and_then(|weapon| weapon.world_facts())
            })
            .is_some_and(|f| f.is_shield())
    {
        let shared = shared?.for_weapon(weapons?, world_weapons?, weapon, None)?;
        let entry = shared.model_entry();
        entry.skel.pose.as_ref()?;
        xmodel_runtime::tp_weapon_attach_tag(&body.skel.bone_names)?;
        skels.push(KitModel {
            name: entry.skel.name.as_str(),
            skel: &entry.skel,
            hide_tags: if with_hide_tags {
                shared.hide_tags().to_vec()
            } else {
                Vec::new()
            },
            source: KitSource::World(shared.model.order()),
        });
        for (entry, index, _) in shared.attachment_entries() {
            skels.push(KitModel {
                name: entry.skel.name.as_str(),
                skel: &entry.skel,
                hide_tags: Vec::new(),
                source: KitSource::World(index.order()),
            });
        }
    }

    if let Some(shield) = shield {
        let registry = weapons?;
        let catalog = world_weapons?;
        let index = registry
            .registry()
            .world_model_edge_of(shield.weapon)?
            .bound_index()?;
        let entry = catalog.0.get_at(index)?;
        entry.skel.pose.as_ref()?;
        skels.push(KitModel {
            name: entry.skel.name.as_str(),
            skel: &entry.skel,
            hide_tags: Vec::new(),
            source: KitSource::World(index),
        });
    }
    let radius = kit_dobj_radius(skels.iter().filter_map(|model| model.skel.radius));
    Some((skels, radius))
}

pub struct RemoteModelSet<'a> {
    pub body_name: String,
    pub head_name: String,
    pub body: &'a asset_model::BodyMeshEntry,
    pub head: Option<&'a asset_model::BodyMeshEntry>,
    pub gun: Option<&'a asset_model::WorldWeaponEntry>,

    pub world_gun_gap: Option<WorldGunGap>,
    pub dobj_models: Vec<(
        &'a xmodel_runtime::ModelPoseSrc,
        Option<xmodel_runtime::Attach>,
    )>,
    pub gun_model_index: usize,
    pub attachments: Vec<(&'a asset_model::WorldWeaponEntry, usize)>,
    pub shield: Option<sim::ShieldAttachment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldGunGap {
    pub weapon: u32,

    pub world_model: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteModelLods {
    pub body: u8,
    pub head: Option<u8>,
    pub gun: Option<u8>,
    pub attachments: Vec<Option<u8>>,
}

impl RemoteModelLods {
    pub const fn tuple(&self) -> (Option<u8>, Option<u8>, Option<u8>) {
        (Some(self.body), self.head, self.gun)
    }
}

pub fn select_remote_lods(
    models: &RemoteModelSet<'_>,
    slot_lods: &[i8],
    mut camera_lod: impl FnMut(&asset_model::ModelSkel) -> Option<u8>,
) -> Option<RemoteModelLods> {
    let mut slot_lod = |model: usize, skel: &asset_model::ModelSkel| -> Option<u8> {
        resolve_slot_lod(slot_lods, model, || camera_lod(skel))
    };
    let body = slot_lod(0, &models.body.skel)?;
    let head = models.head.and_then(|head| slot_lod(1, &head.skel));
    let gun = models
        .gun
        .and_then(|gun| slot_lod(usize::from(models.head.is_some()) + 1, &gun.skel));
    let attachments = models
        .attachments
        .iter()
        .map(|(entry, model)| slot_lod(*model, &entry.skel))
        .collect();
    Some(RemoteModelLods {
        body,
        head,
        gun,
        attachments,
    })
}

pub fn remote_dobj_reuse_key(
    e_type: i32,
    body_name: &str,
    head_name: &str,
    gun_name: &str,
    attachment_names: &[&str],
) -> xmodel_runtime::DObjReuseKey {
    let mut parts = vec![body_name, head_name, gun_name];
    parts.extend_from_slice(attachment_names);
    xmodel_runtime::DObjReuseKey {
        e_type,
        model: xmodel_runtime::model_token(&parts),
    }
}

pub fn remote_dobj_reuses(
    has_dobj: bool,
    cached: Option<xmodel_runtime::DObjReuseKey>,
    next: xmodel_runtime::DObjReuseKey,
) -> bool {
    has_dobj && cached.is_some_and(|cached| xmodel_runtime::reuse_matches(cached, next))
}

pub(crate) fn select_remote_models<'a>(
    bodies: &'a assets::PreparedBodies,
    weapons: Option<&assets::PreparedWeapons>,
    world_weapons: Option<&'a assets::PreparedWorldWeapons>,
    axis: bool,
    weapon: u32,
    camo: u8,
    shield: Option<sim::ShieldAttachment>,
    shared: Option<&'a ItemComposition>,
) -> Result<RemoteModelSet<'a>, String> {
    let kits = bodies.0.kits();
    let kit = kits
        .kit(axis)
        .ok_or_else(|| "no third-person kit".to_string())?;
    let body = bodies
        .0
        .get(&kit.body)
        .ok_or_else(|| format!("body `{}` missing from catalog", kit.body))?;
    let pose_src = body
        .skel
        .pose
        .as_ref()
        .ok_or_else(|| format!("body `{}` has no ModelPoseSrc", kit.body))?;

    let mut dobj_models = vec![(pose_src, None)];
    let head = match kit.head.as_deref() {
        None => None,
        Some(name) => {
            let entry = bodies
                .0
                .get(name)
                .ok_or_else(|| format!("head `{name}` missing from catalog"))?;
            let head_pose = entry
                .skel
                .pose
                .as_ref()
                .ok_or_else(|| format!("head `{name}` has no ModelPoseSrc"))?;
            let tag =
                xmodel_runtime::tp_head_attach_tag(&body.skel.bone_names).ok_or_else(|| {
                    format!(
                        "body `{}` has no {}; refusing a headless DObj for `{name}`",
                        kit.body,
                        xmodel_runtime::TP_HEAD_ATTACH_TAG
                    )
                })?;
            dobj_models.push((
                head_pose,
                Some(xmodel_runtime::Attach {
                    parent_model: 0,
                    tag: tag.into(),
                }),
            ));
            Some(entry)
        }
    };
    let gun_model_index = dobj_models.len();

    let world_gun_gap = None;
    let mut attachments = Vec::new();
    let held = if weapons
        .and_then(|w| {
            w.registry()
                .bind_published_row(weapon)
                .ok()
                .and_then(|weapon| weapon.world_facts())
        })
        .is_some_and(|f| f.is_shield())
    {
        0
    } else {
        weapon
    };
    let gun = match held {
        0 => None,
        index => {
            let shared = shared
                .and_then(|shared| shared.for_weapon(weapons?, world_weapons?, index, Some(camo)))
                .ok_or_else(|| format!("weapon {index} has no admitted world composition"))?;
            let entry = shared.model_entry();
            let tag =
                xmodel_runtime::tp_weapon_attach_tag(&body.skel.bone_names).ok_or_else(|| {
                    format!("body `{}` has no world weapon attachment socket", kit.body)
                })?;
            for (part_index, (pose, attach)) in shared.models().into_iter().enumerate() {
                let attach = if part_index == 0 {
                    Some(xmodel_runtime::Attach {
                        parent_model: 0,
                        tag: tag.into(),
                    })
                } else {
                    attach.map(|attach| xmodel_runtime::Attach {
                        parent_model: gun_model_index + attach.parent_model,
                        tag: attach.tag,
                    })
                };
                dobj_models.push((pose, attach));
            }
            attachments.extend(
                shared
                    .attachment_entries()
                    .enumerate()
                    .map(|(i, (entry, _, _))| (entry, gun_model_index + i + 1)),
            );
            Some(entry)
        }
    };
    if let Some(shield) = shield {
        let registry = weapons.ok_or("shield weapon registry missing")?;
        let catalog = world_weapons.ok_or("shield world model catalog missing")?;
        let entry = registry
            .registry()
            .world_model_entry(shield.weapon, &catalog.0)
            .ok_or("shield world model missing")?;
        let pose = entry.skel.pose.as_ref().ok_or("shield pose missing")?;
        attachments.push((entry, dobj_models.len()));
        dobj_models.push((
            pose,
            Some(xmodel_runtime::Attach {
                parent_model: 0,
                tag: shield.tag().into(),
            }),
        ));
    }
    Ok(RemoteModelSet {
        body_name: kit.body.clone(),
        head_name: kit.head.clone().unwrap_or_default(),
        body,
        head,
        gun,
        world_gun_gap,
        dobj_models,
        gun_model_index,
        attachments,
        shield,
    })
}

pub fn ensure_remote_dobj(
    models: &RemoteModelSet<'_>,
    e_type: i32,
    persist_key: u32,
    trees: &mut RemoteBodyTrees,
    prepared: Option<&std::sync::Arc<xmodel_runtime::DObj>>,
) -> Result<(), String> {
    let gun_name = models
        .gun
        .map(|entry| entry.skel.name.as_str())
        .unwrap_or("");
    let mut attachment_names: Vec<&str> = models
        .attachments
        .iter()
        .map(|(entry, _)| entry.skel.name.as_str())
        .collect();
    if let Some(shield) = models.shield {
        attachment_names.push(shield.tag());
    }
    let reuse_key = remote_dobj_reuse_key(
        e_type,
        models.body_name.as_str(),
        models.head_name.as_str(),
        gun_name,
        &attachment_names,
    );
    let slot = trees.get_mut(persist_key).expect("tree slot inserted");
    let same_prepared = models.shield.is_some()
        || slot
            .dobj
            .as_ref()
            .zip(prepared)
            .is_some_and(|(current, next)| Arc::ptr_eq(current, next));
    if !same_prepared || !remote_dobj_reuses(slot.dobj.is_some(), slot.reuse_key, reuse_key) {
        slot.dobj = Some(if models.shield.is_some() {
            std::sync::Arc::new(
                xmodel_runtime::DObj::build(&models.dobj_models).map_err(|e| format!("{e:?}"))?,
            )
        } else {
            std::sync::Arc::clone(prepared.ok_or_else(|| {
                format!(
                    "remote representation not prepared: body={} head={} weapon={gun_name}",
                    models.body_name, models.head_name
                )
            })?)
        });
        slot.reuse_key = Some(reuse_key);
    }
    Ok(())
}
