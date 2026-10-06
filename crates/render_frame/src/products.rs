use crate::{PackedFrontendLists, RetainedDrawItem};
use render_material::{MaterialDrawKey, MaterialGenerationId, MaterialRefusal, TechType};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SourceRevisions {
    pub topology: u64,

    pub vertices: u64,

    pub materials: u64,

    pub draws: u64,

    pub admission: u64,
}

impl SourceRevisions {
    pub fn set_topology_from(&mut self, indices: &[u32], ranges: &[(u32, u32)], decoded_n: usize) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        decoded_n.hash(&mut hasher);
        indices.hash(&mut hasher);
        ranges.hash(&mut hasher);
        self.topology = hasher.finish();
    }

    pub fn bump_vertices(&mut self) {
        self.vertices = self.vertices.wrapping_add(1);
    }

    pub fn bump_materials(&mut self) {
        self.materials = self.materials.wrapping_add(1);
    }

    pub fn bump_surfaces(&mut self) {
        self.bump_vertices();
        self.bump_materials();
    }

    pub fn bump_draws(&mut self) {
        self.draws = self.draws.wrapping_add(1);
    }

    pub fn bump_admission(&mut self) {
        self.admission = self.admission.wrapping_add(1);
    }

    pub fn bump_packed_write(&mut self, revision: &mut u64) {
        self.bump_surfaces();
        self.bump_draws();
        *revision = revision.wrapping_add(1);
    }
}

/// Hands a rebuilt row list to the plan that owns it and answers the one
/// question its consumers have about it: did this rebuild change anything? The
/// comparison happens here, at the owner, once — not once per consumer per
/// frame — so `SourceRevisions` is the whole truth about the rows and nobody
/// downstream has to look at them to find out whether they moved.
///
/// `rebuilt` comes back empty with its allocation intact, ready for the next
/// frame; the rows it carried are now the published ones.
pub fn publish_rows<T: PartialEq>(published: &mut Vec<T>, rebuilt: &mut Vec<T>) -> bool {
    if published == rebuilt {
        rebuilt.clear();
        return false;
    }
    std::mem::swap(published, rebuilt);
    rebuilt.clear();
    true
}

