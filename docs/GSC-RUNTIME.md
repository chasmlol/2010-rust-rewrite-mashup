# GSC → IR → Bevy

Rust language, IR and host integration live under `sim::script`.
Gameplay policy belongs to loaded GSC; Rust supplies language and engine primitives.
The scripts own the match flow,
player connect/spawn/damage/killed, the in-game menus, and the sounds they play.

## Sources and installation

Matches support IW4 modes only (dm, war, dom, sd, sab, ctf, koth, dd).
T5 and IW5 maps supply entities and map declarations; their gameplay/UI scripts
are excluded. Menu/HUD layout and match flow stay IW4, with weapon and team icons
from the corresponding game. Objective models, materials, shaders and effects
come from the map's game, including the plant/defuse weapon viewmodels and animations.
`ObjectiveVisuals` binds IW4 names to those assets at
install; missing bindings or foreign objective assets refuse the transaction.
Common scene models are selected from retained IW4 source strings, table cells,
map entities and objective bindings, without a per-model capture allowlist.

`Program::load` compiles a `SourceResolver`. `FileSources` reads explicit files.
IW4 asset walks retain GSC RawFiles from common_mp and the map; map files override
common files. `ScriptSources` also keeps every string table (code_post_gfx_mp and
patch_mp, then common_mp, then the map, later zones winning) and the map's entity
string. Sources travel through the common/resident caches into PreparedMatch.
Session preflight compiles the mode and map modules and builds `LevelData` (parsed
entities, tables). The install transaction installs the program, runs
`codescripts/struct::initstructs` synchronously (it must not wait), spawns the map
entities, sets the `mapname` and `g_gametype` dvars and schedules the `Iw4Startup`
entries before authority ticks: the gametype `main`, the map `main` (if present), then
`_callbacksetup`'s `codecallback_startgametype`. Missing dependencies, unsupported
syntax, unresolved names and unbound natives fail the transaction.

Map entities: `worldspawn` is kept as level settings (`getnorthyaw`),
`script_struct` blocks become plain objects appended to `level.struct`, and every other
block, known classname or not, becomes a script entity with typed keys (`origin`,
`angles`, `angle` as yaw, integer `spawnflags`/`count`/`health`/`dmg`/`maxhealth`,
float `speed`/`radius`/`height`, strings otherwise). Every entity except a hud element
carries `classname`, `code_classname` (`script_vehicle` for any `script_vehicle*`) and a
zero `origin` and `angles` until something sets them. Entity numbers start at the first
non-client slot.
Names are normalized; traversal is rejected. The transaction announces its script
outcome on stdout for a controller: `gsc: installed …` with the program fingerprint
and module/function/native/entry counts once the world is published, `gsc: refused …
stage=compile|install|entry fault="…"` when the load stopped at the script.
`crates/approved_tests` records and asserts both.

Natives are linked against a `Catalog` of the game's builtins (generated into
`script/profile/iw4_catalog.rs`, split into function and method namespaces), not against the
registry. An unqualified call resolves to a function in the same file, then a builtin
in the call's namespace, then a unique include. Qualified names and `thread` calls
never resolve to builtins. A developer builtin used as a statement compiles to nothing
(arguments are not evaluated); using its value or referencing it fails the load.
`prof_begin`/`prof_end` statements compile to nothing. SHA-256 covers decompressed source bytes
after terminator removal. Fingerprints hash IR version 5 and each module's
`(site, realm, origin)`; server modules of the IW4 and T5 realms are instantiated. Calls, `::f` references (script
`Function` or native `Builtin`), locals (per-function slots) and field names (symbol ids)
are resolved at load, and `install` binds native slots once.
UTF-8 and single-byte sources are supported; developer blocks are excluded.

## Language and scheduling

IR supports calls/references/indirect calls, receiver threads, locals, object fields,
arrays/index lvalues, constants, for/foreach/while, switch, conditional expressions,
waits and receiver-bound events. Localized strings and animation literals retain
separate value types. Values are `Int(i32)` and `Float(f32)`. The operator, cast, truth and
string-conversion rules are:
- int/int `/` yields a float. Division or `%` by zero yields zero and does not fault.
- `% & | ^ << >>` accept ints only.
- A type mismatch in `==` or a comparison is a runtime error.
- Only ints and floats have truth.
- Floats print with MSVC `%g`.
- Overlong int literals wrap.


