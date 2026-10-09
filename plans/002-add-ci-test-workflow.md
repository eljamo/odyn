# Plan 002: Every push and pull request runs format, lint and the test suite

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 565080d..HEAD -- .github/workflows Makefile`
> If either changed since this plan was written, compare the "Current state"
> excerpts below against the live files before proceeding; on a mismatch,
> treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/001-fix-stale-locked-pane-footer-assertion.md`
- **Category**: dx
- **Planned at**: commit `565080d`, 2026-10-06

## Why this matters

This repository has 1,623 library tests and 62 integration tests, and nothing
runs them automatically. `.github/workflows/` holds a release builder and two
Claude automations; none of them invokes `cargo test`, `cargo fmt` or `clippy`.

The concrete cost is already on the books: two end-to-end tests broke when a
footer string changed on 2026-09-23 and stayed broken through 35 commits and
two releases, because no one is told. Plan 001 fixes those two; this plan makes
the next one visible within minutes instead of weeks.

The gate is cheap. Measured at `565080d` on an Apple-silicon laptop: library
tests 5 s, integration tests 54 s, `cargo fmt --all -- --check` instant. There
is no reason for this suite not to run on every push.

## Current state

### What is in `.github/workflows/` today

Three files, none of which runs a check:

- `release.yml` — builds a 4-target matrix on `push` of a `v*` tag and on
  `workflow_dispatch`, then publishes a GitHub release. Relevant excerpt,
  `release.yml:30-41`:

  ```yaml
      steps:
        - uses: actions/checkout@v4

        - name: Install musl toolchain
          if: contains(matrix.target, 'musl')
          run: sudo apt-get update && sudo apt-get install -y musl-tools

        - name: Add rust target
          run: rustup target add ${{ matrix.target }}

        - name: Build
          run: cargo build --release --locked --target ${{ matrix.target }}
  ```

  Note `--locked`: the release build already refuses to update `Cargo.lock`.
  The new workflow should match that, so lockfile drift fails a pull request
  rather than a tag.

- `claude.yml` — responds to `@claude` in issues and review comments.
- `claude-code-review.yml` — reviews pull requests. It carries a convention
  worth copying, `claude-code-review.yml:15-20`:

  ```yaml
      # A pull request from a fork runs without the repository's secrets and
      # without an OIDC token (GitHub withholds both from `pull_request` runs of
      # foreign code), so the action can't authenticate and every contributor PR
      # shows a red check. Skip the job there: the check reports "skipped"
      # instead of "failure", and same-repo branches are reviewed as before.
      if: github.event.pull_request.head.repo.full_name == github.repository
  ```

  **The workflow you are adding must NOT copy that `if:`.** It needs no
  secrets, so it can and should run on fork pull requests — that is where an
  automated check is worth the most.

### The gate the repo already defines

`Makefile:219-240`:

```make
check: ## Typecheck the workspace (fastest feedback)
	cargo check --workspace --all-targets

fmt: ## Format the workspace
	cargo fmt --all

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

Two things follow from that comment. First, `make lint` does **not** fail on
warnings today, and there are 11 clippy warnings at `565080d` — so this
workflow must run clippy *without* `-D warnings`, or it is born red. Making it
strict is plan 003's job, and that plan clears the 11 warnings first. Second,
the comment's claim "CI runs no clippy at all" stops being true with this
plan; update it (step 4).

### What the suite needs from the machine

Verified by reading the tests, not assumed:

- `git` — the harness builds its own repos and sets its own identity per repo,
  so no global `git config` is needed. `crates/nebula/tests/e2e_tui.rs:134-145`:

  ```rust
      fn make_repo(&self, name: &str) -> PathBuf {
          let repo = self._repos.path().join(name);
          std::fs::create_dir_all(&repo).unwrap();
          let git = |args: &[&str]| repo_git(&repo, args);
          git(&["init", "-b", "main"]);
          git(&["config", "user.email", "t@nebula.dev"]);
          git(&["config", "user.name", "nebula-test"]);
          std::fs::write(repo.join(".keep"), "").unwrap();
          git(&["add", "."]);
          git(&["commit", "-m", "init"]);
          repo
      }
  ```

- A POSIX shell and a PTY. The tests spawn real daemons and real PTYs, but
  into temporary directories: `crates/nebula/tests/e2e_pty.rs:30` takes a
  `tempfile::tempdir()`, and `:43-44` points the child at it:

  ```rust
          cmd.env(env::RUNTIME_DIR, &self.runtime_dir)
              .env(env::DATA_DIR, self.tmp.path().join("data"));
  ```

  So a CI run cannot touch any real nebula state, and two jobs cannot collide.

- `gh` is **not** needed: the tests that exercise GitHub paths write their own
  stub `gh` onto `PATH` (`e2e_tui.rs:546`, `:610`).

- `ps`, `kill`, `setsid`/`flock` equivalents — present on both GitHub runner
  images.

### Repo conventions that apply here

- Workflow files are lowercase-hyphenated `.yml` under `.github/workflows/`.
- Every non-obvious decision in this repository carries a comment explaining
  *why*, often several lines. `release.yml:6-8` and
  `claude-code-review.yml:15-20` are the house style for workflow comments —
  match that density. A step whose reason is not obvious gets a comment.
- `release.yml` pins actions to a major version (`actions/checkout@v4`,
  `actions/upload-artifact@v4`). Do the same.

## Commands you will need

| Purpose              | Command                                      | Expected on success                 |
|----------------------|----------------------------------------------|-------------------------------------|
| Format check         | `cargo fmt --all -- --check`                 | exit 0, no output                   |
| Typecheck            | `cargo check --workspace --all-targets`      | exit 0                              |
| Lint (non-strict)    | `make lint`                                  | exit 0, ~11 warnings printed         |
| Tests                | `cargo test --workspace --locked`            | exit 0, every target `ok`           |
| The whole gate       | `make ci`                                    | exit 0                              |
| Workflow YAML syntax | `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml'))"` | no output, exit 0 |

