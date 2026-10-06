use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use asset_iw4::{
    IWI_V8_HEADER_LEN, ImgFormatKind, IwiHeader, WaveletBits, WaveletError, img_format_info,
    wavelet_check_header, wavelet_decompress_level, wavelet_level_size, wavelet_pixel_stride,
    wavelet_top_level,
};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use bevy::tasks::TaskPool;

use crate::progress::StageHandle;
use crate::{AuthoredImage, ImageVariantId, MaterialDefinitions, TS_NORMAL_MAP, TS_WATER_MAP};

/// The texels one archive entry decodes to.
///
/// The mip chain is one allocation with the levels laid out end to end, in the
/// order an upload wants them, and `level_sizes` says where each one stops.
struct DecodedMips {
    width: u32,
    height: u32,

    packed: Vec<u8>,
    level_sizes: Vec<u32>,
    storage: MipStorage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MipStorage {
    Rgba8,
    Bc1,
    Bc2,
    Bc3,
    Bc5,
}

impl DecodedMips {
    fn single(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self::single_compressed(width, height, MipStorage::Rgba8, pixels)
    }

    fn single_compressed(width: u32, height: u32, storage: MipStorage, bytes: Vec<u8>) -> Self {
        Self {
            width,
            height,
            level_sizes: vec![bytes.len() as u32],
            packed: bytes,
            storage,
        }
    }

    fn from_levels(width: u32, height: u32, storage: MipStorage, levels: Vec<Vec<u8>>) -> Self {
        let mut packed = Vec::with_capacity(levels.iter().map(Vec::len).sum());
        let mut level_sizes = Vec::with_capacity(levels.len());
        for level in levels {
            level_sizes.push(level.len() as u32);
            packed.extend_from_slice(&level);
        }
        Self {
            width,
            height,
            packed,
            level_sizes,
            storage,
        }
    }

    fn level_count(&self) -> u32 {
        self.level_sizes.len() as u32
    }

    fn into_payload(self) -> Vec<u8> {
        self.packed
    }

