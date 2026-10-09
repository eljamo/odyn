# Plan 003: The toolchain is pinned and a clippy warning fails the build

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 565080d..HEAD -- Makefile .github/workflows crates/nebula-tui/src/ui.rs crates/nebula-tui/src/markdown.rs crates/nebula-tui/src/event_loop.rs crates/nebula-daemon/src/pty/kitty.rs crates/nebula-daemon/src/lifecycle.rs`
> If any of those changed since this plan was written, compare the "Current
> state" excerpts and the warning list below against the live code before
> proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/002-add-ci-test-workflow.md`
- **Category**: dx
- **Planned at**: commit `565080d`, 2026-10-06

## Why this matters

`make lint` deliberately does not fail on warnings, and the `Makefile` says
exactly why:

> Not `-D warnings` by default, so a lint a new toolchain adds doesn't fail
> the gate on its own

That is a sound reaction to an unpinned toolchain — every contributor and every
CI runner compiles with whatever `stable` happens to be that week, so a new
clippy lint can turn a green tree red without anybody changing a line. The
cost of living with it is that the lint gate advises rather than gates, and 11
warnings have accumulated.

Two of those 11 are from lints that did not exist in older releases
(`clippy::chunks_exact_to_as_chunks`, `clippy::manual_checked_ops`), which is
the Makefile's concern happening in practice.

Pinning the toolchain removes the reason for the compromise. Once every build
uses one known compiler, a new lint arrives only when somebody deliberately
moves the pin — and at that moment it is one person's job to clear it, not a
surprise on an unrelated pull request. Pinning also makes release builds
reproducible: `release.yml` currently builds the published binaries with
whatever stable the runner image shipped with.

## Current state

### No pin exists

There is no `rust-toolchain.toml` or `rust-toolchain` file in the repository
root. `Cargo.toml:13-16` sets only the edition:

```toml
[workspace.package]
version = "0.43.0"
edition = "2021"
license = "MIT"
```

Validated against `rustc 1.88.0 (6b00bc388 2025-06-23)`.

`release.yml:37-41` adds targets to whatever toolchain the runner defaults to:

```yaml
      - name: Add rust target
        run: rustup target add ${{ matrix.target }}

      - name: Build
        run: cargo build --release --locked --target ${{ matrix.target }}
```

A `rust-toolchain.toml` is honoured by `rustup` for every `cargo`/`rustc`
invocation in the directory, including that one, so release builds pick up the
pin with no change to `release.yml`.

### The lint target and the gate

`Makefile:227-240`:

```make
# Not `-D warnings` by default, so a lint a new toolchain adds doesn't fail
# the gate on its own; the workspace does clear that bar today, so keep it
# that way. CI runs no clippy at all. `make lint STRICT=1` opts into the
# stricter gate.
lint: ## Clippy over the workspace (STRICT=1 to fail on warnings)
	cargo clippy --workspace --all-targets $(if $(STRICT),-- -D warnings)

test: ## Full test suite (e2e_pty spawns real daemons — slow)
	cargo test --workspace

ci: ## The whole gate: fmt check, clippy, tests
	cargo fmt --all -- --check
	@$(MAKE) --no-print-directory lint
	@$(MAKE) --no-print-directory test
```

Note: plan 002 will already have replaced the sentence `CI runs no clippy at
all.` in that comment. Expect to find a sentence pointing at
`.github/workflows/ci.yml` instead. If plan 002 has not landed, STOP.

### The 11 warnings to clear

Every one, as reported by `cargo clippy --workspace --all-targets` at
`565080d` on 1.88.0. All 11 are in library code; `--all-targets` adds none
from test or integration targets.

