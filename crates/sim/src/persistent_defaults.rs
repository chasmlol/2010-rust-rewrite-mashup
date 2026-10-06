use structured_data_iw4::{DataType, Definition, Error, Key, Lookup, Value};

use crate::{PLAYER_DATA_BUFFER_BYTES, PersistentDataError};

#[derive(Clone, Debug)]
pub struct PlayerDataDefaults {
    pub class_names: [Vec<u8>; 10],
    pub config: String,
}

pub(crate) fn initialize(
    definition: &Definition,
    defaults: &PlayerDataDefaults,
) -> Result<Vec<u8>, PersistentDataError> {
    if !(8..=PLAYER_DATA_BUFFER_BYTES).contains(&definition.size) {
        return Err(PersistentDataError::InvalidBuffer);
    }
    let mut bytes = vec![0; PLAYER_DATA_BUFFER_BYTES];
    bytes[..4].copy_from_slice(&definition.version.to_le_bytes());
    bytes[4..8].copy_from_slice(&definition.checksum.to_le_bytes());
    let mut dirty = vec![0; PLAYER_DATA_BUFFER_BYTES.div_ceil(8)];
    for (index, name) in defaults.class_names.iter().enumerate() {
        let keys = [
            Key::Name("customClasses"),
            Key::Index(index as i32),
            Key::Name("name"),
        ];
        definition.write(
            &mut bytes,
            &mut dirty,
            definition.lookup(&keys)?,
            Value::Bytes(name),
        )?;
    }
    for command in commands(&defaults.config)? {
        if !command
            .first()
            .is_some_and(|name| name.eq_ignore_ascii_case("setPlayerData"))
            || command.len() < 3
        {
            return Err(PersistentDataError::InvalidDefaults);
        }
        let lookup = lookup(definition, &command[1..command.len() - 1])?;
        let text = command.last().unwrap().split('\0').next().unwrap();
        let value = match lookup.ty {
            DataType::String(_) | DataType::Enum(_) => Value::String(text),
            DataType::Int | DataType::Byte | DataType::Short => Value::Int(
                text.parse()
                    .map_err(|_| PersistentDataError::InvalidDefaults)?,
            ),
            DataType::Bool => Value::Bool(
                text.parse::<i32>()
                    .map_err(|_| PersistentDataError::InvalidDefaults)?
                    != 0,
            ),
            DataType::Float => Value::Float(
                text.parse()
                    .map_err(|_| PersistentDataError::InvalidDefaults)?,
            ),
            _ => return Err(PersistentDataError::InvalidDefaults),
        };
        definition.write(&mut bytes, &mut dirty, lookup, value)?;
    }
    Ok(bytes)
}

fn lookup(definition: &Definition, tokens: &[String]) -> Result<Lookup, Error> {
    let mut keys = Vec::with_capacity(tokens.len());
    for token in tokens {
        keys.push(Key::Name(token));
        if definition.lookup(&keys) == Err(Error::WrongKey) {
            let index = token
                .split('\0')
                .next()
                .unwrap()
                .parse()
                .map_err(|_| Error::WrongKey)?;
            *keys.last_mut().unwrap() = Key::Index(index);
        }
        match definition.lookup(&keys) {
            Ok(_) | Err(Error::MissingKey) => {}
            Err(error) => return Err(error),
        }
    }
    definition.lookup(&keys)
}

fn commands(source: &str) -> Result<Vec<Vec<String>>, PersistentDataError> {
    let mut commands = Vec::new();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut present = false;
    let mut quoted = false;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            match ch {
                '"' => quoted = false,
                '\\' if chars.peek().is_some_and(|ch| matches!(ch, '\\' | '"')) => {
                    word.push(chars.next().unwrap())
                }
                _ => word.push(ch),
            }
            continue;
        }
        match ch {
            '"' => {
                quoted = true;
                present = true;
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                while chars.peek().is_some_and(|ch| !matches!(ch, '\n' | '\r')) {
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                if present {
                    words.push(std::mem::take(&mut word));
                    present = false;
                }
                chars.next();
                let mut closed = false;
                while let Some(ch) = chars.next() {
                    if ch == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err(PersistentDataError::InvalidDefaults);
                }
            }
            ch if ch.is_whitespace() || ch == ';' => {
                if present {
                    words.push(std::mem::take(&mut word));
                    present = false;
                }
                if matches!(ch, '\n' | '\r' | ';') && !words.is_empty() {
                    commands.push(std::mem::take(&mut words));
                }
            }
            _ => {
                word.push(ch);
                present = true;
            }
        }
    }
    if quoted {
        return Err(PersistentDataError::InvalidDefaults);
    }
    if present {
        words.push(word);
    }
    if !words.is_empty() {
        commands.push(words);
    }
    Ok(commands)
}
