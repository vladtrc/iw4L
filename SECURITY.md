# Reporting a security issue

Use GitHub's **Report a vulnerability** button under the repository's Security
tab. It opens a private advisory that only the maintainer can see. Please use it
instead of a public issue when the report contains exploitation detail.

This is a channel, not a program: there is no bounty, no triage SLA and no
promised fix date. A single maintainer reads the reports.

## What is in scope

IW4L is an experimental runtime and its network is meant for arranged playtests
between people who agreed to play. Reports are most useful where a problem
reaches past that:

* code execution or file writes outside `iw4l-artifacts/` from map data, a
  replay, a demo file or another client's traffic
* anything that writes to, patches or deletes files in an original game
  installation — IW4L reads those, and only reads them
* the update path: a way for something other than the release host to deliver a
  binary that the launcher accepts
* leaking the machine's files, credentials or private keys to a peer or to the
  master

## What is already known and not a finding

* The protocol is unversioned across builds and unstable by design; two
  different releases failing to talk to each other is expected.
* The master relays between clients and vouches for nobody. Connecting to a
  match means trusting who you are playing with, as stated in `README.md`.
* IW4L parses game data that a local user chose to supply. Malformed data from
  your own install crashing the runtime is a bug, not a vulnerability.

## What a report needs

The commit or release, the OS, what data reproduced it, and the steps. A crash
backtrace or the offending file is more useful than a description of it. Do not
send `.env`, private keys, or copies of game files.