| Lint | Site | Auto-fixable |
|---|---|---|
| `clippy::unnecessary_sort_by` | `crates/nebula-tui/src/ui.rs:1511` | no |
| `clippy::unnecessary_sort_by` | `crates/nebula-tui/src/ui.rs:1515` | no |
| `clippy::manual_checked_ops` | `crates/nebula-tui/src/markdown.rs:1029` | no |
| `clippy::collapsible_match` | `crates/nebula-tui/src/event_loop.rs:5631` | yes |
| `clippy::collapsible_match` | `crates/nebula-tui/src/event_loop.rs:9434` | yes |
| `clippy::collapsible_match` | `crates/nebula-tui/src/event_loop.rs:9569` | yes |
| `clippy::collapsible_match` | `crates/nebula-tui/src/event_loop.rs:10118` | yes |
| `clippy::byte_char_slices` | `crates/nebula-daemon/src/pty/kitty.rs:230` | yes |
| `clippy::byte_char_slices` | `crates/nebula-daemon/src/pty/kitty.rs:234` | yes |
| `clippy::byte_char_slices` | `crates/nebula-daemon/src/pty/kitty.rs:236` | yes |
| `clippy::chunks_exact_to_as_chunks` | `crates/nebula-daemon/src/lifecycle.rs:175` | no |

`cargo clippy --fix` handles 7 of them (4 in `nebula-tui`, 3 in
`nebula-daemon`). The other 4 are small and mechanical but want a human-shaped
edit.

**If you pin a toolchain newer than 1.88.0, expect a different and probably
longer list.** The table is the baseline, not the contract; step 3 works from
whatever your pinned toolchain reports.

### Repo conventions that apply here

- Every non-obvious decision carries a comment explaining *why*, often several
  lines. `Cargo.toml:6-11` and `Cargo.toml:39-54` are the house style for
  explaining a build-configuration choice — match that density in the new
  `rust-toolchain.toml`.
- `cargo fmt` is authoritative and the tree is clean under it. Never
  hand-format around it.
- `#[allow(clippy::…)]` is used sparingly and only for shape lints — 14
  occurrences across 80K lines of library code, all
  `too_many_arguments`, `option_option` or `large_enum_variant`. None of them
  silences a correctness lint, and none of them is one of your 11. Fix the
  code; an `allow` needs a comment saying why the lint is wrong here, and only
  after you have shown the fix is worse.

## Commands you will need

| Purpose                   | Command                                                        | Expected on success                   |
|---------------------------|----------------------------------------------------------------|---------------------------------------|
| Current toolchain         | `rustc --version`                                              | prints a version                      |
| Update stable             | `rustup update stable`                                         | exit 0                                |
| Show active toolchain     | `rustup show active-toolchain`                                 | names the pinned toolchain after step 2 |
| Lint, lenient             | `cargo clippy --workspace --all-targets`                       | exit 0                                |
| Lint, strict              | `make lint STRICT=1`                                           | exit 0, **no** warnings               |
| Count warnings            | `cargo clippy --workspace --all-targets 2>&1 \| grep -c '^warning: [a-z]'` | `0` after step 3           |
| Auto-fix (tui)            | `cargo clippy --fix --lib -p nebula-tui`                       | exit 0                                |
| Auto-fix (daemon)         | `cargo clippy --fix --lib -p nebula-daemon`                    | exit 0                                |
| Format check              | `cargo fmt --all -- --check`                                   | exit 0, no output                     |
| Tests                     | `cargo test --workspace --locked`                              | exit 0, every target `ok`             |
| The whole gate            | `make ci`                                                      | exit 0                                |

Clippy caches its results. To force a re-run after no source change, append a
no-op flag: `cargo clippy --workspace --all-targets -- -A clippy::needless_return`.

## Scope

**In scope**:
- `rust-toolchain.toml` (create)
- `Makefile` — the `lint` comment and the `ci` target, as described in step 4
- `.github/workflows/ci.yml` — the Clippy step only, as described in step 5
- The 11 warning sites, and only to clear their lints:
  - `crates/nebula-tui/src/ui.rs`
  - `crates/nebula-tui/src/markdown.rs`
  - `crates/nebula-tui/src/event_loop.rs`
  - `crates/nebula-daemon/src/pty/kitty.rs`
  - `crates/nebula-daemon/src/lifecycle.rs`

