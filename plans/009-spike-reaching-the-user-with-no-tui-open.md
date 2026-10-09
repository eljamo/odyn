# Plan 009 (SPIKE): Decide how nebula reaches you when no TUI is open

> **Executor instructions**: This is a **design spike, not a build task**. Its
> deliverable is a written findings document, not merged product code. You may
> prototype to answer a question, but every prototype is throwaway and nothing
> under `crates/` is committed. Follow the steps in order, and if anything in
> "STOP conditions" occurs, stop and report rather than improvising. When done,
> update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 565080d..HEAD -- crates/nebula-tui/src/event_loop/alerts.rs crates/nebula-tui/src/event_loop.rs crates/nebula-daemon/src/registry.rs crates/nebula-daemon/src/worktree_hooks.rs crates/nebula-daemon/src/config.rs`
> If any of those changed, compare the "Current state" excerpts below against
> the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M (investigation + write-up; mostly design, implementation separate)
- **Risk**: LOW (no product code ships from this plan)
- **Depends on**: none
- **Category**: direction
- **Planned at**: commit `565080d`, 2026-10-06
- **Deliverable**: `plans/spikes/009-reaching-the-user-findings.md`

## The question this spike answers

nebula's premise is that the daemon outlives its clients. `ARCHITECTURE.md:10`:

> **TUI** (`nebula`) — a ratatui client. Quit it and nothing dies; relaunch and
> scrollback is replayed.

So agents run unattended by design. But every way nebula reaches a user lives in
the **client**: the DONE SOUND, the FEEDBACK SOUND and the desktop notification
are all in `crates/nebula-tui/src/event_loop/alerts.rs`, driven from the TUI's
frame loop. There is no notification path in `nebula-daemon` at all. Close the
TUI — the thing you are explicitly invited to do — and nothing can tell you an
agent is waiting.

The daemon is the half that knows and the half that lasts. It owns the status
machine, so it sees the exact edge where a turn stops to ask; it reads
`config.json` itself; and it already runs user-provided executables on events,
with a worked-out trust model.

The spike's job is to decide **what a daemon may do unattended**, and that is
mostly a policy question rather than a coding one. Four shapes to weigh:

1. **A notify hook** — a user-provided executable the daemon runs on the
   status edge, exactly like WORKTREE HOOKS. The user supplies the transport, so
   nebula needs no opinion about desktops, networks or secrets.
2. **A webhook / push URL from config** — the daemon POSTs to a URL the user
   configured (ntfy, Slack, a personal endpoint). Machine-agnostic, but nebula
   now holds a URL that may be a credential and makes outbound network calls.
3. **Daemon-side desktop notification** — move `notify_desktop` into the
   daemon. Simplest for the common case, wrong on every headless box, and the
   daemon currently has no way to know which it is on.
4. **Nothing** — the client is the right place, and the answer to "I closed the
   TUI" is "leave it open". Record why, so it stays closed.

Two sub-questions have to be answered first:

- **Which edges are worth reporting?** Today the client rings on a turn
  *finishing* and on a turn *stopping to ask*. A daemon-side path could also
  report an agent dying, a session exiting, a worktree hook failing. Deciding the
  set is part of the design.
- **How does the daemon avoid double-reporting?** If a TUI *is* open, the client
  already notifies. Two notifications for one event is worse than one.

## Current state

### Everything that reaches the user is client-side

`crates/nebula-tui/src/event_loop/alerts.rs:1-10` — the module's own summary:

```rust
//! The two ways nebula reaches a user who is not looking at it: the DONE
//! SOUND and FEEDBACK SOUND (`Config::done_sound` / `Config::feedback_sound`,
//! played here), and the desktop notification a session that stops to ask
//! posts while the terminal window is in the background. `event_loop.rs`
//! decides *when* — the status edges, the per-frame drain of
//! `App::pending_ding` and `App::pending_feedback`, the focus and ssh
//! gates — this module only does the reaching, and fails soft: an `afplay`
//! that won't start falls back to the bell, a notifier that is missing or
//! exits non-zero is a debug line in tui.log.
```

The drive site, `crates/nebula-tui/src/event_loop.rs:739-762`:

```rust
        // A turn reached FINISHED: ring the DONE SOUND. The bell goes out
        // through the same terminal as the OSC writes above, so over ssh it
        // rings the terminal the user is sitting at. CONFIG.JSON is read
        // fresh, like every other setting.
        if std::mem::take(&mut app.pending_ding) {
            if let Some(sound) = crate::config::Config::load().done_sound() {
                alerts::play_sound(terminal.backend_mut(), sound);
            }
        }

        // One or more turns stopped to ask the user: ring the FEEDBACK
        // SOUND once for the lot and, while the terminal window is in the
        // background, name each of them in a desktop notification — never
        // over ssh, where the desktop is the wrong machine's. `off` is
        // silence for both.
        let alerts = std::mem::take(&mut app.pending_feedback);
        if !alerts.is_empty() {
            if let Some(sound) = crate::config::Config::load().feedback_sound() {
                alerts::play_sound(terminal.backend_mut(), sound);
                if !app.window_focused && !app.is_remote {
                    alerts::notify_desktop(&alerts);
                }
            }
        }
