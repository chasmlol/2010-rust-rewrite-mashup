pub use super::sun_shadow_clip::{
    SunShadowClipInput, SunShadowFrustumRays, sun_shadow_clip_planes, sun_shadow_frustum_rays,
};
pub use render_frame::{
    SUN_SHADOW_FORCED_PROFILE, SunShadowClipPlanes, SunShadowForcedFrame, SunShadowPartition,
    SunShadowReceiverConstants,
};
pub use render_frame::{SUN_SHADOW_PARTITION_COUNT, SunShadowAtlasProfile, SunShadowViewport};

#[derive(Default)]
pub(crate) struct SunShadowStaging {
    pub surface_vis: [Vec<u8>; 2],
    pub smodel_vis: [Vec<u8>; 2],
    pub frustum_draw_msb: [Vec<u32>; 2],
    pub draw_cell_n: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SunShadowMapMetrics {
    pub pixels_per_tile: u32,

    pub tiles_per_texture: u32,

    pub min_coord: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunShadowProjectionSetup {
    pub near_shadow_min_dist: f32,

    pub shadow_org_pixel_center: [f32; 2],

    pub snapped_shadow_org: [[f32; 2]; 2],

    pub sample_size: [f32; 2],
}

impl SunShadowProjectionSetup {
    pub fn partition_fraction(
        self,
        view_origin: [f32; 3],
        view_axis_forward: [f32; 3],
    ) -> [f32; 4] {
        let scale = 1.0 / self.near_shadow_min_dist;
        let xyz = [
            view_axis_forward[0] * scale,
            view_axis_forward[1] * scale,
            view_axis_forward[2] * scale,
        ];
        [
            xyz[0],
            xyz[1],
            xyz[2],
            -(view_origin[0] * xyz[0] + view_origin[1] * xyz[1] + view_origin[2] * xyz[2]),
        ]
    }

    pub fn receiver_constants(
        self,
        metrics: SunShadowMapMetrics,
        use_shadow_offset: bool,
        shadowmap_scale: f32,
    ) -> SunShadowReceiverConstants {
        let pixels_per_tile = metrics.pixels_per_tile as f32;
        let tiles_per_texture = metrics.tiles_per_texture as f32;
        let min_coord = metrics.min_coord as f32;
        let sample_size_far = self.sample_size[1];
        let snapped_delta_x = self.snapped_shadow_org[0][0] - self.snapped_shadow_org[1][0];
        let snapped_delta_y = self.snapped_shadow_org[0][1] - self.snapped_shadow_org[1][1];

        let switch_x = (0.75 * (min_coord + self.shadow_org_pixel_center[0])
            + snapped_delta_x / sample_size_far)
            / pixels_per_tile;
        let switch_y = (1.75
            - (0.75 * (min_coord + self.shadow_org_pixel_center[1])
                + snapped_delta_y / sample_size_far)
                / pixels_per_tile)
            / tiles_per_texture;

        SunShadowReceiverConstants {
            switch_partition: [
                switch_x,
                switch_y,
                if use_shadow_offset { 0.0 } else { 1.0 },
                0.25,
            ],
            shadowmap_scale: [
                16.0 / shadowmap_scale,
                (16.0 * tiles_per_texture) / shadowmap_scale,
                -8.0 / shadowmap_scale,
                -24.0 / shadowmap_scale,
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SunShadowSamplingPath {
    DepthTexture,

    Fallback,
}

impl SunShadowSamplingPath {
    pub const fn code_image_sampler(self) -> u8 {
        match self {
            Self::DepthTexture => 0x62,
            Self::Fallback => 0x61,
        }
    }
}

pub const CODE_TEXTURE_SHADOWMAP_SUN: u32 = 6;

pub const CODE_TEXTURE_SHADOWMAP_SPOT: u32 = 7;

pub const CODE_SHADOWMAP_POLYGON_OFFSET: u16 = 0x15;

pub const CODE_SHADOWMAP_SWITCH_PARTITION: u16 = 0x1f;

pub const CODE_SHADOWMAP_SCALE: u16 = 0x20;

pub const CODE_SUN_SHADOWMAP_PIXEL_ADJUST: u16 = 0x34;

pub const CODE_SHADOWMAP_LOOKUP_TRANSPOSE: u16 = 0x5a;

pub const SUN_SHADOW_QUARTER: f32 = 0.25;

pub const SUN_SHADOW_HALF: f32 = 0.5;

pub const SUN_SHADOW_NEG_QUARTER: f32 = -0.25;

pub const SUN_SHADOW_SAMPLE_EXTENT_PIXELS: f32 = 1024.0;

pub const SM_SUN_PARTITION_RATIO: f32 = 4.0;

pub const SUN_SHADOW_CASTER_TECH: u8 = 3;

pub const STATIC_MODEL_FLAG_NO_CAST_SHADOW: u8 = 0x10;

pub fn sun_shadowmap_pixel_adjust(profile: SunShadowAtlasProfile) -> [f32; 4] {
    let partition = profile.partition_size() as f32;
    let atlas_width = profile.atlas_width() as f32;
    [
        SUN_SHADOW_QUARTER / partition,
        SUN_SHADOW_HALF / atlas_width,
        SUN_SHADOW_HALF / partition,
        SUN_SHADOW_NEG_QUARTER / atlas_width,
    ]
}

pub fn sun_shadow_lookup_matrix(
    view_projection: bevy::math::Mat4,
    partition_fraction: [f32; 4],
    profile: SunShadowAtlasProfile,
) -> bevy::math::Mat4 {
    let partition = profile.partition_size() as f32;
    let atlas_width = profile.atlas_width() as f32;
    let atlas_columns = profile.atlas_columns() as f32;
    let x_scale = SUN_SHADOW_HALF;
    let x_shift = SUN_SHADOW_HALF + SUN_SHADOW_HALF / partition;
    let y_scale = -SUN_SHADOW_HALF / atlas_columns;
    let y_shift = SUN_SHADOW_HALF / atlas_columns + SUN_SHADOW_HALF / atlas_width;
    let cols = view_projection.to_cols_array_2d();

    let mut out = [[0.0f32; 4]; 4];
    for i in 0..4 {
        let vp_0i = cols[i][0];
        let vp_1i = cols[i][1];
        let vp_2i = cols[i][2];
        let vp_3i = cols[i][3];
        out[0][i] = vp_0i * x_scale + vp_3i * x_shift;
        out[1][i] = vp_1i * y_scale + vp_3i * y_shift;
        out[2][i] = vp_2i;
        out[3][i] = partition_fraction[i];
    }
    bevy::math::Mat4::from_cols_array_2d(&out)
}

pub const SUN_SHADOW_STAND_EYE_TO_FEET: f32 = 60.0;

pub const SUN_SHADOW_PAVEMENT_ALONG: f32 = 200.0;

pub const SUN_SHADOW_LOOK_ALONG: f32 = 80.0;

pub const SUN_SHADOW_RIGHT_ALONG: f32 = 40.0;

pub fn shadowmap_polygon_offset(projection_m22: f32) -> [f32; 4] {
    [
        asset_iw4::SM_POLYGON_OFFSET_BIAS_DEFAULT * SUN_SHADOW_QUARTER * projection_m22,
        asset_iw4::SM_POLYGON_OFFSET_SCALE_DEFAULT,
        0.0,
        0.0,
    ]
}

pub fn sun_shadow_forward_from_light_dir(dir: [f32; 3]) -> [f32; 3] {
    [-dir[0], -dir[1], -dir[2]]
}

pub const SUN_AXIS_HELPER_Z_MAX: f32 = 0.9;

pub fn sun_shadow_packed_axes_from_light_dir(dir: [f32; 3]) -> [[f32; 3]; 3] {
    let dir = bevy::math::Vec3::from_array(dir);
    let dir = if dir.length_squared() > 1e-12 {
        dir.normalize()
    } else {
        bevy::math::Vec3::Z
    };
    let helper = if dir.z.abs() <= SUN_AXIS_HELPER_Z_MAX {
        bevy::math::Vec3::Z
    } else {
        bevy::math::Vec3::X
    };
    let axis0 = helper.cross(dir);
    let axis0 = if axis0.length_squared() > 1e-12 {
        axis0.normalize()
    } else {
        bevy::math::Vec3::X
    };
    let axis1 = dir.cross(axis0);
    [axis0.to_array(), axis1.to_array(), dir.to_array()]
}

pub fn sun_axes_from_dir(direction: [f32; 3]) -> [[f32; 3]; 3] {
    let light = [-direction[0], -direction[1], -direction[2]];
    sun_axes_from_light_dir(light)
}

pub fn sun_axes_from_light_dir(dir: [f32; 3]) -> [[f32; 3]; 3] {
    let packed = sun_shadow_packed_axes_from_light_dir(dir);
    let axis0 = bevy::math::Vec3::from_array(packed[0]);
    let axis1 = bevy::math::Vec3::from_array(packed[1]);
    let axis2 = bevy::math::Vec3::from_array(packed[2]);
    [(-axis2).to_array(), (-axis0).to_array(), axis1.to_array()]
}

pub fn sun_shadow_view_matrix(
    sun_axes: [[f32; 3]; 3],
    snapped_sun_proj: [f32; 2],
) -> bevy::math::Mat4 {
    let right = bevy::math::Vec3::from_array(sun_axes[1]);
    let up = bevy::math::Vec3::from_array(sun_axes[2]);
    let forward = bevy::math::Vec3::from_array(sun_axes[0]);
    let x = -right;
    bevy::math::Mat4::from_cols(
        bevy::math::Vec4::new(x.x, up.x, forward.x, 0.0),
        bevy::math::Vec4::new(x.y, up.y, forward.y, 0.0),
        bevy::math::Vec4::new(x.z, up.z, forward.z, 0.0),
        bevy::math::Vec4::new(-snapped_sun_proj[0], -snapped_sun_proj[1], 0.0, 1.0),
    )
}

pub fn sun_shadow_ortho_projection(
    snapped_clip: [f32; 2],
    sample_size: f32,
    near_clip: f32,
    far_clip: f32,
) -> bevy::math::Mat4 {
    let extents = sample_size * SUN_SHADOW_SAMPLE_EXTENT_PIXELS;
    let m00 = 2.0 / extents;
    let m22 = sun_shadow_projection_m22(near_clip, far_clip);
    let m32 = sun_shadow_projection_m32(near_clip, far_clip);
    bevy::math::Mat4::from_cols(
        bevy::math::Vec4::new(m00, 0.0, 0.0, 0.0),
        bevy::math::Vec4::new(0.0, m00, 0.0, 0.0),
        bevy::math::Vec4::new(0.0, 0.0, m22, 0.0),
        bevy::math::Vec4::new(snapped_clip[0], snapped_clip[1], m32, 1.0),
    )
}

pub use crate::prepare::scene::frustum::clip_from_world_frustum_planes;

pub fn sun_shadow_projection_m22(near_clip: f32, far_clip: f32) -> f32 {
    1.0 / (far_clip - near_clip + 2.0)
}

pub fn sun_shadow_projection_m32(near_clip: f32, far_clip: f32) -> f32 {
    -(near_clip - 1.0) * sun_shadow_projection_m22(near_clip, far_clip)
}

pub fn view_org_in_sun_proj(origin: [f32; 3], sun_axes: [[f32; 3]; 3]) -> [f32; 2] {
    let o = bevy::math::Vec3::from_array(origin);
    let right = bevy::math::Vec3::from_array(sun_axes[1]);
    let up = bevy::math::Vec3::from_array(sun_axes[2]);
    [-o.dot(right), o.dot(up)]
}

pub fn snap_sun_proj_origin(origin: [f32; 2], scale: f32) -> [f32; 2] {
    [
        ((origin[0] / scale).floor() + SUN_SHADOW_HALF) * scale,
        ((origin[1] / scale).floor() + SUN_SHADOW_HALF) * scale,
    ]
}

pub fn scene_extents_along_dir(forward: [f32; 3], mid: [f32; 3], half: [f32; 3]) -> (f32, f32) {
    let mut center = 0.0f32;
    let mut extent = 0.0f32;
    for axis in 0..3 {
        center += forward[axis] * mid[axis];
        extent += forward[axis].abs() * half[axis];
    }
    (center - extent, center + extent)
}

pub fn tan_half_fov_from_clip(clip_from_view: bevy::math::Mat4) -> (f32, f32) {
    let tx = if clip_from_view.x_axis.x.abs() > 1e-12 {
        1.0 / clip_from_view.x_axis.x
    } else {
        1.0
    };
    let ty = if clip_from_view.y_axis.y.abs() > 1e-12 {
        1.0 / clip_from_view.y_axis.y
    } else {
        1.0
    };
    (tx, ty)
}

#[derive(Clone, Copy, Debug)]
pub struct SunShadowCamera {
    pub origin: [f32; 3],
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub tan_half_fov_x: f32,
    pub tan_half_fov_y: f32,

    pub z_near: f32,
}

#[derive(Clone, Copy, Debug)]
struct SunShadowProjectionFit {
    pixel_center: [f32; 2],
    scale: [f32; 2],
    min_forward_dot: f32,
}

/// The four corner rays of the camera frustum, in the winding the arc
/// classification expects: adjacent indices share a frustum edge.
fn sun_shadow_corner_rays(camera: SunShadowCamera) -> [[f32; 3]; 4] {
    let fwd = bevy::math::Vec3::from_array(camera.forward);
    let right = bevy::math::Vec3::from_array(camera.right);
    let up = bevy::math::Vec3::from_array(camera.up);
    let mut rays = [[0.0f32; 3]; 4];
    for (index, (sx, sy)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
        .into_iter()
        .enumerate()
    {
        let p = fwd + right * (sx * camera.tan_half_fov_x) + up * (sy * camera.tan_half_fov_y);
        let p = if p.length_squared() > 1e-12 {
            p.normalize()
        } else {
            fwd
        };
        rays[index] = p.to_array();
    }
    rays
}

fn sun_shadow_projection_fit(
    rays: &SunShadowFrustumRays,
    view_forward: [f32; 3],
    pixels_per_tile: f32,
) -> SunShadowProjectionFit {
    let fwd = bevy::math::Vec3::from_array(view_forward);
    let min_forward_dot = rays
        .world_rays
        .iter()
        .map(|ray| bevy::math::Vec3::from_array(*ray).dot(fwd))
        .fold(f32::INFINITY, f32::min);
    let mins = rays.mins;
    let maxs = rays.maxs;
    let mut pixel_center = [0.0; 2];
    let mut scale = [0.0; 2];
    for axis in 0..2 {
        let span = maxs[axis] - mins[axis];
        assert!(span > 0.0, "sun-shadow frustum projection has zero span");
        let initial_scale = (pixels_per_tile - 2.0) / span;
        let leading = (0.5 - initial_scale * mins[axis]).ceil() + 0.5;
        let trailing = (pixels_per_tile - 0.5 - initial_scale * maxs[axis]).floor() - 0.5;
        if trailing != leading {
            if maxs[axis] <= -mins[axis] {
                pixel_center[axis] = leading - 1.0;
                scale[axis] = (1.0 - pixel_center[axis]) / mins[axis];
            } else {
                pixel_center[axis] = trailing + 1.0;
                scale[axis] = (pixels_per_tile - 1.0 - pixel_center[axis]) / maxs[axis];
            }
        } else {
            pixel_center[axis] = leading;
            scale[axis] = initial_scale;
        }
    }
    SunShadowProjectionFit {
        pixel_center,
        scale,
        min_forward_dot,
    }
}

pub const fn smodel_casts_sun_shadow(flags: u8) -> bool {
    flags & STATIC_MODEL_FLAG_NO_CAST_SHADOW == 0
}

pub fn sun_shadow_cutout_name(name: &str) -> bool {
    let n = name.as_bytes();
    contains_ignore_ascii_case(n, b"chainlink")
        || contains_ignore_ascii_case(n, b"chain_link")
        || contains_ignore_ascii_case(n, b"chain-link")
        || contains_ignore_ascii_case(n, b"fence")
        || contains_ignore_ascii_case(n, b"grate")
        || contains_ignore_ascii_case(n, b"grating")
        || contains_ignore_ascii_case(n, b"razorwire")
}

fn contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

pub const fn forced_fallback_metrics() -> SunShadowMapMetrics {
    SunShadowMapMetrics {
        pixels_per_tile: SUN_SHADOW_FORCED_PROFILE.partition_size(),
        tiles_per_texture: SUN_SHADOW_FORCED_PROFILE.atlas_columns(),
        min_coord: 0,
    }
}

pub fn forced_fallback_frame(
    sun_direction: [f32; 3],
    camera: SunShadowCamera,

    world_mid: [f32; 3],

    world_half: [f32; 3],

    sample_near: f32,
) -> SunShadowForcedFrame {
    let profile = SUN_SHADOW_FORCED_PROFILE;
    let axes = sun_axes_from_dir(sun_direction);
    let packed_axes = sun_shadow_packed_axes_from_light_dir([
        -sun_direction[0],
        -sun_direction[1],
        -sun_direction[2],
    ]);
    let view_origin = camera.origin;
    let org = view_org_in_sun_proj(view_origin, axes);
    let sample_far = sample_near * SM_SUN_PARTITION_RATIO;
    let sample_size = [sample_near, sample_far];
    let corner_rays = sun_shadow_corner_rays(camera);
    let rays = sun_shadow_frustum_rays(corner_rays, packed_axes);
    let fit = sun_shadow_projection_fit(&rays, camera.forward, profile.partition_size() as f32);
    let snapped = [
        snap_sun_proj_origin(org, sample_near),
        snap_sun_proj_origin(org, sample_far),
    ];
    let projection_offset_clip = [
        fit.pixel_center[0] * 2.0 / profile.partition_size() as f32 - 1.0,
        fit.pixel_center[1] * 2.0 / profile.partition_size() as f32 - 1.0,
    ];
    let near_shadow_min_dist = sample_near * fit.scale[0].min(fit.scale[1]) * fit.min_forward_dot;
    assert!(
        near_shadow_min_dist > 0.0,
        "sun-shadow near partition has no forward extent"
    );
    let (near_clip, far_clip) = scene_extents_along_dir(axes[0], world_mid, world_half);
    let first_view = sun_shadow_view_matrix(axes, snapped[0]);
    let mut partitions = [SunShadowPartition {
        clip_from_world: bevy::math::Mat4::IDENTITY,
        view: first_view,
        projection: bevy::math::Mat4::IDENTITY,
        polygon_offset: [0.0; 4],
        viewport: SunShadowViewport {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
        clip_planes: SunShadowClipPlanes::EMPTY,
    }; 2];
    for (i, partition) in partitions.iter_mut().enumerate() {
        let view = sun_shadow_view_matrix(axes, snapped[i]);
        let projection = sun_shadow_ortho_projection(
            projection_offset_clip,
            sample_size[i],
            near_clip,
            far_clip,
        );
        let m22 = sun_shadow_projection_m22(near_clip, far_clip);
        partition.view = view;
        partition.projection = projection;
        partition.clip_from_world = projection * view;
        partition.polygon_offset = shadowmap_polygon_offset(m22);
        partition.viewport = profile
            .partition_viewport(i as u32)
            .expect("forced fallback has two partitions");
    }
    let setup = SunShadowProjectionSetup {
        near_shadow_min_dist,
        shadow_org_pixel_center: fit.pixel_center,
        snapped_shadow_org: snapped,
        sample_size,
    };
    let partition_fraction_world = setup.partition_fraction(view_origin, camera.forward);

    let clip_planes = sun_shadow_clip_planes(
        &SunShadowClipInput {
            packed_axes,
            camera_origin: view_origin,
            camera_forward: camera.forward,
            camera_near_distance: camera.z_near,
            near_shadow_min_distance: near_shadow_min_dist,
            shadow_origin: org,
            shadow_origin_pixel_center: fit.pixel_center,
            snapped_shadow_origin: snapped,
            sample_size,
            useful_size: profile.partition_size(),
        },
        &rays,
    );
    for (partition, planes) in partitions.iter_mut().zip(clip_planes) {
        partition.clip_planes = planes;
    }

    let receiver_clip = partitions[0].clip_from_world
        * bevy::math::Mat4::from_translation(bevy::math::Vec3::from_array(view_origin));

    let partition_fraction = [
        partition_fraction_world[0],
        partition_fraction_world[1],
        partition_fraction_world[2],
        partition_fraction_world[3]
            + view_origin[0] * partition_fraction_world[0]
            + view_origin[1] * partition_fraction_world[1]
            + view_origin[2] * partition_fraction_world[2],
    ];
    let lookup = sun_shadow_lookup_matrix(receiver_clip, partition_fraction, profile);
    let receiver = setup.receiver_constants(forced_fallback_metrics(), false, 1.0);
    SunShadowForcedFrame {
        partitions,
        pixel_adjust: sun_shadowmap_pixel_adjust(profile),
        lookup,
        receiver,
        partition_fraction,
        axes,
        view_forward: camera.forward,
        view_right: camera.right,
        tan_half_fov: [camera.tan_half_fov_x, camera.tan_half_fov_y],
        near_shadow_min_dist,
        shadow_org_pixel_center: fit.pixel_center,
        projection_offset_clip,
    }
}
