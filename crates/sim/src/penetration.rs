use movement_iw4::GroundTraceInput;
use movement_iw4::penetration::{
    Capsule, Contact, ContactResult, Plane, Triangle, Vec3, capsule_plane_hull, capsule_triangle,
};
use movement_iw4::recovery::{ContactBuffer, Coverage};

use crate::bullet_collision::LinkedBrushCollisionBrush;
use crate::world::{SimBrush, SimClipBsp, SimClipMesh};

pub(crate) struct RecoveryScene<'a> {
    pub brushes: &'a [SimBrush],
    pub bsp: &'a SimClipBsp,
    pub mesh: &'a SimClipMesh,
    pub cmodels: &'a [clipmap_iw4::ClipCmodel],
    pub linked: &'a [LinkedBrushCollisionBrush],
    pub models: &'a [SimBrush],
    pub glass_is_solid: &'a dyn Fn(u16) -> bool,
}

fn vec3(p: [f64; 3]) -> Vec3 {
    Vec3 {
        x: p[0],
        y: p[1],
        z: p[2],
    }
}
fn array(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

pub(crate) fn capsule(input: GroundTraceInput, temporary: bool) -> Option<Capsule> {
    if !input
        .start
        .iter()
        .chain(input.mins.iter())
        .chain(input.maxs.iter())
        .all(|x| x.is_finite())
        || (0..3).any(|i| input.mins[i] > input.maxs[i])
    {
        return None;
    }
    let (center, radius, segment): ([f64; 3], f64, f64) = if temporary {
        let center: [f64; 3] =
            core::array::from_fn(|i| (f64::from(input.mins[i]) + f64::from(input.maxs[i])) * 0.5);
        let half: [f64; 3] =
            core::array::from_fn(|i| (f64::from(input.maxs[i]) - f64::from(input.mins[i])) * 0.5);
        let radius = half[0].min(half[1]).min(half[2]);
        (
            core::array::from_fn(|i| f64::from(input.start[i]) + center[i]),
            radius,
            half[2] - radius,
        )
    } else {
        let offset: [f32; 3] = core::array::from_fn(|i| (input.mins[i] + input.maxs[i]) * 0.5);
        let half: [f32; 3] = core::array::from_fn(|i| input.maxs[i] - offset[i]);
        let radius = half[0].min(half[2]);
        (
            core::array::from_fn(|i| f64::from(input.start[i] + offset[i])),
            f64::from(radius),
            f64::from(half[2] - radius),
        )
    };
    let capsule = Capsule {
        a: vec3([center[0], center[1], center[2] - segment]),
        b: vec3([center[0], center[1], center[2] + segment]),
        radius,
    };
    capsule.valid().then_some(capsule)
}

pub(crate) fn append(result: ContactResult, out: &mut ContactBuffer) -> Result<(), Coverage> {
    match result {
        ContactResult::Clear => Ok(()),
        ContactResult::Contact(contact) => {
            if out.push(contact) {
                Ok(())
            } else {
                Err(Coverage::Overflow)
            }
        }
        ContactResult::Invalid | ContactResult::Unsupported => Err(Coverage::Unsupported),
    }
}

fn brush_contact(cap: Capsule, brush: &SimBrush) -> ContactResult {
    capsule_plane_hull(
        cap,
        brush.planes.iter().map(|p| Plane {
            normal: vec3([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]),
            d: f64::from(p[3]),
        }),
    )
}

pub(crate) fn contacts(
    scene: &RecoveryScene<'_>,
    input: GroundTraceInput,
    out: &mut ContactBuffer,
) -> Coverage {
    match gather(scene, input, out) {
        Ok(()) => Coverage::Complete,
        Err(status) => status,
    }
}

fn gather(
    scene: &RecoveryScene<'_>,
    input: GroundTraceInput,
    out: &mut ContactBuffer,
) -> Result<(), Coverage> {
    let cap = capsule(input, false).ok_or(Coverage::Unsupported)?;
    let map = clipmap_iw4::ClipMapRef {
        nodes: &scene.bsp.nodes,
        leaves: &scene.bsp.leaves,
        leafbrushes: &scene.bsp.leafbrushes,
        brushes: scene.brushes,
    };
    let ext = clipmap_iw4::TraceExtents::new(
        input.start,
        input.start,
        input.mins,
        input.maxs,
        input.tracemask,
    );
    let linear = map.nodes.is_empty() || map.leaves.is_empty();
    let hits = if linear {
        None
    } else {
        Some(clipmap_iw4::position_leaf_hits(&map, &ext).ok_or(Coverage::Unsupported)?)
    };
    let brush_allowed = |brush: &SimBrush| {
        brush.contents & input.tracemask != 0
            && (brush.glass_encoded == 0 || (scene.glass_is_solid)(brush.glass_encoded - 1))
    };
    if let Some(hits) = &hits {
        for &id in &hits.brush_ids {
            let brush = scene
                .brushes
                .get(usize::from(id))
                .ok_or(Coverage::Unsupported)?;
            if brush_allowed(brush) {
                append(brush_contact(cap, brush), out)?;
            }
        }
    } else {
        for brush in scene.brushes {
            if brush_allowed(brush) {
                append(brush_contact(cap, brush), out)?;
            }
        }
    }
    let (roots, _) = if linear {
        (scene.mesh.tables.aabb_roots.as_slice(), true)
    } else {
        clipmap_iw4::mesh_aabb_roots_for_trace(
            hits.as_ref(),
            &scene.mesh.tables.aabb_roots,
            &scene.bsp.leaves,
        )
    };
    mesh_contacts(&scene.mesh.tables, roots, cap, input.tracemask, out)?;
    for linked in scene.linked {
        let model = scene
            .cmodels
            .get(linked.cmodel_handle as usize)
            .ok_or(Coverage::Unsupported)?;
        let transform = clipmap_iw4::CmodelTransform::new(linked.origin, linked.angles)
            .ok_or(Coverage::Unsupported)?;
        let start = transform.local_point(input.start);
        let local_cap = capsule(
            GroundTraceInput {
                start,
                end: start,
                ..input
            },
            false,
        )
        .ok_or(Coverage::Unsupported)?;
        let first = model.first_brush as usize;
        let last = first
            .checked_add(usize::from(model.num_brushes))
            .ok_or(Coverage::Unsupported)?;
        let ids = scene
            .bsp
            .leafbrushes
            .get(first..last)
            .ok_or(Coverage::Unsupported)?;
        for &id in ids {
            let brush = scene
                .brushes
                .get(usize::from(id))
                .ok_or(Coverage::Unsupported)?;
            if brush.contents & input.tracemask == 0 {
                continue;
            }
            let result = match brush_contact(local_cap, brush) {
                ContactResult::Contact(contact) => {
                    let world = transform.world_direction(array(contact.normal));
                    let length = world.iter().map(|x| x * x).sum::<f64>().sqrt();
                    if !length.is_finite() || length <= 0.0 {
                        return Err(Coverage::Unsupported);
                    }
                    ContactResult::Contact(Contact {
                        normal: vec3(world.map(|x| x / length)),
                        depth: contact.depth / length,
                    })
                }
                other => other,
            };
            append(result, out)?;
        }
    }
    for brush in scene.models {
        if brush.contents & input.tracemask != 0 {
            append(brush_contact(cap, brush), out)?;
        }
    }
    Ok(())
}

fn mesh_contacts(
    mesh: &clipmap_iw4::ClipMeshTables,
    roots: &[u16],
    cap: Capsule,
    mask: u32,
    out: &mut ContactBuffer,
) -> Result<(), Coverage> {
    let a = array(cap.a);
    let b = array(cap.b);
    let lo: [f64; 3] = core::array::from_fn(|i| a[i].min(b[i]) - cap.radius);
    let hi: [f64; 3] = core::array::from_fn(|i| a[i].max(b[i]) + cap.radius);
    let mut triangle = |ti: usize, base: usize| -> Result<(), Coverage> {
        let contents = if mesh.tri_content_flags.is_empty() {
            1
        } else {
            *mesh
                .tri_content_flags
                .get(ti)
                .ok_or(Coverage::Unsupported)?
        };
        if mask & contents == 0 {
            return Ok(());
        }
        let first = ti.checked_mul(3).ok_or(Coverage::Unsupported)?;
        let ids = mesh
            .tri_indices
            .get(first..first.checked_add(3).ok_or(Coverage::Unsupported)?)
            .ok_or(Coverage::Unsupported)?;
        let mut verts = [[0.0; 3]; 3];
        for (i, &id) in ids.iter().enumerate() {
            verts[i] = mesh
                .verts
                .get(base + usize::from(id))
                .ok_or(Coverage::Unsupported)?
                .map(f64::from);
            if !verts[i].iter().all(|x| x.is_finite()) {
                return Err(Coverage::Unsupported);
            }
        }
        if (0..3).any(|i| {
            verts.iter().map(|v| v[i]).fold(f64::INFINITY, f64::min) > hi[i]
                || verts.iter().map(|v| v[i]).fold(f64::NEG_INFINITY, f64::max) < lo[i]
        }) {
            return Ok(());
        }
        append(
            capsule_triangle(
                cap,
                Triangle {
                    a: vec3(verts[0]),
                    b: vec3(verts[1]),
                    c: vec3(verts[2]),
                },
            ),
            out,
        )
    };
    if mesh.aabb_trees.is_empty() {
        if !mesh.tri_indices.len().is_multiple_of(3) {
            return Err(Coverage::Unsupported);
        }
        for ti in 0..mesh.tri_count() {
            triangle(ti, 0)?;
        }
        return Ok(());
    }
    let mut stack: Vec<usize> = roots.iter().map(|&id| usize::from(id)).collect();
    let mut seen = vec![false; mesh.aabb_trees.len()];
    while let Some(id) = stack.pop() {
        let node = mesh.aabb_trees.get(id).ok_or(Coverage::Unsupported)?;
        if seen[id] {
            return Err(Coverage::Unsupported);
        }
        seen[id] = true;
        if !(0..3).all(|i| {
            node.origin[i].is_finite() && node.half_size[i].is_finite() && node.half_size[i] >= 0.0
        }) {
            return Err(Coverage::Unsupported);
        }
        if (0..3).any(|i| {
            f64::from(node.origin[i] - node.half_size[i]) > hi[i]
                || f64::from(node.origin[i] + node.half_size[i]) < lo[i]
        }) {
            continue;
        }
        let index = usize::try_from(node.u).map_err(|_| Coverage::Unsupported)?;
        if node.child_count != 0 {
            let end = index
                .checked_add(usize::from(node.child_count))
                .ok_or(Coverage::Unsupported)?;
            if end > mesh.aabb_trees.len() {
                return Err(Coverage::Unsupported);
            }
            stack.extend(index..end);
        } else {
            let part = mesh.partitions.get(index).ok_or(Coverage::Unsupported)?;
            let first = usize::try_from(part.first_tri).map_err(|_| Coverage::Unsupported)?;
            let base = usize::from(part.first_vert_segment) * clipmap_iw4::VERTS_PER_SEGMENT;
            for ti in first..first + usize::from(part.tri_count) {
                triangle(ti, base)?;
            }
        }
    }
    Ok(())
}
