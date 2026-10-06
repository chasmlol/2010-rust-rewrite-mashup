use std::sync::Arc;

use bevy::prelude::*;
use render_material::{PreparedMaterialTable, RuntimeMaterialCatalog};

#[derive(Resource, Clone, Debug)]
pub struct TessMaterials {
    catalog: Arc<RuntimeMaterialCatalog>,
    prepared: Arc<PreparedMaterialTable>,

    pub material_images: Arc<Vec<Option<Handle<Image>>>>,
}

impl TessMaterials {
    pub fn new(
        catalog: Arc<RuntimeMaterialCatalog>,
        prepared: Arc<PreparedMaterialTable>,
    ) -> Result<Self, render_material::MaterialRefusal> {
        if catalog.generation_id() != prepared.generation_id() {
            return Err(render_material::MaterialRefusal::StaleMaterialGeneration {
                retained: prepared.generation_id(),
                current: catalog.generation_id(),
            });
        }
        Ok(Self {
            catalog,
            prepared,
            material_images: Arc::new(Vec::new()),
        })
    }
    pub fn catalog(&self) -> &Arc<RuntimeMaterialCatalog> {
        &self.catalog
    }
    pub fn prepared(&self) -> &Arc<PreparedMaterialTable> {
        &self.prepared
    }
}

impl Default for TessMaterials {
    fn default() -> Self {
        let catalog = Arc::new(RuntimeMaterialCatalog::default());
        let prepared = Arc::new(PreparedMaterialTable::from_catalog(&catalog, |_, _| None));
        Self::new(catalog, prepared).expect("empty material publication")
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct WorldPresentFacts {
    pub spawned: bool,
}