Ordinary strings retain their original bytes. UTF-8 strings and legacy byte
strings use the same GSC string type; equality and array keys compare bytes.
String size, indexing, `getSubStr`, `isSubStr`, `strTok` and `toLower` operate
on bytes, with ASCII-only case conversion. Concatenation preserves the bytes.
Single-byte source literals retain the source encoding. The public
`ScriptString::as_bytes()` and `Value::byte_string()` APIs carry non-UTF-8
values without converting them into overlong UTF-8 strings. Text adapters for
asset names, dvars and presentation remain separate from this byte storage.

Arrays copy on assignment and argument/event transfer; objects preserve aliases.
Foreach snapshots sorted keys.

Scheduling uses time buckets:
- The run loop pops the head of the current bucket.
- `wait` pushes to the head of its target bucket.
- `waittillframeend` pushes to the tail of the current bucket.
- `wait` counts server frames: an int is 20 frames per second, a float rounds
  `f * 20 + 0.5` down in f32, and a nonzero wait is at least one frame. A negative or
  too-long wait faults. `wait 0` resumes in the same frame, after the current thread yields.
- `thread f()` runs `f` inline until its first yield, then the spawner continues.
  Calls and thread calls share a limit of 31 frames across the running thread and the
  spawners suspended under it. An endon that fires on a suspended spawner unwinds it
  when control returns to it.

Events use one waiter list per program:
- Notify walks the waiters oldest-registration first and never runs script.
- Woken threads go to the head of the current bucket, so they resume newest first.
- `waittillmatch` skips non-matching payloads silently.
- `endon` is scoped to the registering frame. Firing it kills that frame and the frames above
  it. The caller resumes with `undefined`, or the thread ends if the root frame was killed.
- An endon registered before a waittill on the same event wins.

Deleting an entity ends the threads that `waittill` or `endon` on it, checked at the
start of each tick and before a thread resumes. A thread whose `self` is deleted keeps
running. The `delete` native sends `death` before retiring the entity. A deleted entity
keeps its fields until the frame ends, so `death` handlers can still read them; after that, references point at a
fieldless dead entity: field reads give `undefined`, writes land, `isdefined` is false
and the id is reclaimed by heap collection.

Runtime errors recover: the failed
operation leaves `undefined` in place of its results and the thread carries on. A failed
condition falls into the body, a failed switch takes the default, and a failed `foreach`
operand iterates nothing. Each site is logged once (`gsc: runtime_error at=… fault="…"`)
and counted. A thread that runs 16M instructions in one resumption is killed as a runaway
loop (level startup runs in one frame and needs over a million on the larger maps). A
panicking builtin is a runtime error of its caller. Array copies have bounds.
Heap collection follows globals and live threads, including cycles; natives must
keep persistent script values in script-owned roots rather than retain raw handles.

## Engine boundary and restoration

Natives are grouped by what they touch; the core three:
- `script/host/natives/iw4.rs`: dvars (`getdvar*` with an optional default for a missing dvar,
  `setdvar` storing a localized value as its reference, `setdvarifuninitialized`,
  `makedvarserverinfo`), precache and `loadfx` (allowed only while the first tick is
  loading), string helpers, presented client state (fog, vision, ambient).
- `script/host/natives/math.rs`: math in degrees, vectors, deterministic randomness (a runtime-owned
  xorshift seeded from the program fingerprint, so clones and replays draw the same
  numbers), substrings, and string tables (case-insensitive lookups, `""` or `-1` when
  missing).
- `script/host/natives/engine.rs`: entity queries and `spawn`, entity methods (origin, model,
  visibility, contents, link offsets, attachments), timed moves and rotations that
  notify `movedone`/`rotatedone`, radius-trigger `istouching`, `placespawnpoint`,
  hud elements, world traces against the map's clip, team scores and radar, match data,
  level settings, and weapon facts from the captured weapon table.

Argument reads live in `script/host/args.rs`, array helpers in `script/host/arrays.rs`,
and table lookup in `script/host/tables.rs`. All three use the one `Runtime`
(`arrays`, `next_object`, `tables`, `rng`). The compiler is `sim::script::compiler`:
`Token`/`lex` and the call, expression, assignment, and statement methods sit in child
modules, and `Parser` stays in the parent.

`SoundExists(alias)` checks the composed match sound-alias catalog, installed
before script entry points run even when audio output is disabled. Host aliases
resolve case-insensitively; imported aliases use an explicit `t5:` or `iw5:`
prefix. A missing catalog reports an unavailable native error. Arguments use the usual
script string conversion.

