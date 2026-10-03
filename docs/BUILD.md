# Build dependencies

Rust stable via rustup (`rust-toolchain.toml` pins it), plus a C toolchain
and the Bevy system libs. IW4L enables both `x11` and `wayland`
(`Cargo.toml`), so the Wayland headers stay required. Gamepad input and
rumble (`bevy_gilrs`) link libudev, so the udev headers (`systemd-devel` /
`libudev-dev`) are required too.

```bash
# Fedora
sudo dnf install gcc-c++ pkgconf-pkg-config libX11-devel alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel

# Debian / Ubuntu
sudo apt install g++ pkg-config libx11-dev libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev

# Arch / Manjaro
sudo pacman -S gcc pkgconf libx11 alsa-lib systemd wayland libxkbcommon libxcursor libxrandr libxi
```

Upstream list (other distros, GPU Vulkan drivers): Bevy
[`linux_dependencies.md`](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md).

What is **not** needed for a native Linux build: `clang` / `llvm` / `lld` /
`glibc-static` — plain `gcc-c++` links it. `llvm` (the `llvm-lib` tool) is
only required for the Windows cross build:

```bash
sudo dnf install llvm        # Fedora: provides llvm-lib
make setup-windows           # rustup target + cargo-xwin, once
```

macOS needs only the Xcode command line tools (`xcode-select --install`)
and rustup: winit, wgpu (Metal), CoreAudio and gilrs link system frameworks,
and the `x11` / `wayland` features compile to nothing. Pipelined rendering
defaults to on, requesting a maximum frame latency of two on macOS.

For native macOS or Linux play, use your own MW2 (2009) Windows multiplayer
files: a folder containing `iw4mp.exe`, `zone` and `main`, copied from your
installation, or downloaded with SteamCMD's Windows platform override:
`steamcmd +@sSteamCmdForcePlatformType windows +force_install_dir ~/Games/MW2 +login <user> +app_update 10190 +quit`. The game uses those files; it does
not run the Windows executable. Then, from this checkout:

```bash
IW4L_GAMES="/absolute/path/to/MW2" cargo run --profile play -p launcher -- menu
```

For repeated runs, copy `.env.example` to `.env`, set `IW4L_GAMES` to the
absolute path, then use `make menu` or `make map mp_boneyard`. Skate 3 is optional:
set `IW4L_SKATE_ASSETS` to your converted `skate-data/assets` folder
to generate missing `rig.json` and `board.json` on launch. Windows: [`SKATE.md`](SKATE.md), [`WINDOWS.md`](WINDOWS.md).
