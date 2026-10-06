# Absolute repo root from this Makefile's location — works with `make -C …`.
ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))

# Machine-local paths live in `.env` (see `.env.example`). No hardcoded games root.
ifneq (,$(wildcard $(ROOT)/.env))
include $(ROOT)/.env
export
endif

GOAL := $(firstword $(MAKECMDGOALS))
ARGS := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))

.PHONY: map export-gltf play bench bench-load-session bench-live bench-overhead bench-perf menu menu-shots scenario chaos lifecycle-all lifecycle-swap lifecycle-replace lifecycle-play-in lifecycle-demo-out lifecycle-demo-map launcher deploy logs loc clean help
.PHONY: build-windows setup-windows release publish provision
.PHONY: mr publish-check approved
.PHONY: $(ARGS)

$(ARGS):
	@:

# Console script played into the running game. `make` itself cannot carry an
# unknown `--cmds` flag (it parses its own options first and exits), so the
# spelling here is a make variable; the launcher binary takes `--cmds` verbatim:
#   make map mp_boneyard CMDS='spawn 0; hold +attack'
#   cargo run -p launcher -- map mp_boneyard --cmds 'spawn 0; hold +attack'
CMDS ?=
CMDS_ARG = $(if $(CMDS),--cmds '$(CMDS)',)
ZONE ?=
ZONE_ARG = $(if $(ZONE),--zone $(ZONE),)

# Live recipes use `[profile.play]` (Cargo.toml): release opt-level without
# the fat-LTO link. PROFILE=release is the LTO binary. `make deploy`
# uses this same PROFILE (default play); prod/dev is the publish channel.
PROFILE ?= play
PROFILE_ARG = --profile $(PROFILE)
RELEASE ?=
CARGO = cargo

# The scripted match. Jump on the spawn pad (open sky) before +forward carries under cover.
# Trailing wait lets the automatic reload finish. `quit` flushes `.pftrace`.
SCENARIO_ZONE ?= mp_boneyard
SCENARIO_CMDS ?= wait world; spawn 0; force_match_start; hold +attack; bot add 3; wait 3s; press +gostand; hold +forward; wait 5s; wait 4s; quit
# Truck 234 roof looking down + 7 bots RPG into the floor. Local does not fire:
# `hold +attack` made the truck splash a suicide, which has no killcam.
# Gate: T1/R1/R2/T6/K0/K1/K2/K3/K4/K6/K7/L1/F1/C1/P1/P2.
CHAOS_BOT_SETUP ?= bot tp 1 -1066 1391 127 174 85; bot tp 2 -1073 1362 80 174 85; bot tp 3 519 -44 16 61 85; bot tp 4 528 -14 16 61 85; bot tp 5 62 915 80 180 85; bot tp 6 62 900 80 180 85; bot tp 7 80 915 80 180 85; bot give 1 rpg; bot give 2 rpg; bot give 3 rpg; bot give 4 rpg; bot give 5 rpg; bot give 6 rpg; bot give 7 rpg; wait 1s
CHAOS_BOT_SALVO ?= $(CHAOS_BOT_SETUP); bot fire all
CHAOS_OBSERVER_SALVO ?= $(CHAOS_BOT_SETUP); spawn 0; move -1066 1391 500 174 85; bot fire all
CHAOS_CMDS ?= wait world; spawn 0; force_match_start; wait 16s; wait 2s; move -1066 1391 7 174 85; wait 1s; bot add 7; bot hold on; wait 3s; $(CHAOS_BOT_SALVO); wait 12s; $(CHAOS_OBSERVER_SALVO); wait 4s; $(CHAOS_OBSERVER_SALVO); wait 4s; $(CHAOS_OBSERVER_SALVO); wait 4s; $(CHAOS_OBSERVER_SALVO); wait 12s; quit
# Live trace run (not a demo). `force_match_start` so holds are not frozen in
# warmup. Local `+attack`/`+forward` plus `mouserate` (hold-yaw; not one-shot
# `mousemove`). `bot add 16` is the console clamp. Wait 10s, then quit so the
# native Perfetto session flushes.
BENCH_LIVE_CMDS ?= wait world; spawn 0; force_match_start; hold +attack; hold +forward; mouserate 10; bot add 16; wait 10s; quit
PERF_OVERHEAD_PAIRS ?= 10
PERF_OVERHEAD_WARMUP_PAIRS ?= 1
PROFILE_BIN = $(ROOT)/target/$(if $(filter dev,$(PROFILE)),debug,$(PROFILE))/iw4l

