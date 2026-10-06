use std::collections::HashMap;
use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};

use crate::classes::setup::picker_icon_stems;
use asset_game::CacItemPresentation;
use frame::showcase_classes;

#[derive(Resource, Clone, Debug, Default)]
pub struct UiAssetRoot(pub Option<PathBuf>);

#[derive(Resource, Default)]
pub struct ClassSelectIconCache {
    pub images: HashMap<String, Handle<Image>>,

    pub warmed: bool,
}

impl ClassSelectIconCache {
    pub fn get(&self, stem: &str) -> Option<Handle<Image>> {
        self.images.get(stem).cloned()
    }
}

type DecodedClassIcons = Vec<(String, Image)>;

pub fn warm_class_select_icons(
    root: Res<UiAssetRoot>,
    mut cache: ResMut<ClassSelectIconCache>,
    mut images: ResMut<Assets<Image>>,
    mut pending: Local<Option<Task<DecodedClassIcons>>>,
) {
    if cache.warmed {
        return;
    }
    if let Some(task) = pending.as_mut() {
        if let Some(decoded) = future::block_on(future::poll_once(task)) {
            for (stem, image) in decoded {
                cache.images.insert(stem, images.add(image));
            }
            // Publish completion only with the images so the open menu rebuilds.
            cache.warmed = true;
            *pending = None;
        }
        return;
    }
    let Some(games) = root.0.clone() else {
        return;
    };
    // Archive discovery and decoding must not hold up window event processing.
    *pending =
        Some(AsyncComputeTaskPool::get().spawn(async move { decode_class_select_icons(&games) }));
}

fn decode_class_select_icons(games: &std::path::Path) -> DecodedClassIcons {
    let started = std::time::Instant::now();
    let mut stems = Vec::new();
    for preset in showcase_classes() {
        if let Some(stem) = CacItemPresentation::prepare_weapon(preset.primary).archive_image() {
            stems.push(stem.to_owned());
        }
        if let Some(stem) = CacItemPresentation::prepare_weapon(preset.secondary).archive_image() {
            stems.push(stem.to_owned());
        }
        if let Some(stem) = CacItemPresentation::prepare_weapon(preset.lethal).archive_image() {
            stems.push(stem.to_owned());
        }
        if let Some(stem) = CacItemPresentation::prepare_weapon(preset.tactical).archive_image() {
            stems.push(stem.to_owned());
        }
        for attach in preset.primary_attachments {
            if let Some(stem) = CacItemPresentation::prepare_attachment(attach).archive_image() {
                stems.push(stem.to_owned());
            }
        }
        for attach in preset.secondary_attachments {
            if let Some(stem) = CacItemPresentation::prepare_attachment(attach).archive_image() {
                stems.push(stem.to_owned());
            }
        }
        for perk in preset.perks {
            if let Some(stem) = CacItemPresentation::prepare_perk(perk).archive_image() {
                stems.push(stem.to_owned());
            }
        }
        if let Some(stem) = CacItemPresentation::prepare_perk(preset.deathstreak).archive_image() {
            stems.push(stem.to_owned());
        }
    }
    stems.sort_unstable();
    stems.dedup();
    for stem in picker_icon_stems() {
        if !stems.contains(&stem) {
            stems.push(stem);
        }
    }

    let mut decoded = Vec::new();
    let mut missing = 0usize;
    for stem in stems {
        match asset_material::decode_ui_image(games, &stem) {
            Ok(Some((width, height, pixels))) => {
                decoded.push((stem.to_owned(), rgba_ui_image(width, height, pixels)));
            }
            Ok(None) => {
                missing += 1;
            }
            Err(error) => {
                diag::warn!(Ui, "class icons: {stem}: {error}");
                missing += 1;
            }
        }
    }
    diag::info!(
        Ui,
        "class icons: decoded {} from IWD ({missing} missing) in {:.1}ms on worker",
        decoded.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    decoded
}

pub(crate) fn rgba_ui_image(width: u32, height: u32, pixels: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}
