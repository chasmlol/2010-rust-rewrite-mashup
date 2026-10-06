use crate::asset_graph::{AssetRef, AssetRefCensus};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bevy::prelude::{Image, Resource};
use fastfile_iw4::{
    AssetLinkSink, AssetType, GfxImageGeometry, Ptr, Result, ZoneStream, block_is_aliasable,
};

pub const TS_2D: u8 = 0;
pub const TS_FUNCTION: u8 = 1;
pub const TS_COLOR_MAP: u8 = 2;

pub const TS_DETAIL_MAP: u8 = 3;
pub const TS_NORMAL_MAP: u8 = 5;
pub const TS_SPECULAR_MAP: u8 = 8;
pub const TS_WATER_MAP: u8 = 0x0B;

pub const TS_T5_COLOR0_MAP: u8 = 0x0C;
pub const TS_T5_COLOR15_MAP: u8 = 0x1B;
pub const TS_T5_THROW_MAP: u8 = 0x1C;

/// Which prepared variant a decoded image is: the bytes it decodes to, and the
/// image built around those bytes.
///
/// Two claims on the same *name* are not the same image — IW4, IW5 and T5 each
/// ship their own `hud_teamcaret` — so a name is not enough to say whether one
/// plan's decode could have answered another's. This is, and it travels on the
/// row so the merge can say which of the two happened to a claim it dropped:
/// the same bytes prepared twice, or a genuine override by another source.
///
/// The two halves are separate because only one of them is the decode. The
/// payload is what `decode` reads and produces; `usage` is everything the
/// decoded bytes are then wrapped in. A claim that matches on `payload` and
/// differs on `usage` wanted the very same texels — it is repeated work, and
/// the census has to be able to say so rather than call it another source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageVariantId {
    /// Digest of everything that decides a byte of the decoded payload: the
    /// resolved archive entries the decode reads, in order, and the map type,
    /// which chooses between the 2D and the cubemap decode.
    pub payload: u64,
    /// The options that change no byte of the payload but do change the image
    /// wrapped around it: the view's colour space and the sampler.
    pub usage: u32,
}

#[derive(Clone, Debug)]
pub struct AuthoredImage {
    pub namespace: crate::AssetNamespace,
    pub name: AssetRef,
    pub map_type: u8,
    pub semantic: u8,
    pub category: u8,
    pub use_srgb_reads: bool,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub level_count: u8,
    pub format: u32,
    pub payload: Arc<Vec<u8>>,
    pub decoded: Option<Arc<Image>>,
    pub common_owned: bool,
    /// Which variant `decoded` is, when it came from a plan. `None` means it
    /// was decoded from this row's own inline payload or never decoded.
    pub decoded_variant: Option<ImageVariantId>,
    /// Which decode plan filled `decoded`. `None` means an inline body or a
    /// row nothing has answered yet. It is what lets the merge census say
    /// *who* won a disputed name rather than only that somebody did.
    pub decoded_by: Option<u64>,

    pub pending_decode: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct StandInTextures {
    pub color: Option<(String, Arc<Image>, bool)>,
    pub normal: Option<(String, Arc<Image>, bool)>,
    pub specular: Option<(String, Arc<Image>, bool)>,
}

#[derive(Clone, Debug)]
pub struct MaterialTextureBinding {
    pub name_hash: u32,

