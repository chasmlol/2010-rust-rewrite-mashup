use super::*;
use std::result::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundHandle {
    revision: u64,
    alias: usize,
    variant: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundBindingRefusal {
    ForeignOwner,
    UnknownAlias,
    UnpublishedPolicy,
    ForeignMedia,
}

#[derive(Clone, Copy)]
pub struct BoundSound<'a> {
    catalog: &'a SoundCatalog,
    handle: SoundHandle,
}

pub enum BoundSoundMedia<'a> {
    Loaded {
        index: usize,
        sound: &'a LoadedSoundPcm,
    },
    Streamed {
        namespace: AssetNamespace,
        dir: String,
        name: String,
        decode: crate::StreamedDecodePolicy,
    },
}

impl<'a> BoundSound<'a> {
    pub fn revision(self) -> u64 {
        self.catalog.revision()
    }
    pub fn handle(self) -> SoundHandle {
        self.handle
    }
    pub fn alias_index(self) -> usize {
        self.handle.alias
    }
    pub fn variant(self) -> usize {
        self.handle.variant
    }
    pub fn policy(self) -> &'a crate::AliasPlaybackPolicy {
        &self.catalog.playback[self.handle.alias][self.handle.variant]
    }
    pub fn media(self) -> Result<Option<BoundSoundMedia<'a>>, SoundBindingRefusal> {
        let policy = self.policy();
        let row = &self.catalog.sounds[self.handle.alias].aliases[self.handle.variant];
        if let Some(index) = row.loaded.bound_index() {
            let sound = self
                .catalog
                .pcm_at(index)
                .ok_or(SoundBindingRefusal::ForeignMedia)?;
            if ns_of(sound.game) != policy.namespace {
                return Err(SoundBindingRefusal::ForeignMedia);
            }
            return Ok(Some(BoundSoundMedia::Loaded { index, sound }));
        }
        Ok(self
            .catalog
            .streamed_for_variant_at(self.handle.alias, self.handle.variant)
            .map(|(namespace, dir, name)| BoundSoundMedia::Streamed {
                namespace,
                dir,
                name,
                decode: policy.streamed_decode(),
            }))
    }
}

impl SoundCatalog {
    pub fn bind(&self, handle: SoundHandle) -> Result<BoundSound<'_>, SoundBindingRefusal> {
        if self.revision == 0 || self.revision != handle.revision {
            return Err(SoundBindingRefusal::ForeignOwner);
        }
        let sound = self
            .sounds
            .get(handle.alias)
            .ok_or(SoundBindingRefusal::UnknownAlias)?;
        sound
            .aliases
            .get(handle.variant)
            .ok_or(SoundBindingRefusal::UnknownAlias)?;
        let policy = self
            .playback_policy(handle.alias, handle.variant)
            .ok_or(SoundBindingRefusal::UnpublishedPolicy)?;
        if policy.namespace != ns_of(sound.game) {
            return Err(SoundBindingRefusal::UnpublishedPolicy);
        }
        Ok(BoundSound {
            catalog: self,
            handle,
        })
    }

    pub fn bind_published_alias(
        &self,
        alias: usize,
        variant: usize,
    ) -> Result<BoundSound<'_>, SoundBindingRefusal> {
        self.bind(SoundHandle {
            revision: self.revision,
            alias,
            variant,
        })
    }
}
