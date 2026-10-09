# Plan 005: A security advisory in any dependency reaches you without being asked

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 565080d..HEAD -- Cargo.toml Cargo.lock .github vendor/vt100/Cargo.toml`
> If any of those changed since this plan was written, compare the "Current
> state" excerpts below against the live files before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/002-add-ci-test-workflow.md`
- **Category**: deps
- **Planned at**: commit `565080d`, 2026-10-06

## Why this matters

`Cargo.lock` pins 287 packages, among them `axum`, `rusqlite` (with a bundled
SQLite), `portable-pty`, `nix`, `rand` and `subtle`. Nothing in the repository
or its workflows checks any of them against the RustSec advisory database, and
there is no `dependabot.yml` or `renovate.json` — so a published advisory in a
dependency on the daemon's loopback HTTP surface, its SQLite store or its PTY
layer arrives only if somebody happens to read about it.

Twelve packages are already resolved at two or three versions each
(`bitflags`, `getrandom` ×3, `nix` ×3, `rand`, `syn` ×3, `thiserror`, and
others), which is ordinary transitive drift — but it means an advisory can
apply to an old copy pulled in sideways, which is exactly the case nobody
notices by reading release notes.

The gap has a second half. `Cargo.toml:6-11` vendors a patched `vt100`:

```toml
# Vendored vt100 with one patch: rows scrolled out of a top-anchored DECSTBM
# region on the primary screen go into scrollback (upstream discards them).
# Without it, codex's ratatui inline viewport leaves the TUI scrollback empty
# — wheel-up over a codex agent did nothing. See vendor/vt100/Cargo.toml.
[patch.crates-io]
vt100 = { path = "vendor/vt100" }
```

That is a deliberate, well-documented decision and not a finding. But
`vendor/vt100/Cargo.toml:8` adds `version must stay 0.16.2`, and nothing in
the repository will ever tell anyone that upstream has moved. A dependency
update tool cannot help with a path dependency, so that one needs a human
pointer rather than a bot.

## Current state

### Nothing checks advisories, nothing proposes updates

- `ls .github/` → `workflows` only. No `dependabot.yml`.
- No `renovate.json` at the root.
- Neither `cargo-audit` nor `cargo-deny` is referenced anywhere in the
  repository — not in `Makefile`, not in any workflow.
- `.github/workflows/` holds `release.yml`, `claude.yml`,
  `claude-code-review.yml`, and (after plan 002) `ci.yml`.

Confirm with:

```sh
ls .github
grep -rl 'cargo audit\|cargo-audit\|cargo deny\|cargo-deny' . --exclude-dir=target --exclude-dir=.git
```

The second command should print only `plans/` files.

**This plan contains no advisory data.** `cargo-audit` was not installed when
the plan was written, so the current advisory status of these 287 packages is
unknown. Step 1 establishes it, and that is the first thing you report.

### The dependency surface

Workspace members and their direct non-workspace dependencies, from the
manifests at `565080d`:

- `nebula-daemon/Cargo.toml` — `rusqlite 0.40` (feature `bundled`),
  `axum 0.8`, `nix 0.31` (`process`, `fs`, `signal`, `term`),
  `tokio-util 0.7`, `rand 0.10`, `subtle 2`, plus the workspace set.
- `nebula-tui/Cargo.toml` — `ratatui 0.30`, `crossterm 0.29`, `tui-term 0.3`,
  `futures 0.3`, `base64 0.22`, `pulldown-cmark 0.13`, `unicode-width 0.2`.
- `Cargo.toml:18-36` — the workspace set: `serde`, `serde_json`,
  `serde_bytes`, `rmp-serde`, `tokio`, `anyhow`, `tracing`,
  `tracing-subscriber`, `ulid`, `directories`, `portable-pty`, `vt100`,
  `tempfile`.

`Cargo.toml:4` excludes the vendored crate from the workspace:

```toml
exclude = ["vendor/vt100"]
```

so `cargo audit` reads only the workspace's `Cargo.lock`. The lockfile's
`vt100` entry has no `source` field — it resolves to the local path — which
means it is very likely invisible to advisory matching. Step 2 establishes
whether that is actually so rather than assuming it.

