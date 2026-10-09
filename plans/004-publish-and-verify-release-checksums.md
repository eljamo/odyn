# Plan 004: Every release publishes checksums and the installer verifies them

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 565080d..HEAD -- .github/workflows/release.yml install.sh crates/nebula/src/upgrade.rs`
> If any of those changed since this plan was written, compare the "Current
> state" excerpts below against the live files before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: security
- **Planned at**: commit `565080d`, 2026-10-06

## Why this matters

The release pipeline publishes four `.tar.gz` binaries with no checksum file,
and `install.sh` downloads one and installs it with no integrity check. A
truncated, corrupted or substituted artifact installs silently, and there is
nothing a user, a reviewer or a downstream packager can verify an archive
against.

This is the remaining gap rather than a pattern of neglect — the surrounding
risks were clearly considered. `upgrade.rs:242-244` already explains why the
installer is staged to a file rather than piped to `sh`:

> `curl … | sh` executes whatever arrived when a connection drops
> mid-transfer; a staged file either passes the shebang check below or never
> runs at all.

and it stages into the 0700 runtime directory for exactly the right reason
(`upgrade.rs:60-62`). The binary archive is the one hop in that chain with
nothing checking it.

Publishing `SHA256SUMS` from the release job and verifying it in the installer
closes it, costs a handful of lines, and gives anyone packaging nebula
something to pin to.

## Current state

### The release pipeline publishes no checksums

`.github/workflows/release.yml:40-49` — the end of the per-target build job:

```yaml
      - name: Build
        run: cargo build --release --locked --target ${{ matrix.target }}

      - name: Package
        run: tar -czf nebula-${{ matrix.target }}.tar.gz -C target/${{ matrix.target }}/release nebula

      - uses: actions/upload-artifact@v4
        with:
          name: nebula-${{ matrix.target }}
          path: nebula-${{ matrix.target }}.tar.gz
```

`.github/workflows/release.yml:51-63` — the job that publishes them:

```yaml
  release:
    needs: build
    if: startsWith(github.ref, 'refs/tags/v')
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/download-artifact@v4
        with:
          merge-multiple: true

      - uses: softprops/action-gh-release@v2
        with:
          files: nebula-*.tar.gz
          generate_release_notes: true
```

The matrix (`release.yml:18-29`) is four targets:
`aarch64-apple-darwin` and `x86_64-apple-darwin` on `macos-latest`,
`x86_64-unknown-linux-musl` on `ubuntu-24.04`, and
`aarch64-unknown-linux-musl` on `ubuntu-24.04-arm`.

**The two runner families have different hashing tools.** macOS ships
`shasum` and no `sha256sum`; the Ubuntu images ship both. That difference has
to be handled in the build step, and again in the installer.

### The installer does not verify

`install.sh:43-52`:

```sh
install_from_release() {
    url="https://github.com/$REPO/releases/latest/download/nebula-$1.tar.gz"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "downloading $url"
    curl -fsSL "$url" -o "$tmp/nebula.tar.gz" || return 1
    tar -xzf "$tmp/nebula.tar.gz" -C "$tmp"
    mkdir -p "$INSTALL_DIR"
    install -m 755 "$tmp/nebula" "$INSTALL_DIR/nebula"
}
```

Its caller, `install.sh:64-73`, treats a non-zero return as "fall back to
building from source", which is the behaviour a failed download already gets:

```sh
    installed=""
    if target=$(detect_target); then
        if install_from_release "$target"; then
            installed="$INSTALL_DIR/nebula"
        else
            reason="couldn't download nebula-$target from the latest release"
        fi
    else
        reason="no prebuilt binary for $(uname -s) $(uname -m)"
    fi
