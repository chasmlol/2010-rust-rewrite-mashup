use asset_game::{WeaponHandle, WeaponRegistry};
use asset_model::{WorldWeaponCatalog, WorldWeaponEntry};
use assets::{PreparedWeapons, PreparedWorldWeapons, WorldWeaponIndex};
use bevy::prelude::Resource;
use std::collections::HashMap;
use std::sync::Arc;
use xmodel_runtime::{Attach, DObj, ModelPoseSrc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorldCompositionRefusal {
    ForeignOwner,
    Configuration(asset_game::ConfigurationRefusal),
    RequiredModel,
    RequiredPose,
    Topology(String),
}

pub(crate) struct WorldAttachment {
    pub(crate) index: WorldWeaponIndex,
    pub(crate) tag: String,
}

pub(crate) struct ItemComposition {
    pub(crate) name: String,
    pub(crate) model: WorldWeaponIndex,
    pub(crate) attachments: Vec<WorldWeaponIndex>,
    pub(crate) key: String,
    pub(crate) dobj: DObj,
    registry: Arc<WeaponRegistry>,
    world: Arc<WorldWeaponCatalog>,
    weapon: WeaponHandle,
    camo: u8,
    topology: Vec<WorldAttachment>,
    hide_tags: Vec<String>,
}

impl ItemComposition {
    pub(crate) fn for_weapon(
        &self,
        weapons: &PreparedWeapons,
        world: &PreparedWorldWeapons,
        wire: u32,
        camo: Option<u8>,
    ) -> Option<&Self> {
        (self.owned_by(weapons, world)
            && self
                .registry
                .bind(self.weapon)
                .is_ok_and(|bound| bound.wire_id() == wire)
            && camo.is_none_or(|camo| camo == self.camo))
        .then_some(self)
    }
    pub(crate) fn model_entry(&self) -> &WorldWeaponEntry {
        self.world.get_at(self.model.order()).unwrap()
    }
    pub(crate) fn attachment_entries(
        &self,
    ) -> impl Iterator<Item = (&WorldWeaponEntry, WorldWeaponIndex, &str)> {
        self.topology.iter().map(|a| {
            (
                self.world.get_at(a.index.order()).unwrap(),
                a.index,
                a.tag.as_str(),
            )
        })
    }
    pub(crate) fn hide_tags(&self) -> &[String] {
        &self.hide_tags
    }
    pub(crate) fn models(&self) -> Vec<(&ModelPoseSrc, Option<Attach>)> {
        let mut models = vec![(self.model_entry().skel.pose.as_ref().unwrap(), None)];
        models.extend(self.attachment_entries().map(|(entry, _, tag)| {
            (
                entry.skel.pose.as_ref().unwrap(),
                Some(Attach {
                    parent_model: 0,
                    tag: tag.into(),
                }),
            )
        }));
        models
    }
    pub(crate) fn owned_by(&self, weapons: &PreparedWeapons, world: &PreparedWorldWeapons) -> bool {
        self.registry.revision() == weapons.registry().revision()
            && self.world.identity() == world.0.identity()
            && weapons.registry().bind(self.weapon).is_ok()
    }
}

#[derive(Resource, Default)]
pub(crate) struct PreparedItemCompositions {
    owner: Option<(Arc<WeaponRegistry>, Arc<WorldWeaponCatalog>)>,
    by_weapon: HashMap<(WeaponHandle, u8), Result<Arc<ItemComposition>, WorldCompositionRefusal>>,
}

impl PreparedItemCompositions {
    pub(crate) fn clear(&mut self) {
        self.owner = None;
        self.by_weapon.clear();
    }
    pub(crate) fn reset_for(&mut self, weapons: &PreparedWeapons, world: &PreparedWorldWeapons) {
        if !self.owner.as_ref().is_some_and(|(w, c)| {
            w.revision() == weapons.registry().revision() && c.identity() == world.0.identity()
        }) {
            self.by_weapon.clear();
            self.owner = Some((weapons.registry().clone(), world.0.clone()));
        }
    }
    pub(crate) fn prepare(
        &mut self,
        weapons: &PreparedWeapons,
        world: &PreparedWorldWeapons,
        weapon: WeaponHandle,
        camo: u8,
    ) -> Result<Arc<ItemComposition>, WorldCompositionRefusal> {
        self.reset_for(weapons, world);
        let (registry, catalog) = self.owner.as_ref().unwrap();
        let bound = registry
            .bind(weapon)
            .map_err(|_| WorldCompositionRefusal::ForeignOwner)?;
        let id = bound.wire_id();
        self.by_weapon
            .entry((weapon, camo))
            .or_insert_with(|| {
                let result = compose_item(registry, catalog, weapon, camo).map(Arc::new);
                if let Err(refusal) = &result {
                    diag::warn!(World, "world weapon {id}: composition refused: {refusal:?}");
                }
                result
            })
            .clone()
    }
    pub(crate) fn get(
        &self,
        weapons: &PreparedWeapons,
        world: &PreparedWorldWeapons,
        weapon: u32,
        camo: u8,
    ) -> Option<&ItemComposition> {
        let handle = weapons.registry().bind_published_row(weapon).ok()?.handle();
        let composition = self.by_weapon.get(&(handle, camo))?.as_ref().ok()?;
        composition
            .owned_by(weapons, world)
            .then_some(composition.as_ref())
    }
}

fn compose_item(
    registry: &Arc<WeaponRegistry>,
    catalog: &Arc<WorldWeaponCatalog>,
    weapon: WeaponHandle,
    camo: u8,
) -> Result<ItemComposition, WorldCompositionRefusal> {
    let id = registry
        .bind(weapon)
        .map_err(|_| WorldCompositionRefusal::ForeignOwner)?
        .wire_id();
    if registry.world_catalog_identity() != catalog.identity() {
        return Err(WorldCompositionRefusal::ForeignOwner);
    }
    registry
        .configuration_admission(id)
        .map_err(WorldCompositionRefusal::Configuration)?;
    let entry = registry
        .world_model_entry_for(id, camo, catalog)
        .ok_or(WorldCompositionRefusal::RequiredModel)?;
    let order = if camo != 0 {
        registry
            .camo_world_edge_of(id, camo)
            .and_then(|e| e.bound_index())
            .filter(|&i| catalog.get_at(i).is_some())
            .or_else(|| registry.world_model_edge_of(id)?.bound_index())
    } else {
        registry
            .world_model_edge_of(id)
            .and_then(|e| e.bound_index())
    }
    .ok_or(WorldCompositionRefusal::RequiredModel)?;
    let pose = entry
        .skel
        .pose
        .as_ref()
        .ok_or(WorldCompositionRefusal::RequiredPose)?;
    let mut key = entry.skel.name.clone();
    let mut topology = Vec::new();
    let mut models = vec![(pose, None)];
    for (edge, tag) in registry
        .attachment_world_model_edges_of(id)
        .iter()
        .zip(registry.attachment_world_mounts_of(id))
    {
        let Some((index, tag)) = edge.bound_index().zip(tag.as_deref()) else {
            continue;
        };
        let Some(attachment) = catalog.get_at(index) else {
            continue;
        };
        let Some(pose) = attachment.skel.pose.as_ref() else {
            continue;
        };
        key.push('+');
        key.push_str(&attachment.skel.name);
        topology.push(WorldAttachment {
            index: WorldWeaponIndex::from_order(index),
            tag: tag.into(),
        });
        models.push((
            pose,
            Some(Attach {
                parent_model: 0,
                tag: tag.into(),
            }),
        ));
    }
    let dobj =
        DObj::build(&models).map_err(|e| WorldCompositionRefusal::Topology(e.to_string()))?;
    Ok(ItemComposition {
        name: entry.skel.name.clone(),
        model: WorldWeaponIndex::from_order(order),
        attachments: topology.iter().map(|a| a.index).collect(),
        key,
        dobj,
        registry: registry.clone(),
        world: catalog.clone(),
        weapon,
        camo,
        topology,
        hide_tags: asset_game::effective_hide_tags(registry, id),
    })
}
