# Sentry placement geometry

IW4L places sentries using an independently authored octagonal support proxy.
Eight vertical queries supply a least squares support plane. Its intercept is
raised to the observed upper envelope, and yaw is projected onto that plane to
form an orthonormal frame. The tilted footprint is queried again before a pose
can pass. At most four refits and 40 support queries are allowed.

At least three loadable pads must support the vertical gravity projection of
the body midpoint strictly inside their convex hull. Loaded gaps are bounded;
observed steep surfaces constrain penetration without carrying load. Eight
frame-edge sweeps, one body sweep and an eye-to-body visibility query check
clearance. Unsupported or invalid geometry cannot supply load.

The adapter settles current model presence and excludes the actor and its own
carried turret proxy. Other alive players use standing movement capsules.
World and model sweeps use the existing axis-aligned hull collision backend:
a box enclosing each sphere is conservative and can reject extra placements.
Visibility uses current entity collision. Existing collision skin and numerical
precision remain backend behavior; these queries do not replace brush traversal.

The proxy uses reach 64, footprint radius 24, slope limit 25 degrees, vertical
support spans 48 above and 64 below, maximum loaded gap 6, clearance 1, frame
radius 2, and a body radius 12 with axial endpoints at heights 16 and 48.
These are independently selected IW4L policy, not recovered retail tuning or
a representation of every part of the visible model. Sampled supports cannot
establish contact across arbitrary terrain between pads.

A separate implementer received only a source-free geometry contract and
analytic fixtures. Filesystem isolation was procedural. The integrator read
existing callers and wrote the collision adapter. The local independent
reimplementations artifact retains the contract, derivation and disposable
probes. This records the development boundary without a legal conclusion or
retail parity claim. Turret targeting and firing are outside this replacement.