# Sync-by-default: `map`/`demo`/`disconnect`/`spawn`/`move` hold the FIFO until their
# fact (scene resident / torn hold / InGame / presented pose), so recipes carry no ritual gates.
# A leading `wait world` still guards the *CLI-initiated* load — the console
# cannot retroactively block a load it did not start. Deliberate overlap is a
# loud trailing `&` (`map mp_rust &`). Timed `wait Ns` stays for pacing.
# LIFECYCLE-SWAP. Second match in one Listen process. The post-disconnect gate
# is torn hold (HasWorld=0 and scene.spawned=0), not `wait 2s`. Occupancy
# claims are Perfetto events (`cargo xtask live swap`), not dump MAX(sequence).
LIFECYCLE_SWAP_CMDS ?= wait world; wait 2s; disconnect; wait torn; wait 1s; map mp_rust; wait world; wait ambient; quit
# LIFECYCLE-REPLACE. map over map, no disconnect.
# `wait world` after rust is scene.spawned; `wait ambient` is MapAmbientBooted.
LIFECYCLE_REPLACE_CMDS ?= wait world; wait 2s; map mp_rust; wait world; wait ambient; quit
# Match → demo (console `demo`). Records a few ticks first.
LIFECYCLE_PLAY_IN_CMDS ?= wait world; spawn 0; wait 2s; record swap_in; wait 2s; stoprecord; demo swap_in; wait 20s; quit
# Demo → disconnect → main menu.
# Theater occupancy is `theater` events; idle after torn is `cgame_hold`.
LIFECYCLE_DEMO_OUT_CMDS ?= wait world; spawn 0; record swap_out; wait 2s; stoprecord; demo swap_out; wait 8s; disconnect; wait torn; wait 8s; quit
# Demo → map (theater replaced by a live match).
# `record` before InGame refuses: sim_cam/cmds_enabled arm on presented Alive;
# sync `spawn` blocks until InGame, so the record below always arms.
LIFECYCLE_DEMO_MAP_CMDS ?= wait world; spawn 0; record swap_map; wait 2s; stoprecord; demo swap_map; wait 1s; map mp_rust; wait world; wait 3s; quit

require-games:
	@test -n "$(IW4L_GAMES)" || { echo "IW4L_GAMES unset — copy .env.example to .env and set the games root"; exit 1; }

# ZONE= is the colon escape hatch: GNU make parses `iw5:mp_overwatch` as a
# pattern rule (`target pattern contains no '%'`). `make map mp_boneyard` still
# uses ARGS. ZONE wins if both are set.
map: require-games
	@test -n "$(or $(ZONE),$(ARGS))" || { echo "usage: make map <zone>   e.g. make map mp_boneyard"; echo "       make map ZONE=iw5:mp_overwatch   (colon is a make pattern)"; exit 1; }
	cd $(ROOT) && $(CARGO) run $(PROFILE_ARG) -p launcher -- map $(or $(ZONE),$(ARGS)) $(CMDS_ARG)

export-gltf: require-games
	@test -n "$(or $(ZONE),$(ARGS))" || { echo "usage: make export-gltf <zone>"; exit 1; }
	cd $(ROOT) && $(CARGO) run $(PROFILE_ARG) -p launcher -- export-gltf $(or $(ZONE),$(ARGS))

