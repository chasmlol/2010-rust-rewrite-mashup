use std::sync::Arc;

use crate::RuntimeImageId;

#[derive(Clone, Debug, Default)]
pub struct RuntimeCodeSources {
    constants: Vec<CodeConstantSlot>,

    textures: Vec<Option<u8>>,

    texture_images: Vec<Option<RuntimeImageId>>,

    overlay_mode: bool,
    written_const: Vec<u16>,
    written_tex: Vec<u32>,
    /// Every code-constant write this source has taken. See [`const_writes`].
    ///
    /// [`const_writes`]: RuntimeCodeSources::const_writes
    const_writes: u64,
}

#[derive(Clone, Debug, Default)]
struct CodeConstantSlot {
    rows: Option<Arc<[[u32; 4]]>>,
    live: bool,
}

impl CodeConstantSlot {
    fn live_rows(&self) -> Option<&Arc<[[u32; 4]]>> {
        self.live.then_some(self.rows.as_ref()).flatten()
    }
}

impl PartialEq for RuntimeCodeSources {
    fn eq(&self, other: &Self) -> bool {
        fn live(bank: &RuntimeCodeSources) -> Vec<Option<&Arc<[[u32; 4]]>>> {
            bank.constants
                .iter()
                .map(CodeConstantSlot::live_rows)
                .collect()
        }
        live(self) == live(other)
            && self.textures == other.textures
            && self.texture_images == other.texture_images
    }
}

impl Eq for RuntimeCodeSources {}

pub trait CodeSourceLookup {
    fn constant_arc(&self, index: u16) -> Option<Arc<[[u32; 4]]>>;
    fn texture(&self, index: u32) -> Option<u8>;
    fn texture_image(&self, index: u32) -> Option<RuntimeImageId>;
}

pub struct LayeredCodeSources<'a> {
    pub base: &'a RuntimeCodeSources,
    pub overlay: &'a RuntimeCodeSources,
}

impl CodeSourceLookup for RuntimeCodeSources {
    fn constant_arc(&self, index: u16) -> Option<Arc<[[u32; 4]]>> {
        RuntimeCodeSources::constant_arc(self, index)
    }

    fn texture(&self, index: u32) -> Option<u8> {
        RuntimeCodeSources::texture(self, index)
    }

    fn texture_image(&self, index: u32) -> Option<RuntimeImageId> {
        RuntimeCodeSources::texture_image(self, index)
    }
}

impl CodeSourceLookup for LayeredCodeSources<'_> {
    fn constant_arc(&self, index: u16) -> Option<Arc<[[u32; 4]]>> {
        self.overlay
            .constant_arc(index)
            .or_else(|| self.base.constant_arc(index))
    }

    fn texture(&self, index: u32) -> Option<u8> {
        self.overlay
            .texture(index)
            .or_else(|| self.base.texture(index))
    }

    fn texture_image(&self, index: u32) -> Option<RuntimeImageId> {
        if self.overlay.texture(index).is_some() {
            self.overlay.texture_image(index)
        } else {
            self.base.texture_image(index)
        }
    }
}

impl RuntimeCodeSources {
    pub fn begin_overlay(&mut self) {
        self.clear_overlay_slots();
        self.overlay_mode = true;
    }

    pub fn reset_overlay(&mut self) {
        self.clear_overlay_slots();
        self.overlay_mode = false;
    }

    fn clear_overlay_slots(&mut self) {
        for index in self.written_const.drain(..) {
            if let Some(slot) = self.constants.get_mut(usize::from(index)) {
                slot.live = false;
            }
        }
        for index in self.written_tex.drain(..) {
            let index = index as usize;
            if let Some(slot) = self.textures.get_mut(index) {
                *slot = None;
            }
            if let Some(slot) = self.texture_images.get_mut(index) {
                *slot = None;
            }
        }
    }

    fn note_const_write(&mut self, index: u16) {
        if !self.overlay_mode {
            return;
        }
        let occupied = self
            .constants
            .get(usize::from(index))
            .is_some_and(|slot| slot.live);
        if !occupied {
            self.written_const.push(index);
        }
    }

    fn note_tex_write(&mut self, index: u32) {
        if !self.overlay_mode {
            return;
        }
        let occupied = usize::try_from(index)
            .ok()
            .and_then(|i| self.textures.get(i))
            .copied()
            .flatten()
            .is_some();
        if !occupied {
            self.written_tex.push(index);
        }
    }