```

Note the three gates, because they encode most of the policy that a daemon-side
path would have to re-derive: the sound is configurable and can be `off`
(`feedback_sound()` returning `None` *also* stands down the notification —
`config.rs:2473-2476` calls it "the one switch for both"); the notification only
fires when the terminal window is **not** focused; and it never fires when the
session is remote, *"where the desktop is the wrong machine's"*.

The flags are set on the status edge, client-side:
`crates/nebula-tui/src/event_loop.rs:10275` sets `pending_ding`, and `:10309-10310`
pushes a `FeedbackAlert` via `alerts::alert_for(&app.tree, &agent)`
(`alerts.rs:42`). `App::pending_ding` and `App::pending_feedback` are at
`crates/nebula-tui/src/app.rs:3595` and `:3602`.

The transports, `alerts.rs:90-112`: `osascript -e 'display notification …'` on
macOS, `notify-send --app-name=nebula …` elsewhere, and `afplay` or a bare BEL
byte for sound. `Sound` is `Bell | File(PathBuf)`
(`crates/nebula-tui/src/config.rs:2451-2457`), and
`Config::done_sound()` / `Config::feedback_sound()` (`:2465`, `:2477`) resolve a
configured name to a file **only** on macOS, on a local terminal, and when the
file exists — over ssh they fall back to the bell, because *"`afplay` would ring
the remote box"*.

Confirm the daemon has none of this:

```sh
grep -rn 'osascript\|notify-send\|afplay' crates/nebula-daemon/src --include='*.rs'
```

At `565080d` that returns nothing.

### The daemon already sees the exact edge

`crates/nebula-daemon/src/registry.rs:298-322` — the status machine's effects
are applied and broadcast here:

```rust
    fn apply_status_effects(&self, agent_id: &AgentId, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::SetStatus(status) => {
                    let (changed_at, unseen) = match self.store.set_agent_status(agent_id, status) {
                        Ok(stamped) => stamped,
                        Err(e) => {
                            tracing::warn!(error = %e, "persist status failed");
                            (nebula_core::clock::now_ms(), false)
                        }
                    };
                    self.broadcast(ServerEvent::StatusChanged {
                        agent: agent_id.clone(),
                        status,
                        changed_at,
                        unseen,
                    });
                }
```

`Daemon::broadcast` (`registry.rs:325-327`) is a `tokio::sync::broadcast::Sender<ServerEvent>`
(`registry.rs:125`, capacity 1024 at `:187`). So the edge is already a
first-class, in-process event with a persisted timestamp — a daemon-side
notifier is a subscriber, not a new mechanism.

### WORKTREE HOOKS: the precedent that answers most of the policy

The daemon **already** runs user-provided executables on events, and the trust
model is written down. `crates/nebula-daemon/src/worktree_hooks.rs:1-22`:

```rust
//! WORKTREE HOOKS: a user-provided executable the DAEMON runs after it
//! creates or deletes a worktree, so a project can provision and release
//! what a checkout owns outside its own directory — a dev-server port, a
//! Caddy route, a docker compose project, a database. Configured per
//! repository in git config, read fresh at each use:
//!
//! ```sh
//! git config nebula.worktreeCreateHook /absolute/path/to/script
//! git config nebula.worktreeDeleteHook /absolute/path/to/script
//! ```
//!
//! Never a file inside the checkout — a committed hook would run whatever
//! a clone brought with it, which is why git itself refuses hooks from the
//! working tree. Git resolves the key the usual way, so a `--global` value
//! serves every project and a repo's own `.git/config` overrides it.
```

And the spawn, `worktree_hooks.rs:126-160`, which is the exemplar for how this
repo runs a user's program — read every comment:

```rust
    // Output lands in unlinked temp files, not pipes. A hook that starts a
    // dev server in the background and exits 0 leaves that server holding
    // its stdout; a pipe would keep the wait open until the timeout and
    // then report a success as "timed out". A file is inherited harmlessly
    // and the wait below is on the process alone.
    let stdout = tempfile::tempfile().context("hook output file")?;
    let stderr = tempfile::tempfile().context("hook output file")?;
    let mut child = tokio::process::Command::new(program)
        .arg(ctx.repo)
        .arg(ctx.worktree)
        .env(env::HOOK, label)
        .env(env::WORKTREE_BRANCH, ctx.branch)
        .env(env::WORKTREE_ID, ctx.id.as_str())
        .current_dir(ctx.repo)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr.try_clone()?))
        // Its own process group, so a timeout takes everything it started
        // down with it, not just the script.
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
```

Four decisions already made here that a notify hook would inherit for free: the
path comes from **git config, never the checkout**; the program is spawned
**directly, with no shell**; it gets its **own process group** so a timeout kills
everything it started; and its output goes to **temp files, not pipes**. Shape 1
is therefore much cheaper than it looks, and arrives with its trust story
already argued.

### What the daemon's config already holds

`crates/nebula-daemon/src/config.rs:14-70` — the daemon reads its own view of
`config.json`:

```rust
pub struct Config {
    pub prewarm_agents: bool,
    pub prewarm_sessions: bool,
    pub session_idle_timeout: String,
    pub worktree_base_branch: String,
    pub custom_harnesses: Vec<nebula_core::harness::CustomHarness>,
    pub harnesses: BTreeMap<String, nebula_core::harness::HarnessOverride>,
    pub projects: BTreeMap<PathBuf, ProjectConfig>,
}
```

`config.rs:60-62` records the split: *"the Project tab, of which the daemon reads
one. The rest (`open_command`) are the"* — i.e. settings are deliberately
partitioned into what the daemon acts on and what the client does.
`done_sound` and `feedback_sound` live on the **client** side of that line
today. Whether a notification setting crosses it is one of the things to decide.

### Repo conventions the findings doc must respect

- **Comments explain why, at length, and name the failure that motivated them.**
  `worktree_hooks.rs:126-130` and `alerts.rs:1-10` are the register. Any
  interface this spike proposes should be documented to that standard so a build
  plan can lift the prose.
- **The domain vocabulary is capitalised in prose**: DONE SOUND, FEEDBACK SOUND,
  WORKTREE HOOKS, STATUS DOT, SESSION, AGENT, DAEMON. `ARCHITECTURE.md` is the
  glossary; read its "Status path (not MCP)" paragraph (`:60`) before writing.
- **Reaching the user fails soft.** `alerts.rs:7-9`: an `afplay` that will not
  start falls back to the bell; a missing notifier is a debug line in the log.
  A notification path must never be able to fail a turn, block a spawn, or crash
  the daemon. Compare `registry.rs:2474-2476` on hook installation: *"a failure
  here degrades to 'no status updates', never blocks the spawn."*
- **Settings are read fresh, not cached.** `event_loop.rs:742` reloads
  `Config::load()` at the moment of use. A daemon-side notifier should match.
- **Nothing runs from inside a checkout.** `worktree_hooks.rs:12-14` and
  `ARCHITECTURE.md:81` both turn on this: a committed file must never be
  something nebula executes without a keypress. Any hook path this spike
  proposes inherits that rule.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Confirm no daemon notifier | `grep -rn 'osascript\|notify-send\|afplay' crates/nebula-daemon/src --include='*.rs'` | nothing |
| Find the status edge | `grep -n 'StatusChanged' crates/nebula-daemon/src/registry.rs` | `:309` |
| Read the hook precedent | `sed -n '1,60p' crates/nebula-daemon/src/worktree_hooks.rs` | the trust model |
| Daemon log location | `grep -rn 'daemon.log\|fn log_path' crates/nebula-core/src/paths.rs` | where a failure would be visible |
| Typecheck | `cargo check --workspace --all-targets` | exit 0 |
| Tests | `cargo test --workspace` | exit 0 (see STOP conditions) |
| Throwaway worktree | `git worktree add /tmp/nebula-spike-009 HEAD` | a detached checkout |
| Remove it | `git worktree remove --force /tmp/nebula-spike-009` | gone |
| Isolated run | `NEBULA_RUNTIME_DIR=/tmp/nb-009-rt NEBULA_DATA_DIR=/tmp/nb-009-data cargo run` | never touches real sessions |

**The isolation variables are mandatory for any run.** `NEBULA_RUNTIME_DIR` and
`NEBULA_DATA_DIR` (`crates/nebula-core/src/env.rs:16-18`) keep a prototype off
the operator's live daemon and real sessions — and this spike is *specifically*
about a daemon that acts on its own, which makes an unisolated run more dangerous
here than in most. `Makefile:52-53` does the same for `make dev`.

## Scope

**In scope** — you may create or modify only:
- `plans/spikes/009-reaching-the-user-findings.md` (create; create
  `plans/spikes/` if absent)
- `plans/README.md` — your status row only

**Out of scope** (do NOT commit changes to any of these):
- Everything under `crates/`. In particular, do not move `notify_desktop` into
  the daemon, however mechanical that looks — whether the daemon should post
  desktop notifications at all is the question, not the task.
- Any outbound network call to a real endpoint. If you test a webhook shape, POST
  to a local listener you started (`python3 -m http.server`, `nc -l`), never to
  Slack, ntfy, a real webhook URL, or anything the operator owns.
- `crates/nebula-core/src/protocol.rs` — `PROTOCOL_VERSION` (`protocol.rs:9`) is
  strict-equality gated (`crates/nebula-daemon/src/server.rs:67`), so a protocol
  change is coordinated plus a version bump. Specify; do not make.
- `ARCHITECTURE.md` and `docs/`.
- The client-side path. `alerts.rs` and its drive site work and are not being
  replaced; whatever is recommended has to coexist with them, which is
  sub-question 2.
- Anything to do with plans 001–008.

## Steps

### Step 1: Read both halves

Read, in this order:

1. `ARCHITECTURE.md`, the "Process model" and "Status path (not MCP)" paragraphs
   (`:6-16`, `:60`).
2. `crates/nebula-tui/src/event_loop/alerts.rs` in full — it is under 200 lines
   including tests, and its tests pin the notification wording.
3. `crates/nebula-tui/src/event_loop.rs:735-765` — the drive site and its three
   gates.
4. `crates/nebula-daemon/src/worktree_hooks.rs` in full. This is the single most
   important file for the spike: it is the daemon already doing the thing, with
   the trust model argued.
5. `crates/nebula-daemon/src/status.rs:1-80` — the state machine's doc header, so
   you can name the edges precisely.

**Verify**: a findings section `## What reaches the user today` listing each
transport, where it lives, what gates it, and — verbatim — the three gate
conditions at `event_loop.rs:754-762`.

