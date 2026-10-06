# `sim::step` — the one funnel

```rust
pub fn step(
    world: &mut SimWorld,
    tick: Tick,
    input: &TickInput,
    msec: i32,
    reason: StepReason,
) -> TickResult
```

`crates/sim/src/carrier.rs`. Everything able to change the authoritative world
enters through `TickInput`, including recorded shot-sample provenance. The tick
publishes a `TickResult` containing the snapshot, per-client action outcomes,
script effects and presentation metadata. `try_step` returns
`Result<TickResult, script::Fault>`;
`step` panics on script failure. Authority requires a loaded, started GSC program.

## One function, three callers

Server authority, client prediction and replay all call this same `step`. They
differ only in `StepReason`:

| reason | meaning |
|---|---|
| `AuthorityFrame` | the authoritative advance — the only reason `advances_authority_world()` holds |
| `PredictNew` | the local client running ahead of the server |
| `Replay` | a recorded input stream played back |

A human at a keyboard and a bot both arrive as entries in `TickInput.cmds`.
`sim` cannot tell them apart, and nothing downstream needs to.

* **State is explicit.** `&mut SimWorld` owns the `bevy_ecs` world, so two of
  them step side by side without touching each other: one in
  `net::authority::runtime`, one in `net::client::predict`.
* **Time is an argument.** `tick` and `msec` arrive per call instead of living
  in the world, so re-running a tick means passing it again.
* **Input is canonical.** `TickInput::canonicalize()` sorts commands and
  actions by `ClientId`, so the same set of inputs is the same input whatever
  order the network delivered it in.
* **Publication is closed.** Networking consumes the returned snapshot and
  effects without draining the simulation world. Each action reports applied,
  accepted for script execution, or refused; absence of a rejection is no verdict.

`adopt_snapshot` installs an authoritative snapshot; `adopt_prediction_snapshot`
reconciles the local client against one. Both live in `crates/sim/src/adopt.rs`
and answer with an `AdoptReport`.

See [`ENTITIES.md`](ENTITIES.md) for snapshots and [`GSC-RUNTIME.md`](GSC-RUNTIME.md) for script execution.
