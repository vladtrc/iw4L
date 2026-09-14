# Portable Windows folder

`make launcher windows` creates `dist/windows/iw4l-windows-{dev,prod}.zip`. Both
archives use the password `contextrot` and are self-contained — the game binary
is inside, so the folder runs with no network at all.

```text
iw4l-portable/
├── iw4launcher.exe       from the archive
├── iw4l.exe              from the archive; replaced only by `iw4launcher update`
├── iw4l-ca.pem           public trust anchor
├── LICENSE NOTICE OFL-Oxanium.txt COPYING-FreeFont.txt
├── .env                  from the archive; player edits win forever
├── Modern Warfare 2.lnk  target: game folder or title .exe
├── Black Ops.lnk         optional
├── Modern Warfare 3.lnk  optional
└── iw4l-artifacts/       saves, caches, demos, logs and captures
```

The four licence files are not decoration: `iw4l.exe` has both fonts compiled in
with `include_bytes!`, so an archive carrying the binary carries their licences
too. `release.rs::LEGAL_FILES` is the list; a build shipping without them is a
bug.

Extract into any dedicated folder and add ordinary Windows `.lnk` shortcuts to
installed Call of Duty title folders or executables. MW2 **Multiplayer** files
are required for the menu (`zone/**/common_mp.ff`); `zone/dlc/*.ff` alone is
insufficient. BO1 and MW3 are optional. The launcher starts the game without
modifying those installations.

**Updating is something you ask for.** Plain `iw4launcher.exe` starts the
`iw4l.exe` already in the folder and touches nothing else. Fetching a newer
build is `iw4launcher.exe update` (or `IW4L_UPDATE=1` for a shortcut that cannot
pass an argument), which needs `IW4L_UPDATE_URL` in the portable `.env`. No
build replaces its own binary on its own. Publishing is [`DEPLOY.md`](DEPLOY.md).

Steam shipped MW2 and MW3 re-releases whose FastFiles are serialized with 64-bit
pointers. MW2 is read in both serializations; **MW3 x64 zones are detected and
refused**, so an MW3 shortcut to such an install contributes no weapons and no
maps, and says so in the log (`IW5 zone is serialized x64 (Steam re-release)`).

The folder holding the two executables is the process working directory even
when Explorer supplies another; `IW4L_GAMES` defaults to it, and `.lnk` targets
inside it are additional read-only search roots. `.env` is loaded before
release-manifest defaults, created only when missing and never overwritten.

`.env` holds `IW4L_UPDATE_URL=https://host:8443/prod` plus the master keys
from [`MASTER.md`](MASTER.md). An unreachable master is not a launch error: the browser keeps retrying. All
writable state stays below `iw4l-artifacts/` (including `settings.cfg`, unless
`IW4L_SETTINGS_PATH` overrides it); shortcut targets are never output paths.