### Where the new check belongs, and why not in `ci.yml`

Plan 002's maintenance notes anticipated this plan adding a job to
`.github/workflows/ci.yml`. **Do not.** Put it in its own workflow instead,
for two reasons that only became clear once the shape was settled:

1. An advisory is published upstream, not by a push. `ci.yml` runs on `push`
   and `pull_request`, which cannot express "check again next week". A
   `schedule:` trigger can, and that is the trigger that matters most here.
2. A newly published advisory in a transitive dependency should not turn an
   unrelated pull request red. Keeping it separate means the signal lands
   where it belongs — on the dependency work — instead of blocking whoever
   pushed next.

Say so in the workflow's own comment, and fix plan 002's note (step 5).

### Repo conventions that apply here

- Every non-obvious decision carries a comment explaining *why*, often several
  lines. `Cargo.toml:6-11`, `Cargo.toml:39-54` and
  `.github/workflows/claude-code-review.yml:15-20` are the house style. Match
  that density — a schedule, a group, or an `ignore` with no stated reason is
  not in keeping with this codebase.
- Workflow actions are pinned to a major version (`actions/checkout@v4`,
  `actions/upload-artifact@v4`, `softprops/action-gh-release@v2`).
- Workflow files are lowercase-hyphenated `.yml` under `.github/workflows/`.

## Commands you will need

| Purpose                   | Command                                                                   | Expected on success               |
|---------------------------|---------------------------------------------------------------------------|-----------------------------------|
| Install the auditor       | `cargo install cargo-audit --locked`                                      | exit 0                            |
| Audit                     | `cargo audit`                                                             | exit 0, `0 vulnerabilities found` |
| Audit, JSON               | `cargo audit --json`                                                      | parseable JSON                    |
| What the audit saw        | `cargo audit --json \| python3 -c "import json,sys; d=json.load(sys.stdin); print(d['lockfile']['dependency-count'])"` | a number near 287 |
| Lockfile package count    | `grep -c '^\[\[package\]\]' Cargo.lock`                                   | `287`                             |
| Workflow YAML syntax      | `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/audit.yml'))"` | no output, exit 0            |
| Dependabot YAML syntax    | `python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"`| no output, exit 0                 |
| The whole gate            | `make ci`                                                                 | exit 0                            |

`cargo install cargo-audit --locked` writes a binary into `~/.cargo/bin` and
takes a few minutes. It is the only thing in this plan that touches anything
outside the repository.

## Scope

**In scope**:
- `.github/workflows/audit.yml` (create)
- `.github/dependabot.yml` (create)
- `Makefile` — one new `audit` target, as described in step 4
- `vendor/vt100/Cargo.toml` — two added comment lines, step 3
- `plans/002-add-ci-test-workflow.md` — correct its deferred-work note, step 5

**Out of scope** (do NOT touch, even though they look related):
- `Cargo.toml`, any crate's `Cargo.toml`, and `Cargo.lock`. **Do not run
  `cargo update`, and do not bump a dependency version.** This plan installs
  the signal; acting on what it reports is separate work with its own testing.
- `vendor/vt100/src/**` — the patch itself. Nothing here changes the vendored
  code.
- `.github/workflows/ci.yml` — plan 003 edits the Clippy step there; this plan
  adds a new file instead (see above).
- `.github/workflows/release.yml` — plan 004 is editing it.
- `cargo-deny` and its `deny.toml`. It does more than advisories (licences,
  duplicate-version bans, source allow-lists) and each of those is a policy
  decision somebody has to make. `cargo-audit` answers the one question this
  plan is about. See the maintenance notes.
- Any `.rs` file.

## Git workflow

- Branch: `watch-dependency-advisories` (this repo uses bare descriptive
  branch names, sometimes issue-numbered — e.g. `fix-106-open-pr-list-stale`).
- Two commits reads well: the advisory check, then the update bot. This repo
  does **not** use conventional commits; messages are descriptive sentences,
  capitalised, no type prefix. For example:
  `A weekly advisory sweep over the 287 locked crates and a bot that proposes the updates, so a published vulnerability arrives without anyone having to read about it`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Find out where the tree actually stands

This is the first thing to report, whatever else happens.

