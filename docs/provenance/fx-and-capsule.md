# Deterministic FX sampling and vertical capsule queries

FX sampling uses the Apache-2.0/MIT BLAKE3 component. IW4L defines its own
versioned little endian tuple encoding, domain labels and property identifiers.
Effect allocation identity and start time produce a 64-bit effect key; element
sequence and time produce an element key. Trail sequence has a separate domain.
Property and sample index select a stateless sample. Particle update order and
thread scheduling therefore do not advance a shared random generator.

Float samples use 24 bits in [0,1); integer samples use 16 bits. Direction
sampling uses uniform altitude and azimuth. Glass fracture has its own keyed
counter stream. The algorithm version is 1. Existing recordings reconstruct
visual effects with the current algorithm; older seed-only data does not
reproduce the previous particle pattern or fracture geometry. No claim of
cross-version visual or glass-geometry compatibility is made.

Temporary capsule queries use the distance from a moving point to a vertical
segment. Relative capsule motion adds the radii and central segment lengths.
Altitude crossings split the sweep into quadratic distance intervals. The
first contact sets the fraction; its segment distance vector sets the normal.
Initial strict overlap is startsolid. Endpoint strict overlap then implies
allsolid by convexity. Initial touching blocks only inward movement; an outside
tangent contact is a hit. There is no geometric skin inflation.

Bounds become a centered vertical capsule with radius equal to the smallest
half extent and central half length equal to vertical half extent minus radius.
For circular horizontal bounds this is the usual capsule. Unequal horizontal
bounds use an inscribed circular capsule. Invalid bounds return a clear trace.
Walkability uses the adapter's existing 0.7 normal-Z policy.

Both cores were authored by separate implementers receiving source-free
contracts and analytic fixtures. Their access restriction was procedural;
they shared the host filesystem. The integrator inspected existing callers
and wrote the adapters. This establishes the stated development boundary,
not a legal conclusion or a claim of technical source isolation.

The maintainer's local independent-reimplementations artifact retains the
contracts, derivations, disposable probes and validation evidence. Collision
brush traversal, solid recovery, recoil, sway and placement policies are
outside these two replacement boundaries.
