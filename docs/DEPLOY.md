# Shipping a release

```bash
make setup-windows
make release prod|dev           # local build and packaging
make publish prod|dev           # upload an existing RELEASE=
make deploy prod|dev            # release + publish; PROFILE=play
make provision                  # users, systemd, certificates, firewall
make logs prod|dev SINCE=2h
```

Set `IW4L_DEPLOY_HOST`, `IW4L_DEPLOY_ROOT` and `IW4L_RELEASE_KEY` in `.env`.
The public CA must already exist; release preparation never replaces it.
The master listens on UDP and TCP 4433 for prod, 4434 for dev. HTTPS uses
`/updates/manifest.toml`, independently of the QUIC protocol and ALPN.
Hosting panels must expose both transports on the selected allocation.

`cargo xtask release prod` creates `dist/releases/prod/<id>/` containing
`deployment.json`, `client/`, and `server/`. It also creates a player ZIP and an
unencrypted `iw4l-server-release-<id>.zip`, ready to upload through a panel:

```text
server/
├── iw4l-master
└── updates/
    ├── manifest.toml
    └── iw4l-<executable-sha256>.exe.zst
```

Keep your server certificate/key beside the binary; they are operator-managed.
Start it with `./iw4l-master serve --bind 0.0.0.0:4433 --cert server-cert.pem
--key server-key.pem --updates updates` (one command). `/health` answers over
HTTPS. The process serves static update files with the same certificate as QUIC.

Upload the complete immutable blob first, then replace the manifest last.
Old blobs may remain. Files are read per request: a client-only update needs no
master restart. Master and client identities are independent; changing only the
master can retain the same client manifest; use `cargo xtask master update`
for an independent relay update. `publish` verifies staged hashes,
switches the master only when its hash changed, and atomically renames the
manifest after master health succeeds. It does not rewrite systemd units;
existing installations need the new units from `provision` before publishing.

The player starts the single `iw4l.exe`; its HTTPS check precedes QUIC, so a
breaking protocol update remains downloadable. See [`WINDOWS.md`](WINDOWS.md).
A first adoption requires distributing this executable and its descriptor.

Public releases are pre-releases tagged `v0.1.0-demo.N`. Notes identify the
commit, actually exercised platforms, scenario and limitations. Player archives
carry LICENSE, NOTICE and both font licences, never game data or private keys.
