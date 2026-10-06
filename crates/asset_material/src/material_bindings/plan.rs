use super::{BindingWriter, MaterialFrameBindingInputs, float4_bits};
use bevy::math::Vec3;

#[derive(Clone, Debug)]
pub(super) enum FrameStep {
    Static(Vec<(u16, [u32; 4])>),
    EyeOffset(u16),
    ViewportToWorld,
    Fog(super::fog::FogPacking),
    Sun,
    LinearExposure {
        default: f32,
    },
    ExponentialExposure,
    LightSampleDecode,
    Sky {
        fixed: Vec<(u16, [u32; 4])>,
        targets: Vec<u16>,
    },
    Water {
        target: u16,
        angular_speed: f64,
    },
    TreeScatter,
}

pub(super) fn prepare(requested: &[u16]) -> Vec<FrameStep> {
    let mut steps = Vec::new();
    super::iw5::prepare(&mut steps, requested);
    super::t5::prepare_camera(&mut steps, requested);
    super::fog::prepare(&mut steps, requested);
    super::t5::prepare_sun(&mut steps, requested);
    super::t5::prepare_exposure(&mut steps, requested);
    super::t6::prepare_exposure(&mut steps, requested);
    super::sky::prepare(&mut steps, requested);
    super::t5::prepare_defaults(&mut steps, requested);
    super::t5::prepare_water(&mut steps, requested);
    super::t5::prepare_environment_defaults(&mut steps, requested);
    steps
}

impl FrameStep {
    pub(super) fn execute(
        &self,
        sources: &mut BindingWriter<'_>,
        inputs: &MaterialFrameBindingInputs,
    ) {
        match self {
            Self::Static(rows) => {
                for &(index, row) in rows {
                    sources.set_constant_rows(index, &[row]);
                }
            }
            Self::EyeOffset(index) => sources.set_constant_rows(
                *index,
                &[float4_bits([inputs.eye.x, inputs.eye.y, inputs.eye.z, 1.0])],
            ),
            Self::ViewportToWorld => super::t5::produce_leftover_t5_code_consts(
                sources,
                inputs.clip_from_view,
                inputs.world_from_view,
                inputs.target_size[0],
                inputs.target_size[1],
            ),
            Self::Fog(packing) => super::fog::produce(
                sources,
                &inputs.fog,
                inputs.eye.z,
                inputs.fog_enabled,
                *packing,
            ),
            Self::Sun => {
                if let Some(light) = &inputs.sun {
                    super::t5::produce_leftover_t5_sun_constants(sources, light);
                }
            }
            Self::LinearExposure { default } => super::t5::produce_leftover_t5_hdrcontrol(
                sources,
                inputs.exposure.unwrap_or(*default),
            ),
            Self::ExponentialExposure => {
                super::t6::produce_exposure(sources, inputs.world.exposure_stops)
            }
            Self::LightSampleDecode => super::t6::produce_sample_decode(sources, inputs),
            Self::Sky { fixed, targets } => {
                if let Some(authored) = inputs.world.sky_intensity {
                    let forward_z = inputs.world_from_view.transform_vector3(Vec3::NEG_Z).z;
                    super::sky::produce(sources, fixed, targets, authored, forward_z);
                }
            }
            Self::Water {
                target,
                angular_speed,
            } => {
                let phase = (*angular_speed * f64::from(inputs.time)) as f32;
                sources.set_constant_rows(*target, &[float4_bits([phase; 4])]);
            }
            Self::TreeScatter => {
                if let Some(scatter) = inputs.tree_scatter {
                    super::t5::produce_leftover_t5_treecanopy_parms(
                        sources, scatter[0], scatter[1],
                    );
                }
            }
        }
    }
}
