use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use asset_core::AssetNamespace;
use asset_game::{HUD_CHROME_MENUS, MenuCatalog};
use asset_material::{TS_2D, TS_COLOR_MAP};
use assets::{NamespaceTrees, SessionCompass};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use gamemode_iw4::DAMAGE_FEEDBACK_SHADER;
use hud_iw4::{COMPASS_ENEMY_FIRING_PING_IMAGE, COMPASS_RADAR_LINE_IMAGE};

use crate::gaps::ImageMiss;

pub const HUD_CHROME_NAMESPACE: AssetNamespace = AssetNamespace::Iw4;

const DATA_SAMPLED_IMAGES: &[&str] = &[BLOOD_OVERLAY_MASK, BLOOD_OVERLAY_COLOR];

pub(crate) const BLOOD_OVERLAY_MASK: &str = "blood_defocus_mask";
pub(crate) const BLOOD_OVERLAY_COLOR: &str = "blood_defocus_color";

pub(crate) fn hud_sampling_for(name: &str) -> HudSampling {
    let key = cache_key(name);
    if DATA_SAMPLED_IMAGES.iter().any(|mask| *mask == key) {
        HudSampling::Data
    } else {
        HudSampling::Color
    }
}

fn cache_key(name: &str) -> String {
    asset_core::AssetRef::bare_name(name).to_ascii_lowercase()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HudSampling {
    #[default]
    Color,

    Data,
}

impl HudSampling {
    fn texture_format(self) -> TextureFormat {
        match self {
            Self::Color => TextureFormat::Rgba8UnormSrgb,
            Self::Data => TextureFormat::Rgba8Unorm,
        }
    }
}

type IwdKey = (AssetNamespace, String, HudSampling, Option<u8>);

type ZoneKey = (String, HudSampling, Option<u8>);

type CachedRgba = Option<(u32, u32, Vec<u8>)>;

fn iwd_key(ns: AssetNamespace, name: &str, sampling: HudSampling, sampler: Option<u8>) -> IwdKey {
    (ns, cache_key(name), sampling, sampler)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BloodMaterialBinding {
    pub state: render_material::CompiledPassState,
    pub color_sampler: u8,
    pub mask_sampler: u8,
}

fn blood_material_binding(catalog: &MenuCatalog) -> Result<BloodMaterialBinding, String> {
    let pass = "splatter_alt/unlit/pass 0";
    let plan = catalog
        .material_2d_plans
        .get("splatter_alt")
        .ok_or_else(|| format!("{pass}: material plan missing"))?;
    if plan.unlit_pass_count != Some(1) || plan.unlit_pass_states.len() != 1 {
        return Err(format!(
            "{pass}: exactly one scanned pass and state row required"
        ));
    }
    if plan.textures.len() != 2 {
        return Err(format!("{pass}: expected two texture bindings"));
    }
    let [color, mask] = &plan.textures[..] else {
        unreachable!();
    };
    if color.semantic != TS_COLOR_MAP
        || mask.semantic != TS_COLOR_MAP
        || color.image.as_deref() != Some(BLOOD_OVERLAY_COLOR)
        || mask.image.as_deref() != Some(BLOOD_OVERLAY_MASK)
    {
        return Err(format!(
            "{pass}: color/mask color-map bindings missing or out of order"
        ));
    }
    let state =
        asset_material::compile_material_state(HUD_CHROME_NAMESPACE, plan.unlit_pass_states[0]);
    if let Some(fields) = state.unsupported_host_fields() {
        return Err(format!("{pass}: unsupported material state: {fields:?}"));
    }
    Ok(BloodMaterialBinding {
        state,
        color_sampler: color.sampler_state,
        mask_sampler: mask.sampler_state,
    })
}

fn make_image(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    sampling: HudSampling,
    sampler: Option<u8>,
) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        sampling.texture_format(),
        RenderAssetUsages::default(),
    );
    if let Some(state) = sampler {
        image.sampler = ImageSampler::Descriptor(asset_material::sampler_from_iw4(state, 1, false));
    }
    image
}

#[derive(Resource, Default)]
pub struct HudImages {
    games_root: PathBuf,

    trees: NamespaceTrees,
    adopted_zone: Option<PathBuf>,

    map_namespace: Option<AssetNamespace>,
    by_name: HashMap<IwdKey, Option<Handle<Image>>>,
    rgba_by_name: HashMap<(AssetNamespace, String), CachedRgba>,
    zone_rgba: HashMap<String, (u32, u32, Arc<Vec<u8>>)>,
    zone_handles: HashMap<ZoneKey, Handle<Image>>,
    zone_image_name: HashMap<String, String>,
    material_images: HashMap<String, String>,
    zone_states: HashMap<String, Option<render_material::CompiledPassState>>,
    zone_srgb_reads: HashMap<String, bool>,
    zone_samplers: HashMap<String, u8>,
    blood_plan: Option<Result<BloodMaterialBinding, String>>,
    zone_installed: bool,
    zone_uploaded: bool,
    iwd_warmed: bool,
}