`Objective_Team(index, player)` is an extension: given a player instead of a team
name, the objective shows to that player alone, in team and free-for-all modes
(T6 sensor grenade pings, `iw4l_t6/equipment`). The compass draws objectives in
free-for-all only when they are addressed this way.

`AmbientPlay(alias, [fadeSeconds])`, `AmbientStop([fadeSeconds])` and
`SetAC130Ambience(alias, [fadeSeconds])` update persistent sound state in
snapshots. The client prepares changed aliases, selects authored variants and
applies voice fades. The player AC130 flag selects the separate AC130 track;
leaving AC130 restores the normal track. Stops affect global ambience, while
CreateFX emitters retain their own playback. Joining clients use the remaining
fade deadline.

`PlayRumbleOnEntity(name)` and `StopRumble(name)` are entity methods;
`PlayRumbleOnPosition(name, position)` is a global function. Names must be
registered by `PrecacheRumble` during script loading. These commands send
ordered events to the client controller mixer. Authored rumble graphs determine
motor intensity and duration; `broadcast`, `range` and `fadeWithDistance`
determine spatial eligibility and attenuation. Represented moving entities use
their presented positions. Named stops affect matching entity rumbles, including
local weapon notetracks. Vibration preferences, focus, pad selection and player
lifecycle still govern controller output. Missing definitions or graphs are
diagnosed by the client.

`PlaySound(alias)` and `PlaySoundAsMaster(alias)` emit the same spatial sound
event in the MP runtime. They take one alias argument and require a live entity;
a valid alias is sent before an extra notify argument produces an error. Missing aliases, looping aliases, unavailable
looping flags, and a full sound-alias configstring table produce errors without
emitting an event. Use
`PlayLoopSound` for looping aliases.

Sounds the scripts play (`playLocalSound`, `playSoundToPlayer`, `playSoundToTeam`)
reach the client. Effects, rumble and earthquakes validate their receiver and are
kept in `Runtime.presented` or dropped. In-game menus (team, class, escape, leave-game,
scoreboard header) and the IW4L frontend run from menuDefs through the same ordered
GPU menu pass. `crates/ui/menus/frontend.json` defines IW4L navigation and native
menu styling; frontend service commands connect it to session and master APIs.

`MoveSlide(center, radius, velocity)` starts continuous script-mover motion with
an offset collision sphere, gravity and collision-plane sliding plus an 18-unit
step attempt. `slideVelocity` reads the current velocity and accepts finite vector
writes while sliding; it reads zero when inactive. `StopMoveSlide()` freezes the
current pose without emitting `movedone`. Stop sliding before a timed origin move.
Linked players follow the mover and are excluded from its collision query.

`CanMantle()` probes the current player's facing direction against mantle
surfaces, landing space and clearance using the movement collision backend.
It leaves player state unchanged. `ForceMantle()` repeats that probe and enters
the existing mantle root-motion controller without requiring jump input.
Dead or linked players and unavailable ledges cannot start a mantle; a failed
force request reports a native error. Authored mantle animations are used when
loaded, with the existing movement fallback otherwise.

`AllowADS(bool)` gates ADS through the replicated player weapon flags.
`AllowSprint(bool)` gates new sprints and ends an active sprint when disabled.
`IsReloading()` and `IsSwitchingWeapon()` inspect both weapon hands;
`IsDualWielding()` inspects the held inventory weapon's dual-wield latch.
`GetCurrentWeaponClipAmmo()` reads the held weapon's primary-hand clip, and
`GetOffhandPrimaryClass()` reads the current offhand class.
`SwitchToWeaponImmediate(weapon)` arms an owned weapon immediately, cancels its
pending switch/reload state and preserves the player's ammo inventory.

`MoveX/Y/Z(distance, seconds, accelSeconds = 0, decelSeconds = 0)` move an
non-player entity by a relative distance along the corresponding world axis,
with the same ramp and `movedone` behavior as `MoveTo`. `IsLinked()` reads the active
entity or player link. `LocalToWorldCoords(vector)` rotates a local vector by
the entity's angles and adds its origin; players use their current view angles.

`GetFirstArrayKey(array)` and `GetNextArrayKey(array, previousKey)` traverse
keys in the same deterministic descending order as `GetArrayKeys`, preserving
integer and string key types. Empty arrays and the end return undefined. A
previous key must still exist; mutation during iteration is not stabilized.

`Kick(clientNumber, reason = "EXE_PLAYERKICKED")` requests client retirement.
The reason must contain 1–256 bytes and no control characters. Repeated requests
before retirement preserve the first reason. The client receives a terminal
control failure through the existing reliable route; transport membership is
retired, and the next authority tick runs the script disconnect callback and
removes the player, owned HUD, trigger claims and command state.