**Out of scope** (do NOT touch, even though they look related):
- The edition. `edition = "2021"` stays. Moving to 2024 is a real migration
  with its own breakage (keyword changes, `unsafe` attribute syntax) and
  belongs in its own plan.
- `.github/workflows/release.yml` — the pin reaches it through `rustup` with
  no edit, and plan 004 is editing that file.
- Any behaviour change at the 11 sites. These are lint fixes; the code must do
  exactly what it did before. If clearing a lint would change behaviour, STOP.
- Any refactor of `event_loop.rs`, `ui.rs` or `app.rs` beyond the four
  `collapsible_match` arms and two `sort_by` calls. Those files are large and
  busy and a drive-by change there is a merge conflict waiting to happen.
- Adding `#![warn(...)]` or `#![deny(...)]` crate attributes, or a
  `[lints]` table in `Cargo.toml`. The gate is the build command.

## Git workflow

- Branch: `pin-the-toolchain-and-gate-on-clippy` (this repo uses bare
  descriptive branch names, sometimes issue-numbered — e.g.
  `fix-106-open-pr-list-stale`, `dead-code-sweep-91`).
- Three commits reads well here: the pin, the warning fixes, then the gate.
  This repo does **not** use conventional commits; messages are descriptive
  sentences, capitalised, no type prefix. For example:
  `One pinned toolchain for every build, the eleven clippy warnings cleared under it, and a warning now fails the gate instead of advising`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Confirm the starting point

```sh
make ci
cargo clippy --workspace --all-targets -- -A clippy::needless_return 2>&1 | grep -c '^warning: [a-z]'
```

**Verify**: `make ci` exits 0, and the warning count is 11. Record the exact
count and the output of `rustc --version`.

If `make ci` fails, STOP: plans 001 and 002 are the prerequisites.

If the count is not 11, that is fine — your toolchain differs from the one
this plan was validated on. Record the real list (lint name, file, line) and
carry it forward instead of the table above. If the count is **0**, STOP:
somebody already did step 3.

### Step 2: Pin the toolchain

Take the version to pin from the current stable:

```sh
rustup update stable
rustc --version
```

Create `rust-toolchain.toml` in the repository root, substituting the exact
version `rustc --version` printed (for example `1.88.0`):

```toml
# One compiler for every build — this checkout, every contributor's, CI's
# gate, and the release matrix (rustup honours this file for the
# `cargo build --target …` in .github/workflows/release.yml too, so the
# published binaries are reproducible).
#
# This is what lets the lint gate be strict: `make lint STRICT=1` and the CI
# Clippy step both fail on a warning, which is only safe when a new clippy
# release cannot change the answer under you. Moving the pin is therefore a
# deliberate commit of its own — bump the channel, run `make lint STRICT=1`,
# and clear whatever the newer clippy has started noticing in the same change.
[toolchain]
channel = "<the version rustc --version printed>"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

**Verify**:
- `rustup show active-toolchain` → names the pinned version and says
  `(overridden by '<repo>/rust-toolchain.toml')`.
- `rustc --version` → the pinned version.
- `cargo fmt --all -- --check` → exit 0.
- `cargo check --workspace --all-targets` → exit 0.

If `cargo check` now fails under the pinned toolchain, STOP and report the
errors — a compile break is not something to fix inside a lint plan.

### Step 3: Clear every warning

Re-run clippy under the pinned toolchain and work from *that* list:

```sh
cargo clippy --workspace --all-targets -- -A clippy::needless_return
```

**3a — the auto-fixable ones.** Commit or stash nothing first; `cargo
clippy --fix` requires a clean tree, so run it before hand-editing.

```sh
cargo clippy --fix --lib -p nebula-tui
cargo clippy --fix --lib -p nebula-daemon
```

Read the resulting diff (`git diff`). Every hunk must be a pure lint fix.
`collapsible_match` merges an inner `if let` into the outer `match` arm's
pattern; `byte_char_slices` rewrites `[b'0']`-style slices as `*b"0"`. If any
hunk changes control flow or a value, revert that hunk and do it by hand.

**Verify**: `cargo test --workspace --locked` → exit 0. Then
`cargo fmt --all -- --check` → exit 0 (run `cargo fmt --all` if `--fix`
left formatting off).

**3b — the rest, by hand.** On 1.88.0 that is four sites:

- `crates/nebula-tui/src/ui.rs:1511` and `:1515` —
  `clippy::unnecessary_sort_by`. A `sort_by` whose closure is just
  `|a, b| key(a).cmp(&key(b))` becomes `sort_by_key(|x| key(x))`. Keep the
  sort *stable* and the key identical; if the key borrows and `sort_by_key`
  will not type-check, `sort_by_cached_key` is the next option, and an
  `#[allow]` with a one-line reason is the last.
