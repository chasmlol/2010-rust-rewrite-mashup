use bevy::prelude::*;

use crate::ModelLightingRequest;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XModelSurfaceDraw {
    pub surface: u32,
    pub material: u32,
    pub world_from_local: Mat4,
    pub lighting_handle: u32,

    pub pending_lighting: Option<ModelLightingRequest>,

    pub colour_refusal: Option<XModelColourRefusal>,

    pub object_id: u16,

    pub scene_light_index: u8,

    pub reflection_probe_index: u8,

    pub packed_lighting: Option<[u8; 4]>,

    pub is_scope: bool,

    pub scene_entnum: Option<u32>,
    pub body_client: Option<u32>,

    /// World-space bounding sphere of the object this surface belongs to.
    /// `None` is "unbounded": the surface is admitted to every shadow
    /// partition, which is what a producer that cannot state a bound gets.
    pub caster_bound: Option<XModelCasterBound>,
}

/// A shadow caster's world-space bounding sphere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XModelCasterBound {
    pub origin: [f32; 3],
    pub radius: f32,
}

impl XModelCasterBound {
    /// `true` when the whole sphere is behind at least one plane, so no part of
    /// it is inside the volume the planes bound. Planes face inwards and carry
    /// their distance in `[3]`, as `dpvs_iw4::bounds_culled` reads them.
    #[must_use]
    pub fn outside(self, planes: &[[f32; 4]]) -> bool {
        planes.iter().any(|plane| {
            plane[0] * self.origin[0]
                + plane[1] * self.origin[1]
                + plane[2] * self.origin[2]
                + plane[3]
                + self.radius
                <= 0.0
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XModelColourRefusal {
    CameraFrustum,
}
