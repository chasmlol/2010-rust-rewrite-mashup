# `docs/` — the one-page map

Short files (each ≤50 lines) on what lives where and how to poke the live
game. Keep them this short: nobody opens a long file twice.

| file | about | when to read |
|---|---|---|
| [`JAK.md`](JAK.md) | Jak Mode: Jak 3's JET-Board and Blaster rewritten in Rust, controls, where the pieces live, parity tests, what isn't done | playing or changing Jak Mode |
| [`JAK-TESTING.md`](JAK-TESTING.md) | trying Jak Mode on your machine: build, run, each control and what it should do | checking Jak Mode works locally |
| [`SKATE.md`](SKATE.md) | Skate 3 mode: what you need, where `default.xex` comes from, setup, controls, how it works, building a release | playing or changing the skate mode |
| [`PERFORMANCE.md`](PERFORMANCE.md) | the frame-time work in this fork: before/after numbers and every change | "why is it faster", profiling |
| [`IW4L.md`](IW4L.md) | upstream IW4L's own README | what IW4L is |
| [`BUILD.md`](BUILD.md) | system packages per distro (Fedora / Debian / Arch), macOS, what the Windows cross build needs | before your first build |
| [`RUN.md`](RUN.md) | running (`make map`, `--cmds`), controls frozen until `Playing`, `force_match_start`, sync-by-default, the verb list and the traps | before your first live run |
| [`WINDOWS.md`](WINDOWS.md) | portable `iw4launcher.exe`: `.env`, shortcuts into CoD, writable `iw4l-artifacts/` | building and running on Windows |
| [`DEPLOY.md`](DEPLOY.md) | `make release` / `publish` / `deploy`: the play profile, hashed `.zst`, master by SHA, provision kept separate | shipping a release, "why is the player on an old version" |
| [`DUO.md`](DUO.md) | `make duo`: two windows, fresh lobby ID, per-client console commands | reproducing multiplayer bugs locally |
| [`MASTER.md`](MASTER.md) | your own master over ssh from the machine with the clone: `cargo xtask master install`, a self-signed certificate with no domain, what to hand players | standing up a relay for yourself or your friends |
| [`PERF.md`](PERF.md) | native `.pftrace` — the only runtime truth; picking a UUID, the manifest, `IW4L_PERF`, SQL | traces, scenario SQL, why a frame took 80 ms |
| [`BENCH.md`](BENCH.md) | `make bench`: the map-load waterfall and stage `exclusive` time, frame time as a span tree, the render/GPU/work counters, and the run package (`manifest.json`, `summary.json`); in-process, no trace needed | "where did this run spend its time" |
| [`RENDER.md`](RENDER.md) | the nine crates of the island, the frame path `IR → cull → one drawsurf list → tess → material → SM3 → wgpu`, GPU-side ownership, the `d3d9_*` border | touching the picture, techsets, lighting |
| [`ANIM.md`](ANIM.md) | three floors: `anim_iw4` (facts and curves), `xmodel_runtime` (tree and pose), who picks the clip (`sim` / `render_frontend/adapters/anim/`) | viewmodel, skeleton, bone hits |
| [`MAP-LOAD.md`](MAP-LOAD.md) | map load: the `session` → `assets` → install transaction, the `load_prepared_match` walk, the lane by `ZoneGame`, the artifact cache | a zone won't load, an asset went missing, "why didn't the match come up" |
| [`ENTITIES.md`](ENTITIES.md) | the `TickInput → sim::step → Snapshot` funnel, the `entity_iw4` taxonomy (`EntityState` / `Centity` / `ET_*` / trajectories), what sits where in `sim` | gameplay, networking, replay |
| [`SIM-STEP.md`](SIM-STEP.md) | `sim::step`: one `TickInput` → `Snapshot` funnel for authority, prediction and replay; `StepReason`; what makes a step deterministic | touching the step, prediction or replay |
| [`GSC-RUNTIME.md`](GSC-RUNTIME.md) | GSC → executable IR → Bevy runtime; args, arrays and tables still share one `Runtime` | implementing gameplay or script execution |
| [`BOTS.md`](BOTS.md) | host AI: the per-tick pipeline, what a probe that never ran may not claim, the shared query budget, resumable routes, fighting from a position | bot decisions, bot movement, "why is it standing there" |
