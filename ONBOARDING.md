# Onboarding

This guide is for a person who is new to this repository. It tells you what the project is, how to build and run it safely, how the code is arranged, and where to read next.

## 1. What this repository is

**odyn** is a fork of [AgentSystemLabs/nebula](https://github.com/AgentSystemLabs/nebula). ("Odyn" is Welsh for kiln.) The binary is `odyn`, its splash screen shows a kiln, and everything a user sees or types says "odyn": messages, TUI labels, help pages, environment variables, files and directories. Only the crate names (`nebula-*`) and the upstream docs (`README.md`, `docs/`, `ARCHITECTURE.md`) keep the upstream name. In this guide, "nebula" means the upstream code in this repository.

nebula is "mission control for coding agents". It runs agent CLIs (Claude Code, Codex, Cursor, Pi, Muse, Grok, OpenCode) in terminals across many git checkouts, and shows all of them in one grid. Each agent has a coloured STATUS DOT, so you can see which agent is working, which is done, and which waits for you.

Two facts make the design:

- **There are two processes from one binary.** The DAEMON (`odyn daemon`) owns every PTY, the SQLite database and the agent status. The TUI (`odyn`) is a client. When you close the TUI, the agents continue to run.
- **Status comes from hooks, not from screen scraping.** At spawn, nebula writes MANAGED HOOKS into the agent's config (for example `.claude/settings.local.json`). Each hook sends a `curl` to a loopback HTTP server in the DAEMON. A state machine turns those events into the status dots.

To see the product before you read code, watch the [walkthrough video](https://youtu.be/ZBPqH36BPgI) and read the "What you get" table in [README.md](README.md).

### Fork notes

- `origin` is `eljamo/odyn`. There is no `upstream` remote by default. To compare with upstream:

  ```sh
  git remote add upstream https://github.com/AgentSystemLabs/nebula.git
  git fetch upstream
  git log --oneline main..upstream/main   # upstream commits that we do not have
  ```

- odyn installs the same way nebula does, from this fork's GitHub releases:

  ```sh
  curl -fsSL https://raw.githubusercontent.com/eljamo/odyn/main/install.sh | sh
  ```

  `install.sh` downloads `odyn-<platform>.tar.gz` from the latest release, or builds from source with `cargo install --git` when there is no release. `odyn upgrade` runs the same script, `odyn ssh` and `odyn tunnel` run it on the remote when `odyn` is not there, and the FOOTER's update indicator checks this fork's latest release. The fork's URLs are in `crates/odyn/src/lib.rs`.
- The README badges still point to the upstream repository, because `README.md` is an upstream file.

### odyn and nebula are separate

odyn does not share anything with nebula. It has its own DATA DIR, RUNTIME DIR and DAEMON, so both can be installed and run at the same time. odyn starts with no projects: add them with `odyn add` or from the splash screen.

odyn reads only odyn names. The upstream docs in `docs/` still use the nebula names, so use this table to read them:

| nebula name | odyn name |
|---|---|
| `nebula` command | `odyn` |
| `NEBULA_*` environment variables (for example `NEBULA_LOG`) | `ODYN_*` (`ODYN_LOG`) |
| `.nebula.json` project file | `.odyn.json` |
| `nebula.worktreeCreateHook`, `nebula.worktreeDeleteHook` git config keys | `odyn.worktreeCreateHook`, `odyn.worktreeDeleteHook` |
| `nebula-settings.json` export, `nebula_bundle` marker | `odyn-settings.json`, `odyn_bundle` |
| DATA DIR `~/Library/Application Support/dev.nebula.nebula` (macOS), `~/.local/share/nebula` (Linux) | `dev.odyn.odyn`, `~/.local/share/odyn` |
| RUNTIME DIR `$XDG_RUNTIME_DIR/nebula`, else `/tmp/nebula-<uid>` | `$XDG_RUNTIME_DIR/odyn`, else `/tmp/odyn-<uid>` |
| database `nebula.db` | `odyn.db` |
| `_nebulaManaged` tag on MANAGED HOOKS | `_odynManaged` |
| pi and OpenCode plugin files `nebula.ts`, Cursor rule `nebula-title.mdc` | `odyn.ts`, `odyn-title.mdc` |
| `[nebula]` tag on injected prompts | `[odyn]` |

When you use both tools on one repository, each writes its own MANAGED HOOKS into the agents' settings files. Each hook checks for its own tool's environment variables, so a nebula hook does nothing in an odyn session, and the reverse.

### The fork rule

We want to merge upstream often and with few conflicts. Every line we change in an upstream file can conflict with upstream's next change, so we sort each change into one of three kinds:

| Kind | Where it goes | Cost when we sync |
|---|---|---|
| **Bug fix** in upstream logic | In place, in the `nebula-*` crate. Also send it upstream as a PR. | None after upstream merges it, because it leaves our diff. |
| **Opinionated change**: new behaviour, a different default, an odyn-only feature | In our own crates: `crates/odyn` (no dependencies), `crates/odyn-cli` (clap types) or `crates/odyn-tui` (ratatui types). | None. Upstream never edits these files. |
| **Seam**: the hook that lets odyn override upstream behaviour | In upstream code, as small as possible, marked with a `// odyn:` comment. | Small. After an upstream refactor, you move a line or two. |

How the seams work:

- **Rust has no monkey-patching, so every override needs a seam.** The rule is not "never touch upstream". It is "touch upstream only to fix a bug or to add a seam".
- **The odyn crates depend on no nebula crate, and nebula crates call them.** This prevents a dependency cycle. A seam is a call from upstream code into an odyn crate, for example `odyn_tui::draw_sky` at the top of `splash::draw_sky`.
- **Keep upstream's code beside the seam when you can.** `splash::draw_sky` calls odyn's kiln and returns, but the galaxy code below it stays. Upstream changes to the galaxy then merge without conflicts.
- **If an override needs nebula's own types** (for example `App`), a leaf crate cannot hold it. Then the binary must install overrides at startup, which needs the `nebula` crate split into a library and a thin `main.rs`. We do that only when the first such override comes.
- **The command name is one constant.** Use `nebula_core::CLI_NAME`, or `nebula_core::cli_name!()` inside `concat!` for a `const` string. Agents run the command from `PATH`, so every place that tells a user or an agent which command to type uses the constant. Names stored on disk are odyn's too (see "odyn and nebula are separate" above).
- **An override does not need a runtime switch.** Add a setting only when you want to switch a behaviour on and off, because settings live in upstream files (`settings.rs` and the settings screen) and add conflicts.

More parts of the rule:

- **Docs follow the same rule.** `README.md` and `docs/` belong to upstream. Write odyn docs in new files, as `CLAUDE.md` and this guide are.
- **Put seams in stable places.** A seam in `event_loop.rs` (about 32,000 lines, changed often) conflicts more often than a seam at a module boundary.
- **Merge `upstream/main`. Do not rebase.** This fork is published, and a rebase rewrites history that other people may have.
- **Keep the footprint small.** `git diff --stat upstream/main...main -- crates/nebula* vendor Cargo.toml` shows how much upstream code we changed. `git grep 'odyn:'` lists every seam.

## 2. Set up

You need:

- **Rust stable** (the code uses edition 2021; some dependencies need a recent Cargo). There is no `rust-toolchain.toml`.
- **macOS or Linux.** WSL2 works as Linux. WSL1 does not work.
- **At least one agent CLI on your `PATH`**, for example `claude`. For tests and for `make dev AGENT=/bin/cat`, you do not need one.
- Optional: `gh` (pull request and issue views), `sqlite3` (seeds the dev instance), `tmux` and Python 3 (screenshot and latency harnesses), `ttyd` (`make browser`).

Then:

```sh
make check       # typecheck the workspace — about the fastest feedback you can get
make help        # list every make target
```

## 3. Run your changes safely

This is the most important section. **Your real odyn DAEMON runs the agents you work with every day, maybe including the agent that helps you now.** Some commands stop it.

| You want to… | Use | Effect on your real sessions |
|---|---|---|
| Try a change | `make dev` | None. An isolated instance with its own socket and DB, one per checkout. On first run it copies your real projects and settings (not your sessions). |
| Try a change in a browser tab | `make browser` | None. Same isolated instance, served by `ttyd`. |
| Try a change with no real agents | `make dev AGENT=/bin/cat` | None. Every agent becomes `cat`. |
| Start the dev instance empty | `make dev SEED=0` | None. |
| See every dev instance | `make dev-ls` | None. |
| Reset the dev instance | `make dev-reset` | None. |
| Use your build for real | `make install` | Installs `odyn` to `~/.cargo/bin`. The running DAEMON continues on the **old** binary. |
| Switch the DAEMON to the new binary | `odyn reload` | Restarts the DAEMON in place. Agents continue to run. |
| Stop everything | `make kill` / `odyn kill` | **Stops every real session.** |
| Install, kill, prune, and run dev | `make cycle` | **Stops every real session.** Run it from a terminal outside nebula. |

If you run the binary by hand, set `ODYN_DATA_DIR` and `ODYN_RUNTIME_DIR` to scratch directories. Then it cannot touch your real data.

`make prune` removes old build artifacts. `target/` can grow to tens of GB if you do not prune it.

## 4. Test

```sh
cargo test -p nebula-core                 # one crate
cargo test -p nebula-tui some_test_name   # one test
make test                                 # the full workspace (slow)
make ci                                   # fmt check + clippy + tests, the same gate as CI
make lint STRICT=1                        # clippy with warnings as errors
```

`crates/nebula/tests/e2e_pty.rs` and `e2e_tui.rs` spawn real DAEMONs and PTYs, so they are slow. CI (`.github/workflows/ci.yml`) runs on Ubuntu and macOS. It skips one mouse-drag test that hangs in a headless PTY.

Two harnesses help you check work that a unit test cannot:

- `make shot SCENE=<name>`: renders a screenshot of the debug TUI with demo data into `design-screenshots/`. The scenes are in `scripts/shot/scenes/`.
- `make perf`: measures input latency per action. Use it when you change the event loop or painting.

## 5. Code map

| Crate | What it does | Start reading at |
|---|---|---|
| `nebula` | The `odyn` binary. No arguments opens the TUI; subcommands are `add`, `daemon`, `kill`, `reload`, `worktree`, `spawn`, `open`, `ssh`, `tunnel`, `browser`, `upgrade`, … | `src/main.rs`, `src/cli.rs` |
| `nebula-core` | Types that both processes share: protocol, entities, IDs, paths, settings, the MessagePack codec. | `protocol.rs`, `entities.rs`, `settings.rs` |
| `nebula-daemon` | PTYs, SQLite, git, the hook receiver, the status machine. | `server.rs`, `registry.rs`, `store.rs`, `status.rs`, `hooks/`, `pty/` |
| `nebula-tui` | The ratatui client: grid, modals, keys, mouse, attach and scrollback. | `app.rs`, `event_loop.rs`, `ui.rs`, `keymap.rs` |
| `nebula-fuzzy` | The fuzzy matcher for list filters. It is a separate crate only so that dev builds optimise it. | `src/lib.rs` |
| `odyn` | odyn's constants: the command name (`CLI_NAME`, `cli_name!`). | `src/lib.rs` |
| `odyn-cli` | odyn's command-line overrides: renames the command in every help `Examples:` block when the parser is built, and keeps the description column aligned. | `src/lib.rs` |
| `odyn-tui` | odyn's TUI overrides: the O D Y N wordmark and the kiln scene, with its art as an editable table (`KILN`). | `src/lib.rs` |
| `vendor/vt100` | A patched copy of the terminal parser. Rows that scroll out of a top-anchored region go to scrollback. | `Cargo.toml` |

The domain is a tree: **PROJECT** (a git repo) → **WORKTREE** (the root checkout or a `git worktree`) → **SESSION** (an agent or a plain terminal).

The TUI and the DAEMON talk over a unix socket with length-prefixed MessagePack. The TUI sends `ClientRequest`s; the DAEMON pushes `ServerEvent`s. Both enums are in `nebula-core/src/protocol.rs`. [docs/client-daemon-interaction.html](docs/client-daemon-interaction.html) shows this as a diagram.

`nebula-tui/src/event_loop.rs` has about 32,000 lines. Do not read it from top to bottom. Search for the feature you need, and read that part.

## 6. Rules the code follows

[ARCHITECTURE.md](ARCHITECTURE.md) explains these in full. In short:

- **Input is not action.** A key handler and a mouse handler only decide which row you chose. Then they call one function that does the action (`event_loop/activate.rs`). Tests named `INPUT PARITY` check that a key and a click do the same thing.
- **A key handler never blocks.** Work that spawns a process, reads a file or uses the network runs in the background (`view_jobs.rs`). The TUI shows the expected result at once and rolls it back if the DAEMON sends an error.
- **The DAEMON is the source of truth** for projects, worktrees and sessions.
- **Old and new builds must work together.** Users upgrade the TUI and the DAEMON at different times, and settings files move between machines. Protocol changes, SQLite migrations and settings keys must be safe in both directions.
- **nebula writes into user files** (hook configs, `~/.codex`, `~/.pi`). Its entries carry `_odynManaged`. The user's own entries must stay unchanged.

## 7. Conventions

- **Language.** The docs write domain terms in capitals: SESSION, CARD, BAND, WORKTREE, DAEMON, PROJECT TAB, STATUS DOT, QUICK PROMPT, PANE. The pages in `docs/` are the glossary. Use the same terms in code comments, docs and PRs.
- **Comments** explain why. Many comments describe the bug that a line prevents, or the measurement behind a number. Search the Makefile for "load-bearing" to see the style.
- **Docs change with the code.** If you change a key, a command or a setting, update `docs/keys.md`, `docs/commands.md` or `docs/configuration.md` in the same PR.
- **Commit and PR titles** are one sentence about what the user now sees, for example "A session in a deleted checkout refuses to start, and its pane says why". The repository does not use conventional-commit prefixes.
- **Shared checkout.** Many agents can edit one checkout at the same time. Before you commit, read the diff of each file and stage only your own hunks. For longer work, use a `git worktree`.

## 8. Claude Code in this repository

- [CLAUDE.md](CLAUDE.md) gives agents the short version of this guide.
- `.claude/skills/` has three project skills:
  - `pr-description`: writes a PR body in the house style.
  - `pr-reviewer`: reviews a PR by reading only. It never builds or runs the PR.
  - `release`: bumps the version, tags, pushes, and writes the release notes.
- On GitHub, a PR from a branch in this repository gets an automatic Claude review. Comment `@claude` on an issue or PR to ask Claude for help.
- `plans/` (if present) has improvement plans from an audit. `plans/README.md` shows their order and status.

## 9. Releases

A `v*` tag starts `.github/workflows/release.yml`. It builds macOS (arm64, x86_64) and Linux musl (x86_64, arm64) binaries, packs each as `odyn-<platform>.tar.gz`, and attaches them to a GitHub release on this fork. Use the `release` skill; it does each step in a private worktree.

Tag with the workspace version (`version` in the root `Cargo.toml`). The update indicator compares the latest release tag with the running binary's version, so a tag below the current version never shows as an update.

## 10. What to read next

1. [README.md](README.md): the product, from a user's view.
2. [docs/keys.md](docs/keys.md) and [docs/sessions.md](docs/sessions.md): how the TUI is used.
3. [ARCHITECTURE.md](ARCHITECTURE.md): the process model and each data path.
4. [docs/how-it-works.md](docs/how-it-works.md): the DAEMON, the hooks, prewarm, persistence.
5. [docs/configuration.md](docs/configuration.md): settings, the project file, logs and every environment variable. It uses the nebula names; see the table in section 1 for odyn's.

A good first task: run `make dev`, find a small behaviour in the TUI, then follow it from its key in `keymap.rs` to its action and, if it sends one, to the `ClientRequest` that the DAEMON handles in `server.rs` or `registry.rs`.