```sh
cargo install cargo-audit --locked
cargo audit
```

**Verify**: record the full output verbatim — the advisory count, and for each
advisory its RUSTSEC id, the package, the affected and patched versions, and
the dependency path `cargo audit` prints.

- **Zero vulnerabilities** → carry on to step 2.
- **One or more** → finish nothing else; see STOP conditions. Do not add
  `--ignore RUSTSEC-…`, do not bump a version, do not add the workflow with
  the finding suppressed. A real advisory is more important than the rest of
  this plan and it needs the operator's decision.

Also capture the baseline, so the workflow can be compared against it:

```sh
grep -c '^\[\[package\]\]' Cargo.lock
cargo audit --json | python3 -c "import json,sys; d=json.load(sys.stdin); print('audited:', d['lockfile']['dependency-count'])"
```

**Verify**: the lockfile count is 287 (or whatever HEAD now holds), and the
audited count is in the same region.

### Step 2: Establish whether the vendored `vt100` is audited at all

```sh
cargo audit --json | python3 -c "
import json,sys
d = json.load(sys.stdin)
names = [p['name'] for p in d.get('lockfile', {}).get('packages', [])] if isinstance(d.get('lockfile'), dict) else []
print('vt100 in audited set:', 'vt100' in names)
"
grep -A3 'name = \"vt100\"' Cargo.lock
```

**Verify**: record the answer. If the JSON shape does not expose a package
list, say so and fall back to the plain-text evidence: `Cargo.lock`'s `vt100`
entry has no `source` line, which means it resolves through
`[patch.crates-io]` to `vendor/vt100` and advisory matching has no registry
version to match against.

Either way the conclusion is the same and step 3 acts on it. The point of
measuring is that the comment you write in step 3 should state what is true,
not what is assumed.

### Step 3: Leave a human pointer on the vendored crate

No tool watches a path dependency, so the note is the mechanism.

`vendor/vt100/Cargo.toml:1-8` currently reads:

```toml
# Vendored from crates.io vt100 0.16.2 (MIT, https://github.com/doy/vt100-rust)
# with one Nebula patch — see src/grid.rs `scroll_up`: lines scrolled out of a
# TOP-ANCHORED DECSTBM region on the primary screen are pushed into scrollback
# (upstream discards them). Ratatui inline-viewport apps (codex) insert chat
# history that way, and real terminals keep those lines; without the patch the
# TUI's codex scrollback stays empty. Wired in via [patch.crates-io] in the
# workspace Cargo.toml. Dev-dependencies dropped; version must stay 0.16.2 so
# the patch satisfies the crates' `vt100 = "0.16"` requirement.
```

Append to that comment block:

```toml
#
# Nothing automatic watches this copy: `[patch.crates-io]` makes it a path
# dependency, so Cargo.lock carries no registry source for it and neither
# `cargo audit` nor Dependabot can see it. Upstream fixes and advisories reach
# this file only by somebody checking
# https://github.com/doy/vt100-rust/releases and re-applying the patch onto a
# newer 0.16.x. Worth doing whenever the terminal emulation is being touched
# anyway.
```

Adjust the first sentence if step 2 found that `cargo audit` *does* see it —
write what you measured.

**Verify**:
- `cargo check --workspace` → exit 0 (a comment cannot break it; this confirms
  you did not disturb the manifest).
- `grep -c 'vt100-rust/releases' vendor/vt100/Cargo.toml` → `1`.

### Step 4: Add the advisory workflow and a `make audit` target

Create `.github/workflows/audit.yml`:

```yaml
name: Audit

# An advisory is published upstream, not by a push — so the schedule is the
# trigger that matters, and a lockfile change is the one that cannot wait for
# it. Deliberately NOT a job in ci.yml: `on: schedule` has no place there, and
# an advisory published in a transitive dependency should land on the
# dependency work rather than turn somebody's unrelated pull request red.
on:
  schedule:
    # Mondays, 07:00 UTC — a week's advisories waiting at the start of one.
    - cron: "0 7 * * 1"
  push:
    branches: [main]
    paths:
      - "**/Cargo.toml"
      - Cargo.lock
      - .github/workflows/audit.yml
  pull_request:
    paths:
      - "**/Cargo.toml"
      - Cargo.lock
      - .github/workflows/audit.yml
  workflow_dispatch:

permissions:
  contents: read

jobs:
  audit:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4

      # The vendored vt100 is a path dependency through [patch.crates-io], so
      # Cargo.lock carries no registry source for it and this sweep cannot
      # see it. vendor/vt100/Cargo.toml says who watches it instead.
      - uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
```

