# Skate 3 in MW2

Skate 3 skating inside [IW4L](https://github.com/vladtrc/iw4L), the from-scratch
Rust Modern Warfare 2 runtime. Walk around any MW2 map as a soldier, press
**J**, and drop onto a board driven by Skate 3's own physics, tricks and grinds.

No game files ship with this repository or its releases. You bring your own
MW2 and Skate 3.

## What you need

- **Call of Duty: Modern Warfare 2** (2009, PC) with its multiplayer files. The
  Steam version works.
- **Skate 3 for Xbox 360, extracted**: its `default.xex` with the game's
  `data` folder beside it.
- An **Xbox / XInput controller** to skate.

### Where does `default.xex` come from?

`default.xex` is the game's executable in the root of the Skate 3 disc, next
to `data`. It never comes on its own. You get it by extracting your own copy:

- **ISO of your disc**: run
  [extract-xiso](https://github.com/XboxDev/extract-xiso)
  (`extract-xiso -x "Skate 3.iso"`) and use the folder it creates.
- **Games on Demand / Marketplace copy** from a 360 hard drive: open the
  container with a tool such as Velocity and extract it.

Either way you end up with a folder like this. Select its `default.xex`:

```text
Skate 3/
├── default.xex
├── data/
└── ...
```

**ISO files themselves do not work.** Only the extracted `default.xex`.

## Install and play (Windows)

1. Download `IW4L-Skate-windows-x64.zip` from
   [Releases](../../../releases/latest). Extract it into its own folder,
   somewhere you can write to (not Program Files).
2. Double-click `iw4l.exe`.
3. It looks for MW2 in your Steam libraries and asks you to confirm. If it
   can't find it, or you say no, select the MW2 folder yourself (the one
   with `iw4mp.exe` and `zone`).
4. Select your Skate 3 `default.xex`. Setup extracts the skater/gameplay files
   and decodes the Create-a-Park dynamic prop meshes and embedded retail
   collision into `skate-data/`. Your game folders are never modified.
5. The menu opens. Every later double-click goes straight to the menu.

The prop files are under `skate-data/assets/private/park-props`: `catalog.json`
indexes runtime mesh and collision files; the accompanying `.npz` files are
available for inspection. Older converted data without this runtime catalog
format is detected and setup asks for `default.xex` again to refresh it.

Both folders are saved in `.env` next to `iw4l.exe`. Delete `.env` (and
`skate-data/`) to run the setup again.

## Controls

| | |
|---|---|
| **J** | get on / off the board |
| **F6** or **Shift + Down** / **LB + D-pad down** | set the session marker |
| **Hold F7** or **hold Shift + Up** / **hold LB + D-pad up** | return to the session marker |
| **Tab** / **Back (Select)** while skating | open or close the object dropper |
| **Up / Down** or **D-pad up / down** in the dropper | browse props |
| **Left / Right** or **D-pad left / right** in the dropper | rotate the selected prop |
| **Enter / A** in the dropper | place the prop in front of the skater |
| **Backspace / X** in the dropper | delete the last placed prop |
| **Escape / Back (Select)** in the dropper | close the dropper |
| controller | skate (Skate 3 flick-it controls) |
| `~` | console: `skate on`, `skate off`, `skate status` |

## Known issues

- Dying while in skate mode leaves bodies piled up.
- The skateboard is invisible on some maps.
- Dropper placements are session-only and disappear when leaving Skate mode.
  Prop meshes currently use simple flat colors; extracted material textures
  are not yet applied.
- Props spawn a fixed distance in front of the skater; placement does not yet
  raycast against the map to snap to the ground or other surfaces.

## How it works

| piece | where | what |
|---|---|---|
| skate simulation | [`skate/crates`](../skate/crates) | Skate 3 physics, animation graph and grind code from [SK8-ENGINE/skate-3-rust-engine](https://github.com/SK8-ENGINE/skate-3-rust-engine) at `cb79689`, run as a worker the soldier hands control to |
| skate mode | [`crates/render_anim/src/skate.rs`](../crates/render_anim/src/skate.rs) | toggling, feeding MW2 map collision to the skate world, posing the soldier from the skater |
| grind rails | [`crates/render_anim/src/skate/rails.rs`](../crates/render_anim/src/skate/rails.rs) | finds grindable lips in any map's collision: walkable edges where the ground drops away and nothing rises, merged and chained into rails |
| retarget | [`crates/render_anim/src/skate/rig.rs`](../crates/render_anim/src/skate/rig.rs) | maps Skate 3 bones onto the MW2 soldier skeleton |
| board | [`crates/assets/src/skate_board.rs`](../crates/assets/src/skate_board.rs) | derives `board.json` and `rig.json` from the converted skater model on first run |
| setup | [`crates/launcher/src/first_run.rs`](../crates/launcher/src/first_run.rs) | the double-click flow: find MW2, pick `default.xex`, run the converter, write `.env` |
| converter | [`skate/converter`](../skate/converter) | `iw4l-skate-convert.exe`: converts the player's own `default.xex`, skater assets, and local Create-a-Park model/collision catalog |

## Building

Build IW4L as usual ([`BUILD.md`](BUILD.md), [`WINDOWS.md`](WINDOWS.md)). The
skate crates are plain path dependencies.

A release folder is:

```text
IW4L-Skate/
├── iw4l.exe                     cargo build --release -p launcher
├── skate/iw4l-skate-convert.exe skate/converter/build.ps1
├── skate/licenses/              Python, NumPy, Pillow, UTT, Custom Engine Layer
└── LICENSE NOTICE OFL-Oxanium.txt COPYING-FreeFont.txt
```

`skate/converter/build.ps1 -SkateEngine <checkout> -Out <folder>` packages the
converter from a skate-3-rust-engine checkout at `cb79689`. It needs Python
3.13 with `pyinstaller`, `numpy` and `Pillow`, plus `rustc`.

Unofficial fan project, not affiliated with Activision, Infinity Ward or EA.
