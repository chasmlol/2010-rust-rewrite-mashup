# approved_tests

APPROVED TEST POLICY

Only explicitly owner-approved end-to-end scenarios may exist in this crate.
Agents may create temporary tests and probes during development, but those
must be deleted before the final squash/push unless specifically approved. A
useful test is not automatically an approved test.

A new permanent scenario is added here only after the owner has approved that
scenario by name, as a scenario the runner drives, never as a `cargo test`.
`make publish-check` refuses `#[test]`, `#[cfg(test)]`, `mod tests` and `tests/`
directories everywhere in the workspace, this crate included.

## Approved scenarios

| name | what it does |
| --- | --- |
| `master_duo_chaos` | two real clients join one dev master lobby, load `mp_boneyard` once, move for 4 seconds and run 16 seconds of RPG chaos with seven bots |
| `heavy_gameplay_lifecycle` | cold load `mp_overgrown` with 16 players, three input scenes, production disconnect, a watched menu, a second map with three scenes, production quit |

The scenario's steps are in `src/scenarios/heavy_gameplay_lifecycle.rs` and
nowhere else: maps, seed rule, scenes, positions, angles, weapons, durations
and input. To pin a scene to an owner-chosen position, replace its `Place`
there; the runner does not change.

The final `quit` phase sends `finish_run`, which uses the normal process-exit
boundary after the scenario's pending screenshots finish writing. Interactive
`quit` intentionally has only a short write grace period and can leave a
scenario-owned PNG incomplete under load.

## Running

```sh
make approved                                  # cold cache, fresh seed
make approved ARGS='--seed 1234'               # fixed seed
make approved ARGS='--replay iw4l-artifacts/approved-tests/<run>/run.json'
make approved ARGS='--cache shared'            # reuse the repo cache (not cold)
```

The runner builds nothing; `make approved` builds `target/play/iw4l` first.
Each run owns one directory, `iw4l-artifacts/approved-tests/<run-id>/`.
`heavy_gameplay_lifecycle` starts the game with that directory as its working
directory, so everything
IW4L writes (`iw4l-artifacts/cache`, logs, perf run, dumps, screenshots) lands
under it. On Windows the launcher enters the folder holding its executable, so
the runner hard-links (or copies) the binary into the run directory and starts
that one. With `--cache cold` that cache starts empty; with `--cache shared`
it is a symlink to the repository's cache.

A seed resolves to maps and spawn picks once. `run.json` keeps the resolved
set — the positions and angles the game actually used — and `--replay` runs
that set again rather than the seed, so a change in catalog order or spawn
selection cannot move a replayed scene.

## What a run leaves

- `run.json` — build, platform, render, content, maps, seed, the resolved
  scenes and the exact command script, cache mode, bot target and actual,
  lifecycle timestamps, phases with start/end or a failure reason, the GSC
  program each map installed (or the refusal that stopped it), liveness
  assertions, and paths to the large artifacts.
- `00_cold_load.png`, `01_overgrown_gameplay.{png,dump.txt}`,
  `02_after_disconnect.{png,dump.txt}`, `03_second_map_gameplay.{png,dump.txt}`
  when the run got that far.
- `child_stdout.txt`, `child_stderr.txt`, and the game's own
  `iw4l-artifacts/logs/latest.log` and `iw4l-artifacts/runs/<id>/` perf run.

Assertions are liveness only: the map loaded, its GSC program installed, the
players exist, simulation advanced, the authority applied the scene's movement
commands to the local player, the player covered a path of at least 32 units or
died, the lifecycle boundaries happened in order, no script fault ended the
process, and `quit` ended the process on its own.

Every match runs a GSC program, so the game announces it on stdout the way it
announces lifecycle boundaries: `gsc: installed map=… gametype=… fingerprint=…
modules=… functions=… natives=… entries=…` when the world is published, or
`gsc: refused map=… stage=compile|install|entry fault="…"` when the match load
stopped at the script. `run.json` keeps both under `gsc.a` / `gsc.b` and asserts
`a.gsc_installed`, `b.gsc_installed` and `gsc.no_execution_fault`. A phase that
fails after a refusal carries the refusal as its `failure_reason`, and a script
fault that ends the process (`GSC execution failed: …` in `child_stderr.txt`)
becomes the run's `failure`.

Movement is three facts, not one. `mark` reports the local player's input
receipt — commands the authority applied, how many of them carried a move, and
the summed length of every applied move — so a scene that turns while it walks
is judged by the path it walked, not by where it ended up. Each scene records an
`outcome`: `moved`, `died`, `died_before_input`, `held_in_place` (commands
applied, alive, no path: collision or a frozen player) or `input_not_applied`
(the break the scenario exists to catch). There
is no frame-time assertion. A run the controller had to kill is a failure,
never a quit.

## Two clients through the dev master

```sh
cargo xtask master install user@vps --channel dev --ca ~/.iw4l/dev-vpn-ca
cp ~/.iw4l/dev-vpn-ca/community-dev.iw4l-server context/simulated-iw4l-folder/
make approved SCENARIO=master_duo_chaos
```

`master_duo_chaos` stages the Linux game binary directly in
`context/simulated-iw4l-folder`. Both processes run from that directory and
select `IW4L_COMMUNITY` when set, otherwise its `community-dev.iw4l-server`.
The runner refuses descriptors outside
the dev channel (port 4434 and a TLS name matching the descriptor host or `iw4l-dev`). Networking uses the descriptor's address,
TLS name and embedded CA. Linux development binaries select the descriptor without fetching
Windows updates.

The host creates a lobby, the second client joins its ID, and the host waits
for exactly two members before starting. There is one map installation per
process. The host simulates seven bots in addition to the two human seats.
Four seconds of movement exercise the remote command path before the truck
and RPG scene. Eight samples cover another sixteen seconds, then both games
exit through `finish_run`.

Each run keeps separate host/client logs, account and settings files, dumps,
screenshots and native Perfetto traces under its approved-tests directory.
`IW4L_ARTIFACTS_DIR` redirects output without changing the game working directory.
`run.json` stores the descriptor, room, build, sample ticks and tick gaps,
assertions and failures; `commands.txt` stores every console script sent to each
process. Assertions check distinct human seats, replicated player count,
advancing simulation, remote movement applied by the authority, exactly one
map installation, script health and successful process exit. Client marks record both the latest adopted snapshot tick and the presented
tick, plus clock debt. Timing samples
and Perfetto `feel`/`remote` events expose stalls and presentation lag without
applying localhost latency thresholds to a VPS connection.

Dev master installs use `/usr/local/lib/iw4l-dev` and `/etc/iw4l-dev`, with the
`iw4l-master-dev.service` unit. Installing or updating dev does not overwrite
the prod executable or certificates.

Dev descriptors use the VPS host as the TLS identity. For an IP address,
the independently issued dev certificate includes that IP in its SAN. This
allows IP verification without sending a synthetic DNS label as SNI, preserving
the intended destination through proxies that inspect QUIC server names.
