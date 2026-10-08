# Windows keyboard support

Source for the numpad controller mapper, physical keypad detection, live configuration reload, and direct-to-Rust launcher. The native **Options > Skate Controls** page is in `crates/ui/menus/skate.json`; its persistence and mapper context are in `crates/console/src/skate_controls.rs`.

The mapper keeps the ordinary keyboard available for PC input. The default skate toggle is Num Lock. `mapping.default.ini` contains the defaults from the current Rust source, including the Num Enter keypad layer. The game writes user settings to `keyboard/mapping.ini`, and the mapper reloads them without reconnecting its virtual controller.

## Build

Build the game from the repository root using the existing build instructions. For a native Windows build, the command used locally is:

```powershell
cargo build --profile play -p launcher --locked
```

The Windows mapper uses the .NET Framework compiler bundled with Windows. Supply a local directory containing the three dependency DLLs from the upstream [Keyboard2Xinput](https://github.com/RDCH106/Keyboard2Xinput) distribution:

| Dependency | Assembly version used locally |
| --- | --- |
| INIFileParser.dll | 2.5.2.0 |
| log4net.dll | 2.0.8.0 |
| Nefarius.ViGEmClient.dll | 1.15.16.0 |

```powershell
.\tools\keyboard\build.ps1 -DependencyDirectory 'C:\path\to\Keyboard2Xinput'
```

The output is `tools/keyboard/bin/`, which Git ignores. Copy its contents beside the built `iw4l.exe`, preserving any existing `keyboard/mapping.ini`. The game also needs its normal runtime files and locally configured game asset paths. The signed [ViGEmBus driver](https://github.com/nefarius/ViGEmBus) must already be installed; this script does not install drivers.

Run `Play Rust.cmd` to start the mapper, load `mp_rust`, and spawn automatically. Configure the skating keys through **Esc > Options > Skate Controls**.

Click a binding, then press its numpad key. Hold Num Enter while pressing a key to assign the alternate layer. Escape cancels and Backspace clears a binding; the board toggle must retain a key. Assigning an occupied key swaps the two bindings. Changes save automatically. Push defaults to numpad 0, Jump/Ollie to numpad Del (hold, then release), and Alternate Push is unassigned. Ordinary number-row keys remain available to MW2.

## Source and licensing

`source/Keyboard2Xinput.cs`, `Config.cs`, `StateListener.cs`, `AssemblyInfo.cs`, and `ViGEmBusNotFoundException.cs` are adapted from Keyboard2Xinput. Its MIT license and copyright notice are retained in `source/LICENSE.txt`. The local host and launcher are provided with these sources under the same MIT license. The rest of the project retains its existing licenses.

Dependency DLLs, drivers, and game assets are not included here. Retain the dependencies' own license notices if preparing a binary distribution.
