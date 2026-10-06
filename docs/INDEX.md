# `docs/` — the one-page map

Short files (each ≤50 lines) on what lives where and how to poke the live
game. Keep them this short: nobody opens a long file twice.

| file | about | when to read |
|---|---|---|
| [`SKATE.md`](SKATE.md) | Skate 3 mode: what you need, where `default.xex` comes from, setup, controls, how it works, building a release | playing or changing the skate mode |
| [`PERFORMANCE.md`](PERFORMANCE.md) | the frame-time work in this fork: before/after numbers and every change | "why is it faster", profiling |
| [`IW4L.md`](IW4L.md) | upstream IW4L's own README | what IW4L is |
| [`BUILD.md`](BUILD.md) | system packages per distro (Fedora / Debian / Arch), macOS, what the Windows cross build needs | before your first build |
| [`RUN.md`](RUN.md) | running (`make map`, `--cmds`), controls frozen until `Playing`, `force_match_start`, sync-by-default, the verb list and the traps | before your first live run |
| [`WINDOWS.md`](WINDOWS.md) | portable `iw4l.exe`: community updates, shortcuts into CoD, writable `iw4l-artifacts/` | building and running on Windows |
| [`DEPLOY.md`](DEPLOY.md) | `make release` / `publish` / `deploy`: the play profile, hashed `.zst`, master by SHA, provision kept separate | shipping a release, "why is the player on an old version" |
| [Approved scenarios](../crates/approved_tests/README.md) | gameplay lifecycle and two clients through the dev master | repeatable end-to-end checks |
| [`MASTER.md`](MASTER.md) | your own master over ssh from the machine with the clone: `cargo xtask master install`, a self-signed certificate with no domain, what to hand players | standing up a relay for yourself or your friends |
| [`PERF.md`](PERF.md) | native `.pftrace` — the only runtime truth; picking a UUID, the manifest, `IW4L_PERF`, SQL | traces, scenario SQL, why a frame took 80 ms |
| [`FFI.md`](FFI.md) | native data ownership, lifetimes, nullability and Perfetto adapters | changing a native boundary or diagnosing trace failures |
| [`BENCH.md`](BENCH.md) | `make bench`: the map-load waterfall and stage `exclusive` time, frame time as a span tree, the render/GPU/work counters, and the run package (`manifest.json`, `summary.json`); in-process, no trace needed | "where did this run spend its time" |
| [`RENDER.md`](RENDER.md) | the nine crates of the island, the frame path `IR → cull → one drawsurf list → tess → material → SM3 → wgpu`, GPU-side ownership, the `d3d9_*` border | touching the picture, techsets, lighting |
| [`MATERIALS.md`](MATERIALS.md) | compiled material state, preparation generations, dynamic dependencies and adaptation provenance | changing material preparation or state admission |
| [`AUDIO.md`](AUDIO.md) | client AudioRuntime, control/render/device threads, fixed slots, virtualization and offline PCM execution | touching sound ownership, playback or callback resources |
| [`CUES.md`](CUES.md) | compiled cue semantics, typed spatial/routing requirements, media decode policy and revision ownership | changing cue or media preparation |
| [`FAMILIES.md`](FAMILIES.md) | family identity, native dependencies, map-selected characters and supported FPV combinations | changing cross-family composition |
| [`WEAPONS.md`](WEAPONS.md) | weapon configuration resolution, published consumer projections, host rules and registry lifetimes | changing attachments, combat or FPV preparation |
| [`ANIM.md`](ANIM.md) | three floors: `anim_iw4` (facts and curves), `xmodel_runtime` (tree and pose), who picks the clip (`sim` / `render_frontend/adapters/anim/`) | viewmodel, skeleton, bone hits |
| [`MAP-LOAD.md`](MAP-LOAD.md) | map load: the `session` → `assets` → install transaction, the `load_prepared_match` walk, the lane by `ZoneGame`, the artifact cache | a zone won't load, an asset went missing, "why didn't the match come up" |
| [`ENTITIES.md`](ENTITIES.md) | the `TickInput → sim::step → Snapshot` funnel, the `entity_iw4` taxonomy (`EntityState` / `Centity` / `ET_*` / trajectories), what sits where in `sim` | gameplay, networking, replay |
| [`SIM-STEP.md`](SIM-STEP.md) | `sim::step`: one `TickInput` → `Snapshot` funnel for authority, prediction and replay; `StepReason`; what makes a step deterministic | touching the step, prediction or replay |
| [`SKILL.md`](SKILL.md) | local Elo ratings, account persistence and migration | changing skill updates or account synchronization |
| [`GSC-RUNTIME.md`](GSC-RUNTIME.md) | GSC → executable IR → Bevy runtime; args, arrays and tables still share one `Runtime` | implementing gameplay or script execution |
| [`GSC-POSTFX.md`](GSC-POSTFX.md) | script vision, color correction, blur, DoF and bloom | authoring or debugging GSC post effects |
| [`BOTS.md`](BOTS.md) | host AI: the per-tick pipeline, what a probe that never ran may not claim, the shared query budget, resumable routes, fighting from a position | bot decisions, bot movement, "why is it standing there" |
