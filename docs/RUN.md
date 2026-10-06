# Run and poke the live game

How to touch the live process. Observation is [`PERF.md`](PERF.md).
Two players through the dev master: `make approved SCENARIO=master_duo_chaos`
([scenario and setup](../crates/approved_tests/README.md#two-clients-through-the-dev-master)).

```bash
set -a; . ./.env; set +a          # IW4L_GAMES; DISPLAY=:0 if the session has none
make map mp_boneyard CMDS='spawn 0; wait 2s; quit'
cargo run --profile play -p launcher -- map iw5:mp_overwatch --cmds '…'
```

`make` passes no foreign flags through: in the `Makefile` it is `CMDS`, on the
binary `--cmds`. The colon is a GNU make pattern, so `make map iw5:…` fails —
write `make map ZONE=iw5:mp_overwatch` or use `cargo run`. Recipes: `make
scenario`, `chaos`, `bench` ([`BENCH.md`](BENCH.md)), `bench-live`, `lifecycle-*`
(`*_CMDS` in the `Makefile`). Live recipes use `[profile.play]`; LTO is `PROFILE=release`.

Linux startup creates `iw4`, `iw5`, `t5` and `t6` symlinks in `IW4L_GAMES` for missing games with exactly one valid Steam installation. Existing entries are preserved.

**Cheats are on by default.** The host accepts the debug
actions: `move`, `look`, `tp`, `nudge`, `god`, `kill`, `damage`, `force_spawn`,
`force_match_start`, `rotatevelocity` and the `give` supply commands. `--no-cheats`
turns them off for `map`, `serve` and `menu`; a lobby host toggles them in the
game setup.

**GSC controls the start freeze.** `freezecontrols` constrains authority movement;
snapshots carry the same constraints into client prediction and command replay.
Wait for the loaded mode's countdown, or use the debug command:
`wait world; spawn 0; force_match_start; wait 1s; …`.
Firing, hit and decal probes must start after the countdown releases the player.

**Sync by default.** Ritual `wait`s are unnecessary: a command holds the FIFO
until its own fact — `map`/`demo`/`play` until the swap plus GPU-ready
`WorldScene::spawned`, `spawn` until `AppScreen::InGame`, `move`/`look`/`tp`
until a matching presented pose, `force_match_start` until `Playing`,
`disconnect` until the hold is torn down. `wait Ns` remains, for pacing and
observation windows. The `&` suffix is a loud refusal to wait (`map mp_rust &`),
`!` jumps the queue (in bash write `disconnect !`, or history expansion bites).
The 120 s gate timeout aborts the rest with an echo; parsing is
`crates/console/src/`. Hand-typed `dump`/`clip`/`screenshot` pass through any
wait while preserving the queue, which inside `--cmds` takes an explicit `!`.
A hand-typed `quit`/`exit`/`disconnect` goes further: it jumps **every** hold,
`wait 60s` included, and releases it. A `quit` written into `--cmds` does not —
it is part of the script and waits its turn.

## Verbs and traps

`map spawn class give attach name god kill damage move tp look nudge press hold
release bind bot wait mark record stoprecord clip demo dump screenshot ui disconnect quit
finish_run`
plus the debug `force_match_start` / `showpos`. `quit` leaves now and abandons
whatever screenshot was queued or half-written; `finish_run` is the scripted
ending that waits for those files first. `disconnect` leaves the session, not
just the world — it also leaves the room, or closes it when hosting, and works
with no map installed. `bot` is
`add | dummy | hold | tp | give | fire`; `dump [name]` writes the current snapshot into
`dumps/`. `clip` writes the last available 45 s into `clips/<ULID>/` (demo + dump)
on host and clients — the host records authority, a client the snapshots it
received plus its own presented state; `demo LATEST` plays it back.

* `god` toggles invincibility for a live player; repeat it to turn protection off. It blocks damage while cheats are enabled; `kill` still forces death.
* ADS is `hold +speed_throw`, not `+speed`;
* Hold Shift (`+breath_sprint`) or bind `+holdbreath` to steady eligible sights
  at full ADS. Breath lasts 4.5 seconds; releasing or exhausting it requires recovery.
* Aim binding: `bind MOUSE2 +speed_throw` aims only while held;
  `bind MOUSE2 +toggleads_throw` toggles aim on each press. Both are available
  under Options → Controls → Actions, and the chosen bind is saved in settings.
* bolt action (`fire_type=1`): `hold +attack` is **one** shot, and a full
  magazine will not reload itself (`press +attack` ×N, then `press +reload`);
* `look` without `LookState` only writes `ps.viewangles` — no aiming;
* Use (`+activate` or `+usereload`) retrieves your settled C4, claymores and
  deployable gadgets when there is room in their equipment ammo slot.
* `give` searches one completion list: `give ammo`, `give killstreak/uav`, or `give weapon/iw5:msr [attachment...]`. Search by any part of the name, then accept the suggested item.
* `give killstreak/care_package` and `give killstreak/pave_low` acquire rewards without activating them. Available familiar names appear alongside script names in autocomplete.
* `give ammo` refills carried reserves and equipment; reload magazines normally. Supply commands require a live player and a host allowing debug actions.
* `bot` hints follow the subcommand: counts, on/off, current bot IDs, weapons, and `tp … above`.
* custom classes live in `iw4l-artifacts/profile/classes.txt` (one tab-separated
  row per class); `spawn 0` selects the first slot. Delete the file to generate
  five available classes again. Random presets use a pool of 20: five each for
  IW4, IW5, T5 and T6, with IW4 equipment. Players and bots select only
  presets supported by the loaded weapons, attachments and equipment.

Callsign on the main menu selects a title and emblem. Killstreaks selects three
rewards with different kill requirements; Apply saves the selection for the next
loadout. Both persist in `iw4l-artifacts/profile/barracks.txt`. Titles, emblems and
rewards are available without progression requirements.

Video settings include brightness (50–150%, neutral 100%) and FOV (65–120°)
sliders and a saved First person / Third person camera selector. Use
`thirdperson` to toggle, `thirdperson 0|1` to select, or `set cg_thirdPerson 0|1`.
Death, killcam and remote missile cameras take priority. Drag with the mouse
or use Left/Right on the focused slider; the value
appears to its right. Multiplayer settings contain the player-name field
(Enter to edit and accept, Escape to cancel). These settings persist across
launches in `$XDG_CONFIG_HOME/iw4l/settings.cfg` or `~/.config/iw4l/settings.cfg`
on Linux; `IW4L_SETTINGS_PATH` selects a separate settings file for probes.
Listen hosts also persist the three script completion percentages in `profile.cfg`
beside that file. `IW4L_PROFILE_PATH` overrides the completion-profile path.

Listen hosts and clients store a signing key and derived account ID in
`account.dat` beside `settings.cfg`; `IW4L_ACCOUNT_PATH` overrides that path.
The file can also hold schema-stamped player-data snapshots. Listen imports them
before match scripts start and saves authority changes. New Listen buffers receive
the authored schema stamp, ten localized class names and captured stat defaults
before level entry scripts start. Player-data methods read and write the bound schema buffer;
external scripts block persistent-data writes. IW4 remote admission exchanges a
signed profile before gameplay; host updates save locally before acknowledgement.
Installed multiplayer validation and schema migration remain incomplete.
Older account files upgrade while retaining stats;
conflicting disk revisions refuse save instead of overwriting another writer.
