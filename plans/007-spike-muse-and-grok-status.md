# Plan 007 (SPIKE): Decide what status muse and grok can report

> **Executor instructions**: This is a **design spike, not a build task**. Its
> deliverable is a written findings document, not merged product code. You may
> prototype to answer a question, but every prototype is throwaway and nothing
> under `crates/` is committed. Follow the steps in order, and if anything in
> "STOP conditions" occurs, stop and report rather than improvising. When done,
> update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 565080d..HEAD -- crates/nebula-core/src/harness.rs crates/nebula-core/src/entities.rs crates/nebula-daemon/src/hooks crates/nebula-daemon/src/registry.rs`
> If any of those changed, compare the "Current state" excerpts below against
> the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M (investigation + write-up; implementation separate, and may be S)
- **Risk**: LOW (no product code ships from this plan)
- **Depends on**: none
- **Category**: direction
- **Planned at**: commit `565080d`, 2026-10-06
- **Deliverable**: `plans/spikes/007-muse-and-grok-status-findings.md`

## The question this spike answers

nebula's whole premise is that a colored dot tells you which agent needs you —
`README.md`: *"nebula replaces the reading with a grid and a color."* Five of the
seven built-in harnesses deliver that. **Muse and grok do not**, and the
codebase says so in three separate places, each using the word *until*.

Everything on the hook path is therefore absent for those two: no
needs-feedback dot, no auto-title, no issue/PR context rule injected into the
launch prompt. Muse is further behind still — no resume, so a restart loses the
conversation, and an Effort row that is deliberately a stored-but-unsent
placeholder.

The spike's job is to find out **what muse and grok actually expose** and then
answer, for each:

1. Does the CLI speak one of the five hook dialects nebula already installs
   (`claude`, `codex`, `cursor`, `pi`, `opencode`)? If so, setting one field on
   the built-in descriptor may be the whole change.
2. If not, does it expose any extension, plugin, or event surface a sixth
   dialect could be built on — the way `pi` and `opencode` are TypeScript
   plugins rather than shell hooks?
3. If neither, what should nebula *say*? A silent second class on a
   status-first grid is the worst of the options, and admitting the gap in the
   picker is cheap.
4. For muse specifically: does it have a resume mechanism, and what is the Effort
   row reserved *for*?

A fifth question rides along because the investigation surfaces it:

5. `crates/nebula-core/src/entities.rs:72-75` claims custom harnesses have
   *"process-based status and no resume — like `AgentKind::Muse`"*, but
   `CustomHarness` has a `hooks` field and `hook_dialect()` accepts five
   dialects. Is that doc stale, and does the escape hatch mean a user can
   already get full status for muse or grok today by declaring a custom harness?

## Current state

### The code says this is pending, three times

`crates/nebula-core/src/entities.rs:50-71` — the enum's own doc comments:

```rust
pub enum AgentKind {
    #[default]
    Claude,
    Codex,
    Cursor,
    /// pi.dev's coding agent: the `pi` CLI (npm
    /// `@earendil-works/pi-coding-agent`). Status comes from a managed
    /// TypeScript extension rather than shell hooks.
    Pi,
    /// Meta's Muse Spark coding agent: the `muse` CLI. No managed
    /// hooks yet, so status is process-based (running while the PTY
    /// is live) until a hook dialect is mapped.
    Muse,
    /// xAI's Grok Build CLI. Status is process-based until managed hooks
    /// are supported.
    Grok,
    /// OpenCode (opencode.ai): the `opencode` CLI. Status comes from a
    /// managed TypeScript plugin rather than shell hooks (see the daemon's
    /// `hooks::opencode_plugin`); the first prompt rides `--prompt`, since
    /// its positional is the project path, and a session resumes by
    /// `--session <id>`.
    OpenCode,
```

`crates/nebula-daemon/src/registry.rs:2474-2479` — where the dialect is read:

```rust
        let harness = resolve_harness(agent.kind, agent.custom_harness.as_deref())?;
        // Managed status hooks; a failure here degrades to "no status
        // updates", never blocks the spawn. The dialect is data: a custom
        // harness naming one reports status, prompts and permission waits
        // exactly like that harness, and a harness with none runs
        // hookless (process-based status until a dialect is mapped).
        let install_result = match harness.hook_dialect() {
```

### The two descriptors, in full

`crates/nebula-core/src/harness.rs`, the `"muse"` arm of `builtin()`:

```rust
        "muse" => HarnessDescriptor {
            label: "Muse".into(),
            program: "muse".into(),
            model: ModelSpec {
                flag: Some("--model".into()),
                default: default_model_choice(),
                models: Vec::new(),
                catalog: None,
            },
            effort: EffortSpec {
                default: default_model_choice(),
                offered: true,
                ..EffortSpec::default()
            },
            ..base
        },
```

`..base` supplies the rest, which means: `prompt_flag: None`, `hooks: None`,
`resume: ResumeSpec::default()` (no flag, no subcommand — so
`HarnessDescriptor::resumes()` at `harness.rs:266-268` returns false), and
`system: SystemSpec::default()` (no `append_flag`). Its `effort` has
`offered: true` but neither a flag nor a config key — and `harness.rs:112-114`
says what that is:

```rust
    /// Whether effort is a concept for this harness at all: the Agents
    /// tab's Effort row and the effort submenus show only while set. Muse
    /// keeps a reserved row (stored, never sent); legacy customs have none.
    pub offered: bool,
```

The `"grok"` arm:

```rust
        "grok" => HarnessDescriptor {
            label: "Grok Build".into(),
            program: "grok".into(),
            model: ModelSpec {
                flag: Some("--model".into()),
                ..ModelSpec::default()
            },
            effort: EffortSpec {
                flag: Some("--reasoning-effort".into()),
                offered: true,
                ..EffortSpec::default()
            },
            resume: ResumeSpec {
                flag: Some("--resume".into()),
                ..ResumeSpec::default()
            },
            system: SystemSpec {
                append_flag: Some("--rules".into()),
                ..SystemSpec::default()
            },
            ..base
        },
```

So grok **does** resume (`--resume`) and **does** take an appended system prompt
(`--rules`). Its only gap is the hook dialect. Muse's gaps are hooks, resume,
system-append and a real effort flag. Treat them as two different problems of
different sizes.

### The five dialects and how they are installed

`crates/nebula-core/src/harness.rs:276-285`:

```rust
    pub fn hook_dialect(&self) -> Option<crate::AgentKind> {
        match self.hooks.as_deref().map(str::trim) {
            Some("claude") => Some(crate::AgentKind::Claude),
            Some("codex") => Some(crate::AgentKind::Codex),
            Some("cursor") => Some(crate::AgentKind::Cursor),
            Some("pi") => Some(crate::AgentKind::Pi),
            Some("opencode") => Some(crate::AgentKind::OpenCode),
            _ => None,
        }
    }
```

`crates/nebula-daemon/src/registry.rs:2479-2502` dispatches on it, and the
installers are genuinely varied — which is the good news for a sixth dialect:

- **Claude** — shell hooks written into the worktree's
  `.claude/settings.local.json`.
- **Codex** — shell hooks in Codex's *home* (`~/.codex/hooks.json`), so one
  trust approval covers every worktree; per-worktree copies from older nebulas
  are pruned.
- **Cursor** — `.cursor/hooks.json` in the worktree, in Cursor's own camelCase
  dialect, **plus** a managed `.cursor/rules/nebula-title.mdc` project rule,
  because Cursor's hooks have no context-injection channel.
- **Pi** — a TypeScript *extension*, not hooks at all. From
  `crates/nebula-daemon/src/hooks/pi_extension.rs:1-15`:

  ```rust
  //! nebula's MANAGED HOOKS for pi (pi.dev): pi runs TypeScript extensions,
  //! not shell hooks, so a pi AGENT's status signals are one file —
  //! `<pi agent dir>/extensions/nebula.ts` — that maps pi's lifecycle events
  //! onto the HOOK EVENTS the daemon already understands and POSTs them to
  //! `/api/hooks/pi` (the injectable route: the `UserPromptSubmit` reply body
  //! carries the AUTO-TITLE INSTRUCTION, and the extension appends it to that
  //! run's system prompt).
  //!
  //! The file is wholly nebula-owned (namespaced name, rewritten on every
  //! spawn when its content drifts) and env-guarded, so a bare `pi` outside
  //! nebula loads it and does nothing.
  ```

- **OpenCode** — a TypeScript *plugin*, same idea
  (`crates/nebula-daemon/src/hooks/opencode_plugin.rs`, payload
  `nebula_opencode_plugin.ts`).

So the bar for a new dialect is not "the CLI must have shell hooks" — it is "the
CLI must have *some* way to run nebula's code on its lifecycle events and POST
to the loopback receiver". Three different mechanisms already satisfy it.

### What a hookless session still gets

Not nothing. The OSC 9;4 progress scanner lives in `PtySession` itself and is
not gated on harness kind — `crates/nebula-daemon/src/pty/mod.rs:228` and `:300`:

```rust
    progress: Mutex<ProgressScanner>,
```

…and it feeds the status machine as a `HookEvent::Progress { busy }`
(`crates/nebula-daemon/src/status.rs:122`, `:468`). But it only fires if the CLI
drives a terminal progress bar, and `crates/nebula-daemon/src/pty/progress.rs:1-31`
documents that behaviour **only for Claude Code**, verified against version
2.1.241 by capturing raw PTY bytes. Whether muse or grok emit it is unknown and
is one of the things this spike measures. Note the ceiling: `pty/mod.rs:575-577`
returns an `Option`, and a progress signal can start and end a turn but can never
mean *needs feedback* — `progress.rs:25-27` explains why that is deliberate.

### The custom-harness escape hatch

`crates/nebula-core/src/harness.rs:755-790`, the `CustomHarness` struct, has a
`hooks` field with this doc:

```rust
    /// Hook dialect to install for this entry's sessions, naming the
    /// built-in CLI whose hooks the entry's program speaks: `"claude"`,
    /// `"codex"`, `"cursor"` or `"pi"`. With one set the sessions report
    /// status, prompts and permission waits exactly like that harness;
    /// without one they stay process-based (running while the PTY is
    /// live, never waiting-on-you). A Claude-compatible CLI gets title
    /// sync and auto-title with `"claude"`.
    pub hooks: Option<String>,
```

Two things to note. First, that doc lists four dialects while `hook_dialect()`
accepts five — `"opencode"` is missing from the prose. Second, if muse or grok
turns out to speak a dialect, a user can get full status *today* by declaring a
custom harness entry pointing at the same program with `hooks` set — no code
change at all. That possibility changes the shape of the answer and is question
5.

### Repo conventions the findings doc must respect

- **Comments explain why, at length, and name the failure that motivated them.**
  `pty/progress.rs:1-31` is the exemplar: it states the upstream behaviour, the
  version it was verified against, and a table of captured byte sequences. Any
  claim this spike makes about what muse or grok emit should be evidenced to
  that standard — a version number and a captured observation, not a guess.
- **The domain vocabulary is capitalised in prose**: HOOK EVENTS, MANAGED HOOKS,
  AGENT, SESSION, STATUS DOT, PREWARM POOL. `ARCHITECTURE.md` is the glossary;
  read its "Status path (not MCP)" paragraph before writing.
- Managed files are **nebula-owned and env-guarded**: namespaced names,
  rewritten on spawn when content drifts, and inert when the CLI runs outside
  nebula. Any new dialect must follow that rule — see `pi_extension.rs:9-11`.
- Hook installation **never blocks a spawn**. `registry.rs:2474-2476`: *"a
  failure here degrades to 'no status updates', never blocks the spawn."*

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Is muse installed? | `command -v muse && muse --version` | a path and a version, or nothing |
| Is grok installed? | `command -v grok && grok --version` | a path and a version, or nothing |
| muse's surface | `muse --help` | its flags; look for hooks/extensions/plugins/events/resume |
| grok's surface | `grok --help` | the same |
| Read the dialects | `grep -n 'Some("' crates/nebula-core/src/harness.rs \| head -20` | the five-arm match at `:277-284` |
| Typecheck | `cargo check --workspace --all-targets` | exit 0 |
| Tests | `cargo test --workspace` | exit 0 (see STOP conditions) |
| Throwaway worktree | `git worktree add /tmp/nebula-spike-007 HEAD` | a detached checkout |
| Remove it | `git worktree remove --force /tmp/nebula-spike-007` | gone |
| Isolated run | `NEBULA_RUNTIME_DIR=/tmp/nb-007-rt NEBULA_DATA_DIR=/tmp/nb-007-data cargo run` | never touches real sessions |

**The isolation variables are mandatory for any run.** `NEBULA_RUNTIME_DIR` and
`NEBULA_DATA_DIR` (`crates/nebula-core/src/env.rs:16-18`) keep a prototype off
the operator's live daemon and real sessions; a run without them can attach to,
rename or kill the operator's actual work. `Makefile:52-53` does exactly this for
`make dev` — follow it.

To capture what a CLI emits on its PTY — the measurement `progress.rs` did —
`script` is the portable tool:

```sh
script -q /tmp/muse-bytes.txt muse        # macOS: script -q <file> <cmd>
# drive one turn, then exit; then look for the OSC 9;4 sequences:
grep -c $'\x1b]9;4' /tmp/muse-bytes.txt
```

## Scope

**In scope** — you may create or modify only:
- `plans/spikes/007-muse-and-grok-status-findings.md` (create; create
  `plans/spikes/` if absent)
- `plans/README.md` — your status row only

**Out of scope** (do NOT commit changes to any of these):
- Everything under `crates/`. In particular do **not** set `hooks:` on the muse
  or grok descriptor, however small that change looks. Whether it is correct is
  precisely what this spike decides, and an unverified dialect produces *wrong*
  status, which is worse than none.
- `crates/nebula-daemon/src/hooks/` — no new installer, no new `.ts` payload.
- `ARCHITECTURE.md`, `docs/`, and the `entities.rs` doc comments. Correcting the
  stale custom-harness doc (question 5) is a finding to record, not an edit to
  make here — it wants to land with whatever change it describes.
- The other plans in `plans/`, except your own status row.
- Anything to do with plans 001–005.

## Steps

### Step 1: Read nebula's side first

Read, in this order, so you know what a dialect has to supply before you go
looking for one:

1. `ARCHITECTURE.md`, the "Status path (not MCP)" and "Auto-title path"
   paragraphs. These are the contract.
2. `crates/nebula-daemon/src/hooks/mod.rs` — the receiver, its routes, and the
   `HookEvent` enum it maps onto.
3. `crates/nebula-daemon/src/hooks/pi_extension.rs` **and**
   `crates/nebula-daemon/src/hooks/nebula_pi_extension.ts` — the smallest
   complete example of a non-shell dialect, end to end.
4. `crates/nebula-daemon/src/status.rs:1-80` — the state machine's doc header,
   which is where the semantics of each event live.

**Verify**: a findings section `## What a dialect must supply` listing the
`HookEvent` variants the status machine consumes, which of them are
load-bearing for each STATUS DOT, and which require a context-injection channel
(the auto-title path) versus which do not.

### Step 2: Find out what muse and grok actually expose

For each of the two CLIs:

```sh
command -v muse && muse --version && muse --help
command -v grok && grok --version && grok --help
```

Look specifically for: a hooks or extensions or plugins mechanism; a lifecycle
event or webhook surface; a resume or session flag; a reasoning-effort flag; a
JSON/scripting output mode. Check for a config directory (`~/.muse`, `~/.grok`,
or whatever `--help` names) and list what is in it.

If a CLI is **not installed**, do not stop — fall back to desk research and say
so explicitly in the findings:

- Its published documentation, release notes, and changelog.
- Its npm/package metadata if it has one.
- Whether a hook or plugin surface is announced, planned, or absent.

Mark every claim with its source and date. A findings doc that cannot tell a
measured observation from a documentation claim is not usable.

**Verify**: a findings section per CLI — `## muse` and `## grok` — each stating
whether it is installed, its version if so, its resume mechanism, its effort
flag, and whether any hook/extension/event surface exists. Every claim carries
a source.

### Step 3: Measure whether they emit OSC 9;4

Only if the CLI is installed. This decides how degraded a hookless session
really is today, which is the baseline every other answer is measured against.

```sh
script -q /tmp/muse-bytes.txt muse
# drive one full turn: submit a prompt, let it finish, exit
grep -c $'\x1b]9;4' /tmp/muse-bytes.txt
```

Then read `crates/nebula-daemon/src/pty/progress.rs:14-27` and build the same
table for this CLI: what sequence appears at startup, on submit, while a
permission prompt waits, at turn end, and on cancel.

**Verify**: a findings section `## Does the progress signal arrive?` with a table
per installed CLI in `progress.rs`'s format, or an explicit "not installed, not
measured".

### Step 4: Answer the dialect question for each CLI

For each of muse and grok, pick exactly one conclusion and justify it:

- **(a) It speaks an existing dialect.** Name which, and what evidence. State
  the change: one `hooks:` field on the descriptor in `harness.rs`'s `builtin()`.
  Then state what could go *wrong* — a CLI that accepts a `claude`-shaped hooks
  file but fires different events produces wrong status, and a dot that lies is
  worse than a grey one. Specify how you would verify before shipping.
- **(b) It has an extension/plugin surface a sixth dialect could use.** Specify
  the dialect: where the managed file goes, how it is env-guarded, which
  lifecycle events map to which `HookEvent`, whether it can inject context (and
  so whether auto-title works or needs the Cursor-style project-rule
  workaround). Estimate the work against `pi_extension.rs` as the yardstick.
- **(c) It exposes nothing.** Then answer question 3: what should nebula say?
  Options to weigh — a note in the NEW SESSION PICKER, a distinct dot or glyph
  for "this harness cannot report", a line in `docs/sessions.md`, or nothing at
  all on the grounds that grey already means unknown. Recommend one.

**Verify**: findings sections `## Verdict — muse` and `## Verdict — grok`, each
naming (a), (b) or (c) with its evidence and its specified change or message.

### Step 5: Answer muse's extra gaps

Separately from status:

- **Resume.** Does `muse` have any session-resume mechanism? If yes, the
  descriptor's `ResumeSpec` is a two-line change — but check
  `harness.rs:191-196` first, which gates whether a relocated session resumes
  with a trailing "continue in this checkout" prompt on *"CLIs verified to open a
  resumed session"*. Note what verification that implies.
- **The reserved Effort row.** `effort.offered` is true with nothing to send
  (`harness.rs:112-114` calls it *"a reserved row (stored, never sent)"*). Find
  out whether muse has a reasoning-effort concept at all. If it does, the row
  becomes real; if it does not, the reserved row is misleading UI and should be
  recorded as such.

**Verify**: a findings section `## muse's other gaps` answering both, each with a
recommendation.

### Step 6: Settle the custom-harness escape hatch (question 5)

Establish by reading:

- Can a `CustomHarness` entry pointing at `muse` or `grok` with `hooks` set
  already get full status today? Trace `CustomHarness::as_descriptor()`
  (`harness.rs:~807`) and `CustomHarness::hook_dialect()` (`harness.rs:857-859`)
  to confirm.
- Is `entities.rs:72-75`'s *"process-based status and no resume — like
  `AgentKind::Muse`"* accurate? The `hooks` field suggests the status half is
  stale. Check whether a custom entry can resume.
- Is `CustomHarness::hooks`'s own doc (`harness.rs:780-785`) stale for listing
  four dialects where `hook_dialect()` accepts five?

If the escape hatch does work, that materially changes the recommendation: the
answer to a user asking for muse status today becomes a `config.json` snippet
rather than a release.

**Verify**: a findings section `## The custom-harness escape hatch` stating
whether it works, with a worked `config.json` example if it does, and listing
every doc comment found stale with its `file:line`.

### Step 7: Prototype only if a question is still open

Skip entirely if steps 2–6 are answered. Prototype only to settle something
reading and `--help` could not — most likely "does this CLI actually fire the
events a `claude`-shaped hooks file asks for".

If you do, use the throwaway worktree and **always** the isolation variables:

```sh
git worktree add /tmp/nebula-spike-007 HEAD
cd /tmp/nebula-spike-007
NEBULA_RUNTIME_DIR=/tmp/nb-007-rt NEBULA_DATA_DIR=/tmp/nb-007-data cargo run
```

Clean up:

```sh
git worktree remove --force /tmp/nebula-spike-007
rm -rf /tmp/nb-007-rt /tmp/nb-007-data
```

A prototype that writes a hooks file into a real repository's `.claude/` or
`.cursor/` must do so in a scratch git repo under `/tmp`, never in the operator's
own checkouts. Note that `.gitignore:4` and `:13` already ignore
nebula-managed `/.codex/` and `/.claude/worktrees/` in *this* repo, which tells
you these files do get written into checkouts — be deliberate about where.

**Verify**: `git worktree list` shows one entry; `git status --short` in the main
checkout lists nothing under `crates/`.

### Step 8: Write the findings doc

Assemble `plans/spikes/007-muse-and-grok-status-findings.md`:

```markdown
# Spike 007 findings: what status muse and grok can report

- **Spiked at**: commit <short SHA>, <YYYY-MM-DD>
- **Question**: <one sentence>
- **Recommendation**: <one paragraph: per CLI, what to build or say, in what order>

## What a dialect must supply
## muse
## grok
## Does the progress signal arrive?
## Verdict — muse
## Verdict — grok
## muse's other gaps
## The custom-harness escape hatch
## Open questions for the operator
## What I did not investigate
```

`## Open questions for the operator` is required and must not be empty. Anything
that is a product call — whether to admit a harness's limits in the picker,
whether to ship a harness that cannot report status at all — belongs there
rather than being decided by you.

**Verify**: every section present and non-empty; the Recommendation names a
verdict for each CLI and an order.

### Step 9: Final check

**Verify**:
- `git status --short` lists only
  `plans/spikes/007-muse-and-grok-status-findings.md` and `plans/README.md`.
- `git worktree list` shows one entry.
- `git diff --stat -- crates Cargo.toml Cargo.lock` produces no output.

## Test plan

A spike ships no product code, so there are no new tests. Verification is the
findings doc's completeness, per the done criteria.

For whoever writes the build plan, record these in the findings doc's own
maintenance notes:

- Hook dialects are covered by `crates/nebula-daemon/src/hooks/mod.rs`'s inline
  tests, which POST to the real loopback receiver with a real bearer token —
  `grep -n 'async fn http_post' crates/nebula-daemon/src/hooks/mod.rs` (`:474`)
  shows the helper, and the tests from `:560` onward are the pattern. A sixth
  dialect gets a test of that shape per event.
- Installer behaviour is covered in `crates/nebula-daemon/src/hooks/installer.rs`'s
  tests; `install_unless_unchanged` (used by `pi_extension.rs`) is the idempotency
  contract any new installer must honour.
- Descriptor validity is covered by `HarnessDescriptor::problem()`
  (`harness.rs:297-340`) and its tests — note that `registry.rs:4173` already
  asserts `grok.hook_dialect() == None`, so **that test must be updated in the
  same change** if grok gains a dialect. Say so explicitly; it is the one test a
  build plan will otherwise miss.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `plans/spikes/007-muse-and-grok-status-findings.md` exists
- [ ] `grep -c '^## ' plans/spikes/007-muse-and-grok-status-findings.md` returns at least 9
- [ ] `## Verdict — muse` and `## Verdict — grok` each name exactly one of (a), (b) or (c)
- [ ] For each CLI the doc states whether it was installed and, if so, its version
- [ ] Every claim about muse's or grok's surface carries a source (a captured
      observation, or a documentation URL with a date)
- [ ] `## The custom-harness escape hatch` reaches an explicit yes or no
- [ ] `## Open questions for the operator` is present and non-empty
- [ ] `grep -rn 'hooks:' crates/nebula-core/src/harness.rs | grep -c 'muse\|grok'` returns 0 — no descriptor was edited
- [ ] `git diff --stat -- crates Cargo.toml Cargo.lock` produces no output
- [ ] `git worktree list` shows exactly one worktree
- [ ] `git status --short` lists only the findings doc and `plans/README.md`
- [ ] `plans/README.md` status row for 007 updated

## STOP conditions

Stop and report back (do not improvise) if:

- `cargo test --workspace` fails on anything other than the two known
  `e2e_tui` failures at `565080d`
  (`nebula_open_from_inside_a_session_raises_the_file_tabs` and
  `tui_drag_past_the_pane_top_autoscrolls_and_copies_the_run`, both timing out
  on `text "^q: sessions"`, fixed by plan 001). Those two are the known state —
  note them and carry on, since this spike ships no code. Anything else is a STOP.
- Neither muse nor grok is installed **and** their public documentation does not
  say whether a hook surface exists. Report that: the spike's answer is then
  "unknowable from here", and the operator needs to decide whether to install
  them or park the question. Do not guess.
- A descriptor change looks obviously right and tempting. It is out of scope by
  design — an unverified dialect ships *wrong* status, and a dot that lies is
  worse than a grey one. Record the proposed change in the findings doc instead.
- You conclude a sixth dialect is needed and it is larger than
  `pi_extension.rs` (~250 lines plus a `.ts` payload). Report the estimate
  rather than specifying something nobody has budgeted.
- A prototype run was made without `NEBULA_RUNTIME_DIR` and `NEBULA_DATA_DIR`
  set, or wrote a managed hooks file into one of the operator's real checkouts.
  Stop immediately and report it, naming the paths touched.
- `crates/nebula-core/src/harness.rs`'s `builtin()` no longer matches the muse or
  grok excerpts above.

## Maintenance notes

- Muse and grok are two problems, not one, and the findings doc should keep them
  apart to the end. Grok needs a dialect and nothing else. Muse needs a dialect,
  a resume mechanism, a system-append flag and an honest Effort row — four
  independent gaps, each shippable alone.
- `registry.rs:4173` asserts `grok.hook_dialect() == None`. It is a correct
  test of today's behaviour and it will fail the moment grok gains a dialect.
  Whoever makes that change owns updating it; flag it in the findings doc so it
  is not discovered in CI.
- The honest fallback is worth taking seriously. A grid whose premise is a
  colored dot has two defensible answers for a CLI that cannot report — tell the
  user before they launch it, or do not offer it — and "ship it silently
  second-class" is not one of them. If the verdict for either CLI is (c), the
  follow-up work is UI and docs, not hooks.
- Upstream moves. Both of these CLIs are young, and a dialect that does not
  exist today may ship next month. Whatever the verdict, the findings doc should
  record the versions it was true for, the way `pty/progress.rs:14` records
  "Claude Code 2.1.241".
- Related and out of scope: `crates/nebula-core/src/entities.rs:72-75` and
  `crates/nebula-core/src/harness.rs:780-785` both look stale about custom
  harnesses. Record them; they should land with whatever change describes them,
  not as a drive-by.
