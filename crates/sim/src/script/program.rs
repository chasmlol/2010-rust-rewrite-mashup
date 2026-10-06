use bevy_ecs::prelude::Resource;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::script::error::Fault;
use crate::script::ir::{Function, IR_VERSION};
use crate::script::profile::catalog::{Builtin, Catalog};
use crate::script::source::{SourceOrigin, SourceResolver};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Site {
    Server,
    Client,
}

pub use asset_core::FamilyId as Realm;
#[derive(Clone, Debug)]
pub struct ModuleIdentity {
    pub site: Site,
    pub realm: Realm,
    pub module: String,
    pub sha256: [u8; 32],
    pub origin: SourceOrigin,
}

#[derive(Resource, Clone, Debug)]
pub struct Program {
    pub(crate) functions: Vec<Function>,
    pub(crate) names: BTreeMap<String, usize>,
    pub(crate) modules: Vec<ModuleIdentity>,
    pub(crate) symbols: Vec<Arc<str>>,
    pub(crate) symbol_ids: BTreeMap<Arc<str>, u32>,
    pub(crate) natives: Vec<Builtin>,
    pub(crate) rules: Realm,
    pub(crate) impure_scripts: bool,
}
impl Program {
    pub fn load(
        resolver: &impl SourceResolver,
        roots: &[&str],
        catalog: &Catalog,
    ) -> Result<Self, Fault> {
        super::compiler::compile(resolver, roots, catalog)
    }
    pub fn modules(&self) -> &[ModuleIdentity] {
        &self.modules
    }
    pub fn has_impure_scripts(&self) -> bool {
        self.impure_scripts
    }
    pub fn rules(&self) -> Realm {
        self.rules
    }
    pub fn function_count(&self) -> usize {
        self.functions.len()
    }
    pub fn native_count(&self) -> usize {
        self.natives.len()
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        digest.update(IR_VERSION.to_le_bytes());
        for module in &self.modules {
            digest.update([module.site as u8, module.realm as u8]);
            digest.update([module.origin as u8]);
            digest.update((module.module.len() as u64).to_le_bytes());
            digest.update(module.module.as_bytes());
            digest.update(module.sha256);
        }
        digest.finalize().into()
    }
}
