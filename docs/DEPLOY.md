# Shipping a release

What `make deploy` does, what reaches the Windows folder with no human involved,
and what does not. That folder is [`WINDOWS.md`](WINDOWS.md); your own master
rather than ours is [`MASTER.md`](MASTER.md).

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
`provision`, `logs`, `certs` — and `make` forwards only `PROFILE` and `RELEASE`;
`cargo xtask` alone lists them. `prod`/`dev` is the publication channel, not the
compilation profile; address and root are `IW4L_DEPLOY_HOST` /
`IW4L_DEPLOY_ROOT` in `.env`. prod: udp/4433 and `https://<host>:8443/prod/`;
dev: 4434 and `/dev/`. Git, protocol and SHA freeze into `deployment.json` at
`release`, not at `publish`.

An ordinary publish compiles nothing, calls no zstd, rewrites nothing under
`/etc` and leaves the master running if its SHA matched. The uncompressed
`iw4l.exe` stays local, the archive is `iw4l-<sha>.exe.zst`, the old one on the
VPS survives, and the manifest is swapped in by rename.

Changing `master_protocol::PROTOCOL_VERSION` also changes `ALPN`: an old client
is refused at the handshake instead of silently decoding a drifted format. The
master and the clients of such a release ship together, and `publish` restarts
the master because its SHA changed.

`make launcher windows` builds the portable ZIP in `dist/windows/`; it is not a
deploy, and a fix to the updater arrives only by copying `iw4launcher.exe` by
hand. What the player's launcher does with `IW4L_UPDATE_URL`: [`WINDOWS.md`](WINDOWS.md).

**Cutting a public release.** Tag `v0.1.0-demo.N`, titled `IW4L Technical
Demo N`, and mark it a **pre-release**. The notes carry four things and are not worth writing without
them: the commit it was built from; the platforms the archive was actually
**run** on; the demo scenario (which game, version and map, and what to do); the
known limitations. A target that merely compiled is not a verified platform, and
a Linux release waits for no unstarted Windows archive, nor the reverse.

Every archive carries `LICENSE`, `NOTICE` and the two font licences
(`release.rs::LEGAL_FILES`) and no game data. Check the archive, not just
`git ls-files`: `make publish-check` reads the tracked tree and cannot see what
was staged into a ZIP. Published tags are never moved, and history is never
recreated once anything has been pushed.
