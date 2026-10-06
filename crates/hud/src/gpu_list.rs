use std::hash::{Hash, Hasher};

use bevy::prelude::*;

use crate::draw2d::{DRAW2D_QUAD_INDICES, Draw2dQuad};
use crate::images::HudImages;
use crate::ui_write::adopt_display;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudTessVertex {
    pub xyzw: [f32; 4],
    pub color_bgra: [u8; 4],
    pub uv: [f32; 2],
    pub packed_normal: u32,
}

impl From<hud_iw4::GfxTessVertex2d> for HudTessVertex {
    fn from(v: hud_iw4::GfxTessVertex2d) -> Self {
        Self {
            xyzw: v.xyzw,
            color_bgra: v.color.to_le_bytes(),
            uv: v.texcoord,
            packed_normal: v.packed_normal,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HudTessTechnique {
    #[default]
    Modulate,

    SplatterAlt,

    SavedScreen,
}

#[derive(Clone, Debug)]
pub struct HudTessBatch {
    pub image: Handle<Image>,
    pub mask: Option<Handle<Image>>,
    pub technique: HudTessTechnique,

    pub state: Option<render_material::CompiledPassState>,
    pub first_index: u32,
    pub index_count: u32,

    pub first_vertex: u32,

    pub vertex_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellshockScreen {
    pub world_generation: Option<u64>,
    pub client: u32,
    pub view_client: u32,
    pub life_sequence: u64,
    pub authoritative_life: u32,
    pub time_ms: i32,
    pub blend_ms: i32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct HudTessGpuFrame {
    pub vertices: Vec<HudTessVertex>,
    pub indices: Vec<u16>,
    pub batches: Vec<HudTessBatch>,

    pub surface_w: f32,
    pub surface_h: f32,
    pub visible: bool,

    pub saved_screen_sequence: u64,
    pub shellshock_screen: Option<ShellshockScreen>,
}

impl HudTessGpuFrame {
    pub fn clear_geometry(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.batches.clear();
    }

    pub(crate) fn append_packed(&mut self, packed: &PackedList) {
        let vert_base = self.vertices.len() as u16;
        let index_base = self.indices.len() as u32;
        self.vertices.extend_from_slice(&packed.vertices);
        self.indices.extend_from_slice(&packed.indices);
        for batch in &packed.batches {
            self.batches.push(HudTessBatch {
                image: batch.image.clone(),
                mask: batch.mask.clone(),
                technique: batch.technique,
                state: batch.state,
                first_index: index_base.saturating_add(batch.first_index),
                index_count: batch.index_count,
                first_vertex: u32::from(vert_base).saturating_add(batch.first_vertex),
                vertex_count: batch.vertex_count,
            });
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct PackedList {
    vertices: Vec<HudTessVertex>,
    indices: Vec<u16>,
    batches: Vec<HudTessBatch>,
}

impl PackedList {
    pub(crate) fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

#[derive(Component, Default)]
pub struct GpuListLatch {
    packed: PackedList,
    last_fp: u64,
    last_shown: usize,
    hidden: bool,
}

impl GpuListLatch {
    pub fn mark_hidden(&mut self) {
        self.hidden = true;
    }
}

fn hash_one_quad(h: &mut impl Hasher, q: &Draw2dQuad) {
    for p in &q.xy {
        p[0].to_bits().hash(h);
        p[1].to_bits().hash(h);
    }
    for p in &q.st {
        p[0].to_bits().hash(h);
        p[1].to_bits().hash(h);
    }
    for c in q.color {
        c.to_bits().hash(h);
    }
    q.material.hash(h);
    q.material_namespace.hash(h);
    match q.clip {
        Some(c) => {
            1u8.hash(h);
            for v in c {
                v.to_bits().hash(h);
            }
        }
        None => 0u8.hash(h),
    }
}

fn fingerprint_quads<'a>(n: usize, quads: impl Iterator<Item = &'a Draw2dQuad>) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    n.hash(&mut h);
    for q in quads {
        hash_one_quad(&mut h, q);
    }
    h.finish()
}

pub fn register(app: &mut App) {
    app.init_resource::<HudTessPass>()
        .init_resource::<HudTessGpuFrame>();
}

#[derive(Default)]
pub enum TessJob {
    #[default]
    None,
    Hide,
    Quads(Vec<Draw2dQuad>),
}

#[derive(Resource, Default)]
pub struct HudTessPass {
    pub overhead_names: TessJob,
    pub compass: TessJob,
    pub scorebar: TessJob,
    pub splash: TessJob,
    pub killfeed: TessJob,
    pub playercard: TessJob,
    pub weaponbar: TessJob,
    pub scoreboard: TessJob,
    pub killcam_skip: TessJob,
    pub targetmap: TessJob,
    pub emp_static: TessJob,
    pub mantle_hint: TessJob,
    pub breath_hint: TessJob,
    pub use_hint: TessJob,
    pub hud_elems: TessJob,
    pub hud_elems_back: TessJob,
    pub script_menus: TessJob,
    /// The Minecraft map's hotbar and inventory.
    pub minecraft: TessJob,
}

/// What the HUD tess flush systems' own bodies cost this frame.
///
/// Taken inside the functions, not off the gap between two systems: a gap is
/// the executor's to fill — `.chain()` fixes the order of the HUD systems and
/// promises nothing about what runs between them — so it measures the schedule
/// rather than the HUD.
static BODY_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static JOBS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Charge the scope it is held in to the frame's HUD tess total.
#[must_use = "the body is timed until this is dropped"]
pub struct TessBody(std::time::Instant);

impl TessBody {
    pub fn open() -> Self {
        Self(std::time::Instant::now())
    }
}

impl Drop for TessBody {
    fn drop(&mut self) {
        BODY_NS.fetch_add(
            self.0.elapsed().as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}

/// The frame's total and job count, reset for the next frame.
pub fn take_tess_body_cost() -> (f32, u32) {
    let ns = BODY_NS.swap(0, std::sync::atomic::Ordering::Relaxed);
    let jobs = JOBS.swap(0, std::sync::atomic::Ordering::Relaxed);
    (ns as f32 / 1e6, jobs)
}

pub fn apply_tess_job(
    job: TessJob,
    host: &mut Node,
    latch: &mut GpuListLatch,
    hud_images: &mut HudImages,
    images: &mut Assets<Image>,
    frame: &mut HudTessGpuFrame,
    surface_w: f32,
    surface_h: f32,
) {
    JOBS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    match job {
        TessJob::None => {
            if !latch.hidden && !latch.packed.vertices.is_empty() {
                frame.append_packed(&latch.packed);
            }
        }
        TessJob::Hide => hide_list(host, latch),
        TessJob::Quads(quads) => {
            if quads.is_empty() {
                hide_list(host, latch);
            } else {
                present_list(PresentInput {
                    host,
                    latch,
                    quads: &quads,
                    hud_images,
                    images,
                    frame,
                    surface_w,
                    surface_h,
                });
            }
        }
    }
}

pub fn hide_host(node: &mut Node) {
    adopt_display(node, Display::None);
}

pub fn hide_list(host: &mut Node, latch: &mut GpuListLatch) {
    hide_host(host);
    latch.hidden = true;
    latch.packed = PackedList::default();
}

pub fn clip_aa_quad(quad: &Draw2dQuad) -> Option<Draw2dQuad> {
    let x0 = quad.xy[0][0];
    let y0 = quad.xy[0][1];
    let x1 = quad.xy[2][0];
    let y1 = quad.xy[2][1];
    let (cx0, cy0, cx1, cy1) = match quad.clip {
        Some(c) => (c[0], c[1], c[2], c[3]),
        None => return Some(quad.clone()),
    };
    let nx0 = x0.max(cx0);
    let ny0 = y0.max(cy0);
    let nx1 = x1.min(cx1);
    let ny1 = y1.min(cy1);
    if nx1 <= nx0 || ny1 <= ny0 {
        return None;
    }
    if (nx0 - x0).abs() <= f32::EPSILON
        && (ny0 - y0).abs() <= f32::EPSILON
        && (nx1 - x1).abs() <= f32::EPSILON
        && (ny1 - y1).abs() <= f32::EPSILON
    {
        return Some(quad.clone());
    }
    let dw = x1 - x0;
    let dh = y1 - y0;
    if dw.abs() <= f32::EPSILON || dh.abs() <= f32::EPSILON {
        return None;
    }
    let u0 = (nx0 - x0) / dw;
    let u1 = (nx1 - x0) / dw;
    let v0 = (ny0 - y0) / dh;
    let v1 = (ny1 - y0) / dh;
    let st = |u: f32, v: f32| -> [f32; 2] {
        let top = lerp2(quad.st[0], quad.st[1], u);
        let bot = lerp2(quad.st[3], quad.st[2], u);
        lerp2(top, bot, v)
    };
    let mut out = quad.clone();
    out.xy = [[nx0, ny0], [nx1, ny0], [nx1, ny1], [nx0, ny1]];
    out.st = [st(u0, v0), st(u1, v0), st(u1, v1), st(u0, v1)];
    Some(out)
}

fn lerp2(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

struct Prepared {
    texture: Handle<Image>,
    state: Option<render_material::CompiledPassState>,
    quad: Draw2dQuad,
}

pub struct PresentInput<'a> {
    pub host: &'a mut Node,
    pub latch: &'a mut GpuListLatch,
    pub quads: &'a [Draw2dQuad],
    pub hud_images: &'a mut HudImages,
    pub images: &'a mut Assets<Image>,
    pub frame: &'a mut HudTessGpuFrame,
    pub surface_w: f32,
    pub surface_h: f32,
}

pub fn present_list(input: PresentInput<'_>) -> usize {
    let PresentInput {
        host,
        latch,
        quads,
        hud_images,
        images,
        frame,
        surface_w,
        surface_h,
    } = input;
    if surface_w <= 1.0 || surface_h <= 1.0 {
        hide_list(host, latch);
        return 0;
    }
    let mut prepared = Vec::with_capacity(quads.len());
    for quad in quads {
        let Some(clipped) = clip_aa_quad(quad) else {
            continue;
        };
        let Some(texture) =
            hud_images.get_native(clipped.material_namespace, &clipped.material, images)
        else {
            continue;
        };
        prepared.push(Prepared {
            texture,
            state: hud_images.material_state(clipped.material_namespace, &clipped.material),
            quad: clipped,
        });
    }
    let shown = prepared.len();
    let fp = fingerprint_quads(shown, prepared.iter().map(|p| &p.quad));
    if !latch.hidden
        && fp == latch.last_fp
        && latch.last_shown == shown
        && !latch.packed.vertices.is_empty()
    {
        frame.append_packed(&latch.packed);
        return shown;
    }

    let packed = pack_material_runs(&prepared);
    if shown == 0 {
        hide_list(host, latch);
        return 0;
    }
    hide_host(host);
    latch.hidden = false;
    latch.last_fp = fp;
    latch.last_shown = shown;
    latch.packed = packed;
    frame.append_packed(&latch.packed);
    shown
}

fn pack_quad_verts(quad: &Draw2dQuad) -> [HudTessVertex; 4] {
    let color = u32::from_le_bytes(hud_iw4::convert_color_to_bytes(quad.color));
    core::array::from_fn(|n| {
        HudTessVertex::from(hud_iw4::set_vertex_2d(
            quad.xy[n][0],
            quad.xy[n][1],
            quad.st[n][0],
            quad.st[n][1],
            color,
        ))
    })
}

fn pack_material_runs(prepared: &[Prepared]) -> PackedList {
    let mut packed = PackedList::default();
    let mut i = 0;
    while i < prepared.len() {
        let id = prepared[i].texture.id();
        let mut batch_verts = 0u32;
        let mut batch_idx = 0u32;
        let first_index = packed.indices.len() as u32;
        let vert0 = packed.vertices.len() as u16;
        let mut k = 0usize;
        let mut j = i;
        while j < prepared.len()
            && prepared[j].texture.id() == id
            && prepared[j].state == prepared[i].state
            && prepared[j].quad.material == prepared[i].quad.material
            && prepared[j].quad.material_namespace == prepared[i].quad.material_namespace
        {
            if hud_iw4::tess_stretchpic_must_flush(batch_verts, batch_idx) {
                break;
            }
            let verts = pack_quad_verts(&prepared[j].quad);
            let local_base = (k * 4) as u16;
            packed.vertices.extend(verts);
            for t in DRAW2D_QUAD_INDICES {
                packed.indices.push(local_base.saturating_add(t));
            }
            batch_verts = batch_verts.saturating_add(4);
            batch_idx = batch_idx.saturating_add(6);
            k = k.saturating_add(1);
            j += 1;
        }
        packed.batches.push(HudTessBatch {
            image: prepared[i].texture.clone(),
            mask: None,
            technique: HudTessTechnique::Modulate,
            state: prepared[i].state,
            first_index,
            index_count: batch_idx,
            first_vertex: u32::from(vert0),
            vertex_count: batch_verts,
        });
        i = j;
    }
    packed
}

pub(crate) fn pack_splatter_alt(
    quad: &Draw2dQuad,
    color: Handle<Image>,
    mask: Handle<Image>,
    state: render_material::CompiledPassState,
) -> PackedList {
    let mut packed = PackedList::default();
    packed.vertices.extend(pack_quad_verts(quad));
    packed.indices.extend_from_slice(&DRAW2D_QUAD_INDICES);
    packed.batches.push(HudTessBatch {
        image: color,
        mask: Some(mask),
        technique: HudTessTechnique::SplatterAlt,
        state: Some(state),
        first_index: 0,
        index_count: 6,
        first_vertex: 0,
        vertex_count: 4,
    });
    packed
}

pub(crate) fn pack_saved_screen(quad: &Draw2dQuad, stand_in: Handle<Image>) -> PackedList {
    let mut packed = pack_modulate(quad, stand_in);
    for batch in &mut packed.batches {
        batch.technique = HudTessTechnique::SavedScreen;
    }
    packed
}

pub(crate) fn pack_modulate(quad: &Draw2dQuad, image: Handle<Image>) -> PackedList {
    let mut packed = PackedList::default();
    packed.vertices.extend(pack_quad_verts(quad));
    packed.indices.extend_from_slice(&DRAW2D_QUAD_INDICES);
    packed.batches.push(HudTessBatch {
        image,
        mask: None,
        technique: HudTessTechnique::Modulate,
        state: None,
        first_index: 0,
        index_count: 6,
        first_vertex: 0,
        vertex_count: 4,
    });
    packed
}
