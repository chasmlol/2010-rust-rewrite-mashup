use std::collections::BTreeSet;

use asset_iw4::size as sz;
use asset_material::MaterialDefinitions;
use fastfile_iw4::{Ptr, ZonePtr, ZoneStream};
use fx_iw4::{
    FX_GLASS_DEF, FX_GLASS_GEOMETRY_DATA, FX_GLASS_INIT_PIECE_STATE, FX_GLASS_PIECE_PLACE,
    FX_GLASS_PIECE_STATE, glass_place_origin, glass_place_quat, glass_reset_copy_geo,
    glass_reset_copy_piece, glass_state_geo_start, unit_quat_to_axis,
};
use weapon_iw4::CONTENTS_GLASS;

use crate::ClipCollision;

const SURF_TYPE_GLASS: u32 = 9;

const INIT_PIECE_ORIGIN: usize = 0x10;

const DEF_MATERIAL: usize = 24;
const DEF_MATERIAL_SHATTERED: usize = 28;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlassZoneCensus {
    pub fx_recorded: bool,
    pub fx_def_n: usize,
    pub fx_piece_limit: usize,
    pub fx_init_piece_n: usize,
    pub fx_init_geo_n: usize,
    pub fx_origins: Vec<[f32; 3]>,

    pub fx_def_materials: Vec<(String, String)>,
    pub g_recorded: bool,
    pub g_piece_n: usize,
    pub g_name_n: usize,
    pub g_names: Vec<String>,
    pub clip_brush_n: usize,
    pub clip_surf_glass_n: usize,
    pub clip_encoded_n: usize,
    pub clip_contents_glass_n: usize,
    pub clip_encoded_unique: usize,
    pub clip_encoded_max: u16,
    pub clip_surf_glass_and_encoded: usize,
    pub clip_surf_glass_not_encoded: usize,
    pub clip_encoded_not_surf_glass: usize,
}

impl GlassZoneCensus {
    pub fn report_line(&self) -> String {
        format!(
            "glass zone: fx init_pieces={} defs={} init_geo={} | glass data pieces={} names={} | clip encoded={} unique={} surf9={} contents_glass={} (not tess, not hitType=4)",
            self.fx_init_piece_n,
            self.fx_def_n,
            self.fx_init_geo_n,
            self.g_piece_n,
            self.g_name_n,
            self.clip_encoded_n,
            self.clip_encoded_unique,
            self.clip_surf_glass_n,
            self.clip_contents_glass_n
        )
    }
}

pub fn build_glass_census(
    stream: &ZoneStream<'_>,
    clip: Option<&ClipCollision>,
) -> GlassZoneCensus {
    let mut out = GlassZoneCensus::default();
    fill_fx(&mut out, stream);
    fill_g_glass(&mut out, stream);
    if let Some(clip) = clip {
        fill_clip(&mut out, clip);
    }
    out
}

fn fill_fx(out: &mut GlassZoneCensus, s: &ZoneStream<'_>) {
    let Some(g) = s.fx_world() else {
        return;
    };
    out.fx_recorded = true;
    out.fx_def_n = g.def_count;
    out.fx_piece_limit = g.piece_limit;
    out.fx_init_piece_n = g.init_piece_count;
    out.fx_init_geo_n = g.init_geo_count;
    if let Some(states) = g.init_piece_states {
        out.fx_origins.reserve(g.init_piece_count);
        for i in 0..g.init_piece_count {
            let p = states.at(i * sz::FX_GLASS_INIT_PIECE_STATE);
            match (
                s.f32_at(p, INIT_PIECE_ORIGIN),
                s.f32_at(p, INIT_PIECE_ORIGIN + 4),
                s.f32_at(p, INIT_PIECE_ORIGIN + 8),
            ) {
                (Ok(x), Ok(y), Ok(z)) => out.fx_origins.push([x, y, z]),
                _ => {}
            }
        }
    }
    if let Some(defs) = g.defs {
        out.fx_def_materials.reserve(g.def_count);
        for i in 0..g.def_count {
            let d = defs.at(i * s.layout(sz::FX_GLASS_DEF, 48));
            out.fx_def_materials.push((
                material_name_at(s, d.at(DEF_MATERIAL)).unwrap_or_default(),
                material_name_at(s, d.at(s.layout(DEF_MATERIAL_SHATTERED, 32))).unwrap_or_default(),
            ));
        }
    }
}

