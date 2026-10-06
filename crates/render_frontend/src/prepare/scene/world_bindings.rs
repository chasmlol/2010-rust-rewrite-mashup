use asset_material::MaterialWorldBindingInputs;
use asset_model::LightGridColorEncoding;

pub(super) fn prepare(
    source_namespace: Option<asset_core::AssetNamespace>,
    encoding: LightGridColorEncoding,
    exposure_stops: Option<f32>,
    sky_intensity: Option<[f32; 4]>,
) -> MaterialWorldBindingInputs {
    MaterialWorldBindingInputs {
        source_namespace,
        exposure_stops,
        model_lighting_decode_scale: asset_model::model_lighting_texel_decode_scale(encoding),
        reflection_probe_alpha_weight: f32::from(
            encoding == LightGridColorEncoding::T6Coefficients,
        ),
        sky_intensity,
    }
}

pub(crate) fn fog(fog: &asset_world::ExpFog) -> asset_material::MaterialFogInputs {
    asset_material::MaterialFogInputs {
        color_rgb: fog.color_rgb,
        max_opacity: fog.max_opacity,
        halfway_dist: fog.halfway_dist,
        start_dist: fog.start_dist,
        volumetric: fog
            .volumetric
            .map(|v| asset_material::MaterialFogVolumeInputs {
                halfway_height: v.halfway_height,
                base_height: v.base_height,
                color_scale: v.color_scale,
            }),
        sun: fog.sun.map(|s| asset_material::MaterialFogSunInputs {
            color_rgb: s.color_rgb,
            sun_dir: s.sun_dir,
            begin_angle_deg: s.begin_angle_deg,
            end_angle_deg: s.end_angle_deg,
        }),
    }
}

pub(crate) fn sun(light: &super::world::MapDirPrimaryLight) -> asset_material::MaterialSunInputs {
    asset_material::MaterialSunInputs {
        direction: light.direction,
        color: light.color,
        diffuse_color: light.t5_diffuse_color,
        specular_color: light.t5_specular_color,
    }
}