    pub name_start: u8,
    pub name_end: u8,
    pub sampler_state: u8,
    pub semantic: u8,
    pub image: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct MaterialConstant {
    pub name_hash: u32,
    pub name: [u8; 12],
    pub literal: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct AuthoredShader {
    pub namespace: crate::AssetNamespace,
    pub name: AssetRef,
    pub kind: AssetType,

    pub program: Vec<u8>,
}

impl AuthoredShader {
    pub const fn is_vertex(&self) -> bool {
        matches!(self.kind, AssetType::VertexShader)
    }

    pub const fn is_pixel(&self) -> bool {
        matches!(self.kind, AssetType::PixelShader)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredVertexDecl {
    pub family: crate::VertexLayoutFamily,
    pub name: AssetRef,
    pub stream_count: u8,
    pub has_optional_source: u8,
    pub routing: [[u8; 2]; asset_iw4::vertex_decl::ROUTING_COUNT],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShaderSourceCensus {
    pub programs: usize,

    pub unresolved_aliases: usize,

    pub byteless: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VertexDeclStreamCensus {
    pub n: usize,
    pub stream0: usize,
    pub ppcc_n: usize,
    pub ppcc_stream_count: Option<u8>,
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssetRefDumpCensus {
    pub materials: AssetRefCensus,
    pub images: AssetRefCensus,
    pub shaders: AssetRefCensus,
    pub decls: AssetRefCensus,

    pub mat_iw4_n: usize,
    pub mat_t5_n: usize,
    pub mat_iw5_n: usize,
}

impl AssetRefDumpCensus {
    pub fn from_catalog(catalog: &MaterialDefinitions) -> Self {
        Self {
            materials: catalog.material_ref_census(),
            images: catalog.image_ref_census(),
            shaders: catalog.shader_ref_census(),
            decls: catalog.vertex_decl_ref_census(),
            mat_iw4_n: catalog.namespace_count(crate::AssetNamespace::Iw4),
            mat_t5_n: catalog.namespace_count(crate::AssetNamespace::T5),
            mat_iw5_n: catalog.namespace_count(crate::AssetNamespace::Iw5),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetPointerIdentity {
    pub block: u8,
    pub offset: u32,
}

impl From<Ptr> for AssetPointerIdentity {
    fn from(pointer: Ptr) -> Self {
        Self {
            block: pointer.block,
            offset: pointer.offset,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedShaderRef {
    pub pointer_identity: AssetPointerIdentity,
    pub shader: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnedShaderArgument {
    MaterialVertexConstant {
        destination: u16,
        name_hash: u32,
    },
    LiteralVertexConstant {
        destination: u16,
        words: Option<[u32; 4]>,
    },
    MaterialPixelSampler {
        destination: u16,
        name_hash: u32,
    },
    CodeVertexConstant {
        destination: u16,
        index: u16,
        first_row: u8,
        row_count: u8,
    },
    CodePixelSampler {
        destination: u16,
        index: u32,
    },
    CodePixelConstant {
        destination: u16,
        index: u16,
        first_row: u8,
        row_count: u8,
    },
    MaterialPixelConstant {
        destination: u16,
        name_hash: u32,
    },

    LiteralPixelConstant {
        destination: u16,
        words: Option<[u32; 4]>,
    },
    Unknown {
        argument_type: u16,
        raw: [u8; 8],
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedMaterialPass {
    pub pass_index: u8,
    pub vertex_decl_identity: AssetPointerIdentity,

    pub vertex_decl: Option<usize>,
    pub vertex_shader: OwnedShaderRef,
    pub pixel_shader: OwnedShaderRef,
    pub per_prim_arg_count: u8,
    pub per_obj_arg_count: u8,
    pub stable_arg_count: u8,

    pub custom_sampler_flags: u8,

    pub t5_custom_sampler_flags: u8,
    pub arguments: Vec<OwnedShaderArgument>,
    pub arguments_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedTechnique {
    pub source_selection: Option<render_material::SourceTechniqueSelection>,
    pub flags: u16,
    pub passes: Vec<OwnedMaterialPass>,
    pub body_scanned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedTechniqueGraph {
    pub slots: Vec<Option<OwnedTechnique>>,
    pub rows_truncated: u16,
    pub arguments_truncated: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechniqueTable {
    pub slots: u64,

    pub scanned: u64,

    pub technique0_flags: u8,

    pub model_lighting_const: Option<bool>,

    pub max_pass_count: u16,

    pub pass_count_by_slot: [u8; asset_iw4::size::TECHNIQUE_SLOT_COUNT],

    pub graph: Option<OwnedTechniqueGraph>,
}

impl Default for TechniqueTable {
    fn default() -> Self {
        Self {
            slots: 0,
            scanned: 0,
            technique0_flags: 0,
            model_lighting_const: None,
            max_pass_count: 0,
            pass_count_by_slot: [0; asset_iw4::size::TECHNIQUE_SLOT_COUNT],
            graph: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct T5TechniqueOccupancy {
    pub slots: [u64; fastfile_t5::TECHNIQUE_OCCUPANCY_WORDS],
    pub scanned: [u64; fastfile_t5::TECHNIQUE_OCCUPANCY_WORDS],
    pub technique0_flags: u8,
    pub max_pass_count: u16,
    pub pass_count_by_slot: [u8; fastfile_t5::TECHNIQUE_SLOT_COUNT],
}

impl Default for T5TechniqueOccupancy {
    fn default() -> Self {
        Self {
            slots: [0; fastfile_t5::TECHNIQUE_OCCUPANCY_WORDS],
            scanned: [0; fastfile_t5::TECHNIQUE_OCCUPANCY_WORDS],
            technique0_flags: 0,
            max_pass_count: 0,
            pass_count_by_slot: [0; fastfile_t5::TECHNIQUE_SLOT_COUNT],
        }
    }
}

impl T5TechniqueOccupancy {
    pub fn slot_occupied(self, slot: usize) -> bool {
        fastfile_t5::occupancy_test(&self.slots, slot)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossGameReason {
    T5FeatureTokenDonor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossGameTechsetResolution {
    pub want_namespace: crate::AssetNamespace,
    pub want_name: String,
    pub got_namespace: crate::AssetNamespace,
    pub got_name: String,
    pub reason: CrossGameReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechniqueSetFacts {
    pub namespace: crate::AssetNamespace,
    pub name: AssetRef,

    pub zone: crate::ZoneOwner,

    pub table: Option<TechniqueTable>,

    pub t5_occupancy: Option<T5TechniqueOccupancy>,

    pub iw5_fallback_table: Option<TechniqueTable>,

    pub t5_fallback_table: Option<TechniqueTable>,

    pub world_vert_format: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TechsetKey<'a> {
    pub namespace: crate::AssetNamespace,
    pub name: &'a str,
}

impl<'a> TechsetKey<'a> {
    pub fn new(namespace: crate::AssetNamespace, name: &'a str) -> Self {
        Self { namespace, name }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechsetResolve<'a> {
    Hit {
        index: usize,
        facts: &'a TechniqueSetFacts,
    },

    GraphMissing {
        index: usize,
    },

    Foreign {
        got: crate::AssetNamespace,
        index: usize,
    },
    Missing,
}

#[derive(Clone, Debug)]
pub struct AuthoredMaterial {
    pub name: AssetRef,

    pub namespace: crate::AssetNamespace,
    pub technique_set: AssetRef,

    pub technique_set_edge: crate::AssetEdge<crate::TechniqueSetSpace>,
    pub draw_surf: u64,
    pub sort_key: u8,

    pub info_game_flags: u8,

    pub texture_atlas: Option<[u8; 2]>,

    pub surface_type_bits: Option<u32>,
    pub t5_layered_surface_types: Option<u32>,

    pub state_flags: u8,

    pub camera_region: u8,

    pub state_bits: Vec<[u32; 2]>,

    pub state_bits_entry: Option<[u8; asset_iw4::size::TECHNIQUE_SLOT_COUNT]>,

    pub t5_state_bits_entry: Option<[u8; fastfile_t5::TECHNIQUE_SLOT_COUNT]>,

    pub iw5_state_bits_entry: Option<[u8; fastfile_iw5::size::TECHNIQUE_SLOT_COUNT]>,

    pub technique_table: Option<TechniqueTable>,

    pub route: Option<asset_iw4::MaterialDrawRoute>,
    pub textures: Vec<MaterialTextureBinding>,
    pub constants: Vec<MaterialConstant>,

    pub zone: crate::asset_graph::ZoneOwner,
}

#[derive(Clone, Copy, Debug)]
enum Link {
    Direct(usize),
    Alias(Ptr),
}

/// What reading one zone needs and the population it produces does not: the
/// pointer→row maps of the zone in the stream, the technique bodies still
/// being assembled, and the technique set last seen. It is reset between zones
/// and dropped when the population is finalized — a live catalog carries no
/// link state of a finished walk.
#[derive(Clone, Debug, Default)]
struct ZoneLinkState {
    materials: HashMap<Ptr, Link>,
    images: HashMap<Ptr, Link>,
    techsets: HashMap<Ptr, Link>,
    vertex_shaders: HashMap<Ptr, Link>,
    pixel_shaders: HashMap<Ptr, Link>,
    vertex_decls: HashMap<Ptr, Link>,
    technique_bodies: HashMap<Ptr, OwnedTechnique>,
    last_technique_set: Option<TechniqueSetFacts>,
}

/// The population a build finished with. Everything here is an answer: names
/// are resolved, technique sets are bound and material rows are final. What is
/// *not* here is the walk that produced it — there is no pointer→row map, no
/// technique body still being assembled, no zone cursor and no `absorb_*`, so a
/// consumer holding this cannot carry on linking where the importer left off.
#[derive(Clone, Debug, Default)]
pub struct MaterialDefinitions {
    pub materials: Vec<AuthoredMaterial>,
    pub images: Vec<AuthoredImage>,
    pub shaders: Vec<AuthoredShader>,
    pub vertex_decls: Vec<AuthoredVertexDecl>,
    techsets: Vec<TechniqueSetFacts>,

    pub capture_gaps: usize,

    pub leftover_iw5_arg_n: u32,
    leftover_iw5_arg_hits: BTreeMap<String, u32>,

    pub leftover_t5_arg_n: u32,
    leftover_t5_arg_hits: BTreeMap<String, u32>,

    pub link_reused_materials: usize,
    pub link_reused_images: usize,

    pub cross_game_techset_resolutions: Vec<CrossGameTechsetResolution>,
}

/// The build. It owns the definitions while they are still being assembled,
/// plus the state that assembling needs; `publish` hands the definitions on and
/// drops the rest on the floor, which is the whole point of the two types.
#[derive(Clone, Debug, Default)]
pub struct MaterialCatalog {
    defs: MaterialDefinitions,

    link: ZoneLinkState,
    capture_zone: crate::asset_graph::ZoneOwner,
    capture_ns: Option<crate::AssetNamespace>,

    cross_zone_link: bool,
}

impl std::ops::Deref for MaterialCatalog {
    type Target = MaterialDefinitions;

    fn deref(&self) -> &Self::Target {
        &self.defs
    }
}

impl std::ops::DerefMut for MaterialCatalog {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.defs
    }
}

impl MaterialCatalog {
    pub fn mark_images_common_owned(&mut self) {
        for image in &mut self.images {
            image.common_owned = true;
        }
    }

    /// Ends the build: drops the reference rows, resolves the technique-set
    /// edges, and hands the definitions on with the remap from provisional row
    /// to final row. The link state does not travel with them — it is dropped
    /// here, along with the catalog that needed it, which is the one thing this
    /// pair of types exists to guarantee.
    pub fn publish(mut self) -> (MaterialDefinitions, Vec<Option<usize>>) {
        let remap = self.finalize_asset_population();
        (self.defs, remap)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaterialImageMemory {
    pub images: usize,
    pub decoded_images: usize,
    pub payload_bytes: usize,
    pub decoded_bytes: usize,
}

impl MaterialImageMemory {
    pub const fn total_bytes(self) -> usize {
        self.payload_bytes + self.decoded_bytes
    }

    pub fn report_row(self, label: &str) -> String {
        format!(
            "{label}: images={} decoded={} payload={:.1}MiB decoded_pixels={:.1}MiB total={:.1}MiB",
            self.images,
            self.decoded_images,
            self.payload_bytes as f64 / (1024.0 * 1024.0),
            self.decoded_bytes as f64 / (1024.0 * 1024.0),
            self.total_bytes() as f64 / (1024.0 * 1024.0),
        )
    }
}

impl MaterialCatalog {
    pub fn set_capture_zone(&mut self, zone: crate::asset_graph::ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn set_capture_ns(&mut self, ns: crate::AssetNamespace) {
        self.capture_ns = Some(ns);
    }

    pub fn capture_ns(&self) -> crate::AssetNamespace {
        self.capture_ns
            .expect("asset capture requires an explicit family")
    }

    /// Pointer→row lookup: this is the walk's question, and it only has an
    /// answer while the walk is still on.
    pub fn material_index(&self, slot: Ptr) -> Option<crate::WalkLocalMaterialIndex> {
        resolve(&self.link.materials, slot).map(crate::WalkLocalMaterialIndex::from_walk)
    }

    pub fn image_index(&self, slot: Ptr) -> Option<usize> {
        resolve(&self.link.images, slot)
    }

    pub fn prepare_for_next_zone(&mut self) {
        self.link = ZoneLinkState::default();
        self.cross_zone_link = !self.materials.is_empty()
            || !self.images.is_empty()
            || !self.shaders.is_empty()
            || !self.vertex_decls.is_empty()
            || !self.techsets.is_empty();
        self.link_reused_materials = 0;
        self.link_reused_images = 0;
    }

    pub fn link_image(&mut self, incoming: AuthoredImage) -> usize {
        if let Some(index) = self.images.iter().position(|owned| {
            owned.namespace == incoming.namespace && owned.name.same_name(&incoming.name)
        }) {
            let existing = &self.images[index];
            if existing.name.is_real()
                && incoming.name.is_real()
                && existing.use_srgb_reads != incoming.use_srgb_reads
            {
                let index = self.images.len();
                self.images.push(incoming);
                return index;
            }
            self.link_reused_images = self.link_reused_images.saturating_add(1);
            let take_body = AssetRef::incoming_owns_slot(&self.images[index].name, &incoming.name)
                && !(incoming.payload.is_empty() && !self.images[index].payload.is_empty());
            if take_body {
                let mut owned = incoming;
                owned.decoded = owned.decoded.or_else(|| self.images[index].decoded.take());
                self.images[index] = owned;
            } else if incoming.decoded.is_some() && self.images[index].decoded.is_none() {
                self.images[index].decoded = incoming.decoded;
                self.images[index].common_owned = incoming.common_owned;
            }
            index
        } else {
            let index = self.images.len();
            self.images.push(incoming);
            index
        }
    }

    fn take_image_slot(&mut self, incoming: AuthoredImage) -> usize {
        if self.cross_zone_link {
            self.link_image(incoming)
        } else {
            let index = self.images.len();
            self.images.push(incoming);
            index
        }
    }

    fn linkable_material_index(&self, incoming: &AuthoredMaterial) -> Option<usize> {
        self.materials.iter().position(|owned| {
            owned.namespace == incoming.namespace && owned.name.same_name(&incoming.name)
        })
    }

    pub fn link_material(&mut self, incoming: AuthoredMaterial) -> usize {
        let canonical = incoming.name.clone();
        if let Some(index) = self.linkable_material_index(&incoming) {
            self.link_reused_materials = self.link_reused_materials.saturating_add(1);
            if AssetRef::incoming_owns_slot(&self.materials[index].name, &canonical) {
                self.materials[index] = incoming;
            }
            index
        } else {
            let index = self.materials.len();
            self.materials.push(incoming);
            index
        }
    }

    fn take_material_slot(&mut self, incoming: AuthoredMaterial) -> usize {
        if self.cross_zone_link {
            self.link_material(incoming)
        } else {
            let index = self.materials.len();
            self.materials.push(incoming);
            index
        }
    }

    fn link_material_host_real_wins(&mut self, incoming: AuthoredMaterial) -> usize {
        if let Some(index) = self.linkable_material_index(&incoming)
            && self.materials[index].name.is_real()
        {
            self.link_reused_materials = self.link_reused_materials.saturating_add(1);
            index
        } else {
            self.link_material(incoming)
        }
    }

    pub(crate) fn link_shader(&mut self, incoming: AuthoredShader) -> usize {
        if let Some(index) = self.shaders.iter().position(|owned| {
            owned.namespace == incoming.namespace
                && owned.kind == incoming.kind
                && owned.name.same_name(&incoming.name)
        }) {
            let take_body = AssetRef::incoming_owns_slot(&self.shaders[index].name, &incoming.name)
                && !(incoming.program.is_empty() && !self.shaders[index].program.is_empty());
            if take_body {
                self.shaders[index] = incoming;
            }
            index
        } else {
            let index = self.shaders.len();
            self.shaders.push(incoming);
            index
        }
    }

    fn take_shader_slot(&mut self, incoming: AuthoredShader) -> usize {
        if self.cross_zone_link {
            self.link_shader(incoming)
        } else {
            let index = self.shaders.len();
            self.shaders.push(incoming);
            index
        }
    }

    pub(crate) fn link_vertex_decl(&mut self, incoming: AuthoredVertexDecl) -> usize {
        if let Some(index) = self.vertex_decls.iter().position(|owned| {
            owned.family == incoming.family
                && if incoming.name.is_empty() {
                    owned == &incoming
                } else {
                    owned.name.same_name(&incoming.name)
                }
        }) {
            let take_body =
                AssetRef::incoming_owns_slot(&self.vertex_decls[index].name, &incoming.name)
                    && !(incoming.stream_count == 0 && self.vertex_decls[index].stream_count != 0);
            if take_body {
                self.vertex_decls[index] = incoming;
            }
            index
        } else {
            let index = self.vertex_decls.len();
            self.vertex_decls.push(incoming);
            index
        }
    }

    fn take_vertex_decl_slot(&mut self, incoming: AuthoredVertexDecl) -> usize {
        if self.cross_zone_link && !incoming.name.is_empty() {
            self.link_vertex_decl(incoming)
        } else {
            let index = self.vertex_decls.len();
            self.vertex_decls.push(incoming);
            index
        }
    }

    fn shader_identity_complete(technique: &OwnedTechnique) -> bool {
        technique
            .passes
            .iter()
            .all(|pass| pass.vertex_shader.shader.is_some() && pass.pixel_shader.shader.is_some())
    }

    fn same_techset_identity(owned: &TechniqueSetFacts, incoming: &TechniqueSetFacts) -> bool {
        owned.namespace == incoming.namespace && owned.name.same_name(&incoming.name)
    }

    pub fn link_techset(&mut self, mut facts: TechniqueSetFacts) -> usize {
        if let Some(index) = self
            .techsets
            .iter()
            .position(|owned| Self::same_techset_identity(owned, &facts))
        {
            let existing_occ = self.techsets[index].t5_occupancy;
            let merged_occ = merge_t5_occupancy(facts.t5_occupancy, existing_occ);
            if let Some(mut incoming) = facts.table.take() {
                if let Some(existing) = &self.techsets[index].table {
                    incoming.slots |= existing.slots;
                    incoming.scanned |= existing.scanned;
                    match (&mut incoming.graph, &existing.graph) {
                        (Some(incoming_graph), Some(existing_graph)) => {
                            if incoming_graph.slots.len() < existing_graph.slots.len() {
                                incoming_graph
                                    .slots
                                    .resize(existing_graph.slots.len(), None);
                            }
                            for (slot, existing_technique) in
                                existing_graph.slots.iter().enumerate()
                            {
                                let incoming_lost_shader_identity = incoming_graph.slots[slot]
                                    .as_ref()
                                    .is_some_and(|incoming_technique| {
                                        !Self::shader_identity_complete(incoming_technique)
                                            && existing_technique.as_ref().is_some_and(|existing| {
                                                Self::shader_identity_complete(existing)
                                            })
                                    });
                                if incoming_graph.slots[slot].is_none()
                                    || incoming_lost_shader_identity
                                {
                                    incoming_graph.slots[slot] = existing_technique.clone();
                                }
                            }
                        }
                        (None, Some(existing_graph)) => {
                            incoming.graph = Some(existing_graph.clone());
                        }
                        _ => {}
                    }
                }
                let existing_wvf = self.techsets[index].world_vert_format;
                let fallback = facts
                    .iw5_fallback_table
                    .clone()
                    .or_else(|| self.techsets[index].iw5_fallback_table.clone());
                let t5_fallback = facts
                    .t5_fallback_table
                    .clone()
                    .or_else(|| self.techsets[index].t5_fallback_table.clone());
                self.techsets[index] = TechniqueSetFacts {
                    namespace: facts.namespace,
                    name: facts.name,
                    zone: facts.zone,
                    table: Some(incoming),
                    t5_occupancy: merged_occ,
                    iw5_fallback_table: fallback,
                    t5_fallback_table: t5_fallback,
                    world_vert_format: merge_world_vert_format(
                        facts.world_vert_format,
                        existing_wvf,
                    ),
                };
            } else {
                if AssetRef::incoming_owns_slot(&self.techsets[index].name, &facts.name) {
                    self.techsets[index].name = facts.name.clone();
                    self.techsets[index].zone = facts.zone;
                }
                self.techsets[index].t5_occupancy = merged_occ;
                if self.techsets[index].iw5_fallback_table.is_none() {
                    self.techsets[index].iw5_fallback_table = facts.iw5_fallback_table;
                }
                if self.techsets[index].t5_fallback_table.is_none() {
                    self.techsets[index].t5_fallback_table = facts.t5_fallback_table;
                }
                self.techsets[index].world_vert_format = merge_world_vert_format(
                    facts.world_vert_format,
                    self.techsets[index].world_vert_format,
                );
            }
            index
        } else {
            let index = self.techsets.len();
            self.techsets.push(facts);
            index
        }
    }

    fn take_techset_slot(&mut self, facts: TechniqueSetFacts) -> usize {
        let index = if self.cross_zone_link {
            self.link_techset(facts)
        } else {
            let index = self.techsets.len();
            self.techsets.push(facts);
            index
        };
        self.link.last_technique_set = self.techsets.get(index).cloned();
        index
    }

    fn note_leftover_iw5_arg(&mut self, iw5_type: u16, raw: [u8; 8]) {
        let index = u16::from_le_bytes([raw[4], raw[5]]);
        let key = format!("t{iw5_type}i{index}");
        self.leftover_iw5_arg_n = self.leftover_iw5_arg_n.saturating_add(1);
        *self.leftover_iw5_arg_hits.entry(key).or_default() += 1;
    }

    fn note_leftover_t5_arg(&mut self, raw: [u8; 8]) {
        let dest = u16::from_le_bytes([raw[2], raw[3]]);
        let index = u16::from_le_bytes([raw[4], raw[5]]);
        let name = crate::t5_code_remap::t5_code_const_name(index).unwrap_or("?");
        let key = format!("d{dest}i{index}:{name}");
        self.leftover_t5_arg_n = self.leftover_t5_arg_n.saturating_add(1);
        *self.leftover_t5_arg_hits.entry(key).or_default() += 1;
    }

    pub fn resolve_technique_set_edges(&mut self) {
        let MaterialDefinitions {
            techsets,
            materials,
            ..
        } = &mut self.defs;
        let techsets = &*techsets;
        for material in materials {
            if material.technique_set.is_empty() {
                material.technique_set_edge = crate::AssetEdge::Absent;
                continue;
            }
            material.technique_set_edge = match MaterialDefinitions::resolve_technique_set_in(
                techsets,
                TechsetKey::new(material.namespace, material.technique_set.as_str()),
            ) {
                TechsetResolve::Hit { index, .. } | TechsetResolve::GraphMissing { index } => {
                    crate::AssetEdge::bind_order(index, techsets[index].zone)
                }
                TechsetResolve::Foreign { .. } | TechsetResolve::Missing => {
                    crate::AssetEdge::Unresolved(crate::AssetEdgeReason::CatalogMiss)
                }
            };
        }
    }

    pub fn absorb_asset_population(&mut self, donor: MaterialCatalog) -> Vec<usize> {
        self.absorb_asset_population_with_material_policy(donor, false)
    }

    pub fn absorb_asset_population_host_materials_win(
        &mut self,
        donor: MaterialCatalog,
    ) -> Vec<usize> {
        self.absorb_asset_population_with_material_policy(donor, true)
    }

    fn absorb_asset_population_with_material_policy(
        &mut self,
        donor: MaterialCatalog,
        host_materials_win: bool,
    ) -> Vec<usize> {
        let MaterialCatalog {
            defs:
                MaterialDefinitions {
                    images,
                    vertex_decls,
                    shaders,
                    techsets,
                    materials,
                    cross_game_techset_resolutions,
                    leftover_iw5_arg_n,
                    leftover_iw5_arg_hits,
                    leftover_t5_arg_n,
                    leftover_t5_arg_hits,
                    ..
                },
            ..
        } = donor;
        self.leftover_iw5_arg_n = self.leftover_iw5_arg_n.saturating_add(leftover_iw5_arg_n);
        for (key, count) in leftover_iw5_arg_hits {
            *self.leftover_iw5_arg_hits.entry(key).or_default() += count;
        }
        self.leftover_t5_arg_n = self.leftover_t5_arg_n.saturating_add(leftover_t5_arg_n);
        for (key, count) in leftover_t5_arg_hits {
            *self.leftover_t5_arg_hits.entry(key).or_default() += count;
        }
        let image_ids = images
            .into_iter()
            .map(|image| self.link_image(image))
            .collect::<Vec<_>>();
        let vertex_decl_ids = vertex_decls
            .into_iter()
            .map(|decl| self.link_vertex_decl(decl))
            .collect::<Vec<_>>();
        let shader_ids = shaders
            .into_iter()
            .map(|shader| self.link_shader(shader))
            .collect::<Vec<_>>();
        let rebase_table = |mut table: TechniqueTable| {
            if let Some(graph) = &mut table.graph {
                for technique in graph.slots.iter_mut().flatten() {
                    for pass in &mut technique.passes {
                        pass.vertex_decl = pass
                            .vertex_decl
                            .and_then(|index| vertex_decl_ids.get(index).copied());
                        pass.vertex_shader.shader = pass
                            .vertex_shader
                            .shader
                            .and_then(|index| shader_ids.get(index).copied());
                        pass.pixel_shader.shader = pass
                            .pixel_shader
                            .shader
                            .and_then(|index| shader_ids.get(index).copied());
                    }
                }
            }
            table
        };
        self.cross_game_techset_resolutions
            .extend(cross_game_techset_resolutions);
        for mut facts in techsets {
            facts.table = facts.table.map(&rebase_table);
            facts.iw5_fallback_table = facts.iw5_fallback_table.map(&rebase_table);
            facts.t5_fallback_table = facts.t5_fallback_table.map(&rebase_table);
            self.link_techset(facts);
        }

        materials
            .into_iter()
            .map(|mut material| {
                for texture in &mut material.textures {
                    texture.image = texture
                        .image
                        .and_then(|index| image_ids.get(index).copied());
                }
                material.technique_table = material.technique_table.map(&rebase_table);
                if host_materials_win {
                    self.link_material_host_real_wins(material)
                } else {
                    self.link_material(material)
                }
            })
            .collect()
    }

    pub fn absorb_missing_reals(&mut self, donor: MaterialCatalog) {
        let MaterialCatalog {
            defs:
                MaterialDefinitions {
                    images,
                    vertex_decls,
                    shaders,
                    techsets,
                    materials,
                    cross_game_techset_resolutions,
                    leftover_t5_arg_n,
                    leftover_t5_arg_hits,
                    ..
                },
            ..
        } = donor;
        self.leftover_t5_arg_n = self.leftover_t5_arg_n.saturating_add(leftover_t5_arg_n);
        for (key, count) in leftover_t5_arg_hits {
            *self.leftover_t5_arg_hits.entry(key).or_default() += count;
        }
        let image_ids = images
            .into_iter()
            .map(|image| {
                if let Some(index) = self.real_image_index(image.namespace, &image.name) {
                    return index;
                }
                self.link_image(image)
            })
            .collect::<Vec<_>>();
        let vertex_decl_ids = vertex_decls
            .into_iter()
            .map(|decl| {
                if let Some(index) = self.real_vertex_decl_index(&decl) {
                    return index;
                }
                self.link_vertex_decl(decl)
            })
            .collect::<Vec<_>>();
        let shader_ids = shaders
            .into_iter()
            .map(|shader| {
                if let Some(index) =
                    self.real_shader_index(shader.namespace, &shader.name, shader.kind)
                {
                    return index;
                }
                self.link_shader(shader)
            })
            .collect::<Vec<_>>();
        let rebase_table = |mut table: TechniqueTable| {
            if let Some(graph) = &mut table.graph {
                for technique in graph.slots.iter_mut().flatten() {
                    for pass in &mut technique.passes {
                        pass.vertex_decl = pass
                            .vertex_decl
                            .and_then(|index| vertex_decl_ids.get(index).copied());
                        pass.vertex_shader.shader = pass
                            .vertex_shader
                            .shader
                            .and_then(|index| shader_ids.get(index).copied());
                        pass.pixel_shader.shader = pass
                            .pixel_shader
                            .shader
                            .and_then(|index| shader_ids.get(index).copied());
                    }
                }
            }
            table
        };
        self.cross_game_techset_resolutions
            .extend(cross_game_techset_resolutions);
        for mut facts in techsets {
            if self
                .real_techset_index(facts.namespace, &facts.name)
                .is_some()
            {
                continue;
            }
            facts.table = facts.table.map(&rebase_table);
            facts.iw5_fallback_table = facts.iw5_fallback_table.map(&rebase_table);
            facts.t5_fallback_table = facts.t5_fallback_table.map(&rebase_table);
            self.link_techset(facts);
        }
        for mut material in materials {
            if self.real_material_index(&material).is_some() {
                continue;
            }
            for texture in &mut material.textures {
                texture.image = texture
                    .image
                    .and_then(|index| image_ids.get(index).copied());
            }
            material.technique_table = material.technique_table.map(&rebase_table);
            self.link_material(material);
        }
    }

    fn real_image_index(&self, namespace: crate::AssetNamespace, name: &AssetRef) -> Option<usize> {
        self.images.iter().position(|owned| {
            owned.namespace == namespace && owned.name.is_real() && owned.name.same_name(name)
        })
    }

    fn real_shader_index(
        &self,
        namespace: crate::AssetNamespace,
        name: &AssetRef,
        kind: AssetType,
    ) -> Option<usize> {
        self.shaders.iter().position(|owned| {
            owned.namespace == namespace
                && owned.kind == kind
                && owned.name.is_real()
                && owned.name.same_name(name)
        })
    }

    fn real_vertex_decl_index(&self, decl: &AuthoredVertexDecl) -> Option<usize> {
        if decl.name.is_empty() {
            return None;
        }
        self.vertex_decls.iter().position(|owned| {
            owned.name.is_real() && owned.family == decl.family && owned.name.same_name(&decl.name)
        })
    }

    fn real_techset_index(
        &self,
        namespace: crate::AssetNamespace,
        name: &AssetRef,
    ) -> Option<usize> {
        self.techsets.iter().position(|owned| {
            owned.namespace == namespace && owned.name.is_real() && owned.name.same_name(name)
        })
    }

    fn real_material_index(&self, material: &AuthoredMaterial) -> Option<usize> {
        self.materials.iter().position(|owned| {
            owned.name.is_real()
                && owned.namespace == material.namespace
                && owned.name.same_name(&material.name)
        })
    }

    pub fn finalize_asset_population(&mut self) -> Vec<Option<usize>> {
        let mut remap = vec![None; self.materials.len()];
        let mut finalized = Vec::with_capacity(self.materials.len());
        for (old_id, material) in self.materials.drain(..).enumerate() {
            if material.name.is_reference() {
                continue;
            }
            remap[old_id] = Some(finalized.len());
            finalized.push(material);
        }
        self.materials = finalized;
        self.resolve_technique_set_edges();
        self.link = ZoneLinkState::default();
        remap
    }

    pub fn absorb_technique_set_tables(&mut self, donor: &[TechniqueSetFacts]) -> usize {
        let mut absorbed = 0usize;
        for facts in donor {
            if facts.name.is_reference() {
                continue;
            }
            let mut did = false;
            if facts.table.is_some()
                && !self.techsets.iter().any(|owned| {
                    owned.namespace == facts.namespace
                        && owned.name.same_name(&facts.name)
                        && owned.table.is_some()
                })
            {
                let table = facts.table.as_ref().map(Self::table_without_donor_indices);
                if let Some(owned) = self.techsets.iter_mut().find(|owned| {
                    owned.namespace == facts.namespace && owned.name.same_name(&facts.name)
                }) {
                    if AssetRef::incoming_owns_slot(&owned.name, &facts.name) {
                        owned.name = facts.name.clone();
                        owned.zone = facts.zone;
                    }
                    owned.table = table;
                    owned.world_vert_format =
                        merge_world_vert_format(facts.world_vert_format, owned.world_vert_format);
                } else {
                    self.techsets.push(TechniqueSetFacts {
                        namespace: facts.namespace,
                        name: facts.name.clone(),
                        zone: facts.zone,
                        table,
                        t5_occupancy: None,
                        iw5_fallback_table: None,
                        t5_fallback_table: None,
                        world_vert_format: facts.world_vert_format,
                    });
                }
                did = true;
            }
            if facts.t5_occupancy.is_some()
                && !self.techsets.iter().any(|owned| {
                    owned.namespace == facts.namespace
                        && owned.name.same_name(&facts.name)
                        && owned.t5_occupancy.is_some()
                })
            {
                if let Some(owned) = self.techsets.iter_mut().find(|owned| {
                    owned.namespace == facts.namespace && owned.name.same_name(&facts.name)
                }) {
                    if AssetRef::incoming_owns_slot(&owned.name, &facts.name) {
                        owned.name = facts.name.clone();
                        owned.zone = facts.zone;
                    }
                    owned.t5_occupancy = facts.t5_occupancy;
                    owned.world_vert_format =
                        merge_world_vert_format(facts.world_vert_format, owned.world_vert_format);
                } else {
                    self.techsets.push(TechniqueSetFacts {
                        namespace: facts.namespace,
                        name: facts.name.clone(),
                        zone: facts.zone,
                        table: None,
                        t5_occupancy: facts.t5_occupancy,
                        iw5_fallback_table: None,
                        t5_fallback_table: None,
                        world_vert_format: facts.world_vert_format,
                    });
                }
                did = true;
            }
            if did {
                absorbed += 1;
            }
        }
        absorbed
    }

    pub fn absorb_t5_feature_token_donors(&mut self) -> usize {
        let mut copies = Vec::new();
        let mut keep_fallback = Vec::new();
        for (index, owned) in self.techsets.iter().enumerate() {
            if owned
                .table
                .as_ref()
                .is_some_and(table_shader_identity_ready)
            {
                continue;
            }

            if owned
                .t5_fallback_table
                .as_ref()
                .is_some_and(table_shader_identity_ready)
            {
                keep_fallback.push(index);
                continue;
            }

            if owned.t5_occupancy.is_none() && owned.name.is_real() {
                continue;
            }
            let stripped = t5_feature_token_stripped(owned.name.as_str());
            if stripped == owned.name.as_str() {
                continue;
            }
            let Some(donor) = self.techsets.iter().find(|donor| {
                donor.name.is_real()
                    && donor.name.as_str() == stripped
                    && donor
                        .table
                        .as_ref()
                        .is_some_and(table_shader_identity_ready)
            }) else {
                continue;
            };
            copies.push((
                index,
                donor.table.clone(),
                donor.world_vert_format,
                donor.namespace,
                donor.name.as_str().to_owned(),
            ));
        }
        for index in keep_fallback {
            if !self.techsets[index]
                .table
                .as_ref()
                .is_some_and(table_shader_identity_ready)
            {
                self.techsets[index].table = self.techsets[index].t5_fallback_table.clone();
            }
        }
        let absorbed = copies.len();
        for (index, table, world_vert_format, donor_ns, donor_name) in copies {
            if self.techsets[index].name.is_reference() {
                self.techsets[index].name =
                    AssetRef::Real(self.techsets[index].name.as_str().to_owned());
            }
            let resolution = CrossGameTechsetResolution {
                want_namespace: self.techsets[index].namespace,
                want_name: self.techsets[index].name.as_str().to_owned(),
                got_namespace: donor_ns,
                got_name: donor_name,
                reason: CrossGameReason::T5FeatureTokenDonor,
            };
            self.cross_game_techset_resolutions.push(resolution);
            self.techsets[index].table = table;
            self.techsets[index].world_vert_format = world_vert_format;
        }
        absorbed
    }

    fn table_without_donor_indices(table: &TechniqueTable) -> TechniqueTable {
        let mut table = table.clone();
        if let Some(graph) = &mut table.graph {
            for technique in graph.slots.iter_mut().flatten() {
                for pass in &mut technique.passes {
                    pass.vertex_decl = None;
                    pass.vertex_shader.shader = None;
                    pass.pixel_shader.shader = None;
                }
            }
        }
        table
    }

    pub fn reroute_stub_materials(&mut self) -> usize {
        let mut routed = 0usize;
        let MaterialDefinitions {
            techsets,
            materials,
            ..
        } = &mut self.defs;
        let techsets = &*techsets;
        for material in materials {
            if material.route.is_some() {
                continue;
            }
            let Some(table) = Self::resolve_technique_table(
                techsets,
                material.namespace,
                &material.technique_set,
            ) else {
                continue;
            };
            material.technique_table = Some(table.clone());
            material.route = Some(route_from_table(
                material.sort_key,
                material.info_game_flags,
                material.state_flags,
                &table,
            ));
            routed += 1;
        }
        routed
    }

    pub fn promote_iw5_fallback_tables(&mut self) -> usize {
        let mut promoted = 0usize;
        for facts in &mut self.techsets {
            if facts.table.is_some() {
                continue;
            }
            let Some(table) = facts
                .t5_fallback_table
                .as_ref()
                .filter(|table| table_shader_identity_ready(table))
                .cloned()
                .or_else(|| facts.iw5_fallback_table.clone())
                .or_else(|| facts.t5_fallback_table.clone())
            else {
                continue;
            };
            facts.table = Some(table);
            promoted = promoted.saturating_add(1);
        }
        promoted
    }

    fn resolve_technique_table(
        techsets: &[TechniqueSetFacts],
        namespace: crate::AssetNamespace,
        technique_set: impl AsRef<str>,
    ) -> Option<TechniqueTable> {
        match MaterialDefinitions::resolve_technique_set_in(
            techsets,
            TechsetKey::new(namespace, technique_set.as_ref()),
        ) {
            TechsetResolve::Hit { facts, .. } => facts.table.clone(),
            TechsetResolve::GraphMissing { .. }
            | TechsetResolve::Foreign { .. }
            | TechsetResolve::Missing => None,
        }
    }

    fn bind(map: &mut HashMap<Ptr, Link>, slot: Ptr, link: Link) {
        if block_is_aliasable(slot.block) {
            map.insert(slot, link);
        }
    }

    fn capture_image(&mut self, s: &ZoneStream<'_>, geometry: GfxImageGeometry) -> Option<usize> {
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let payload = if geometry.source_len == 0 {
            Vec::new()
        } else {
            s.source_slice(geometry.source_offset, geometry.source_len)
                .ok()?
                .to_vec()
        };
        Some(
            self.take_image_slot(AuthoredImage {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                map_type: geometry.map_type,
                semantic: geometry.semantic,
                category: geometry.category,
                use_srgb_reads: geometry.use_srgb_reads,
                width: geometry.width,
                height: geometry.height,
                depth: geometry.depth,
                level_count: geometry.level_count,
                format: geometry.format,
                payload: Arc::new(payload),
                decoded: None,
                common_owned: false,
                decoded_variant: None,
                decoded_by: None,
                pending_decode: None,
            }),
        )
    }

    fn capture_material(&mut self, s: &ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_material()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;

        let technique_set = self.link.last_technique_set.take()?;
        let mut textures = Vec::with_capacity(geometry.texture_count);
        if let Some(table) = geometry.textures {
            for i in 0..geometry.texture_count {
                let texture = table.at(i * geometry.texture_stride);
                let semantic = s.u8_at(texture, 7).ok()?;
                textures.push(MaterialTextureBinding {
                    name_hash: s.u32_at(texture, 0).ok()?,
                    name_start: s.u8_at(texture, 4).ok()?,
                    name_end: s.u8_at(texture, 5).ok()?,
                    sampler_state: s.u8_at(texture, 6).ok()?,
                    semantic,
                    image: (semantic != 11)
                        .then(|| resolve(&self.link.images, texture.at(8)))
                        .flatten(),
                });
            }
        }
        let mut constants = Vec::with_capacity(geometry.constant_count);
        if let Some(table) = geometry.constants {
            for i in 0..geometry.constant_count {
                let constant = table.at(i * asset_iw4::size::MATERIAL_CONSTANT_DEF);
                let mut name = [0; 12];
                for (offset, byte) in name.iter_mut().enumerate() {
                    *byte = s.u8_at(constant, 4 + offset).ok()?;
                }
                constants.push(MaterialConstant {
                    name_hash: s.u32_at(constant, 0).ok()?,
                    name,
                    literal: [
                        s.f32_at(constant, 16).ok()?,
                        s.f32_at(constant, 20).ok()?,
                        s.f32_at(constant, 24).ok()?,
                        s.f32_at(constant, 28).ok()?,
                    ],
                });
            }
        }

        let table = technique_set.table.clone().or_else(|| {
            Self::resolve_technique_table(
                &self.techsets,
                self.capture_ns
                    .expect("asset capture requires an explicit family"),
                &technique_set.name,
            )
        });
        let route = table.as_ref().map(|table| {
            route_from_table(
                geometry.sort_key,
                geometry.info_game_flags,
                geometry.state_flags,
                table,
            )
        });
        Some(
            self.take_material_slot(AuthoredMaterial {
                name,
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                technique_set_edge: if technique_set.name.is_empty() {
                    crate::AssetEdge::Absent
                } else {
                    crate::AssetEdge::Unresolved(crate::AssetEdgeReason::CatalogMiss)
                },
                technique_set: technique_set.name,
                draw_surf: geometry.draw_surf,
                sort_key: geometry.sort_key,
                info_game_flags: geometry.info_game_flags,
                texture_atlas: Some(geometry.texture_atlas),
                surface_type_bits: geometry.surface_type_bits,
                t5_layered_surface_types: None,
                state_flags: geometry.state_flags,
                camera_region: geometry.camera_region,
                state_bits: read_state_bits(
                    |offset| s.u32_at(geometry.state_bits?, offset).ok(),
                    geometry.state_bits_count,
                ),
                state_bits_entry: geometry.state_bits_entry,
                t5_state_bits_entry: None,
                iw5_state_bits_entry: None,
                technique_table: table,
                route,
                textures,
                constants,
                zone: self.capture_zone,
            }),
        )
    }

    fn capture_owned_technique_graph(
        &mut self,
        geometry: fastfile_iw4::TechniqueSetGeometry,
        graph: &fastfile_iw4::TechniqueGraphGeometry,
    ) -> OwnedTechniqueGraph {
        let mut slots = Vec::with_capacity(asset_iw4::size::TECHNIQUE_SLOT_COUNT);
        for tech_slot in 0..asset_iw4::size::TECHNIQUE_SLOT_COUNT {
            if geometry.technique_slots & (1u64 << tech_slot) == 0 {
                slots.push(None);
                continue;
            }
            let mut passes = Vec::new();
            for row in graph
                .iter_rows()
                .filter(|row| usize::from(row.tech_slot) == tech_slot)
            {
                let arguments = graph
                    .arguments_for(row)
                    .iter()
                    .map(|argument| {
                        let argument_type = u16::from_le_bytes([argument.raw[0], argument.raw[1]]);
                        let destination = u16::from_le_bytes([argument.raw[2], argument.raw[3]]);
                        let payload = u32::from_le_bytes([
                            argument.raw[4],
                            argument.raw[5],
                            argument.raw[6],
                            argument.raw[7],
                        ]);
                        let code_constant = || {
                            (
                                u16::from_le_bytes([argument.raw[4], argument.raw[5]]),
                                argument.raw[6],
                                argument.raw[7],
                            )
                        };
                        match argument_type {
                            asset_iw4::size::mtl_arg::MATERIAL_VERTEX_CONST => {
                                OwnedShaderArgument::MaterialVertexConstant {
                                    destination,
                                    name_hash: payload,
                                }
                            }
                            asset_iw4::size::mtl_arg::LITERAL_VERTEX_CONST => {
                                OwnedShaderArgument::LiteralVertexConstant {
                                    destination,
                                    words: argument
                                        .literal_present
                                        .then_some(argument.literal_words),
                                }
                            }
                            asset_iw4::size::mtl_arg::MATERIAL_PIXEL_SAMPLER => {
                                OwnedShaderArgument::MaterialPixelSampler {
                                    destination,
                                    name_hash: payload,
                                }
                            }
                            asset_iw4::size::mtl_arg::CODE_VERTEX_CONST => {
                                let (index, first_row, row_count) = code_constant();
                                OwnedShaderArgument::CodeVertexConstant {
                                    destination,
                                    index,
                                    first_row,
                                    row_count,
                                }
                            }
                            asset_iw4::size::mtl_arg::CODE_PIXEL_SAMPLER => {
                                OwnedShaderArgument::CodePixelSampler {
                                    destination,
                                    index: payload,
                                }
                            }
                            asset_iw4::size::mtl_arg::CODE_PIXEL_CONST => {
                                let (index, first_row, row_count) = code_constant();
                                OwnedShaderArgument::CodePixelConstant {
                                    destination,
                                    index,
                                    first_row,
                                    row_count,
                                }
                            }
                            asset_iw4::size::mtl_arg::MATERIAL_PIXEL_CONST => {
                                OwnedShaderArgument::MaterialPixelConstant {
                                    destination,
                                    name_hash: payload,
                                }
                            }
                            asset_iw4::size::mtl_arg::LITERAL_PIXEL_CONST => {
                                OwnedShaderArgument::LiteralPixelConstant {
                                    destination,
                                    words: argument
                                        .literal_present
                                        .then_some(argument.literal_words),
                                }
                            }
                            _ => OwnedShaderArgument::Unknown {
                                argument_type,
                                raw: argument.raw,
                            },
                        }
                    })
                    .collect();
                passes.push(OwnedMaterialPass {
                    pass_index: row.pass_index,
                    vertex_decl_identity: row.vertex_decl_slot.into(),
                    vertex_decl: resolve(&self.link.vertex_decls, row.vertex_decl_slot),
                    vertex_shader: OwnedShaderRef {
                        pointer_identity: row.vertex_shader_slot.into(),
                        shader: resolve(&self.link.vertex_shaders, row.vertex_shader_slot),
                    },
                    pixel_shader: OwnedShaderRef {
                        pointer_identity: row.pixel_shader_slot.into(),
                        shader: resolve(&self.link.pixel_shaders, row.pixel_shader_slot),
                    },
                    per_prim_arg_count: row.per_prim_arg_count,
                    per_obj_arg_count: row.per_obj_arg_count,
                    stable_arg_count: row.stable_arg_count,
                    custom_sampler_flags: row.custom_sampler_flags,
                    t5_custom_sampler_flags: 0,
                    arguments,
                    arguments_truncated: row.arguments_truncated,
                });
            }
            let scanned = geometry.technique_slots_scanned & (1u64 << tech_slot) != 0;
            let technique = if scanned {
                Some(OwnedTechnique {
                    source_selection: None,
                    flags: geometry.technique_flags_by_slot[tech_slot],
                    passes,
                    body_scanned: true,
                })
            } else {
                geometry.technique_body_by_slot[tech_slot]
                    .and_then(|body| self.link.technique_bodies.get(&body).cloned())
            };
            if let (Some(body), Some(technique)) =
                (geometry.technique_body_by_slot[tech_slot], &technique)
            {
                self.link.technique_bodies.insert(body, technique.clone());
            }
            slots.push(technique);
        }
        OwnedTechniqueGraph {
            slots,
            rows_truncated: graph.rows_truncated,
            arguments_truncated: graph.arguments_truncated,
        }
    }

    fn capture_technique_set(&mut self, s: &ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_technique_set()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_default();

        let graph = s
            .latest_technique_graph()
            .map(|graph| self.capture_owned_technique_graph(geometry, graph));
        let table = (geometry.technique_slots != 0).then_some(TechniqueTable {
            slots: geometry.technique_slots,
            scanned: geometry.technique_slots_scanned,
            technique0_flags: geometry.technique0_flags,
            model_lighting_const: Some(geometry.uses_model_lighting_const),
            max_pass_count: geometry.max_pass_count,
            pass_count_by_slot: geometry.pass_count_by_slot,
            graph,
        });

        if let Some(graph) = s.latest_technique_graph() {
            if graph.rows_truncated != 0 || graph.arguments_truncated != 0 {
                diag::warn!(
                    Zone,
                    "technique graph truncated for {name}: rows_truncated={} arguments_truncated={} (fixed capture cap)",
                    graph.rows_truncated,
                    graph.arguments_truncated
                );
            }
        }
        Some(
            self.take_techset_slot(TechniqueSetFacts {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                zone: self.capture_zone,
                table,
                t5_occupancy: None,
                iw5_fallback_table: None,
                t5_fallback_table: None,
                world_vert_format: geometry.world_vert_format,
            }),
        )
    }

    fn capture_shader(&mut self, s: &ZoneStream<'_>, kind: AssetType) -> Option<usize> {
        let geometry = s.latest_shader()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let mut bytes = Vec::with_capacity(geometry.program_words * 4);
        if let Some(program) = geometry.program {
            for i in 0..geometry.program_words {
                bytes.extend_from_slice(&s.u32_at(program, i * 4).ok()?.to_le_bytes());
            }
        }
        Some(
            self.take_shader_slot(AuthoredShader {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                kind,
                program: bytes,
            }),
        )
    }

    fn capture_vertex_decl(&mut self, s: &ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_vertex_decl()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_default();
        Some(self.take_vertex_decl_slot(AuthoredVertexDecl {
            family: crate::VertexLayoutFamily::Iw4,
            name,
            stream_count: geometry.stream_count,
            has_optional_source: geometry.has_optional_source,
            routing: geometry.routing,
        }))
    }
}

impl AssetLinkSink for MaterialCatalog {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> Result<()> {
        let (map, index) = match ty {
            AssetType::Image => {
                let Some(geometry) = s.latest_image() else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                let Some(index) = self.capture_image(s, geometry) else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                (&mut self.link.images, index)
            }
            AssetType::Material => {
                let header = s.latest_material().and_then(|g| g.header);
                let Some(index) = self.capture_material(s) else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                if let Some(header) = header {
                    Self::bind(&mut self.link.materials, header, Link::Direct(index));
                }
                (&mut self.link.materials, index)
            }
            AssetType::TechniqueSet => {
                let Some(index) = self.capture_technique_set(s) else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                (&mut self.link.techsets, index)
            }
            AssetType::VertexDecl => {
                let Some(index) = self.capture_vertex_decl(s) else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                (&mut self.link.vertex_decls, index)
            }
            AssetType::PixelShader | AssetType::VertexShader => {
                let Some(index) = self.capture_shader(s, ty) else {
                    self.capture_gaps += 1;
                    return Ok(());
                };
                let links = match ty {
                    AssetType::VertexShader => &mut self.link.vertex_shaders,
                    AssetType::PixelShader => &mut self.link.pixel_shaders,
                    _ => unreachable!("shader arm accepts only vertex or pixel assets"),
                };
                Self::bind(links, slot, Link::Direct(index));
                if let Some(insert_slot) = insert_slot {
                    Self::bind(links, insert_slot, Link::Direct(index));
                }
                return Ok(());
            }
            _ => return Ok(()),
        };
        Self::bind(map, slot, Link::Direct(index));
        if let Some(insert_slot) = insert_slot {
            Self::bind(map, insert_slot, Link::Direct(index));
        }

        if ty == AssetType::Material {
            if let Some(header) = s.latest_material().and_then(|g| g.header) {
                Self::bind(map, header, Link::Direct(index));
            }
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> Result<()> {
        let map = match ty {
            AssetType::Image => &mut self.link.images,
            AssetType::Material => &mut self.link.materials,
            AssetType::TechniqueSet => {
                self.link.last_technique_set = resolve(&self.link.techsets, target)
                    .and_then(|index| self.techsets.get(index).cloned());
                &mut self.link.techsets
            }
            AssetType::VertexShader => &mut self.link.vertex_shaders,
            AssetType::PixelShader => &mut self.link.pixel_shaders,
            AssetType::VertexDecl => &mut self.link.vertex_decls,
            _ => return Ok(()),
        };
        Self::bind(map, slot, Link::Alias(target));
        Ok(())
    }

    fn linked_asset_name(&self, slot: Ptr) -> Option<&str> {
        let index = self.material_index(slot)?;
        let name = self.materials.get(index.get())?.name.as_str();
        (!name.is_empty()).then_some(name)
    }
}

fn merge_world_vert_format(incoming: u8, existing: u8) -> u8 {
    if incoming != 0 { incoming } else { existing }
}

pub fn t5_feature_token_stripped(name: &str) -> String {
    name.replace("x0", "").replace("x1", "")
}

fn table_shader_identity_ready(table: &TechniqueTable) -> bool {
    table.graph.as_ref().is_some_and(|graph| {
        graph.slots.iter().flatten().any(|technique| {
            technique.passes.iter().any(|pass| {
                pass.vertex_shader.shader.is_some() && pass.pixel_shader.shader.is_some()
            })
        })
    })
}

fn merge_t5_occupancy(
    incoming: Option<T5TechniqueOccupancy>,
    existing: Option<T5TechniqueOccupancy>,
) -> Option<T5TechniqueOccupancy> {
    match (incoming, existing) {
        (None, None) => None,
        (Some(occupancy), None) | (None, Some(occupancy)) => Some(occupancy),
        (Some(mut incoming), Some(existing)) => {
            for i in 0..fastfile_t5::TECHNIQUE_OCCUPANCY_WORDS {
                incoming.slots[i] |= existing.slots[i];
                incoming.scanned[i] |= existing.scanned[i];
            }
            incoming.max_pass_count = incoming.max_pass_count.max(existing.max_pass_count);
            if incoming.technique0_flags == 0 {
                incoming.technique0_flags = existing.technique0_flags;
            }
            for i in 0..fastfile_t5::TECHNIQUE_SLOT_COUNT {
                if incoming.pass_count_by_slot[i] == 0 {
                    incoming.pass_count_by_slot[i] = existing.pass_count_by_slot[i];
                }
            }
            Some(incoming)
        }
    }
}

fn route_from_table(
    sort_key: u8,
    info_game_flags: u8,
    state_flags: u8,
    table: &TechniqueTable,
) -> asset_iw4::MaterialDrawRoute {
    asset_iw4::MaterialDrawRoute::route(
        sort_key,
        dpvs_iw4::material_prepass(
            table.slots & 1 == 0,
            table.slots & 2 != 0,
            state_flags,
            table.technique0_flags,
        ),
        info_game_flags >> 6,
        table.slots,
        table.model_lighting_const.unwrap_or(false),
    )
}

fn argument_is_model_lighting_const(argument: &OwnedShaderArgument) -> bool {
    matches!(
        argument,
        OwnedShaderArgument::CodeVertexConstant {
            index: asset_iw4::material::CODE_CONST_MODEL_LIGHTING,
            ..
        } | OwnedShaderArgument::CodePixelConstant {
            index: asset_iw4::material::CODE_CONST_MODEL_LIGHTING,
            ..
        }
    )
}

pub(crate) fn graph_slots_bind_model_lighting_const(slots: &[Option<OwnedTechnique>]) -> bool {
    slots.iter().flatten().any(|technique| {
        technique
            .passes
            .iter()
            .any(|pass| pass.arguments.iter().any(argument_is_model_lighting_const))
    })
}

fn read_state_bits(mut word: impl FnMut(usize) -> Option<u32>, count: usize) -> Vec<[u32; 2]> {
    let mut table = Vec::with_capacity(count);
    for index in 0..count {
        let offset = index * asset_iw4::size::GFX_STATE_BITS;
        let (Some(low), Some(high)) = (word(offset), word(offset + 4)) else {
            break;
        };
        table.push([low, high]);
    }
    table
}

fn t5_colour_keeps_prepass_depth(
    entry: Option<&[u8; fastfile_t5::TECHNIQUE_SLOT_COUNT]>,
    mut table: Vec<[u32; 2]>,
) -> Vec<[u32; 2]> {
    const DEPTH_PREPASS: usize = 0;
    let prepass_writes_depth = entry
        .map(|entry| usize::from(entry[DEPTH_PREPASS]))
        .and_then(|row| table.get(row))
        .is_some_and(|bits| bits[1] & asset_iw4::GFXS1_DEPTHWRITE != 0);
    if !prepass_writes_depth {
        return table;
    }
    for bits in &mut table {
        let unblended = matches!(
            crate::MaterialDrawMode::from_state_bits(*bits),
            crate::MaterialDrawMode::Opaque | crate::MaterialDrawMode::AlphaTest { .. }
        );
        if unblended && bits[1] & asset_iw4::GFXS1_DEPTHTEST_DISABLE == 0 {
            bits[1] |= asset_iw4::GFXS1_DEPTHWRITE;
        }
    }
    table
}

fn resolve(map: &HashMap<Ptr, Link>, mut slot: Ptr) -> Option<usize> {
    for _ in 0..32 {
        match map.get(&slot).copied()? {
            Link::Direct(index) => return Some(index),
            Link::Alias(target) => slot = target,
        }
    }
    None
}

fn iw4_ptr(p: fastfile_t5::Ptr) -> Ptr {
    Ptr {
        block: p.block,
        offset: p.offset,
    }
}

fn remap_iw5_code_const_source(iw5: u16) -> Option<u16> {
    crate::iw5_tech_map::remap_code_const_index(iw5)
        .or_else(|| crate::iw5_tech_map::leftover_iw5_code_bank(iw5))
}

fn remap_iw5_owned_shader_argument(argument: OwnedShaderArgument) -> Option<OwnedShaderArgument> {
    match argument {
        OwnedShaderArgument::CodeVertexConstant {
            destination,
            index,
            first_row,
            row_count,
        } => remap_iw5_code_const_source(index).map(|index| {
            OwnedShaderArgument::CodeVertexConstant {
                destination,
                index,
                first_row,
                row_count,
            }
        }),
        OwnedShaderArgument::CodePixelConstant {
            destination,
            index,
            first_row,
            row_count,
        } => {
            remap_iw5_code_const_source(index).map(|index| OwnedShaderArgument::CodePixelConstant {
                destination,
                index,
                first_row,
                row_count,
            })
        }
        OwnedShaderArgument::CodePixelSampler { destination, index } => {
            crate::iw5_tech_map::remap_code_texture_index(index)
                .map(|index| OwnedShaderArgument::CodePixelSampler { destination, index })
        }
        other => Some(other),
    }
}

fn take_iw5_pass_argument(
    raw: [u8; 8],
    literal_words: [u32; 4],
    literal_present: bool,
) -> Option<OwnedShaderArgument> {
    let iw5_type = u16::from_le_bytes([raw[0], raw[1]]);
    let argument_type = crate::iw5_tech_map::remap_shader_arg_type(iw5_type)?;
    remap_iw5_owned_shader_argument(owned_shader_argument(
        argument_type,
        raw,
        literal_words,
        literal_present,
    ))
}

fn remap_t5_owned_shader_argument(argument: OwnedShaderArgument) -> Option<OwnedShaderArgument> {
    match argument {
        OwnedShaderArgument::CodeVertexConstant {
            destination,
            index,
            first_row,
            row_count,
        } => crate::t5_code_remap::remap_t5_code_const_source(index).map(|index| {
            OwnedShaderArgument::CodeVertexConstant {
                destination,
                index,
                first_row,
                row_count,
            }
        }),
        OwnedShaderArgument::CodePixelConstant {
            destination,
            index,
            first_row,
            row_count,
        } => crate::t5_code_remap::remap_t5_code_const_source(index).map(|index| {
            OwnedShaderArgument::CodePixelConstant {
                destination,
                index,
                first_row,
                row_count,
            }
        }),
        OwnedShaderArgument::CodePixelSampler { destination, index }
            if (0x1d..=0x20).contains(&index) =>
        {
            Some(OwnedShaderArgument::MaterialPixelSampler {
                destination,
                name_hash: crate::t5_code_remap::terrain_scorch_binding_hash(
                    (index - 0x1d) as usize,
                ),
            })
        }
        OwnedShaderArgument::CodePixelSampler { destination, index } => {
            crate::t5_code_remap::remap_code_texture_index(index)
                .map(|index| OwnedShaderArgument::CodePixelSampler { destination, index })
        }
        other => Some(other),
    }
}

fn take_t5_pass_argument(
    raw: [u8; 8],
    literal_words: [u32; 4],
    literal_present: bool,
) -> Option<OwnedShaderArgument> {
    let argument_type = u16::from_le_bytes([raw[0], raw[1]]);
    remap_t5_owned_shader_argument(owned_shader_argument(
        argument_type,
        raw,
        literal_words,
        literal_present,
    ))
}

fn owned_shader_argument(
    argument_type: u16,
    raw: [u8; 8],
    literal_words: [u32; 4],
    literal_present: bool,
) -> OwnedShaderArgument {
    let destination = u16::from_le_bytes([raw[2], raw[3]]);
    let payload = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
    let code_constant = (u16::from_le_bytes([raw[4], raw[5]]), raw[6], raw[7]);
    match argument_type {
        asset_iw4::size::mtl_arg::MATERIAL_VERTEX_CONST => {
            OwnedShaderArgument::MaterialVertexConstant {
                destination,
                name_hash: payload,
            }
        }
        asset_iw4::size::mtl_arg::LITERAL_VERTEX_CONST => {
            OwnedShaderArgument::LiteralVertexConstant {
                destination,
                words: literal_present.then_some(literal_words),
            }
        }
        asset_iw4::size::mtl_arg::MATERIAL_PIXEL_SAMPLER => {
            OwnedShaderArgument::MaterialPixelSampler {
                destination,
                name_hash: payload,
            }
        }
        asset_iw4::size::mtl_arg::CODE_VERTEX_CONST => {
            let (index, first_row, row_count) = code_constant;
            OwnedShaderArgument::CodeVertexConstant {
                destination,
                index,
                first_row,
                row_count,
            }
        }
        asset_iw4::size::mtl_arg::CODE_PIXEL_SAMPLER => OwnedShaderArgument::CodePixelSampler {
            destination,
            index: payload,
        },
        asset_iw4::size::mtl_arg::CODE_PIXEL_CONST => {
            let (index, first_row, row_count) = code_constant;
            OwnedShaderArgument::CodePixelConstant {
                destination,
                index,
                first_row,
                row_count,
            }
        }
        asset_iw4::size::mtl_arg::MATERIAL_PIXEL_CONST => {
            OwnedShaderArgument::MaterialPixelConstant {
                destination,
                name_hash: payload,
            }
        }
        asset_iw4::size::mtl_arg::LITERAL_PIXEL_CONST => {
            OwnedShaderArgument::LiteralPixelConstant {
                destination,
                words: literal_present.then_some(literal_words),
            }
        }
        _ => OwnedShaderArgument::Unknown { argument_type, raw },
    }
}

fn iw5_ptr(p: fastfile_iw5::Ptr) -> Ptr {
    Ptr {
        block: p.block,
        offset: p.offset,
    }
}

impl MaterialCatalog {
    pub fn t5_loaded(
        &mut self,
        s: &fastfile_t5::ZoneStream<'_>,
        ty: fastfile_t5::AssetType,
        slot: fastfile_t5::Ptr,
        insert_slot: Option<fastfile_t5::Ptr>,
    ) {
        use fastfile_t5::AssetType as T5;
        let slot = iw4_ptr(slot);
        let insert_slot = insert_slot.map(iw4_ptr);
        let (map, index) = match ty {
            T5::Image => {
                let Some(geometry) = s.latest_image() else {
                    self.capture_gaps += 1;
                    return;
                };
                let Some(index) = self.capture_image_t5(s, geometry) else {
                    self.capture_gaps += 1;
                    return;
                };
                (&mut self.link.images, index)
            }
            T5::Material => {
                let Some(index) = self.capture_material_t5(s) else {
                    self.capture_gaps += 1;
                    return;
                };
                (&mut self.link.materials, index)
            }
            T5::TechniqueSet => {
                let Some(index) = self.capture_technique_set_t5(s) else {
                    self.capture_gaps += 1;
                    return;
                };
                (&mut self.link.techsets, index)
            }
            _ => return,
        };
        Self::bind(map, slot, Link::Direct(index));
        if let Some(insert_slot) = insert_slot {
            Self::bind(map, insert_slot, Link::Direct(index));
        }
    }

    pub fn t5_alias(
        &mut self,
        ty: fastfile_t5::AssetType,
        slot: fastfile_t5::Ptr,
        target: fastfile_t5::Ptr,
    ) {
        use fastfile_t5::AssetType as T5;
        let slot = iw4_ptr(slot);
        let target = iw4_ptr(target);
        let map = match ty {
            T5::Image => &mut self.link.images,
            T5::Material => &mut self.link.materials,
            T5::TechniqueSet => {
                self.link.last_technique_set = resolve(&self.link.techsets, target)
                    .and_then(|index| self.techsets.get(index).cloned());
                &mut self.link.techsets
            }
            _ => return,
        };
        Self::bind(map, slot, Link::Alias(target));
    }

    pub fn t5_nested_shader(
        &mut self,
        s: &fastfile_t5::ZoneStream<'_>,
        kind: fastfile_t5::NestedShaderKind,
        slot: fastfile_t5::Ptr,
    ) {
        let kind = match kind {
            fastfile_t5::NestedShaderKind::Vertex => AssetType::VertexShader,
            fastfile_t5::NestedShaderKind::Pixel => AssetType::PixelShader,
        };
        let Some(index) = self.capture_shader_t5(s, kind) else {
            self.capture_gaps += 1;
            return;
        };
        let links = match kind {
            AssetType::VertexShader => &mut self.link.vertex_shaders,
            AssetType::PixelShader => &mut self.link.pixel_shaders,
            _ => return,
        };
        let slot = iw4_ptr(slot);
        Self::bind(links, slot, Link::Direct(index));
        if let Some(header) = s.latest_shader().and_then(|g| g.header) {
            Self::bind(links, iw4_ptr(header), Link::Direct(index));
        }
    }

    pub fn t5_nested_shader_alias(
        &mut self,
        kind: fastfile_t5::NestedShaderKind,
        slot: fastfile_t5::Ptr,
        target: fastfile_t5::Ptr,
    ) {
        let links = match kind {
            fastfile_t5::NestedShaderKind::Vertex => &mut self.link.vertex_shaders,
            fastfile_t5::NestedShaderKind::Pixel => &mut self.link.pixel_shaders,
        };
        Self::bind(links, iw4_ptr(slot), Link::Alias(iw4_ptr(target)));
    }

    pub fn t5_nested_vertex_decl(
        &mut self,
        s: &fastfile_t5::ZoneStream<'_>,
        slot: fastfile_t5::Ptr,
    ) {
        let Some(index) = self.capture_vertex_decl_t5(s) else {
            self.capture_gaps += 1;
            return;
        };
        let slot = iw4_ptr(slot);
        Self::bind(&mut self.link.vertex_decls, slot, Link::Direct(index));
        if let Some(header) = s.latest_vertex_decl().and_then(|g| g.header) {
            Self::bind(
                &mut self.link.vertex_decls,
                iw4_ptr(header),
                Link::Direct(index),
            );
        }
    }

    pub fn t5_nested_vertex_decl_alias(
        &mut self,
        slot: fastfile_t5::Ptr,
        target: fastfile_t5::Ptr,
    ) {
        Self::bind(
            &mut self.link.vertex_decls,
            iw4_ptr(slot),
            Link::Alias(iw4_ptr(target)),
        );
    }

    pub fn iw5_loaded(
        &mut self,
        s: &fastfile_iw5::ZoneStream<'_>,
        ty: fastfile_iw5::AssetType,
        slot: fastfile_iw5::Ptr,
        insert_slot: Option<fastfile_iw5::Ptr>,
    ) {
        use fastfile_iw5::AssetType as Iw5;
        let slot = iw5_ptr(slot);
        let insert_slot = insert_slot.map(iw5_ptr);
        let (map, index) = match ty {
            Iw5::Image => {
                let Some(geometry) = s.latest_image() else {
                    self.capture_gaps += 1;
                    return;
                };
                let Some(index) = self.capture_image_iw5(s, geometry) else {
                    self.capture_gaps += 1;
                    return;
                };
                (&mut self.link.images, index)
            }
            Iw5::Material => {
                let Some(index) = self.capture_material_iw5(s) else {
                    self.capture_gaps += 1;
                    return;
                };
                (&mut self.link.materials, index)
            }
            Iw5::TechniqueSet => {
                let Some(index) = self.capture_technique_set_iw5(s) else {
                    self.capture_gaps += 1;
                    return;
                };
                (&mut self.link.techsets, index)
            }
            Iw5::VertexDecl => {
                let Some(index) = self.capture_vertex_decl_iw5(s) else {
                    self.capture_gaps += 1;
                    return;
                };
                (&mut self.link.vertex_decls, index)
            }
            Iw5::PixelShader | Iw5::VertexShader => {
                let Some(index) = self.capture_shader_iw5(s, ty) else {
                    self.capture_gaps += 1;
                    return;
                };
                let links = match ty {
                    Iw5::VertexShader => &mut self.link.vertex_shaders,
                    Iw5::PixelShader => &mut self.link.pixel_shaders,
                    _ => unreachable!("shader arm accepts only vertex or pixel assets"),
                };
                Self::bind(links, slot, Link::Direct(index));
                if let Some(insert_slot) = insert_slot {
                    Self::bind(links, insert_slot, Link::Direct(index));
                }
                return;
            }
            _ => return,
        };
        Self::bind(map, slot, Link::Direct(index));
        if let Some(insert_slot) = insert_slot {
            Self::bind(map, insert_slot, Link::Direct(index));
        }
    }

    pub fn iw5_alias(
        &mut self,
        ty: fastfile_iw5::AssetType,
        slot: fastfile_iw5::Ptr,
        target: fastfile_iw5::Ptr,
    ) {
        use fastfile_iw5::AssetType as Iw5;
        let slot = iw5_ptr(slot);
        let target = iw5_ptr(target);
        let map = match ty {
            Iw5::Image => &mut self.link.images,
            Iw5::Material => &mut self.link.materials,
            Iw5::TechniqueSet => {
                self.link.last_technique_set = resolve(&self.link.techsets, target)
                    .and_then(|index| self.techsets.get(index).cloned());
                &mut self.link.techsets
            }
            Iw5::VertexDecl => &mut self.link.vertex_decls,
            Iw5::VertexShader => &mut self.link.vertex_shaders,
            Iw5::PixelShader => &mut self.link.pixel_shaders,
            _ => return,
        };
        Self::bind(map, slot, Link::Alias(target));
    }

    fn capture_image_t5(
        &mut self,
        s: &fastfile_t5::ZoneStream<'_>,
        geometry: fastfile_t5::GfxImageGeometry,
    ) -> Option<usize> {
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let payload = if geometry.source_len == 0 {
            Vec::new()
        } else {
            s.source_slice(geometry.source_offset, geometry.source_len)
                .ok()?
                .to_vec()
        };
        Some(
            self.take_image_slot(AuthoredImage {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                map_type: geometry.map_type,
                semantic: geometry.semantic,
                category: geometry.category,
                use_srgb_reads: geometry.use_srgb_reads,
                width: geometry.width,
                height: geometry.height,
                depth: geometry.depth,
                level_count: geometry.level_count,
                format: geometry.format,
                payload: Arc::new(payload),
                decoded: None,
                common_owned: false,
                decoded_variant: None,
                decoded_by: None,
                pending_decode: None,
            }),
        )
    }

    fn capture_material_t5(&mut self, s: &fastfile_t5::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_material()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let technique_set = self.link.last_technique_set.take()?;
        let mut textures = Vec::with_capacity(geometry.texture_count);
        if let Some(table) = geometry.textures {
            for i in 0..geometry.texture_count {
                let texture = table.at(i * 16);
                let semantic = s.u8_at(texture, 7).ok()?;
                textures.push(MaterialTextureBinding {
                    name_hash: s.u32_at(texture, 0).ok()?,
                    name_start: s.u8_at(texture, 4).ok()?,
                    name_end: s.u8_at(texture, 5).ok()?,
                    sampler_state: s.u8_at(texture, 6).ok()?,
                    semantic,
                    image: (semantic != 11)
                        .then(|| resolve(&self.link.images, iw4_ptr(texture.at(12))))
                        .flatten(),
                });
            }
        }
        let mut constants = Vec::with_capacity(geometry.constant_count);
        if let Some(table) = geometry.constants {
            for i in 0..geometry.constant_count {
                let constant = table.at(i * 32);
                let mut name = [0; 12];
                for (offset, byte) in name.iter_mut().enumerate() {
                    *byte = s.u8_at(constant, 4 + offset).ok()?;
                }
                constants.push(MaterialConstant {
                    name_hash: s.u32_at(constant, 0).ok()?,
                    name,
                    literal: [
                        s.f32_at(constant, 16).ok()?,
                        s.f32_at(constant, 20).ok()?,
                        s.f32_at(constant, 24).ok()?,
                        s.f32_at(constant, 28).ok()?,
                    ],
                });
            }
        }
        Some(
            self.take_material_slot(AuthoredMaterial {
                name,
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                technique_set_edge: if technique_set.name.is_empty() {
                    crate::AssetEdge::Absent
                } else {
                    crate::AssetEdge::Unresolved(crate::AssetEdgeReason::CatalogMiss)
                },
                technique_set: technique_set.name,
                draw_surf: geometry.draw_surf,
                sort_key: geometry.sort_key,

                info_game_flags: geometry.info_game_flags,
                texture_atlas: None,
                surface_type_bits: Some(geometry.surface_type_bits),
                t5_layered_surface_types: Some(geometry.layered_surface_types),
                state_flags: geometry.state_flags,
                camera_region: geometry.camera_region,
                state_bits: t5_colour_keeps_prepass_depth(
                    geometry.state_bits_entry.as_ref(),
                    read_state_bits(
                        |offset| s.u32_at(geometry.state_bits?, offset).ok(),
                        geometry.state_bits_count,
                    ),
                ),

                state_bits_entry: geometry
                    .state_bits_entry
                    .as_ref()
                    .map(crate::t5_tech_map::remap_t5_state_bits_entry),
                t5_state_bits_entry: geometry.state_bits_entry,
                iw5_state_bits_entry: None,
                technique_table: None,
                route: None,
                textures,
                constants,
                zone: self.capture_zone,
            }),
        )
    }

    fn capture_technique_set_t5(&mut self, s: &fastfile_t5::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_technique_set()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_default();
        let occupancy =
            fastfile_t5::occupancy_any(&geometry.technique_slots).then_some(T5TechniqueOccupancy {
                slots: geometry.technique_slots,
                scanned: geometry.technique_slots_scanned,
                technique0_flags: geometry.technique0_flags,
                max_pass_count: geometry.max_pass_count,
                pass_count_by_slot: geometry.pass_count_by_slot,
            });

        let fallback = s.latest_technique_graph().and_then(|graph| {
            fastfile_t5::occupancy_any(&geometry.technique_slots)
                .then(|| self.capture_owned_technique_graph_t5(geometry, graph))
        });
        if let Some(graph) = s.latest_technique_graph() {
            if graph.rows_truncated != 0 || graph.arguments_truncated != 0 {
                diag::warn!(
                    Zone,
                    "T5 technique graph truncated for {name}: rows_truncated={} arguments_truncated={}",
                    graph.rows_truncated,
                    graph.arguments_truncated
                );
            }
        }
        Some(
            self.take_techset_slot(TechniqueSetFacts {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                zone: self.capture_zone,
                table: None,
                t5_occupancy: occupancy,
                iw5_fallback_table: None,
                t5_fallback_table: fallback,
                world_vert_format: geometry.world_vert_format,
            }),
        )
    }

    fn capture_owned_technique_graph_t5(
        &mut self,
        geometry: fastfile_t5::TechniqueSetGeometry,
        graph: &fastfile_t5::TechniqueGraphGeometry,
    ) -> TechniqueTable {
        let slots_bits = crate::t5_tech_map::remap_occupancy_bits(&geometry.technique_slots);
        let scanned_bits =
            crate::t5_tech_map::remap_occupancy_bits(&geometry.technique_slots_scanned);
        let pass_count_by_slot =
            crate::t5_tech_map::remap_pass_count_by_slot(&geometry.pass_count_by_slot);
        let flags_by_slot =
            crate::t5_tech_map::remap_technique_flags_by_slot(&geometry.technique_flags_by_slot);
        let mut slots = Vec::with_capacity(asset_iw4::size::TECHNIQUE_SLOT_COUNT);
        for iw4_slot in 0..asset_iw4::size::TECHNIQUE_SLOT_COUNT {
            if slots_bits & (1u64 << iw4_slot) == 0 {
                slots.push(None);
                continue;
            }
            let mut passes = Vec::new();
            for row in graph.iter_rows() {
                if crate::t5_tech_map::iw4_slot_to_t5(iw4_slot) != Some(usize::from(row.tech_slot))
                {
                    continue;
                }
                let authored = graph.arguments_for(row);
                let prim_n = usize::from(row.per_prim_arg_count);
                let obj_n = usize::from(row.per_obj_arg_count);
                let mut arguments = Vec::with_capacity(authored.len());
                let mut per_prim_arg_count = 0u8;
                let mut per_obj_arg_count = 0u8;
                let mut stable_arg_count = 0u8;
                for (index, argument) in authored.iter().enumerate() {
                    match take_t5_pass_argument(
                        argument.raw,
                        argument.literal_words,
                        argument.literal_present,
                    ) {
                        Some(owned) => {
                            arguments.push(owned);
                            if index < prim_n {
                                per_prim_arg_count = per_prim_arg_count.saturating_add(1);
                            } else if index < prim_n.saturating_add(obj_n) {
                                per_obj_arg_count = per_obj_arg_count.saturating_add(1);
                            } else {
                                stable_arg_count = stable_arg_count.saturating_add(1);
                            }
                        }
                        None => self.note_leftover_t5_arg(argument.raw),
                    }
                }
                let vertex_decl_slot = iw4_ptr(row.vertex_decl_slot);
                let vertex_shader_slot = iw4_ptr(row.vertex_shader_slot);
                let pixel_shader_slot = iw4_ptr(row.pixel_shader_slot);
                passes.push(OwnedMaterialPass {
                    pass_index: row.pass_index,
                    vertex_decl_identity: vertex_decl_slot.into(),
                    vertex_decl: resolve(&self.link.vertex_decls, vertex_decl_slot),
                    vertex_shader: OwnedShaderRef {
                        pointer_identity: vertex_shader_slot.into(),
                        shader: resolve(&self.link.vertex_shaders, vertex_shader_slot),
                    },
                    pixel_shader: OwnedShaderRef {
                        pointer_identity: pixel_shader_slot.into(),
                        shader: resolve(&self.link.pixel_shaders, pixel_shader_slot),
                    },
                    per_prim_arg_count,
                    per_obj_arg_count,
                    stable_arg_count,
                    custom_sampler_flags: crate::t5_code_remap::remap_t5_custom_sampler_flags(
                        row.custom_sampler_flags,
                    ),
                    t5_custom_sampler_flags: row.custom_sampler_flags,
                    arguments,
                    arguments_truncated: row.arguments_truncated,
                });
            }
            let scanned = scanned_bits & (1u64 << iw4_slot) != 0;
            let t5_slot = crate::t5_tech_map::iw4_slot_to_t5(iw4_slot);
            let technique = if scanned {
                Some(OwnedTechnique {
                    source_selection: None,
                    flags: flags_by_slot[iw4_slot],
                    passes,
                    body_scanned: true,
                })
            } else {
                t5_slot
                    .and_then(|slot| geometry.technique_body_by_slot.get(slot).copied().flatten())
                    .and_then(|body| self.link.technique_bodies.get(&iw4_ptr(body)).cloned())
            };
            if let (Some(slot), Some(technique)) = (t5_slot, &technique) {
                if let Some(body) = geometry.technique_body_by_slot.get(slot).copied().flatten() {
                    self.link
                        .technique_bodies
                        .insert(iw4_ptr(body), technique.clone());
                }
            }
            slots.push(technique);
        }
        TechniqueTable {
            slots: slots_bits,
            scanned: scanned_bits,
            technique0_flags: geometry.technique0_flags,
            model_lighting_const: Some(graph_slots_bind_model_lighting_const(&slots)),
            max_pass_count: geometry.max_pass_count,
            pass_count_by_slot,
            graph: Some(OwnedTechniqueGraph {
                slots,
                rows_truncated: graph.rows_truncated,
                arguments_truncated: graph.arguments_truncated,
            }),
        }
    }

    fn capture_shader_t5(
        &mut self,
        s: &fastfile_t5::ZoneStream<'_>,
        kind: AssetType,
    ) -> Option<usize> {
        let geometry = s.latest_shader()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_default();
        let mut bytes = Vec::with_capacity(geometry.program_words * 4);
        if let Some(program) = geometry.program {
            for i in 0..geometry.program_words {
                bytes.extend_from_slice(&s.u32_at(program, i * 4).ok()?.to_le_bytes());
            }
        }
        Some(
            self.take_shader_slot(AuthoredShader {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                kind,
                program: bytes,
            }),
        )
    }

    fn capture_vertex_decl_t5(&mut self, s: &fastfile_t5::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_vertex_decl()?;
        let mut routing = [[0u8; 2]; asset_iw4::vertex_decl::ROUTING_COUNT];
        for (index, pair) in geometry.routing.iter().enumerate() {
            if let Some(slot) = routing.get_mut(index) {
                *slot = *pair;
            }
        }
        Some(self.take_vertex_decl_slot(AuthoredVertexDecl {
            family: crate::VertexLayoutFamily::T5,
            name: AssetRef::default(),
            stream_count: geometry.stream_count,
            has_optional_source: geometry.has_optional_source,
            routing,
        }))
    }

    fn capture_image_iw5(
        &mut self,
        s: &fastfile_iw5::ZoneStream<'_>,
        geometry: fastfile_iw5::GfxImageGeometry,
    ) -> Option<usize> {
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let payload = if geometry.source_len == 0 {
            Vec::new()
        } else {
            s.source_slice(geometry.source_offset, geometry.source_len)
                .ok()?
                .to_vec()
        };
        Some(
            self.take_image_slot(AuthoredImage {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                map_type: geometry.map_type,
                semantic: geometry.semantic,
                category: geometry.category,
                use_srgb_reads: geometry.use_srgb_reads,
                width: geometry.width,
                height: geometry.height,
                depth: geometry.depth,
                level_count: geometry.level_count,
                format: geometry.format,
                payload: Arc::new(payload),
                decoded: None,
                common_owned: false,
                decoded_variant: None,
                decoded_by: None,
                pending_decode: None,
            }),
        )
    }

    fn capture_material_iw5(&mut self, s: &fastfile_iw5::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_material()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let technique_set = self.link.last_technique_set.take()?;
        let mut textures = Vec::with_capacity(geometry.texture_count);
        if let Some(table) = geometry.textures {
            for i in 0..geometry.texture_count {
                let texture = table.at(i * s.layout(fastfile_iw5::size::MATERIAL_TEXTURE_DEF, 16));
                let semantic = s.u8_at(texture, 7).ok()?;
                textures.push(MaterialTextureBinding {
                    name_hash: s.u32_at(texture, 0).ok()?,
                    name_start: s.u8_at(texture, 4).ok()?,
                    name_end: s.u8_at(texture, 5).ok()?,
                    sampler_state: s.u8_at(texture, 6).ok()?,
                    semantic,
                    image: (semantic != 11)
                        .then(|| {
                            resolve(
                                &self.link.images,
                                iw5_ptr(texture.at(fastfile_iw5::size::MATERIAL_TEXTURE_DEF_U_OFF)),
                            )
                        })
                        .flatten(),
                });
            }
        }
        let mut constants = Vec::with_capacity(geometry.constant_count);
        if let Some(table) = geometry.constants {
            for i in 0..geometry.constant_count {
                let constant = table.at(i * 32);
                let mut name = [0; 12];
                for (offset, byte) in name.iter_mut().enumerate() {
                    *byte = s.u8_at(constant, 4 + offset).ok()?;
                }
                constants.push(MaterialConstant {
                    name_hash: s.u32_at(constant, 0).ok()?,
                    name,
                    literal: [
                        s.f32_at(constant, 16).ok()?,
                        s.f32_at(constant, 20).ok()?,
                        s.f32_at(constant, 24).ok()?,
                        s.f32_at(constant, 28).ok()?,
                    ],
                });
            }
        }
        let header = geometry.header?;

        let info_game_flags = s.u8_at(header, s.layout(4, 8)).ok()?;
        let counts = s.layout(fastfile_iw5::size::MATERIAL_TEXTURE_COUNT_OFF, 86);
        let state_flags = s.u8_at(header, counts + 3).ok()?;
        let camera_region = s.u8_at(header, counts + 4).ok()?;
        let iw5_state_bits_entry = geometry.state_bits_entry;
        let state_bits_entry = iw5_state_bits_entry
            .as_ref()
            .map(crate::iw5_tech_map::remap_state_bits_entry);
        Some(
            self.take_material_slot(AuthoredMaterial {
                name,
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                technique_set_edge: if technique_set.name.is_empty() {
                    crate::AssetEdge::Absent
                } else {
                    crate::AssetEdge::Unresolved(crate::AssetEdgeReason::CatalogMiss)
                },
                technique_set: technique_set.name,
                draw_surf: geometry.draw_surf,
                sort_key: geometry.sort_key,
                info_game_flags,
                texture_atlas: None,
                surface_type_bits: None,
                t5_layered_surface_types: None,
                state_flags,
                camera_region,
                state_bits: read_state_bits(
                    |offset| s.u32_at(geometry.state_bits?, offset).ok(),
                    geometry.state_bits_count,
                ),
                state_bits_entry,
                t5_state_bits_entry: None,
                iw5_state_bits_entry,
                technique_table: technique_set.table,
                route: None,
                textures,
                constants,
                zone: self.capture_zone,
            }),
        )
    }

    fn capture_technique_set_iw5(&mut self, s: &fastfile_iw5::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_technique_set()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_default();
        let fallback = s.latest_technique_graph().and_then(|graph| {
            (geometry.occupancy != 0)
                .then(|| self.capture_owned_technique_graph_iw5(geometry, graph))
        });
        if let Some(graph) = s.latest_technique_graph() {
            if graph.rows_truncated != 0 || graph.arguments_truncated != 0 {
                diag::warn!(
                    Zone,
                    "IW5 technique graph truncated for {name}: rows_truncated={} arguments_truncated={}",
                    graph.rows_truncated,
                    graph.arguments_truncated
                );
            }
        }

        Some(
            self.take_techset_slot(TechniqueSetFacts {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                zone: self.capture_zone,
                table: None,
                t5_occupancy: None,
                iw5_fallback_table: fallback,
                t5_fallback_table: None,
                world_vert_format: geometry.world_vert_format,
            }),
        )
    }

    fn capture_owned_technique_graph_iw5(
        &mut self,
        geometry: fastfile_iw5::TechniqueSetGeometry,
        graph: &fastfile_iw5::TechniqueGraphGeometry,
    ) -> TechniqueTable {
        let slots_bits = crate::iw5_tech_map::remap_occupancy_bits(geometry.occupancy);
        let scanned_bits = crate::iw5_tech_map::remap_occupancy_bits(geometry.occupancy_scanned);
        let pass_count_by_slot =
            crate::iw5_tech_map::remap_pass_count_by_slot(&geometry.pass_count_by_slot);
        let flags_by_slot =
            crate::iw5_tech_map::remap_technique_flags_by_slot(&geometry.technique_flags_by_slot);
        let mut slots = Vec::with_capacity(asset_iw4::size::TECHNIQUE_SLOT_COUNT);
        for iw4_slot in 0..asset_iw4::size::TECHNIQUE_SLOT_COUNT {
            if slots_bits & (1u64 << iw4_slot) == 0 {
                slots.push(None);
                continue;
            }
            let mut passes = Vec::new();
            for row in graph.iter_rows() {
                let Some(mapped) = crate::iw5_tech_map::iw5_slot_to_iw4(usize::from(row.tech_slot))
                else {
                    continue;
                };
                if mapped != iw4_slot {
                    continue;
                }
                let authored = graph.arguments_for(row);
                let prim_n = usize::from(row.per_prim_arg_count);
                let obj_n = usize::from(row.per_obj_arg_count);
                let mut arguments = Vec::with_capacity(authored.len());
                let mut per_prim_arg_count = 0u8;
                let mut per_obj_arg_count = 0u8;
                let mut stable_arg_count = 0u8;
                for (index, argument) in authored.iter().enumerate() {
                    match take_iw5_pass_argument(
                        argument.raw,
                        argument.literal_words,
                        argument.literal_present,
                    ) {
                        Some(owned) => {
                            arguments.push(owned);
                            if index < prim_n {
                                per_prim_arg_count = per_prim_arg_count.saturating_add(1);
                            } else if index < prim_n.saturating_add(obj_n) {
                                per_obj_arg_count = per_obj_arg_count.saturating_add(1);
                            } else {
                                stable_arg_count = stable_arg_count.saturating_add(1);
                            }
                        }
                        None => {
                            let iw5_type = u16::from_le_bytes([argument.raw[0], argument.raw[1]]);
                            self.note_leftover_iw5_arg(iw5_type, argument.raw);
                        }
                    }
                }
                let vertex_decl_slot = iw5_ptr(row.vertex_decl_slot);
                let vertex_shader_slot = iw5_ptr(row.vertex_shader_slot);
                let pixel_shader_slot = iw5_ptr(row.pixel_shader_slot);
                passes.push(OwnedMaterialPass {
                    pass_index: row.pass_index,
                    vertex_decl_identity: vertex_decl_slot.into(),
                    vertex_decl: resolve(&self.link.vertex_decls, vertex_decl_slot),
                    vertex_shader: OwnedShaderRef {
                        pointer_identity: vertex_shader_slot.into(),
                        shader: resolve(&self.link.vertex_shaders, vertex_shader_slot),
                    },
                    pixel_shader: OwnedShaderRef {
                        pointer_identity: pixel_shader_slot.into(),
                        shader: resolve(&self.link.pixel_shaders, pixel_shader_slot),
                    },
                    per_prim_arg_count,
                    per_obj_arg_count,
                    stable_arg_count,
                    custom_sampler_flags: row.custom_sampler_flags,
                    t5_custom_sampler_flags: 0,
                    arguments,
                    arguments_truncated: row.arguments_truncated,
                });
            }
            let scanned = scanned_bits & (1u64 << iw4_slot) != 0;
            let iw5_slot = crate::iw5_tech_map::iw4_slot_to_iw5(iw4_slot);
            let technique = if scanned {
                Some(OwnedTechnique {
                    source_selection: None,
                    flags: flags_by_slot[iw4_slot],
                    passes,
                    body_scanned: true,
                })
            } else {
                iw5_slot
                    .and_then(|slot| geometry.technique_body_by_slot.get(slot).copied().flatten())
                    .and_then(|body| self.link.technique_bodies.get(&iw5_ptr(body)).cloned())
            };
            if let (Some(slot), Some(technique)) = (iw5_slot, &technique) {
                if let Some(body) = geometry.technique_body_by_slot.get(slot).copied().flatten() {
                    self.link
                        .technique_bodies
                        .insert(iw5_ptr(body), technique.clone());
                }
            }
            slots.push(technique);
        }
        TechniqueTable {
            slots: slots_bits,
            scanned: scanned_bits,
            technique0_flags: geometry.technique0_flags,
            model_lighting_const: Some(graph_slots_bind_model_lighting_const(&slots)),
            max_pass_count: geometry.max_pass_count,
            pass_count_by_slot,
            graph: Some(OwnedTechniqueGraph {
                slots,
                rows_truncated: graph.rows_truncated,
                arguments_truncated: graph.arguments_truncated,
            }),
        }
    }

    fn capture_shader_iw5(
        &mut self,
        s: &fastfile_iw5::ZoneStream<'_>,
        kind: fastfile_iw5::AssetType,
    ) -> Option<usize> {
        let geometry = s.latest_shader()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let mut bytes = Vec::with_capacity(geometry.program_words * 4);
        if let Some(program) = geometry.program {
            for i in 0..geometry.program_words {
                bytes.extend_from_slice(&s.u32_at(program, i * 4).ok()?.to_le_bytes());
            }
        }
        let kind = match kind {
            fastfile_iw5::AssetType::VertexShader => AssetType::VertexShader,
            fastfile_iw5::AssetType::PixelShader => AssetType::PixelShader,
            _ => return None,
        };
        Some(
            self.take_shader_slot(AuthoredShader {
                namespace: self
                    .capture_ns
                    .expect("asset capture requires an explicit family"),
                name,
                kind,
                program: bytes,
            }),
        )
    }

    fn capture_vertex_decl_iw5(&mut self, s: &fastfile_iw5::ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_vertex_decl()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
            .unwrap_or_default();
        let mut routing = [[0u8; 2]; asset_iw4::vertex_decl::ROUTING_COUNT];
        for (index, pair) in geometry.routing.iter().enumerate() {
            if let Some(slot) = routing.get_mut(index) {
                *slot = *pair;
            }
        }
        Some(self.take_vertex_decl_slot(AuthoredVertexDecl {
            family: crate::VertexLayoutFamily::Iw4,
            name,
            stream_count: geometry.stream_count,
            has_optional_source: geometry.has_optional_source,
            routing,
        }))
    }
}

/// Read-only questions about a finished population. They are here and not on
/// the build catalog because the answers do not change any more.
impl MaterialDefinitions {
    pub fn leftover_iw5_arg_top(&self) -> Option<String> {
        self.leftover_iw5_arg_ranked(0)
    }

    pub fn leftover_iw5_arg_ranked(&self, rank: usize) -> Option<String> {
        let mut hits: Vec<(&String, &u32)> = self.leftover_iw5_arg_hits.iter().collect();
        hits.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        hits.get(rank).map(|(key, count)| format!("{key}:{count}"))
    }

    pub fn leftover_t5_arg_top(&self) -> Option<String> {
        self.leftover_t5_arg_ranked(0)
    }

    pub fn leftover_t5_arg_ranked(&self, rank: usize) -> Option<String> {
        let mut hits: Vec<(&String, &u32)> = self.leftover_t5_arg_hits.iter().collect();
        hits.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        hits.get(rank).map(|(key, count)| format!("{key}:{count}"))
    }

    pub fn leftover_t5_arg_dest(&self, dest: u16) -> Option<String> {
        let prefix = format!("d{dest}i");
        let mut hits: Vec<(&String, &u32)> = self
            .leftover_t5_arg_hits
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .collect();
        hits.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        if hits.is_empty() {
            return None;
        }
        Some(
            hits.into_iter()
                .take(4)
                .map(|(key, count)| format!("{key}:{count}"))
                .collect::<Vec<_>>()
                .join(","),
        )
    }

    fn resolve_technique_set_in<'a>(
        techsets: &'a [TechniqueSetFacts],
        key: TechsetKey<'_>,
    ) -> TechsetResolve<'a> {
        let want = AssetRef::bare_name(key.name);
        if want.is_empty() {
            return TechsetResolve::Missing;
        }
        let find_named = |name: &str| {
            techsets.iter().enumerate().find(|(_, facts)| {
                facts.namespace == key.namespace
                    && facts.name.is_real()
                    && facts.name.as_str() == name
            })
        };
        if let Some((index, facts)) = find_named(want) {
            if facts.table.is_some() {
                return TechsetResolve::Hit { index, facts };
            }
            let stripped = t5_feature_token_stripped(want);
            if stripped != want {
                if let Some((index, facts)) = find_named(&stripped) {
                    if facts.table.is_some() {
                        return TechsetResolve::Hit { index, facts };
                    }
                }
            }
            return TechsetResolve::GraphMissing { index };
        }
        let stripped = t5_feature_token_stripped(want);
        if stripped != want {
            if let Some((index, facts)) = find_named(&stripped) {
                if facts.table.is_some() {
                    return TechsetResolve::Hit { index, facts };
                }
                return TechsetResolve::GraphMissing { index };
            }
        }
        if let Some((index, facts)) = techsets.iter().enumerate().find(|(_, facts)| {
            facts.namespace != key.namespace
                && facts.name.is_real()
                && facts.name.as_str() == want
                && facts.table.is_some()
        }) {
            return TechsetResolve::Foreign {
                got: facts.namespace,
                index,
            };
        }
        TechsetResolve::Missing
    }

    fn ref_census<'a>(names: impl Iterator<Item = &'a AssetRef>) -> AssetRefCensus {
        let mut census = AssetRefCensus::default();
        for name in names {
            census.push(name);
        }
        census
    }

    pub fn image_memory(&self) -> MaterialImageMemory {
        let mut census = MaterialImageMemory {
            images: self.images.len(),
            ..MaterialImageMemory::default()
        };
        for image in &self.images {
            census.payload_bytes += image.payload.len();
            if let Some(decoded) = image.decoded.as_ref() {
                census.decoded_images += 1;
                census.decoded_bytes += decoded.data.as_ref().map_or(0, Vec::len);
            }
        }
        census
    }

    pub fn namespace_count(&self, ns: crate::AssetNamespace) -> usize {
        self.materials.iter().filter(|m| m.namespace == ns).count()
    }

    pub fn zone_of(&self, index: usize) -> crate::asset_graph::ZoneOwner {
        self.materials
            .get(index)
            .map(|material| material.zone)
            .unwrap_or_default()
    }

    pub fn material_index_by_key(&self, key: &crate::MaterialKey) -> Option<crate::MaterialIndex> {
        self.material_index_by_ns(key.namespace, &key.name)
    }

    pub fn material_index_by_ns(
        &self,
        namespace: crate::AssetNamespace,
        name: &str,
    ) -> Option<crate::MaterialIndex> {
        let want = AssetRef::bare_name(name);
        if want.is_empty() {
            return None;
        }
        self.materials
            .iter()
            .position(|m| m.namespace == namespace && m.name.is_real() && m.name.as_str() == want)
            .map(crate::MaterialIndex::from_order)
    }

    pub fn image_index_by_key(
        &self,
        namespace: crate::AssetNamespace,
        name: &str,
    ) -> Option<usize> {
        let want = AssetRef::bare_name(name);
        if want.is_empty() {
            return None;
        }
        self.images.iter().position(|image| {
            image.namespace == namespace && image.name.is_real() && image.name.as_str() == want
        })
    }

    pub fn material_ref_census(&self) -> AssetRefCensus {
        Self::ref_census(self.materials.iter().map(|m| &m.name))
    }

    pub fn image_ref_census(&self) -> AssetRefCensus {
        Self::ref_census(self.images.iter().map(|image| &image.name))
    }

    pub fn shader_ref_census(&self) -> AssetRefCensus {
        Self::ref_census(self.shaders.iter().map(|shader| &shader.name))
    }

    pub fn vertex_decl_ref_census(&self) -> AssetRefCensus {
        Self::ref_census(self.vertex_decls.iter().map(|decl| &decl.name))
    }

    pub fn vertex_decl_stream_census(&self) -> VertexDeclStreamCensus {
        let mut census = VertexDeclStreamCensus {
            n: self.vertex_decls.len(),
            ..VertexDeclStreamCensus::default()
        };
        for decl in &self.vertex_decls {
            if decl.stream_count == 0 {
                census.stream0 += 1;
            }
            if decl.name.as_str() == "ppcc0t0t0nn" {
                census.ppcc_n += 1;
                census.ppcc_stream_count = Some(decl.stream_count);
            }
        }
        census
    }

    pub fn state_bits_agreement<T: PartialEq>(
        &self,
        material: &AuthoredMaterial,
        decode: impl Fn([u32; 2]) -> T,
    ) -> asset_iw4::ColorPassAgreement<T> {
        asset_iw4::color_pass_agreement(
            material.state_bits_entry.as_ref(),
            &material.state_bits,
            material.route.map(|route| route.technique_slots),
            decode,
        )
    }

    pub fn agreed_draw_mode(&self, material: &AuthoredMaterial) -> Option<crate::MaterialDrawMode> {
        self.state_bits_agreement(material, crate::MaterialDrawMode::from_state_bits)
            .agreed()
    }

    pub fn agreed_alpha_test_cutoff(&self, material: &AuthoredMaterial) -> Option<Option<f32>> {
        self.state_bits_agreement(material, crate::alpha_test_cutoff_from_state_bits)
            .agreed()
    }

    pub fn agreed_draw_mode_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| self.agreed_draw_mode(material).is_some())
            .count()
    }

    pub fn lit_band_draw_mode_conflict_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| {
                let Some(route) = material.route else {
                    return false;
                };
                if !matches!(route.pass, asset_iw4::MaterialPass::Lit)
                    || route.takes_model_lighting()
                {
                    return false;
                }
                asset_iw4::lit_band_decode_conflicts(
                    material.state_bits_entry.as_ref(),
                    &material.state_bits,
                    route.technique_slots,
                    crate::MaterialDrawMode::from_state_bits,
                )
            })
            .count()
    }

    pub fn unresolved_draw_mode_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| self.agreed_draw_mode(material).is_none())
            .count()
    }

    pub fn is_sky(&self, material: &AuthoredMaterial) -> bool {
        matches!(
            material.route.map(|route| route.pass),
            Some(asset_iw4::MaterialPass::Sky(_))
        )
    }

    pub fn is_multiply(&self, material: &AuthoredMaterial) -> bool {
        self.agreed_draw_mode(material) == Some(crate::MaterialDrawMode::Multiply)
    }

    pub fn is_shadowcaster(&self, material: &AuthoredMaterial) -> bool {
        matches!(
            material.route.map(|route| route.pass),
            Some(asset_iw4::MaterialPass::ShadowOnly)
        )
    }

    pub fn takes_model_lighting(&self, material: &AuthoredMaterial) -> Option<bool> {
        Some(material.route?.takes_model_lighting())
    }

    pub fn is_unlit(&self, material: &AuthoredMaterial) -> Option<bool> {
        Some(matches!(
            material.route?.pass,
            asset_iw4::MaterialPass::Unlit | asset_iw4::MaterialPass::Sky(_)
        ))
    }

    pub fn technique_set_facts(&self) -> &[TechniqueSetFacts] {
        &self.techsets
    }

    pub fn resolve_technique_set(&self, key: TechsetKey<'_>) -> TechsetResolve<'_> {
        Self::resolve_technique_set_in(&self.techsets, key)
    }

    pub fn technique_set_edge_census(&self) -> crate::AssetEdgeCensus {
        let mut census = crate::AssetEdgeCensus::default();
        for material in &self.materials {
            census.push(material.technique_set_edge);
        }
        census
    }

    pub fn shader_source_census(&self) -> ShaderSourceCensus {
        ShaderSourceCensus {
            programs: self.shaders.len(),
            unresolved_aliases: self
                .shaders
                .iter()
                .filter(|shader| shader.name.is_reference())
                .count(),
            byteless: self
                .shaders
                .iter()
                .filter(|shader| shader.program.is_empty())
                .count(),
        }
    }

    pub fn unrouted_material_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| material.route.is_none())
            .count()
    }

    pub fn cull_face(&self, material: &AuthoredMaterial) -> Option<crate::MaterialCullFace> {
        self.state_bits_agreement(material, crate::cull_face_from_state_bits)
            .agreed()
    }

    pub fn constant(material: &AuthoredMaterial, name: &str) -> Option<[f32; 4]> {
        material
            .constants
            .iter()
            .find(|constant| crate::material_constant_name(&constant.name) == name)
            .map(|constant| constant.literal)
    }

    pub fn material_animation(material: &AuthoredMaterial) -> [[f32; 4]; 4] {
        let uv_anim = Self::constant(material, "uvAnimParms").unwrap_or([0.0; 4]);
        let (Some(parms), Some(begin), Some(end)) = (
            Self::constant(material, "falloffParms"),
            Self::constant(material, "falloffBegin"),
            Self::constant(material, "falloffEndCo"),
        ) else {
            return [uv_anim, [0.0; 4], [0.0; 4], [0.0; 4]];
        };
        [uv_anim, parms, [begin[0], begin[1], begin[2], 1.0], end]
    }

    pub fn color_map_transform(&self, material: &AuthoredMaterial) -> crate::ColorMapTransform {
        match self.agreed_draw_mode(material) {
            Some(
                crate::MaterialDrawMode::Blend
                | crate::MaterialDrawMode::Additive
                | crate::MaterialDrawMode::Screen,
            ) => crate::ColorMapTransform::Unknown,
            None => match material.route.map(|route| route.pass) {
                Some(
                    asset_iw4::MaterialPass::Lit
                    | asset_iw4::MaterialPass::Unlit
                    | asset_iw4::MaterialPass::Sky(_),
                ) => crate::ColorMapTransform::Square,
                Some(asset_iw4::MaterialPass::ShadowOnly) | None => {
                    crate::ColorMapTransform::Unknown
                }
            },
            Some(_) => crate::ColorMapTransform::Square,
        }
    }

    pub fn hud_image_name(&self, material: &AuthoredMaterial) -> Option<&str> {
        let named = |semantic: u8| {
            material
                .textures
                .iter()
                .find(|texture| texture.semantic == semantic && texture.image.is_some())
                .and_then(|texture| self.images.get(texture.image?))
                .map(|image| image.name.as_str())
                .filter(|name| !name.is_empty())
        };
        named(TS_2D).or_else(|| named(TS_COLOR_MAP))
    }

    pub fn color_binding_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| {
                material
                    .textures
                    .iter()
                    .any(|texture| texture.semantic == TS_COLOR_MAP && texture.image.is_some())
            })
            .count()
    }

    pub fn normal_binding_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| {
                material
                    .textures
                    .iter()
                    .any(|texture| texture.semantic == TS_NORMAL_MAP && texture.image.is_some())
            })
            .count()
    }

    pub fn alpha_test_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| {
                self.agreed_alpha_test_cutoff(material)
                    .is_some_and(|v| v.is_some())
            })
            .count()
    }

    pub fn blend_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| {
                matches!(
                    self.agreed_draw_mode(material),
                    Some(crate::MaterialDrawMode::Blend)
                )
            })
            .count()
    }

    pub fn multiply_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| self.is_multiply(material))
            .count()
    }

    pub fn sky_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| self.is_sky(material))
            .count()
    }
}