fn fill_g_glass(out: &mut GlassZoneCensus, s: &ZoneStream<'_>) {
    let Some(g) = s.glass_data() else {
        return;
    };
    out.g_recorded = true;
    out.g_piece_n = g.piece_count;
    out.g_name_n = g.name_count;
    let Some(names) = g.names else {
        return;
    };
    out.g_names.reserve(g.name_count);
    for i in 0..g.name_count {
        let row = names.at(i * s.layout(sz::G_GLASS_NAME, 24));
        out.g_names.push(cstr_field(s, row, 0).unwrap_or_default());
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FxGlassReset {
    pub init_piece_states: Vec<[u8; FX_GLASS_INIT_PIECE_STATE]>,
    pub init_geo_data: Vec<[u8; FX_GLASS_GEOMETRY_DATA]>,
    pub piece_places: Vec<[u8; FX_GLASS_PIECE_PLACE]>,
    pub piece_states: Vec<[u8; FX_GLASS_PIECE_STATE]>,
    pub geo_data: Vec<[u8; FX_GLASS_GEOMETRY_DATA]>,

    pub defs: Vec<[u8; FX_GLASS_DEF]>,

    pub namespace: asset_material::AssetNamespace,
    pub def_materials: Vec<(String, String)>,

    pub def_material_edges: Vec<(
        crate::AssetEdge<crate::MaterialSpace>,
        crate::AssetEdge<crate::MaterialSpace>,
    )>,

    pub lighting_handles: Vec<u16>,
    pub half_thickness: Vec<Option<f32>>,

    pub geo_cursor: u16,

    pub piece_limit: usize,

    pub geo_data_limit: usize,
}

impl FxGlassReset {
    pub fn report_line(&self) -> String {
        format!(
            "fx glass reset: pieces={} geo={} cursor={} defs={} (reset copies the pieces; not tess, not hitType=4)",
            self.piece_places.len(),
            self.geo_data.len(),
            self.geo_cursor,
            self.defs.len()
        )
    }

    pub fn pane_basis(&self, piece: usize) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
        let place = self.piece_places.get(piece)?;
        let origin = glass_place_origin(place);
        let axis = unit_quat_to_axis(glass_place_quat(place));
        Some((origin, axis[0], axis[1]))
    }

    pub fn resolve_material_edges(&mut self, materials: &MaterialDefinitions) {
        let ns = self.namespace;
        self.def_material_edges = self
            .def_materials
            .iter()
            .map(|(intact, shattered)| {
                (
                    glass_material_edge(ns, intact, materials),
                    glass_material_edge(ns, shattered, materials),
                )
            })
            .collect();
    }

    pub fn material_edge(
        &self,
        def_index: usize,
        shattered: bool,
    ) -> Option<crate::AssetEdge<crate::MaterialSpace>> {
        self.def_material_edges
            .get(def_index)
            .map(|(intact, broken)| if shattered { *broken } else { *intact })
    }

    pub fn material_edge_census(&self) -> crate::AssetEdgeCensus {
        let mut census = crate::AssetEdgeCensus::default();
        for (intact, shattered) in &self.def_material_edges {
            census.push(*intact);
            census.push(*shattered);
        }
        census
    }

    pub fn geo_start(&self, piece: usize) -> Option<u16> {
        self.piece_states.get(piece).map(glass_state_geo_start)
    }
}

pub fn build_fx_glass_reset(stream: &ZoneStream<'_>) -> Option<FxGlassReset> {
    let g = stream.fx_world()?;
    let init_piece_states =
        copy_rows::<FX_GLASS_INIT_PIECE_STATE>(stream, g.init_piece_states, g.init_piece_count)?;
    let init_geo_data =
        copy_rows::<FX_GLASS_GEOMETRY_DATA>(stream, g.init_geo_data, g.init_geo_count)?;

    let defs = copy_rows_with_stride::<FX_GLASS_DEF>(
        stream,
        g.defs,
        g.def_count,
        stream.layout(FX_GLASS_DEF, 48),
    )?;

    let mut piece_places = Vec::with_capacity(init_piece_states.len());
    let mut piece_states = Vec::with_capacity(init_piece_states.len());
    let mut half_thickness = Vec::with_capacity(init_piece_states.len());
    let mut geo_cursor = 0u16;
    for (i, init) in init_piece_states.iter().enumerate() {
        let piece = u16::try_from(i).ok()?;
        let out = glass_reset_copy_piece(init, piece, geo_cursor, &defs);
        geo_cursor = out.next_geo_start;
        piece_places.push(out.place);
        piece_states.push(out.state);
        half_thickness.push(out.half_thickness);
    }

    let mut geo_flat = vec![0u8; init_geo_data.len() * FX_GLASS_GEOMETRY_DATA];
    let src: Vec<u8> = init_geo_data.iter().flatten().copied().collect();
    glass_reset_copy_geo(&mut geo_flat, &src);
    let mut geo_data = Vec::with_capacity(init_geo_data.len());
    for chunk in geo_flat.chunks_exact(FX_GLASS_GEOMETRY_DATA) {
        let mut word = [0u8; FX_GLASS_GEOMETRY_DATA];
        word.copy_from_slice(chunk);
        geo_data.push(word);
    }

    Some(FxGlassReset {
        init_piece_states,
        init_geo_data,
        piece_places,
        piece_states,
        geo_data,
        defs,
        namespace: asset_material::AssetNamespace::Iw4,
        def_materials: Vec::new(),
        def_material_edges: Vec::new(),
        half_thickness,
        lighting_handles: copy_u16s(stream, g.init_piece_indices, g.init_piece_count)
            .unwrap_or_default(),
        geo_cursor,
        piece_limit: g.piece_limit,
        geo_data_limit: g.geo_data_limit,
    })
}

fn glass_material_edge(
    namespace: asset_material::AssetNamespace,
    hint: &str,
    materials: &MaterialDefinitions,
) -> crate::AssetEdge<crate::MaterialSpace> {
    if hint.is_empty() {
        return crate::AssetEdge::Absent;
    }
    match materials.material_index_by_ns(namespace, hint) {
        Some(index) => crate::AssetEdge::bind(index, materials.zone_of(index.order())),
        None => crate::AssetEdge::Unresolved(crate::AssetEdgeReason::CatalogMiss),
    }
}

fn copy_u16s(stream: &ZoneStream<'_>, base: Option<Ptr>, count: usize) -> Option<Vec<u16>> {
    if count == 0 {
        return Some(Vec::new());
    }
    let base = base?;
    let bytes = stream.slice_at(base, 0, 2 * count).ok()?;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let lo = bytes[i * 2];
        let hi = bytes[i * 2 + 1];
        out.push(u16::from_le_bytes([lo, hi]));
    }
    Some(out)
}

