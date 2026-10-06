use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct EncodedAudio {
    bytes: Arc<[u8]>,
    content_id: [u8; 32],
}

impl From<Vec<u8>> for EncodedAudio {
    fn from(bytes: Vec<u8>) -> Self {
        let content_id = *blake3::hash(&bytes).as_bytes();
        Self {
            bytes: bytes.into(),
            content_id,
        }
    }
}

impl Default for EncodedAudio {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl EncodedAudio {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn shared(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
    }

    pub fn content_id(&self) -> [u8; 32] {
        self.content_id
    }
}
