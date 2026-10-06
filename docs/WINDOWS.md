# Portable Windows folder

Players download `iw4l-windows.zip` from the GitHub release; it holds the executable.
A server operator hands out the `.iw4l-server` descriptor separately.
`make release prod|dev` includes the descriptor; `make launcher windows` writes the pair under `dist/windows/`.
The archive password is `t.me/contextrot`. Extract into a dedicated writable folder:

```text
IW4L/
├── iw4l.exe
├── Modern Warfare 2.lnk
├── Black Ops.lnk            optional
├── Modern Warfare 3.lnk     optional
├── community.iw4l-server    optional
└── iw4l-artifacts/          created on first launch: saves, caches, demos, logs
```

`iw4l.exe licenses` prints the licence and notice texts compiled into the executable.
The runtime reads the installations those shortcuts point to. MW2 multiplayer data
is required for the menu; BO1 and MW3 are optional. For each missing title,
`iw4l.exe` checks the Steam libraries and creates its shortcut when exactly one
install has supported multiplayer data. Existing shortcuts and titles already
found in the configured folders are preserved. If several installs qualify,
create a shortcut to the one you want. If MW2 is still missing, the launcher shows
the folders it tried and how to add the shortcut: right-click inside the launcher
folder, New > Shortcut, paste the game folder path.

Launch `iw4l.exe`. Before starting the game or contacting QUIC, it checks its
community's HTTPS manifest. An unchanged executable starts normally. An update
is downloaded, decompressed with a size limit and verified against SHA-256.
A temporary copy of the same executable waits for the original process, replaces
it and restarts with the original arguments. Failed installation or spawning
restores the previous executable. There is no permanent second executable.

The temporary files are `iw4l.update.exe`, `iw4l.previous.exe` and a helper in
`%TEMP%`; the next verified startup removes the backup and helper. The lock file
`iw4l.update.lock` serializes update checks; `iw4l.update-helper` records cleanup.
An update error stops startup and reports the failure; a configured community requires a reachable update origin.
Without a community descriptor, local development can launch without checking.

A descriptor pins a public CA and the master's TLS name for both HTTPS and QUIC.
Use a descriptor from a trusted source: its operator can distribute executable
updates. Its settings take precedence over the legacy master environment keys.
One adjacent `.iw4l-server` is selected automatically. With several, choose under
Options → Community Servers → Apply & Restart; the choice is saved beside the
executable in `iw4l.community.json`. With no choice, the menu opens offline; `IW4L_COMMUNITY` overrides the saved choice.
Invalid adjacent descriptors are skipped. See [`MASTER.md`](MASTER.md).

On Windows, the executable directory is the working directory; `.env` and
`IW4L_GAMES` can override game discovery. Publishing: [`DEPLOY.md`](DEPLOY.md).