# Play a recorded demo under iw4l-artifacts/demos/<name>.iw4ldemo, then quit.
# CMDS= still runs during playback (wait world; quit).
play: require-games
	@test -n "$(ARGS)" || { echo "usage: make play <demoname>   e.g. make play demo0000"; exit 1; }
	cd $(ROOT) && $(CARGO) run $(PROFILE_ARG) -p launcher -- play $(ARGS) $(ZONE_ARG) $(CMDS_ARG)

# The bench. One process, three independent reports written at exit: where the
# map load spent its time, where a gameplay frame spends its time by span, and
# what that frame asked the machine to do. All three are in-process — no trace
# file and no Trace Processor — and land in iw4l-artifacts/bench/<stamp>.txt as
# well as on stdout. The run's own directory gets report.txt, manifest.json and
# summary.json beside the .pftrace, so a number can be read months later.
#
#   make bench demo0011            play a demo   (also: DEMO=demo0011)
#   make bench ZONE=mp_boneyard    live map instead of a demo
#   make bench demo0011 PROFILE=release          the LTO binary
#
# A goal after `bench` is always a demo name, never a zone: one goal cannot
# tell the two apart, and ZONE= already names a map. `--demo` cannot be
# written on the command line — make would read it as a flag of its own —
# so the demo is a goal or DEMO=, and ZONE= still overrides a demo's header.
# IW4L_PERF is on as well, so `cargo xtask bench` can still read the .pftrace
# afterwards for the per-frame view this report deliberately does not give.
# Same [profile.play] binary as `make play`; AutoNoVsync matches renderer
# acceptance, Fifo would measure the monitor's refresh queue instead.
BENCH_DEMO ?= $(DEMO)
# The live-map script has to serve both halves: load the map, then stay long
# enough that section two has gameplay frames to count. `force_match_start`
# skips the 20 s warmup the controls are frozen through (docs/RUN.md).
BENCH_MAP_CMDS ?= wait world; spawn 0; wait ambient; force_match_start; wait 10s; quit
BENCH_DEMO_CMDS ?=
bench: require-games
	@test -n "$(or $(BENCH_DEMO),$(ZONE),$(ARGS))" || { echo "usage: make bench <demo>            e.g. make bench demo0011"; echo "       make bench ZONE=<zone>       e.g. make bench ZONE=mp_boneyard"; exit 1; }
	cd $(ROOT) && IW4L_BENCH_STARTED_NS=$$(date +%s%N) IW4L_BENCH=1 IW4L_PERF=1 IW4L_PRESENT_MODE=AutoNoVsync $(CARGO) run $(PROFILE_ARG) -p launcher -- $(BENCH_TARGET)

# `play <demo>` when a demo was named, `map <zone>` otherwise. A demo carries
# its own zone in the header, so ZONE= only overrides it.
ifneq ($(BENCH_DEMO)$(ARGS),)
BENCH_TARGET = play $(or $(BENCH_DEMO),$(ARGS)) $(ZONE_ARG) $(if $(or $(CMDS),$(BENCH_DEMO_CMDS)),--cmds '$(or $(CMDS),$(BENCH_DEMO_CMDS))')
else
BENCH_TARGET = map $(or $(ZONE),$(ARGS)) --cmds '$(or $(CMDS),$(BENCH_MAP_CMDS))'
endif

# One menu process, two synchronous loads. Engine markers exclude Cargo and
# process startup; the external reader subtracts their monotonic timestamps.
BENCH_LOAD_SESSION_CMDS ?= mark in_menu; map mp_hanoi; spawn; mark in_hanoi; map mp_underpass; spawn; mark in_underpass; quit
BENCH_MARKS_LOG ?= iw4l-artifacts/bench-load-session.log
bench-load-session: require-games
	@mkdir -p $(dir $(BENCH_MARKS_LOG))
	@bash -o pipefail -c 'IW4L_PERF=1 $(MAKE) menu CMDS="$(BENCH_LOAD_SESSION_CMDS)" | tee "$(BENCH_MARKS_LOG)"'
	python3 $(ROOT)/scripts/bench-marks.py $(BENCH_MARKS_LOG)

