# Plan 008 (SPIKE): Decide how a session's branch becomes a pull request

> **Executor instructions**: This is a **design spike, not a build task**. Its
> deliverable is a written findings document, not merged product code. You may
> prototype to answer a question, but every prototype is throwaway and nothing
> under `crates/` is committed. Follow the steps in order, and if anything in
> "STOP conditions" occurs, stop and report rather than improvising. When done,
> update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 565080d..HEAD -- crates/nebula-tui/src/pull_request.rs crates/nebula-tui/src/pr_modal.rs crates/nebula-tui/src/app.rs crates/nebula-core/src/protocol.rs`
> If any of those changed, compare the "Current state" excerpts below against
> the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P3
- **Effort**: M (investigation + write-up; implementation separate)
- **Risk**: LOW (no product code ships from this plan)
- **Depends on**: none
- **Category**: direction
- **Planned at**: commit `565080d`, 2026-10-06
- **Deliverable**: `plans/spikes/008-create-a-pull-request-findings.md`

## The question this spike answers

nebula reads GitHub deeply and cannot create anything. Nine `gh` verbs across
the TUI — pull request list, diff, comment; issue list, view, comment, edit;
two `gh api` calls — and **no `create`, anywhere**.

The gap is visible in the maintainer's own workflow. `.claude/skills/pr-description/SKILL.md`
exists to write a pull request description from one of ten templates and then
run `gh pr create --body-file`, and `.claude/skills/release/SKILL.md` drives
`git tag` and `gh release edit`. So the loop today is: work in nebula → leave
nebula → run a skill. Meanwhile the PR modal already turns a pull request *into*
a session; the reverse direction is missing.

The spike's job is to decide **which shape of "create a pull request" belongs in
nebula, if any** — and the honest answer may be the smallest one. Four candidate
shapes, to be weighed rather than assumed:

1. **Hand it to the agent.** A verb that tells the session in that worktree to
   open the pull request itself — it has the diff, the context, and (if the user
   has one) a skill that does this well. nebula contributes a keystroke.
2. **Push and open the web form.** Push the branch, then
   `gh pr create --web`, letting GitHub's own form take it from there. No body
   composition in nebula at all.
3. **A compose form in the TUI.** Title and body typed into a modal, posted with
   `gh pr create --title … --body-file -`.
4. **Nothing.** Record why, so it is not re-opened.

Two questions have to be answered before any of those can be chosen, and they
are the real work of this spike:

- **Does nebula know whether the branch is pushed?** It does not, today. That is
  a prerequisite for shapes 2 and 3 and a cost nobody has priced.
- **Who writes the body?** This is the crux. A description worth posting is what
  `pr-description` spends ten templates on. nebula composing one itself risks
  shipping worse descriptions than the agent already sitting in that pane.

## Current state

### The nine `gh` verbs nebula already runs

All client-side, in `nebula-tui`:

| Verb | Site |
|---|---|
| `gh pr list` (via the list query) | `crates/nebula-tui/src/pull_request.rs:345`, `:813` |
| `gh pr diff <n>` | `crates/nebula-tui/src/pull_request.rs:893` |
| `gh pr comment <n> --body-file -` | `crates/nebula-tui/src/pull_request.rs:924` |
| `gh api user --jq .login` | `crates/nebula-tui/src/pull_request.rs:373` |
| `gh api graphql` (check verdicts) | `crates/nebula-tui/src/pull_request.rs:640-641` |
| `gh issue list` | `crates/nebula-tui/src/issues.rs:451` |
| `gh issue view <n>` | `crates/nebula-tui/src/issues.rs` |
| `gh issue comment <n> --body-file -` | `crates/nebula-tui/src/issues.rs:500` |
| `gh issue edit <n> … --body-file -` | `crates/nebula-tui/src/issues.rs:550` |

Confirm the absence yourself: `grep -rn '"create"' crates --include='*.rs'`
returns only `crates/nebula-tui/src/syntax.rs`, where `"create"` is a SQL
keyword in the highlighter's word list.

### The exact shape a `create` would take

`crates/nebula-tui/src/pull_request.rs:905-950` is the existing write path, and
it is the template to follow. Read its doc comment especially — the reasoning
about *why* the body crosses on stdin, and *why* the error distinguishes "could
not run" from "ran and refused", is the standard this repo holds:

```rust
/// could not be run, or it ran and stalled past [`COMMENT_TIMEOUT`].
pub const GH_NOT_RUN: &str = "gh could not be run";
pub const GH_TIMED_OUT: &str = "gh timed out";