- `crates/nebula-tui/src/markdown.rs:1029` — `clippy::manual_checked_ops`. A
  hand-written zero check in front of a division becomes `checked_div`. The
  result for a zero divisor must stay exactly what it is today; read the
  surrounding code before rewriting.
- `crates/nebula-daemon/src/lifecycle.rs:175` —
  `clippy::chunks_exact_to_as_chunks`. `chunks_exact(N)` with a constant `N`
  becomes `as_chunks::<N>()`. Mind the remainder: `as_chunks` returns the tail
  alongside the chunks, where `chunks_exact` drops it unless
  `.remainder()` is called. Preserve the existing handling of a partial final
  chunk.

Each of these sites sits in code with explanatory comments. If a comment
describes the construct you are rewriting, update the comment in the same
edit — a stale comment is treated as a defect in this repository.

**Verify after each file**:
- `cargo clippy --workspace --all-targets -- -A clippy::needless_return 2>&1 | grep -c '^warning: [a-z]'`
  → decreasing, and `0` when you are done.
- `cargo test --workspace --locked` → exit 0.

### Step 4: Make the Makefile gate strict

Two edits to `Makefile`.

First, the comment above `lint`. Replace the opening sentence —

```
# Not `-D warnings` by default, so a lint a new toolchain adds doesn't fail
# the gate on its own; the workspace does clear that bar today, so keep it
# that way.
```

— with:

```
# Lenient by default so `make lint` can be read as advice while you work.
# The gate is strict: `make ci` and the CI workflow both run
# `make lint STRICT=1`, which is only safe because rust-toolchain.toml pins
# the compiler — an unpinned toolchain could add a lint and fail the gate
# with nobody having changed a line.
```

Keep the rest of that comment (the sentence plan 002 added about
`.github/workflows/ci.yml`, and the `make lint STRICT=1` note) and keep the
`lint` recipe itself exactly as it is.

Second, the `ci` target. Change its middle line from

```make
	@$(MAKE) --no-print-directory lint
```

to

```make
	@$(MAKE) --no-print-directory lint STRICT=1
```

**Verify**:
- `make ci` → exit 0, and clippy prints no warnings.
- `grep -n 'no-print-directory lint STRICT=1' Makefile` → shows the `ci` recipe line.
- `make lint` (no `STRICT`) → still exit 0.

### Step 5: Make the CI workflow strict

In `.github/workflows/ci.yml` (created by plan 002), the Clippy step reads:

```yaml
      # Deliberately not `-D warnings`. See the `lint` target in the Makefile:
      # a lint a new toolchain adds would otherwise fail this gate on its own.
      - name: Clippy
        run: cargo clippy --workspace --all-targets
```

Replace it with:

```yaml
      # `-D warnings` is safe now that rust-toolchain.toml pins the compiler:
      # a new clippy release cannot turn this red until somebody moves the
      # pin, and clearing the new lints is part of that commit.
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
```

Do not add a `dtolnay/rust-toolchain` or `actions-rs` step: `rustup` is
preinstalled on both runner images and reads `rust-toolchain.toml` on the
first `cargo` invocation, installing the pinned version if the runner does not
have it.

**Verify**:
- `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`
  → no output, exit 0.
- `grep -c -- '-- -D warnings' .github/workflows/ci.yml` → `1`.

