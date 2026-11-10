# Your own master

`iw4l-master` is a relay and server browser, not a game server: the host's client
simulates the match. It pulls in no engine dependency, so a VPS needs no assets and no GPU. Publishing our own releases: [`DEPLOY.md`](DEPLOY.md).

## Install — from the machine with the clone; the VPS needs only ssh

```bash
cargo xtask master install root@203.0.113.7 --name us-ny --port 4433
cargo xtask master logs us-ny --since 10min
```

`install` mints a CA and server certificate under `~/.iw4l/<name>` (or
`--ca DIR`), builds a static binary, writes `iw4l-master-<name>.service` and
starts it. Binary and served `updates/` live in `/usr/local/lib/iw4l-<name>`,
certificates in `/etc/iw4l-<name>`. It writes `<name>.iw4l-server` into the
root folder (`IW4L_GAMES`), where the game lists it and `make deploy <name>`
publishes to it ([`DEPLOY.md`](DEPLOY.md)); an existing file there is kept. The
CA key never leaves the local machine.

`master update|status|logs|uninstall <name>` read that root descriptor and act
on whichever unit serves its port. `update` restarts only when the binary
changed; uninstall retains certificates and the descriptor.

The core `community` crate owns descriptor validation, certificate trust, and the immutable selected-community context shared by networking and UI. Desktop startup discovers descriptors and publishes that context; desktop also supplies the platform restart callback. Core validates restart selection against the published descriptor list before invoking it.

## TLS identity

The descriptor supplies `master.address`, `master.server_name` and an embedded
CA. The client trusts that CA and verifies the server name independently of the
connection address. Install uses the fixed label `iw4l-<name>`, so the
certificate can follow the installation to a new VPS. The certificate also
covers the VPS host; a descriptor may use that host as its TLS name instead.
For IP targets this retains CA and IP verification without sending a synthetic
DNS label as SNI, which preserves the intended address through proxies that
inspect QUIC names and replace destinations
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
address = "203.0.113.7:4433"
server_name = "iw4l-us-ny"
[updates]
url = "https://203.0.113.7:4433/updates/manifest.toml"
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
skips the Windows update flow. Descriptors are the `.iw4l-server` files in the
games root; the selected one is saved in `iw4l-artifacts/community.json`.

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

## Family content versions

Families ship together in one builder release. The builder version identifies
the content version; a family change that changes a match descriptor ships as a
new builder build through the updater. Lobby `ContentFamilies` advertises family
names for availability, not independent content versions. Signon compares the
map/weapon/class content hashes in `MatchDescriptor` and refuses mismatches.
A room may therefore be listed before an incompatible build is refused at
connect. Revisit per-family digests and earlier lobby diagnostics when a game
repo first ships independently.

## Browser pagination

Master protocol 12 limits each room-list page by both the 16 KiB control-frame
budget and 128 advertisements. `ListRooms.after` starts after the supplied
`AdvertId`; `RoomList.next` supplies the next cursor, or is absent on the last
page. Both the game browser and `iw4l-master list` collect every page.

Advertisements are ordered by ID. Pages describe the live room list rather
than a frozen snapshot: rooms inserted before the cursor appear on the next
refresh, and closed rooms disappear immediately. The response generation
identifies the service state used to build each page.
