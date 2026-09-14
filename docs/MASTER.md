# Your own master

`iw4l-master` is a relay and server browser, not a game server: the host's client
simulates the match. It pulls in no engine dependency, so a VPS needs no assets
and no GPU. Publishing our own releases: [`DEPLOY.md`](DEPLOY.md).

## Install — from the machine with the clone; the VPS needs only ssh

```bash
cargo xtask master install root@1.2.3.4
cargo xtask master logs    root@1.2.3.4 --since 10min
```

`install` mints a CA and server certificate under `~/.iw4l/ca`, builds a static
binary, installs it with the certificates under `/usr/local/lib/iw4l/` and
`/etc/iw4l/`, writes the unit through `iw4l-master print-unit` and runs
`enable --now`. The CA key never leaves the local machine. `master update`,
`status` and `uninstall` follow, each taking `--channel dev` and `--ca DIR`;
`uninstall` keeps the certificates and CA, so a reinstall stays trusted.

## No domain required — the certificate is not tied to one

The client connects by address but checks the certificate against a **separate**
name — `IW4L_MASTER_SERVER_NAME`, a fixed label — so nothing in the SAN depends on
the host; with `IW4L_MASTER_CA_CERT` set it loads only that PEM into an empty
`RootCertStore` (`net/src/transport/master.rs:3107`), bypassing the platform
verifier. `install` signs `San::Labels` alone: the certificate is minted **once
per user, not per server**, and a new IP or VPS keeps it.

## What it records — nothing on disk

Rooms, the advertised match name, peers and room membership live in `ServiceState`
in memory, gone when the room closes or the process restarts. No account, history,
analytics or telemetry. What survives is the systemd journal —
startup and failures, not matches or players — under the VPS's own `journald`
retention. Peer IPs are visible to the kernel and to any packet capture on that
host while a connection is open, as with any server. Run it for others and that is
the honest description: it forwards packets and vouches for nobody.

## Hand out to players — this block plus `iw4l-ca.pem`

The host opens a match with `IW4L_MASTER_HOST_NAME='name' make map mp_boneyard`,
the rest join from `make menu`, 2..=18 (`IW4L_MASTER_MAX_PLAYERS`). Trust rests
entirely on pinning `ca.pem`, so hand it over a trusted channel.

```dotenv
IW4L_MASTER_ADDR=1.2.3.4:4433
IW4L_MASTER_SERVER_NAME=iw4l-prod
IW4L_MASTER_CA_CERT=/path/to/iw4l-ca.pem
```
