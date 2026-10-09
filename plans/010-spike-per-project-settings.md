# Plan 010 (SPIKE): Decide which global settings want to be per-project

> **Executor instructions**: This is a **design spike, not a build task**. Its
> deliverable is a written findings document, not merged product code. You may
> prototype to answer a question, but every prototype is throwaway and nothing
> under `crates/` is committed. Follow the steps in order, and if anything in
> "STOP conditions" occurs, stop and report rather than improvising. When done,
> update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 565080d..HEAD -- crates/nebula-tui/src/config.rs crates/nebula-daemon/src/config.rs crates/nebula-core/src/project_file.rs crates/nebula-daemon/src/registry.rs`
> If any of those changed, compare the "Current state" excerpts below against
> the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P3
- **Effort**: M (investigation + write-up; each setting it specifies is S)
- **Risk**: LOW (no product code ships from this plan)
- **Depends on**: none
- **Category**: direction
- **Planned at**: commit `565080d`, 2026-10-06
- **Deliverable**: `plans/spikes/010-per-project-settings-findings.md`

## The question this spike answers

A project's settings are two shell lines. Everything that shapes *how agents
run* in that project is one global value shared by every repository you own.

The sharpest case is `worktree_base_branch`. It is a single string, read once
for every new WORKTREE in every project. A user with repos on `main`, `master`
and `develop` has a setting that is wrong for two of the three — and nothing
remembers a per-project answer, because `nebula worktree --base` only overrides
one invocation.

The spike's job is to decide **which settings are per-project**, and then
**where a per-project value lives** — because there are two candidate homes with
very different properties:

- **`config.json`'s `projects` map**, keyed by repo path. Machine-local, already
  exists, already has exactly this shape for two settings.
- **`.nebula.json`, committed in the checkout.** Travels with the repository —
  and runs straight into the reason that file is safe at all.

Note a tension in stated intent up front, because it bounds the answer.
`README.md:1-4` says nebula *"is built for one person's workflow — mine"*. So
the team-sharing argument for `.nebula.json` is weak here; the single-user,
many-repos argument for `config.json` is not. The spike should weigh them
separately rather than treating "per-project" and "committable" as one idea.

## Current state

### A project's settings are exactly two fields

`crates/nebula-tui/src/config.rs:1354-1379` — the whole per-project surface:

```rust
pub struct ProjectSettings {
    /// The RUN COMMAND a menu's **Run** starts in this project's
    /// worktrees, typed on the Project tab. Empty — the default, shown as `.nebula.json` — is
    /// the checkout's PROJECT FILE `run`, where the command lived before
    /// the row existed; set, it wins over the file. The DAEMON reads it
    /// (`nebula-daemon/src/config.rs`); the TUI only edits it. Left out
    /// of the file while empty, so an entry written before the row reads
    /// the same after a save.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub run_command: String,
    /// The OPEN COMMAND `Shift+Enter` / `Shift+O` fires on this project's
    /// worktrees, typed on the Project tab — `open http://localhost:3000`,
    /// say. Empty (shown as `.nebula.json`) is the checkout's PROJECT FILE
    /// `open`; set, it wins over the file. The TUI both edits and runs it
    /// (`event_loop::open_worktree`): what it opens belongs on the machine
    /// the user sits at, never the DAEMON's. Left out of the file while
    /// empty, like `run_command`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub open_command: String,
    /// Keys in the entry this build doesn't know — a newer nebula's, most
    /// likely, or the retired `hide_root_worktree` an older one wrote —
    /// carried through a save untouched, as the file's top-level keys
    /// are. An entry holding one is never dropped as "all default".
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
}
```

Three conventions are already settled here and any new field inherits them:
**empty means fall through** to `.nebula.json`; `skip_serializing_if` keeps an
untouched entry byte-identical across a save; and the `other` flatten map carries
unknown keys so a newer nebula's settings survive an older one's write.

`.nebula.json` holds the same two.
`crates/nebula-core/src/project_file.rs:37-43`:

```rust
pub struct ProjectFile {
    #[serde(default)]
    pub run: Option<String>,
    #[serde(default)]
    pub open: Option<String>,
```

So the two homes are at parity today. The asymmetry is not between them — it is
between both of them and the global config.

### Everything else is one value for every repo

`crates/nebula-daemon/src/config.rs:14-57` — the daemon's view of `config.json`:

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

Plus, on the client side, `agent_presets.json` as a separate global file
(`crates/nebula-tui/src/agent_presets.rs`, described at `ARCHITECTURE.md:73`),
and the whole `SettingKind` list at `crates/nebula-tui/src/config.rs:385-418`,
of which only two are per-project — `config.rs:459-461`:

```rust
    pub fn is_project(self) -> bool {
        matches!(self, SettingKind::RunCommand | SettingKind::OpenCommand)
    }
```

`SettingKind::WorktreeBaseBranch` (`config.rs:387`) sits in that same enum as a
**global** row.

### The base-branch case, end to end

`crates/nebula-daemon/src/registry.rs:850-866` — the only read:

```rust
        // A base the caller named (`nebula worktree --base`) is resolved
        // against the fetched origin — `main` means `origin/main`, never
        // this checkout's local branch; every other new WORKTREE — `n` in
        // the WORKTREES PANEL, a bare `nebula worktree`, the QUICK PROMPT's
        // auto-created one — starts at the `worktree_base_branch` SETTING
        // when one is set (`master`, resolved the same way), else at the
        // fetched `origin/HEAD`; never at this checkout's HEAD.
        let path = match base {
            Some(base) => git::add_worktree_off_ref(&project.repo_path, branch, base).await?,
            None => match crate::config::Config::load().worktree_base_branch() {
                Some(configured) => {
                    git::add_worktree_off_configured(&project.repo_path, branch, configured).await?
                }
                None => git::add_worktree_off_default(&project.repo_path, branch).await?,
            },
        };
```

Note that `project.repo_path` is already in hand at the call site — so a
per-project lookup needs no new plumbing to reach it.

The global accessor, `crates/nebula-daemon/src/config.rs:114-118`:

```rust
    pub fn worktree_base_branch(&self) -> Option<&str> {
        let name = self.worktree_base_branch.trim();
        let name = name.strip_prefix("origin/").unwrap_or(name).trim();
        (!name.is_empty()).then_some(name)
    }
```

And immediately below it, the **per-project accessor that already exists** —
`crates/nebula-daemon/src/config.rs:120-129`. This is the exact shape a
per-project base branch would take:

```rust
    /// The RUN COMMAND set for the project checked out at `repo_path` in
    /// Settings → Project, trimmed; None when the project has no entry or
    /// the row is empty, which means the checkout's `.nebula.json` decides.
    pub fn run_command(&self, repo_path: &Path) -> Option<&str> {
        self.projects
            .get(repo_path)
            .map(|p| p.run_command.trim())
            .filter(|c| !c.is_empty())
    }
```

The daemon's own per-project struct, `crates/nebula-daemon/src/config.rs:58-69`:

```rust
/// One project's entry under `projects` — the rows of the TUI's Project
/// tab, of which the daemon reads one. The rest (`open_command`) are the
/// TUI's and pass through unread.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    /// The RUN COMMAND a menu's **Run** starts in this project's
    /// worktrees, typed into Settings → Project. Empty means the checkout's `.nebula.json`
    /// `run`, as before the row existed.
    pub run_command: String,
}
```

That doc records a deliberate split: the daemon deserializes only the fields it
acts on, and the client's fields pass through unread. Any new field has to be
placed on the right side of that line.

### The Project tab already exists, with its own comment about scope

`crates/nebula-tui/src/config.rs:717-737`:

```rust
    // Settings that belong to one project rather than to nebula. The tab
    // edits the selected project's entry in `projects` and names that
    // project on its first line, so a row here never reads as a switch
    // for every project at once.
    SettingsTab {
        title: "Project",
        body: TabBody::Project(&[
            SettingSpec {
                kind: SettingKind::RunCommand,
                label: "Run command",
                hint: "Shell line a menu's Run starts in this project's worktrees (empty = its .nebula.json \"run\")",
                group: "",
            },
            SettingSpec {
                kind: SettingKind::OpenCommand,
                label: "Open command",
                hint: "Shell line ⇧Enter / ⇧O runs to open a worktree of this project, e.g. open http://localhost:3000 (empty = its .nebula.json \"open\")",
                group: "",
            },
        ]),
    },