/// Post `body` as an issue comment on pull request `number`, from a
/// checkout of its repo, as whoever `gh` is logged in as. `Ok` carries the
/// URL `gh` prints for the new comment; `Err` carries the reason it did
/// not post — `gh`'s own first line when it ran and refused, or
/// [`GH_NOT_RUN`] / [`GH_TIMED_OUT`] when it never answered — because "not
/// logged in" and "no network" are different things to tell the person
/// still holding the text.
///
/// The body crosses on stdin (`--body-file -`) rather than in argv: a
/// comment is markdown written by a person and may be long, start with a
/// dash, or hold anything else an argument parser would misread.
pub async fn comment(dir: &Path, number: u64, body: &str) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    let number = number.to_string();
    let mut cmd = tokio::process::Command::new("gh");
    cmd.args(["pr", "comment", &number, "--body-file", "-"])
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let run = async {
        let mut child = cmd.spawn().ok()?;
        let mut stdin = child.stdin.take()?;
        // A `gh` that exits before reading the pipe (bad auth, a usage
        // error) closes its end and the write fails; its stderr says why,
        // and `wait_with_output` is what reads that.
        let _ = stdin.write_all(body.as_bytes()).await;
        drop(stdin);
        child.wait_with_output().await.ok()
    };
    let out = match tokio::time::timeout(COMMENT_TIMEOUT, run).await {
        Ok(Some(out)) => out,
        Ok(None) => return Err(GH_NOT_RUN.into()),
        Err(_) => return Err(GH_TIMED_OUT.into()),
    };
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(comment_error(&String::from_utf8_lossy(&out.stderr)))
    }
}
```

The read path has its own helper, `crates/nebula-tui/src/pull_request.rs:175-210`:

```rust
/// Run `gh` with `args` (in `dir` when given) under `timeout`, yielding
/// stdout on success. Every failure — no `gh`, bad exit, timeout — is
/// `None`, since each is an ordinary "couldn't ask" to every caller.
pub(crate) async fn gh(
    dir: Option<&Path>,
    args: &[&str],
    timeout: std::time::Duration,
) -> Option<String> {
    run_gh(dir, args, timeout).await.ok()
}
```

Timeouts in use: `TIMEOUT` = 20s (`pull_request.rs:29`), `DIFF_TIMEOUT` = 45s
(`:735`), and a `COMMENT_TIMEOUT` for the write path.

### nebula does not know whether the branch is pushed

This is the finding that prices shapes 2 and 3. Search for upstream tracking:

```sh
grep -rn 'upstream\|@{u}\|rev-list --count\|--ahead-behind' crates --include='*.rs' | grep -v '/tests/'
```

At `565080d` this returns nothing relevant — the hits are all the English word
"ahead" in comments. What nebula *does* know per worktree:

- The branch name, from `git worktree list --porcelain`
  (`crates/nebula-daemon/src/git.rs:68`).
- Changed-file counts and line deltas, from `git status --porcelain=v1 -z -uall`
  (`crates/nebula-tui/src/git_diff.rs:898`).
- Whether a pull request exists on that branch, from a client-side `gh pr view`
  on the git-poll tick, cached per worktree (`ARCHITECTURE.md:32`).

So "has this branch been pushed, and is it ahead of its remote" is a new
measurement. `.claude/skills/pr-description/SKILL.md:178` records why it matters:
*"`gh pr create` needs the branch pushed and an account with write access."*

Note also the constraint the maintainer wrote down in
`.claude/skills/pr-reviewer/SKILL.md:21`: *"The SHARED CHECKOUT belongs to
several sessions at once, so a `gh pr checkout`, a merge…"* — anything this
spike proposes that mutates a checkout has to reckon with the fact that several
sessions may be living in it.

### Where a verb would hang

Menu actions are an enum in `crates/nebula-tui/src/app.rs:252-355`. The
pull-request neighbours already there:

```rust
    ViewPrDiff,
    CommentPullRequest,