```

A **mismatched** checksum must not take that path. Falling back to
`cargo install --git` after a hash failure would turn a tampered download into
a silent source build, which hides the thing worth shouting about. A mismatch
is a hard `err` (which `exit 1`s — see `install.sh:18-21`).

`install.sh:12` sets `set -eu`; there is no `pipefail`, so avoid relying on
exit status through a pipe.

### A rollout ordering problem you must handle

`README.md:26` and `upgrade.rs:13-14` both point users at
`https://raw.githubusercontent.com/AgentSystemLabs/nebula/main/install.sh` —
the installer is always read from `main`, but it installs
`releases/latest/download/…`. So the moment this change lands on `main`,
the new installer is live while the latest release still has no `SHA256SUMS`.
Every install would break until the next tag.

Therefore: a **missing** `SHA256SUMS` warns and continues; a **present** one
that does not match is fatal. Once a release or two has shipped with sums the
missing case can be tightened, and that is noted as a follow-up rather than
done here.

### `nebula upgrade` needs no change

`crates/nebula/src/upgrade.rs:215-239` downloads `install.sh` and runs it:

```rust
    let script = stage_script(url, staging_dir)?;
    // Inherited stdio: the script's own progress lines are the UI here.
    // NEBULA_UPGRADE_HANDOFF tells install.sh to skip its "daemon still
    // running" note — finish_daemon_handoff owns that messaging here.
    let result = Command::new("sh")
        .arg(&script)
        .env(nebula_core::env::UPGRADE_HANDOFF, "1")
        .status()
```

So `nebula upgrade` inherits the verification for free. Do not add a second
implementation in Rust.

### Repo conventions that apply here

- `install.sh` is POSIX `sh`, not bash: no `[[`, no arrays, no `local` beyond
  what `dash` accepts. It uses `say()` for stdout and `err()` for a fatal
  message to stderr (`install.sh:17-21`).
- Every non-obvious decision carries a comment explaining *why*, often several
  lines — `install.sh:89-95` and `release.yml:6-8` are the house style. Match
  that density.
- Workflow actions are pinned to a major version (`actions/checkout@v4`,
  `actions/upload-artifact@v4`, `softprops/action-gh-release@v2`).
- `install.sh` quotes every expansion.

## Commands you will need

| Purpose                | Command                                                                   | Expected on success             |
|------------------------|---------------------------------------------------------------------------|---------------------------------|
| Workflow YAML syntax   | `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"` | no output, exit 0        |
| Shell syntax           | `sh -n install.sh`                                                        | no output, exit 0               |
| Shellcheck (if present)| `shellcheck install.sh`                                                   | exit 0                          |
| Hash a file (Linux)    | `sha256sum <file>`                                                        | `<64 hex>  <file>`              |
| Hash a file (macOS)    | `shasum -a 256 <file>`                                                    | `<64 hex>  <file>`              |
| Tests                  | `cargo test --workspace --locked`                                         | exit 0, every target `ok`       |
| The whole gate         | `make ci`                                                                 | exit 0                          |

`make ci` is listed because the repository's own gate must stay green; this
plan changes no Rust, so it should pass untouched.

## Scope

**In scope**:
- `.github/workflows/release.yml`
- `install.sh`

**Out of scope** (do NOT touch, even though they look related):
- `crates/nebula/src/upgrade.rs` — it runs `install.sh`, so it inherits the
  check. A second verification path in Rust would be two things to keep in
  step.
- `.github/workflows/ci.yml` — plan 002 creates it and plan 003 edits it.
- GPG/minisign signing, and `actions/attest-build-provenance`. Both are
  stronger than a checksum and both are a larger decision (key custody, or a
  verifier dependency on the client). See the maintenance notes.
- The `install_from_source` fallback at `install.sh:54-59` — `cargo install
  --locked` has its own integrity story via the registry.
- Any Rust file. If this plan seems to need one, STOP.

## Git workflow

- Branch: `publish-and-verify-release-checksums` (this repo uses bare
  descriptive branch names, sometimes issue-numbered — e.g.
  `fix-106-open-pr-list-stale`).
