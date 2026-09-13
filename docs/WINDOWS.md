# Portable Windows folder

`make launcher windows` creates `dist/windows/iw4l-windows-dev.zip` and
`dist/windows/iw4l-windows-prod.zip`. Both archives use the password
`contextrot` and are self-contained — the game binary is in the archive, so the
folder runs with no network at all:

```text
.env
iw4l-ca.pem
iw4launcher.exe
iw4l.exe
LICENSE
NOTICE
OFL-Oxanium.txt
COPYING-FreeFont.txt
```

The four licence files are not decoration: `iw4l.exe` has both fonts compiled
into it with `include_bytes!`, so an archive carrying the binary carries their
licences too. `release.rs::LEGAL_FILES` is the list, and a build that ships
without them is the bug.

Extract the selected archive into any dedicated folder and add ordinary Windows
`.lnk` shortcuts to installed Call of Duty title folders or title executables.
MW2 **Multiplayer** files are required for the menu (`zone/**/common_mp.ff`);
a folder containing only `zone/dlc/*.ff` is insufficient. BO1 and MW3 are optional.
Run the launcher; it starts the game without modifying those installations.

**Updating is something you ask for.** Plain `iw4launcher.exe` starts the
`iw4l.exe` that is already in the folder and touches nothing else. Fetching a
newer build is `iw4launcher.exe update` (or `IW4L_UPDATE=1` for a shortcut that
cannot pass an argument), which needs `IW4L_UPDATE_URL` in the portable `.env`.
No build replaces its own binary on its own. How a release gets published is
[`DEPLOY.md`](DEPLOY.md).

Steam shipped MW2 and MW3 re-releases whose FastFiles are serialized with
64-bit pointers. MW2 is read in both serializations; **MW3 x64 zones are
detected and refused**, so an MW3 shortcut to such an install contributes no
weapons and no maps, and says so in the log
(`IW5 zone is serialized x64 (Steam re-release)`).

```text
iw4l-portable/
├── iw4launcher.exe       from the archive
├── iw4l.exe              from the archive; replaced only by `iw4launcher update`
├── iw4l-ca.pem           public trust anchor
├── LICENSE NOTICE …      the four licence files, kept with the binary
├── .env                  from the archive; player edits win forever
├── Modern Warfare 2.lnk  target: game folder or title .exe
├── Black Ops.lnk         optional
├── Modern Warfare 3.lnk  optional
└── iw4l-artifacts/       saves, caches, demos, logs and captures
```

The folder containing the two executables is the process working directory even
when Explorer supplies another one. `IW4L_GAMES` defaults to this folder on
Windows; `.lnk` targets directly inside it are additional read-only search
roots. An explicit `IW4L_GAMES` still works and may itself contain shortcuts.

`.env` is loaded before release-manifest defaults. The launcher creates it only
when missing and never overwrites it. `IW4L_UPDATE_URL` is read only by
`iw4launcher.exe update`, never by a plain start; the same `iw4launcher.exe`
serves both channels.

```dotenv
IW4L_UPDATE_URL=https://host:8443/prod
IW4L_MASTER_ADDR=master.example.net:4433
IW4L_MASTER_SERVER_NAME=iw4l-prod
IW4L_MASTER_CA_CERT=iw4l-ca.pem
```

An unreachable master is not a launch error: the browser keeps retrying and
shows the connection error. All writable game state stays below
`iw4l-artifacts/` (including Windows `settings.cfg`, unless
`IW4L_SETTINGS_PATH` overrides it); shortcut targets are never output paths.

Startup errors print the detected FastFile counts, missing base assets and log
path in the console. On Windows, an interactive console waits for Enter before
closing; redirected input exits immediately with code 2. No extra dialog opens.
