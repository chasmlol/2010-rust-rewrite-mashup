use glam::{Mat4, Quat, Vec3};
pub use render_frame::code_math::{
    camera_relative_view_projection, code_transpose_matrix_row4, code_transpose_matrix_rows,
    float4_bits,
};
use render_frame::{LightAttenuationBind, OutdoorLookup};
use render_material::{
    CompiledConstantOverlay, MaterialRefusal, RuntimeCodeSources, RuntimeImageId,
    RuntimeMaterialCatalog,
};

pub const CODE_LIGHT_POSITION: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_POSITION;

pub const CODE_SPOT_SHADOWMAP_PIXEL_ADJUST: u16 = 0x35;

pub const CODE_LIGHT_DIFFUSE: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_DIFFUSE;

pub const CODE_LIGHT_SPECULAR: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPECULAR;

pub const CODE_LIGHT_SPOTDIR: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPOTDIR;

pub const CODE_LIGHT_SPOTFACTORS: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPOTFACTORS;

pub const CODE_LIGHT_FALLOFF_PLACEMENT: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_FALLOFF_PLACEMENT;

pub const CODE_BASE_LIGHTING_COORDS: u16 = 0x3a;

pub const CODE_LIGHT_PROBE_AMBIENT: u16 = 0x3b;

pub const CODE_MESH_ARG_0: u16 = 0x4a;

pub const CODE_MESH_ARG_1: u16 = 0x4b;
pub const CODE_TRANSPOSE_VIEW_PROJECTION: u16 = 0x56;

pub const CODE_TRANSPOSE_WORLD0: u16 = 0x62;

pub const CODE_TRANSPOSE_WORLD1: u16 = 0x6e;

pub const CODE_TRANSPOSE_WORLD2: u16 = 0x7a;

pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0: u16 = 0x6a;

pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION1: u16 = 0x76;
pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION2: u16 = 0x82;

pub const CODE_TRANSPOSE_WORLD_VIEW0: u16 = 0x66;
pub const CODE_TRANSPOSE_WORLD_VIEW1: u16 = 0x72;
pub const CODE_TRANSPOSE_WORLD_VIEW2: u16 = 0x7e;

pub const CODE_INVERSE_WORLD_VIEW0: u16 = 0x65;

pub const CODE_INVERSE_TRANSPOSE_WORLD_VIEW0: u16 = 0x67;

pub const CODE_MATERIAL_COLOR: u16 = 0x24;

pub const CODE_TRANSPOSE_PROJECTION: u16 = 0x52;

pub const CODE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP: u16 = 0x5e;

pub const CODE_TEXTURE_OUTDOOR: u32 = 0x0e;

pub const CODE_TEXTURE_OUTDOOR_SAMPLER: u8 = 0x62;

pub const CODE_DEPTH_FROM_CLIP: u16 = 0x49;
pub const CODE_TEXTURE_LIGHT_ATTENUATION: u32 =
    lighting_iw4::TEXTURE_SRC_CODE_LIGHT_ATTENUATION as u32;

pub const CODE_SHADOWMAP_POLYGON_OFFSET: u16 = 0x15;

pub const CODE_SHADOWMAP_LOOKUP_TRANSPOSE: u16 = 0x5a;

pub(crate) fn tess_base_lighting_handle(lighting_handle: u32) -> u16 {
    u16::try_from(lighting_handle)
        .ok()
        .filter(|&handle| handle != 0)
        .unwrap_or(lighting_iw4::model_lighting_handle_from_entry(0))
}