- Two commits reads well: the publishing side, then the verifying side. This
  repo does **not** use conventional commits; messages are descriptive
  sentences, capitalised, no type prefix. For example:
  `Every release ships a SHA256SUMS beside its archives and the installer checks the one it downloaded against it, so a corrupted or swapped tarball stops instead of installing`
- Do NOT push, tag, or open a PR unless the operator instructed it. **Do not
  push a tag under any circumstances** — a tag push publishes a real release.

## Steps

### Step 1: Emit a checksum per target in the build job

In `.github/workflows/release.yml`, add a step after `Package`
(`release.yml:43-44`) and before the `upload-artifact` step, then widen the
artifact path.

Replace `release.yml:43-49` with:

```yaml
      - name: Package
        run: tar -czf nebula-${{ matrix.target }}.tar.gz -C target/${{ matrix.target }}/release nebula

      # One `<sha256>  <filename>` line per target, which the release job
      # concatenates into a single SHA256SUMS. Written here rather than in
      # the release job so the hash is taken on the machine that built the
      # archive, before it has crossed the artifact store.
      #
      # macOS runners ship `shasum` and no `sha256sum`; the Ubuntu images ship
      # both. Both tools print the same `<hash>  <name>` format, which is what
      # `sha256sum -c` and install.sh expect.
      - name: Checksum
        run: |
          file=nebula-${{ matrix.target }}.tar.gz
          if command -v sha256sum >/dev/null 2>&1; then
            sha256sum "$file" > "$file.sha256"
          else
            shasum -a 256 "$file" > "$file.sha256"
          fi
          cat "$file.sha256"

      - uses: actions/upload-artifact@v4
        with:
          name: nebula-${{ matrix.target }}
          path: |
            nebula-${{ matrix.target }}.tar.gz
            nebula-${{ matrix.target }}.tar.gz.sha256
```

Note the `path:` becomes a two-line block literal — that is how
`upload-artifact@v4` takes multiple paths.

**Verify**:
`python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"`
→ no output, exit 0.

### Step 2: Collect them into one `SHA256SUMS` and publish it