fn copy_rows<const N: usize>(
    stream: &ZoneStream<'_>,
    base: Option<Ptr>,
    count: usize,
) -> Option<Vec<[u8; N]>> {
    copy_rows_with_stride(stream, base, count, N)
}

fn copy_rows_with_stride<const N: usize>(
    stream: &ZoneStream<'_>,
    base: Option<Ptr>,
    count: usize,
    stride: usize,
) -> Option<Vec<[u8; N]>> {
    if count == 0 {
        return Some(Vec::new());
    }
    let base = base?;
    let bytes = stream.slice_at(base, 0, stride * count).ok()?;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let mut row = [0u8; N];
        row.copy_from_slice(&bytes[i * stride..i * stride + N]);
        out.push(row);
    }
    Some(out)
}

fn fill_clip(out: &mut GlassZoneCensus, clip: &ClipCollision) {
    out.clip_brush_n = clip.brushes.len();
    let mut unique = BTreeSet::new();
    for b in &clip.brushes {
        let surf_glass = b
            .plane_surface_flags
            .iter()
            .any(|f| (f >> 20) & 0x1f == SURF_TYPE_GLASS);
        let encoded = b.glass_encoded != 0;
        let contents_glass = b.contents & CONTENTS_GLASS != 0;
        if surf_glass {
            out.clip_surf_glass_n += 1;
        }
        if encoded {
            out.clip_encoded_n += 1;
            unique.insert(b.glass_encoded);
            out.clip_encoded_max = out.clip_encoded_max.max(b.glass_encoded);
        }
        if contents_glass {
            out.clip_contents_glass_n += 1;
        }
        if surf_glass && encoded {
            out.clip_surf_glass_and_encoded += 1;
        }
        if surf_glass && !encoded {
            out.clip_surf_glass_not_encoded += 1;
        }
        if encoded && !surf_glass {
            out.clip_encoded_not_surf_glass += 1;
        }
    }
    out.clip_encoded_unique = unique.len();
}

fn material_name_at(s: &ZoneStream<'_>, slot: Ptr) -> Option<String> {
    let body = match s.ptr_at(slot, 0).ok()? {
        ZonePtr::Offset(p) => s.resolve_alias(p),
        ZonePtr::Null => return None,
        _ => return None,
    };
    cstr_field(s, body, 0)
}

fn cstr_field(s: &ZoneStream<'_>, parent: Ptr, field: usize) -> Option<String> {
    match s.ptr_at(parent, field).ok()? {
        ZonePtr::Offset(name) => s
            .cstr(s.resolve_alias(name))
            .ok()
            .filter(|n| !n.is_empty())
            .map(str::to_owned),
        _ => None,
    }
}
