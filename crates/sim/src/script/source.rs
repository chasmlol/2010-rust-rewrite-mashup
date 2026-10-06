use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum SourceOrigin {
    Packaged = 0,
    BuiltIn = 1,
    #[default]
    External = 2,
}

pub trait SourceResolver {
    fn read(&self, module: &str) -> Result<String, String>;
    fn read_bytes(&self, module: &str) -> Result<Vec<u8>, String> {
        self.read(module).map(String::into_bytes)
    }
    /// Unclassified sources cannot authorize persistent account mutations.
    fn origin(&self, _module: &str) -> SourceOrigin {
        SourceOrigin::External
    }
}

impl SourceResolver for BTreeMap<String, String> {
    fn read(&self, module: &str) -> Result<String, String> {
        self.get(module)
            .cloned()
            .ok_or_else(|| format!("missing script module {module}"))
    }
}

pub struct FileSources(pub std::path::PathBuf);
impl SourceResolver for FileSources {
    fn read(&self, module: &str) -> Result<String, String> {
        self.read_bytes(module).map(|bytes| decode_source(&bytes))
    }
    fn read_bytes(&self, module: &str) -> Result<Vec<u8>, String> {
        std::fs::read(self.0.join(format!("{module}.gsc"))).map_err(|e| e.to_string())
    }
}

pub fn normalize_module(module: &str) -> Result<String, String> {
    let path = module.replace('\\', "/").to_ascii_lowercase();
    let path = path.strip_suffix(".gsc").unwrap_or(&path);
    if path.is_empty()
        || path.split('/').any(|s| {
            s.is_empty()
                || s == "."
                || s == ".."
                || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        return Err(format!("invalid script module {module:?}"));
    }
    Ok(path.to_owned())
}

pub fn decode_source(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(source) => source.to_owned(),
        Err(_) => bytes.iter().copied().map(char::from).collect(),
    }
}