Then add a target to `Makefile`, under the `# --- checks ---` section
(`Makefile:219`), after `test` and before `ci`:

```make
# The same sweep .github/workflows/audit.yml runs weekly, for when you want
# the answer now. Needs `cargo install cargo-audit --locked` once. Not part
# of `ci`: an advisory published upstream this morning is not a reason for
# today's commit to fail, and the workflow is what makes sure it is seen.
audit: ## Check the locked dependencies against the RustSec advisory database
	cargo audit
```

Add `audit` to the `.PHONY` list at `Makefile:56`.

**Verify**:
- `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/audit.yml'))"`
  → no output, exit 0.
- `make audit` → exit 0, `0 vulnerabilities found`.
- `make help` → lists `audit` with its description (the `help` target greps
  `## ` comments, `Makefile:58-60`).
- `make ci` → exit 0, and its output does **not** include the audit.

### Step 5: Add Dependabot

Create `.github/dependabot.yml`:

```yaml
version: 2

updates:
  # Cargo. Grouped and weekly on purpose: 287 locked packages ungrouped is a
  # pull request a day and a habit of ignoring them. Patch and minor bumps
  # arrive as one pull request that CI either passes or doesn't; a major bump
  # comes on its own, because that is the one worth reading.
  - package-ecosystem: cargo
    directory: "/"
    schedule:
      interval: weekly
      day: monday
    open-pull-requests-limit: 5
    groups:
      minor-and-patch:
        update-types: ["minor", "patch"]
    # The patched copy under vendor/ is a path dependency, so nothing here can
    # update it even if it were listed — see vendor/vt100/Cargo.toml.
    ignore:
      - dependency-name: vt100

  # The workflows pin actions to a major version, which still leaves the
  # action itself unmaintained-and-unnoticed. Four files, so no grouping.
  - package-ecosystem: github-actions
    directory: "/"
    schedule:
      interval: weekly
      day: monday
```

Finally, correct the note plan 002 left behind. In
`plans/002-add-ci-test-workflow.md`, the maintenance notes say:

> Deliberately deferred: strict clippy and a pinned toolchain (plan 003), and
> dependency advisories (plan 005). Both add jobs to this same file, so expect
> to touch it again.

Replace that with:

> Deliberately deferred: strict clippy and a pinned toolchain (plan 003),
> which edits the Clippy step in this file. Dependency advisories (plan 005)
> went into their own `audit.yml` instead — a `schedule:` trigger has no place
> here, and an upstream advisory should not fail an unrelated pull request.

**Verify**:
- `python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"`
  → no output, exit 0.
- `grep -c 'package-ecosystem' .github/dependabot.yml` → `2`.
- `grep -c 'add jobs to this same file' plans/002-add-ci-test-workflow.md`
  → `0`.

### Step 6: Final check

**Verify**:
- `make ci` → exit 0.
- `git status --short` lists exactly five paths:
  `.github/workflows/audit.yml` (new), `.github/dependabot.yml` (new),
  `Makefile`, `vendor/vt100/Cargo.toml`, and
  `plans/002-add-ci-test-workflow.md`.
- `git diff -- Cargo.lock Cargo.toml crates` → **empty**. No dependency was
  moved by this plan.

## Test plan

No unit tests. The artefacts are two YAML files, a Makefile target and a
comment; verification is that each one parses and does what it claims:

1. `cargo audit` runs clean locally (step 1) — the measured baseline the
   workflow must agree with.
2. `make audit` exits 0 and `make help` lists it (step 4).
3. `make ci` exits 0 and does not include the audit — the audit is a signal,
   not a build gate (step 4).
4. Both YAML files parse (steps 4, 5).
5. `git diff -- Cargo.lock Cargo.toml crates` is empty (step 6) — the strongest
   check in this plan, because the easy mistake here is to "fix" something
   `cargo audit` said while adding the thing that said it.

