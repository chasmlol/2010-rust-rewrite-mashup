# Solid recovery

Penetration queries provide outward unit normals and translation depths for
capsules against expanded brush plane hulls, finite mesh triangles, other capsules,
linked brush models and movement proxies. The plane hull is conservative near
corners. Capsule and triangle tangency is clear; closed brush hulls report touching.
Triangle degeneracy and invalid geometry explicitly refuse coverage.

The recovery core minimizes squared correction length subject to each contact's
outward translation inequality. It enumerates independent active sets of up to
three constraints, verifies every inequality and chooses the smallest feasible
correction in deterministic geometry order. Each correction triggers a fresh
query. This is a local solve over collider unions; it does not promise the nearest
free pose in a nonconvex scene.

IW4L policy permits at most eight corrections, nine contact queries and 32 local
contacts, with total displacement strictly below 8 units from the initial origin.
The independent separation margin is 0.02 units. Invalid, incomplete, overflowing,
infeasible, exhausted or unverified results leave the original position unchanged.
Every success requires a complete empty contact query and a separate stationary
trace; the adapter verifies the rounded f32 pose and its displacement budget again.

Ground support is a separate downward trace of 0.5 units from the verified pose.
Recovery never snaps the pose to that trace endpoint. Airborne movement recovery
is permitted; spawn acceptance independently requires a ground hit. A spawn point
starting inside collision first attempts bounded recovery at that point.

Adapters preserve masks, glass state, BSP brush reachability, mesh partitions and
vertex segments, current linked-model transforms and live/predicted player bodies.
Brush and mesh adapters retain their existing bounds-to-capsule conversion; player
body adapters use the temporary capsule query's bounds contract. Linked models
retain their local capsule orientation convention. These are integration contracts,
not core geometric assumptions. Geometry coverage excludes collider families that
movement's stationary trace does not use.

The geometric and optimization core was authored by a separate implementer using
only a source-free specification and its generated work. It derived capsule
support, convex closest features and minimum-norm halfspace projection from
elementary geometry and linear algebra. No old runtime, engine/reference source,
comparative audit or git history was supplied. Filesystem isolation was procedural.
The integrator inspected existing callers and collider traversal. Local origin,
fixtures and validation are retained under
`context/artifacts/2026-10-04-independent-reimplementations/8-SOLID-RECOVERY-PART`.
