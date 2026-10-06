# Movement implementation provenance

Recorded 2026-10-04; replacement scope: `movement_iw4/src/slide.rs` and
its collision-response helpers, including step traversal and ground projection.

## Reason

The previous module had uncertain provenance because of reported similarities
to GPL movement implementations. Its first tracked appearance is commit
`0f387098e7ff811323b8ff532e676d8979cfb506`; the pre-replacement blob is
`177d120ff19d065de7f01b535163d14b036181c8`. This record does not establish
that copying or adaptation occurred. Existing Git history was preserved.

## Source boundary

The new solver was written by the isolated agent `independent_solver`, with
conversation history excluded. Its supplied inputs were a newly written
behavioral contract, Rust API declarations and independent geometry cases.
It derived contact response as Euclidean projection onto a convex cone of
allowed velocities. Step routes are independently swept and compared by
horizontal progress; invalid or unsupported elevated routes are rejected.

The implementer was instructed not to read KisakCOD, Quake implementations,
the previous module, repository history or source comparisons. It worked in a
separate scratch directory. This was procedural isolation on a shared host,
not an enforced filesystem sandbox or a guarantee about model training data.
The integrating agent had read the old module and authored the thin adapter
between game state and the new solver; the source-exclusion claim applies to
the solver implementer, not to the integrating agent.

## Evidence and limits

The maintainer's local artifact `2026-10-04-independent-reimplementations` retains the
contract, exact agent prompt, origin statement, checks and integration logs.
These records live under ignored `context/artifacts/`, per `CONTEXT.md`.
The public replacement commit can be identified from this file's Git history;
its full hash and the solver digest are retained in that artifact.

Step limits (18 standing, 10 prone), supported downward snap (9), and landing
policy (walkable or upward normal with z >= 0.3) are current project settings.
They have not been independently measured against retail IW4 in this task.
Analytic fixtures validate geometry, bounded termination and determinism;
they do not establish bitwise or gameplay parity with retail movement.
This replacement does not resolve historical snapshots or audit other modules.
