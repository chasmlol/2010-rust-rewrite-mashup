use crate::script::Value;
use crate::script::runtime::to_text;

pub(crate) fn arg(args: &[Value], index: usize) -> Result<&Value, String> {
    args.get(index)
        .ok_or_else(|| format!("parameter {} does not exist", index + 1))
}

pub(crate) fn float(args: &[Value], index: usize) -> Result<f32, String> {
    match arg(args, index)? {
        Value::Int(n) => Ok(*n as f32),
        Value::Float(n) => Ok(*n),
        other => Err(format!(
            "parameter {} is {}, not a float",
            index + 1,
            kind(other)
        )),
    }
}

pub(crate) fn int(args: &[Value], index: usize) -> Result<i32, String> {
    match arg(args, index)? {
        Value::Int(n) => Ok(*n),
        Value::Float(n) => Ok(*n as i32),
        other => Err(format!(
            "parameter {} is {}, not an int",
            index + 1,
            kind(other)
        )),
    }
}

pub(crate) fn vector(args: &[Value], index: usize) -> Result<[f32; 3], String> {
    match arg(args, index)? {
        Value::Vector(v) => Ok(*v),
        other => Err(format!(
            "parameter {} is {}, not a vector",
            index + 1,
            kind(other)
        )),
    }
}

pub(crate) fn optional<T>(
    args: &[Value],
    index: usize,
    read: fn(&[Value], usize) -> Result<T, String>,
) -> Result<Option<T>, String> {
    match args.get(index) {
        None | Some(Value::Undefined) => Ok(None),
        Some(_) => read(args, index).map(Some),
    }
}

pub(crate) fn kind(value: &Value) -> &'static str {
    match value {
        Value::Undefined => "undefined",
        Value::Int(_) => "an int",
        Value::Float(_) => "a float",
        Value::String(_) => "a string",
        Value::LocalizedString(_) => "a localized string",
        Value::Vector(_) => "a vector",
        Value::Object(_) => "an object",
        Value::Array(_) => "an array",
        Value::Function(_) | Value::Builtin(_) => "a function",
        Value::Animation { .. } => "an animation",
        Value::AnimationTree(_) => "an animtree",
    }
}

pub(crate) fn string(args: &[Value], index: usize) -> Result<String, String> {
    let value = args
        .get(index)
        .ok_or_else(|| format!("parameter {} does not exist", index + 1))?;
    match value {
        Value::String(text) => Ok(text.to_string()),
        other => to_text(other)
            .ok_or_else(|| format!("parameter {} cannot be cast to string", index + 1)),
    }
}

pub(crate) fn byte_string(
    args: &[Value],
    index: usize,
) -> Result<crate::script::ScriptString, String> {
    match arg(args, index)? {
        Value::String(text) => Ok(text.clone()),
        _ => string(args, index).map(Into::into),
    }
}
