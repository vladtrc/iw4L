# Your own master

`iw4l-master` is a relay and a server browser, not a game server: the host's
client simulates the match. No dedicated server ships in this repository. The
crate pulls in no engine dependency, so a VPS needs neither assets nor a GPU.
Publishing *our* releases is [`DEPLOY.md`](DEPLOY.md).

## Install

Everything runs from the machine holding the clone; the VPS needs only ssh.

```bash
cargo xtask master install root@1.2.3.4
cargo xtask master logs    root@1.2.3.4 --since 10min
```

`install` mints a CA and a server certificate under `~/.iw4l/ca`, builds the
static binary (`crt-static`, so the VPS glibc version does not matter), places
it in `/usr/local/lib/iw4l/` and the certificates in `/etc/iw4l/` (key
`0640 root:iw4l`), writes the unit through `iw4l-master print-unit`, opens the
channel port and runs `enable --now`. The CA private key never leaves the
local machine.

The binary prints its **own** unit, so `ExecStart` cannot drift from what
`serve` parses; port and unit name come from `master_protocol::Channel`. Then
`master update` (rebuild, upload, restart), `status`, `uninstall`. Every verb
takes `--channel dev` for the second port and `--ca DIR` for another CA;
`uninstall` keeps both certificates and CA, so a reinstall stays trusted by
everyone who already has your `iw4l-ca.pem`.

## No domain required

The client connects by address but verifies the certificate against a
**separate** name, and with `IW4L_MASTER_CA_CERT` set it puts only that PEM
into an empty `RootCertStore` (`net/src/transport/master.rs:3107`, `:3150`) —
the platform verifier is bypassed entirely.

| path | certificate checked against | host in SAN |
|---|---|---|
| client → master, QUIC | `IW4L_MASTER_SERVER_NAME`, a fixed label | not needed |
| updater → Caddy, HTTPS | host from `IW4L_UPDATE_URL` (`updater:267`) | needed |

`San::WithHost` / `cert_covers_host` in `xtask/src/certs.rs` serve the second
row, our CDN. Your own master runs no Caddy, so `install` signs `San::Labels`
alone and nothing in the SAN depends on the host: the certificate is minted
**once per user, not per server** — a new IP or a new VPS keeps the same one.

## What it records

Nothing on disk. `iw4l-master` opens no file, no database and no log of its own:
the state it holds — rooms, the advertised match name the host typed, peers and
who is in which room — lives in `ServiceState` in memory and is gone when the
room closes or the process restarts. There is no account, no history and no
analytics; the runtime sends no telemetry to it.

What survives is the systemd journal, and only what the process writes to
stderr — startup and failures, not matches or players. Its retention is the
VPS's `journald` configuration, not ours; `cargo xtask master logs --since` just
reads it. The transport is QUIC, so peer IP addresses are visible to the kernel
and to any packet capture on that host for as long as a connection is open, the
same as for any server you connect to.

If you run the relay for other people, that is the honest description to give
them: it forwards packets between clients who are already talking to each other,
and it vouches for nobody.

## Hand out to players

`install` prints the block below; give it and `iw4l-ca.pem` to your friends:

```dotenv
IW4L_MASTER_ADDR=1.2.3.4:4433
IW4L_MASTER_SERVER_NAME=iw4l-prod
IW4L_MASTER_CA_CERT=/path/to/iw4l-ca.pem
```

The host opens a match with `IW4L_MASTER_HOST_NAME='name' make map mp_boneyard`;
everyone else joins from `make menu`, 2..=18 (`IW4L_MASTER_MAX_PLAYERS`). The
label `iw4l-prod` is identical for everyone, so trust rests entirely on pinning
`ca.pem` — hand it over a channel the players trust.