# One sampling profile of the live bench — the tool that names a frame without
# being told its name first. Same script and same zone as `bench-live`; the
# binary is [profile.perf] (Cargo.toml): play code, plus line tables and frame
# pointers so `--call-graph fp` can walk a stack. Needs the `perf` package;
# user-space sampling needs no root at perf_event_paranoid <= 2. Writes
# perf.data under iw4l-artifacts/perf. This remains a separate on-CPU sampler.
#   make bench-perf                 mp_boneyard, BENCH_LIVE_CMDS
#   make bench-perf PERF_FREQ=2999  finer sampling (more skid-free samples)
PERF_DIR ?= $(ROOT)/iw4l-artifacts/perf
PERF_FREQ ?= 999
bench-perf: require-games
	@command -v perf >/dev/null || { echo "perf not installed: sudo pacman -S perf"; exit 1; }
	@test $$(cat /proc/sys/kernel/perf_event_paranoid) -le 2 || { echo "perf_event_paranoid > 2 — user-space sampling denied"; exit 1; }
	@mkdir -p $(PERF_DIR)
	cd $(ROOT) && RUSTFLAGS="-Cforce-frame-pointers=yes" cargo build --profile perf -p launcher
	cd $(ROOT) && perf record -F $(PERF_FREQ) --call-graph fp -o $(PERF_DIR)/bench-live.data -- \
	  $(ROOT)/target/perf/iw4l map $(or $(ZONE),$(ARGS),$(SCENARIO_ZONE)) --cmds '$(or $(CMDS),$(BENCH_LIVE_CMDS))'
	@echo "perf.data: $(PERF_DIR)/bench-live.data"
	@echo "read it:   perf report -i $(PERF_DIR)/bench-live.data --stdio --no-children"

# Live map, not a demo. Zone from ZONE= / ARGS / SCENARIO_ZONE (mp_boneyard).
# Script is BENCH_LIVE_CMDS; CMDS= replaces it. The process prints the run path,
# then xtask reports active-gameplay percentiles and hot paths from that trace.
bench-live: require-games
	cd $(ROOT) && IW4L_PERF=1 IW4L_PRESENT_MODE=AutoNoVsync $(CARGO) run $(PROFILE_ARG) -p launcher -- map $(or $(ZONE),$(ARGS),$(SCENARIO_ZONE)) --cmds '$(or $(CMDS),$(BENCH_LIVE_CMDS))'
	cd $(ROOT) && $(CARGO) run --quiet -p xtask -- bench

# Alternating paired process benchmark. One binary and one script are used for
# both arms; the runner requires >=5 measured pairs and reports a paired 95% CI.
# VAR picks which recorder the on-arm turns on: IW4L_PERF (the Perfetto session,
# the default) or IW4L_BENCH (the in-process recorder `make bench` reads). They
# are separate costs and each needs its own pairs.
PERF_OVERHEAD_VAR ?= IW4L_PERF
bench-overhead: require-games
	cd $(ROOT) && $(CARGO) build $(PROFILE_ARG) -p launcher
	cd $(ROOT) && $(CARGO) run --quiet -p xtask -- perf-overhead --pairs $(PERF_OVERHEAD_PAIRS) --warmup-pairs $(PERF_OVERHEAD_WARMUP_PAIRS) --bin $(PROFILE_BIN) --zone $(or $(ZONE),$(ARGS),$(SCENARIO_ZONE)) --cmds '$(or $(CMDS),$(BENCH_LIVE_CMDS))' --var $(or $(VAR),$(PERF_OVERHEAD_VAR))

# Play the scripted match, then read player_tick events from the run's .pftrace.
scenario: require-games
	cd $(ROOT) && IW4L_PERF=1 $(CARGO) run $(PROFILE_ARG) -p launcher -- map $(SCENARIO_ZONE) --cmds '$(SCENARIO_CMDS)'
	cd $(ROOT) && $(CARGO) run -p xtask -- scenario

chaos: require-games
	cd $(ROOT) && IW4L_PERF=1 $(CARGO) run $(PROFILE_ARG) -p launcher -- map $(SCENARIO_ZONE) --cmds '$(CHAOS_CMDS)'
	cd $(ROOT) && $(CARGO) run -p xtask -- chaos