    pub fn set_constant(&mut self, index: u16, rows: Vec<[u32; 4]>) {
        self.set_constant_rows(index, &rows);
    }

    /// Code-constant writes since this source was created.
    ///
    /// Monotonic and never reset, so a caller measures a stretch of work by
    /// the difference across it. It counts writes, not live slots: writing the
    /// same constant twice is two, which is the number a caller trying to stop
    /// computing values nobody reads is asking about.
    pub fn const_writes(&self) -> u64 {
        self.const_writes
    }

    pub fn set_constant_rows(&mut self, index: u16, rows: &[[u32; 4]]) {
        self.const_writes += 1;
        self.note_const_write(index);
        let index = usize::from(index);
        if self.constants.len() <= index {
            self.constants
                .resize_with(index + 1, CodeConstantSlot::default);
        }
        let slot = &mut self.constants[index];
        slot.live = true;
        if let Some(existing) = slot.rows.as_mut()
            && existing.len() == rows.len()
            && let Some(unique) = Arc::get_mut(existing)
        {
            unique.copy_from_slice(rows);
            return;
        }
        slot.rows = Some(Arc::from(rows));
    }

    pub fn set_texture(&mut self, index: u32, sampler_state: u8) -> Result<(), CodeSourceError> {
        self.note_tex_write(index);
        let index = usize::try_from(index).map_err(|_| CodeSourceError::TextureIndexOverflow)?;
        if self.textures.len() <= index {
            self.textures.resize(index + 1, None);
        }
        self.textures[index] = Some(sampler_state);
        if self.texture_images.len() <= index {
            self.texture_images.resize(index + 1, None);
        }
        self.texture_images[index] = None;
        Ok(())
    }

    pub fn set_texture_with_image(
        &mut self,
        index: u32,
        sampler_state: u8,
        image: RuntimeImageId,
    ) -> Result<(), CodeSourceError> {
        self.set_texture(index, sampler_state)?;
        let slot = usize::try_from(index).map_err(|_| CodeSourceError::TextureIndexOverflow)?;
        if self.texture_images.len() <= slot {
            self.texture_images.resize(slot + 1, None);
        }
        self.texture_images[slot] = Some(image);
        Ok(())
    }

    fn constant_rows(&self, index: u16) -> Option<&[[u32; 4]]> {
        self.constants
            .get(usize::from(index))
            .and_then(CodeConstantSlot::live_rows)
            .map(|rows| &rows[..])
    }

    pub fn constant_arc(&self, index: u16) -> Option<Arc<[[u32; 4]]>> {
        self.constants
            .get(usize::from(index))
            .and_then(CodeConstantSlot::live_rows)
            .cloned()
    }

    pub fn has_constant(&self, index: u16) -> bool {
        self.constant_rows(index).is_some()
    }

    pub fn texture(&self, index: u32) -> Option<u8> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.textures.get(index))
            .copied()
            .flatten()
    }

    pub fn texture_image(&self, index: u32) -> Option<RuntimeImageId> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.texture_images.get(index))
            .copied()
            .flatten()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeSourceError {
    TextureIndexOverflow,
}

#[derive(Clone, Debug)]
pub struct CompiledConstantOverlay {
    generation: crate::MaterialGenerationId,
    rows: Vec<(u16, [u32; 4])>,
}

pub fn compile_constant_overlay(
    catalog: &crate::RuntimeMaterialCatalog,
    rows: Vec<(u16, [u32; 4])>,
) -> CompiledConstantOverlay {
    CompiledConstantOverlay {
        generation: catalog.generation_id(),
        rows,
    }
}

impl CompiledConstantOverlay {
    pub fn validate_generation(
        &self,
        catalog: &crate::RuntimeMaterialCatalog,
    ) -> Result<(), crate::MaterialRefusal> {
        let generation = catalog.generation_id();
        if self.generation != generation {
            return Err(crate::MaterialRefusal::StaleMaterialGeneration {
                retained: self.generation,
                current: generation,
            });
        }
        Ok(())
    }
    pub fn apply(
        &self,
        catalog: &crate::RuntimeMaterialCatalog,
        sources: &mut RuntimeCodeSources,
    ) -> Result<(), crate::MaterialRefusal> {
        self.validate_generation(catalog)?;
        for &(index, row) in &self.rows {
            sources.set_constant_rows(index, &[row]);
        }
        Ok(())
    }
}
