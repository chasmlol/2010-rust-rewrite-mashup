use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::Arc,
};

#[derive(Clone)]
pub struct ScriptString {
    text: Arc<str>,
    raw: Option<Arc<[u8]>>,
}

impl ScriptString {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let bytes = &bytes[..bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len())];
        match std::str::from_utf8(bytes) {
            Ok(text) => Self::from(text),
            Err(_) => Self {
                text: bytes
                    .iter()
                    .map(|byte| char::from(*byte))
                    .collect::<String>()
                    .into(),
                raw: Some(bytes.into()),
            },
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.raw.as_deref().unwrap_or_else(|| self.text.as_bytes())
    }

    pub(crate) fn symbol_key(&self) -> Arc<str> {
        if self.raw.is_none() && !self.text.starts_with('\u{e000}') {
            return self.text.clone();
        }
        let kind = if self.raw.is_some() { 'b' } else { 's' };
        let hex: String = self
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        format!("\u{e000}{kind}{hex}").into()
    }

    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }
    pub fn is_empty(&self) -> bool {
        self.as_bytes().is_empty()
    }
}
impl From<&str> for ScriptString {
    fn from(text: &str) -> Self {
        Self {
            text: text.split('\0').next().unwrap().into(),
            raw: None,
        }
    }
}
impl From<String> for ScriptString {
    fn from(text: String) -> Self {
        Self::from(Arc::<str>::from(text))
    }
}
impl From<Arc<str>> for ScriptString {
    fn from(text: Arc<str>) -> Self {
        if let Some(end) = text.find('\0') {
            Self::from(&text[..end])
        } else {
            Self { text, raw: None }
        }
    }
}
impl From<ScriptString> for Arc<str> {
    fn from(text: ScriptString) -> Self {
        text.text
    }
}
impl Deref for ScriptString {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}
impl AsRef<str> for ScriptString {
    fn as_ref(&self) -> &str {
        &self.text
    }
}
impl fmt::Display for ScriptString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.text.fmt(f)
    }
}
impl fmt::Debug for ScriptString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.text, f)
    }
}
impl PartialEq for ScriptString {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}
impl Eq for ScriptString {}
impl PartialOrd for ScriptString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ScriptString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}
impl Hash for ScriptString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Undefined,
    Int(i32),
    Float(f32),
    String(ScriptString),
    Vector([f32; 3]),
    Object(u64),
    Array(u64),
    Function(u32),
    Builtin(u32),
    LocalizedString(Arc<str>),
    Animation { tree: Arc<str>, name: Arc<str> },
    AnimationTree(Arc<str>),
}

impl Value {
    pub fn string(text: &str) -> Self {
        Self::String(text.into())
    }

    pub fn byte_string(bytes: &[u8]) -> Self {
        Self::String(ScriptString::from_bytes(bytes))
    }

    pub fn level() -> Self {
        Self::Object(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ArrayKey {
    Integer(i32),
    String(ScriptString),
}
