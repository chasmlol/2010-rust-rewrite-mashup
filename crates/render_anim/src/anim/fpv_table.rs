use crate::anim::fpv_rig::PreparedFpvRig;
use crate::gaps::RenderGapCause;
use asset_anim::XAnimCatalog;
use asset_core::AssetNamespace;
use asset_game::{FpvSideAssemblies, WeaponAnimations, WeaponFpvFacts, WeaponRegistry};
use asset_model::{FpvHands, FpvMeshCatalog};
use assets::FpvMeshIndex;
use bevy::prelude::*;
use render_material::RuntimeMaterialCatalog;
use render_scene::SmodelPassMaterial;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct FpvRigSet {
    pub(super) bare: [Option<Arc<PreparedFpvRig>>; 2],
    pub(super) rocket: [Option<Arc<PreparedFpvRig>>; 2],
    pub(super) melee: [Option<Arc<PreparedFpvRig>>; 2],
    pub(super) ads: [Option<Arc<PreparedFpvRig>>; 2],
    pub(super) jammed: [Option<Arc<PreparedFpvRig>>; 2],
}

impl FpvRigSet {
    pub fn pick(
        &self,
        rocket: bool,
        dual: bool,
        melee: bool,
        ads: bool,
        jammed: bool,
    ) -> Option<&Arc<PreparedFpvRig>> {
        let hand = usize::from(dual);
        if melee && let Some(rig) = &self.melee[hand] {
            return Some(rig);
        }
        if ads && let Some(rig) = &self.ads[hand] {
            return Some(rig);
        }
        match (rocket, &self.rocket[hand]) {
            (true, Some(rig)) => Some(rig),
            _ if jammed && let Some(rig) = &self.jammed[hand] => Some(rig),
            _ => self.bare[hand].as_ref(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FpvViewCensus {
    pub gun_colormap_skip_n: u32,
    pub gun_colormap_skip_names: Vec<String>,
    pub mat_hints: Vec<String>,
}

pub struct FpvWeaponView {
    pub gun_name: String,
    pub gun_index: FpvMeshIndex,
    pub hands: FpvHands,
    pub hands_index: FpvMeshIndex,
    pub namespace: AssetNamespace,
    pub assemblies: FpvSideAssemblies,
    pub right: WeaponAnimations,
    pub left: Option<WeaponAnimations>,
    pub idle_name: Option<String>,
    pub rigs: FpvRigSet,
    pub census: FpvViewCensus,
    pub camos: Vec<(u8, Arc<HashMap<usize, SmodelPassMaterial>>)>,
}

impl FpvWeaponView {
    pub fn camo(&self, slot: u8) -> Option<&Arc<HashMap<usize, SmodelPassMaterial>>> {
        self.camos
            .iter()
            .find(|(own, _)| *own == slot)
            .map(|(_, swaps)| swaps)
    }
}

pub enum FpvWeaponSlot {
    Absent,
    Ready(Arc<FpvWeaponView>),
    Refused(RenderGapCause),
}

static ABSENT_SLOT: FpvWeaponSlot = FpvWeaponSlot::Absent;

#[derive(Clone, Copy, Debug, Default)]
pub struct FpvPreparationCensus {
    pub models: usize,
    pub layouts: usize,
    pub compositions: usize,
    pub rigs: usize,
    pub materials: usize,
    pub elapsed_ms: f64,
}

pub struct FpvWeaponTable {
    pub(super) owner: FpvPreparationOwner,
    pub(super) facts: Vec<Option<WeaponFpvFacts>>,
    pub(super) hud_iris: Vec<bool>,
    pub(super) melee: Vec<u32>,
    pub(super) guns: Vec<Option<FpvMeshIndex>>,
    pub(super) slots: Vec<[FpvWeaponSlot; 2]>,
    pub(super) alternate_slots: HashMap<(u32, u32), [FpvWeaponSlot; 2]>,
    pub(super) census: FpvPreparationCensus,
}

impl FpvWeaponTable {
    pub fn census(&self) -> FpvPreparationCensus {
        self.census
    }

    pub fn material_catalog(&self) -> &Arc<RuntimeMaterialCatalog> {
        &self.owner.materials
    }

    pub fn catalog_id(&self) -> u64 {
        self.owner.meshes.identity()
    }

    pub fn facts_of(&self, weapon: u32) -> Option<WeaponFpvFacts> {
        self.facts.get(weapon as usize).copied().flatten()
    }

    pub fn overlay_is_hud_iris(&self, weapon: u32) -> bool {
        self.hud_iris.get(weapon as usize).copied().unwrap_or(false)
    }

    pub fn melee_weapon_of(&self, weapon: u32) -> u32 {
        self.melee.get(weapon as usize).copied().unwrap_or(weapon)
    }

    pub fn gun_index(&self, weapon: u32) -> Option<FpvMeshIndex> {
        self.guns.get(weapon as usize).copied().flatten()
    }

    pub fn motion_tracker(&self, weapon: u32, parent: u32) -> bool {
        self.facts_of(weapon).is_some_and(|facts| {
            facts.motion_tracker
                || (facts.is_alternate()
                    && self
                        .facts_of(parent)
                        .is_some_and(|parent| parent.motion_tracker))
        })
    }

    pub fn bind<'a>(
        &'a self,
        weapons: &WeaponRegistry,
        meshes: &Arc<FpvMeshCatalog>,
        clips: &Arc<XAnimCatalog>,
        materials: &Arc<RuntimeMaterialCatalog>,
        images: &Arc<Vec<Option<Handle<Image>>>>,
        atlas: Option<bevy::asset::AssetId<Image>>,
    ) -> Result<BoundFpvTable<'a>, FpvBindingRefusal> {
        if self.owner.weapons.revision() != weapons.revision()
            || !Arc::ptr_eq(&self.owner.meshes, meshes)
            || !Arc::ptr_eq(&self.owner.clips, clips)
            || !Arc::ptr_eq(&self.owner.materials, materials)
            || !Arc::ptr_eq(&self.owner.images, images)
            || self.owner.atlas != atlas
        {
            return Err(FpvBindingRefusal::ForeignOwner);
        }
        Ok(BoundFpvTable(self))
    }

    fn slot_unchecked(&self, weapon: u32, parent: u32, axis: bool) -> &FpvWeaponSlot {
        if parent != 0 && self.facts_of(weapon).is_some_and(|f| f.is_alternate()) {
            return self
                .alternate_slots
                .get(&(weapon, parent))
                .map_or(&ABSENT_SLOT, |s| &s[usize::from(axis)]);
        }
        self.slots
            .get(weapon as usize)
            .map_or(&ABSENT_SLOT, |sides| &sides[usize::from(axis)])
    }
}

#[derive(Clone)]
pub(super) struct FpvPreparationOwner {
    pub(super) materials: Arc<RuntimeMaterialCatalog>,
    pub(super) images: Arc<Vec<Option<Handle<Image>>>>,
    pub(super) weapons: Arc<WeaponRegistry>,
    pub(super) meshes: Arc<FpvMeshCatalog>,
    pub(super) clips: Arc<XAnimCatalog>,
    pub(super) atlas: Option<bevy::asset::AssetId<Image>>,
}

impl FpvPreparationOwner {
    pub(super) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.materials, &other.materials)
            && Arc::ptr_eq(&self.images, &other.images)
            && Arc::ptr_eq(&self.weapons, &other.weapons)
            && Arc::ptr_eq(&self.meshes, &other.meshes)
            && Arc::ptr_eq(&self.clips, &other.clips)
            && self.atlas == other.atlas
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FpvBindingRefusal {
    ForeignOwner,
    InvalidAlternateParent,
}

pub struct BoundFpvTable<'a>(&'a FpvWeaponTable);

impl<'a> BoundFpvTable<'a> {
    pub fn slot(
        &self,
        weapon: asset_game::WeaponHandle,
        parent: Option<asset_game::WeaponHandle>,
        axis: bool,
    ) -> Result<&'a FpvWeaponSlot, FpvBindingRefusal> {
        let registry = &self.0.owner.weapons;
        let weapon = registry
            .bind(weapon)
            .map_err(|_| FpvBindingRefusal::ForeignOwner)?;
        let parent = parent
            .map(|p| registry.bind(p))
            .transpose()
            .map_err(|_| FpvBindingRefusal::ForeignOwner)?;
        let id = weapon.wire_id();
        let parent_id = parent.map_or(0, |p| p.wire_id());
        if weapon.fpv_facts().is_some_and(|f| f.is_alternate())
            && parent_id != 0
            && !registry
                .alternate_fpv_pairs()
                .any(|pair| pair == (id, parent_id))
        {
            return Err(FpvBindingRefusal::InvalidAlternateParent);
        }
        Ok(self.0.slot_unchecked(id, parent_id, axis))
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct FpvOwnerInputs<'w> {
    pub(crate) weapons: Option<Res<'w, assets::PreparedWeapons>>,
    pub(crate) meshes: Option<Res<'w, assets::PreparedFpvMeshes>>,
    pub(crate) clips: Option<Res<'w, assets::PreparedXAnims>>,
    tess: Option<Res<'w, render_scene::TessMaterials>>,
    lighting: Option<Res<'w, render_scene::WorldModelLightingAtlas>>,
}

impl FpvOwnerInputs<'_> {
    pub(crate) fn bind<'a>(&self, table: &'a FpvWeaponTable) -> Option<BoundFpvTable<'a>> {
        let tess = self.tess.as_ref()?;
        table
            .bind(
                self.weapons.as_ref()?.registry(),
                &self.meshes.as_ref()?.0,
                &self.clips.as_ref()?.0,
                tess.catalog(),
                &tess.material_images,
                self.lighting.as_ref().map(|atlas| atlas.image.id()),
            )
            .ok()
    }
    pub(crate) fn handles(
        &self,
        epoch: Option<u32>,
        weapon: u32,
        parent: u32,
    ) -> Option<(asset_game::WeaponHandle, Option<asset_game::WeaponHandle>)> {
        let bound = self.weapons.as_ref()?.for_snapshot(epoch).ok()?;
        let weapon = bound.row(weapon)?.handle();
        let parent = if parent == 0 {
            None
        } else {
            Some(bound.row(parent)?.handle())
        };
        Some((weapon, parent))
    }
}
