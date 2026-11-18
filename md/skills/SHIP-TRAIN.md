# Ship train

Getting everything useful that is local or open on GitHub into `origin/master`,
with as few commits as possible, one human playtest, and an empty `mrs/` at the
end. The release mechanics are the repo's own `docs/DEPLOY.md`; the tree pass is
[PUBLIC-PUSH.md](PUBLIC-PUSH.md); landing a clone is [MRS.md](MRS.md). This file
is the order and the owner's standing requirements.

Records of past trains (hashes, releases demo.1–demo.23, deploy evidence):
[archive/SHIP-TRAIN-LOG.md](../../context/archive/SHIP-TRAIN-LOG.md). The owner's original
brief: [archive/MERGE-TRAIN-PROMPT.txt](../../context/archive/MERGE-TRAIN-PROMPT.txt).
New trains are recorded as artifacts ([ARTIFACTS.md](ARTIFACTS.md)), not
appended to that log.

## What the owner wants

1. Merge every external PR whose usefulness is certain, so it lands with its
   contributor's authorship and shows as **merged** on GitHub. Take as many open
   PRs as possible into the train — they should not sit as dead weight. Do not
   fold in other PRs just because their files overlap.
2. Minimum number of commits on master.
3. All unpushed owner work, including `mrs/*` and the PUBLIC-PUSH pass, squashed
   into as few commits as possible — normally one.
4. All useful work on the remote at the end; `mrs/` empty.

**Very strict about the rules in the repo's contract and the skills here.**

## Order

1. **Inventory.** `git fetch`, `git log origin/master..`, `make mr ls`,
   `gh pr list --state open`. Record published head, unpushed commits, clones
   and the PR heads actually taken (hash per PR) in the train artifact.
2. **Rollback.** `refs/backup/pre-train-<date>` and a bundle of all refs, before
   any rewrite. Git experiments go in `target/git-experiment/`, never in the
   tree.
3. **External PRs.** On a fresh branch from `origin/master`, merge all chosen
   PR heads in **one merge commit** (octopus) so their exact hashes, authors and
   messages stay ancestors — examples of a good one: `d7374a30`, `bfe3c2aa`.
   Never squash contributor commits into the owner commit.
4. **Local MRs.** `make mr ship <name>` for every ready clone. The clone is
   deleted by the command; check `mrs/` is empty afterwards.
5. **Owner squash.** All unpushed owner work + shipped MRs + the PUBLIC-PUSH
   pass become one commit on top of the contributor merge. Message: the effect.
   Do not soft-reset onto `origin/master` past the contributor merge — that
   discards the external history.
6. **Checks.** Scoped `make mr fmt`, `git diff --check`, `make publish-check`,
   Linux `play` build, Windows cross-build and static master if releasing,
   Clippy (existing warnings are not this train's). Approved runs:
   `make approved` — fresh `heavy_gameplay_lifecycle` and `master_duo_chaos` on
   the final tree; inspect both clients' FPV screenshots (weapons and hands).
   Failures are retained and disclosed, never retried away silently.
7. **Don't trust the build — expect it to break.** One map per family
   (IW4, IW5, T5, T6), spawn bots, play a minute. No half-hour soaks before
   handing over; expect a stall or a map that will not load.
8. **Hand over (`-READY`).** The prepared result is **on the root master**, not
   in a side folder, so the owner sees exactly what will be pushed. Stop. The
   owner reads the diff and playtests **once** (IW4/IW5/T5 maps, two machines).
9. **Push** only after the owner says so. Normal push, never force.
   GitHub marks the PRs merged; link them to the thread.
10. **Release** when authorized, per `docs/DEPLOY.md`: `make release`, publish to
    **both** community channels (dev and prod) so clients pull the update
    automatically, `make github-release TAG=v0.1.0-demo.N NOTES=…`. Verify
    served manifest + blob over HTTPS, QUIC health, GitHub asset digest =
    local ZIP. Run a post-deploy duo. Certificates, keys, CAs, units, `.env`
    and descriptors are not touched; masters restart only if their binary
    changed.
11. **Close.** Root clean = `origin/master`, `mrs/` empty, artifact `-FINAL`
    with what was not exercised (platforms, controllers, independent machines).

## Hazards from past trains

* Community box (address in its `.iw4l-server` descriptor): dev and prod are both processes named
  `iw4l-master`. **Never `pkill` by name** — kill by PID. That took prod down on
  2026-10-13.
* Prod serves updates from `/usr/local/lib/iw4l/updates/prod`; check the unit's
  `ExecStart --updates` before trusting `.env`'s deploy root.
* Protocol versions: no compatibility checks for old versions — there are no
  users to keep; bump by one.
* Bad train, for reference: a 214-line unit-test file outside the approved list
  slipped into `approved_tests` (publish-check allows `#[test]` there), and
  "format", "merge upstream", "collapse condition" fix-commits landed as
  separate objects. Once pushed, that is a lesson, not a task.
