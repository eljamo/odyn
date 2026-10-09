# Plan 006 (SPIKE): Decide the read half of the in-session agent CLI

> **Executor instructions**: This is a **design spike, not a build task**. Its
> deliverable is a written findings document, not merged product code. You may
> prototype to answer a question, but every prototype is throwaway and nothing
> under `crates/` is committed. Follow the steps in order, and if anything in
> "STOP conditions" occurs, stop and report rather than improvising. When done,
> update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 565080d..HEAD -- crates/nebula-tui/src/ipc.rs crates/nebula-tui/src/lib.rs crates/nebula-core/src/protocol.rs crates/nebula/src/cli.rs`
> If any of those changed, compare the "Current state" excerpts below against
> the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M (investigation + write-up; the implementation it specifies is separate)
- **Risk**: LOW (no product code ships from this plan)
- **Depends on**: none
- **Category**: direction
- **Planned at**: commit `565080d`, 2026-10-06
- **Deliverable**: `plans/spikes/006-agent-facing-read-surface-findings.md`

## The question this spike answers

nebula has a deliberate, growing command surface for an agent to drive nebula
from inside its own session — `nebula rename`, `nebula spawn`, `nebula open`,
`nebula worktree`. Every one of them is a **command**. Not one is a **query**.
An agent can start a sibling session and then never learn anything about it.

Three decisions have to be made before any of this is built, and they are not
the same decision:

1. **Should `nebula spawn` hand back the sibling's id?** The daemon already
   sends it and the client already throws it away. (Expected answer: yes,
   trivially.)
2. **Should there be a read command (`nebula sessions` / `nebula status`), and
   what exactly does it print?** The output is consumed by a language model, not
   a person, which makes the format a real design question rather than a
   cosmetic one.
3. **Should an agent ever be able to _wait_ on another?** This is the one with
   teeth: it turns nebula into an orchestrator and introduces failure modes
   (deadlock, orphaned waits, a parent burning tokens while idle) that none of
   the existing commands have.

A fourth, much smaller question rides along because it is the same surface:

4. **Should `nebula spawn --kind` be able to name a custom harness?** Today it
   explicitly cannot, and the limitation is written into the CLI's own help.

The spike's job is to answer all four with evidence and a recommendation, and to
specify whichever ones survive well enough that a build plan can be written from
the findings doc alone.

## Current state

### The existing one-shot surface — seven commands, zero queries

`crates/nebula-tui/src/lib.rs:64-116` is the whole surface. Each function is a
one-shot IPC client: connect to the daemon's unix socket, handshake, send one
request, read one reply, print something, exit.

```rust
pub fn run_config(op: ConfigOp) -> Result<()> {
pub fn run_tui() -> Result<Option<hosts::HostEntry>> {
pub fn shutdown_daemon_if_idle() -> Result<ipc::IdleShutdown> {
pub fn run_rename(title: String, mode: RenameMode) -> Result<()> {
pub fn run_worktree(name: String, base: Option<String>) -> Result<()> {
pub fn run_open(files: Vec<String>) -> Result<()> {
pub fn run_spawn(task: String, kind: Option<nebula_core::AgentKind>) -> Result<()> {
pub fn run_add_project(path: String) -> Result<()> {
pub fn run_kill() -> Result<()> {
```

### The design idiom: stdout is written for a model

This is the most important convention in the spike, and it is stated outright.
`crates/nebula-tui/src/ipc.rs:318-326`:

```rust
/// CLI: `nebula spawn "<task>" [--kind <claude|codex|cursor>]` from inside
/// an agent session — ask the daemon to start a new agent beside this one,
/// in the same worktree, opening on `task` as its first prompt. The caller
/// is untouched: no relocation, no turn-end wait. Never spawns a daemon: no
/// daemon means no session to sit beside.
///
/// What this prints is read by the model that ran it, so it says what
/// happened and that this session carries on. A daemon-side refusal (a
/// blank task, a missing CLI) is a nonzero exit the model reports.
```

And the output itself, `ipc.rs:350-354`:

```rust
    let harness = kind.map(|k| format!("{} ", k.as_str())).unwrap_or_default();
    println!(
        "started a new {harness}session in this worktree; it is working on that task now and \
         shows in the sessions list. This session is unaffected — carry on."
    );
```

Note what that sentence promises: the sibling *"shows in the sessions list"* — a
list no command can read. `nebula open`'s output has the same shape
(`ipc.rs:406-409`):

```rust
    println!(
        "opened {count} {noun} in nebula's file tabs — the user is looking at them now, one tab \
         each, with a preview and an editor. Don't paste their contents into your reply; carry on."
    );
```

Every one of these strings is written in the imperative, tells the model what
changed, and ends by telling it to carry on. A read command's output has to fit
that register: dense, unambiguous, and safe for a model to act on without
re-reading. That is question 2.

### The sibling's id is already on the wire and already discarded

`crates/nebula-core/src/protocol.rs:230-239` — the request and what the daemon
answers with:

```rust
    /// with `Ack { created: Some(EntityId::Agent(..)) }`; the row reaches
    /// every TUI as an ordinary `EntityUpserted`.
    SpawnSiblingAgent {
        req_id: u64,
        id: AgentId,
        kind: Option<AgentKind>,
        starting_prompt: String,
    },
```

`crates/nebula-core/src/protocol.rs:468-471` — the reply carries the id:

```rust
    Ack {
        req_id: u64,
        created: Option<EntityId>,
    },
```

`crates/nebula-tui/src/ipc.rs:246-251` — but the one-shot client's local reply
type has no room for it:

```rust
/// What the daemon said back to a one-shot request.
enum Reply {
    Ack,
    /// The daemon's own message for a request it declined.
    Error(String),
}
```

…and `ipc.rs:257-267` pattern-matches the field away with `..`:

```rust
async fn await_reply(conn: &mut Connection, req_id: u64) -> Result<Reply> {
    loop {
        match read_frame::<ServerEvent, _>(&mut conn.stream).await? {
            Some(ServerEvent::Ack { req_id: r, .. }) if r == req_id => return Ok(Reply::Ack),
            Some(ServerEvent::Error {
                req_id: Some(r),
                message,
            }) if r == req_id => return Ok(Reply::Error(message)),
            Some(_) => continue,
            None => bail!("{CLOSED_BEFORE_REPLY}"),
        }
```

So question 1 is a `Reply::Ack(Option<EntityId>)` away. Confirm that yourself in
step 2 rather than taking it on trust.

### The daemon already answers queries

Two request/reply query pairs exist, so a read command needs no new daemon
concept:

- `ClientRequest::Subscribe` (`protocol.rs:31-32`) — *"Reply is one Snapshot,
  then deltas stream on this connection forever."* The `Snapshot` variant is at
  `protocol.rs:457-462` and carries `projects`, `worktrees`, agents, terminals
  and `links`.
- `ClientRequest::GetMetrics` (`protocol.rs:348`) → `ServerEvent::Metrics`
  (`protocol.rs:539-540`) — a clean one-shot query already consumed by the TUI's
  memory modal.

`GetMetrics` is the closer precedent for a one-shot: it answers once and is
done, where `Subscribe` opens a stream the client would have to hang up on after
the first frame.

### The one-shot scaffolding a new command would reuse

From `crates/nebula-tui/src/ipc.rs`:

- `ONE_SHOT_REQ_ID: u64 = 1` (`:14`) — every one-shot uses request id 1.
- `CLOSED_BEFORE_REPLY` (`:16`) — the shared hang-up message.
- `async fn try_connect(sock: &Path) -> Result<UnixStream>` (`:56-58`).
- `async fn handshake(stream) -> Result<Connection>` (`:94-104`) — sends
  `Hello { protocol_version: PROTOCOL_VERSION }`.
- `fn current_agent_id(verb: &str) -> Result<String>` (`:242-244`) — resolves
  the caller's own agent from the `NEBULA_AGENT_ID` environment variable.
- `async fn await_ack(conn, req_id) -> Result<()>` (`:273-278`).

Every in-session command refuses to start a daemon — `ipc.rs:334-336`:

```rust
    let Ok(stream) = try_connect(&sock).await else {
        bail!("no nebula daemon is running — no session started");
    };
```

A read command must do the same: no daemon means nothing to report, and
spawning one from inside an agent session would be wrong.

### Why `--kind` cannot name a custom harness today

`crates/nebula/src/cli.rs:56-71` says it in its own doc comment:

```rust
/// `--kind` for `nebula spawn`: one of the agent CLIs nebula runs. A bare
/// `custom` is never accepted: custom harnesses carry a registry id the
/// flag cannot name, so they launch from the TUI picker and presets.
fn parse_agent_kind(s: &str) -> Result<nebula_core::AgentKind, String> {
    nebula_core::AgentKind::parse(s).ok_or_else(|| {
        format!(
            "unknown harness `{s}` — expected one of {} (custom harnesses launch from the TUI)",
            nebula_core::AgentKind::ALL
                .iter()
                .filter(|k| **k != nebula_core::AgentKind::Custom)
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}
```

Custom harnesses are otherwise first class. `crates/nebula-core/src/entities.rs:72-75`:

```rust
    /// A user-defined harness from the `custom_harnesses` registry: the
    /// entry id travels beside the session (see `Agent::custom_harness`),
    /// never in this variant. Launches with the entry's program and model
    /// flag, with process-based status and no resume — like [`AgentKind::Muse`].
```

And the pattern for carrying the id alongside the kind already exists on the
other create path. `crates/nebula-core/src/protocol.rs:92-101`:

```rust
    CreateAgent {
        req_id: u64,
        worktree: WorktreeId,
        name: String,
        kind: AgentKind,
        /// Registry id of the custom harness, when `kind` is
        /// [`AgentKind::Custom`]. Persisted with the row like `model`.
        #[serde(default)]
        custom_harness: Option<String>,
```

The daemon resolves both halves together already —
`crates/nebula-daemon/src/registry.rs:1143`:

```rust
        let harness = resolve_harness(kind, custom_harness.as_deref())?;
```

So question 4 is: add the same `#[serde(default)] custom_harness: Option<String>`
field to `SpawnSiblingAgent`, and decide the CLI spelling. There is no deep
design question here — only the spelling and whether the id should be validated
client-side or left to the daemon's existing create-time refusal.

### Repo conventions the findings doc must respect

- **Comments explain why, at length.** `ARCHITECTURE.md`, `ipc.rs:318-326` and
  `Cargo.toml:38-54` are the register: full sentences, the reason before the
  mechanism, and the failure that motivated it where there was one. Any
  interface the findings doc proposes should be documented to that standard, so
  a build plan can lift the prose.
- **The domain vocabulary is capitalised in prose**: SESSION, WORKTREE, PROJECT,
  AGENT, PREWARM POOL, QUICK PROMPT, FILE TABS, LOCKED PANE. `ARCHITECTURE.md`
  is the glossary — read it before writing, and use its terms rather than
  inventing synonyms.
- **"Input is not action"** (`ARCHITECTURE.md:95`) and **"A key handler never
  blocks"** (`ARCHITECTURE.md:97`) are standing architectural rules in the TUI.
  They do not bind a one-shot CLI client, which is its own process — but say so
  explicitly in the findings doc rather than leaving a reader to wonder.
- Protocol additions use `#[serde(default)]` on new fields so an older peer's
  frames still decode (see `CreateAgent`'s `custom_harness`, `cloud_prompt`).
  Note also that `PROTOCOL_VERSION` (`protocol.rs:9`) is gated on **strict
  equality** by the daemon (`nebula-daemon/src/server.rs:67`), so a protocol
  change means a version bump and a daemon restart, not silent tolerance.

## Commands you will need

All read-only except the prototype step, which runs in a throwaway worktree.

| Purpose | Command | Expected on success |
|---|---|---|
| Read the protocol | `grep -n 'Subscribe\|GetMetrics\|Snapshot {\|Ack {' crates/nebula-core/src/protocol.rs` | the line numbers cited above |
| Typecheck | `cargo check --workspace --all-targets` | exit 0 |
| Tests | `cargo test --workspace` | exit 0, every target `ok` (see STOP conditions) |
| The whole gate | `make ci` | exit 0 (see STOP conditions) |
| Throwaway worktree | `git worktree add /tmp/nebula-spike-006 HEAD` | a detached checkout to prototype in |
| Remove it | `git worktree remove --force /tmp/nebula-spike-006` | gone |
| Isolated run | `NEBULA_RUNTIME_DIR=/tmp/nb-006-rt NEBULA_DATA_DIR=/tmp/nb-006-data cargo run -- <args>` | never touches real sessions |

**The isolation variables are mandatory for any run.** `NEBULA_RUNTIME_DIR` and
`NEBULA_DATA_DIR` (`nebula-core/src/env.rs:16-18`) are what keep a prototype off
the operator's live daemon and live sessions. A prototype run without them can
attach to, rename, or kill the operator's real work. The repo's own `Makefile`
does exactly this for `make dev` (`Makefile:52-53`) — follow it.

## Scope

**In scope** — you may create or modify only:
- `plans/spikes/006-agent-facing-read-surface-findings.md` (create; create the
  `plans/spikes/` directory if absent)
- `plans/README.md` — your status row only

**Out of scope** (do NOT commit changes to any of these):
- Everything under `crates/`. A prototype lives in the throwaway worktree and is
  discarded. If you believe a change belongs in the tree, it goes in the findings
  doc as a specification, not in a commit.
- `crates/nebula-core/src/protocol.rs` specifically. `PROTOCOL_VERSION` is
  strict-equality gated, so touching it is a coordinated change across both
  halves and a version bump — exactly the kind of thing this spike exists to
  specify rather than do.
- `ARCHITECTURE.md` and `docs/`. Promoting any of this into the permanent docs is
  the operator's call, made after reading your findings.
- The other plans in `plans/`, except your own status row.
- Anything to do with plans 001–005 (the test suite, CI, toolchain, release
  checksums, dependency advisories).

## Steps

### Step 1: Read the surface and the glossary

Read, in this order:

1. `ARCHITECTURE.md` in full — in particular the "Domain tree" and "How the
   pieces talk" sections, and the Crate layout table. You need its vocabulary.
2. `crates/nebula-tui/src/ipc.rs` in full. It is ~870 lines and it is the whole
   one-shot client surface. Pay attention to every `println!` — they are the
   corpus of "what nebula says to a model".
3. `crates/nebula-core/src/protocol.rs` — the `ClientRequest` and `ServerEvent`
   enums end to end.
4. `crates/nebula/src/cli.rs:120-200` — the `Rename`, `Spawn` and `Open`
   subcommand definitions, including their long help text.

**Verify**: in the findings doc, open a section `## The existing surface` that
lists every in-session command, the request it sends, and — verbatim — the
string it prints on success. If your list does not match the nine `pub fn`s at
`lib.rs:64-116`, you have missed one.

### Step 2: Confirm the discarded id (question 1)

Establish by reading, not by assumption:

- Does `ServerEvent::Ack` carry `created: Option<EntityId>`? (`protocol.rs:468-471`)
- Does the daemon actually populate it for `SpawnSiblingAgent`? Find the send
  site in `crates/nebula-daemon/src/server.rs` and
  `crates/nebula-daemon/src/registry.rs` and quote it.
- Does `ipc.rs::await_reply` discard it? (`ipc.rs:257-267`)

**Verify**: a findings section `## Question 1 — should spawn return the id?`
with: the three quotes, a yes/no recommendation, and the exact signature change
(`enum Reply` and `await_reply`) that would carry the id through. If the daemon
turns out *not* to populate `created` for this request, say so — that changes the
answer from "trivial" to "small but real".

### Step 3: Specify the read command (question 2)

This is the substantial half of the spike. Decide and write down:

- **Which request.** `GetMetrics`-style one-shot (a new request/reply pair) or
  `Subscribe` + read one `Snapshot` + hang up. Give the trade-off: a new pair
  needs a protocol bump; `Subscribe` reuses what exists but opens a stream whose
  "read one frame and leave" contract is not what the variant documents
  (`protocol.rs:31-32` says the deltas stream *"forever"*).
- **What it returns.** The `Snapshot` carries the whole tree — every project,
  worktree, agent and terminal. An agent asking "how is my sibling doing"
  needs far less. Decide the scope: this worktree's sessions, this project's,
  or everything. Justify it from the caller's actual need, and note the
  information-exposure angle: a session in project A learning the names of
  sessions in project B is a disclosure the current design never makes.
- **What it prints.** This is the design question. The output is read by a model.
  Produce at least two concrete candidate formats in the findings doc, rendered
  with realistic data, and recommend one. Hold them against the existing
  register (step 1's corpus): dense prose, or a table, or stable key/value
  lines a model can parse without ambiguity. Say how an empty result reads, and
  how an error reads.
- **Whether `--json` exists.** A model reads prose well; a script reads JSON
  well. Decide whether both are warranted on day one or whether that is
  speculative.
- **The name.** `nebula sessions`, `nebula status`, `nebula ls`. Check for a
  collision: `nebula <dir>` is shorthand for `nebula add <dir>`
  (`crates/nebula/src/cli.rs:41-45`), so a subcommand name that is also a
  plausible directory name has a documented sharp edge. Note it.

**Verify**: a findings section `## Question 2 — the read command` containing the
chosen request shape, the chosen scope with its justification, at least two
rendered output candidates with a recommendation, the empty and error cases, a
decision on `--json`, and the chosen name with the directory-collision check
done.

### Step 4: Decide whether `wait` should exist (question 3)

Answer in writing, with the failure modes named:

- What would an agent do with it that it cannot do today?
- What happens when the awaited session is archived, deleted, or its CLI dies
  mid-turn? What happens on a daemon restart — note that `nebula reload` execs
  the daemon over itself (`nebula-daemon/src/handoff.rs`) and every connected
  client loses its connection, so a long-lived `wait` would be severed.
- What stops two agents waiting on each other?
- Is there a timeout, and what does the model see when it fires?
- Does a waiting agent cost tokens while it waits? (It holds a turn open.)

A "no, not worth doing" is a perfectly good answer and should be recorded with
its reasoning so nobody re-opens it. A "yes, but only with a mandatory
`--timeout`" is also fine. What is not fine is leaving it unanswered.

**Verify**: a findings section `## Question 3 — should an agent wait on another?`
with a recommendation, every failure mode above addressed by name, and — if the
answer is yes — the command's signature and timeout behaviour.

### Step 5: Specify the custom-harness fix (question 4)

Small and concrete. Decide:

- The CLI spelling: `--kind <id>` widened to accept registry ids, or a separate
  `--harness <id>`. Note that `parse_agent_kind` (`cli.rs:59-71`) currently
  rejects anything outside `AgentKind::ALL` minus `Custom`, and that its error
  message would need rewriting either way.
- Whether the id is validated in the client or left to the daemon. The daemon
  already refuses an invalid entry at create time with a reason —
  `HarnessDescriptor::problem()` (`harness.rs:297-340`) and
  `CustomHarness::problem()` (`harness.rs:~832`) — and `registry.rs:1143`
  resolves it. A client-side check means a faster, clearer error; a daemon-side
  check means one source of truth.
- How the registry is enumerated for the error message. `nebula config harnesses`
  already prints the resolved set (`cli.rs:441-446`); find the function behind it
  and say whether `parse_agent_kind` can reuse it.

**Verify**: a findings section `## Question 4 — custom harnesses from spawn` with
the chosen spelling, the chosen validation site with its reasoning, the new
protocol field written out (`#[serde(default)] custom_harness: Option<String>` on
`SpawnSiblingAgent`), and a note that the field addition requires a
`PROTOCOL_VERSION` bump because `nebula-daemon/src/server.rs:67` gates on
equality.

### Step 6: Prototype only if a question is still open

Skip this step entirely if steps 2–5 are answered from reading. Prototype only
to settle a question reading could not — most likely "does this output format
actually read well to a model" or "does `Subscribe`-then-hang-up behave".

If you do prototype:

```sh
git worktree add /tmp/nebula-spike-006 HEAD
cd /tmp/nebula-spike-006
# ... edit, then always with isolation:
NEBULA_RUNTIME_DIR=/tmp/nb-006-rt NEBULA_DATA_DIR=/tmp/nb-006-data cargo run -- <args>
```

When finished:

```sh
cd /Users/ellis/Code/rust/odyn   # or wherever the main checkout is
git worktree remove --force /tmp/nebula-spike-006
rm -rf /tmp/nb-006-rt /tmp/nb-006-data
```

**Verify**: `git worktree list` shows only the original checkout, and
`git status --short` in the main checkout lists nothing under `crates/`.

### Step 7: Write the findings doc and the recommendation

Assemble `plans/spikes/006-agent-facing-read-surface-findings.md` with these
sections, in this order:

```markdown
# Spike 006 findings: the read half of the in-session agent CLI

- **Spiked at**: commit <short SHA>, <YYYY-MM-DD>
- **Question**: <one sentence>
- **Recommendation**: <one paragraph: what to build, what not to, in what order>

## The existing surface
## Question 1 — should spawn return the id?
## Question 2 — the read command
## Question 3 — should an agent wait on another?
## Question 4 — custom harnesses from spawn
## Open questions for the operator
## What I did not investigate
```

The `## Open questions for the operator` section is required and must not be
empty — if a spike leaves nothing for the maintainer to decide, it has
overstepped. Put anything that is a taste or product call there rather than
deciding it.

**Verify**: every section above is present and non-empty; the Recommendation
paragraph names which of the four questions should become build work and in what
order.

### Step 8: Final check

**Verify**:
- `git status --short` lists only
  `plans/spikes/006-agent-facing-read-surface-findings.md` and `plans/README.md`.
- `git worktree list` shows one entry.
- `git diff --stat -- crates Cargo.toml Cargo.lock` produces no output.

## Test plan

A spike ships no product code, so there are no new tests. The verification is
the findings doc's completeness, checked by the done criteria below.

Two notes for whoever writes the build plan from these findings, which belong in
the findings doc's own maintenance section:

- The in-session commands are covered end to end by `crates/nebula/tests/e2e_pty.rs`,
  which spawns real daemons into tempdirs. `grep -n 'env::AGENT_ID' crates/nebula/tests/e2e_pty.rs`
  shows the pattern (around `:4073`, `:4119`, `:4546`): set `NEBULA_RUNTIME_DIR`
  and `NEBULA_AGENT_ID` on a child process and assert what the daemon did. Any
  new in-session command gets a test of that shape.
- Output strings read by a model deserve a unit test pinning the wording, the way
  `alerts.rs`'s `notifier_command_names_the_session_and_its_place` pins its
  notification text. Say so in the findings doc.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `plans/spikes/006-agent-facing-read-surface-findings.md` exists
- [ ] It contains all eight required H1/H2 headings from step 7 — verify with
      `grep -c '^## ' plans/spikes/006-agent-facing-read-surface-findings.md` returning at least 7
- [ ] `grep -c 'Recommendation' plans/spikes/006-agent-facing-read-surface-findings.md` returns at least 1
- [ ] Each of `## Question 1`, `## Question 2`, `## Question 3`, `## Question 4`
      is followed by at least 10 lines of content before the next `## `
- [ ] `## Open questions for the operator` is present and non-empty
- [ ] Question 2's section contains at least two rendered output candidates
- [ ] Question 3's section reaches an explicit yes or no
- [ ] `git diff --stat -- crates Cargo.toml Cargo.lock` produces no output
- [ ] `git worktree list` shows exactly one worktree
- [ ] `git status --short` lists only the findings doc and `plans/README.md`
- [ ] `plans/README.md` status row for 006 updated

## STOP conditions

Stop and report back (do not improvise) if:

- `cargo test --workspace` or `make ci` fails. Two `e2e_tui` tests are known to
  fail at `565080d` and plan 001 fixes them — if you see exactly
  `nebula_open_from_inside_a_session_raises_the_file_tabs` and
  `tui_drag_past_the_pane_top_autoscrolls_and_copies_the_run` failing on
  `timed out waiting for: text "^q: sessions"`, that is the known state: note it
  and carry on, since this spike ships no code. **Any other failure is a STOP.**
- `crates/nebula-tui/src/ipc.rs:246-267` does not match the excerpts above —
  somebody has already changed how one-shot replies are read, and question 1 may
  be settled.
- `ServerEvent::Ack` has no `created` field, or the daemon does not populate it
  for `SpawnSiblingAgent`. Report what you found; the answer to question 1
  changes shape.
- You conclude a question cannot be answered without shipping product code.
  Report which one and why — do not ship it.
- A prototype run was made without `NEBULA_RUNTIME_DIR` and `NEBULA_DATA_DIR`
  set. Stop immediately and report it: that run may have touched the operator's
  live daemon and real sessions, and they need to know.
- You find yourself wanting to commit anything under `crates/`.

## Maintenance notes

- The four questions are deliberately separable and should stay separable in the
  build plans that follow. Question 1 and question 4 are each an afternoon;
  question 2 is a design commitment to an output format that models will come to
  depend on; question 3 changes what nebula *is*. Bundling them would force the
  smallest change to wait on the largest.
- Any new `ClientRequest` or new field on an existing one needs a
  `PROTOCOL_VERSION` bump (`crates/nebula-core/src/protocol.rs:9`), because
  `crates/nebula-daemon/src/server.rs:67` compares versions for equality and
  refuses a mismatched client. The user-visible consequence is that the daemon
  must restart — `nebula reload` keeps sessions alive across it
  (`nebula-daemon/src/handoff.rs`), which is worth saying in the findings doc.
- A reviewer of the eventual build work should scrutinise the output strings
  hardest. They are an interface to a model, they will be copied into agent
  prompts and skills, and they are much harder to change later than a flag.
- Related, and deliberately not in this spike's scope: the half-retired `links`
  entity recorded in `plans/README.md`. It is a separate question about a
  separate surface.