## Scope

**In scope**:
- `.github/workflows/ci.yml` (create)
- `Makefile` — one comment correction only, described in step 4

**Out of scope** (do NOT touch, even though they look related):
- `.github/workflows/release.yml` — the release pipeline is a separate
  concern. Plan 004 changes it; two plans editing it is a conflict.
- `.github/workflows/claude.yml`, `.github/workflows/claude-code-review.yml` —
  leave the Claude automations alone.
- `-D warnings` / strict clippy and any `rust-toolchain.toml` — that is plan
  003. Adding it here makes this workflow red on arrival.
- Any `.rs` file. If a test fails in CI, that is a finding to report, not a
  thing to fix inside this plan.
- `cargo audit`, `cargo deny`, `dependabot.yml` — plan 005.

## Git workflow

- Branch: `ci-run-the-test-suite` (this repo uses bare descriptive branch
  names, sometimes issue-numbered — e.g. `fix-106-open-pr-list-stale`).
- One or two commits. This repo does **not** use conventional commits;
  messages are descriptive sentences, capitalised, no type prefix. For example:
  `Every push and pull request runs fmt, clippy and the whole test suite, so a broken assertion is caught in minutes instead of releases`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Confirm the gate is green locally first

A workflow added on top of a red suite is worse than no workflow.

**Verify**: `make ci` → exit 0. If it fails, STOP: plan 001 has not landed, or
something else regressed.

Also note the clippy warning count you see, so you can tell in step 3 whether
CI agrees: `make lint 2>&1 | grep -c '^warning: '`.

### Step 2: Write `.github/workflows/ci.yml`

Create the file with this content. The shape is deliberate; the notes after it
explain each choice, and the comments belong in the file.

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

# A second push to the same branch makes the first run's answer worthless, so
# cancel it. `main` is exempt: its runs are the record of what landed.
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: ${{ github.ref != 'refs/heads/main' }}

permissions:
  contents: read

jobs:
  check:
    name: check (${{ matrix.runner }})
    runs-on: ${{ matrix.runner }}
    strategy:
      # Both platforms are shipped, so both answers are wanted even when one
      # of them is already red.
      fail-fast: false
      matrix:
        runner: [ubuntu-24.04, macos-latest]
    steps:
      - uses: actions/checkout@v4

      # The vendored vt100 under vendor/ is wired in through
      # [patch.crates-io], so it is part of every build and belongs in the
      # cache key — which this action takes from the lockfile.
      - uses: Swatinem/rust-cache@v2

      # Cheap and total: a diff here needs no compile to find.
      - name: Format
        run: cargo fmt --all -- --check

      # Deliberately not `-D warnings`. See the `lint` target in the Makefile:
      # a lint a new toolchain adds would otherwise fail this gate on its own.
      - name: Clippy
        run: cargo clippy --workspace --all-targets

      # `--locked` for the same reason the release build uses it: a pull
      # request that moves Cargo.lock should say so here, not at tag time.
      # e2e_pty and e2e_tui spawn real daemons and real PTYs, but each into
      # its own tempdir (NEBULA_RUNTIME_DIR / NEBULA_DATA_DIR), so nothing is
      # shared between jobs. About 60s of the roughly four minutes this job
      # takes cold.
      - name: Test
        run: cargo test --workspace --locked
```

Why each piece:

- **`on: push: branches: [main]` plus `pull_request:`** — a branch that is
  pushed and then opened as a pull request would otherwise run twice.
- **No `if:` guard on fork pull requests.** Unlike `claude-code-review.yml`,
  this job uses no secrets and no OIDC token, so it runs fine on a
  contributor's fork. That is the case where the check earns the most.
- **`permissions: contents: read`** — the job only reads the repo.
- **Both runners.** macOS is the platform this was verified green on;
  `ubuntu-24.04` matches the Linux runner `release.yml` already uses. See the
  STOP conditions about what to do if Linux fails — do not quietly drop it.
- **`Swatinem/rust-cache@v2`** — a cold build of this workspace is the bulk of
  the runtime. The cache keys on the lockfile.
- **No `cargo check` step** — `cargo clippy --workspace --all-targets` already
  type-checks everything `check` would.

**Verify**:
`python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`
→ no output, exit 0.

### Step 3: Prove the same commands pass locally, in the workflow's order

Run exactly what the workflow runs, in order:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo test --workspace --locked
```