`SetSlowMotion(startScale, endScale = 1, seconds = 1)` changes the rate of the
50 ms gameplay ticks and the client game clock. Script waits, entity mechanics,
weapons and player simulation follow that rate together. Scales must be finite
and positive; zero duration applies the end scale immediately. The transition
is linear over real time: its phase is recovered from the integrated game clock
so snapshots and late joins share the same transition. Bevy frame quantization
and its time clamp still apply. Raw input timing and connection clocks use real
time. Removing the match's time policy restores normal speed.

`PhysicsExplosionSphere(origin, outerRadius, innerRadius, magnitude)` applies an
outward impulse with an upward bias, full strength inside the inner radius and
linear falloff to zero at the outer radius. It wakes server bodies registered by
`PhysicsLaunchServer/Client`, including bodies that have settled, and sends the
sphere to clients for map props with a physics preset. Client props use their
mass and explosive-force scale; server script bodies use unit mass. Timed scripted movement or vehicle teleport cancels a
server body's physical motion.

Killstreak hardware:
- `spawnHelicopter` arms the vehicle with its VehicleDef `turretWeaponName` (captured
  from the zone, including weapon bodies loaded inline through the vehicle), so the
  gunner's `turret_fire` → `fireWeapon` loop needs no `setVehWeapon`. Turret and vehicle
  weapons with clip 0 or zero fire/raise times are valid combat rows.
- Sentry placement traces a ray to 42 + 5 units ahead,
  a ±30 box drop, normal ≥ 0.7, then four feet (17, 20, 10) traced 20 down that tilt the
  seat, failing when two miss. A failed placement keeps the fallback pose.
  `setContents(0)` makes the carried turret non-solid, so the traces skip it;
  `setContents` returns the previous contents for the restore pattern.
- `notifyOnPlayerCommand` notifies go out for every bound command, including
  `+actionslot N`; `+breath_sprint`, `+stance` (tap crouch, 300 ms hold prone) and
  `centerview` act on the client.
- The laptop map (`setminimap` frame, location selection with direction and yaw) draws
  from the map's compass corners.
- Entity allocation is shared by movers, grenades, missiles and dropped items. A full
  pool drops the new entity with a warning instead of ending the match. `radiusDamage`
  with an invalid area and `glassRadiusDamage` are no-ops.
- A dying player gets `death` (with the attacker) before `CodeCallback_PlayerKilled`;
  the stock scripts send it themselves only for faux death, so every
  player `endon( "death" )` thread depends on it.

The client draws a carried sentry at the authority's 20 Hz pose, not re-placed per
frame from the predicted player state.

The scripts drive the match phase. Every `level` notify is recorded as a signal, and
the simulation reads them after the scheduler each tick: `prematch_over` finishes the
prematch (movement unlocks), `exitlevel` moves to intermission. `map_restart` frees
the level's entities and hud elements, reloads the scripts and keeps the primitive
part of `game` (and `pers` when asked to persist).
Disconnect requests from transport are queued for an authority frame. The player
callback runs before Rust removes the player state and metadata; it can still read
the name, score and team while the scripts remove the player from their lists.
Scripts run and are checked only on authority frames (`try_step`/`step` with
`StepReason::AuthorityFrame`); prediction of new and replayed commands steps a
script-less world. Per-client script control constraints travel in snapshots and are
restored before prediction and command replay. Button-query natives sample raw commands
from the current authority frame, including presses between its first and last commands.
Clones preserve program, heap, threads and waits. Shutdown resets them. Restoring
snapshots into an installed authority VM rejects absent script state. Persistent
replay pinning/restoration and client-role lifecycle evidence are still missing.
Only terminal faults (exhausted identifiers, inconsistent IR) are sticky; they end the
match and return the host to the lobby. Earlier
writes are not rolled back.

`GetMissileOwner(missile)` returns the original player entity for an adopted missile,
including a lingering grenade after explosion. It returns undefined after that player
disconnects; reconnecting in the same client slot does not transfer ownership.