    fn cache_encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(28 + 4 * self.level_sizes.len() + self.packed.len());
        out.extend_from_slice(MIP_MAGIC);
        out.extend_from_slice(&MIP_CACHE_FORMAT.to_le_bytes());
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        out.push(match self.storage {
            MipStorage::Rgba8 => 0,
            MipStorage::Bc1 => 1,
            MipStorage::Bc2 => 2,
            MipStorage::Bc3 => 3,
            MipStorage::Bc5 => 5,
        });
        out.extend_from_slice(&[0, 0, 0]);
        out.extend_from_slice(&(self.level_sizes.len() as u32).to_le_bytes());
        let mut at = 0usize;
        for &size in &self.level_sizes {
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&self.packed[at..at + size as usize]);
            at += size as usize;
        }
        out
    }

    fn cache_decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 28 || &bytes[..8] != MIP_MAGIC {
            return None;
        }
        let format = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
        if format != MIP_CACHE_FORMAT {
            return None;
        }
        let width = u32::from_le_bytes(bytes[12..16].try_into().ok()?);
        let height = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
        let storage = match bytes[20] {
            0 => MipStorage::Rgba8,
            1 => MipStorage::Bc1,
            2 => MipStorage::Bc2,
            3 => MipStorage::Bc3,
            5 => MipStorage::Bc5,
            _ => return None,
        };
        let level_count = u32::from_le_bytes(bytes[24..28].try_into().ok()?) as usize;
        let mut at = 28;
        let mut packed = Vec::with_capacity(bytes.len().saturating_sub(28));
        let mut level_sizes = Vec::with_capacity(level_count);
        for _ in 0..level_count {
            let len = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
            at += 4;
            packed.extend_from_slice(bytes.get(at..at + len)?);
            at += len;
            level_sizes.push(len as u32);
        }
        Some(Self {
            width,
            height,
            packed,
            level_sizes,
            storage,
        })
    }

    fn into_top_level_rgba8(mut self) -> Result<(u32, u32, Vec<u8>), String> {
        let top = *self
            .level_sizes
            .first()
            .ok_or_else(|| "decoded image carries no mip level".to_owned())?
            as usize;
        self.packed.truncate(top);
        let level0 = self.packed;
        let pixels = match self.storage {
            MipStorage::Rgba8 => level0,
            MipStorage::Bc1 => decode_blocks(&level0, self.width, self.height, PixelFormat::Bc1)?,
            MipStorage::Bc2 => decode_blocks(&level0, self.width, self.height, PixelFormat::Bc2)?,
            MipStorage::Bc3 => decode_blocks(&level0, self.width, self.height, PixelFormat::Bc3)?,
            MipStorage::Bc5 => decode_blocks(
                &bc5_to_dxt5nm(&level0),
                self.width,
                self.height,
                PixelFormat::Bc3,
            )?,
        };
        Ok((self.width, self.height, pixels))
    }

    fn texture_format(self_storage: MipStorage, linear: bool) -> TextureFormat {
        match (self_storage, linear) {
            (MipStorage::Rgba8, true) => TextureFormat::Rgba8Unorm,
            (MipStorage::Rgba8, false) => TextureFormat::Rgba8UnormSrgb,
            (MipStorage::Bc1, true) => TextureFormat::Bc1RgbaUnorm,
            (MipStorage::Bc1, false) => TextureFormat::Bc1RgbaUnormSrgb,
            (MipStorage::Bc2, true) => TextureFormat::Bc2RgbaUnorm,
            (MipStorage::Bc2, false) => TextureFormat::Bc2RgbaUnormSrgb,
            (MipStorage::Bc3, true) => TextureFormat::Bc3RgbaUnorm,
            (MipStorage::Bc3, false) => TextureFormat::Bc3RgbaUnormSrgb,
            (MipStorage::Bc5, _) => TextureFormat::Bc5RgUnorm,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MaterialImageStats {
    pub requested: usize,
    pub decoded: usize,
    pub missing: usize,
    pub unsupported: usize,
    pub archives: usize,
    pub first_gap: Option<String>,
    pub first_unsupported: Option<String>,
}

#[derive(Debug)]
struct IwdIndex(Arc<asset_transport::IwdIndex>);

type CubemapFaces = [Vec<u8>; 6];

pub fn cached_iwd_main_dirs() -> Vec<PathBuf> {
    asset_transport::cached_iwd_dirs()
}

impl IwdIndex {
    fn open(directory: &Path) -> Result<Arc<Self>, String> {
        asset_transport::IwdIndex::open(directory).map(|index| Arc::new(Self(index)))
    }

    fn is_cached(directory: &Path) -> bool {
        asset_transport::IwdIndex::is_cached(directory)
    }

    fn decode(&self, name: &str) -> Option<Result<DecodedMips, String>> {
        let candidates = self.0.image_candidates(name)?;
        let mut first_error = None;
        for candidate in candidates {
            match load_or_decode_mips(candidate) {
                Ok(image) => return Some(Ok(image)),
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        Some(Err(
            first_error.unwrap_or_else(|| "empty IWD candidate list".into())
        ))
    }

    /// Is this name a cubemap, and if so, its faces.
    ///
    /// The material's own `map_type` answers for a declared cubemap. For
    /// everything else the answer is in the IWI usage byte, which is inside
    /// the first thirty-two bytes of the entry — so a 2D image costs a header
    /// here, not a full inflate of a payload the mip cache may already hold
    /// decoded.
    fn decode_cubemap_if_skybox(
        &self,
        name: &str,
        map_type: u8,
    ) -> Option<Result<(u32, CubemapFaces), String>> {
        let candidates = self.0.image_candidates(name)?;
        let mut saw_skybox = false;
        let mut first_error = None;
        for candidate in candidates {
            if map_type != 5 {
                match candidate.read_header(IWI_V8_HEADER_LEN) {
                    Ok(header) if IwiHeader::parse(&header).is_ok_and(|h| h.is_skybox()) => {}
                    Ok(_) => continue,
                    Err(error) => {
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                        continue;
                    }
                }
            }
            match candidate.read() {
                Ok(bytes) => {
                    saw_skybox = true;
                    match decode_iwi_cubemap(&bytes) {
                        Ok(image) => return Some(Ok(image)),
                        Err(error) if first_error.is_none() => first_error = Some(error),
                        Err(_) => {}
                    }
                }
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        saw_skybox.then(|| Err(first_error.unwrap_or_else(|| "empty IWD candidate list".into())))
    }
}

fn color_map_force_linear(
    _is_unlit: Option<bool>,
    transform: crate::ColorMapTransform,
    takes_model_lighting: Option<bool>,
) -> bool {
    transform == crate::ColorMapTransform::Square || takes_model_lighting == Some(true)
}

fn texture_semantic_decodes_as_normal(semantic: u8) -> Option<bool> {
    match semantic {
        TS_WATER_MAP => None,
        TS_NORMAL_MAP => Some(true),
        _ => Some(false),
    }
}

pub fn decode_material_color_maps(
    zone_ff: &Path,
    catalog: &mut MaterialDefinitions,
    stage: &StageHandle,
    pool: &TaskPool,
) -> Result<MaterialImageStats, String> {
    let main = game_main_for_zone(zone_ff)?;
    let index = IwdIndex::open(&main)?;
    let mut stats = MaterialImageStats {
        archives: index.0.archive_count(),
        ..Default::default()
    };

    let work = requested_color_map_slots(catalog);
    stats.requested = work.len();
    stage.set_total(work.len() as u64);

    let images = &catalog.images;
    let decoded = decode_requests_in_parallel(images, index.as_ref(), &work, stage, pool);

    for outcome in decoded {
        match outcome {
            ImageOutcome::Decoded {
                image_index,
                payload,
                wrap,
                variant,
                ..
            } => {
                let slot = &mut catalog.images[image_index];
                slot.decoded = Some(wrap_payload(&payload, wrap));
                slot.decoded_variant = variant;
                slot.common_owned = false;
                stats.decoded += 1;
            }
            ImageOutcome::Missing { gap } => {
                stats.missing += 1;
                stats.first_gap.get_or_insert(gap);
            }
            ImageOutcome::Unsupported { gap } => {
                stats.unsupported += 1;
                stats.first_unsupported.get_or_insert(gap);
            }
        }
    }
    Ok(stats)
}

fn requested_color_map_slots(catalog: &MaterialDefinitions) -> Vec<ImageRequest> {
    let mut requested = vec![None; catalog.images.len()];
    for material in &catalog.materials {
        let alpha_test = catalog
            .agreed_alpha_test_cutoff(material)
            .is_some_and(|cutoff| cutoff.is_some());
        let force_linear = color_map_force_linear(
            catalog.is_unlit(material),
            catalog.color_map_transform(material),
            catalog.takes_model_lighting(material),
        );
        for texture in &material.textures {
            let is_normal = match texture_semantic_decodes_as_normal(texture.semantic) {
                Some(is_normal) => is_normal,
                None => continue,
            };
            let Some(image) = texture.image else {
                continue;
            };
            if catalog
                .images
                .get(image)
                .is_some_and(|image| image.decoded.is_some() || image.pending_decode.is_some())
            {
                continue;
            }
            let Some(slot) = requested.get_mut(image) else {
                continue;
            };
            let sharp = alpha_test && !is_normal;
            let linear = force_linear && !is_normal;
            match slot {
                None => *slot = Some((texture.sampler_state, is_normal, sharp, linear)),
                Some((_, already_normal, already_sharp, already_linear)) => {
                    *already_normal |= is_normal;
                    *already_sharp |= sharp;
                    *already_linear |= linear;
                }
            }
        }
    }
    requested
        .into_iter()
        .enumerate()
        .filter_map(|(image_index, request)| request.map(|request| (image_index, request)))
        .collect()
}

pub fn decode_images_for_keys(
    zone_ff: &Path,
    catalog: &mut MaterialDefinitions,
    keys: impl IntoIterator<Item = crate::MaterialKey>,
    stage: &StageHandle,
    pool: &TaskPool,
) -> Result<usize, String> {
    let work = requested_keyed_image_slots(catalog, keys);
    if work.is_empty() {
        return Ok(0);
    }
    let main = game_main_for_zone(zone_ff)?;
    let index = IwdIndex::open(&main)?;
    stage.set_total(work.len() as u64);
    let images = &catalog.images;
    let decoded = decode_requests_in_parallel(images, index.as_ref(), &work, stage, pool);
    let mut n = 0usize;
    for outcome in decoded {
        if let ImageOutcome::Decoded {
            image_index,
            payload,
            wrap,
            variant,
            ..
        } = outcome
        {
            let slot = &mut catalog.images[image_index];
            slot.decoded = Some(wrap_payload(&payload, wrap));
            slot.decoded_variant = variant;
            slot.common_owned = false;
            n += 1;
        }
    }
    Ok(n)
}

fn requested_keyed_image_slots(
    catalog: &MaterialDefinitions,
    keys: impl IntoIterator<Item = crate::MaterialKey>,
) -> Vec<ImageRequest> {
    let wanted: std::collections::HashSet<(crate::AssetNamespace, String)> = keys
        .into_iter()
        .map(|key| {
            let name = crate::AssetRef::bare_name(&key.name).to_owned();
            (key.namespace, name)
        })
        .filter(|(_, name)| !name.is_empty())
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut work: Vec<ImageRequest> = Vec::new();
    for material in &catalog.materials {
        if !material.name.is_real() {
            continue;
        }
        let bind = material.name.as_str();
        if bind.is_empty() || !wanted.contains(&(material.namespace, bind.to_owned())) {
            continue;
        }
        let alpha_test = catalog
            .agreed_alpha_test_cutoff(material)
            .is_some_and(|cutoff| cutoff.is_some());
        let force_linear = color_map_force_linear(
            catalog.is_unlit(material),
            catalog.color_map_transform(material),
            catalog.takes_model_lighting(material),
        );
        for texture in &material.textures {
            let Some(is_normal) = texture_semantic_decodes_as_normal(texture.semantic) else {
                continue;
            };
            let Some(image) = texture.image else {
                continue;
            };
            if catalog
                .images
                .get(image)
                .is_some_and(|img| img.decoded.is_some() || img.pending_decode.is_some())
            {
                continue;
            }
            let sharp = alpha_test && !is_normal;
            let linear = force_linear && !is_normal;
            if let Some((_, (_, normal, already_sharp, already_linear))) =
                work.iter_mut().find(|(already, _)| *already == image)
            {
                *normal |= is_normal;
                *already_sharp |= sharp;
                *already_linear |= linear;
            } else {
                work.push((image, (texture.sampler_state, is_normal, sharp, linear)));
            }
        }
    }
    work
}

pub fn decode_catalog_images_from_iwd(
    zone_ff: &Path,
    catalog: &mut MaterialDefinitions,
    images: impl IntoIterator<Item = (usize, u8)>,
    stage: &StageHandle,
    pool: &TaskPool,
) -> Result<usize, String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut work: Vec<ImageRequest> = Vec::new();
    for (image, sampler_state) in images {
        if !seen.insert(image) {
            continue;
        }
        if catalog
            .images
            .get(image)
            .is_some_and(|img| img.decoded.is_some())
        {
            continue;
        }
        work.push((image, (sampler_state, false, false, true)));
    }
    if work.is_empty() {
        return Ok(0);
    }
    let main = game_main_for_zone(zone_ff)?;
    let index = IwdIndex::open(&main)?;
    stage.set_total(work.len() as u64);
    let catalog_images = &catalog.images;
    let decoded = decode_requests_in_parallel(catalog_images, index.as_ref(), &work, stage, pool);
    let mut n = 0usize;
    for outcome in decoded {
        if let ImageOutcome::Decoded {
            image_index,
            payload,
            wrap,
            variant,
            ..
        } = outcome
        {
            let slot = &mut catalog.images[image_index];
            slot.decoded = Some(wrap_payload(&payload, wrap));
            slot.decoded_variant = variant;
            n += 1;
        }
    }
    Ok(n)
}

pub fn decode_in_zone_builtin_images(catalog: &mut MaterialDefinitions) -> usize {
    let mut decoded = 0usize;
    for image in &mut catalog.images {
        if image.decoded.is_some() || image.payload.is_empty() || image.map_type != 3 {
            continue;
        }
        if !image.name.is_real() {
            continue;
        }
        let name = image.name.as_str();
        if !name.starts_with('$') {
            continue;
        }
        let Ok(mips) = decode_gfx_image(
            &image.payload,
            u32::from(image.width),
            u32::from(image.height),
            image.format,
        ) else {
            continue;
        };
        let is_normal = name.contains("normal");
        let format = DecodedMips::texture_format(mips.storage, is_normal || !image.use_srgb_reads);
        let levels = mips.level_count();
        let mut gpu = Image::new_uninit(
            Extent3d {
                width: mips.width,
                height: mips.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            format,
            RenderAssetUsages::RENDER_WORLD,
        );
        gpu.texture_descriptor.mip_level_count = levels;
        gpu.data = Some(mips.into_payload());
        gpu.sampler = ImageSampler::Descriptor(sampler_from_iw4(0, levels, false));
        image.decoded = Some(Arc::new(gpu));
        decoded += 1;
    }
    decoded
}

enum ImageOutcome {
    Decoded {
        image_index: usize,
        /// The texels, which askers share. The `Image` around them is built
        /// where it is placed, from `wrap`, because that is the object two
        /// askers for one archive entry cannot share.
        payload: Arc<PreparedPayload>,
        wrap: WrapRecipe,
        /// Which prepared variant this is, when it came out of an archive.
        variant: Option<ImageVariantId>,
        /// The texels came out of another asker's decode rather than this
        /// one's read.
        shared: bool,
    },
    Missing {
        gap: String,
    },
    Unsupported {
        gap: String,
    },
}

type ImageRequest = (usize, (u8, bool, bool, bool));

enum PreparedPayload {
    Mips(std::sync::Mutex<PreparedMip>),
    Cubemap { size: u32, faces: Box<CubemapFaces> },
}

struct PreparedMip {
    mips: DecodedMips,
    images: Vec<(WrapRecipe, Arc<Image>)>,
}

/// Everything an asker wraps around shared texels, and nothing that decides
/// one of them.
///
/// The sampler is not a property of the image at all — a sampler state is bound
/// beside a texture, not inside it — and the colour space picks `Rgba8Unorm`
/// over `Rgba8UnormSrgb` for the same bytes. Keeping them
/// here, out of the payload, is what lets one decode answer several askers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WrapRecipe {
    sampler_state: u8,
    is_normal: bool,
    alpha_test_color: bool,
    force_linear: bool,
    use_srgb_reads: bool,
}

impl WrapRecipe {
    fn of(
        source: &AuthoredImage,
        (sampler_state, is_normal, alpha_test_color, force_linear): (u8, bool, bool, bool),
    ) -> Self {
        Self {
            sampler_state,
            is_normal,
            alpha_test_color,
            force_linear,
            use_srgb_reads: source.use_srgb_reads,
        }
    }

    fn linear(self) -> bool {
        self.is_normal || !self.use_srgb_reads || self.force_linear
    }
}

impl PreparedPayload {
    fn mips(mips: DecodedMips) -> Self {
        Self::Mips(std::sync::Mutex::new(PreparedMip {
            mips,
            images: Vec::new(),
        }))
    }

    /// What holding this payload costs, which is what a second asker did not
    /// have to decode.
    fn bytes(&self) -> u64 {
        match self {
            Self::Mips(state) => {
                let state = state.lock().unwrap_or_else(|poison| poison.into_inner());
                state
                    .images
                    .first()
                    .map_or(state.mips.packed.len() as u64, |(_, image)| {
                        image_bytes(image)
                    })
            }
            Self::Cubemap { faces, .. } => faces.iter().map(|face| face.len() as u64).sum(),
        }
    }

    /// Take ownership of this payload's bytes against the resident total, and
    /// hand back the only thing that can release them again.
    ///
    /// The charge belongs to the payload and not to the batch that asked for
    /// it: two batches pointing at one buffer hold one allocation between
    /// them, and charging it twice would hold plans back against memory nobody
    /// had.
    fn owned(self) -> Arc<Self> {
        RESIDENT_PAYLOAD_BYTES.fetch_add(self.bytes(), Ordering::Relaxed);
        Arc::new(self)
    }
}

impl Drop for PreparedPayload {
    fn drop(&mut self) {
        let bytes = self.bytes();
        if bytes > 0 {
            RESIDENT_PAYLOAD_BYTES.fetch_sub(bytes, Ordering::Relaxed);
        }
    }
}

/// Payloads that are still alive somewhere, so a second plan asking for the
/// same archive entry joins the first one's work instead of repeating it.
///
/// Weak on purpose. A strong map would keep every image the load ever touched
/// resident for the life of the process — a gigabyte of it — to save work that
/// only overlapping plans can save. An entry lives exactly as long as some
/// worker or image still holds the payload, which is the window in which
/// sharing is worth anything.
type Prepared = (std::sync::Mutex<PreparedPayloads>, std::sync::Condvar);

#[derive(Default)]
struct PreparedPayloads {
    live: HashMap<u64, std::sync::Weak<PreparedPayload>>,
    decoding: HashSet<u64>,
}

fn prepared() -> &'static Prepared {
    static PREPARED: std::sync::OnceLock<Prepared> = std::sync::OnceLock::new();
    PREPARED.get_or_init(Default::default)
}

static SHARED_PAYLOADS: AtomicU64 = AtomicU64::new(0);
static SHARED_BYTES: AtomicU64 = AtomicU64::new(0);
static WRAPPED_BYTES: AtomicU64 = AtomicU64::new(0);
static MOVED_BYTES: AtomicU64 = AtomicU64::new(0);

/// Payloads handed to a second asker instead of decoded again, and the decoded
/// bytes that saved.
pub fn shared_variant_census() -> (u64, u64) {
    (
        SHARED_PAYLOADS.load(Ordering::Relaxed),
        SHARED_BYTES.load(Ordering::Relaxed),
    )
}

/// Bytes copied out of a payload into an image, and bytes handed over without
/// a copy.
///
/// What the wrapping costs, against what `shared_variant_census` says sharing
/// saved: an `Image` owns its data, so an asker that is *not* the last holder
/// of the payload pays one `memcpy` of the chain. The last one does not — the
/// decode's own buffer becomes the image's — and the split between the two is
/// what says whether the copying in this path is duplicate work or the first
/// materialization of bytes nobody had yet.
pub fn shared_payload_copy_cost() -> (u64, u64) {
    (
        WRAPPED_BYTES.load(Ordering::Relaxed),
        MOVED_BYTES.load(Ordering::Relaxed),
    )
}

/// Prepare the texels behind `payload` once, however many plans ask for them.
///
/// The first asker decodes; the rest wait for it and take the buffer. Waiting
/// here cannot deadlock: a worker that holds a payload is decoding, never
/// waiting on another one, so the set of holders always drains.
///
/// The key is the payload half of the variant — the resolved archive entries
/// and the map type — and not the whole of it. The colour space and the
/// sampler decide the `Image` built over these bytes; they decide nothing
/// about the bytes, so they must not decide whether the bytes are read again.
fn share_or_decode(
    payload: u64,
    decode: impl FnOnce() -> Result<PreparedPayload, ImageOutcome>,
) -> Result<(Arc<PreparedPayload>, bool), ImageOutcome> {
    let (lock, signal) = prepared();
    let mut state = lock.lock().unwrap_or_else(|poison| poison.into_inner());
    loop {
        if let Some(live) = state.live.get(&payload) {
            if let Some(prepared) = live.upgrade() {
                SHARED_PAYLOADS.fetch_add(1, Ordering::Relaxed);
                SHARED_BYTES.fetch_add(prepared.bytes(), Ordering::Relaxed);
                return Ok((prepared, true));
            }
            state.live.remove(&payload);
        }
        if !state.decoding.contains(&payload) {
            break;
        }
        state = signal
            .wait(state)
            .unwrap_or_else(|poison| poison.into_inner());
    }
    state.decoding.insert(payload);
    drop(state);

    // Held across the decode so that a panic in it wakes the askers waiting on
    // this payload instead of parking them for the life of the process. It is
    // dropped *after* the result is published, so a waiter that wakes finds the
    // payload rather than an empty slot it would decode again.
    let flight = Flight(payload);
    let prepared = decode().map(|prepared| {
        let prepared = prepared.owned();
        lock.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .live
            .insert(payload, Arc::downgrade(&prepared));
        (prepared, false)
    });
    drop(flight);
    prepared
}

/// The claim on a payload while it is being decoded.
struct Flight(u64);

impl Drop for Flight {
    fn drop(&mut self) {
        let (lock, signal) = prepared();
        lock.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .decoding
            .remove(&self.0);
        signal.notify_all();
    }
}

/// Decode this work on the pool the caller names.
///
/// The pool is a parameter because the right one is the loader's, and
/// `asset_material` cannot ask for it: the dependency runs the other way.
/// `ComputeTaskPool` is the frame's pool, with its own thread count and its
/// own affinity; a load stage that decodes on it runs on the frame's threads
/// and competes with the frame for them.
fn decode_requests_in_parallel(
    images: &[AuthoredImage],
    index: &IwdIndex,
    work: &[ImageRequest],
    stage: &StageHandle,
    pool: &TaskPool,
) -> Vec<ImageOutcome> {
    if work.is_empty() {
        return Vec::new();
    }
    let chunk_size = work.len().div_ceil(pool.thread_num().max(1)).clamp(1, 64);

    pool.scope(|scope| {
        for chunk in work.chunks(chunk_size) {
            scope.spawn(async move {
                let mut out = Vec::with_capacity(chunk.len());
                for &(image_index, request) in chunk {
                    if stage.is_canceled() {
                        out.push(ImageOutcome::Missing {
                            gap: "image decode canceled: load retargeted".to_owned(),
                        });
                        continue;
                    }
                    let outcome = decode_one_request(images, index, image_index, request);
                    // What this decode actually produced. Texels that came out
                    // of another asker's decode are that asker's weight, and
                    // counting them here would bill one buffer twice.
                    if let ImageOutcome::Decoded {
                        payload,
                        shared: false,
                        ..
                    } = &outcome
                    {
                        stage.add_bytes(payload.bytes());
                    }
                    out.push(outcome);
                    stage.advance(1);
                }
                out
            });
        }
    })
    .into_iter()
    .flatten()
    .collect()
}

/// What a decode reads and what is built around the result, as one identity.
///
/// The payload half is the ordered list of archive entries the decode would
/// try, by CRC, size and name — not the image's name, which three games spell
/// the same and fill differently — plus the map type, because a cubemap and a
/// 2D image read the same entry into different bytes.
///
/// The usage half is the rest, and none of it reaches `image.data`: a colour
/// space picks `Rgba8Unorm` over `Rgba8UnormSrgb` for the same texels, and a
/// sampler is not a property of the image at all. Keeping it in the identity
/// is what lets the merge tell "the same texels, wanted twice" from "another
/// game shipped a different image under this name".
fn variant_of(
    index: &IwdIndex,
    source: &AuthoredImage,
    name: &str,
    (sampler_state, is_normal, alpha_test_color, force_linear): (u8, bool, bool, bool),
) -> Option<ImageVariantId> {
    let candidates = index.0.image_candidates(name)?;
    let mut payload = crate::fnv1a64(name.as_bytes());
    for candidate in candidates {
        payload = crate::fnv1a64_more(payload, &candidate.crc32().to_le_bytes());
        payload = crate::fnv1a64_more(payload, &candidate.size().to_le_bytes());
        payload = crate::fnv1a64_more(payload, candidate.entry().as_bytes());
    }
    payload = crate::fnv1a64_more(payload, &[source.map_type]);
    let usage = u32::from(sampler_state)
        | u32::from(is_normal) << 8
        | u32::from(alpha_test_color) << 9
        | u32::from(force_linear) << 10
        | u32::from(source.use_srgb_reads) << 11;
    Some(ImageVariantId { payload, usage })
}

fn decode_one_request(
    images: &[AuthoredImage],
    index: &IwdIndex,
    image_index: usize,
    request: (u8, bool, bool, bool),
) -> ImageOutcome {
    let Some(source) = images.get(image_index) else {
        return ImageOutcome::Missing {
            gap: format!("catalog image index {image_index} is out of bounds"),
        };
    };
    let wrap = WrapRecipe::of(source, request);
    if source.payload.is_empty() {
        let name = crate::AssetRef::bare_name(source.name.as_str());
        if let Some(variant) = variant_of(index, source, name, request) {
            return match share_or_decode(variant.payload, || prepare_payload(index, source, name)) {
                Ok((payload, shared)) => ImageOutcome::Decoded {
                    image_index,
                    payload,
                    wrap,
                    variant: Some(variant),
                    shared,
                },
                Err(gap) => gap,
            };
        }
    }
    match prepare_unshared(index, source) {
        Ok(payload) => ImageOutcome::Decoded {
            image_index,
            payload: payload.owned(),
            wrap,
            variant: None,
            shared: false,
        },
        Err(gap) => gap,
    }
}

/// A payload nothing else can be asking for: an image whose body ships inside
/// the zone, or a name the archives do not answer for at all.
fn prepare_unshared(
    index: &IwdIndex,
    source: &AuthoredImage,
) -> Result<PreparedPayload, ImageOutcome> {
    if source.payload.is_empty() {
        return prepare_payload(
            index,
            source,
            crate::AssetRef::bare_name(source.name.as_str()),
        );
    }
    decode_gfx_image(
        &source.payload,
        u32::from(source.width),
        u32::from(source.height),
        source.format,
    )
    .map(PreparedPayload::mips)
    .map_err(|error| ImageOutcome::Unsupported {
        gap: format!("{}: {error}", source.name),
    })
}

/// Read and decode one name out of the archives, with nothing wrapped around
/// the result.
///
/// Everything here is a function of the resolved archive entry and the map
/// type — which is exactly what the sharing key is, so two askers that differ
/// only in how they will use the texels run this once.
fn prepare_payload(
    index: &IwdIndex,
    source: &AuthoredImage,
    name: &str,
) -> Result<PreparedPayload, ImageOutcome> {
    if name.starts_with('$') {
        return Err(ImageOutcome::Missing {
            gap: format!(
                "{} is a builtin alias with empty payload; body ships in code_post_gfx_mp",
                source.name
            ),
        });
    }
    if let Some(cubemap) = index.decode_cubemap_if_skybox(name, source.map_type) {
        return match cubemap {
            Ok((size, faces)) => Ok(PreparedPayload::Cubemap {
                size,
                faces: Box::new(faces),
            }),
            Err(error) => Err(ImageOutcome::Unsupported {
                gap: format!("{}: {error}", source.name),
            }),
        };
    }
    match index.decode(name) {
        Some(Ok(mips)) => Ok(PreparedPayload::mips(mips)),
        Some(Err(error)) => Err(ImageOutcome::Unsupported {
            gap: format!("{}: {error}", source.name),
        }),
        None => Err(ImageOutcome::Missing {
            gap: format!("{} is absent from IWD", source.name),
        }),
    }
}

fn wrap_payload(payload: &PreparedPayload, wrap: WrapRecipe) -> Arc<Image> {
    match payload {
        PreparedPayload::Cubemap { size, faces } => Arc::new(pack_material_cubemap(
            *size,
            faces,
            wrap.use_srgb_reads && !wrap.force_linear,
        )),
        PreparedPayload::Mips(state) => {
            let mut state = state.lock().expect("prepared mip cache is not poisoned");
            if let Some((_, image)) = state.images.iter().find(|(recipe, _)| *recipe == wrap) {
                return Arc::clone(image);
            }
            if state.images.is_empty() {
                // Keep the residency charge while the cached image owns these bytes.
                let packed = std::mem::take(&mut state.mips.packed);
                MOVED_BYTES.fetch_add(packed.len() as u64, Ordering::Relaxed);
                let image = Arc::new(wrap_mips(&state.mips, packed, wrap));
                state.images.push((wrap, Arc::clone(&image)));
                return image;
            }
            let data = state.images[0]
                .1
                .data
                .as_ref()
                .expect("prepared image owns its payload")
                .clone();
            WRAPPED_BYTES.fetch_add(data.len() as u64, Ordering::Relaxed);
            let image = Arc::new(wrap_mips(&state.mips, data, wrap));
            image
        }
    }
}

/// The image `mips` describes, over `data` — copied or moved, whichever the
/// caller could afford.
fn wrap_mips(mips: &DecodedMips, data: Vec<u8>, wrap: WrapRecipe) -> Image {
    let format = DecodedMips::texture_format(mips.storage, wrap.linear());
    let levels = mips.level_count();
    let mut image = Image::new_uninit(
        Extent3d {
            width: mips.width,
            height: mips.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = levels;
    image.data = Some(data);
    image.sampler = ImageSampler::Descriptor(sampler_from_iw4(
        wrap.sampler_state,
        levels,
        wrap.alpha_test_color,
    ));
    image
}

pub fn decode_reflection_probe_cubemap(source: &AuthoredImage) -> Result<Image, String> {
    if source.map_type != 5 || source.width == 0 || source.width != source.height {
        return Err(format!("{} is not a square cubemap", source.name));
    }
    let pixel_format = match source.format {
        21 => PixelFormat::Bgra8,
        value if value == u32::from_le_bytes(*b"DXT1") => PixelFormat::Bc1,
        value if value == u32::from_le_bytes(*b"DXT5") => PixelFormat::Bc3,
        _ => {
            return Err(format!(
                "{} has unsupported probe format {}",
                source.name, source.format
            ));
        }
    };
    let size = u32::from(source.width);
    let levels = size.ilog2() + 1;
    let source_level_bytes = (0..levels)
        .map(|level| {
            compressed_mip_bytes((size >> level).max(1), (size >> level).max(1), pixel_format)
        })
        .collect::<Vec<_>>();
    let face_bytes = source_level_bytes.iter().sum::<usize>();
    let needed = face_bytes * 6;
    if source.payload.len() != needed {
        return Err(format!(
            "{} probe payload: need {needed} bytes for six {size}px mip chains, have {}",
            source.name,
            source.payload.len()
        ));
    }
    let rgba_len = (0..levels)
        .map(|level| {
            let side = (size >> level).max(1);
            (side * side * 4 * 6) as usize
        })
        .sum();
    let mut data = Vec::with_capacity(rgba_len);
    let mut level_offset = 0;
    for level in 0..levels {
        let side = (size >> level).max(1);
        let level_bytes = source_level_bytes[level as usize];
        for face in 0..6 {
            let start = face * face_bytes + level_offset;
            let encoded = &source.payload[start..start + level_bytes];
            match pixel_format {
                PixelFormat::Bgra8 => {
                    for pixel in encoded.chunks_exact(4) {
                        data.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                    }
                }
                PixelFormat::Bc1 | PixelFormat::Bc3 => {
                    data.extend_from_slice(&decode_blocks(encoded, side, side, pixel_format)?);
                }
                _ => unreachable!(),
            }
        }
        level_offset += level_bytes;
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = levels;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image.data = Some(data);
    Ok(image)
}

pub fn sampler_from_iw4(
    sampler_state: u8,
    levels: u32,
    alpha_test_color: bool,
) -> ImageSamplerDescriptor {
    let clamp_u = (sampler_state >> 5) & 1 != 0;
    let clamp_v = (sampler_state >> 6) & 1 != 0;
    let clamp_w = (sampler_state >> 7) & 1 != 0;
    let filter = sampler_state & 0b111;
    let mip_map = (sampler_state >> 3) & 0b11;
    let anisotropy = match filter {
        _ if levels <= 1 || alpha_test_color => 1,
        3 => 2,
        4 => 4,
        _ => 1,
    };
    let point = if alpha_test_color {
        ImageFilterMode::Nearest
    } else {
        ImageFilterMode::Linear
    };
    ImageSamplerDescriptor {
        address_mode_u: if clamp_u {
            ImageAddressMode::ClampToEdge
        } else {
            ImageAddressMode::Repeat
        },
        address_mode_v: if clamp_v {
            ImageAddressMode::ClampToEdge
        } else {
            ImageAddressMode::Repeat
        },
        address_mode_w: if clamp_w {
            ImageAddressMode::ClampToEdge
        } else {
            ImageAddressMode::Repeat
        },
        mag_filter: point,
        min_filter: point,
        mipmap_filter: if alpha_test_color {
            ImageFilterMode::Nearest
        } else if mip_map == 2 || anisotropy > 1 {
            ImageFilterMode::Linear
        } else {
            ImageFilterMode::Nearest
        },
        anisotropy_clamp: anisotropy,
        ..Default::default()
    }
}

fn expand_top_level(decoded: Result<DecodedMips, String>) -> Result<(u32, u32, Vec<u8>), String> {
    decoded.and_then(DecodedMips::into_top_level_rgba8)
}

pub fn decode_menu_background(games_root: &Path) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    for main in ui_decode_mains(games_root) {
        let index = IwdIndex::open(&main)?;
        match index.decode("menu_mp_image").map(expand_top_level) {
            Some(Ok(image)) => return Ok(Some(image)),
            Some(Err(error)) => {
                diag::info!(
                    Zone,
                    "menu: decode menu_mp_image from {}: {error}",
                    main.display()
                );
            }
            None => {}
        }
    }
    Ok(None)
}

pub fn decode_zone_image_rgba(
    width: u32,
    height: u32,
    format: u32,
    payload: &[u8],
) -> Result<(u32, u32, Vec<u8>), String> {
    if payload.is_empty() {
        return Err("empty in-zone payload".into());
    }
    decode_gfx_image(payload, width, height, format)?.into_top_level_rgba8()
}

pub type ZoneUiRgba = (u32, u32, Arc<Vec<u8>>);

static ZONE_UI_IMAGES: RwLock<Vec<(ZoneUiKey, Arc<[u8]>)>> = RwLock::new(Vec::new());

type ZoneUiKey = (asset_core::AssetNamespace, String);

pub fn store_zone_ui_images(
    namespace: asset_core::AssetNamespace,
    images: impl IntoIterator<Item = (String, Arc<[u8]>)>,
) {
    let mut store = ZONE_UI_IMAGES
        .write()
        .unwrap_or_else(|poison| poison.into_inner());
    store.retain(|((ns, _), _)| *ns != namespace);
    store.extend(
        images
            .into_iter()
            .map(|(name, image)| ((namespace, name.to_ascii_lowercase()), image)),
    );
}

fn zone_ui_iwi(namespace: asset_core::AssetNamespace, material: &str) -> Option<Arc<[u8]>> {
    let name = crate::AssetRef::bare_name(material).to_ascii_lowercase();
    ZONE_UI_IMAGES
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .iter()
        .find(|((ns, stored), _)| *ns == namespace && *stored == name)
        .map(|(_, iwi)| Arc::clone(iwi))
}

pub fn has_zone_ui_image(namespace: asset_core::AssetNamespace, material: &str) -> bool {
    zone_ui_iwi(namespace, material).is_some()
}

pub fn zone_ui_image(namespace: asset_core::AssetNamespace, material: &str) -> Option<ZoneUiRgba> {
    let iwi = zone_ui_iwi(namespace, material)?;
    match decode_iwi_rgba(&iwi) {
        Ok((width, height, rgba)) => Some((width, height, Arc::new(rgba))),
        Err(error) => {
            diag::warn!(Zone, "zone UI image {material}: {error}");
            None
        }
    }
}

pub fn decode_iwi_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    expand_top_level(decode_iwi_mips(bytes))
}

pub fn decode_ui_image(
    games_root: &Path,
    image_name: &str,
) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    if image_name.contains(':') {
        let key = asset_core::AssetKey::parse(image_name).map_err(|e| e.to_string())?;
        if key.kind != asset_core::AssetKind::Material {
            return Err("UI image key must name a material".into());
        }
        if let Some((width, height, rgba)) = zone_ui_image(key.namespace, &key.name) {
            return Ok(Some((width, height, rgba.as_ref().clone())));
        }
        let trees = asset_transport::NamespaceTrees::discover(&asset_transport::GamesRoot(
            games_root.to_owned(),
        ));
        let Some(main) = trees.main_for(key.namespace) else {
            return Ok(None);
        };
        return decode_ui_image_from_main(main, &key.name);
    }
    let name = crate::AssetRef::bare_name(image_name);
    for main in ui_decode_mains(games_root) {
        match decode_ui_image_from_main(&main, name)? {
            Some(image) => return Ok(Some(image)),
            None => {}
        }
    }
    Ok(None)
}

pub fn decode_ui_image_from_main(
    main: &Path,
    image_name: &str,
) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    let name = crate::AssetRef::bare_name(image_name);
    let index = IwdIndex::open(main)?;
    let fallback = name
        .strip_prefix("preview_")
        .map(|map| format!("loadscreen_{map}"))
        .or_else(|| {
            name.strip_prefix("menu_mp_map_select_")
                .and_then(|map| map.strip_suffix("_big"))
                .map(|map| format!("loadscreen_mp_{map}"))
        });
    for candidate in std::iter::once(name).chain(fallback.as_deref()) {
        match index.decode(candidate).map(expand_top_level) {
            Some(Ok(image)) => return Ok(Some(image)),
            Some(Err(error)) => {
                diag::warn!(
                    Zone,
                    "ui-image: decode {candidate} from {}: {error}",
                    main.display()
                );
            }
            None => {}
        }
    }
    Ok(None)
}

fn ui_decode_mains(games_root: &Path) -> Vec<PathBuf> {
    let mut mains: Vec<PathBuf> = Vec::new();
    for mut group in asset_transport::game_mains_by_root(games_root) {
        group.sort_by_key(|main| !IwdIndex::is_cached(main));
        for main in group {
            if !mains.contains(&main) {
                mains.push(main);
            }
        }
    }
    mains
}

pub fn decode_map_preview(
    zone_ff: &Path,
    map_name: &str,
) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    if asset_transport::zone_game_for_path(zone_ff) == Some(asset_core::ZoneGame::T6) {
        return Ok(zone_ui_image(
            asset_core::AssetNamespace::T6,
            &format!("loadscreen_{map_name}"),
        )
        .or_else(|| {
            zone_ui_image(
                asset_core::AssetNamespace::T6,
                &format!("menu_{map_name}_map_select_final"),
            )
        })
        .map(|(width, height, rgba)| (width, height, rgba.as_ref().clone())));
    }
    let main = game_main_for_zone(zone_ff)?;
    let index = IwdIndex::open(&main)?;
    for stem in [
        format!("loadscreen_{map_name}"),
        format!("preview_{map_name}"),
    ] {
        match index.decode(&stem).map(expand_top_level) {
            Some(Ok(image)) => return Ok(Some(image)),
            Some(Err(error)) => {
                diag::warn!(Zone, "loading: decode {stem}: {error}");
            }
            None => {}
        }
    }
    Ok(None)
}

pub fn game_main_for_zone(zone_ff: &Path) -> Result<PathBuf, String> {
    asset_transport::game_main_for_zone(zone_ff)
}

const MIP_CACHE_FORMAT: u32 = 2;
const MIP_MAGIC: &[u8; 8] = b"IWL1MIPS";
static MIP_HIT: AtomicU64 = AtomicU64::new(0);
static MIP_MISS: AtomicU64 = AtomicU64::new(0);
static MIP_IO_NS: AtomicU64 = AtomicU64::new(0);

pub fn mip_cache_cost() -> (u64, u64, f64) {
    (
        MIP_HIT.load(Ordering::Relaxed),
        MIP_MISS.load(Ordering::Relaxed),
        MIP_IO_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ImageWorkingSet {
    pub decoded_n: u64,
    pub decoded_bytes: u64,
    pub world_n: u64,
    pub world_bytes: u64,
    pub smodel_n: u64,
    pub smodel_bytes: u64,
    pub fpv_n: u64,
    pub fpv_bytes: u64,
    pub probe_n: u64,
    pub probe_bytes: u64,
    pub lightmap_n: u64,
    pub lightmap_bytes: u64,
    pub fx_n: u64,
    pub fx_bytes: u64,
}

static LAST_WORKING_SET: RwLock<ImageWorkingSet> = RwLock::new(ImageWorkingSet {
    decoded_n: 0,
    decoded_bytes: 0,
    world_n: 0,
    world_bytes: 0,
    smodel_n: 0,
    smodel_bytes: 0,
    fpv_n: 0,
    fpv_bytes: 0,
    probe_n: 0,
    probe_bytes: 0,
    lightmap_n: 0,
    lightmap_bytes: 0,
    fx_n: 0,
    fx_bytes: 0,
});

pub fn last_image_working_set() -> ImageWorkingSet {
    *LAST_WORKING_SET
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn store_image_working_set(set: ImageWorkingSet) {
    *LAST_WORKING_SET
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = set;
}

pub fn cpu_image_census<'a>(images: impl IntoIterator<Item = &'a Image>) -> (u64, u64) {
    let mut n = 0u64;
    let mut bytes = 0u64;
    for image in images {
        n = n.saturating_add(1);
        bytes = bytes.saturating_add(
            image
                .data
                .as_ref()
                .map(|data| data.len() as u64)
                .unwrap_or(0),
        );
    }
    (n, bytes)
}

pub fn census_image_working_set(
    catalog: &MaterialDefinitions,
    world_materials: impl IntoIterator<Item = usize>,
    smodel_materials: impl IntoIterator<Item = usize>,
    fpv_materials: impl IntoIterator<Item = usize>,
    fx_materials: impl IntoIterator<Item = usize>,
) -> ImageWorkingSet {
    fn bound_images(
        catalog: &MaterialDefinitions,
        mats: impl IntoIterator<Item = usize>,
    ) -> (u64, u64) {
        let mut seen = HashSet::new();
        for mi in mats {
            let Some(material) = catalog.materials.get(mi) else {
                continue;
            };
            for texture in &material.textures {
                if let Some(image) = texture.image {
                    seen.insert(image);
                }
            }
        }
        let mut bytes = 0u64;
        for image in &seen {
            if let Some(authored) = catalog.images.get(*image)
                && let Some(decoded) = &authored.decoded
            {
                bytes += decoded
                    .data
                    .as_ref()
                    .map(|data| data.len() as u64)
                    .unwrap_or(0);
            }
        }
        (seen.len() as u64, bytes)
    }
    let mem = catalog.image_memory();
    let (world_n, world_bytes) = bound_images(catalog, world_materials);
    let (smodel_n, smodel_bytes) = bound_images(catalog, smodel_materials);
    let (fpv_n, fpv_bytes) = bound_images(catalog, fpv_materials);
    let (fx_n, fx_bytes) = bound_images(catalog, fx_materials);
    ImageWorkingSet {
        decoded_n: mem.decoded_images as u64,
        decoded_bytes: mem.decoded_bytes as u64,
        world_n,
        world_bytes,
        smodel_n,
        smodel_bytes,
        fpv_n,
        fpv_bytes,
        probe_n: 0,
        probe_bytes: 0,
        lightmap_n: 0,
        lightmap_bytes: 0,
        fx_n,
        fx_bytes,
    }
}

fn read_cached_mips(key: &str, io_at: std::time::Instant) -> Option<DecodedMips> {
    let hit = crate::cache_get("mips", key)?;
    MIP_IO_NS.fetch_add(io_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
    let mips = DecodedMips::cache_decode(&hit)?;
    MIP_HIT.fetch_add(1, Ordering::Relaxed);
    Some(mips)
}

fn mip_cache_key(crc: u32, size: u64, entry: &str) -> String {
    format!(
        "{MIP_CACHE_FORMAT:08x}-{crc:08x}-{size:x}-{:016x}",
        crate::fnv1a64(entry.as_bytes())
    )
}

fn load_or_decode_mips(candidate: &asset_transport::IwdFile) -> Result<DecodedMips, String> {
    let key = mip_cache_key(candidate.crc32(), candidate.size(), candidate.entry());
    let io_at = std::time::Instant::now();
    if let Some(mips) = read_cached_mips(&key, io_at) {
        return Ok(mips);
    }

    let _flight = crate::cache_flight("mips", &key);
    if let Some(mips) = read_cached_mips(&key, std::time::Instant::now()) {
        return Ok(mips);
    }
    let bytes = candidate.read()?;
    let mips = decode_iwi_mips(&bytes)?;
    let blob = mips.cache_encode();
    let store_at = std::time::Instant::now();
    if let Err(error) = crate::cache_put("mips", &key, &blob) {
        diag::warn!(Zone, "mip cache store {key}: {error}");
    }
    MIP_IO_NS.fetch_add(store_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
    MIP_MISS.fetch_add(1, Ordering::Relaxed);
    Ok(mips)
}

pub fn iwd_read_cost() -> (f64, u64, f64) {
    asset_transport::iwd_read_cost()
}

/// Archive entries inflated whole, and entries inflated only far enough to
/// read the thirty-two byte IWI header.
pub fn iwd_entry_reads() -> (u64, u64) {
    asset_transport::iwd_entry_reads()
}

fn decode_iwi_mips(bytes: &[u8]) -> Result<DecodedMips, String> {
    decode_iwi_mips_with(bytes, false)
}

fn decode_iwi_mips_with(bytes: &[u8], keep_bc5: bool) -> Result<DecodedMips, String> {
    if bytes.len() >= IWI_V8_HEADER_LEN
        && bytes[..3] == *b"IWi"
        && bytes[3] == 8
        && let Ok(header) = IwiHeader::parse(bytes)
        && img_format_info(header.format).is_some_and(|info| info.kind == ImgFormatKind::Wavelet)
    {
        return decode_wavelet_iwi(bytes, header);
    }
    let header = parse_iwi_header(bytes)?;
    let end = header.mip0_end.min(bytes.len());
    let start = if (header.header_len..end).contains(&header.mip0_start) {
        header.mip0_start
    } else {
        header.header_len
    };
    let payload = bytes
        .get(start..end)
        .ok_or_else(|| "truncated IWI top mip".to_owned())?;
    let storage = match header.format {
        PixelFormat::Bc5 if keep_bc5 => MipStorage::Bc5,
        format => mip_storage(format),
    };

    let single = || -> Result<DecodedMips, String> {
        let (w, h, pixels) = load_mip_level_with(
            payload,
            header.width,
            header.height,
            header.format,
            keep_bc5,
        )?;
        Ok(match storage {
            MipStorage::Rgba8 => DecodedMips::single(w, h, pixels),
            other => DecodedMips::single_compressed(w, h, other, pixels),
        })
    };
    let count = mip_level_count(header.width, header.height);
    if count <= 1 {
        return single();
    }

    let mut cursor = if header.mip0_end <= bytes.len() {
        header.mip0_end
    } else {
        bytes.len()
    };
    let mut ranges = Vec::with_capacity(count as usize);
    for level in 0..count {
        let (w, h) = (
            (header.width >> level).max(1),
            (header.height >> level).max(1),
        );
        let size = compressed_mip_bytes(w, h, header.format);
        let Some(level_start) = cursor.checked_sub(size) else {
            return single();
        };
        ranges.push((level_start, cursor, w, h));
        cursor = level_start;
    }
    if cursor != header.header_len {
        return single();
    }

    let mut levels = Vec::with_capacity(ranges.len());
    for (level_start, level_end, w, h) in ranges {
        match load_mip_level_with(
            &bytes[level_start..level_end],
            w,
            h,
            header.format,
            keep_bc5,
        ) {
            Ok((_, _, pixels)) => levels.push(pixels),
            Err(_) => return single(),
        }
    }
    Ok(DecodedMips::from_levels(
        header.width,
        header.height,
        storage,
        levels,
    ))
}

fn decode_wavelet_iwi(bytes: &[u8], header: IwiHeader) -> Result<DecodedMips, String> {
    let format = wavelet_check_header(&header).map_err(wavelet_error)?;
    let info = img_format_info(format).ok_or_else(|| format!("unsupported IWI format {format}"))?;
    let channels = info.channels;
    let stride = wavelet_pixel_stride(channels);
    let pixel_format = wavelet_d3d_pixel_format(format)?;
    let payload = bytes
        .get(IWI_V8_HEADER_LEN..)
        .ok_or_else(|| "truncated wavelet IWI payload".to_owned())?;
    let mut bits = WaveletBits::new(payload);
    let mut parent = Vec::new();
    let mut d3d_levels = Vec::new();
    let width = u32::from(header.width);
    let height = u32::from(header.height);
    for level in (0..=wavelet_top_level(&header)).rev() {
        let w = wavelet_level_size(width, level);
        let h = wavelet_level_size(height, level);
        let mut plane = vec![0u8; w as usize * h as usize * stride];
        wavelet_decompress_level(&mut bits, &mut parent, &mut plane, w, h, channels, stride)
            .map_err(wavelet_error)?;
        d3d_levels.push(plane.clone());
        parent = plane;
    }
    bits.check_landing().map_err(wavelet_error)?;
    d3d_levels.reverse();
    let mut levels = Vec::with_capacity(d3d_levels.len());
    for (i, d3d) in d3d_levels.iter().enumerate() {
        let w = (width >> i).max(1);
        let h = (height >> i).max(1);
        let (_, _, rgba) = load_mip_level(d3d, w, h, pixel_format)?;
        levels.push(rgba);
    }
    Ok(DecodedMips::from_levels(
        width,
        height,
        MipStorage::Rgba8,
        levels,
    ))
}

fn wavelet_d3d_pixel_format(format: u8) -> Result<PixelFormat, String> {
    match format {
        6 => Ok(PixelFormat::Bgra8),
        7 => Ok(PixelFormat::Bgrx8),
        8 => Ok(PixelFormat::La8),
        9 => Ok(PixelFormat::L8),
        10 => Ok(PixelFormat::A8),
        other => Err(format!("unsupported IWI format {other}")),
    }
}

fn wavelet_error(error: WaveletError) -> String {
    match error {
        WaveletError::Truncated { needed, have } => {
            format!("wavelet truncated: needed {needed} have {have}")
        }
        WaveletError::Dimensions => "wavelet level is not even 2D".into(),
        WaveletError::UnsupportedFormat(format) => format!("unsupported IWI format {format}"),
        WaveletError::CubemapOrVolume => "wavelet cubemap/volume is unlocated".into(),
        WaveletError::BadLanding { pos, end } => {
            format!("wavelet cursor landed at {pos}, payload ends at {end}")
        }
    }
}

fn mip_storage(format: PixelFormat) -> MipStorage {
    match format {
        PixelFormat::Bc1 => MipStorage::Bc1,
        PixelFormat::Bc2 => MipStorage::Bc2,
        PixelFormat::Bc3 | PixelFormat::Bc5 => MipStorage::Bc3,
        _ => MipStorage::Rgba8,
    }
}

fn load_mip_level(
    data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<(u32, u32, Vec<u8>), String> {
    load_mip_level_with(data, width, height, format, false)
}

fn load_mip_level_with(
    data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
    keep_bc5: bool,
) -> Result<(u32, u32, Vec<u8>), String> {
    match format {
        PixelFormat::Bc5 if keep_bc5 => {
            let needed = compressed_mip_bytes(width, height, format);
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated BC5 image".to_owned())?;
            Ok((width, height, source.to_vec()))
        }
        PixelFormat::Bc1 | PixelFormat::Bc2 | PixelFormat::Bc3 => {
            let needed = compressed_mip_bytes(width, height, format);
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated BC image".to_owned())?;
            Ok((width, height, source.to_vec()))
        }
        PixelFormat::Bc5 => {
            let needed = compressed_mip_bytes(width, height, format);
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated BC5 image".to_owned())?;
            Ok((width, height, bc5_to_dxt5nm(source)))
        }
        _ => decode_pixels(data, width, height, format),
    }
}

struct IwiHeaderInfo {
    width: u32,
    height: u32,
    format: PixelFormat,
    header_len: usize,

    mip0_end: usize,

    mip0_start: usize,
}

fn parse_iwi_header(bytes: &[u8]) -> Result<IwiHeaderInfo, String> {
    if bytes.len() < 4 || &bytes[..3] != b"IWi" {
        return Err("unsupported IWI header".into());
    }
    match bytes[3] {
        8 => {
            if bytes.len() < 32 {
                return Err("truncated IWI v8 header".into());
            }
            let width = u32::from(u16::from_le_bytes([bytes[10], bytes[11]]));
            let height = u32::from(u16::from_le_bytes([bytes[12], bytes[13]]));
            let mip0_end = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
            let mip0_start = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
            Ok(IwiHeaderInfo {
                width,
                height,
                format: iwi_pixel_format(bytes[8])?,
                header_len: 32,
                mip0_end,
                mip0_start,
            })
        }

        13 => {
            if bytes.len() < 48 {
                return Err("truncated IWI v13 header".into());
            }
            let width = u32::from(u16::from_le_bytes([bytes[6], bytes[7]]));
            let height = u32::from(u16::from_le_bytes([bytes[8], bytes[9]]));
            let mip0_end = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;

            let mip0_start = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
            Ok(IwiHeaderInfo {
                width,
                height,
                format: iwi_pixel_format(bytes[4])?,
                header_len: 48,
                mip0_end,
                mip0_start,
            })
        }
        27 => {
            if bytes.len() < 64 {
                return Err("truncated IWI v27 header".into());
            }
            let width = u32::from(u16::from_le_bytes([bytes[6], bytes[7]]));
            let height = u32::from(u16::from_le_bytes([bytes[8], bytes[9]]));
            let mip0_end = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
            let mip0_start = u32::from_le_bytes(bytes[36..40].try_into().unwrap()) as usize;
            Ok(IwiHeaderInfo {
                width,
                height,
                format: iwi_pixel_format(bytes[4])?,
                header_len: 64,
                mip0_end,
                mip0_start,
            })
        }
        version => Err(format!("unsupported IWI version {version}")),
    }
}

pub fn decode_iwi_texture(
    bytes: &[u8],
    sampler_state: u8,
    is_normal: bool,
    use_srgb_reads: bool,
) -> Result<Image, String> {
    let mips = decode_iwi_mips(bytes)?;
    let wrap = WrapRecipe {
        sampler_state,
        is_normal,
        alpha_test_color: false,
        force_linear: false,
        use_srgb_reads,
    };
    let data = mips.packed.clone();
    Ok(wrap_mips(&mips, data, wrap))
}

pub fn decode_iwi_texture_native(
    bytes: &[u8],
    sampler_state: u8,
    use_srgb_reads: bool,
) -> Result<Image, String> {
    let mips = decode_iwi_mips_with(bytes, true)?;
    let wrap = WrapRecipe {
        sampler_state,
        is_normal: false,
        alpha_test_color: false,
        force_linear: !use_srgb_reads,
        use_srgb_reads,
    };
    let data = mips.packed.clone();
    Ok(wrap_mips(&mips, data, wrap))
}

pub fn with_opaque_alpha(image: &Image) -> Image {
    let mut image = image.clone();
    let format = image.texture_descriptor.format;
    let Some(data) = image.data.as_mut() else {
        return image;
    };
    match format {
        TextureFormat::Bc2RgbaUnorm | TextureFormat::Bc2RgbaUnormSrgb => {
            for block in data.as_chunks_mut::<16>().0 {
                block[..8].fill(0xff);
            }
        }
        TextureFormat::Bc3RgbaUnorm | TextureFormat::Bc3RgbaUnormSrgb => {
            for block in data.as_chunks_mut::<16>().0 {
                block[..2].fill(0xff);
                block[2..8].fill(0);
            }
        }
        TextureFormat::Rgba8Unorm
        | TextureFormat::Rgba8UnormSrgb
        | TextureFormat::Bgra8Unorm
        | TextureFormat::Bgra8UnormSrgb => {
            for texel in data.as_chunks_mut::<4>().0 {
                texel[3] = 0xff;
            }
        }
        _ => {}
    }
    image
}

pub fn solid_texture(rgba: [u8; 4], srgb: bool) -> Image {
    let mips = DecodedMips::single(1, 1, rgba.to_vec());
    let wrap = WrapRecipe {
        sampler_state: 0,
        is_normal: false,
        alpha_test_color: false,
        force_linear: !srgb,
        use_srgb_reads: srgb,
    };
    wrap_mips(&mips, rgba.to_vec(), wrap)
}

fn iwi_pixel_format(format: u8) -> Result<PixelFormat, String> {
    match format {
        1 => Ok(PixelFormat::Bgra8),
        2 => Ok(PixelFormat::Rgb8),
        3 => Ok(PixelFormat::La8),
        4 => Ok(PixelFormat::L8),
        5 => Ok(PixelFormat::A8),
        11 => Ok(PixelFormat::Bc1),
        12 => Ok(PixelFormat::Bc2),
        13 => Ok(PixelFormat::Bc3),
        14 => Ok(PixelFormat::Bc5),
        format => Err(format!("unsupported IWI format {format}")),
    }
}

fn mip_level_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).leading_zeros()
}

fn compressed_mip_bytes(width: u32, height: u32, format: PixelFormat) -> usize {
    match format {
        PixelFormat::Bgra8 | PixelFormat::Bgrx8 => width as usize * height as usize * 4,
        PixelFormat::Rgb8 => width as usize * height as usize * 3,
        PixelFormat::La8 => width as usize * height as usize * 2,
        PixelFormat::L8 | PixelFormat::A8 => width as usize * height as usize,
        PixelFormat::Bc1 => width.div_ceil(4) as usize * height.div_ceil(4) as usize * 8,
        PixelFormat::Bc2 | PixelFormat::Bc3 | PixelFormat::Bc5 => {
            width.div_ceil(4) as usize * height.div_ceil(4) as usize * 16
        }
    }
}

fn decode_gfx_image(
    bytes: &[u8],
    width: u32,
    height: u32,
    format: u32,
) -> Result<DecodedMips, String> {
    let format = match format {
        value if value == u32::from_le_bytes(*b"DXT1") => PixelFormat::Bc1,
        value if value == u32::from_le_bytes(*b"DXT3") => PixelFormat::Bc2,
        value if value == u32::from_le_bytes(*b"DXT5") => PixelFormat::Bc3,
        21 => PixelFormat::Bgra8,
        22 => PixelFormat::Bgrx8,
        28 => PixelFormat::A8,
        50 => PixelFormat::L8,
        51 => PixelFormat::La8,
        format => return Err(format!("unsupported D3D format {format}")),
    };
    let (w, h, pixels) = load_mip_level(bytes, width, height, format)?;
    Ok(match mip_storage(format) {
        MipStorage::Rgba8 => DecodedMips::single(w, h, pixels),
        storage => DecodedMips::single_compressed(w, h, storage, pixels),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PixelFormat {
    Rgb8,
    Bgra8,
    Bgrx8,
    L8,
    La8,
    A8,
    Bc1,
    Bc2,
    Bc3,
    Bc5,
}

fn bc5_block_to_dxt5nm(src: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&src[..8]);
    let mut y = [0u8; 16];
    bcdec_rs::bc4(&src[8..16], &mut y, 4, false);
    let (lo, hi) = y
        .iter()
        .fold((u8::MAX, 0u8), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let (g0, g1) = (u16::from(hi >> 2), u16::from(lo >> 2));
    let expand = |g6: u16| f32::from(((g6 << 2) | (g6 >> 4)) as u8);
    let palette = if g0 > g1 {
        let (p0, p1) = (expand(g0), expand(g1));
        [p0, p1, (2.0 * p0 + p1) / 3.0, (p0 + 2.0 * p1) / 3.0]
    } else {
        [expand(g0); 4]
    };
    out[8..10].copy_from_slice(&(g0 << 5).to_le_bytes());
    out[10..12].copy_from_slice(&(g1 << 5).to_le_bytes());
    let mut indices = 0u32;
    for (i, &v) in y.iter().enumerate() {
        let best = (0..4)
            .min_by(|&a, &b| {
                (palette[a] - f32::from(v))
                    .abs()
                    .total_cmp(&(palette[b] - f32::from(v)).abs())
            })
            .unwrap_or(0) as u32;
        indices |= best << (2 * i);
    }
    out[12..16].copy_from_slice(&indices.to_le_bytes());
    out
}

fn bc5_to_dxt5nm(data: &[u8]) -> Vec<u8> {
    data.as_chunks::<16>()
        .0
        .iter()
        .flat_map(|block| bc5_block_to_dxt5nm(block))
        .collect()
}

fn decode_pixels(
    data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<(u32, u32, Vec<u8>), String> {
    if width == 0 || height == 0 {
        return Err("zero-sized image".into());
    }
    let pixels = match format {
        PixelFormat::Bgra8 => take_rgba(data, width, height, true, false)?,
        PixelFormat::Bgrx8 => take_rgba(data, width, height, true, true)?,
        PixelFormat::Rgb8 => {
            let needed = width as usize * height as usize * 3;
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated RGB8".to_owned())?;
            let mut out = Vec::with_capacity(width as usize * height as usize * 4);
            for pixel in source.chunks_exact(3) {
                out.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
            out
        }
        PixelFormat::L8 | PixelFormat::A8 => {
            let needed = width as usize * height as usize;
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated L8/A8".to_owned())?;
            let mut out = Vec::with_capacity(needed * 4);
            for &value in source {
                if matches!(format, PixelFormat::A8) {
                    out.extend_from_slice(&[255, 255, 255, value]);
                } else {
                    out.extend_from_slice(&[value, value, value, 255]);
                }
            }
            out
        }
        PixelFormat::La8 => {
            let needed = width as usize * height as usize * 2;
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated LA8".to_owned())?;
            let mut out = Vec::with_capacity(width as usize * height as usize * 4);
            for pixel in source.chunks_exact(2) {
                out.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
            out
        }
        PixelFormat::Bc1 | PixelFormat::Bc2 | PixelFormat::Bc3 => {
            decode_blocks(data, width, height, format)?
        }
        PixelFormat::Bc5 => decode_blocks(&bc5_to_dxt5nm(data), width, height, PixelFormat::Bc3)?,
    };
    Ok((width, height, pixels))
}

fn take_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    bgra: bool,
    opaque: bool,
) -> Result<Vec<u8>, String> {
    let needed = width as usize * height as usize * 4;
    let mut out = data
        .get(..needed)
        .ok_or_else(|| "truncated RGBA8".to_owned())?
        .to_vec();
    for pixel in out.chunks_exact_mut(4) {
        if bgra {
            pixel.swap(0, 2);
        }
        if opaque {
            pixel[3] = 255;
        }
    }
    Ok(out)
}

pub fn decoded_image_top_level_rgba8(image: &Image) -> Result<(u32, u32, Vec<u8>), String> {
    if image.texture_descriptor.dimension != TextureDimension::D2
        || image.texture_descriptor.size.depth_or_array_layers != 1
    {
        return Err("decoded image is not a single 2D layer".to_owned());
    }
    let width = image.texture_descriptor.size.width;
    let height = image.texture_descriptor.size.height;
    let data = image
        .data
        .as_deref()
        .ok_or_else(|| "decoded image has no CPU data".to_owned())?;
    let pixels = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => {
            take_rgba(data, width, height, false, false)?
        }
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
            take_rgba(data, width, height, true, false)?
        }
        TextureFormat::Bc1RgbaUnorm | TextureFormat::Bc1RgbaUnormSrgb => {
            decode_blocks(data, width, height, PixelFormat::Bc1)?
        }
        TextureFormat::Bc2RgbaUnorm | TextureFormat::Bc2RgbaUnormSrgb => {
            decode_blocks(data, width, height, PixelFormat::Bc2)?
        }
        TextureFormat::Bc3RgbaUnorm | TextureFormat::Bc3RgbaUnormSrgb => {
            decode_blocks(data, width, height, PixelFormat::Bc3)?
        }
        format => return Err(format!("unsupported decoded image format {format:?}")),
    };
    Ok((width, height, pixels))
}

fn decode_blocks(
    data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<Vec<u8>, String> {
    let block_size = if matches!(format, PixelFormat::Bc1) {
        8
    } else {
        16
    };
    let blocks_wide = width.div_ceil(4) as usize;
    let blocks_high = height.div_ceil(4) as usize;
    let needed = blocks_wide * blocks_high * block_size;
    if data.len() < needed {
        return Err("truncated BC image".into());
    }
    let mut out = vec![0; width as usize * height as usize * 4];
    let mut tile = [0u8; 64];
    for block_y in 0..blocks_high {
        for block_x in 0..blocks_wide {
            let offset = (block_y * blocks_wide + block_x) * block_size;
            match format {
                PixelFormat::Bc1 => bcdec_rs::bc1(&data[offset..offset + 8], &mut tile, 16),
                PixelFormat::Bc2 => bcdec_rs::bc2(&data[offset..offset + 16], &mut tile, 16),
                PixelFormat::Bc3 => bcdec_rs::bc3(&data[offset..offset + 16], &mut tile, 16),
                _ => unreachable!(),
            }
            for y in 0..4 {
                for x in 0..4 {
                    let destination_x = block_x * 4 + x;
                    let destination_y = block_y * 4 + y;
                    if destination_x < width as usize && destination_y < height as usize {
                        let destination = (destination_y * width as usize + destination_x) * 4;
                        let source = (y * 4 + x) * 4;
                        out[destination..destination + 4]
                            .copy_from_slice(&tile[source..source + 4]);
                    }
                }
            }
        }
    }
    Ok(out)
}

pub fn lit_color(albedo_rgb: [f32; 3], lighting: [f32; 3]) -> [f32; 3] {
    let albedo = lighting_iw4::lit_albedo(albedo_rgb, [1.0, 1.0, 1.0]);
    lighting_iw4::lit_fragment_color(albedo, lighting, [0.0, 0.0, 0.0])
}

pub fn decode_iwi_cubemap_native(bytes: &[u8], srgb: bool) -> Result<Image, String> {
    let (size, faces) = decode_iwi_cubemap(bytes)?;
    Ok(pack_material_cubemap(size, &faces, srgb))
}

fn decode_iwi_cubemap(bytes: &[u8]) -> Result<(u32, CubemapFaces), String> {
    let header = parse_iwi_header(bytes)?;
    if header.width == 0 || header.width != header.height {
        return Err(format!(
            "non-square cubemap {}x{}",
            header.width, header.height
        ));
    }
    let face_bytes = match header.format {
        PixelFormat::Bgra8 | PixelFormat::Bgrx8 => (header.width * header.height * 4) as usize,
        PixelFormat::Rgb8 => (header.width * header.height * 3) as usize,
        PixelFormat::Bc1 => (header.width.div_ceil(4) * header.height.div_ceil(4) * 8) as usize,
        PixelFormat::Bc2 | PixelFormat::Bc3 => {
            (header.width.div_ceil(4) * header.height.div_ceil(4) * 16) as usize
        }
        _ => return Err("unsupported cubemap pixel layout".into()),
    };

    let start = header
        .mip0_end
        .checked_sub(face_bytes * 6)
        .filter(|&start| start >= header.header_len && header.mip0_end <= bytes.len())
        .ok_or_else(|| "truncated cubemap top mip".to_owned())?;
    let mut faces = std::array::from_fn(|_| Vec::new());
    for (i, face) in faces.iter_mut().enumerate() {
        let start = start + i * face_bytes;
        let (_, _, rgba) = decode_pixels(
            &bytes[start..start + face_bytes],
            header.width,
            header.height,
            header.format,
        )?;
        *face = rgba;
    }
    Ok((header.width, faces))
}

fn pack_material_cubemap(size: u32, faces: &CubemapFaces, srgb: bool) -> Image {
    let mut pixels = Vec::with_capacity(6 * size as usize * size as usize * 4);
    for face in faces {
        pixels.extend_from_slice(face);
    }
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size * 6,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image
        .reinterpret_stacked_2d_as_array(6)
        .expect("six equal cubemap faces");
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..Default::default()
    });
    let mut sampler = ImageSamplerDescriptor::linear();
    sampler.address_mode_u = ImageAddressMode::ClampToEdge;
    sampler.address_mode_v = ImageAddressMode::ClampToEdge;
    sampler.address_mode_w = ImageAddressMode::ClampToEdge;
    image.sampler = ImageSampler::Descriptor(sampler);
    image
}

pub const NORMAL_DECODE_SCALE: [f32; 2] = [4.08, 4.06452];
pub const NORMAL_DECODE_BIAS: [f32; 2] = [-2.08, -2.06452];

pub fn decode_dxt5nm_xy(rgba: [f32; 4]) -> [f32; 2] {
    [
        rgba[3] * NORMAL_DECODE_SCALE[0] + NORMAL_DECODE_BIAS[0],
        rgba[1] * NORMAL_DECODE_SCALE[1] + NORMAL_DECODE_BIAS[1],
    ]
}

pub fn lightmap_bake(
    page0_rgb: [f32; 3],
    page1_rgb: [f32; 3],
    lm_dir: [f32; 2],
    n_ts: [f32; 2],
) -> [f32; 3] {
    let inv_n = (n_ts[0] * n_ts[0] + n_ts[1] * n_ts[1] + 1.0).sqrt().recip();
    let inv_l = (lm_dir[0] * lm_dir[0] + lm_dir[1] * lm_dir[1] + 1.0)
        .sqrt()
        .recip();
    let bake_ndotl =
        ((lm_dir[0] * n_ts[0] + lm_dir[1] * n_ts[1] + 1.0) * inv_l * inv_n).clamp(0.0, 1.0);
    [
        {
            let v = page0_rgb[0] * inv_n + page1_rgb[0] * bake_ndotl;
            v * v
        },
        {
            let v = page0_rgb[1] * inv_n + page1_rgb[1] * bake_ndotl;
            v * v
        },
        {
            let v = page0_rgb[2] * inv_n + page1_rgb[2] * bake_ndotl;
            v * v
        },
    ]
}

#[derive(Clone)]
pub struct ImageDemandPlan {
    id: u64,
    zone_ff: PathBuf,

    demands: Vec<AuthoredImage>,
    requests: Vec<(u8, bool, bool, bool)>,
    /// Catalog rows that pointed at one of `demands`. More rows than demands
    /// is the normal case — several materials share an image — and the
    /// difference is what `duplicate_claims` reports.
    claimed_rows: usize,
    /// Demands [`Self::prune_to`] dropped because the merged catalog had
    /// already been answered for them. They are still claims this plan made,
    /// so they stay in `canonical_variants`; what they are not is work, and
    /// the census names them on their own rather than letting them fall into
    /// the gap between two other numbers.
    pruned_variants: usize,
    pruned_rows: usize,
}

/// One image this plan has ready, and how it got it.
///
/// The texels, not an image: the `Image` is built in `apply`, for the rows the
/// merge keeps. A claim the merge drops is then a decode that was wasted and
/// not also a copy, and two claims on one archive entry are one buffer until
/// they are placed.
#[derive(Clone)]
struct PreparedImage {
    name: String,
    payload: Arc<PreparedPayload>,
    wrap: WrapRecipe,
    variant: Option<ImageVariantId>,
    /// Another asker had prepared these texels first; no decode was repeated
    /// for this one.
    shared: bool,
}

#[derive(Default)]
pub struct DecodedImageBatch {
    plan: Option<u64>,
    decoded: Vec<PreparedImage>,
    pub stats: MaterialImageStats,
    /// Whether this batch is counted in [`UNAPPLIED_BATCHES`]. An empty plan
    /// never enters the queue, and a batch must leave it exactly once.
    queued: bool,
    /// Bytes this plan decoded itself, and bytes it took from another plan's
    /// work. Only the first is what the plan cost.
    newly_prepared_bytes: u64,
    reused_bytes: u64,
    reused_variants: usize,
    claimed_rows: usize,
    canonical_variants: usize,
    pruned_variants: usize,
    pruned_rows: usize,
}

impl Drop for DecodedImageBatch {
    /// A result leaves the unapplied queue when it is applied *or* when it is
    /// thrown away: a canceled load drops batches without applying them, and
    /// leaving them counted would hold the next plan back against a queue that
    /// no longer exists. The bytes themselves are the payloads' own charge and
    /// are released when the last holder drops them, which is not necessarily
    /// here.
    fn drop(&mut self) {
        if self.queued {
            self.queued = false;
            UNAPPLIED_BATCHES.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
pub struct PayloadRetention {
    held: HashMap<usize, Arc<PreparedPayload>>,
    bytes: u64,
}

impl PayloadRetention {
    pub fn keep(&mut self, batch: &DecodedImageBatch) -> u64 {
        let mut added = 0;
        for image in &batch.decoded {
            let key = Arc::as_ptr(&image.payload) as usize;
            if let std::collections::hash_map::Entry::Vacant(slot) = self.held.entry(key) {
                added += image.payload.bytes();
                slot.insert(Arc::clone(&image.payload));
            }
        }
        self.bytes += added;
        RETAINED_PAYLOAD_BYTES.fetch_add(added, Ordering::Relaxed);
        added
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn payloads(&self) -> usize {
        self.held.len()
    }
}

impl Drop for PayloadRetention {
    fn drop(&mut self) {
        RETAINED_PAYLOAD_BYTES.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

/// What one plan's claims cost and what survived the merge.
///
/// The three counts answer different questions: `canonical_variants` is how
/// many distinct images the plan wanted, `prepared_variants` how many it
/// actually decoded, and `discarded_decoded_bytes` how much of that decode
/// the merged catalog threw away because another source had already answered
/// for the same name.
#[derive(Clone, Debug, Default)]
pub struct ImageMergeCensus {
    pub claimed_rows: usize,
    pub canonical_variants: usize,
    pub prepared_variants: usize,
    pub duplicate_claims: usize,
    /// Claims the plan gave up before decoding, because the merged catalog was
    /// already answered for the name. Its own number: it is neither a
    /// duplicate claim — two rows wanting one image — nor a discard, which is
    /// a decode that happened and was thrown away. `canonical_variants` still
    /// counts them, so `canonical - pruned` is what the decode was asked for.
    pub pruned_variants: usize,
    pub pruned_rows: usize,
    pub filled_rows: usize,
    pub already_decoded: usize,
    pub discarded_variants: usize,
    pub discarded_decoded_bytes: u64,
    pub final_cpu_bytes: u64,
    /// Bytes this plan decoded itself, and bytes it took from a variant another
    /// plan had already prepared. `newly_prepared_bytes` is what the plan cost;
    /// `reused_bytes` is what it did not have to pay again. Their sum is what
    /// the plan *served*, which is a different quantity from either and from
    /// the physical bytes the decode budget charges — that one counts each
    /// distinct payload once, whoever asked for it.
    pub newly_prepared_bytes: u64,
    pub reused_bytes: u64,
    pub reused_variants: usize,
    /// Of the discarded variants, how many the winning row had prepared from
    /// the identical variant — the same bytes twice — against how many were
    /// genuinely answered by a different source, and how many the merge
    /// answered from an inline body.
    pub discarded_same_variant: usize,
    pub discarded_overridden: usize,
    pub discarded_inline: usize,
    /// Of the discards the census would otherwise call overridden, how many
    /// hold the *same payload* as the winner and differ only in the colour
    /// space or the sampler wrapped around it. Sharing keys on the payload, so
    /// these cost one copy of the chain rather than a second decode.
    pub discarded_same_payload: usize,
    /// A discarded name whose winner holds the same payload under a different
    /// wrapping.
    pub first_same_payload: Option<String>,
    /// Of the same-variant discards, how many were served out of another
    /// asker's decode rather than read and decoded a second time.
    pub discarded_shared: usize,
    /// Every discarded variant grouped by why the merge had no use for it.
    pub discard_reasons: Vec<(&'static str, usize)>,
    pub first_discarded: Option<String>,
    /// A discarded name whose winner came from a different source, as evidence
    /// that the two are not the same image.
    pub first_override: Option<String>,
    /// Every discarded claim grouped by who answered for it and what kind of
    /// source they were, largest group first.
    pub disputed_winners: Vec<DisputedWinner>,
}

impl ImageMergeCensus {
    /// The reasons as one line, or `None` when nothing was discarded.
    pub fn discard_line(&self) -> Option<String> {
        if self.discarded_variants == 0 {
            return None;
        }
        let reasons = self
            .discard_reasons
            .iter()
            .map(|(reason, n)| format!("{reason}={n}"))
            .collect::<Vec<_>>()
            .join(" ");
        Some(format!(
            "{} variants, {} bytes ({} same variant of which {} cost nothing by sharing, {} same payload under another wrapping, {} overridden by another source, {} by an inline body): {reasons}{}{}{}",
            self.discarded_variants,
            self.discarded_decoded_bytes,
            self.discarded_same_variant,
            self.discarded_shared,
            self.discarded_same_payload,
            self.discarded_overridden,
            self.discarded_inline,
            self.first_discarded
                .as_ref()
                .map_or_else(String::new, |name| format!(" first={name}")),
            self.first_same_payload
                .as_ref()
                .map_or_else(String::new, |name| format!(" first_same_payload={name}")),
            self.first_override
                .as_ref()
                .map_or_else(String::new, |name| format!(" first_override={name}")),
        ))
    }
}

/// Decoded payload bytes resident right now: every prepared buffer alive in
/// the process, each counted once whoever is pointing at it.
///
/// Starting a decode earlier — which is the point of enqueueing a plan the
/// moment it is ready — moves those bytes earlier too, and they sit in the
/// process until the last holder lets go. Without a ceiling, two plans that
/// would otherwise run one after the other hold both peaks at once. This is
/// that ceiling: a plan waits before it starts, never in the middle, so a worker
/// can never block on bytes only another worker in the same pool could free.
///
/// It is a *starting* threshold and not a hard cap, and the two are different
/// claims. A plan that is let through decodes everything it planned, so the
/// bytes outstanding can end up well over the ceiling — what is bounded is how
/// much is already outstanding when the next plan is allowed to begin. A hard
/// cap would need reservation, splitting or eviction inside the decode, which
/// is a different change; saying "ceiling" for this one is what made a run
/// holding 886 MiB under a 768 MiB setting look like a bug.
///
/// This is resident memory, which is *not* the queue of results nobody has
/// taken yet — [`UNAPPLIED_BATCHES`] is that one, and the two move
/// independently: a payload two batches share is one allocation and two
/// queued results, and a batch that has been applied still holds nothing
/// while its payloads stay alive for whoever else asked.
static RESIDENT_PAYLOAD_BYTES: AtomicU64 = AtomicU64::new(0);

static RETAINED_PAYLOAD_BYTES: AtomicU64 = AtomicU64::new(0);

fn gated_payload_bytes() -> u64 {
    RESIDENT_PAYLOAD_BYTES
        .load(Ordering::Relaxed)
        .saturating_sub(RETAINED_PAYLOAD_BYTES.load(Ordering::Relaxed))
}

/// Plans that finished and whose results nobody has applied yet. It is the
/// admission gate's other half: one plan always runs, so the ceiling can
/// throttle the load but never stop it.
static UNAPPLIED_BATCHES: AtomicU64 = AtomicU64::new(0);

/// How long a plan may be held back before it starts anyway.
///
/// The wait is only safe while every plan that holds bytes is joined before the
/// plan that is waiting — which is true of today's join order, and is exactly
/// the kind of ordering a later change breaks silently. A bounded wait cannot
/// deadlock whatever the order becomes: the worst it can do is what the load
/// did before the ceiling existed, and it says so in the log.
const BUDGET_WAIT_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

const DECODE_BUDGET_ENV: &str = "IW4L_IMAGE_DECODE_BUDGET_MIB";
const DECODE_BUDGET_DEFAULT_MIB: u64 = 768;

/// How many bytes of decoded-but-unapplied images the load may hold at once.
pub fn decode_budget_bytes() -> u64 {
    static BUDGET: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *BUDGET.get_or_init(|| {
        let mib = match std::env::var(DECODE_BUDGET_ENV) {
            Ok(value) => value
                .trim()
                .parse::<u64>()
                .unwrap_or(DECODE_BUDGET_DEFAULT_MIB),
            Err(_) => DECODE_BUDGET_DEFAULT_MIB,
        };
        mib.saturating_mul(1024 * 1024)
    })
}

/// Decoded payload bytes resident right now, each distinct buffer once.
pub fn unapplied_decoded_bytes() -> u64 {
    RESIDENT_PAYLOAD_BYTES.load(Ordering::Relaxed)
}

/// Block until the unapplied bytes are under the ceiling, or until this is the
/// only batch in flight — one plan always runs, so the budget can throttle the
/// load but never stop it.
///
/// Returns what was outstanding when the plan was let through and whether it
/// had to wait at all, so the decode timer can start here and the row can say
/// which of the two the time went to.
fn wait_for_decode_budget() -> (u64, bool) {
    let budget = decode_budget_bytes();
    let since = std::time::Instant::now();
    let mut waited = false;
    while UNAPPLIED_BATCHES.load(Ordering::Relaxed) > 0 && gated_payload_bytes() >= budget {
        if since.elapsed() >= BUDGET_WAIT_LIMIT {
            diag::warn!(
                Zone,
                "image decode budget: started a plan anyway after {:.1}s with {} MiB resident over the {} MiB threshold and {} finished result(s) unapplied",
                since.elapsed().as_secs_f32(),
                gated_payload_bytes() >> 20,
                budget >> 20,
                UNAPPLIED_BATCHES.load(Ordering::Relaxed),
            );
            break;
        }
        waited = true;
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    (RESIDENT_PAYLOAD_BYTES.load(Ordering::Relaxed), waited)
}

fn image_bytes(image: &Image) -> u64 {
    image.data.as_ref().map_or(0, Vec::len) as u64
}

impl ImageDemandPlan {
    fn new(zone_ff: &Path) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            zone_ff: zone_ff.to_owned(),
            demands: Vec::new(),
            requests: Vec::new(),
            claimed_rows: 0,
            pruned_variants: 0,
            pruned_rows: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.demands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.demands.is_empty()
    }

    /// Distinct images this plan claimed. Not what it will decode once it has
    /// been pruned — `len` is that — because the claims it gave up are what a
    /// reader is comparing the decode against.
    pub fn canonical_variants(&self) -> usize {
        self.demands.len() + self.pruned_variants
    }

    /// Demands and rows dropped before the decode, because the merged catalog
    /// already holds an answer for the name.
    pub fn pruned(&self) -> (usize, usize) {
        (self.pruned_variants, self.pruned_rows)
    }

    /// Drop every demand whose claim the merge will not take.
    ///
    /// `apply` fills exactly the rows that still carry this plan's id and have
    /// nothing decoded in them; a name with no such row left is a decode whose
    /// result `apply` would count and throw away. The two predicates are the
    /// same one, deliberately: what survives here is what would have survived
    /// there, so pruning changes when the question is asked and not what the
    /// merged catalog ends up holding.
    ///
    /// It is only true of a catalog that is finished with this plan's rivals.
    /// Called against a catalog another plan is still going to apply into, it
    /// would cut names that plan is about to release.
    pub fn prune_to(&mut self, catalog: &MaterialDefinitions) {
        let id = self.id;
        let mut wanted: HashMap<&str, usize> = HashMap::new();
        for image in &catalog.images {
            if image.pending_decode == Some(id) && image.decoded.is_none() {
                *wanted.entry(image.name.as_str()).or_default() += 1;
            }
        }
        let mut kept_rows = 0usize;
        let mut index = 0usize;
        while index < self.demands.len() {
            match wanted.get(self.demands[index].name.as_str()) {
                Some(rows) => {
                    kept_rows += rows;
                    index += 1;
                }
                None => {
                    self.demands.swap_remove(index);
                    self.requests.swap_remove(index);
                    self.pruned_variants += 1;
                }
            }
        }
        self.pruned_rows = self.claimed_rows.saturating_sub(kept_rows);
    }

    /// Hand back every row this plan still holds a claim on, and say how many
    /// there were.
    ///
    /// `apply` is what normally does this: it clears `pending_decode` on every
    /// row carrying the plan's id, whether or not the plan had anything to put
    /// there. A plan that [`Self::prune_to`] emptied never reaches `apply`, so
    /// without this its rows keep a mark saying a decode is owed on them —
    /// and [`requested_color_map_slots`] and [`requested_keyed_image_slots`] read
    /// that mark as "somebody else is already handling this name" and skip the
    /// row. Every row still marked here was answered by another plan, which is
    /// why the plan is empty; the count is that plan's `already_decoded`.
    pub fn release_claims(&self, catalog: &mut MaterialDefinitions) -> usize {
        let id = self.id;
        let mut released = 0usize;
        for image in &mut catalog.images {
            if image.pending_decode == Some(id) {
                image.pending_decode = None;
                released += 1;
            }
        }
        released
    }

    /// Catalog rows that asked for one of those images.
    pub fn claimed_rows(&self) -> usize {
        self.claimed_rows
    }

    /// This plan's id, which is what the merge census names as the winner of a
    /// disputed claim. Reported beside the plan's label so a census line can
    /// be read without knowing the numbering.
    pub fn id(&self) -> u64 {
        self.id
    }

    fn push(&mut self, image: AuthoredImage, request: (u8, bool, bool, bool)) {
        self.claimed_rows += 1;
        if let Some(at) = self
            .demands
            .iter()
            .position(|demand| demand.name.as_str() == image.name.as_str())
        {
            let (_, already_normal, already_sharp, already_linear) = &mut self.requests[at];
            *already_normal |= request.1;
            *already_sharp |= request.2;
            *already_linear |= request.3;
            return;
        }
        self.demands.push(image);
        self.requests.push(request);
    }

    /// Decode this plan's images. `job` is the row this work reports into, and
    /// it is stamped *after* the memory ceiling lets the plan through, so the
    /// decode timer measures decoding and the wait before it is its own column.
    pub fn run(
        self,
        stage: &StageHandle,
        job: asset_transport::Job,
        pool: &TaskPool,
    ) -> DecodedImageBatch {
        if self.demands.is_empty() {
            return DecodedImageBatch::default();
        }
        let (outstanding, waited) = wait_for_decode_budget();
        job.decode_started(outstanding, waited);
        let index = match game_main_for_zone(&self.zone_ff).and_then(|main| IwdIndex::open(&main)) {
            Ok(index) => index,
            Err(error) => {
                return DecodedImageBatch {
                    plan: Some(self.id),
                    decoded: Vec::new(),
                    stats: MaterialImageStats {
                        requested: self.demands.len(),
                        missing: self.demands.len(),
                        first_gap: Some(error),
                        ..Default::default()
                    },
                    queued: false,
                    newly_prepared_bytes: 0,
                    reused_bytes: 0,
                    reused_variants: 0,
                    claimed_rows: self.claimed_rows,
                    canonical_variants: self.canonical_variants(),
                    pruned_variants: self.pruned_variants,
                    pruned_rows: self.pruned_rows,
                };
            }
        };
        let work = self
            .requests
            .iter()
            .copied()
            .enumerate()
            .collect::<Vec<ImageRequest>>();
        let mut stats = MaterialImageStats {
            archives: index.0.archive_count(),
            requested: work.len(),
            ..Default::default()
        };
        stage.set_total(work.len() as u64);
        let outcomes =
            decode_requests_in_parallel(&self.demands, index.as_ref(), &work, stage, pool);
        let mut decoded = Vec::with_capacity(outcomes.len());
        let mut newly_prepared_bytes = 0u64;
        let mut reused_bytes = 0u64;
        let mut reused_variants = 0usize;
        for outcome in outcomes {
            match outcome {
                ImageOutcome::Decoded {
                    image_index,
                    payload,
                    wrap,
                    variant,
                    shared,
                } => {
                    let Some(source) = self.demands.get(image_index) else {
                        continue;
                    };
                    if shared {
                        reused_bytes += payload.bytes();
                        reused_variants += 1;
                    } else {
                        newly_prepared_bytes += payload.bytes();
                    }
                    decoded.push(PreparedImage {
                        name: source.name.as_str().to_owned(),
                        payload,
                        wrap,
                        variant,
                        shared,
                    });
                    stats.decoded += 1;
                }
                ImageOutcome::Missing { gap } => {
                    stats.missing += 1;
                    stats.first_gap.get_or_insert(gap);
                }
                ImageOutcome::Unsupported { gap } => {
                    stats.unsupported += 1;
                    stats.first_unsupported.get_or_insert(gap);
                }
            }
        }
        // The bytes are already charged, once per buffer, by the payloads
        // themselves. What enters the queue here is the *result*: a finished
        // plan nobody has applied, which is the other half of the admission
        // gate and a different quantity from the memory.
        UNAPPLIED_BATCHES.fetch_add(1, Ordering::Relaxed);
        DecodedImageBatch {
            plan: Some(self.id),
            decoded,
            stats,
            queued: true,
            newly_prepared_bytes,
            reused_bytes,
            reused_variants,
            claimed_rows: self.claimed_rows,
            canonical_variants: self.canonical_variants(),
            pruned_variants: self.pruned_variants,
            pruned_rows: self.pruned_rows,
        }
    }
}

impl DecodedImageBatch {
    #[must_use]
    pub fn into_kept(mut self) -> Self {
        if self.queued {
            self.queued = false;
            UNAPPLIED_BATCHES.fetch_sub(1, Ordering::Relaxed);
        }
        self
    }

    pub fn share(&self) -> Self {
        let served = self.decoded.iter().map(|image| image.payload.bytes()).sum();
        Self {
            plan: self.plan,
            decoded: self.decoded.clone(),
            stats: self.stats.clone(),
            queued: false,
            newly_prepared_bytes: 0,
            reused_bytes: served,
            reused_variants: self.decoded.len(),
            claimed_rows: self.claimed_rows,
            canonical_variants: self.canonical_variants,
            pruned_variants: self.pruned_variants,
            pruned_rows: self.pruned_rows,
        }
    }

    /// Hand the decoded images to the merged catalog and say what happened.
    ///
    /// A claimed row does not always survive the merge: another source can
    /// supply the same name with an inline payload, or have decoded it first,
    /// and `link_image` then keeps that row and drops this plan's. The decode
    /// behind the dropped claim is work the run paid for and threw away, so it
    /// is counted here by reason rather than left as a difference between two
    /// other numbers.
    pub fn apply(mut self, catalog: &mut MaterialDefinitions) -> ImageMergeCensus {
        let mut census = ImageMergeCensus {
            claimed_rows: self.claimed_rows,
            canonical_variants: self.canonical_variants,
            prepared_variants: self.stats.decoded,
            duplicate_claims: self.claimed_rows.saturating_sub(self.canonical_variants),
            pruned_variants: self.pruned_variants,
            pruned_rows: self.pruned_rows,
            newly_prepared_bytes: self.newly_prepared_bytes,
            reused_bytes: self.reused_bytes,
            reused_variants: self.reused_variants,
            ..ImageMergeCensus::default()
        };
        let Some(id) = self.plan else {
            return census;
        };
        let mut decoded: HashMap<String, PreparedImage> = std::mem::take(&mut self.decoded)
            .into_iter()
            .map(|prepared| (prepared.name.clone(), prepared))
            .collect();

        let mut rows: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, image) in catalog.images.iter_mut().enumerate() {
            if image.pending_decode != Some(id) {
                continue;
            }
            image.pending_decode = None;
            if image.decoded.is_some() {
                census.already_decoded += 1;
                continue;
            }
            rows.entry(image.name.as_str().to_owned())
                .or_default()
                .push(index);
        }
        for (name, indices) in rows {
            let Some(prepared) = decoded.remove(&name) else {
                continue;
            };
            let variant = prepared.variant;
            let mut indices = indices.into_iter();
            let Some(first) = indices.next() else {
                continue;
            };
            let image = wrap_payload(&prepared.payload, prepared.wrap);
            census.final_cpu_bytes += image_bytes(&image);
            for index in indices {
                if let Some(slot) = catalog.images.get_mut(index) {
                    slot.decoded = Some(image.clone());
                    slot.decoded_variant = variant;
                    slot.decoded_by = Some(id);
                    census.filled_rows += 1;
                }
            }
            if let Some(slot) = catalog.images.get_mut(first) {
                slot.decoded = Some(image);
                slot.decoded_variant = variant;
                slot.decoded_by = Some(id);
                census.filled_rows += 1;
            }
        }

        let mut reasons: HashMap<&'static str, usize> = HashMap::new();
        let mut disputed: HashMap<(&'static str, Option<u64>), (usize, String)> = HashMap::new();
        for (name, prepared) in decoded {
            census.discarded_variants += 1;
            census.discarded_decoded_bytes += prepared.payload.bytes();
            census.first_discarded.get_or_insert_with(|| name.clone());
            *reasons.entry(discard_reason(catalog, &name)).or_default() += 1;
            let (winner, by) = winner_of(catalog, &name, prepared.variant);
            // Who answered for this name, not only that somebody did: a plan
            // id and a source kind are what decides whether the claim could
            // have been resolved before it was prepared.
            let disputed = disputed
                .entry((winner.kind(), by))
                .or_insert((0usize, name.clone()));
            disputed.0 += 1;
            match winner {
                Winner::SameVariant => {
                    census.discarded_same_variant += 1;
                    if prepared.shared {
                        census.discarded_shared += 1;
                    }
                }
                Winner::SamePayload => {
                    census.discarded_same_payload += 1;
                    census
                        .first_same_payload
                        .get_or_insert_with(|| name.clone());
                }
                Winner::OtherSource => {
                    census.discarded_overridden += 1;
                    census.first_override.get_or_insert(name);
                }
                Winner::InlineBody => census.discarded_inline += 1,
                Winner::Unknown => {}
            }
        }
        census.discard_reasons = reasons.into_iter().collect();
        census
            .discard_reasons
            .sort_by_key(|(reason, n)| (std::cmp::Reverse(*n), *reason));
        census.disputed_winners = disputed
            .into_iter()
            .map(|((kind, by), (n, first))| DisputedWinner {
                kind,
                plan: by,
                claims: n,
                first,
            })
            .collect();
        census
            .disputed_winners
            .sort_by(|a, b| b.claims.cmp(&a.claims).then(a.kind.cmp(b.kind)));
        census
    }
}

/// Who answered for the claims one plan prepared and the merge dropped.
///
/// A count by reason says how much work was thrown away; this says who threw
/// it away and what kind of source they were, which is what decides whether
/// the claim could have been resolved before it was prepared at all. A claim
/// the winner answered from the same payload was never a second decode; one a
/// genuinely different source answered was.
#[derive(Clone, Debug)]
pub struct DisputedWinner {
    pub kind: &'static str,
    /// The decode plan whose result the merged catalog kept, where one did.
    /// `None` is an inline body, or a row nothing had answered.
    pub plan: Option<u64>,
    pub claims: usize,
    /// One of the names, so the line can be chased in a log.
    pub first: String,
}

/// Where the merged catalog's answer for a dropped claim came from.
///
/// The reason-string alone cannot make this difference: "another source
/// decoded it first" covers both a plan that prepared the very same archive
/// entry a second time — avoidable work — and two games that ship different
/// images under one name, where preparing both and keeping one is a
/// *scheduling* question and not a duplicate at all.
enum Winner {
    /// The kept row holds the identical variant this plan prepared.
    SameVariant,
    /// The kept row holds the same payload under a different wrapping: the
    /// colour space or the sampler told the two claims apart, and nothing
    /// else did. Both were served from one decode; what the merge threw away
    /// is the copy, not the read.
    SamePayload,
    /// The kept row holds a different payload: another source answered.
    OtherSource,
    /// The kept row carries its own inline body.
    InlineBody,
    /// The name is not in the merged catalog, or the winner records no variant.
    Unknown,
}

fn winner_of(
    catalog: &MaterialDefinitions,
    name: &str,
    ours: Option<ImageVariantId>,
) -> (Winner, Option<u64>) {
    let Some(kept) = catalog
        .images
        .iter()
        .find(|image| image.name.as_str() == name)
    else {
        return (Winner::Unknown, None);
    };
    if !kept.payload.is_empty() {
        return (Winner::InlineBody, kept.decoded_by);
    }
    let winner = match (kept.decoded_variant, ours) {
        (Some(theirs), Some(ours)) if theirs == ours => Winner::SameVariant,
        (Some(theirs), Some(ours)) if theirs.payload == ours.payload => Winner::SamePayload,
        (Some(_), Some(_)) => Winner::OtherSource,
        _ => Winner::Unknown,
    };
    (winner, kept.decoded_by)
}

impl Winner {
    /// What kind of source answered for the name, in the census's words.
    const fn kind(&self) -> &'static str {
        match self {
            Self::SameVariant => "same variant",
            Self::SamePayload => "same payload, other wrapping",
            Self::OtherSource => "another source",
            Self::InlineBody => "inline body",
            Self::Unknown => "no winner recorded",
        }
    }
}

/// Why the merged catalog had no row waiting for a decoded image.
///
/// Each answer is checked against the catalog as it stands now, not inferred:
/// a name the merge kept under another source's row reads differently from a
/// name that left the catalog entirely, and only the first one says the decode
/// was avoidable by asking the merge first.
fn discard_reason(catalog: &MaterialDefinitions, name: &str) -> &'static str {
    match catalog
        .images
        .iter()
        .find(|image| image.name.as_str() == name)
    {
        Some(image) if !image.payload.is_empty() => "merged onto an inline payload",
        Some(image) if image.decoded.is_some() => "another source decoded it first",
        Some(_) => "claim dropped, row kept undecoded",
        None => "name is not in the merged catalog",
    }
}

pub fn plan_material_color_maps(
    zone_ff: &Path,
    catalog: &mut MaterialDefinitions,
    stage: &StageHandle,
    pool: &TaskPool,
) -> (MaterialImageStats, ImageDemandPlan) {
    let mut plan = ImageDemandPlan::new(zone_ff);
    let requested = requested_color_map_slots(catalog);
    let inline = claim(catalog, &mut plan, requested);
    let stats = decode_inline(catalog, &inline, stage, pool);
    (stats, plan)
}

fn claim(
    catalog: &mut MaterialDefinitions,
    plan: &mut ImageDemandPlan,
    requested: Vec<ImageRequest>,
) -> Vec<ImageRequest> {
    let id = plan.id;
    let mut inline = Vec::new();
    for (image_index, request) in requested {
        let Some(image) = catalog.images.get_mut(image_index) else {
            continue;
        };
        if image.payload.is_empty() {
            image.pending_decode = Some(id);
            let mut owned = image.clone();
            owned.decoded = None;
            plan.push(owned, request);
        } else {
            inline.push((image_index, request));
        }
    }
    inline
}

pub fn plan_material_images_for_keys(
    zone_ff: &Path,
    catalog: &mut MaterialDefinitions,
    keys: impl IntoIterator<Item = crate::MaterialKey>,
    stage: &StageHandle,
    pool: &TaskPool,
) -> ImageDemandPlan {
    let mut plan = ImageDemandPlan::new(zone_ff);
    plan_images_for_keys(&mut plan, catalog, keys, stage, pool);
    plan
}

pub fn plan_images_for_keys(
    plan: &mut ImageDemandPlan,
    catalog: &mut MaterialDefinitions,
    keys: impl IntoIterator<Item = crate::MaterialKey>,
    stage: &StageHandle,
    pool: &TaskPool,
) -> usize {
    let requested = requested_keyed_image_slots(catalog, keys);
    let inline = claim(catalog, plan, requested);
    decode_inline(catalog, &inline, stage, pool).decoded
}

fn decode_inline(
    catalog: &mut MaterialDefinitions,
    work: &[ImageRequest],
    stage: &StageHandle,
    pool: &TaskPool,
) -> MaterialImageStats {
    let mut stats = MaterialImageStats {
        requested: work.len(),
        ..Default::default()
    };
    if work.is_empty() {
        return stats;
    }

    let index = IwdIndex(Arc::new(asset_transport::IwdIndex::default()));
    let images = &catalog.images;
    for outcome in decode_requests_in_parallel(images, &index, work, stage, pool) {
        match outcome {
            ImageOutcome::Decoded {
                image_index,
                payload,
                wrap,
                variant,
                ..
            } => {
                let slot = &mut catalog.images[image_index];
                slot.decoded = Some(wrap_payload(&payload, wrap));
                slot.decoded_variant = variant;
                stats.decoded += 1;
            }
            ImageOutcome::Missing { gap } => {
                stats.missing += 1;
                stats.first_gap.get_or_insert(gap);
            }
            ImageOutcome::Unsupported { gap } => {
                stats.unsupported += 1;
                stats.first_unsupported.get_or_insert(gap);
            }
        }
    }
    stats
}
