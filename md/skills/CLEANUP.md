# Artifact cleanup

Run the helper from this repository root. It is scoped to the local
`context/` and `iw4l-artifacts` directories.

Keep the knowledge; remove the generated output after a task's result is recorded.

## Owner instruction — 2026-10-10

The owner requested removal of obsolete logs, raw traces and other disposable
artifact output while retaining useful Markdown. This is an explicit retention
exception to [ARTIFACTS.md](ARTIFACTS.md)'s historical evidence rule. Do not rewrite old journal
entries to pretend that deleted raw evidence is still available. Reports remain
historical records; rerun a measurement when its raw output is needed again.

## Retain

- All `.md` notes and `.ma` instructions, including previous iterations.
- Probe/source code, scripts, patches, diffs, Git histories and rollback bundles.
- Compact result reports, CSV measurements, audit JSON, `run.json`, exact commands,
  seeds, build/source hashes and profile/class inputs needed for replay.
- Useful screenshots, recordings, demos, research databases and reference assets.
- The current playtest package and releases needed by the owner.

Do not decide usefulness by extension alone: a `.txt` can be probe source,
a `.json` can be a replay input, and a binary can be a research database.

## Remove

- Generated runtime `cache/` contents: `mips`, `wgsl`, `xwma_pcm`, `nav`, `localize`.
  These are rebuilt by the runtime. Test runs must not permanently retain their
  own copies after the result is recorded. The next load may be slower.
- Cargo output directories (`target`, `target-*`, `*-target`) identified by Cargo
  markers/build layout; include incremental state and old probe builds.
- Python bytecode, runtime `scratch/` and temporary Windows SDK cache copies.
- Obsolete compiled Linux probe/runtime binaries and generated `iw4l*.exe`
  copies; keep the current owner playtest package.
- Logs, compressed logs, captured stdout/stderr, Perfetto `.pftrace` files and
  the old Bevy raw trace JSON files. Keep the conclusions in compact reports.

Never recursively delete an entire artifact or delete every non-Markdown file.
Keep source snapshots and their histories even when their build output is removed.

## Cleanup sequence

1. Finish or stop the task's game, probe and build processes before cleaning
   their outputs. Check which playtest package is current.
2. Record the outcome, commands, source/build identity and relevant failure
   details in the artifact. Keep replay reports and profiles together.
3. From the repository root, preview the supported disposable output classes:

   ```sh
   python3 context/cleanup-artifacts.py
   ```

4. Apply the cleanup:

   ```sh
   python3 context/cleanup-artifacts.py --apply
   ```

5. Check the summary for errors and measure actual free space:

   ```sh
   sync -f .
   df -h .
   du -h --max-depth=1 context/artifacts iw4l-artifacts
   ```

The helper is scoped to `context/artifacts`, `iw4l-artifacts` and this user's
`~/.cache/iw4l-probes`. It preserves Markdown even inside output directories,
skips `.git`, does not follow symlinks and leaves unknown files for review.
It does not clean `context/mrs`, retail installs, the RE base, externals, release
archives, account profiles or unrelated home directories. Update its current
playtest exemption when a newer package replaces the 2026-10-10 handoff.

Btrfs compression and shared extents make reported file totals differ from
physical space recovered. Use `df` before and after for the actual result.

## Prevent another buildup

- Reuse a shared cache for routine checks. Use a cold cache only for a check
  that needs it, then remove that run's generated cache when it finishes.
- Retain compact reports/replay inputs instead of whole runtime output trees.
- Disable raw tracing except during an investigation. Bound log size and
  suppress repeated errors after capturing their cause.
- Remove a probe's build outputs once its result and source are saved.
- Run this cleanup after a train's validation/handoff and after large experiments.

This cleanup does not certify a warm/cold timing claim or a fresh validation.
Deleting outputs changes what evidence remains, not what historically passed.

## Last completed cleanup — 2026-10-10

The disk went from 96% used / 39 GiB free at the start of the investigation
to 19% used / about 740 GiB free after both cleanup passes. Artifact folders
now total about 28 GiB. All 1,677 Markdown files and the `.ma` instruction were
retained; the current playtest archives still match their recorded hashes.
A second helper preview found zero candidates and zero errors.

The historical cleanup record was `context/artifacts/2026-10-10-disk-cleanup/README.md`;
it is absent from this checkout. The figures above are historical, not a fresh
measurement. Previous task reports were not rewritten.