### Step 2: Name the edges worth reporting

The status machine has more edges than the two the client rings on. Decide which
a daemon-side path should report, and say why for each:

- A turn **finishing** (today's DONE SOUND).
- A turn **stopping to ask** (today's FEEDBACK SOUND plus notification). Note
  from `status.rs:38-47` that Claude's `permission_prompt` notification is
  *deferred* behind a grace period — so "stopped to ask" is not instantaneous and
  a notifier must not fire on a transient.
- An agent **dying** or its session exiting.
- A **worktree hook failing** (`worktree_hooks.rs` already reports these into the
  daemon log).
- Anything else `status.rs` distinguishes.

For each, say whether it is worth waking someone for. The default should be
conservative: a notifier that fires often is one people mute.

**Verify**: a findings section `## Which edges` with a table of edge, worth
reporting (yes/no), and the reason.

### Step 3: Solve the double-reporting problem

If a TUI is open it already notifies. Decide the mechanism that stops two
notifications for one event, and be concrete:

- Does the daemon know whether any client is attached? Find out — look for the
  client/subscriber bookkeeping in `crates/nebula-daemon/src/server.rs` and
  `registry.rs` (`attach_counts` at `registry.rs:525` and `session_interest` at
  `:583` are nearby; establish whether either answers "is a TUI subscribed").
- Is "no client subscribed" the right condition, or is it "no client subscribed
  **and** none has been for N seconds"? A TUI being relaunched would otherwise
  open a gap.
- Or does the daemon always notify and the *client* stand down when it sees the
  daemon will? That inverts the gate and may be simpler.
- What about the window-focus gate? The client knows whether its terminal window
  is focused (`app.window_focused`); the daemon cannot. Does a daemon-side notify
  fire even when the user is looking right at the grid?

**Verify**: a findings section `## Not notifying twice` naming the chosen
condition, what the daemon can actually observe (with the `file:line` that proves
it), and what happens during a TUI relaunch.

### Step 4: Weigh the four shapes

For each of the four, write down: what the user configures, what the daemon does,
what it costs, and how it fails.

Hold each against the constraint that makes this hard. `event_loop.rs:751-754`
suppresses the desktop notification over ssh *"where the desktop is the wrong
machine's"* — and a daemon on a headless box, or one reached through
`nebula ssh` / `nebula tunnel`, is exactly that situation with no client present
to know better. Say for each shape whether it survives being on a remote box.

Specific things to settle:

- **Shape 1 (notify hook).** Would it be a git-config key like the worktree
  hooks (`nebula.worktreeCreateHook`), or a `config.json` key? The worktree hooks
  chose git config because they are per-repository; a notify hook is
  per-*machine*, which argues for `config.json`. Say which and why. What argv and
  environment does it get — model it on `worktree_hooks.rs:133-140`. What timeout?
  Does its output go anywhere?
- **Shape 2 (webhook).** The daemon would hold a URL. Is that a credential? A
  Slack webhook URL is a bearer secret in a query path, and `config.json` is the
  file `nebula config export` and `nebula ssh` carry between machines
  (`ARCHITECTURE.md:71`) — so a URL put there travels. That is a real finding and
  belongs in the doc. Should such a value live in `config.local.json` instead,
  which the export deliberately excludes?
- **Shape 3 (daemon-side desktop notification).** What signal would tell the
  daemon it is on the user's desktop? `nebula_core::host::is_remote_session()`
  exists and is already used by `Config::done_sound()` — establish what it
  actually detects and whether it is meaningful for a detached daemon that holds
  no terminal (`ARCHITECTURE.md:12` says the daemon is `setsid`'d with no
  controlling terminal, which may make the check useless there).
- **Shape 4 (nothing).** State the argument fairly: the client is where the user
  is, and the cost of leaving a TUI open is near zero since it holds no state.

**Verify**: a findings section `## The four shapes weighed` with all four, each
answering: what the user configures, what the daemon does, how it fails, whether
it survives a remote box, and — for shape 2 — whether the URL is a secret that
must stay out of the exported bundle.

### Step 5: Recommend one and specify it

Pick one shape (including shape 4) and specify it well enough that a build plan
can be written from the findings doc alone:

- The configuration surface: which file, which key, what the default is, and
  whether it needs a Settings-tab row. `SettingKind`
  (`crates/nebula-tui/src/config.rs:385-418`) is the list of existing rows, and
  `DoneSound`/`FeedbackSound` are already in it — so a related row has
  precedent and a place.
- Where in the daemon it hooks in. `apply_status_effects`
  (`registry.rs:298`) is where the edge is known; a subscriber on
  `Daemon::events` (`registry.rs:125`) is the looser coupling. Say which and why.
- Failure behaviour, explicitly: it must never block a turn, fail a spawn, or
  panic the daemon. Where does a failure become visible — the daemon log? Name
  the log path.
- Rate limiting. Five agents finishing at once should not mean five
  notifications; `event_loop.rs:754-756` already rings **once for the lot**.
  Match that.
- What a user does to turn it off, and what the default is. Default **off** is
  the conservative choice for anything that sends data off the machine.

**Verify**: a findings section `## Recommended shape` covering the config
surface, the hook point with its `file:line`, failure behaviour, rate limiting,
the default, and the off switch.

### Step 6: Prototype only if a question is still open

Skip entirely if steps 2–5 are answered from reading. Prototype only to settle
something reading could not — most likely "does `is_remote_session()` mean
anything in a detached daemon" or "can the daemon tell whether a TUI is
subscribed".

If you do, use the throwaway worktree and **always** the isolation variables:

```sh
git worktree add /tmp/nebula-spike-009 HEAD
cd /tmp/nebula-spike-009
NEBULA_RUNTIME_DIR=/tmp/nb-009-rt NEBULA_DATA_DIR=/tmp/nb-009-data cargo run
```

If you test a webhook shape, POST to a local listener only:

```sh
python3 -m http.server 8099 --bind 127.0.0.1    # in another shell
# then point the prototype at http://127.0.0.1:8099/
```

Clean up:

```sh
git worktree remove --force /tmp/nebula-spike-009
rm -rf /tmp/nb-009-rt /tmp/nb-009-data
```

**Verify**: `git worktree list` shows one entry; `git status --short` in the main
checkout lists nothing under `crates/`; no request was sent to any non-loopback
address.

### Step 7: Write the findings doc

Assemble `plans/spikes/009-reaching-the-user-findings.md`:

```markdown
# Spike 009 findings: reaching the user when no TUI is open

- **Spiked at**: commit <short SHA>, <YYYY-MM-DD>
- **Question**: <one sentence>
- **Recommendation**: <one paragraph: which shape, its default, and what is explicitly not being built>

## What reaches the user today
## Which edges
## Not notifying twice
## The four shapes weighed
## Recommended shape
## Open questions for the operator
## What I did not investigate
```

`## Open questions for the operator` is required and must not be empty. "Should
nebula ever send anything off this machine" is a product and privacy call, not
yours — put it there.

**Verify**: every section present and non-empty; the Recommendation names one
shape, its default, and what is deliberately not being built.

### Step 8: Final check

**Verify**:
- `git status --short` lists only
  `plans/spikes/009-reaching-the-user-findings.md` and `plans/README.md`.
- `git worktree list` shows one entry.
- `git diff --stat -- crates Cargo.toml Cargo.lock` produces no output.
- No outbound request was made to any address other than loopback.

## Test plan

A spike ships no product code, so there are no new tests. Verification is the
findings doc's completeness, per the done criteria.

For whoever writes the build plan, record these in the findings doc's own
maintenance notes:

- `alerts.rs`'s tests are the model for pinning user-visible text: the
  `notifier_command_names_the_session_and_its_place` test asserts the exact
  `osascript` and `notify-send` argv, and `applescript_literals_are_escaped`
  proves a session name holding a quote or a newline cannot break out of the
  AppleScript literal. **Any new path that passes a session name to a shell,
  a URL or a notifier needs that second test.** A session name is user text.
- `worktree_hooks.rs`'s own tests are the model for testing a daemon-spawned
  user program: `grep -n 'fn ' crates/nebula-daemon/src/worktree_hooks.rs | tail -20`
  shows cases for a hook that cannot start, one that fails, one that times out,
  and one that backgrounds a child. A notify hook wants the same four.
- The status machine's tests (`crates/nebula-daemon/src/status.rs`, 1,200+ lines
  of them) are where "which edge fires when" is already pinned — a new
  notification trigger should be asserted there rather than end to end.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `plans/spikes/009-reaching-the-user-findings.md` exists
- [ ] `grep -c '^## ' plans/spikes/009-reaching-the-user-findings.md` returns at least 6
- [ ] `## Which edges` contains a table with at least four edges and a yes/no for each
- [ ] `## Not notifying twice` names a condition and cites the `file:line` proving the daemon can observe it
- [ ] `## The four shapes weighed` names all four, including "nothing", and answers the remote-box question for each
- [ ] `## Recommended shape` states a default, and that default is justified
- [ ] If shape 2 is recommended, the doc states whether the URL is a secret and whether it belongs in `config.local.json` rather than `config.json`
- [ ] `## Open questions for the operator` is present and non-empty
- [ ] `grep -rn 'osascript\|notify-send\|afplay' crates/nebula-daemon/src --include='*.rs'` returns nothing — the daemon was not touched
- [ ] `git diff --stat -- crates Cargo.toml Cargo.lock` produces no output
- [ ] `git worktree list` shows exactly one worktree
- [ ] `git status --short` lists only the findings doc and `plans/README.md`
- [ ] `plans/README.md` status row for 009 updated

## STOP conditions

Stop and report back (do not improvise) if:

- `cargo test --workspace` fails on anything other than the two known `e2e_tui`
  failures at `565080d` (`nebula_open_from_inside_a_session_raises_the_file_tabs`
  and `tui_drag_past_the_pane_top_autoscrolls_and_copies_the_run`, both timing
  out on `text "^q: sessions"`, fixed by plan 001). Note those and carry on;
  anything else is a STOP.
- You are about to send a request to any non-loopback address. Stop: this spike
  has no mandate to contact anything, and a test POST to a real webhook is
  outward-facing.
- You cannot establish whether the daemon can tell a TUI is subscribed.
  Sub-question 2 depends on it; report what you found and which shapes survive
  without the answer.
- The recommendation is turning into "move `notify_desktop` into the daemon" and
  you have not answered what signal tells the daemon it is on a desktop. That is
  the whole difficulty; a recommendation without it is not usable.
- A prototype run was made without `NEBULA_RUNTIME_DIR` and `NEBULA_DATA_DIR`
  set. Stop immediately and report it — a daemon that notifies on its own,
  pointed at the operator's real data, may have acted on their live sessions.
- `crates/nebula-daemon/src/worktree_hooks.rs:1-22` or
  `crates/nebula-tui/src/event_loop.rs:739-762` no longer match the excerpts
  above.

## Maintenance notes

- WORKTREE HOOKS are the precedent to lean on hardest. They are the daemon
  already running a user's program on an event, with the git-config-not-checkout
  trust decision argued in prose, its own process group, a timeout that kills
  the whole group, and output to temp files rather than pipes
  (`worktree_hooks.rs:1-22`, `:126-160`). A notify hook that copies that shape
  inherits four decisions and a test suite pattern. Any proposal that *doesn't*
  reuse it should say why.
- The three gates at `event_loop.rs:754-762` are the accumulated policy, not
  incidental detail: configurable-and-can-be-off, only-when-unfocused, and
  never-over-ssh. A daemon-side path has to answer all three with less
  information than the client has. If it cannot, that is an argument for shape 1
  or shape 4 rather than something to paper over.
- `config.json` travels. `nebula config export`, `nebula ssh` and `nebula tunnel`
  all carry it between machines (`ARCHITECTURE.md:71`), base64'd into a remote
  script's positional parameter. Anything secret-shaped put in it travels too, and
  argv on the far side is readable by other users on many systems. That is why a
  webhook URL may belong in `config.local.json`, which the export excludes.
- Default off for anything that leaves the machine. The user asked for a
  multiplexer, not a telemetry client, and `README.md` is emphatic that this is
  one person's tool.
- A reviewer of the eventual build work should check two things hardest: that a
  notification failure cannot affect a session (compare
  `registry.rs:2474-2476`), and that a session name reaching a shell, a URL or a
  notifier is escaped — `alerts.rs:117-132` `applescript_str` exists because that
  was a real hazard.