```

…alongside worktree-scoped actions `DeleteWorktree(WorktreeId)`,
`SwitchBranch(WorktreeId)`, `ToggleRun(WorktreeId)`, `OpenWorktree(WorktreeId)`.
So a worktree-scoped `CreatePullRequest(WorktreeId)` has an obvious slot.

The architectural rule that governs how it is wired is `ARCHITECTURE.md:95`,
and it is strict:

> **Input is not action.** In `nebula-tui` a key arm and a mouse arm only
> translate: they say *which row* (`list_hit::row_at` for the pointer) and then
> call the one function that says *what choosing it does* —
> `event_loop::activate` for the grid and the modals `event_loop` owns […]
> Nothing that closes a modal, sends a request, spawns a process or moves a
> cursor lives inline in `handle_mouse` or in a key arm beside a twin that does
> the same.

and `ARCHITECTURE.md:97`:

> **A key handler never blocks.** […] anything that spawns a process, reads a
> file of unknown size or waits on the network is a BACKGROUND READ
> (`view_jobs.rs` […]) or one of the per-feature channels `main_loop` owns
> (`gh`, the BRANCH SWITCHER, issues).

A `gh pr create` is a network call, so it is a background job answered through a
channel — never inline in a handler. `pull_request.rs`'s existing comment-post
path is the model: `event_loop.rs:1635` `post_pr_comment` fires it and
`event_loop.rs:1703` `land_pr_comment` lands the answer.

### The reverse direction already exists

`ClientRequest::CreatePrAgent` (`crates/nebula-core/src/protocol.rs:142`) turns
a pull request into a session: the TUI sends the PR URL and the branch its
checkout should be on, and the daemon finds or cuts the worktree and starts the
agent there (`ARCHITECTURE.md:34`). Whatever this spike recommends should read
as that path's mirror image, and should use its vocabulary.

### Repo conventions the findings doc must respect

- **Comments explain why, at length, and name the failure that motivated them.**
  `pull_request.rs:909-920` is the exemplar — it justifies stdin over argv and
  explains why two failure modes are reported differently. Any interface this
  spike proposes should be documented to that standard so a build plan can lift
  the prose.
- **The domain vocabulary is capitalised in prose**: SESSION, WORKTREE, PROJECT,
  PULL REQUESTS MODAL, QUICK PROMPT, AGENT PRESET, ROOT WORKTREE, SHARED
  CHECKOUT. `ARCHITECTURE.md` is the glossary; read its PULL REQUESTS MODAL and
  ISSUES MODAL paragraphs (`:34`, `:36`) before writing.
- **`gh` failures are ordinary**, not exceptional: a missing `gh`, a bad exit and
  a timeout are all "couldn't ask" and are reported with `gh`'s own first stderr
  line where one exists (`pull_request.rs:952-962` `comment_error`).
- Anything that writes to GitHub is **outward-facing and hard to reverse**. A
  pull request opened by accident is visible to collaborators. Whatever shape is
  recommended must say what confirms it and what the undo is.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Confirm no create verb | `grep -rn '"create"' crates --include='*.rs'` | only `syntax.rs` |
| Confirm no upstream tracking | `grep -rn 'upstream\|@{u}\|rev-list --count' crates --include='*.rs' \| grep -v '/tests/'` | nothing relevant |
| `gh` present? | `command -v gh && gh --version` | a path and version, or nothing |
| `gh` auth state | `gh auth status` | prints the logged-in account, or says not logged in |
| What `gh pr create` offers | `gh pr create --help` | its flags, including `--web`, `--draft`, `--fill` |
| Branch push state, by hand | `git rev-parse --abbrev-ref '@{u}' 2>/dev/null \|\| echo "no upstream"` | an upstream ref, or the fallback |
| Typecheck | `cargo check --workspace --all-targets` | exit 0 |
| Tests | `cargo test --workspace` | exit 0 (see STOP conditions) |
| Throwaway worktree | `git worktree add /tmp/nebula-spike-008 HEAD` | a detached checkout |
| Remove it | `git worktree remove --force /tmp/nebula-spike-008` | gone |

**`gh` may not be on your PATH** even where the operator has it — the TUI runs
`gh` through the user's login-shell environment, which a non-interactive shell
does not inherit. If `command -v gh` fails, try
`PATH="/opt/homebrew/bin:/usr/local/bin:$PATH" gh --version`, and if it still
fails say so in the findings rather than guessing at `gh`'s behaviour.

## Scope

**In scope** — you may create or modify only:
- `plans/spikes/008-create-a-pull-request-findings.md` (create; create
  `plans/spikes/` if absent)
- `plans/README.md` — your status row only

**Out of scope** (do NOT commit changes to any of these):
- Everything under `crates/`.
- **Never run `gh pr create`, `gh issue create`, `gh pr merge`, `gh pr review`,
  `git push`, or `gh release` against any real repository.** These are
  outward-facing and visible to other people. `gh pr create --help` and
  `--dry-run`-style reading is how you learn the interface. If you genuinely
  need to see a create succeed, it must be against a throwaway repository you
  created for the purpose under your own account, and the findings doc must say
  which repository that was.
- `.claude/skills/` — the `pr-description`, `pr-reviewer` and `release` skills
  are evidence to read, not files to edit.
- `crates/nebula-core/src/protocol.rs` — `PROTOCOL_VERSION`
  (`protocol.rs:9`) is strict-equality gated by the daemon
  (`crates/nebula-daemon/src/server.rs:67`), so a protocol change is a
  coordinated change plus a version bump. Specify it; do not make it.
- `ARCHITECTURE.md` and `docs/`.
- The issue-creation half. `gh issue create` is a real adjacent gap, but it is a
  different surface with a different answer (an issue needs no branch and no
  push). Record it as a follow-up; do not design it here.
- Anything to do with plans 001–007.

## Steps

### Step 1: Read both directions and the skills

Read, in this order:

1. `ARCHITECTURE.md`, the PULL REQUESTS MODAL paragraph (`:34`) and the "Input is
   not action" / "A key handler never blocks" rules (`:95`, `:97`).
2. `crates/nebula-tui/src/pull_request.rs` in full — every `gh` call, the
   timeouts, and the error handling.
3. `crates/nebula-tui/src/pr_modal.rs` — how the modal turns a pull request into
   a session, so your recommendation can mirror it.
4. `crates/nebula-tui/src/event_loop.rs:1589-1760` — `open_pr_comment`,
   `post_pr_comment`, `land_pr_comment`, and the diff-request path beside them.
   This is the exact background-job shape a create would use.
5. `.claude/skills/pr-description/SKILL.md` in full. It is the single best piece
   of evidence about what a good pull request body costs here — ten templates and
   a house style. Note that its section 7 (`:166-182`) is the `gh pr create`
   invocation, and `:178` records the branch-pushed prerequisite.
6. `.claude/skills/pr-reviewer/SKILL.md:15-55` — the SHARED CHECKOUT constraint
   and the list of commands it refuses to run.

**Verify**: a findings section `## What exists today` listing the nine `gh`
verbs with their sites, the background-job shape a create would reuse (named
functions), and the SHARED CHECKOUT constraint in one sentence.