### Step 6: Final check

**Verify**:
- `make ci` → exit 0.
- `git status --short` lists only: `rust-toolchain.toml` (new), `Makefile`,
  `.github/workflows/ci.yml`, and the warning-site files you actually edited.
  Nothing else.

## Test plan

No new tests. Every edit in step 3 is behaviour-preserving by construction, and
the existing suite is the proof: `cargo test --workspace --locked` must pass
after each file, not just at the end. The `nebula-tui` library alone has 1,266
tests over the files you are touching, and `kitty.rs` and `lifecycle.rs` are
covered by the daemon's 308.

The regression guard for the gate itself is the gate: after this plan a
warning fails `make ci` and the CI job, which is the behaviour being added.

Do not write a test that asserts the warning count is zero. It would duplicate
the gate and break on every pin bump.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `rust-toolchain.toml` exists, and `rustup show active-toolchain` reports it as an override
- [ ] `grep -c '^channel = ' rust-toolchain.toml` returns 1, and `grep -c '^components = ' rust-toolchain.toml` returns 1
- [ ] `cargo clippy --workspace --all-targets -- -A clippy::needless_return 2>&1 | grep -c '^warning: [a-z]'` returns `0`
- [ ] `make lint STRICT=1` exits 0
- [ ] `cargo fmt --all -- --check` exits 0
- [ ] `cargo test --workspace --locked` exits 0
- [ ] `make ci` exits 0
- [ ] `grep -c 'no-print-directory lint STRICT=1' Makefile` returns 1
- [ ] `grep -c -- '-- -D warnings' .github/workflows/ci.yml` returns 1
- [ ] `git diff -U0 | grep '^+.*allow(clippy'` prints nothing — no lint was silenced instead of fixed (the 14 pre-existing allows stay as they are)
- [ ] `git status --short` contains no file outside the in-scope list
- [ ] `plans/README.md` status row for 003 updated

## STOP conditions

Stop and report back (do not improvise) if:

- `make ci` fails at step 1, or `.github/workflows/ci.yml` does not exist —
  plans 001 and 002 are prerequisites.
- `cargo check --workspace --all-targets` fails after the pin lands. A compile
  error under the pinned toolchain is a real finding and not this plan's job.
- Clearing any lint would change what the code does. Report the site, the
  lint, and what the behaviour difference would be. A behaviour change smuggled
  into a lint fix is the worst outcome of this plan.
- `cargo clippy --fix` produces a hunk you cannot explain as a pure lint
  rewrite. Revert it and say which one.
- A lint can only be cleared by `#[allow]`. Report the site and your reasoning
  rather than adding it — the `allow` may well be right, but it is a judgment
  call and the operator should make it.
- Your pinned toolchain reports more than about 20 warnings. That is a larger
  cleanup than this plan scoped, and the list should be triaged before anyone
  starts editing.
- Any test fails after a lint fix and still fails after one reasonable
  correction attempt.

## Maintenance notes

- **Moving the pin is now a deliberate act.** The procedure: bump `channel` in
  `rust-toolchain.toml`, run `make lint STRICT=1`, clear whatever the newer
  clippy reports, and commit all of it together. Doing the bump without the
  cleanup is what turns the gate red for everybody else.
- The pin reaches `release.yml` implicitly through `rustup`. Anyone who wants
  release binaries built by a newer compiler now changes `rust-toolchain.toml`,
  not the workflow — worth knowing before someone edits `release.yml` looking
  for the toolchain.
- `make lint` stays lenient on purpose, so reading clippy's advice mid-change
  does not mean fighting the gate. Only `make ci` and CI are strict.
- A reviewer should scrutinise the step-3 diff hardest: each hunk must be
  explainable as the named lint's rewrite and nothing else, and any comment
  describing rewritten code must have been updated with it.
- Deliberately deferred: the edition bump to 2024, and a `[lints]` table in
  `Cargo.toml` (which would let the gate live in the manifest rather than the
  build command). Both are reasonable follow-ups; neither belongs in the same
  change as the pin.