```

So the UI mechanism is complete: a tab, a `TabBody::Project` body, rows that
`is_project()` identifies, rendering that branches on it (`ui.rs:1265`) and
editing that branches on it (`event_loop.rs:6523`). Adding a per-project row is
filling in an existing slot, not building one.

One convention to respect — `crates/nebula-tui/src/config.rs:463-468`:

```rust
    /// The day the row first shipped, `(year, month, day)`: the date of
    /// the first release tag whose settings overlay lists it, or the day
    /// it was written for a row no release carries yet. The overlay marks
    /// a row `(new)` for [`NEW_SETTING_DAYS`] from here. The match is
    /// exhaustive on purpose — a new row can't compile without its date.
    pub fn added_on(self) -> (i32, u32, u32) {
```

A new `SettingKind` variant will not compile without a date here. Worth knowing
before anyone estimates "one field".

### Why `.nebula.json` is the harder home

`ARCHITECTURE.md:81`, the last sentence of the Run path paragraph:

> `Shift+Enter` (or `Shift+O` / `Alt+Enter`) is TUI-only: it reads the project's
> `open_command` out of the same `projects` entry, else the same file's `open`,
> and runs it once through `$SHELL -c` in the checkout, detached, output
> discarded. **Nothing in the file runs without a keypress, which is what lets
> it live in the repository when WORKTREE HOOKS cannot.**

And the hooks that cannot —
`crates/nebula-daemon/src/worktree_hooks.rs:11-14`:

```rust
//! Never a file inside the checkout — a committed hook would run whatever
//! a clone brought with it, which is why git itself refuses hooks from the
//! working tree. Git resolves the key the usual way, so a `--global` value
//! serves every project and a repo's own `.git/config` overrides it.
```

So the repository has a worked-out rule: a committed file may hold something the
user explicitly triggers, and may not hold something nebula acts on by itself. A
committed *default harness* would be a repository telling your machine which
binary to launch — on the wrong side of that line. A committed *base branch* is
inert data that changes which ref a `git worktree add` starts from, which is
arguably on the right side. Deciding that, per setting, is the substance of the
second half of this spike.

Note there is also a third home already in use for per-repository configuration:
**git config**, which is how WORKTREE HOOKS are set
(`git config nebula.worktreeCreateHook …`). It is per-repository, not committed,
and not carried by `nebula config export`. Weigh it as a candidate.

### Repo conventions the findings doc must respect

- **Comments explain why, at length, and name the failure that motivated them.**
  `config.rs:1354-1372` and `worktree_hooks.rs:11-14` are the register. Any field
  the findings doc proposes should be documented to that standard so a build plan
  can lift the prose.
- **The domain vocabulary is capitalised in prose**: PROJECT, WORKTREE, SESSION,
  RUN COMMAND, OPEN COMMAND, PROJECT FILE, WORKTREE HOOKS, SETTING, AGENT PRESET,
  ROOT WORKTREE. `ARCHITECTURE.md` is the glossary.
- **Empty means fall through.** Every per-project row so far is "empty = the
  layer below decides", shown in the overlay as `.nebula.json`
  (`PROJECT_FILE_CHOICE`, `config.rs:1387`). A new row should keep that, so
  nobody has to set a value they do not care about.
- **Settings are forward-compatible by construction.** `ProjectSettings::other`
  (`config.rs:1377-1378`) and the top-level equivalent carry keys this build
  does not know, and `ARCHITECTURE.md:71` explains the whole scheme: a merged
  object that will not deserialize is read key by key, so *"a value one release
  can't read costs only that key"*. Any proposal must not break that.
- **`config.json` travels; `config.local.json` does not.** `nebula config export`,
  `nebula ssh` and `nebula tunnel` carry `config.json` and presets between
  machines (`ARCHITECTURE.md:71`). A per-project setting keyed by **absolute repo
  path** travels to a machine where that path means nothing — or worse, means
  something else. That is a real consequence to address.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| The per-project surface | `grep -n 'struct ProjectSettings' -A 30 crates/nebula-tui/src/config.rs` | the two fields plus `other` |
| The global surface | `grep -n 'pub struct Config' -A 50 crates/nebula-daemon/src/config.rs` | the seven fields |
| Which rows are per-project | `grep -n 'fn is_project' -A 4 crates/nebula-tui/src/config.rs` | `RunCommand \| OpenCommand` |
| The base-branch read | `grep -n 'worktree_base_branch()' crates/nebula-daemon/src/registry.rs` | `:859` |
| Your own repos' default branches | `git -C <repo> symbolic-ref --short refs/remotes/origin/HEAD` | e.g. `origin/main` |
| Typecheck | `cargo check --workspace --all-targets` | exit 0 |
| Tests | `cargo test --workspace` | exit 0 (see STOP conditions) |
| Throwaway worktree | `git worktree add /tmp/nebula-spike-010 HEAD` | a detached checkout |
| Remove it | `git worktree remove --force /tmp/nebula-spike-010` | gone |
| Isolated run | `NEBULA_RUNTIME_DIR=/tmp/nb-010-rt NEBULA_DATA_DIR=/tmp/nb-010-data cargo run` | never touches real sessions |

**The isolation variables are mandatory for any run.** `NEBULA_RUNTIME_DIR` and
`NEBULA_DATA_DIR` (`crates/nebula-core/src/env.rs:16-18`) keep a prototype off
the operator's live daemon, real sessions and real `config.json` — and this spike
is about settings, so an unisolated run could **write the operator's actual
configuration file**. `Makefile:52-53` does the same for `make dev`.

Do not read or quote the operator's real `config.json` into the findings doc. If
you need to know how many of their repos differ in default branch, count them and
report the count, not the contents.

## Scope

**In scope** — you may create or modify only:
- `plans/spikes/010-per-project-settings-findings.md` (create; create
  `plans/spikes/` if absent)
- `plans/README.md` — your status row only

**Out of scope** (do NOT commit changes to any of these):
- Everything under `crates/`. In particular, do not add a `SettingKind` variant
  or a `ProjectSettings` field, however small — which settings are per-project is
  the question, not the task.
- `crates/nebula-core/src/project_file.rs`. Extending `.nebula.json` is the half
  of this spike with a trust argument attached; it must not be done before that
  argument is made.
- The operator's real `config.json` or `config.local.json`, at any path. Read-only
  at most, and never quoted into the findings doc.
- `ARCHITECTURE.md` and `docs/configuration.md`. Documenting a new setting is part
  of the build work, not the spike.
- `crates/nebula-core/src/protocol.rs` — `PROTOCOL_VERSION` (`protocol.rs:9`) is
  strict-equality gated (`crates/nebula-daemon/src/server.rs:67`). Settings do not
  cross the socket today; if a proposal would make them, specify it rather than
  doing it.
- Anything to do with plans 001–009.

## Steps

### Step 1: Read the three layers

Read, in this order:

1. `ARCHITECTURE.md`, the "Settings path" paragraph (`:71`) and the "Run path"
   paragraph (`:81`). The second contains the trust rule this spike turns on.
2. `crates/nebula-tui/src/config.rs:1340-1420` — `ProjectSettings` and its
   `value_label`.
3. `crates/nebula-daemon/src/config.rs` in full. It is ~400 lines including
   tests, and it is the whole daemon-side settings story including the
   per-project accessor pattern.
4. `crates/nebula-core/src/project_file.rs` in full — `.nebula.json`, its
   lookup order, and its error handling.
5. `crates/nebula-daemon/src/worktree_hooks.rs:1-22` — the other per-repository
   configuration mechanism, and the trust argument.
6. `docs/configuration.md` — the user-facing description of all of this, which is
   what a new setting would have to fit into.

**Verify**: a findings section `## The three layers` naming each configuration
home (global `config.json`, `config.json`'s `projects` map, `.nebula.json`, git
config), what lives in each today, whether it travels with
`nebula config export` / `nebula ssh`, and whether nebula acts on it without a
keypress.

### Step 2: Triage every global setting

Go through every field of the daemon's `Config` (`nebula-daemon/src/config.rs:14-57`)
**and** every variant of `SettingKind` (`nebula-tui/src/config.rs:385-418`), and
put each into one of three buckets with a one-line reason:

- **Clearly per-project** — the value is a property of the repository.
- **Clearly global** — the value is a property of the machine or the user.
- **Arguable** — say what the argument is on each side.

Candidates worth particular thought: `worktree_base_branch` (clearly
per-project); `prewarm_agents` / `prewarm_sessions` (machine resources, so
probably global); a default harness or AGENT PRESET per project (arguable — the
preference may be about the repo's language, or about the user's mood);
`session_idle_timeout` (arguable); `custom_harnesses` / `harnesses` (a registry,
so global).

Be ruthless. A settings surface that doubles in size is a worse product than one
that is wrong for two repos out of three, and the recommendation should probably
name **one** setting to move, not six.

**Verify**: a findings section `## Triage` with a three-column table covering
every `Config` field and every `SettingKind` variant, no omissions.

### Step 3: Measure whether the base-branch problem is real

Do not take the premise on trust. For each repository registered as a project on
this machine, find its default branch:

```sh
git -C <repo> symbolic-ref --short refs/remotes/origin/HEAD 2>/dev/null
```

Report the **distribution** — how many repos on `main`, how many on `master`, how
many on something else — not the paths or names. If every repo is on `main`, the
problem is theoretical and the recommendation should say so; a setting added for
a problem nobody has is a cost with no payer.

**Verify**: a findings section `## Is the base-branch problem real?` with the
count per default branch and an explicit verdict on whether this justifies the
change on this machine.

### Step 4: Specify the per-project home for the chosen setting(s)

For each setting the triage moved, specify:

- The field on `ProjectSettings` (`nebula-tui/src/config.rs:1354`) with its
  `skip_serializing_if`, and the matching field on the daemon's `ProjectConfig`
  (`nebula-daemon/src/config.rs:64`) **if** the daemon is the one that acts on
  it. Note the split documented at `nebula-daemon/src/config.rs:58-61`: the
  daemon deserializes only what it reads.
- The accessor, modelled exactly on `Config::run_command(&self, repo_path)`
  (`nebula-daemon/src/config.rs:123-129`), including the `trim()` and the
  "empty means fall through" filter.
- The fallback chain, written out in order. For base branch that is: an explicit
  `--base`, then the project's value, then the global value, then
  `origin/HEAD` — and say whether the global layer survives at all or is
  replaced.
- The `SettingSpec` row for the Project tab (`nebula-tui/src/config.rs:721-737`),
  with its `label` and `hint` written in the house voice. Look at the two
  existing hints: they name the gesture, the effect, and what empty means.
- The `SettingKind` variant and its `added_on()` date
  (`nebula-tui/src/config.rs:463-468`), which will not compile without one.
- Migration: what happens to a user who has a global value set today. Does it
  keep working? The answer must be yes — note `ARCHITECTURE.md:71` on reading
  settings key by key so an unreadable value costs only that key.

**Verify**: a findings section `## Specification` covering all seven points above
for each setting being moved.

### Step 5: Settle whether `.nebula.json` may carry any of it

This is the half with the argument attached. Answer in writing:

- Restate the rule from `ARCHITECTURE.md:81` and
  `worktree_hooks.rs:11-14`: a committed file may hold something the user
  explicitly triggers; it may not hold something nebula acts on by itself.
- For each setting the triage moved, say which side of that line it falls on. A
  base branch is inert data consumed by a `git worktree add`; a default harness
  is a repository naming a binary to launch. Those are different.
- If any setting may be committed, specify the `.nebula.json` field and the
  precedence against the `projects` entry — the existing two settings have the
  `projects` entry **win** over the file (`config.rs:1356-1361`), and a new one
  should match.
- Address the stated-intent tension explicitly: `README.md:1-4` says this is one
  person's tool, which weakens the "so a team can share it" argument. Say whether
  the committable half is worth building at all, or is a follow-up for a future
  where that changes.

**Verify**: a findings section `## Should .nebula.json carry it?` with the rule
restated, a per-setting verdict, and an explicit position on the single-user
tension.

### Step 6: Address the path-keyed-config-travels problem

`config.json`'s `projects` map is keyed by **absolute repo path**
(`BTreeMap<PathBuf, ProjectConfig>`), and `config.json` is what
`nebula config export`, `nebula ssh` and `nebula tunnel` carry to other machines
(`ARCHITECTURE.md:71`). So a per-project entry arrives on a machine where the
path may not exist, or may be a different repository.

Decide and record: is that already a problem for `run_command` and
`open_command` today? Does adding a third per-project setting make it worse or
just more visible? Should per-project entries be excluded from the exported
bundle, the way `ssh_hosts.json` already is (`ARCHITECTURE.md:71` — the bundle
carries *"config and presets, not hosts"*)?

**Verify**: a findings section `## Path-keyed settings and the bundle` stating
whether this is pre-existing, whether it worsens, and a recommendation.

### Step 7: Prototype only if a question is still open

Skip entirely if steps 2–6 are answered from reading. Prototype only to settle
something reading could not — most likely how the Project tab renders and saves a
third row.

If you do, use the throwaway worktree and **always** the isolation variables.
This spike is about settings, so an unisolated run can write the operator's real
`config.json`:

```sh
git worktree add /tmp/nebula-spike-010 HEAD
cd /tmp/nebula-spike-010
NEBULA_RUNTIME_DIR=/tmp/nb-010-rt NEBULA_DATA_DIR=/tmp/nb-010-data cargo run
```

Clean up:

```sh
git worktree remove --force /tmp/nebula-spike-010
rm -rf /tmp/nb-010-rt /tmp/nb-010-data
```

**Verify**: `git worktree list` shows one entry; `git status --short` in the main
checkout lists nothing under `crates/`; the operator's real `config.json` has an
unchanged modification time.

### Step 8: Write the findings doc

Assemble `plans/spikes/010-per-project-settings-findings.md`:

```markdown
# Spike 010 findings: which global settings want to be per-project

- **Spiked at**: commit <short SHA>, <YYYY-MM-DD>
- **Question**: <one sentence>
- **Recommendation**: <one paragraph: which setting(s) move, to which home, and what stays global>

## The three layers
## Triage
## Is the base-branch problem real?
## Specification
## Should .nebula.json carry it?
## Path-keyed settings and the bundle
## Open questions for the operator
## What I did not investigate
```

`## Open questions for the operator` is required and must not be empty. Anything
in the "arguable" bucket that you could not decide from evidence belongs there —
a default harness per project is a taste call, not a technical one.

**Verify**: every section present and non-empty; the Recommendation names the
setting(s) to move, the home, and what stays global.

### Step 9: Final check

**Verify**:
- `git status --short` lists only
  `plans/spikes/010-per-project-settings-findings.md` and `plans/README.md`.
- `git worktree list` shows one entry.
- `git diff --stat -- crates Cargo.toml Cargo.lock` produces no output.
- No part of the operator's real `config.json` is quoted in the findings doc.

## Test plan

A spike ships no product code, so there are no new tests. Verification is the
findings doc's completeness, per the done criteria.

For whoever writes the build plan, record these in the findings doc's own
maintenance notes:

- `crates/nebula-daemon/src/config.rs`'s inline tests are the pattern for a
  per-project accessor: `run_command_is_read_per_project_and_blank_means_the_file`
  (`:257`) builds a `config.json` literal with two project entries and asserts
  the lookup and the blank-means-fallback behaviour. A new accessor wants exactly
  that test.
- `crates/nebula-tui/src/config.rs`'s tests already assert the `is_project()`
  membership (`:4068`, `:4153`) and that the overlay's specs agree with it
  (`:4226`). **Those tests will need updating** when the membership changes —
  flag it, because it is the one place a build plan will otherwise be surprised.
- Round-tripping matters more than it looks. `ProjectSettings` uses
  `skip_serializing_if` so an untouched entry saves byte-identically; there should
  be a test that an entry written by a build *without* the new field survives a
  save by a build *with* it, and vice versa. `ARCHITECTURE.md:71` is the contract
  that demands it.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `plans/spikes/010-per-project-settings-findings.md` exists
- [ ] `grep -c '^## ' plans/spikes/010-per-project-settings-findings.md` returns at least 7
- [ ] `## Triage` contains a row for every field of the daemon's `Config` and every `SettingKind` variant — cross-check the counts against `grep -c 'pub [a-z_]*:' ` on the struct and the variant list
- [ ] `## Is the base-branch problem real?` reports a count per default branch and an explicit verdict
- [ ] `## Specification` covers all seven points from step 4 for each setting being moved
- [ ] `## Should .nebula.json carry it?` restates the keypress rule and gives a per-setting verdict
- [ ] `## Path-keyed settings and the bundle` reaches a recommendation
- [ ] `## Open questions for the operator` is present and non-empty
- [ ] `grep -c 'is_project' crates/nebula-tui/src/config.rs` is unchanged from its value at `565080d` — no membership was edited
- [ ] No absolute path from the operator's real `config.json` appears in the findings doc
- [ ] `git diff --stat -- crates Cargo.toml Cargo.lock` produces no output
- [ ] `git worktree list` shows exactly one worktree
- [ ] `git status --short` lists only the findings doc and `plans/README.md`
- [ ] `plans/README.md` status row for 010 updated

## STOP conditions

Stop and report back (do not improvise) if:

- `cargo test --workspace` fails on anything other than the two known `e2e_tui`
  failures at `565080d` (`nebula_open_from_inside_a_session_raises_the_file_tabs`
  and `tui_drag_past_the_pane_top_autoscrolls_and_copies_the_run`, both timing
  out on `text "^q: sessions"`, fixed by plan 001). Note those and carry on;
  anything else is a STOP.
- Step 3 finds every registered repo on the same default branch. The premise of
  the headline case is then theoretical on this machine. Report it — the answer
  may legitimately become "not worth doing", and that is a useful finding, not a
  failed spike.
- The triage moves more than two settings. Report the list and stop before
  specifying them: a settings surface that grows by six rows is a product
  decision the operator should make, not a consequence of a spike.
- A prototype run was made without `NEBULA_RUNTIME_DIR` and `NEBULA_DATA_DIR`
  set. Stop immediately and report it, and check whether the operator's real
  `config.json` was modified — this spike's blast radius is their settings file.
- `crates/nebula-tui/src/config.rs:1354-1379` or
  `crates/nebula-daemon/src/config.rs:14-69` no longer match the excerpts above.

## Maintenance notes

- The cheapest honest outcome is **one** setting moved: `worktree_base_branch`
  into the `projects` entry, with the global value kept as the fallback. The
  mechanism is complete already — a Project tab, a `TabBody::Project`,
  `is_project()`, a per-project accessor pattern, and `project.repo_path` already
  in hand at the one call site. Everything else in the triage should have to
  argue for itself.
- A new `SettingKind` variant will not compile without an `added_on()` date
  (`crates/nebula-tui/src/config.rs:463-468`), and the overlay marks the row
  `(new)` for a window afterwards. Small, but it is a step a build plan must not
  miss.
- Keep "per-project" and "committable" apart to the end. They are two questions
  with two different answers and two different risk profiles: the first is a
  `config.json` shape change, the second runs into why `.nebula.json` is safe at
  all. Conflating them is how a reasonable setting ends up in a file a clone
  brings with it.
- The path-keyed-bundle problem (step 6) predates this spike and may deserve a
  finding of its own regardless of the outcome here. A `projects` map keyed by
  absolute path, carried to another machine by `nebula ssh`, is either harmless
  or quietly wrong depending on how the far side resolves it — and nobody has
  written down which.
- A reviewer of the eventual build work should check the fallback chain hardest.
  Four layers (`--base`, project, global, `origin/HEAD`) is one more than the
  existing settings have, and the order has to be the same in the daemon, in the
  overlay's value label, and in `docs/configuration.md`.
