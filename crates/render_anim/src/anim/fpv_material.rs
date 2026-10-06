use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn admit_surface(
    global: &RuntimeMaterialCatalog,
    images: &[Option<Handle<Image>>],
    entry: &asset_model::FpvMeshEntry,
    surface_index: usize,
    lighting: &WorldModelLightingAtlas,
    admission: &mut FpvMaterialAdmission,
    image_cache: &mut HashMap<u32, Handle<Image>>,
) -> FpvSurfaceVerdict {
    let edge = entry
        .material_edges
        .get(surface_index)
        .copied()
        .unwrap_or(AssetEdge::Absent);
    let leftover = entry
        .material_keys
        .get(surface_index)
        .and_then(|key| Some(key.as_ref()?.name.as_str()))
        .unwrap_or("-");
    let refused = |material: &str, cause: &'static str| FpvSurfaceVerdict::Refused {
        material: material.to_owned(),
        cause,
    };
    let bound = match edge {
        AssetEdge::Absent => return FpvSurfaceVerdict::Inapplicable("no material"),
        AssetEdge::Unresolved(_) => return refused(leftover, edge.edge_kind()),
        AssetEdge::Bound(_) => match edge.bound_index() {
            Some(bound) => bound,
            None => return refused(leftover, "bound edge without an index"),
        },
    };
    let Some(present_name) = entry.material_present_name(surface_index) else {
        return refused(leftover, "bound material has no name");
    };
    admit_material(
        global,
        images,
        bound,
        present_name,
        lighting,
        admission,
        image_cache,
    )
}

pub(super) fn admit_material(
    global: &RuntimeMaterialCatalog,
    images: &[Option<Handle<Image>>],
    mat_i: usize,
    present_name: &str,
    lighting: &WorldModelLightingAtlas,
    admission: &mut FpvMaterialAdmission,
    image_cache: &mut HashMap<u32, Handle<Image>>,
) -> FpvSurfaceVerdict {
    use lighting_iw4::{
        MODEL_LIGHTING_INV_ATLAS_WIDTH, MODEL_LIGHTING_VOLUME_W, model_lighting_inv_image_height,
        model_lighting_lookup_scale,
    };

    let refused = |material: &str, cause: &'static str| FpvSurfaceVerdict::Refused {
        material: material.to_owned(),
        cause,
    };
    if let Some(&row) = admission.by_authored.get(&mat_i) {
        return FpvSurfaceVerdict::Admitted(row);
    }
    let Some(authored) = global.parts().materials.get(mat_i) else {
        return refused(present_name, "material outside the session catalog");
    };
    let Some(ordinal) = global.ordinal_for_asset_id(assets::MaterialIndex::from_order(mat_i))
    else {
        return refused(present_name, "material has no sorted ordinal");
    };
    let Some(color_image) = authored
        .textures
        .iter()
        .find_map(|(_, texture)| texture.filter(|binding| binding.semantic == TS_COLOR_MAP))
        .map(|binding| binding.image.0)
    else {
        return FpvSurfaceVerdict::Inapplicable(NO_COLOUR_MAP);
    };
    let mut image = |image_index: u32| -> Option<Handle<Image>> {
        if let Some(cached) = image_cache.get(&image_index) {
            return Some(cached.clone());
        }
        let handle = images.get(image_index as usize).cloned().flatten()?;
        image_cache.insert(image_index, handle.clone());
        Some(handle)
    };
    let Some(color) = image(color_image) else {
        return refused(present_name, "colour map not uploaded");
    };
    let specular = authored
        .textures
        .iter()
        .find_map(|(_, texture)| {
            texture.filter(|binding| binding.semantic == asset_material::TS_SPECULAR_MAP)
        })
        .and_then(|binding| image(binding.image.0));

    const ENV_MAP_PARMS: u32 = 1_033_475_292;
    let env_map_parms = authored
        .constants
        .iter()
        .find(|(hash, _)| *hash == ENV_MAP_PARMS)
        .map(|(_, words)| words.map(f32::from_bits))
        .unwrap_or([0.0; 4]);
    let Some(inv_h) = model_lighting_inv_image_height(lighting.dims.image_height) else {
        return refused(present_name, "model lighting atlas has no rows");
    };
    let scale = model_lighting_lookup_scale(inv_h);
    let cull_mode = authored
        .pass_states
        .first()
        .and_then(|state| match state.cull() {
            1 => Some(bevy::render::render_resource::Face::Back),
            2 => Some(bevy::render::render_resource::Face::Front),
            _ => None,
        });
    let material = SmodelPassMaterial {
        model_lighting_required: true,
        color: Some(color),
        specular,
        probe: None,
        atlas: Some(lighting.image.clone()),
        alpha_mode: AlphaMode::Opaque,
        draw_mode: None,
        cull_mode,
        env_map_parms,
        lighting_lookup_scale: [scale.u, scale.v, scale.w, scale.q],
        atlas_lookup: [
            MODEL_LIGHTING_INV_ATLAS_WIDTH as f32,
            inv_h,
            MODEL_LIGHTING_VOLUME_W,
            0.0,
        ],
        sort_key: authored.sort_key,
        material_sorted_index: Some(ordinal.get()),
    };
    let row = admission.materials.len() as u32;
    admission.materials.push(material);
    admission.by_authored.insert(mat_i, row);
    FpvSurfaceVerdict::Admitted(row)
}