Replace `release.yml:51-63` (the whole `release` job's steps) with:

```yaml
  release:
    needs: build
    if: startsWith(github.ref, 'refs/tags/v')
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/download-artifact@v4
        with:
          merge-multiple: true

      # One file for the whole release, in `sha256sum -c` format, sorted so
      # the asset is byte-identical whatever order the matrix finished in.
      # The `-c` run is the self-check: it fails here if an archive did not
      # survive the artifact round-trip, which is better than publishing sums
      # that do not match what is beside them.
      - name: Collect checksums
        run: |
          cat nebula-*.tar.gz.sha256 | sort -k2 > SHA256SUMS
          rm -f nebula-*.tar.gz.sha256
          sha256sum -c SHA256SUMS
          cat SHA256SUMS

      - uses: softprops/action-gh-release@v2
        with:
          files: |
            nebula-*.tar.gz
            SHA256SUMS
          generate_release_notes: true
```

Two details that matter:

- `rm -f nebula-*.tar.gz.sha256` after concatenating, so the per-target files
  are not also published — `files: nebula-*.tar.gz` would otherwise be fine,
  but leaving them around invites a later glob from picking them up.
- `sha256sum -c SHA256SUMS` runs in the same directory as the archives, which
  `merge-multiple: true` guarantees.

**Verify**:
- `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"`
  → no output, exit 0.
- `grep -c 'name: Collect checksums' .github/workflows/release.yml` → `1`, and `grep -q 'SHA256SUMS' .github/workflows/release.yml` → exit 0.

### Step 3: Rehearse the workflow's shell logic locally

The workflow cannot be run here, but its two shell snippets can. In a scratch
directory outside the repository (`/tmp`, not the working tree):

```sh
cd "$(mktemp -d)"
printf 'not really a binary\n' > nebula
tar -czf nebula-x86_64-unknown-linux-musl.tar.gz nebula
tar -czf nebula-aarch64-apple-darwin.tar.gz nebula

# step 1's snippet, both branches
for t in x86_64-unknown-linux-musl aarch64-apple-darwin; do
  file=nebula-$t.tar.gz
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" > "$file.sha256"
  else
    shasum -a 256 "$file" > "$file.sha256"
  fi
done

# step 2's snippet
cat nebula-*.tar.gz.sha256 | sort -k2 > SHA256SUMS
rm -f nebula-*.tar.gz.sha256
cat SHA256SUMS
```

**Verify**: `SHA256SUMS` holds exactly two lines, each `<64 hex>  nebula-<target>.tar.gz`,
sorted by filename. Then verify it the way the installer will:

```sh
if command -v sha256sum >/dev/null 2>&1; then sha256sum -c SHA256SUMS
else shasum -a 256 -c SHA256SUMS; fi
```

→ both archives report `OK`.

Now prove it catches tampering:

```sh
printf 'tampered\n' >> nebula-aarch64-apple-darwin.tar.gz
if command -v sha256sum >/dev/null 2>&1; then sha256sum -c SHA256SUMS
else shasum -a 256 -c SHA256SUMS; fi; echo "exit: $?"
```

→ that archive reports `FAILED` and the exit status is non-zero. If it does
not, STOP — the format is wrong and the installer change would be a no-op.

### Step 4: Verify in `install.sh`

Add a helper above `install_from_release`, and the verification inside it.
Replace `install.sh:43-52` with:

```sh
# Hash `$1` with whatever this box has. Prints the bare hex digest, or
# nothing when neither tool is installed — minimal containers sometimes have
# neither, and refusing to install there would be a worse trade than saying
# the check was skipped.
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# Check the downloaded archive against the release's SHA256SUMS.
#
# A missing SHA256SUMS warns and continues: install.sh is always read from
# `main` but installs `releases/latest`, so between this landing and the next
# tag the newest release legitimately has no sums file. A mismatch is fatal
# and does NOT fall back to a source build — that would turn the one thing
# worth shouting about into a silent four-minute compile.
verify_archive() {
    archive="$1"
    target="$2"
    sums="$3"
    if ! curl -fsSL "https://github.com/$REPO/releases/latest/download/SHA256SUMS" -o "$sums"; then
        say "note: this release publishes no SHA256SUMS — skipping the integrity check"
        return 0
    fi
    want=$(grep " nebula-$target\.tar\.gz\$" "$sums" | cut -d' ' -f1)
    if [ -z "$want" ]; then
        say "note: SHA256SUMS names no nebula-$target.tar.gz — skipping the integrity check"
        return 0
    fi
    got=$(sha256_of "$archive")
    if [ -z "$got" ]; then
        say "note: neither sha256sum nor shasum is installed — skipping the integrity check"
        return 0
    fi
    if [ "$got" != "$want" ]; then
        err "checksum mismatch for nebula-$target.tar.gz
  expected $want
  got      $got
The download does not match what the release published. Nothing was installed.
Re-run to retry; if it keeps failing, open an issue at
https://github.com/$REPO/issues rather than installing it."
    fi
    say "checksum verified"
}

install_from_release() {
    url="https://github.com/$REPO/releases/latest/download/nebula-$1.tar.gz"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "downloading $url"
    curl -fsSL "$url" -o "$tmp/nebula.tar.gz" || return 1
    verify_archive "$tmp/nebula.tar.gz" "$1" "$tmp/SHA256SUMS"
    tar -xzf "$tmp/nebula.tar.gz" -C "$tmp"
    mkdir -p "$INSTALL_DIR"
    install -m 755 "$tmp/nebula" "$INSTALL_DIR/nebula"
}
```

Things to get right:

- `verify_archive` is called **before** `tar -xzf`, so a bad archive is never
  unpacked.
- The `grep` pattern anchors on a leading space and a trailing end-of-line, so
  `nebula-x86_64-apple-darwin.tar.gz` cannot match a line for
  `nebula-x86_64-unknown-linux-musl.tar.gz`. Both `sha256sum` and
  `shasum -a 256` write two spaces between hash and name, so
  `cut -d' ' -f1` takes the hash and the pattern's single leading space still
  matches the second of the two.
- `err` already exits (`install.sh:18-21`), so a mismatch never reaches the
  caller's source-build fallback.
- `set -eu` is in force and there is no `pipefail`, so do not test the status
  of a pipeline.

**Verify**:
- `sh -n install.sh` → no output, exit 0.
- If `shellcheck` is installed: `shellcheck install.sh` → exit 0.
- `grep -n 'verify_archive "$tmp/nebula.tar.gz"' install.sh` → appears once,
  on a line before the `tar -xzf` line.

### Step 5: Exercise `install.sh` against a local fake release

`curl` speaks `file://`, which is how `upgrade.rs`'s own tests drive this path
(`upgrade.rs:295-299`). But `install_from_release` builds a `https://github.com`
URL from `$REPO`, so the honest local test is of the helper, not the whole
script. In a scratch directory outside the working tree:

```sh
cd "$(mktemp -d)"
# Reuse the function definitions without running main().
sed -n '/^sha256_of()/,/^}/p;/^verify_archive()/,/^}/p' /path/to/repo/install.sh > fns.sh
cat > harness.sh <<'EOF'
set -eu
REPO="example/example"
say() { printf '%s\n' "$*"; }
err() { printf 'error: %s\n' "$*" >&2; exit 1; }
. ./fns.sh
EOF

printf 'archive bytes\n' > archive.tgz

# (a) matching sums → "checksum verified", exit 0
( . ./harness.sh
  if command -v sha256sum >/dev/null 2>&1; then h=$(sha256sum archive.tgz | cut -d' ' -f1)
  else h=$(shasum -a 256 archive.tgz | cut -d' ' -f1); fi
  printf '%s  nebula-x86_64-apple-darwin.tar.gz\n' "$h" > sums
  curl() { cp sums "$4"; }   # stand in for the SHA256SUMS download
  verify_archive archive.tgz x86_64-apple-darwin sums.out )
echo "exit (a): $?"

# (b) wrong hash → fatal, non-zero
( . ./harness.sh
  printf '%064d  nebula-x86_64-apple-darwin.tar.gz\n' 0 > sums
  curl() { cp sums "$4"; }
  verify_archive archive.tgz x86_64-apple-darwin sums.out ) || echo "exit (b): $?"

# (c) no SHA256SUMS → warns, exit 0
( . ./harness.sh
  curl() { return 22; }
  verify_archive archive.tgz x86_64-apple-darwin sums.out )
echo "exit (c): $?"
```

**Verify**:
- (a) prints `checksum verified`, exit 0.
- (b) prints `error: checksum mismatch for nebula-x86_64-apple-darwin.tar.gz`
  on **stderr** with both hashes, and exits non-zero.
- (c) prints the `no SHA256SUMS` note and exits 0.

If `curl`'s argument position differs from `$4` on your shell's reading of the
call, adjust the stub rather than the script — the stub is scaffolding and is
not committed.

### Step 6: Final check

**Verify**:
- `make ci` → exit 0 (no Rust changed; this confirms you did not touch any).
- `git status --short` lists exactly two paths:
  `.github/workflows/release.yml` and `install.sh`.
- Nothing from step 3 or step 5 is inside the repository — both were done in
  `mktemp -d` directories.

## Test plan

There is no Rust test suite for `install.sh` or the workflows, and this plan
does not add one — a shell-test harness would be a new tool in a repository
that has none, for two files.

Verification is the three rehearsals above, and they are the test cases:

1. **Round-trip** (step 3): hashes written by the build snippet are accepted
   by `sha256sum -c` / `shasum -a 256 -c` on the same files.
2. **Tamper detection** (step 3): appending a byte to an archive makes the
   check fail with a non-zero status.
3. **Installer, matching** (step 5a): verification passes and says so.
4. **Installer, mismatch** (step 5b): fatal, message on stderr naming both
   hashes, non-zero exit, and no fallback to a source build.
5. **Installer, sums absent** (step 5c): warns and continues, so the rollout
   window does not break installs.

The remaining verification needs a GitHub run and is the operator's: a
`workflow_dispatch` of `release.yml` builds the whole matrix without
publishing (`release.yml:6-8` documents that this is what dispatch is for),
which exercises steps 1 and 2 on real runners. Say in your report that this is
outstanding. **Do not trigger it yourself and do not push a tag.**

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"` exits 0
- [ ] `sh -n install.sh` exits 0
- [ ] `grep -c 'name: Checksum' .github/workflows/release.yml` returns 1 and `grep -c 'name: Collect checksums' .github/workflows/release.yml` returns 1
- [ ] `grep -c 'shasum -a 256' .github/workflows/release.yml` returns 1 (the macOS branch)
- [ ] `grep -c 'sha256sum -c SHA256SUMS' .github/workflows/release.yml` returns 1 (the self-check)
- [ ] `grep -c 'verify_archive' install.sh` returns 2 (the definition and the one call)
- [ ] `awk '/verify_archive "\$tmp/{v=NR} /tar -xzf/{t=NR} END{exit !(v && t && v<t)}' install.sh` exits 0 — verification precedes extraction
- [ ] `grep -c 'checksum mismatch' install.sh` returns 1
- [ ] steps 3 and 5 all behaved as described, including the non-zero exit on a tampered archive
- [ ] `make ci` exits 0
- [ ] `git status --short` lists only `.github/workflows/release.yml` and `install.sh`
- [ ] `plans/README.md` status row for 004 updated

## STOP conditions

Stop and report back (do not improvise) if:

- `.github/workflows/release.yml:40-63` or `install.sh:43-52` do not match the
  excerpts above.
- Step 3's tamper test does **not** fail. The whole plan rests on it; a
  passing check against a modified archive means the format or the tool
  invocation is wrong, and shipping that would be worse than shipping nothing.
- You find yourself needing to change `crates/nebula/src/upgrade.rs`. It runs
  `install.sh`; if that is not enough, the reason matters and the operator
  should hear it.
- A hash mismatch would reach `install.sh:64-73`'s source-build fallback. That
  path must be unreachable from a mismatch.
- The operator asks you to push a tag, or you are tempted to, to "test the
  release". A tag push publishes a real release to real users.
- `make ci` fails. This plan changes no Rust, so a failure means either you
  touched something out of scope or plan 001 has not landed.

## Maintenance notes

- **Tighten the missing-sums case later.** Once two or three releases have
  shipped with `SHA256SUMS`, the `return 0` in `verify_archive`'s "publishes
  no SHA256SUMS" branch should become an `err`. Doing it now would break every
  install between this landing on `main` and the next tag, because `install.sh`
  is read from `main` while the binary comes from `releases/latest`. That
  coupling is worth remembering any time `install.sh` grows an expectation
  about a release asset.
- The checksum is taken on the machine that built the archive and verified
  again in the release job after the artifact round-trip. It protects against
  corruption and against a swapped asset; it does **not** prove who built it,
  because the sums file lives next to the archives and whoever could replace
  one could replace both.
- The stronger options, both deliberately deferred: `actions/attest-build-provenance`
  (GitHub-native, verifiable with `gh attestation verify`, but adds a `gh`
  dependency to the install path) and minisign/GPG signing (strongest, but
  introduces a key to hold and rotate). Either would sit on top of this rather
  than replace it.
- A reviewer should check three things: that `verify_archive` is called before
  `tar -xzf`, that a mismatch cannot fall through to the source build, and
  that the `grep` pattern in `verify_archive` cannot match the wrong target's
  line.
- `nebula upgrade` gets this for free by running `install.sh`. Keep it that
  way — a second verification path in Rust is a second thing to keep correct.
