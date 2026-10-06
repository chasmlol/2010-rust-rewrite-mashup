use sim::{FireCommandOutcome, FireCommandRefusal, FireCommandResult};

use super::meta_wire::{decode_fire_cause, encode_fire_cause};
use super::wire::{WireError, WireReader, WireWriter};

pub const MAX_FIRE_RESULTS: usize = 64;

pub fn encode_fire_results(out: &mut WireWriter, results: &[FireCommandResult]) {
    assert!(
        results.len() <= MAX_FIRE_RESULTS,
        "fire result batch exceeded capacity"
    );
    out.put_u8(results.len() as u8);
    for result in results {
        out.put_u32(result.client.0);
        out.put_u32(result.command.0);
        match result.life {
            None => out.put_u8(0),
            Some(life) => {
                out.put_u8(1);
                out.put_u32(life.0);
            }
        }
        match result.outcome {
            FireCommandOutcome::NotRun(reason) => {
                out.put_u8(0);
                out.put_u8(match reason {
                    FireCommandRefusal::MatchInactive => 0,
                    FireCommandRefusal::NotAlive => 1,
                    FireCommandRefusal::MissingPlayer => 2,
                    FireCommandRefusal::StaleCommand => 3,
                });
            }
            FireCommandOutcome::Executed { accepted } => {
                out.put_u8(1);
                for cause in accepted {
                    encode_fire_cause(out, cause);
                }
            }
        }
    }
}

pub fn decode_fire_results(
    input: &mut WireReader<'_>,
) -> Result<Vec<FireCommandResult>, WireError> {
    let count = input.get_u8()? as usize;
    if count > MAX_FIRE_RESULTS {
        return Err(WireError::Malformed("fire result batch capacity"));
    }
    let mut results = Vec::with_capacity(count);
    for _ in 0..count {
        let client = sim::ClientId(input.get_u32()?);
        let command = sim::CommandSequence(input.get_u32()?);
        let life = match input.get_u8()? {
            0 => None,
            1 => Some(sim::LifeSequence(input.get_u32()?)),
            _ => return Err(WireError::Malformed("invalid fire result life tag")),
        };
        let outcome = match input.get_u8()? {
            0 => FireCommandOutcome::NotRun(match input.get_u8()? {
                0 => FireCommandRefusal::MatchInactive,
                1 => FireCommandRefusal::NotAlive,
                2 => FireCommandRefusal::MissingPlayer,
                3 => FireCommandRefusal::StaleCommand,
                _ => return Err(WireError::Malformed("invalid fire refusal tag")),
            }),
            1 => {
                if life.is_none() {
                    return Err(WireError::Malformed("executed fire result has no life"));
                }
                let accepted = [decode_fire_cause(input)?, decode_fire_cause(input)?];
                for (ordinal, cause) in accepted.iter().enumerate() {
                    if let Some(cause) = cause
                        && (cause.client != client
                            || Some(cause.life) != life
                            || cause.command != command
                            || cause.ordinal != ordinal as u16)
                    {
                        return Err(WireError::Malformed("fire result cause mismatch"));
                    }
                }
                if accepted[1].is_some() && accepted[0].is_none()
                    || accepted[0]
                        .zip(accepted[1])
                        .is_some_and(|(a, b)| a.hand == b.hand)
                {
                    return Err(WireError::Malformed("invalid accepted fire set"));
                }
                FireCommandOutcome::Executed { accepted }
            }
            _ => return Err(WireError::Malformed("invalid fire result outcome tag")),
        };
        results.push(FireCommandResult {
            client,
            life,
            command,
            outcome,
        });
    }
    Ok(results)
}