impl HudImages {
    /// An image made at run time in a namespace, under a name of its own.
    pub fn insert_runtime_in(&mut self, ns: AssetNamespace, name: &str, handle: Handle<Image>) {
        self.by_name.insert(iwd_key(ns, name, HudSampling::Color, None), Some(handle));
    }

    /// An image made at run time, drawn under a material name of its own.
    pub fn insert_runtime(&mut self, name: &str, handle: Handle<Image>) {
        // Draw lists look images up by their native sampling, which is Data
        // for any name the zone has not marked sRGB, so the image answers to
        // both.
        for sampling in [HudSampling::Color, HudSampling::Data] {
            self.by_name.insert(
                iwd_key(HUD_CHROME_NAMESPACE, name, sampling, None),
                Some(handle.clone()),
            );
        }
    }

    pub fn set_games_root(&mut self, root: &Path) {
        if self.games_root == root {
            return;
        }
        self.games_root = root.to_path_buf();
        self.trees = NamespaceTrees::discover(&asset_transport::GamesRoot(self.games_root.clone()));
        self.adopted_zone = None;
        self.by_name.clear();
        self.rgba_by_name.clear();
        self.zone_uploaded = false;
        self.iwd_warmed = false;
        self.log_trees();
    }

    pub fn adopt_map_zone(&mut self, zone_ff: &Path) {
        if self.adopted_zone.as_deref() == Some(zone_ff) {
            return;
        }
        self.adopted_zone = Some(zone_ff.to_path_buf());
        let Some(namespace) = asset_transport::zone_game_for_path(zone_ff) else {
            return;
        };
        let mut trees = self.trees.clone();
        trees.adopt_zone(zone_ff);
        if trees == self.trees && Some(namespace) == self.map_namespace {
            return;
        }
        self.trees = trees;
        self.map_namespace = Some(namespace);
        self.by_name.clear();
        self.rgba_by_name.clear();
        self.iwd_warmed = false;
        self.log_trees();
    }

    pub fn map_namespace(&self) -> Option<AssetNamespace> {
        self.map_namespace
    }

    fn log_trees(&self) {
        for line in self.trees.report_lines() {
            diag::info!(Ui, "hud: {line}");
        }
    }

    pub fn has_games_root(&self) -> bool {
        !self.games_root.as_os_str().is_empty()
    }

    pub fn install_zone_catalog(&mut self, catalog: &MenuCatalog) {
        if self.zone_installed {
            return;
        }
        self.zone_installed = true;
        self.zone_uploaded = false;
        self.blood_plan = Some(blood_material_binding(catalog));
        self.zone_srgb_reads.extend(
            catalog
                .material_srgb_reads
                .iter()
                .map(|(name, &srgb)| (cache_key(name), srgb)),
        );
        self.material_images.extend(
            catalog
                .material_images
                .iter()
                .map(|(material, image)| (cache_key(material), image.clone())),
        );
        for (name, plan) in &catalog.material_2d_plans {
            let binding =
                plan.textures
                    .iter()
                    .rev()
                    .find(|binding| binding.semantic == TS_2D && binding.image.is_some())
                    .or_else(|| {
                        plan.textures.iter().rev().find(|binding| {
                            binding.semantic == TS_COLOR_MAP && binding.image.is_some()
                        })
                    });
            if let Some(binding) = binding
                && catalog.material_images.get(name) == binding.image.as_ref()
            {
                self.zone_samplers
                    .insert(cache_key(name), binding.sampler_state);
            }
        }
        for (name, state) in &catalog.material_state_bits {
            let compiled = state.agreed().and_then(|words| {
                let compiled = asset_material::compile_material_state(HUD_CHROME_NAMESPACE, words);
                if let Some(fields) = compiled.unsupported_host_fields() {
                    diag::warn!(Ui, "hud material state refused: {name}: {fields:?}");
                    None
                } else {
                    Some(compiled)
                }
            });
            self.zone_states.insert(name.clone(), compiled);
            if catalog.zone_images.contains_key(name) && state.agreed().is_none() {
                diag::warn!(Ui, "hud material state gap: {name}: {state:?}");
            }
        }
        for (key, atlas) in &catalog.zone_images {
            self.zone_rgba
                .insert(key.clone(), (atlas.width, atlas.height, atlas.rgba.clone()));
            self.zone_image_name
                .insert(key.clone(), atlas.image.clone());
            self.zone_handles.retain(|(name, _, _), _| name != key);
            self.by_name.retain(|(_, name, _, _), _| name != key);
            self.rgba_by_name.retain(|(_, name), _| name != key);
        }
    }