# Occupancy claims are Perfetto events (`cargo xtask live`); this target
# records, then runs that gate. Dump SQLite is not the reader.
lifecycle-all: require-games
	$(MAKE) lifecycle-swap
	$(MAKE) lifecycle-replace
	$(MAKE) lifecycle-play-in
	$(MAKE) lifecycle-demo-out
	$(MAKE) lifecycle-demo-map

lifecycle-swap: require-games
	cd $(ROOT) && IW4L_PERF=1 $(CARGO) run $(PROFILE_ARG) -p launcher -- map mp_boneyard --cmds '$(LIFECYCLE_SWAP_CMDS)'
	cd $(ROOT) && $(CARGO) run -p xtask -- live swap

lifecycle-replace: require-games
	cd $(ROOT) && IW4L_PERF=1 $(CARGO) run $(PROFILE_ARG) -p launcher -- map mp_boneyard --cmds '$(LIFECYCLE_REPLACE_CMDS)'
	cd $(ROOT) && $(CARGO) run -p xtask -- live replace

lifecycle-play-in: require-games
	cd $(ROOT) && IW4L_PERF=1 $(CARGO) run $(PROFILE_ARG) -p launcher -- map mp_boneyard --cmds '$(LIFECYCLE_PLAY_IN_CMDS)'
	cd $(ROOT) && $(CARGO) run -p xtask -- live play-in

lifecycle-demo-out: require-games
	cd $(ROOT) && IW4L_PERF=1 $(CARGO) run $(PROFILE_ARG) -p launcher -- map mp_boneyard --cmds '$(LIFECYCLE_DEMO_OUT_CMDS)'
	cd $(ROOT) && $(CARGO) run -p xtask -- live demo-out

lifecycle-demo-map: require-games
	cd $(ROOT) && IW4L_PERF=1 $(CARGO) run $(PROFILE_ARG) -p launcher -- map mp_boneyard --cmds '$(LIFECYCLE_DEMO_MAP_CMDS)'
	cd $(ROOT) && $(CARGO) run -p xtask -- live demo-map

menu: require-games
	cd $(ROOT) && $(CARGO) run $(PROFILE_ARG) -p launcher -- menu $(CMDS_ARG)

menu-shots:
	@test -n "$$IW4L_GAMES" -o -n "$(IW4L_GAMES)" || { echo "set IW4L_GAMES to the games root"; exit 1; }
	@mkdir -p iw4l-artifacts/menu-shots
	IW4L_GAMES="$(IW4L_GAMES)" IW4L_MENU_SHOT_DIR="$(CURDIR)/iw4l-artifacts/menu-shots" \
		$(CARGO) run $(PROFILE_ARG) -p launcher -- menu

# Every recipe below this line is `cargo xtask` — one Rust binary, no shell
# scripts. `cargo xtask` alone lists them; PROFILE and RELEASE reach it through
# the environment, the same way they reached the scripts.
XTASK = cd $(ROOT) && $(CARGO) run --quiet -p xtask --

launcher:
	@test "$(ARGS)" = windows || { echo "usage: make launcher windows"; exit 2; }
	@PROFILE="$(PROFILE)" $(XTASK) release bundles

build-windows:
	@PROFILE="$(PROFILE)" $(XTASK) windows build

setup-windows:
	@$(XTASK) windows setup

release:
	@test "$(firstword $(ARGS))" = prod -o "$(firstword $(ARGS))" = dev || { echo "usage: make release prod|dev"; exit 2; }
	@PROFILE="$(PROFILE)" $(XTASK) release $(firstword $(ARGS))

publish:
	@test "$(firstword $(ARGS))" = prod -o "$(firstword $(ARGS))" = dev || { echo "usage: make publish prod|dev"; exit 2; }
	@RELEASE="$(RELEASE)" $(XTASK) publish $(firstword $(ARGS))