pub fn overlay_smodel_tess_code_constants(
    sources: &mut RuntimeCodeSources,
    lighting_handle: u32,
    inv_image_height: Option<f32>,
    packed_lighting: Option<[u8; 4]>,
    need: OverlayCodeNeed,
) {
    if need.lighting_coords
        && let Some(inv_h) = inv_image_height
        && let Some(coords) = lighting_iw4::model_lighting_coords_from_handle(
            tess_base_lighting_handle(lighting_handle),
            inv_h,
        )
    {
        sources.set_constant_rows(CODE_BASE_LIGHTING_COORDS, &[float4_bits(coords.as_array())]);
    }
    if need.light_probe
        && let Some(packed) = packed_lighting
    {
        sources.set_constant_rows(
            CODE_LIGHT_PROBE_AMBIENT,
            &[float4_bits(lighting_iw4::light_probe_ambient_from_packed(
                packed,
            ))],
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayCodeNeed {
    pub world0: bool,
    pub world_view: bool,
    pub inverse_world_view: bool,
    pub world_view_proj: bool,
    pub inverse_transpose_world_view: bool,
    pub lighting_coords: bool,
    pub light_probe: bool,
}

impl OverlayCodeNeed {
    pub const ALL: Self = Self {
        world0: true,
        world_view: true,
        inverse_world_view: true,
        world_view_proj: true,
        inverse_transpose_world_view: true,
        lighting_coords: true,
        light_probe: true,
    };
    pub const NONE: Self = Self {
        world0: false,
        world_view: false,
        inverse_world_view: false,
        world_view_proj: false,
        inverse_transpose_world_view: false,
        lighting_coords: false,
        light_probe: false,
    };

    pub fn note(&mut self, index: u16) {
        match index {
            CODE_TRANSPOSE_WORLD0 => self.world0 = true,
            CODE_TRANSPOSE_WORLD_VIEW0 => self.world_view = true,
            CODE_INVERSE_WORLD_VIEW0 => self.inverse_world_view = true,
            CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0 => self.world_view_proj = true,
            CODE_INVERSE_TRANSPOSE_WORLD_VIEW0 => self.inverse_transpose_world_view = true,
            CODE_BASE_LIGHTING_COORDS => self.lighting_coords = true,
            CODE_LIGHT_PROBE_AMBIENT => self.light_probe = true,
            _ => {}
        }
    }
}

pub fn overlay_smodel_world_matrix(
    sources: &mut RuntimeCodeSources,
    view_origin: Vec3,
    world_from_local: Mat4,
    clip_from_world: Option<Mat4>,
    view_from_world: Option<Mat4>,
    need: OverlayCodeNeed,
) {
    if need.world0 {
        sources.set_constant_rows(
            CODE_TRANSPOSE_WORLD0,
            &code_transpose_matrix_row4(Mat4::from_translation(-view_origin) * world_from_local),
        );
    }
    if need.world_view_proj
        && let Some(clip) = clip_from_world
    {
        sources.set_constant_rows(
            CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
            &code_transpose_matrix_row4(clip * world_from_local),
        );
    }
    if (need.world_view || need.inverse_world_view || need.inverse_transpose_world_view)
        && let Some(view) = view_from_world
    {
        let world_view = view * world_from_local;
        if need.world_view {
            sources.set_constant_rows(
                CODE_TRANSPOSE_WORLD_VIEW0,
                &code_transpose_matrix_row4(world_view),
            );
        }
        let inverse = (need.inverse_world_view || need.inverse_transpose_world_view)
            .then(|| world_view.inverse());
        if need.inverse_world_view {
            sources.set_constant_rows(
                CODE_INVERSE_WORLD_VIEW0,
                &code_transpose_matrix_row4(inverse.unwrap().transpose()),
            );
        }
        if need.inverse_transpose_world_view {
            sources.set_constant_rows(
                CODE_INVERSE_TRANSPOSE_WORLD_VIEW0,
                &code_transpose_matrix_row4(inverse.unwrap()),
            );
        }
    }
}

pub fn overlay_particle_cloud_tess_code_constants(
    sources: &mut RuntimeCodeSources,
    view_origin: Vec3,
    clouds: &[fx_iw4::GfxParticleCloud; 3],
    clip_from_world: Option<Mat4>,
    view_from_world: Option<Mat4>,
    clip_from_view: Option<Mat4>,
    outdoor: Option<&OutdoorLookup>,
) {
    let world_indices = [
        CODE_TRANSPOSE_WORLD0,
        CODE_TRANSPOSE_WORLD1,
        CODE_TRANSPOSE_WORLD2,
    ];
    let world_view_indices = [
        CODE_TRANSPOSE_WORLD_VIEW0,
        CODE_TRANSPOSE_WORLD_VIEW1,
        CODE_TRANSPOSE_WORLD_VIEW2,
    ];
    let wvp_indices = [
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION1,
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION2,
    ];
    if let Some(projection) = clip_from_view {
        sources.set_constant_rows(
            CODE_TRANSPOSE_PROJECTION,
            &code_transpose_matrix_row4(projection),
        );
    }
    if let Some(outdoor) = outdoor {
        if let Some(image) = outdoor.image {
            let _ = sources.set_texture_with_image(
                CODE_TEXTURE_OUTDOOR,
                CODE_TEXTURE_OUTDOOR_SAMPLER,
                image,
            );
        }
    }
    for (slot, cloud) in clouds.iter().enumerate() {
        let world_from_local = particle_cloud_world_from_local(cloud);
        sources.set_constant_rows(
            world_indices[slot],
            &code_transpose_matrix_row4(Mat4::from_translation(-view_origin) * world_from_local),
        );
        if let Some(view) = view_from_world {
            sources.set_constant_rows(
                world_view_indices[slot],
                &code_transpose_matrix_row4(view * world_from_local),
            );
        }
        if let Some(clip) = clip_from_world {
            sources.set_constant_rows(
                wvp_indices[slot],
                &code_transpose_matrix_row4(clip * world_from_local),
            );
        }
        let matrix = fx_iw4::particle_cloud_matrix_diag(cloud.size0, cloud.size1);
        sources.set_constant_rows(
            u16::from(fx_iw4::FX_CODE_PARTICLE_CLOUD_MATRIX0) + slot as u16,
            &[float4_bits(matrix)],
        );
        let color = fx_iw4::particle_cloud_color_const(cloud.color);
        sources.set_constant_rows(
            u16::from(fx_iw4::FX_CODE_SPARK_COLOR0) + slot as u16,
            &[float4_bits(color)],
        );
        if slot == 0 {
            sources.set_constant_rows(
                u16::from(fx_iw4::FX_CODE_PARTICLE_CLOUD_COLOR),
                &[float4_bits(color)],
            );
            sources.set_constant_rows(
                u16::from(fx_iw4::FX_CODE_FOUNTAIN_PARM0),
                &[float4_bits(fx_iw4::particle_fountain_parm0(cloud.scale))],
            );
            sources.set_constant_rows(
                u16::from(fx_iw4::FX_CODE_FOUNTAIN_PARM1),
                &[float4_bits([0.0, 0.0, -1.0, view_origin.z])],
            );
            if let Some(outdoor) = outdoor {
                let lookup = d3d_row_major_bits_to_mat4(outdoor.lookup);

                let derived = lookup * world_from_local;
                sources.set_constant_rows(
                    CODE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP,
                    &code_transpose_matrix_row4(derived)[..3],
                );
            }
        }
    }
}

fn d3d_row_major_bits_to_mat4(m: [u32; 16]) -> Mat4 {
    Mat4::from_cols_array(&m.map(f32::from_bits))
}

fn particle_cloud_world_from_local(cloud: &fx_iw4::GfxParticleCloud) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::splat(cloud.placement_scale),
        Quat::from_xyzw(cloud.quat[0], cloud.quat[1], cloud.quat[2], cloud.quat[3]),
        Vec3::from_array(cloud.pos),
    )
}

pub fn overlay_code_mesh_tess_code_constants(sources: &mut RuntimeCodeSources, args: &[[f32; 4]]) {
    for (i, row) in args.iter().take(2).enumerate() {
        sources.set_constant_rows(CODE_MESH_ARG_0 + i as u16, &[float4_bits(*row)]);
    }
}

fn overlay_shadow_lookup(sources: &mut RuntimeCodeSources, lookup: &[f32; 16]) {
    sources.set_constant_rows(
        CODE_SHADOWMAP_LOOKUP_TRANSPOSE,
        &[
            float4_bits([lookup[0], lookup[1], lookup[2], lookup[3]]),
            float4_bits([lookup[4], lookup[5], lookup[6], lookup[7]]),
            float4_bits([lookup[8], lookup[9], lookup[10], lookup[11]]),
            float4_bits([lookup[12], lookup[13], lookup[14], lookup[15]]),
        ],
    );
}

pub fn apply_shadowable_light(
    sources: &mut RuntimeCodeSources,
    packed_drawsurf: u64,
    lights: &[lighting_iw4::GfxLightPack],
    attenuation: &[LightAttenuationBind],
    local_light_bindings: &[CompiledConstantOverlay],
    catalog: &RuntimeMaterialCatalog,
    eye: Vec3,
    spot_receivers: &[Option<render_frame::SpotShadowReceiver>],
) -> Result<(), MaterialRefusal> {
    let index = dpvs_iw4::GfxDrawSurf {
        packed: packed_drawsurf,
    }
    .scene_light_index();
    let packed = lighting_iw4::pack_shadowable_light(
        index,
        lights.get(usize::from(index)),
        [eye.x, eye.y, eye.z],
        lighting_iw4::R_COLOR_SCALE_DEFAULT,
        lighting_iw4::R_COLOR_SCALE_DEFAULT,
    );
    let bindings = if matches!(packed, lighting_iw4::ShadowableLightPack::Unchanged) {
        None
    } else {
        let bindings = local_light_bindings
            .get(usize::from(index))
            .ok_or(MaterialRefusal::MissingLightBindings { scene_light: index })?;
        bindings.validate_generation(catalog)?;
        Some(bindings)
    };
    match packed {
        lighting_iw4::ShadowableLightPack::Unchanged => {}
        lighting_iw4::ShadowableLightPack::Dir {
            position,
            diffuse,
            specular,
        } => {
            sources.set_constant_rows(CODE_LIGHT_POSITION, &[float4_bits(position)]);
            sources.set_constant_rows(CODE_LIGHT_DIFFUSE, &[float4_bits(diffuse)]);
            sources.set_constant_rows(CODE_LIGHT_SPECULAR, &[float4_bits(specular)]);
        }
        lighting_iw4::ShadowableLightPack::Omni {
            position,
            diffuse,
            specular,
            spot_dir,
            falloff_placement,
        } => {
            sources.set_constant_rows(CODE_LIGHT_POSITION, &[float4_bits(position)]);
            sources.set_constant_rows(CODE_LIGHT_DIFFUSE, &[float4_bits(diffuse)]);
            sources.set_constant_rows(CODE_LIGHT_SPECULAR, &[float4_bits(specular)]);
            sources.set_constant_rows(CODE_LIGHT_SPOTDIR, &[float4_bits(spot_dir)]);
            overlay_falloff_placement(sources, falloff_placement);
            overlay_attenuation(sources, attenuation.get(usize::from(index)));
        }
        lighting_iw4::ShadowableLightPack::Spot {
            position,
            diffuse,
            specular,
            spot_dir,
            spot_factors,
            falloff_placement,
        } => {
            sources.set_constant_rows(CODE_LIGHT_POSITION, &[float4_bits(position)]);
            sources.set_constant_rows(CODE_LIGHT_DIFFUSE, &[float4_bits(diffuse)]);
            sources.set_constant_rows(CODE_LIGHT_SPECULAR, &[float4_bits(specular)]);
            sources.set_constant_rows(CODE_LIGHT_SPOTDIR, &[float4_bits(spot_dir)]);
            sources.set_constant_rows(CODE_LIGHT_SPOTFACTORS, &[float4_bits(spot_factors)]);
            overlay_falloff_placement(sources, falloff_placement);
            overlay_attenuation(sources, attenuation.get(usize::from(index)));
        }
    }

    if matches!(
        packed,
        lighting_iw4::ShadowableLightPack::Omni { .. }
            | lighting_iw4::ShadowableLightPack::Spot { .. }
    ) && let Some(receiver) = spot_receivers.get(usize::from(index)).copied().flatten()
    {
        overlay_shadow_lookup(sources, &receiver.lookup);
        sources.set_constant_rows(
            CODE_SPOT_SHADOWMAP_PIXEL_ADJUST,
            &[float4_bits(receiver.pixel_adjust)],
        );
    }
    if let Some(bindings) = bindings {
        bindings.apply(catalog, sources)?;
    }
    Ok(())
}

fn overlay_falloff_placement(sources: &mut RuntimeCodeSources, row: Option<[f32; 4]>) {
    let Some(row) = row else {
        return;
    };
    sources.set_constant_rows(CODE_LIGHT_FALLOFF_PLACEMENT, &[float4_bits(row)]);
}

fn overlay_attenuation(sources: &mut RuntimeCodeSources, bind: Option<&LightAttenuationBind>) {
    let Some(bind) = bind else {
        return;
    };
    let Some(id) = bind.image else {
        return;
    };
    let _ = sources.set_texture_with_image(
        CODE_TEXTURE_LIGHT_ATTENUATION,
        bind.sampler,
        RuntimeImageId(id),
    );
}

pub fn produce_depth_from_clip(sources: &mut RuntimeCodeSources, viewmodel: bool) {
    let w = if viewmodel { -1.0 } else { 1.0 };
    sources.set_constant_rows(CODE_DEPTH_FROM_CLIP, &[float4_bits([0.0, 0.0, 0.0, w])]);
}

pub fn overlay_viewmodel_depth_hack(
    sources: &mut RuntimeCodeSources,
    viewmodel_clip_from_world: Mat4,
    view_origin: Vec3,
    world_from_local: Mat4,
) {
    produce_depth_from_clip(sources, true);
    sources.set_constant_rows(
        CODE_TRANSPOSE_VIEW_PROJECTION,
        &code_transpose_matrix_row4(camera_relative_view_projection(
            viewmodel_clip_from_world,
            view_origin,
        )),
    );
    sources.set_constant_rows(
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
        &code_transpose_matrix_row4(viewmodel_clip_from_world * world_from_local),
    );
}