### Step 2: Price the branch-push prerequisite

Establish, by reading and by hand-running git:

- Confirm nebula tracks no upstream state (the grep above).
- What would it cost to know? `git rev-parse --abbrev-ref '@{u}'` and
  `git rev-list --count '@{u}..HEAD'` are the two measurements. Where would they
  go — the daemon's git poll (`crates/nebula-daemon/src/git.rs`) or a client-side
  BACKGROUND READ (`crates/nebula-tui/src/view_jobs.rs`)? Note that
  `ARCHITECTURE.md:26` says the daemon already polls git metadata on a tick, and
  `view_jobs.rs:45-73` is the client-side pattern with its ticket-keyed
  staleness handling.
- Does the prerequisite disappear under shape 1 (hand it to the agent)? The
  agent can run `git push` itself.
- Who pushes, if nebula does? A push is a mutation of a shared remote from a
  SHARED CHECKOUT that several sessions may be using. Say what confirms it.

**Verify**: a findings section `## The branch-push prerequisite` with the two git
measurements, a recommended home for them, the per-shape answer to whether it is
needed, and an explicit position on whether nebula should ever push.

### Step 3: Answer "who writes the body"

This is the crux. Weigh, in writing:

- **The agent writes it.** It has the diff and the conversation. If the user has
  a `pr-description`-style skill, it writes a far better body than a TUI form
  would. nebula's contribution is a keystroke and the context (which branch,
  which base, which issue to close).
- **nebula composes it.** Then nebula owns a house style it cannot know, for
  every repository its user owns. Hold this against the ten templates in
  `.claude/skills/pr-description/SKILL.md` and say honestly whether a modal can
  compete.
