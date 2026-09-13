# Shipping a release

Players update themselves — but not entirely. What `make deploy` does, what
reaches the Windows folder with no human involved, and what does not. The
contents of that folder are [`WINDOWS.md`](WINDOWS.md). Your own master rather
than ours is [`MASTER.md`](MASTER.md).

```bash
make setup-windows              # rustup target + cargo-xwin, once
make release prod|dev           # local directory, VPS untouched
make publish prod|dev           # upload a finished RELEASE=
make deploy prod|dev            # release + publish; PROFILE=play
make deploy prod PROFILE=release
make provision                  # users, Caddy, systemd, certificates
make logs prod|dev SINCE=2h
```

Each recipe is one `cargo xtask` command — `windows`, `release`, `publish`,
`provision`, `logs`, `certs` — and `make` only forwards `PROFILE` and
`RELEASE`. `cargo xtask` on its own lists them.

`prod`/`dev` is the publication channel, not the compilation profile. Address
and root are `IW4L_DEPLOY_HOST` / `IW4L_DEPLOY_ROOT` in `.env`. prod: udp/4433
and `https://<host>:8443/prod/`; dev: 4434 and `/dev/`. Git, protocol and SHA
are frozen into `deployment.json` at `release`, not at `publish`.

An ordinary publish compiles nothing, calls no zstd, rewrites nothing under
`/etc` and does not restart the master if its SHA matched. The uncompressed
`iw4l.exe` stays local. The archive is named `iw4l-<sha>.exe.zst`; the old
`iw4l.exe.zst` on the VPS is not deleted. The manifest is swapped in by rename.

`iw4launcher.exe update` reads `IW4L_UPDATE_URL` from the sidecar `.env`,
fetches `manifest.json`, checks size/sha256 and downloads `download`. Without
that verb it starts the `iw4l.exe` already in the folder and makes no request at
all — the archive ships one, so updating is a thing the player asks for rather
than a thing that happens to them. The launcher never updates itself. The
player's `.env` is created once and never overwritten.

Changing `master_protocol::PROTOCOL_VERSION` is also a change of `ALPN`: an old
client is refused at the handshake instead of silently decoding a drifted
format. The master and the clients of such a release ship together; `publish`
restarts the master, because its SHA changed.

`make launcher windows` builds the portable ZIP in `dist/windows/`; it is not a
deploy. A fix to the updater arrives only by copying `iw4launcher.exe` by hand.

## Cutting a public release

Tag `v0.1.0-demo.N`, titled `IW4L Technical Demo N`, and mark it a
**pre-release** on GitHub — the runtime is experimental and nothing about it is
production-ready. The release notes carry four things and are not worth writing
without them:

* the commit it was built from;
* the platforms the archive was actually **run** on. A target that compiled is
  not a verified platform, and a Linux release does not wait for a Windows
  archive that has not been started yet — nor the other way round;
* the demo scenario: which game, which version, which map, what to do;
* the known limitations.

Every archive carries `LICENSE`, `NOTICE` and the two font licences
(`release.rs::LEGAL_FILES`) and no game data. Check the archive, not just
`git ls-files`: `make publish-check` reads the tracked tree and cannot see what
was staged into a ZIP.

The published tags are not moved afterwards, and the history is not recreated
again once anything has been pushed.
