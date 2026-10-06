use std::{fmt, sync::Arc};

use master_protocol::MemberId;
use ring::{
    digest::{self, SHA256},
    rand::{SecureRandom, SystemRandom},
    signature::{self, Ed25519KeyPair, KeyPair},
};
use sim::AccountId;

use crate::ConnectionId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountIdentityError {
    Entropy,
    InvalidKey,
    InvalidContext,
    InvalidProof,
}

struct KeyMaterial {
    seed: [u8; 32],
    pair: Ed25519KeyPair,
}

#[derive(Clone)]
pub struct AccountKey(Arc<KeyMaterial>);

impl fmt::Debug for AccountKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccountKey")
            .field("account", &self.account())
            .finish_non_exhaustive()
    }
}

impl AccountKey {
    pub fn generate() -> Result<Self, AccountIdentityError> {
        let mut seed = [0; 32];
        while seed == [0; 32] {
            SystemRandom::new()
                .fill(&mut seed)
                .map_err(|_| AccountIdentityError::Entropy)?;
        }
        Self::from_seed(seed)
    }

    pub fn from_seed(seed: [u8; 32]) -> Result<Self, AccountIdentityError> {
        if seed == [0; 32] {
            return Err(AccountIdentityError::InvalidKey);
        }
        let pair = Ed25519KeyPair::from_seed_unchecked(&seed)
            .map_err(|_| AccountIdentityError::InvalidKey)?;
        Ok(Self(Arc::new(KeyMaterial { seed, pair })))
    }

    pub fn export_seed(&self) -> [u8; 32] {
        self.0.seed
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.0.pair.public_key().as_ref().try_into().unwrap()
    }

    pub fn account(&self) -> AccountId {
        account_from_public_key(&self.public_key())
    }

    pub fn prove(&self, challenge: &AccountChallenge, payload: &[u8]) -> AccountProof {
        let account = self.account();
        AccountProof {
            account,
            public_key: self.public_key(),
            signature: self
                .0
                .pair
                .sign(&proof_message(challenge, account, payload))
                .as_ref()
                .try_into()
                .unwrap(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccountChallenge {
    pub match_key: frame::MatchKey,
    pub connection: ConnectionId,
    pub member: MemberId,
    pub nonce: [u8; 32],
}

impl AccountChallenge {
    pub fn new(
        match_key: frame::MatchKey,
        connection: ConnectionId,
        member: MemberId,
    ) -> Result<Self, AccountIdentityError> {
        if match_key.is_none()
            || match_key.session_id == [0; 16]
            || connection.0 == 0
            || member.0 == [0; 16]
        {
            return Err(AccountIdentityError::InvalidContext);
        }
        let mut nonce = [0; 32];
        while nonce == [0; 32] {
            SystemRandom::new()
                .fill(&mut nonce)
                .map_err(|_| AccountIdentityError::Entropy)?;
        }
        Ok(Self {
            match_key,
            connection,
            member,
            nonce,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountProof {
    pub account: AccountId,
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

impl AccountProof {
    pub fn verify(
        &self,
        challenge: &AccountChallenge,
        payload: &[u8],
    ) -> Result<AccountId, AccountIdentityError> {
        if self.account.0 == [0; 16] || account_from_public_key(&self.public_key) != self.account {
            return Err(AccountIdentityError::InvalidProof);
        }
        signature::UnparsedPublicKey::new(&signature::ED25519, self.public_key)
            .verify(
                &proof_message(challenge, self.account, payload),
                &self.signature,
            )
            .map_err(|_| AccountIdentityError::InvalidProof)?;
        Ok(self.account)
    }
}

fn account_from_public_key(public_key: &[u8; 32]) -> AccountId {
    let mut hash = digest::Context::new(&SHA256);
    hash.update(b"IW4L account identity v1\0");
    hash.update(public_key);
    AccountId(hash.finish().as_ref()[..16].try_into().unwrap())
}

fn proof_message(challenge: &AccountChallenge, account: AccountId, payload: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(160);
    message.extend_from_slice(b"IW4L account proof v1\0");
    message.extend_from_slice(&challenge.match_key.session_id);
    message.extend_from_slice(&challenge.match_key.match_epoch.to_le_bytes());
    message.extend_from_slice(&challenge.connection.0.to_le_bytes());
    message.extend_from_slice(&challenge.member.0);
    message.extend_from_slice(&challenge.nonce);
    message.extend_from_slice(&account.0);
    message.extend_from_slice(digest::digest(&SHA256, payload).as_ref());
    message
}