    pub fn material_state(
        &self,
        ns: AssetNamespace,
        name: &str,
    ) -> Option<render_material::CompiledPassState> {
        (ns == HUD_CHROME_NAMESPACE)
            .then(|| self.zone_states.get(&cache_key(name)).copied().flatten())
            .flatten()
    }

    pub(crate) fn blood_material_binding(&self) -> Result<BloodMaterialBinding, &str> {
        self.blood_plan
            .as_ref()
            .ok_or("splatter_alt/unlit/pass 0: menu catalog missing")?
            .as_ref()
            .map(|binding| *binding)
            .map_err(String::as_str)
    }

    pub fn zone_installed(&self) -> bool {
        self.zone_installed
    }

    pub fn zone_image_name(&self, material: &str) -> Option<&str> {
        let key = cache_key(material);
        self.zone_image_name.get(&key).map(String::as_str)
    }

    pub fn miss_reason(&self) -> ImageMiss {
        if self.has_games_root() || self.zone_installed || !self.trees.is_empty() {
            ImageMiss::NotDecoded
        } else {
            ImageMiss::NoGamesRoot
        }
    }

    pub fn get(
        &mut self,
        ns: AssetNamespace,
        name: &str,
        images: &mut Assets<Image>,
    ) -> Option<Handle<Image>> {
        self.get_sampled(ns, name, HudSampling::Color, images)
    }

    pub(crate) fn get_native(
        &mut self,
        ns: AssetNamespace,
        name: &str,
        images: &mut Assets<Image>,
    ) -> Option<Handle<Image>> {
        let srgb = ns == HUD_CHROME_NAMESPACE
            && self
                .zone_srgb_reads
                .get(&cache_key(name))
                .copied()
                .unwrap_or(false);
        let sampler = (ns == HUD_CHROME_NAMESPACE)
            .then(|| self.zone_samplers.get(&cache_key(name)).copied())
            .flatten();
        self.get_sampled_with_sampler(
            ns,
            name,
            if srgb {
                HudSampling::Color
            } else {
                HudSampling::Data
            },
            sampler,
            images,
        )
    }

    pub fn get_sampled(
        &mut self,
        ns: AssetNamespace,
        name: &str,
        sampling: HudSampling,
        images: &mut Assets<Image>,
    ) -> Option<Handle<Image>> {
        self.get_sampled_with_sampler(ns, name, sampling, None, images)
    }

    pub(crate) fn get_sampled_with_sampler(
        &mut self,
        ns: AssetNamespace,
        name: &str,
        sampling: HudSampling,
        sampler: Option<u8>,
        images: &mut Assets<Image>,
    ) -> Option<Handle<Image>> {
        let key = iwd_key(ns, name, sampling, sampler);
        if ns == HUD_CHROME_NAMESPACE && self.zone_states.get(&key.1) == Some(&None) {
            return None;
        }
        if let Some(cached) = self.by_name.get(&key) {
            return cached.clone();
        }
        if ns == HUD_CHROME_NAMESPACE {
            if let Some(handle) = self.upload_zone(name, sampling, sampler, images) {
                return Some(handle);
            }
        }
        let decoded = self.decode_iwd(ns, name, sampling, sampler, images);
        self.by_name.insert(key, decoded.clone());
        decoded
    }

    pub fn ensure_rgba(&mut self, ns: AssetNamespace, name: &str) {
        let key = (ns, cache_key(name));
        if self.rgba_by_name.contains_key(&key) {
            return;
        }
        if ns == HUD_CHROME_NAMESPACE {
            if let Some((width, height, rgba)) = self.zone_lookup(name) {
                self.rgba_by_name
                    .insert(key, Some((width, height, rgba.to_vec())));
                return;
            }
        }
        let decoded = self.decode_iwd_rgba(ns, name);
        self.rgba_by_name.insert(key, decoded);
    }

    pub fn rgba(&self, ns: AssetNamespace, name: &str) -> Option<&(u32, u32, Vec<u8>)> {
        self.rgba_by_name
            .get(&(ns, cache_key(name)))
            .and_then(Option::as_ref)
    }