Damage triggers support `EnableGrenadeTouchDamage()` and `DisableGrenadeTouchDamage()`.
Enabled triggers test the grenade's actual swept movement through their hulls and emit
`damage` (inner explosion damage, grenade entity, direction, zero point, MOD_GRENADE),
then `trigger` when threshold, accumulate and response flags permit activation.
Bullet segments and radius damage use the same activation policy without requiring
grenade touch to be enabled. Splash uses bounds distance, cone/visibility samples and
per-entity radius eligibility. Single-use triggers retire after notification delivery.
Melee tests its central trace with base damage, including physical misses. Terminal
missile impacts test their collision point; duds report MOD_IMPACT. Adopted projectile
impact/splash attribution uses the captured owner object across client-slot reuse.

`IsUsingTurret()` reads the player's authoritative turret-active prone/duck flags.


Player 0 can read and write `percentcompletesp`, `percentcompletemp` and
`percentcompleteso` through `GetLocalPlayerProfileData(name)` and
`SetLocalPlayerProfileData(name, value)`. Names ignore ASCII case; unknown fields
fail. The setter requires an integer and stores its low byte, including values
outside 0–100. Get needs at least one argument and set at least two; extra
arguments are ignored. Other players return undefined before validating arguments.
These fields are separate from `Get/SetPlayerData` and survive script resets and
simulation copies. New local profiles start at zero.

Listen hosts load the local profile before scripts run and save changes at the
end of each application frame, before normal console exit. Map replacement
inherits the current authority profile. `profile.cfg` sits beside `settings.cfg`;
`IW4L_PROFILE_PATH` selects a separate file. Client, Replay and Dedicated roles
do not load or save this local file. This storage does not implement multiplayer
account stats, unlocks, rested time or platform identities.


Listen hosts and clients load `account.dat` beside `settings.cfg`, with
`IW4L_ACCOUNT_PATH` as its override. It holds a signing key, derived account ID and
an optional schema-stamped player-data buffer. Listen binds existing buffers to
the local client before level entry scripts start and saves authority changes
at the end of the frame. Schema installation rejects recursive layouts, overflowing
storage spans and fields or array elements that exceed their containers. Packed
booleans use bit offsets; zero-size struct metadata derives its span from fields.
Schema mismatches refuse import. Client frame-end saves
persist supplied snapshots. IW4 relay admission verifies a signed account profile
and binds it before accepting gameplay or releasing Enter. Host changes return
to that owner over control; saved ACKs require an exact successful disk receipt. Replay and
Dedicated roles do not read or write this file. Version 1 files upgrade to keyed
identities while retaining all stat bytes and revisions. Writes use a file lock
and reject stale disk revisions. On Unix the key file is owner-readable/writable.
A new Listen account initializes an 8,188-byte buffer with the first authored schema stamp, ten raw localized
class names and the captured `mp/stats_init.cfg` assignments before level entry
scripts start. Missing or invalid defaults refuse initialization without
publishing a partial record. Existing account records are retained.
`Get/SetPlayerData` use the bound account buffer with exact schema keys and
scalar types. Struct and enum-array names use GSC string casts; indexed arrays
require integer keys. Name matching stops at the first NUL byte. Strings retain
original bytes; failed writes preserve the record.
Missing accounts fail rather than return guessed stats. `developer_script`
blocks script writes. Explicit class selection writes the schema atomically;
reconnecting does not reseed saved classes. Loading any external or unclassified
script blocks persistent-data writes for the entire program, including packaged
callers. Captured zone scripts and runtime-generated scripts carry explicit
packaged/built-in origins. File and plain-map resolvers default to external;
custom resolvers must declare their source origin. Clones and map restarts keep
the loaded program's policy; a fresh program load derives a new policy. Origin
records source delivery, not a signature or platform identity. Full connection
state parity, schema migration and platform identity remain incomplete.
Remote profiles bind a signing key to member, connection, session, epoch and
a fresh host challenge. Duplicate owners and stale contexts refuse admission.
This protocol uses game-wire version 106; installed multiplayer validation remains
required.
Host bots bind distinct temporary accounts before their join actions are queued.
Their 8,188-byte buffers start at zero and use the first installed player schema
without human profile defaults. Script writes and map restarts retain these
records; departure or a new match discards them. Temporary records are excluded
from durable saves. Occupied clients or accounts refuse admission before any
record is imported or initialized.

## Remaining

A live bomb plant/defuse check. Native IW4L menus can create a private lobby,
select a map/mode, start a match and return to the frontend. Browser/public-lobby
actions are connected but still need multiplayer validation. Display mode/resolution, brightness, volume, VSync, shadows, depth of field and
bloom are bound to runtime settings and persisted locally. Surround output, voice/chat
settings, third-person camera and spectator restrictions remain incomplete. A true
dedicated server still runs on the listen runtime.
