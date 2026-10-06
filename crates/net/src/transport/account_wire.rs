use sim::{AccountId, AccountSnapshot, PLAYER_DATA_BUFFER_BYTES};

use super::wire::{WireError, WireReader, WireWriter};
use crate::{AccountChallenge, AccountProof, ConnectionId};

const MAGIC: &[u8; 8] = b"IW4LACP2";
const MAX_BYTES: usize = 8500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AccountMessage {
    Challenge(AccountChallenge),
    Profile {
        challenge: AccountChallenge,
        proof: AccountProof,
        snapshot: Option<AccountSnapshot>,
    },
    Update {
        challenge: AccountChallenge,
        snapshot: AccountSnapshot,
    },
    Saved {
        challenge: AccountChallenge,
        account: AccountId,
        revision: u64,
    },
    Refused(AccountChallenge),
}

pub(crate) fn profile_payload(
    account: AccountId,
    snapshot: Option<&AccountSnapshot>,
) -> Result<Vec<u8>, WireError> {
    let mut out = WireWriter::new();
    out.put_bytes(&account.0);
    put_snapshot(&mut out, account, snapshot)?;
    Ok(out.finish())
}

fn put_snapshot(
    out: &mut WireWriter,
    account: AccountId,
    snapshot: Option<&AccountSnapshot>,
) -> Result<(), WireError> {
    out.put_u8(u8::from(snapshot.is_some()));
    if let Some(snapshot) = snapshot {
        if snapshot.account != account
            || snapshot.bytes.len() != PLAYER_DATA_BUFFER_BYTES
            || snapshot.bytes[..4] != snapshot.version.to_le_bytes()
            || snapshot.bytes[4..8] != snapshot.checksum.to_le_bytes()
        {
            return Err(WireError::Malformed(
                "account snapshot owner, size or stamp",
            ));
        }
        out.put_u64(snapshot.revision);
        out.put_i32(snapshot.version);
        out.put_u32(snapshot.checksum);
        out.put_bytes(&snapshot.bytes);
        out.put_bytes(&snapshot.skills.encode());
    }
    Ok(())
}

fn get_array<const N: usize>(input: &mut WireReader<'_>) -> Result<[u8; N], WireError> {
    let mut bytes = [0; N];
    input.get_bytes(&mut bytes)?;
    Ok(bytes)
}

fn get_snapshot(
    input: &mut WireReader<'_>,
    account: AccountId,
) -> Result<Option<AccountSnapshot>, WireError> {
    match input.get_u8()? {
        0 => Ok(None),
        1 => {
            let revision = input.get_u64()?;
            let version = input.get_i32()?;
            let checksum = input.get_u32()?;
            let mut bytes = vec![0; PLAYER_DATA_BUFFER_BYTES];
            input.get_bytes(&mut bytes)?;
            let skills =
                sim::SkillRatings::decode(&get_array::<{ sim::SKILL_RATING_BYTES }>(input)?)
                    .map_err(|_| WireError::Malformed("account skill ratings"))?;
            if bytes[..4] != version.to_le_bytes() || bytes[4..8] != checksum.to_le_bytes() {
                return Err(WireError::Malformed("account snapshot stamp"));
            }
            Ok(Some(AccountSnapshot {
                account,
                revision,
                version,
                checksum,
                bytes,
                skills,
            }))
        }
        _ => Err(WireError::Malformed("account snapshot tag")),
    }
}

fn put_context(out: &mut WireWriter, context: &AccountChallenge) {
    out.put_bytes(&context.match_key.session_id);
    out.put_u32(context.match_key.match_epoch);
    out.put_u64(context.connection.0);
    out.put_bytes(&context.member.0);
    out.put_bytes(&context.nonce);
}

fn get_context(input: &mut WireReader<'_>) -> Result<AccountChallenge, WireError> {
    let session_id = get_array(input)?;
    let epoch = input.get_u32()?;
    let connection = ConnectionId(input.get_u64()?);
    let member = master_protocol::MemberId(get_array(input)?);
    let nonce = get_array(input)?;
    if session_id == [0; 16]
        || epoch == 0
        || connection.0 == 0
        || member.0 == [0; 16]
        || nonce == [0; 32]
    {
        return Err(WireError::Malformed("account challenge context"));
    }
    Ok(AccountChallenge {
        match_key: frame::MatchKey::new(session_id, epoch),
        connection,
        member,
        nonce,
    })
}

impl AccountMessage {
    pub(crate) fn encode(&self) -> Result<Vec<u8>, WireError> {
        let mut out = WireWriter::new();
        out.put_bytes(MAGIC);
        match self {
            Self::Challenge(context) | Self::Refused(context) => {
                out.put_u8(if matches!(self, Self::Challenge(_)) {
                    0
                } else {
                    4
                });
                put_context(&mut out, context);
            }
            Self::Profile {
                challenge,
                proof,
                snapshot,
            } => {
                out.put_u8(1);
                put_context(&mut out, challenge);
                out.put_bytes(&proof.account.0);
                out.put_bytes(&proof.public_key);
                out.put_bytes(&proof.signature);
                put_snapshot(&mut out, proof.account, snapshot.as_ref())?;
            }
            Self::Update {
                challenge,
                snapshot,
            } => {
                out.put_u8(2);
                put_context(&mut out, challenge);
                out.put_bytes(&snapshot.account.0);
                put_snapshot(&mut out, snapshot.account, Some(snapshot))?;
            }
            Self::Saved {
                challenge,
                account,
                revision,
            } => {
                out.put_u8(3);
                put_context(&mut out, challenge);
                out.put_bytes(&account.0);
                out.put_u64(*revision);
            }
        }
        Ok(out.finish())
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Option<Self>, WireError> {
        if !bytes.starts_with(MAGIC) {
            return Ok(None);
        }
        if bytes.len() > MAX_BYTES {
            return Err(WireError::Malformed("account packet size"));
        }
        let mut input = WireReader::new(&bytes[MAGIC.len()..]);
        let tag = input.get_u8()?;
        let challenge = get_context(&mut input)?;
        let message = match tag {
            0 => Self::Challenge(challenge),
            1 => {
                let account = AccountId(get_array(&mut input)?);
                let public_key = get_array(&mut input)?;
                let signature = get_array(&mut input)?;
                let snapshot = get_snapshot(&mut input, account)?;
                Self::Profile {
                    challenge,
                    proof: AccountProof {
                        account,
                        public_key,
                        signature,
                    },
                    snapshot,
                }
            }
            2 => {
                let account = AccountId(get_array(&mut input)?);
                let snapshot = get_snapshot(&mut input, account)?
                    .ok_or(WireError::Malformed("empty account update"))?;
                Self::Update {
                    challenge,
                    snapshot,
                }
            }
            3 => Self::Saved {
                challenge,
                account: AccountId(get_array(&mut input)?),
                revision: input.get_u64()?,
            },
            4 => Self::Refused(challenge),
            _ => return Err(WireError::Malformed("account packet tag")),
        };
        if !input.is_empty() {
            return Err(WireError::Malformed("trailing account packet bytes"));
        }
        Ok(Some(message))
    }
}
