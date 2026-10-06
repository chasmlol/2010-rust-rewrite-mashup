use std::{collections::HashMap, sync::Arc};

use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimSceneSubmit;

#[derive(Clone, Debug)]
pub struct AnimDObjSceneModel {
    pub lod: i8,
    pub bone_count: u8,
    pub skel: Arc<asset_model::ModelSkel>,
}

#[derive(Message, Clone, Debug)]
pub struct AnimDObjSceneSubmission {
    pub render_fx_flags: u32,
    pub has_tree: bool,
    pub origin: [f32; 3],
    pub lighting_origin: [f32; 3],
    pub radius: Option<f32>,
    pub entnum: u32,
    pub quat: Option<[f32; 4]>,
    pub occupy_model_n: u8,
    pub models: Vec<AnimDObjSceneModel>,
    pub hide_part_bits: [u32; 6],
    pub store_skin: bool,
}

#[derive(Resource, Default)]
pub struct AnimDObjSceneSkels {
    by_name: HashMap<String, Arc<asset_model::ModelSkel>>,
    seeded_for: Option<(usize, u64)>,
}

impl AnimDObjSceneSkels {
    pub fn seed(&mut self, bodies: &assets::PreparedBodies, world: &assets::PreparedWorldWeapons) {
        let owner = (Arc::as_ptr(&bodies.0) as usize, world.0.identity());
        if self.seeded_for == Some(owner) {
            return;
        }
        let kits = bodies.0.kits();
        for kit in [kits.kit(false), kits.kit(true)].into_iter().flatten() {
            for name in std::iter::once(&kit.body).chain(kit.head.as_ref()) {
                let Some(entry) = bodies.0.get(name) else {
                    continue;
                };
                self.by_name
                    .entry(entry.skel.name.clone())
                    .or_insert_with(|| Arc::new(entry.skel.clone()));
            }
        }
        for index in 0..world.0.len() {
            if let Some(entry) = world.0.get_at(index) {
                self.by_name
                    .entry(entry.skel.name.clone())
                    .or_insert_with(|| Arc::clone(&entry.skel));
            }
        }
        self.seeded_for = Some(owner);
    }

    pub fn shared(
        &mut self,
        name: &str,
        skel: &Arc<asset_model::ModelSkel>,
        lod: i8,
    ) -> AnimDObjSceneModel {
        let skel = Arc::clone(
            self.by_name
                .entry(name.to_owned())
                .or_insert_with(|| Arc::clone(skel)),
        );
        AnimDObjSceneModel {
            lod,
            bone_count: u8::try_from(skel.bones.len()).unwrap_or(u8::MAX),
            skel,
        }
    }

    pub fn model(
        &mut self,
        name: &str,
        skel: &asset_model::ModelSkel,
        lod: i8,
    ) -> AnimDObjSceneModel {
        let skel = match self.by_name.get(name) {
            Some(skel) => Arc::clone(skel),
            None => {
                let skel = Arc::new(skel.clone());
                self.by_name.insert(name.to_owned(), Arc::clone(&skel));
                skel
            }
        };
        AnimDObjSceneModel {
            lod,
            bone_count: u8::try_from(skel.bones.len()).unwrap_or(u8::MAX),
            skel,
        }
    }
}
