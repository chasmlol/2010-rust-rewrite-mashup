# Your own master

`iw4l-master` is a relay and server browser, not a game server: the host's client
simulates the match. It pulls in no engine dependency, so a VPS needs no assets and no GPU. Publishing our own releases: [`DEPLOY.md`](DEPLOY.md).

## Install — from the machine with the clone; the VPS needs only ssh

```bash
cargo xtask master install root@1.2.3.4
cargo xtask master logs    root@1.2.3.4 --since 10min
```

`install` mints a CA and server certificate under `~/.iw4l/ca` (or `--ca DIR`),
builds a static binary, writes the systemd unit and starts the selected service.
Prod uses `/usr/local/lib/iw4l` and `/etc/iw4l`; dev uses
`/usr/local/lib/iw4l-dev` and `/etc/iw4l-dev`. Update files go under the selected
library directory's `updates/<channel>`. Populate that directory before handing
out the descriptor. The CA key never leaves the local machine. `master update`,
`status` and `uninstall` take the same channel; uninstall retains certificates.
Use an independent CA directory for dev:

```bash
cargo xtask master install user@vps --channel dev --ca ~/.iw4l/dev-vpn-ca
```

## TLS identity

The descriptor supplies `master.address`, `master.server_name` and an embedded
CA. The client trusts that CA and verifies the server name independently of the
connection address. Prod uses the fixed label `iw4l-prod`, so its certificate
can follow the installation to a new VPS.

Dev certificates also cover the VPS host; generated dev descriptors use that
host as their TLS identity. For IP targets this retains CA and IP verification
without sending a synthetic DNS label as SNI. This preserves the intended
address through proxies that inspect QUIC names and replace destinations
([Xray sniffing configuration](https://xtls.github.io/config/inbound.html)).


## What it records — nothing on disk

Rooms, the advertised match name, peers and room membership live in `ServiceState`
in memory, gone when the room closes or the process restarts. No account, history,
analytics or telemetry. What survives is the systemd journal —
startup and failures, not matches or players — under the VPS's own `journald`
retention. Peer IPs are visible to the kernel and to any packet capture on that
host while a connection is open, as with any server. Run it for others and that is
the honest description: it forwards packets and vouches for nobody.

## Hand out to players

Give players a trusted `community.iw4l-server` and the self-updating `iw4l.exe`:

```toml
schema = 1
name = "IW4L Community"
[master]
address = "1.2.3.4:4433"
server_name = "iw4l-prod"
[updates]
url = "https://1.2.3.4:4433/updates/manifest.toml"
ca_pem = """
-----BEGIN CERTIFICATE-----
... public iw4l-ca.pem contents ...
-----END CERTIFICATE-----
"""
```

Both TCP and UDP allocations must be open. `serve --updates PATH` selects the
static update directory; its default is `./updates`. Upload the release blob
before atomically replacing `manifest.toml`; see [`DEPLOY.md`](DEPLOY.md).
A community descriptor is required for master networking. Address, TLS name
and CA come only from that file.
On Linux, selecting a descriptor retains the locally built executable and
skips the Windows update flow. `IW4L_COMMUNITY` selects a descriptor next to the
executable (an absolute path is also accepted).

| Setting | What it changes and when to set it |
| --- | --- |
| `IW4L_MASTER_HOST_NAME` | The room name other players see. To host via `map`, set a name that is not empty or only whitespace, and leave `IW4L_MASTER_JOIN` unset. Names can occupy at most 48 UTF-8 bytes. Hosting through the menu uses `iw4l host` if the name is unset. Joining through the menu needs no host name. |
| `IW4L_MASTER_MAX_PLAYERS` | The room capacity, **including the host**. Optional; defaults to `18`. Set an integer from `2` through `18` to limit the room size. It is read only when creating a room, via `map` or the menu; invalid values prevent room creation rather than being clamped. |
| `IW4L_MASTER_PASSWORD` | The room password for command-line hosting and joining. Lobby settings can set, change or remove it; joining a protected room through the menu prompts for it. |

The host can start a match with:

```bash
make map mp_boneyard IW4L_MASTER_HOST_NAME='Friday match' IW4L_MASTER_MAX_PLAYERS=8
```

Other players open `make menu` and select the room, including rooms marked
`IN MATCH`. Joining an ongoing match loads its current map; password, capacity
and installed-content checks still apply. Master networking needs
MW2 multiplayer data; it is disabled during demo replay.

When a setting appears in more than one place:

* **Direct game launch:** existing environment values win. The game loads one
  `.env`: next to its binary first, otherwise the first found from the working
  directory upwards. It fills only unset variables; files are not merged.
* **GNU make recipes (without `-e`):** command-line assignments as above win
  over the repository `.env`, which wins over inherited environment values
  for keys it defines. The Makefile exports these values to the game.
  See GNU make's [environment rules](https://www.gnu.org/software/make/manual/html_node/Environment.html).
* **Community launch:** the selected descriptor supplies master address, TLS
  name and CA. See [`WINDOWS.md`](WINDOWS.md) for selection and updating.
