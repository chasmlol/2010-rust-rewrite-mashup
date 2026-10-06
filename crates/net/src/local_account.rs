use bevy::prelude::Resource;
use sim::{AccountId, AccountSnapshot, ClientId, PersistentDataError, PersistentDataStore};

#[derive(Resource, Clone, Debug)]
pub struct LocalAccount {
    pub id: AccountId,
    pub key: crate::AccountKey,
    pub snapshot: Option<AccountSnapshot>,
}

impl LocalAccount {
    pub fn bind_or_initialize(
        &self,
        store: &mut PersistentDataStore,
        client: ClientId,
        defaults: Option<&sim::PlayerDataDefaults>,
    ) -> Result<(), PersistentDataError> {
        if self.key.account() != self.id {
            return Err(PersistentDataError::InvalidIdentity);
        }
        store.admit(client, self.id, self.snapshot.as_ref(), defaults)
    }

    pub fn bind(
        &self,
        store: &mut PersistentDataStore,
        client: ClientId,
    ) -> Result<bool, PersistentDataError> {
        if self.key.account() != self.id {
            return Err(PersistentDataError::InvalidIdentity);
        }
        store.bind_snapshot(client, self.id, self.snapshot.as_ref())
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct AccountSaveReceipt(pub Option<AccountSnapshot>);