deploy:
	@test "$(firstword $(ARGS))" = prod -o "$(firstword $(ARGS))" = dev || { echo "usage: make deploy prod|dev"; exit 2; }
	@$(MAKE) -C $(ROOT) release $(firstword $(ARGS)) PROFILE=$(PROFILE)
	@$(MAKE) -C $(ROOT) publish $(firstword $(ARGS))

provision:
	@$(XTASK) provision

logs:
	@test "$(firstword $(ARGS))" = prod -o "$(firstword $(ARGS))" = dev || { echo "usage: make logs prod|dev [SINCE=2h]"; exit 2; }
	@SINCE="$(SINCE)" $(XTASK) logs $(firstword $(ARGS))

# The agent-clone lifecycle, one verb:
#
#   make mr new <name>    clone the repo under context/mrs/<name> on branch
#                         agent/<name>, .env copied and the shared trees
#                         (artifacts, externals, the RE bases) symlinked in.
#                         A single agent on the root needs no clone.
#   make mr ship <name>   land it: rebase onto root master → rustfmt touched
#                         .rs + commit → FF-merge → delete the clone. The agent
#                         only commits code; none of those steps are done by
#                         hand. Refuses uncommitted WIP, rebase conflict, dirty
#                         root — and then the clone stays on disk.
#                         Read `git diff origin/master...HEAD` before running
#                         it: probes, throwaway tests and debug prints come back
#                         out of the tree first. CONTEXT.md, "Before shipping".
#   make mr ls            the clones on disk and whose move each one is
#   make mr fmt FILES='crates/foo/src/a.rs'   rustfmt exactly those paths.
#                         Empty / dir / non-rs refuse: `cargo fmt --all`
#                         rewrites files this branch does not own.
FILES ?=
# Owner-approved end-to-end scenarios (crates/approved_tests/README.md).
SCENARIO ?= heavy_gameplay_lifecycle
# Cold cache by default; ARGS='--seed N' | '--replay <run.json>' | '--cache shared'.
approved: require-games
	cd $(ROOT) && $(CARGO) build --profile play -p launcher
	cd $(ROOT) && $(CARGO) run --quiet -p approved_tests -- $(SCENARIO) $(ARGS)

# What a push would publish: nothing under `context/`, no `.env`, no key, no
# piece of a game install, and no retail offsets left over in the code. It is a
# grep, not a proof of provenance — where the code came from is README/NOTICE.
publish-check:
	@$(CARGO) run -q -p xtask -- publish-check

mr:
	@test -n "$(ARGS)" || { echo "usage: make mr <new|ship|ls|fmt> …   e.g. make mr new fps-retail-machines"; exit 1; }
	@$(XTASK) mr $(ARGS) $(FILES)

# Workspace Rust footprint: per-crate files / lines / bytes, group totals,
# and the heaviest source files. Counts only crates/ + xtask.
loc:
	@$(XTASK) loc

# Reclaim disk: wipe target/debug only. Keeps target/play (live recipes)
# and target/release (`PROFILE=release`) so neither has to relink. No rebuild.
clean:
	@before=$$(du -sh $(ROOT)/target/debug 2>/dev/null | cut -f1); \
	 if [ -z "$$before" ]; then echo "nothing to clean (no target/debug)"; exit 0; fi; \
	 echo "removing target/debug ($$before)…"; \
	 rm -rf $(ROOT)/target/debug; \
	 echo "done. kept: play $$(du -sh $(ROOT)/target/play 2>/dev/null | cut -f1 || echo none), release $$(du -sh $(ROOT)/target/release 2>/dev/null | cut -f1 || echo none)"; \
	 df -h $(ROOT) | tail -1

