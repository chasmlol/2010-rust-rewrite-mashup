#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldDrawPolicy {
    pub family: asset_core::FamilyId,
    pub resolve_specular_env: bool,
    pub lightmap_requires_image: bool,
    pub decode_color_at_convert: bool,
    pub sun_sample_size_near: f32,
}

impl WorldDrawPolicy {
    pub const fn for_family(family: asset_core::FamilyId) -> Self {
        match family {
            asset_core::FamilyId::Iw4 => Self::iw4(),
            asset_core::FamilyId::Iw5 => Self::iw5(),
            asset_core::FamilyId::T5 => Self::t5(),
            asset_core::FamilyId::T6 => Self::t6(),
        }
    }
    pub const fn iw4() -> Self {
        Self {
            family: asset_core::FamilyId::Iw4,
            resolve_specular_env: true,
            lightmap_requires_image: false,
            decode_color_at_convert: false,
            sun_sample_size_near: 0.25,
        }
    }

    pub const fn t5() -> Self {
        Self {
            family: asset_core::FamilyId::T5,
            resolve_specular_env: false,
            lightmap_requires_image: true,
            decode_color_at_convert: true,
            sun_sample_size_near: 0.25,
        }
    }

    pub const fn iw5() -> Self {
        Self {
            family: asset_core::FamilyId::Iw5,
            resolve_specular_env: false,
            lightmap_requires_image: false,
            decode_color_at_convert: false,
            sun_sample_size_near: 0.25,
        }
    }

    pub const fn t6() -> Self {
        Self {
            family: asset_core::FamilyId::T6,
            sun_sample_size_near: 0.5,
            resolve_specular_env: true,
            lightmap_requires_image: false,
            decode_color_at_convert: false,
        }
    }
}