- **GitHub's web form takes it.** `gh pr create --web` opens the browser with
  the branch preselected and lets the user write it there, with GitHub's own
  template support. nebula composes nothing.
- **`gh pr create --fill`** uses the commit messages. Read `gh pr create --help`
  and say whether that is a credible default for this repo's commit style —
  `git log --format=%s -8` shows the house style is long descriptive sentences,
  which may or may not fill well.

**Verify**: a findings section `## Who writes the body` with all four weighed and
one recommended, naming what evidence decided it.

### Step 4: Specify the recommended shape

Pick one of the four shapes from "The question this spike answers" — including
shape 4, *nothing* — and specify it to the point that a build plan can be
written from the findings doc alone:

- The user-facing gesture: which menu, which key, which row it hangs off, and
  the `MenuAction` variant name. Check the existing names at `app.rs:252-355`
  for the naming register.
- The wiring: which function fires the background job, which lands the answer,
  which channel carries it. Name the `event_loop.rs` functions it would sit
  beside.
- What confirms it. A pull request is outward-facing; say whether this goes
  behind a confirm dialog, and note that `AskBeforeArchive`
  (`crates/nebula-tui/src/config.rs:417`) is precedent for a confirm being a
  *setting* rather than always-on.
- Every failure, and what the user sees: no `gh`, `gh` not logged in, no write
  access, branch not pushed, branch has no commits, a pull request already open
  on that branch, a timeout. `pull_request.rs:952-962` `comment_error` is the
  existing pattern for turning `gh`'s stderr into one flashable line.
- What the undo is, or that there is none.

**Verify**: a findings section `## Recommended shape` covering the gesture, the
wiring with named functions, the confirmation, every failure above with its
user-visible message, and the undo.

### Step 5: Prototype only if a question is still open

Skip entirely if steps 2–4 are answered from reading. Prototype only to settle
something reading could not — most likely how `gh pr create --web` behaves when
the branch has no upstream, or what `gh pr create` prints on each failure.

Investigate `gh`'s *failure* behaviour without creating anything real: run it in
a scratch git repository with no remote, or with a bogus remote, and capture what
it says.

```sh
cd "$(mktemp -d)" && git init -b main .
git -c user.email=t@example.invalid -c user.name=t commit --allow-empty -m init
gh pr create --title x --body y 2>&1 | head -5   # expect a clean refusal, nothing created
```

If you need the real tree, use the throwaway worktree, and **always** the
isolation variables for any nebula run:

```sh
git worktree add /tmp/nebula-spike-008 HEAD
cd /tmp/nebula-spike-008
NEBULA_RUNTIME_DIR=/tmp/nb-008-rt NEBULA_DATA_DIR=/tmp/nb-008-data cargo run
```

Clean up:

```sh
git worktree remove --force /tmp/nebula-spike-008
rm -rf /tmp/nb-008-rt /tmp/nb-008-data
```

`NEBULA_RUNTIME_DIR` and `NEBULA_DATA_DIR` (`crates/nebula-core/src/env.rs:16-18`)
keep a prototype off the operator's live daemon and real sessions. A run without
them can attach to, rename or kill their actual work. `Makefile:52-53` does the
same for `make dev`.

**Verify**: `git worktree list` shows one entry; `git status --short` in the main
checkout lists nothing under `crates/`; no pull request was created on any real
repository.

### Step 6: Write the findings doc

Assemble `plans/spikes/008-create-a-pull-request-findings.md`:

```markdown
# Spike 008 findings: how a session's branch becomes a pull request

- **Spiked at**: commit <short SHA>, <YYYY-MM-DD>
- **Question**: <one sentence>
- **Recommendation**: <one paragraph: which shape, and what is explicitly not being built>

## What exists today
## The branch-push prerequisite
## Who writes the body
## The four shapes weighed
## Recommended shape
## Follow-ups not designed here
## Open questions for the operator
## What I did not investigate
```

`## Follow-ups not designed here` should name `gh issue create` and anything else
the investigation surfaced. `## Open questions for the operator` is required and
must not be empty — whether nebula should create pull requests at all is a
product call, and if you are not certain, that is where it goes.

**Verify**: every section present and non-empty; the Recommendation names one
shape and states what is deliberately not being built.

### Step 7: Final check

**Verify**:
- `git status --short` lists only
  `plans/spikes/008-create-a-pull-request-findings.md` and `plans/README.md`.