pub const PACKED_SEGMENT_OWNERS: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PackedSegment {
    pub start: u32,

    pub rows: u32,

    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PackedSegments {
    pub layout: u64,

    pub live: bool,
    pub owners: [PackedSegment; PACKED_SEGMENT_OWNERS],
}

impl PackedSegments {
    pub fn forget(&mut self) {
        self.live = false;
        self.owners = [PackedSegment::default(); PACKED_SEGMENT_OWNERS];
        self.layout = self.layout.wrapping_add(1);
    }

    pub fn publish(&mut self, owners: [PackedSegment; PACKED_SEGMENT_OWNERS]) {
        let moved = !self.live
            || self
                .owners
                .iter()
                .zip(owners.iter())
                .any(|(was, now)| was.start != now.start || was.rows != now.rows);
        self.owners = owners;
        self.live = true;
        if moved {
            self.layout = self.layout.wrapping_add(1);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameProductKind {
    Depth,
    FloatZ,
    SunShadow,
    SpotShadow,
    Colour,
    Emissive,
    Distortion,
    Light,
    TwoD,
}

impl FrameProductKind {
    pub const ALL: [Self; 9] = [
        Self::Depth,
        Self::FloatZ,
        Self::SunShadow,
        Self::SpotShadow,
        Self::Colour,
        Self::Emissive,
        Self::Distortion,
        Self::Light,
        Self::TwoD,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductTarget {
    Core3dViewColour,

    ResolvedPostSun,

    SunShadowFallbackAtlas,

    SpotShadowMaps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingProductCause {
    MissingFrontendEmitter,
    MissingSunShadowProducer,
    MissingSpotShadowProducer,
    MissingDistortionSortKey,
    HostOwnedTwoD,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameProductStatus {
    ResolveReady {
        target: ProductTarget,
    },
    Ready {
        tech_type: TechType,
        target: ProductTarget,
    },
    Missing(MissingProductCause),
}

#[derive(Clone, Debug)]
pub struct SpotShadowFrameSlot {
    pub emitted: lighting_iw4::SpotShadowEmittedSlot,

    pub packed: Arc<PackedFrontendLists>,
}

#[derive(Clone, Debug)]
pub struct FrameMaterialRefusal {
    pub key: MaterialDrawKey,
    pub tech_type: TechType,
    pub cause: MaterialRefusal,
}

#[derive(Clone, Debug)]
pub struct FrameProduct {
    pub kind: FrameProductKind,
    pub generation_id: MaterialGenerationId,
    pub status: FrameProductStatus,
    pub ordered_draws: Vec<RetainedDrawItem>,

    pub sun_near_n: usize,

    pub sun_packed: Option<Arc<[PackedFrontendLists; 2]>>,

    pub spot_slots: Vec<SpotShadowFrameSlot>,

    pub list_digest: u64,

    pub world_pretess_id: u64,

    pub draw_tech: Vec<TechType>,

    pub code_sampler_mask: u64,

    pub material_refusals: Vec<FrameMaterialRefusal>,

    pub has_codemesh: bool,
}

impl FrameProduct {
    pub fn begin_fill(&mut self, cause: MissingProductCause) {
        self.status = FrameProductStatus::Missing(cause);
        self.ordered_draws.clear();
        self.sun_near_n = 0;
        self.sun_packed = None;
        self.spot_slots.clear();
        self.draw_tech.clear();
        self.list_digest = 0;
        self.world_pretess_id = 0;
        self.code_sampler_mask = 0;
        self.material_refusals.clear();
        self.has_codemesh = false;
    }

    pub fn binds_code_texture(&self, index: u32) -> bool {
        index < 64 && self.code_sampler_mask & (1u64 << index) != 0
    }

    pub fn tech_type(&self) -> Option<TechType> {
        match self.status {
            FrameProductStatus::Ready { tech_type, .. } => Some(tech_type),
            FrameProductStatus::ResolveReady { .. } | FrameProductStatus::Missing(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderFocusFrame {
    pub owner_id: u32,
    pub model: Option<String>,
    pub outcome: &'static str,
    pub object_id: Option<u16>,
    pub camera_hidden: Option<bool>,
    pub lighting_handle: Option<u32>,
    pub planned_surfaces: u32,
}

impl RenderFocusFrame {
    pub fn unresolved(owner_id: u32, outcome: &'static str) -> Self {
        Self {
            owner_id,
            model: None,
            outcome,
            object_id: None,
            camera_hidden: None,
            lighting_handle: None,
            planned_surfaces: 0,
        }
    }
}

#[derive(Debug)]
pub struct FrameProductsSnapshot {
    pub frame_id: u64,
    pub focus: Option<RenderFocusFrame>,
    pub products: Vec<FrameProduct>,

    pub world_run_surfs: Vec<u16>,

    pub world_run_revision: u64,
}

impl FrameProductsSnapshot {
    pub fn empty() -> Self {
        Self {
            frame_id: 0,
            focus: None,
            products: missing_products(),
            world_run_surfs: Vec::new(),
            world_run_revision: 0,
        }
    }

    pub fn product(&self, kind: FrameProductKind) -> &FrameProduct {
        self.products
            .iter()
            .find(|product| product.kind == kind)
            .expect("RenderFrameProducts must contain every declared product")
    }

    pub fn focus(&self) -> Option<&RenderFocusFrame> {
        self.focus.as_ref()
    }
}

fn missing_products() -> Vec<FrameProduct> {
    FrameProductKind::ALL
        .into_iter()
        .map(|kind| {
            let cause = match kind {
                FrameProductKind::Colour => MissingProductCause::MissingFrontendEmitter,
                FrameProductKind::SunShadow => MissingProductCause::MissingSunShadowProducer,
                FrameProductKind::SpotShadow => MissingProductCause::MissingSpotShadowProducer,
                FrameProductKind::TwoD => MissingProductCause::HostOwnedTwoD,
                _ => MissingProductCause::MissingFrontendEmitter,
            };
            FrameProduct {
                kind,
                generation_id: MaterialGenerationId::default(),
                status: FrameProductStatus::Missing(cause),
                ordered_draws: Vec::new(),
                sun_near_n: 0,
                sun_packed: None,
                spot_slots: Vec::new(),
                list_digest: 0,
                world_pretess_id: 0,
                draw_tech: Vec::new(),
                code_sampler_mask: 0,
                material_refusals: Vec::new(),
                has_codemesh: false,
            }
        })
        .collect()
}
