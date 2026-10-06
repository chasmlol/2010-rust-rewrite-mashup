use super::WeaponRegistry;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WeaponHandle {
    revision: u64,
    row: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponBindingRefusal {
    ForeignOwner,
    UnknownWeapon,
    StaleSnapshot,
}

#[derive(Clone, Copy)]
pub struct BoundWeapon<'a> {
    registry: &'a WeaponRegistry,
    handle: WeaponHandle,
}

impl<'a> BoundWeapon<'a> {
    pub fn preparation(self) -> &'a super::WeaponPreparationRecipe {
        &self.registry.rows[self.handle.row as usize].preparation
    }

    pub fn handle(self) -> WeaponHandle {
        self.handle
    }

    pub fn wire_id(self) -> u32 {
        self.handle.row
    }

    pub fn fpv_facts(self) -> Option<super::WeaponFpvFacts> {
        self.registry.fpv_facts_of(self.handle.row)
    }

    pub fn hud_facts(self) -> Option<super::WeaponHudFacts> {
        self.registry.hud_facts_of(self.handle.row)
    }

    pub fn event_facts(self) -> Option<super::WeaponEventFacts> {
        self.registry.event_facts_of(self.handle.row)
    }

    pub fn world_facts(self) -> Option<super::WeaponWorldFacts> {
        self.registry.world_facts_of(self.handle.row)
    }

    pub fn combat_facts(
        self,
        rules: weapon_iw4::WeaponHostRules,
        global_location: Option<[f32; weapon_iw4::HITLOC_COUNT]>,
    ) -> Result<weapon_iw4::WeaponCombatFacts, super::WeaponCombatRefusal> {
        self.registry
            .combat_facts_of(self.handle.row, rules, global_location)
    }
}

impl Default for WeaponRegistry {
    fn default() -> Self {
        crate::WeaponCatalog::default().into_build().registry
    }
}

impl WeaponRegistry {
    pub fn bind(&self, handle: WeaponHandle) -> Result<BoundWeapon<'_>, WeaponBindingRefusal> {
        if handle.revision != self.revision {
            return Err(WeaponBindingRefusal::ForeignOwner);
        }
        if self.rows.get(handle.row as usize).is_none() {
            return Err(WeaponBindingRefusal::UnknownWeapon);
        }
        Ok(BoundWeapon {
            registry: self,
            handle,
        })
    }

    pub fn bind_published_row(&self, row: u32) -> Result<BoundWeapon<'_>, WeaponBindingRefusal> {
        self.bind(WeaponHandle {
            revision: self.revision,
            row,
        })
    }

    pub fn published_weapons(&self) -> impl Iterator<Item = BoundWeapon<'_>> {
        (0..self.rows.len() as u32).map(|row| BoundWeapon {
            registry: self,
            handle: WeaponHandle {
                revision: self.revision,
                row,
            },
        })
    }
}