- `git worktree list` shows one entry.
- `git diff --stat -- crates Cargo.toml Cargo.lock` produces no output.
- No pull request, issue, release or push was made against any repository the
  operator owns.

## Test plan

A spike ships no product code, so there are no new tests. Verification is the
findings doc's completeness, per the done criteria.

For whoever writes the build plan, record these in the findings doc's own
maintenance notes:

- `gh`-calling code is tested by stubbing `gh` on `PATH`. `crates/nebula/tests/e2e_tui.rs:546`
  and `:610` write a stub `gh` into a temp directory and prepend it — that is the
  pattern, and it means a create path can be tested end to end without touching
  GitHub.
- `pull_request.rs`'s own inline tests cover the parsing and error-reduction
  helpers; `comment_error`'s test is the model for a `create_error`.
- `ARCHITECTURE.md:95` requires INPUT PARITY tests for any new activation: the
  same row chosen once by key and once by pointer must do the same thing. The
  existing parity tests live in `crates/nebula-tui/src/event_loop.rs`'s test
  module. A new menu action needs one.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `plans/spikes/008-create-a-pull-request-findings.md` exists
- [ ] `grep -c '^## ' plans/spikes/008-create-a-pull-request-findings.md` returns at least 7
- [ ] `## The four shapes weighed` names all four, including "nothing"
- [ ] `## Who writes the body` reaches an explicit recommendation
- [ ] `## Recommended shape` lists at least six distinct failure cases with their user-visible messages
- [ ] `## The branch-push prerequisite` states whether nebula should ever push
- [ ] `## Open questions for the operator` is present and non-empty
- [ ] `grep -rn '"create"' crates --include='*.rs' | grep -vc syntax.rs` returns 0 — no create verb was added
- [ ] `git diff --stat -- crates Cargo.toml Cargo.lock` produces no output
- [ ] `git worktree list` shows exactly one worktree
- [ ] `git status --short` lists only the findings doc and `plans/README.md`
- [ ] `plans/README.md` status row for 008 updated

## STOP conditions

Stop and report back (do not improvise) if:

- `cargo test --workspace` fails on anything other than the two known `e2e_tui`
  failures at `565080d` (`nebula_open_from_inside_a_session_raises_the_file_tabs`
  and `tui_drag_past_the_pane_top_autoscrolls_and_copies_the_run`, both timing
  out on `text "^q: sessions"`, fixed by plan 001). Note those and carry on;
  anything else is a STOP.
- `gh` is unavailable and cannot be found on any likely path. Report it — the
  spike can still weigh the four shapes from `gh`'s published documentation, but
  say which claims are unverified.
- You are about to run `gh pr create`, `gh issue create`, `gh pr merge`,
  `gh pr review`, `git push` or `gh release` against a repository the operator
  owns. Stop. These are visible to other people and this spike has no mandate to
  create anything.
- You created something outward-facing by accident. Stop immediately and report
  it with the URL so the operator can delete it.
- The recommendation is turning into "nebula should compose the body itself" and
  you have not read `.claude/skills/pr-description/SKILL.md` in full. Read it
  first; it is the strongest argument against that shape and the spike is not
  honest without it.
- `crates/nebula-tui/src/pull_request.rs:905-950` no longer matches the excerpt
  above.

## Maintenance notes

- "Nothing" is a legitimate recommendation and should not be treated as a
  failed spike. nebula's GitHub surface is read-and-comment, and that is a
  coherent position: the agent in the pane is better placed to open a pull
  request than the multiplexer around it. If that is the answer, the findings doc
  should say it plainly enough that the question stays closed.
- Whatever is recommended, the branch-push question outlives it. Knowing whether
  a worktree's branch is ahead of its remote is useful on the grid independently
  of pull requests — it is the thing that tells you a session's work is still
  only on your disk. Worth recording as a finding in its own right.
- `gh issue create` is the cheaper sibling of this question and was deliberately
  excluded: an issue needs no branch, no push and no write access to code, so its
  answer may well be yes where this one is no. It deserves its own small spike.
- A reviewer of the eventual build work should check the confirmation path
  hardest. Every other `gh` write in nebula (`pr comment`, `issue comment`,
  `issue edit`) modifies something that already exists; creating a pull request
  is the first irreversible, collaborator-visible action nebula would take, and
  it should not be one keystroke from a menu without a confirm.