    pub fn warm_present_stems(
        &mut self,
        images: &mut Assets<Image>,
        catalog: Option<&MenuCatalog>,
        compass: Option<&SessionCompass>,
    ) {
        self.upload_pending_zone(images);
        if self.iwd_warmed {
            return;
        }
        if self.trees.is_empty() {
            return;
        }

        let blood = self.blood_material_binding().ok();
        for (name, sampling, sampler) in [
            (
                BLOOD_OVERLAY_COLOR,
                HudSampling::Color,
                blood.map(|b| b.color_sampler),
            ),
            (
                BLOOD_OVERLAY_MASK,
                HudSampling::Data,
                blood.map(|b| b.mask_sampler),
            ),
            (DAMAGE_FEEDBACK_SHADER, HudSampling::Color, None),
            (COMPASS_ENEMY_FIRING_PING_IMAGE, HudSampling::Color, None),
            (COMPASS_RADAR_LINE_IMAGE, HudSampling::Color, None),
        ] {
            let _ = self.get_sampled_with_sampler(
                HUD_CHROME_NAMESPACE,
                name,
                sampling,
                sampler,
                images,
            );
        }
        if let Some(compass) = compass {
            if let Some(name) = compass.declaration.image.as_deref()
                && let Some(ns) = self.map_namespace
            {
                let _ = self.get(ns, name, images);
                self.ensure_rgba(ns, name);
            }
        }
        if let Some(catalog) = catalog {
            for font in catalog.fonts.values() {
                if !font.material.is_empty() {
                    let _ = self.get(HUD_CHROME_NAMESPACE, &font.material, images);
                }
            }
            for menu_name in HUD_CHROME_MENUS {
                let Some(menu) = catalog.get(menu_name) else {
                    continue;
                };
                if !menu.window_background.is_empty() {
                    let _ = self.get(HUD_CHROME_NAMESPACE, &menu.window_background, images);
                }
                for item in &menu.items {
                    if !item.background.is_empty() {
                        let _ = self.get(HUD_CHROME_NAMESPACE, &item.background, images);
                    }
                }
            }
        }
        self.iwd_warmed = true;
    }

    fn upload_pending_zone(&mut self, images: &mut Assets<Image>) {
        if self.zone_uploaded || !self.zone_installed {
            return;
        }
        let keys: Vec<String> = self.zone_rgba.keys().cloned().collect();
        let blood = self.blood_material_binding().ok();
        for key in keys {
            let sampling = hud_sampling_for(&key);
            let sampler = match key.as_str() {
                BLOOD_OVERLAY_COLOR => blood.map(|binding| binding.color_sampler),
                BLOOD_OVERLAY_MASK => blood.map(|binding| binding.mask_sampler),
                _ => None,
            };
            let _ = self.upload_zone(&key, sampling, sampler, images);
        }
        self.zone_uploaded = true;
    }

    fn decode_iwd(
        &self,
        ns: AssetNamespace,
        name: &str,
        sampling: HudSampling,
        sampler: Option<u8>,
        images: &mut Assets<Image>,
    ) -> Option<Handle<Image>> {
        match self.decode_iwd_rgba(ns, name) {
            Some((width, height, rgba)) => {
                Some(images.add(make_image(width, height, rgba, sampling, sampler)))
            }
            None => None,
        }
    }

    fn decode_iwd_rgba(&self, ns: AssetNamespace, name: &str) -> CachedRgba {
        if cache_key(name) == "white" {
            return Some((1, 1, vec![255; 4]));
        }
        if let Some((width, height, rgba)) = asset_material::zone_ui_image(ns, name) {
            return Some((width, height, rgba.as_ref().clone()));
        }
        let main = self.trees.main_for(ns)?;
        let mapped = (ns == HUD_CHROME_NAMESPACE)
            .then(|| {
                self.material_images
                    .get(&cache_key(name))
                    .map(String::as_str)
            })
            .flatten();
        for image_name in mapped
            .filter(|image| *image != name)
            .into_iter()
            .chain(std::iter::once(name))
        {
            match asset_material::decode_ui_image_from_main(main, image_name) {
                Ok(Some(image)) => return Some(image),
                Ok(None) => {}
                Err(error) => {
                    diag::warn!(
                        Ui,
                        "hud: decode `{}:{image_name}` from {}: {error}",
                        ns.as_str(),
                        main.display()
                    );
                }
            }
        }
        None
    }

    fn zone_lookup(&self, name: &str) -> Option<(u32, u32, Arc<Vec<u8>>)> {
        self.zone_rgba.get(&cache_key(name)).cloned()
    }

    fn upload_zone(
        &mut self,
        name: &str,
        sampling: HudSampling,
        sampler: Option<u8>,
        images: &mut Assets<Image>,
    ) -> Option<Handle<Image>> {
        let key = (cache_key(name), sampling, sampler);
        if let Some(handle) = self.zone_handles.get(&key) {
            return Some(handle.clone());
        }
        let (width, height, rgba) = self.zone_lookup(name)?;
        let handle = images.add(make_image(
            width,
            height,
            rgba.as_ref().clone(),
            sampling,
            sampler,
        ));
        self.zone_handles.insert(key, handle.clone());
        Some(handle)
    }
}