**Verify**: all three exit 0. The clippy step prints warnings — that is
expected and is not a failure; the count should match what you noted in step 1
(11 at `565080d`). `cargo test --workspace --locked` must not report that
`Cargo.lock` needs updating.

### Step 4: Correct the Makefile comment this plan invalidates

`Makefile:227-230` currently says, in the comment above the `lint` target:

```make
# Not `-D warnings` by default, so a lint a new toolchain adds doesn't fail
# the gate on its own; the workspace does clear that bar today, so keep it
# that way. CI runs no clippy at all. `make lint STRICT=1` opts into the
# stricter gate.
```

Replace the sentence `CI runs no clippy at all.` with:
`CI runs this same non-strict clippy on every push and pull request
(.github/workflows/ci.yml).`

Leave the rest of the comment and the recipe exactly as they are. This repo
treats a comment that no longer describes reality as a defect, which is why it
is a step and not an afterthought.

**Verify**: `grep -n 'CI runs' Makefile` → shows the new sentence, and
`grep -c 'CI runs no clippy' Makefile` → `0`.

Then `make ci` → exit 0 (the Makefile still parses and behaves the same).

### Step 5: Final check

**Verify**: `git status --short` lists exactly two paths:
`.github/workflows/ci.yml` (new) and `Makefile` (modified).

## Test plan

There are no unit tests for a workflow file. Verification is:

1. The YAML parses (step 2).
2. The three commands the workflow runs all pass locally, in order, on a clean
   tree (step 3).
3. After the branch is pushed — which only the operator does — the `check`
   job must be green on **both** `ubuntu-24.04` and `macos-latest`. If the
   operator has not asked you to push, say so in your report and stop: the
   remaining verification needs a GitHub run and is theirs to trigger.

Do not add a test that shells out to `act` or similar; nothing in this repo
does, and it would be a new dependency for one file.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `.github/workflows/ci.yml` exists and `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` exits 0
- [ ] `cargo fmt --all -- --check` exits 0
- [ ] `cargo clippy --workspace --all-targets` exits 0
- [ ] `cargo test --workspace --locked` exits 0
- [ ] `make ci` exits 0
- [ ] `grep -c 'cancel-in-progress' .github/workflows/ci.yml` returns 1
- [ ] `grep -c 'head.repo.full_name' .github/workflows/ci.yml` returns 0 (fork PRs are not skipped)
- [ ] `grep -c -- '-- -D warnings' .github/workflows/ci.yml` returns 0 (strict clippy is plan 003)
- [ ] `grep -c 'CI runs no clippy' Makefile` returns 0
- [ ] `git status --short` lists only `.github/workflows/ci.yml` and `Makefile`
- [ ] `plans/README.md` status row for 002 updated

## STOP conditions

Stop and report back (do not improvise) if:

- `make ci` fails in step 1. Plan 001 is the prerequisite; if it has landed and
  the suite is still red, report the failing test names and output.
- `cargo test --workspace --locked` fails only because of `--locked` —
  i.e. cargo says `Cargo.lock` needs updating. That means the lockfile is
  already out of step with the manifests at HEAD. Report it; do not run
  `cargo update` to make it go away.
- Any test fails on `ubuntu-24.04` but passes on `macos-latest` (or the
  reverse) once the workflow runs. **Do not** make the failing platform
  `continue-on-error`, drop it from the matrix, or add `|| true`. Report which
  tests fail on which platform and their output — a platform difference in a
  tool that ships on both is a finding, and weakening the gate to hide it
  defeats the plan.
- The suite needs a tool that is not on a GitHub runner image. Report the tool;
  installing it is a decision, not an improvisation.
- `Makefile:219-240` does not match the excerpt above.

## Maintenance notes

- `make ci` and `.github/workflows/ci.yml` now encode the same gate in two
  places. Keep them in step: a check added to one belongs in the other. If
  that drift becomes a nuisance, the workflow calling `make ci` directly is
  the obvious consolidation — it was not done here because the three separate
  steps give a clearer failure line in the GitHub UI.
- A reviewer should check: no fork-skipping `if:`, no `-D warnings`, and
  `--locked` present on the test step.
- The integration tests spawn real daemons. If a future test ever reads the
  *real* `NEBULA_RUNTIME_DIR` or `NEBULA_DATA_DIR` instead of a tempdir, CI
  jobs on a shared runner could interfere. The isolation at
  `e2e_pty.rs:43-44` is what makes this safe; treat it as load-bearing.
- Deliberately deferred: strict clippy and a pinned toolchain (plan 003), and
  dependency advisories (plan 005). Both add jobs to this same file, so expect
  to touch it again.
- Not covered by this gate: the vendored `vendor/vt100` crate, which
  `Cargo.toml:4` excludes from the workspace. `cargo test --workspace` does not
  run its tests.