help:
	@echo "make map <zone>   run crates/launcher, zone e.g. mp_boneyard"
	@echo "                  make map ZONE=iw5:mp_overwatch  (colon cannot be a make goal)"
	@echo "                  add CMDS='spawn 0; hold +attack' to script it"
	@echo "                  sync-by-default: map/demo/disconnect/spawn block the FIFO"
	@echo "                  until done; trailing '&' opts out (map mp_rust &)"
	@echo "make play <demo>  play iw4l-artifacts/demos/<demo>.iw4ldemo, then quit"
	@echo "                  ZONE= overrides header"
	@echo "                  CMDS='wait world; wait 5s; quit' mid-play"
	@echo "make bench <demo>  the bench: map-load breakdown, frame time by span,"
	@echo "                  render/GPU/work counters; printed at exit, written to"
	@echo "                  iw4l-artifacts/bench/ and the run dir as a package"
	@echo "                  a goal is always a demo name; ZONE=mp_boneyard benches"
	@echo "                  a live map instead, DEMO=demo0011 is the goal form"
	@echo "                  same [profile.play] as map/play; PROFILE=release for LTO"
	@echo "make bench-live [zone]  live map: force start, fire/walk/yaw hold, 16 bots,"
	@echo "                  wait 10s, then flush trace.pftrace (default mp_boneyard)"
	@echo "                  ZONE=iw5:mp_overwatch  (colon cannot be a make goal)"
	@echo "                  then prints p50/p90/p95/p99 and hot paths from .pftrace"
	@echo "make bench-load-session  menu → Hanoi → Underpass; engine mark intervals"
	@echo "make bench-overhead  paired default-off/on runs of one built binary;"
	@echo "                  PERF_OVERHEAD_PAIRS=10, alternating order, paired 95% CI"
	@echo "make bench-perf   same live run under perf record (needs the perf package),"
	@echo "                  [profile.perf] binary, perf.data under iw4l-artifacts/perf"
	@echo "make scenario     play the scripted match, then gate its .pftrace"
	@echo "make chaos        truck + RPG into the floor, then gate the .pftrace"
	@echo "make lifecycle-all  run and gate all five occupancy transition recipes"
	@echo "make lifecycle-swap  LIFECYCLE-SWAP: boneyard → disconnect → rust, then .pftrace"
	@echo "make lifecycle-replace  map over map: boneyard → map mp_rust, no disconnect"
	@echo "make lifecycle-play-in  match → record → demo (console demo/play)"
	@echo "make lifecycle-demo-out  demo → disconnect → menu"
	@echo "make lifecycle-demo-map  demo → map mp_rust"
	@echo "make menu         run the main-menu shell (Maps / Settings / Quit)"
	@echo "                  add CMDS='wait 2s; quit' to script it"
	@echo "make menu-shots   2D UI pack under iw4l-artifacts/menu-shots (no map)"
	@echo "make launcher windows  build password-protected dev + prod portable ZIPs"
	@echo "make build-windows     local Windows bins only (PROFILE=play)"
	@echo "make setup-windows     rustup target + cargo-xwin (once)"
	@echo "make release prod|dev  build+pack a local release; VPS untouched"
	@echo "make publish prod|dev  upload RELEASE= (or dist/releases/<ch>/LATEST)"
	@echo "make deploy prod|dev   release + publish; PROFILE=play unless set"
	@echo "make provision         VPS users/dirs/caddy/systemd/certs/firewall"
	@echo "make logs prod|dev [SINCE=2h]  print the bounded master journal"
	@echo "cargo xtask master install user@host   your own relay on your own VPS"
	@echo "                  then: master update|status|logs|uninstall (docs/MASTER.md)"
	@echo "make mr new <name>   clone the repo for one agent under context/mrs/<name>"
	@echo "make mr ship <name>  rebase → rustfmt touched .rs → FF onto master → rm clone"
	@echo "make mr ls           the clones on disk and whose move each one is"
	@echo "make mr fmt FILES='a.rs b.rs'  rustfmt exactly those files, nothing else"
	@echo "make loc          Rust LOC / file counts / sizes per crate"
	@echo "make clean        wipe target/debug (keep play + release); no rebuild"
	@echo "IW4L_GAMES=$(IW4L_GAMES)"
	@echo "config: $(ROOT)/.env (from .env.example)"
	@echo "ROOT=$(ROOT)"