Two verifications need GitHub and belong to the operator; say so in your
report:

- `workflow_dispatch` the `Audit` workflow once and confirm the job is green.
- Confirm Dependabot is enabled for the repository (Settings → Code security).
  `.github/dependabot.yml` alone does nothing if the feature is off, and the
  file will sit there looking like coverage it is not providing.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo audit` exits 0 and reports `0 vulnerabilities found`
- [ ] `.github/workflows/audit.yml` exists and `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/audit.yml'))"` exits 0
- [ ] `.github/dependabot.yml` exists and `python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"` exits 0
- [ ] `grep -c 'cron' .github/workflows/audit.yml` returns 1
- [ ] `grep -c 'package-ecosystem' .github/dependabot.yml` returns 2
- [ ] `grep -l -- '--ignore' .github/workflows/audit.yml Makefile` prints nothing — nothing was suppressed
- [ ] `make audit` exits 0
- [ ] `make help` lists `audit`
- [ ] `make ci` exits 0 and does not run the audit
- [ ] `grep -c 'vt100-rust/releases' vendor/vt100/Cargo.toml` returns 1
- [ ] `grep -c 'add jobs to this same file' plans/002-add-ci-test-workflow.md` returns 0
- [ ] `git diff -- Cargo.lock Cargo.toml crates` produces no output
- [ ] `git status --short` lists only the five in-scope paths
- [ ] `plans/README.md` status row for 005 updated

## STOP conditions

Stop and report back (do not improvise) if:

- **`cargo audit` reports any advisory.** Stop at step 1 and report every one:
  RUSTSEC id, package, affected and patched versions, and the dependency path.
  Do not add `--ignore`, do not bump anything, and do not add the workflow
  with the finding hidden. Whether to patch, wait, or accept is the operator's
  call and it is a bigger decision than this plan.
- `cargo install cargo-audit --locked` fails. Report the error; it is the one
  step that writes outside the repository and it may be an environment problem
  rather than a repository one.
- `cargo audit` reports a *yanked* crate rather than an advisory. That is a
  different class of problem with a different fix. Report it separately and do
  not act on it here.
- Adding any of this appears to require a change to `Cargo.toml`, a crate
  manifest, or `Cargo.lock`.
- `Cargo.toml:4-11` or `vendor/vt100/Cargo.toml:1-8` do not match the
  excerpts above.
- `plans/002-add-ci-test-workflow.md` has no maintenance note matching step
  5's quote — say so and leave it alone rather than rewriting a note you
  cannot find.
- `make ci` fails. This plan changes no Rust, so that means something out of
  scope moved.

## Maintenance notes

- **The signal is only worth having if somebody acts on it.** A weekly job
  that goes red and stays red is worse than no job, because it teaches people
  that red is normal — which is the same failure mode that let two tests stay
  broken for 35 commits (plan 001). Whoever owns this should expect to handle
  a `minor-and-patch` group pull request most weeks.
- Dependabot needs enabling in the repository settings. The file is necessary
  and not sufficient.
- `cargo-deny` is the obvious next step and was deliberately left out: beyond
  advisories it checks licences, bans duplicate versions and allow-lists
  sources, and each of those needs a policy somebody has decided. The twelve
  packages currently resolved at multiple versions make its `bans` check
  immediately noisy, so adopting it means doing that triage — fine as its own
  plan, wrong as a side effect of this one.
- The vendored `vt100` stays outside every automatic check as long as it is a
  path dependency. The comment added in step 3 is the whole mechanism;
  anybody deleting it is removing the only thing that remembers.
- A reviewer should check two things: that `Cargo.lock` is untouched, and that
  nothing is suppressed with `--ignore` or an `ignore:` entry beyond the
  `vt100` one, which exists because the tool cannot act on it rather than
  because the answer was unwelcome.
- `audit.yml` runs on pull requests only when a manifest or the lockfile
  changes, so a Dependabot pull request gets both this and the full `ci.yml`
  gate. That is the intended pairing: the bot proposes, `ci.yml` says whether
  it compiles and passes, `audit.yml` says whether it was worth proposing.
