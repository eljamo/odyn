# CLAUDE.md

**odyn** is a fork of [AgentSystemLabs/nebula](https://github.com/AgentSystemLabs/nebula). The binary is `odyn`, and every name a user sees or types is odyn's: messages, env vars (`ODYN_*`), files (`.odyn.json`), data dirs, git config keys. Only the crate names (`nebula-*`) and the upstream docs (`README.md`, `docs/`, `ARCHITECTURE.md`) still say **nebula**; `ONBOARDING.md` maps the names.

nebula is a Rust TUI plus a background DAEMON that runs coding-agent CLIs (Claude, Codex, Cursor, …) in PTYs across git WORKTREES. Read `ARCHITECTURE.md` before a change that crosses the TUI–DAEMON boundary.

## Fork rule: keep upstream sync cheap

Every line changed in an upstream file can become a merge conflict when we sync. Sort each change into one of three kinds before you write it:

- **Bug fix** in upstream logic: fix it in place in the `nebula-*` crate. Tell the user it is a candidate for an upstream PR.
- **Opinionated change** (new behaviour, a different default, an odyn-only feature): put the logic in `crates/odyn` (no dependencies), `crates/odyn-cli` (clap types) or `crates/odyn-tui` (ratatui types).
- **Seam**: the smallest edit to upstream code that calls into an odyn crate. Mark each one with a `// odyn:` comment so `git grep 'odyn:'` lists all of them. Keep upstream's code beside the seam when you can (see `splash::draw_sky`), so upstream changes to it still merge.

The odyn crates depend on no nebula crate; nebula crates call them. If an override ever needs nebula's own types, stop and ask: that needs the binary to install overrides at startup instead.

- **The name users see** is `nebula_core::CLI_NAME`: `{CLI_NAME}` in a format string, `concat!(…, nebula_core::cli_name!(), …)` in a `const` or other `&'static str`. Use it in every command, message and TUI label you add or change. CLI help in `cli.rs` stays upstream's text; `odyn-cli` renames it when the parser is built. Stored and typed names are odyn's as literals: `ODYN_*` variables, `.odyn.json`, `odyn.worktree*Hook` git keys, `_odynManaged`, and the `[odyn]` tag that starts injected prompts (`prompt_history.rs` filters on it). odyn reads no nebula name as a fallback.
- **No odyn-only SQLite migrations.** Migrations are numbered by their position in `MIGRATIONS` (`store.rs`). An odyn migration takes the position of upstream's next one, so a merge would make existing databases skip a migration without an error.
- Put odyn docs in new files, not in `README.md` or `docs/`. Put seams at module boundaries, not deep in `event_loop.rs`.

## Blast radius: the real daemon

You are probably running *inside* a nebula or odyn SESSION. The real DAEMON owns the user's live agents, this one included.

- Run and test changes with `make dev`: an isolated instance per checkout, with its own socket and DB.
- Ask before `make install`, `make kill`, `make cycle`, or `kill`, `reload` or `upgrade` on `odyn` or `nebula`. Each one stops or restarts the user's real sessions.
- Point any ad-hoc run of the binary at scratch dirs with `ODYN_DATA_DIR` and `ODYN_RUNTIME_DIR`.

## Commands

`make help` lists all targets. The ones you need:

- `make check`: fastest feedback.
- `cargo test -p <crate> <filter>`: the focused loop. `cargo test --workspace` is slow because `e2e_pty` and `e2e_tui` spawn real daemons and PTYs.
- `make ci`: the full gate (fmt check, clippy, tests). Clear clippy with `make lint STRICT=1`.
- `make shot SCENE=<name>`: render a TUI screenshot from `scripts/shot/scenes/` when you change what the grid draws.
- `make perf`: measure input latency when you touch the event loop or painting.

## Shared checkout

Other agents edit this tree at the same time. Before you stage, read `git diff` for each file and stage only your hunks. For a commit, release or anything long-running, work in a `git worktree`.

## Code rules

- **Input is not action.** In `nebula-tui`, key and mouse arms only resolve *which row*. They call one function that does the action (`event_loop/activate.rs`, or a modal module's own `activate_selected`). The `INPUT PARITY` tests in `event_loop.rs` check this.
- **A key handler never blocks.** Process spawns, file reads and network go to a BACKGROUND READ (`view_jobs.rs`) or a feature channel.
- **The DAEMON is the source of truth** for projects, worktrees and sessions. The TUI only shows optimistic state (`event_loop/optimistic.rs`) and rolls it back on error.
- **Old and new builds must stay compatible.** A protocol change in `nebula-core/src/protocol.rs`, a SQLite migration in `nebula-daemon/src/store.rs`, or a settings key in `nebula-core/src/settings.rs` reaches every user's disk. A newer daemon, an older TUI and an unknown config key must all keep working.
- **Managed hooks** that nebula writes into `.claude/`, `.cursor/` and `~/.codex/` carry `_odynManaged`. Keep the user's own entries, and nebula's, intact.
- `vendor/vt100` is a patched fork wired in by `[patch.crates-io]`. Change it only for terminal-parser bugs.
- `crates/nebula-tui/src/event_loop.rs` is about 32k lines. Grep it and read ranges.

## Comments and docs

- Comments explain *why*: the bug the code prevents, the measurement behind a number, what is load-bearing. Match the prose style already in the file.
- Domain terms are written in CAPS in prose (SESSION, CARD, BAND, WORKTREE, DAEMON, PROJECT TAB, STATUS DOT, QUICK PROMPT, …). The user docs in `docs/` and `README.md` are the glossary. Reuse their terms; do not coin synonyms.
- A change to keys, commands, settings or behaviour updates the matching page in `docs/` in the same change.

## Commits and PRs

- Titles are one plain sentence about what the user now sees ("A session in a deleted checkout refuses to start, and its pane says why"). No conventional-commit prefixes.
- Use the `pr-description` skill for PR bodies, `pr-reviewer` to review a PR, and `release` to cut a release.
- The `release` skill is upstream's. For odyn, the repo slug is `eljamo/odyn` (pass `--repo eljamo/odyn` to `gh`, because `gh` in a fork can pick the parent), the install line is `curl -fsSL https://raw.githubusercontent.com/eljamo/odyn/main/install.sh | sh`, and its notes about upstream's GitHub accounts do not apply.
- `plans/` holds audit plans from the `improve` skill. Check `plans/README.md` before you start on CI, the toolchain, release checksums or a listed spike.
