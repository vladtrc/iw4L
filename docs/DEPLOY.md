# Shipping a release

```bash
make setup-windows
make release                    # local build and packaging; VPS untouched
make publish [NAME...]          # upload the prepared package
make deploy [NAME...]           # release + publish that exact package
make github-release TAG=v0.1.0-demo.N NOTES=notes.md
make logs NAME SINCE=2h
```

## Servers are the root's descriptors

The root folder (`IW4L_GAMES`) is the folder the game reads. Every
`.iw4l-server` file in it is a server, and `NAME` is its file name without the
extension. Address, TLS name, update URL and CA are read from that file. Each
release ships every one of them byte for byte, so editing or adding a
descriptor and running `make deploy` is how players receive a new address, name
or server. Descriptors and the addresses in them stay outside the repository;
`make publish-check` refuses a tracked `.iw4l-server` file or a public IPv4
literal.

`cargo xtask master install` creates a master and writes its descriptor into
the root (see [`MASTER.md`](MASTER.md)). Publishing reaches `root@<host>` from
the descriptor's address and uses the systemd unit whose `serve --bind` listens
on its port. HTTPS serves `/updates/manifest.toml` independently of the QUIC
protocol and ALPN.

## Packages

`make release` builds the Windows executable and the static master once, then
writes `dist/releases/<id>/`: `deployment.json`, `client/` and `server/`,
`iw4l-windows-community.zip` with every descriptor, the public
`iw4l-windows.zip` without them, and an unencrypted
`iw4l-server-release-<id>.zip` for hosting panels:

```text
server/
├── iw4l-master
└── updates/
    ├── manifest.toml
    └── iw4l-<executable-sha256>.exe.zst
```

A panel install keeps its certificate/key beside the binary and starts
`./iw4l-master serve --bind 0.0.0.0:4433 --cert server-cert.pem --key
server-key.pem --updates updates`.

## Publishing

`publish` takes `RELEASE=` or `dist/releases/LATEST`, and publishes to the
descriptors inside that package: all of them, or the named ones. Every server
must answer over ssh and have a unit before anything uploads. The unit's
`ExecStart` names the binary, its directory (which holds `masters/<sha>/`,
archived `manifests/` and staging), the certificate directory and the served
updates directory. Publish refuses a server whose installed CA differs from
its descriptor's.

Uploads are inventoried by sha256 and staged, then promoted. The master is
replaced and restarted only when its hash changed, and the previous binary is
restored if the new one does not report the release protocol. The manifest is
switched last; the one it replaces is kept as `manifests/replaced-<sha>.toml`.
Then the manifest and the full blob are fetched over HTTPS from the
descriptor's address under its TLS name and CA; a mismatch puts the previous
manifest back.

Servers are published one after another, each to completion. A failure prints
the exact `make publish … RELEASE=…` line for the servers that remain;
rerunning skips files already in place and leaves an unchanged master running.

## GitHub

`make github-release` takes the package (`RELEASE=` or `LATEST`), requires a
clean build whose commit is `origin/main`, uploads `iw4l-windows.zip` to a
draft pre-release on that commit, compares GitHub's asset digest with the local
file, then makes it public. Rerunning resumes a draft.

Public releases are pre-releases tagged `v0.1.0-demo.N`. Notes identify the
commit, actually exercised platforms, scenario and limitations. Player archives
carry the executable, whose legal notices are embedded; never game data or
private keys.

The player starts the single `iw4l.exe`; its HTTPS check precedes QUIC, so a
breaking protocol update remains downloadable. See [`WINDOWS.md`](WINDOWS.md).
