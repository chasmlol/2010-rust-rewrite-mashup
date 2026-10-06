use std::sync::Arc;

use structured_data_iw4::{DataType, DefinitionSet, Key};

use crate::script::Value;

#[derive(Clone, Copy)]
pub(crate) enum Scope {
    Match,
    Client,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MatchData {
    match_data: Option<Buffer>,
    client_data: Option<Buffer>,
}

#[derive(Clone, Debug)]
struct Buffer {
    schema: Arc<DefinitionSet>,
    bytes: Vec<u8>,
    dirty: Vec<u8>,
}

impl MatchData {
    pub(crate) fn bind(&mut self, scope: Scope, schema: Arc<DefinitionSet>) -> Result<(), String> {
        let definition = schema
            .definitions
            .first()
            .ok_or("match data schema is empty")?;
        definition.validate().map_err(data_error)?;
        if definition.size < 8 {
            return Err("match data schema does not contain its version/checksum header".into());
        }
        let mut bytes = vec![0; definition.size];
        bytes[..4].copy_from_slice(&definition.version.to_le_bytes());
        bytes[4..8].copy_from_slice(&definition.checksum.to_le_bytes());
        let dirty = vec![0; definition.size.div_ceil(8)];
        let buffer = Some(Buffer {
            schema,
            bytes,
            dirty,
        });
        match scope {
            Scope::Match => self.match_data = buffer,
            Scope::Client => self.client_data = buffer,
        }
        Ok(())
    }

    fn buffer(&mut self, scope: Scope) -> Result<&mut Buffer, String> {
        match scope {
            Scope::Match => self.match_data.as_mut(),
            Scope::Client => self.client_data.as_mut(),
        }
        .ok_or_else(|| "match data schema has not been selected".into())
    }

    pub(crate) fn set(&mut self, scope: Scope, args: &[Value]) -> Result<Value, String> {
        let (value, path) = args
            .split_last()
            .ok_or("match data needs a key and value")?;
        let buffer = self.buffer(scope)?;
        let definition = &buffer.schema.definitions[0];
        let keys = keys(path)?;
        let lookup = definition.lookup(&keys).map_err(data_error)?;
        let value = data_value(lookup.ty, value)?;
        definition
            .write(&mut buffer.bytes, &mut buffer.dirty, lookup, value)
            .map_err(data_error)?;
        Ok(Value::Undefined)
    }

    pub(crate) fn get(&mut self, scope: Scope, path: &[Value]) -> Result<Value, String> {
        let buffer = self.buffer(scope)?;
        let definition = &buffer.schema.definitions[0];
        let lookup = definition.lookup(&keys(path)?).map_err(data_error)?;
        use structured_data_iw4::Value as V;
        Ok(
            match definition.read(&buffer.bytes, lookup).map_err(data_error)? {
                V::Int(n) => Value::Int(n),
                V::Bool(n) => Value::Int(n.into()),
                V::Float(n) => Value::Float(n),
                V::String(s) => Value::string(s),
                V::Bytes(s) => Value::byte_string(s),
            },
        )
    }
}

fn keys(values: &[Value]) -> Result<Vec<Key<'_>>, String> {
    values
        .iter()
        .map(|value| match value {
            Value::Int(index) => Ok(Key::Index(*index)),
            Value::String(name) => schema_name(name.as_bytes()).map(Key::Name),
            _ => Err("match data keys must be strings or integers".into()),
        })
        .collect()
}

fn data_value(ty: DataType, value: &Value) -> Result<structured_data_iw4::Value<'_>, String> {
    use structured_data_iw4::Value as V;
    match (ty, value) {
        (DataType::Int | DataType::Byte | DataType::Short, Value::Int(n)) => Ok(V::Int(*n)),
        (DataType::Bool, Value::Int(n)) => Ok(V::Bool(*n != 0)),
        (DataType::Float, Value::Float(n)) => Ok(V::Float(*n)),
        (DataType::Float, Value::Int(n)) => Ok(V::Float(*n as f32)),
        (DataType::String(_), Value::String(s)) => Ok(V::Bytes(s.as_bytes())),
        (DataType::String(_), Value::LocalizedString(s)) => Ok(V::String(s)),
        (DataType::Enum(_), Value::String(s)) => schema_name(s.as_bytes()).map(V::String),
        _ => Err(format!(
            "match data value does not match schema type {ty:?}"
        )),
    }
}

fn data_error(error: structured_data_iw4::Error) -> String {
    format!("match data: {error:?}")
}

fn schema_name(bytes: &[u8]) -> Result<&str, String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let prefix = &bytes[..end];
    std::str::from_utf8(prefix).map_err(|_| "match data schema name is not UTF-8".into())
}
