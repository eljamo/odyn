# Plan 001: `cargo test --workspace` passes again

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 565080d..HEAD -- crates/nebula/tests/e2e_tui.rs crates/nebula-tui/src/ui.rs`
> If either file changed since this plan was written, compare the "Current
> state" excerpts below against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `565080d`, 2026-10-06

## Why this matters

Two end-to-end TUI tests fail deterministically at `565080d`. They assert the
terminal pane's locked-input footer reads `"^q: sessions"`, but the TUI stopped
rendering that string when the grid became the only view — the footer now reads
`"^`: back to the card  …"`. Nothing is broken in the product; the assertion is
stale.

The cost is that `make test` and `make ci` both exit non-zero, so the suite
cannot gate anything, and a red suite trains everyone to ignore it. 1,623
library tests and 60 of 62 integration tests pass; these two are the only
failures. Plan 002 adds a CI workflow that runs this suite, and it cannot land
until this is green.

## Current state

### The stale assertion

`crates/nebula/tests/e2e_tui.rs:33-37` — test-file constants:

```rust
/// A row only the PROJECT's own menu carries.
const PROJECT_MENU_ROW: &str = "Remove from list";
/// Terminal pane input-locked: keys forward to the PTY. The footer spells
/// chords the compact way `KeyChord::display` does — `^q`, not `Ctrl+q`.
const FOOTER_TERMINAL_LOCKED: &str = "^q: sessions";
```

It is waited on in two places. `crates/nebula/tests/e2e_tui.rs:376-387` —
the `start_session` helper:

```rust
fn start_session(tui: &mut TuiHarness, task: &str) {
    tui.send(b"p");
    tui.wait_for_text("what should the agent do?");
    tui.type_str(task);
    tui.send(ENTER);
    tui.wait_for_gone("what should the agent do?");
    // The launch lands the card without taking the pane; Enter steps into
    // it, which is where the keys have to be to type at the agent.
    tui.wait_for_text("agent-1");
    tui.send(ENTER);
    tui.wait_for_text(FOOTER_TERMINAL_LOCKED);
}
```

…and `crates/nebula/tests/e2e_tui.rs:479-481`, at the end of
`nebula_open_from_inside_a_session_raises_the_file_tabs`:

```rust
    // ---- Ctrl+Q from the strip closes; the locked pane is back ----
    tui.send(CTRL_Q);
    tui.wait_for_gone("Open files (2)");
    tui.wait_for_text(FOOTER_TERMINAL_LOCKED);
```

### What the footer actually renders

`crates/nebula-tui/src/ui.rs:4125-4156` — the locked-pane arm of the footer:

```rust
            Focus::Terminal if app.term_locked => format!(
                "{}: {}  {}  ⌥click: open link",
                // The pane under the cards is left by the fold's own key
                // (`^``: back to the card, again: fold the pane).
                if app.launcher_grid() {
                    app.keymap
                        .chords(Action::ToggleLauncherPane)
                        .iter()
                        .find(|c| !crate::key_combo::is_text_key(c))
                        .map(|c| c.display())
                } else {
                    None
                }
                .or_else(|| app.keymap.first(Action::UnlockTerminal).map(|c| c.display()))
                .unwrap_or_else(|| "^q".into()),
                // The LAUNCHER VIEW has its grid of sessions to go back to,
                // and a full-screen session comes back down to its pane.
                if app.launcher_grid() {
                    "back to the card"
                } else if app.launcher_active() && app.collapsed {
                    "normal size"
                } else {
                    "sessions"
                },
                // A program that asked for the mouse gets the drag (its
                // own selection copies); promising nebula's would lie.
                if app.child_mouse_mode().0 != vt100::MouseProtocolMode::None {
                    "drag: to the app (⇧drag: terminal)"
                } else {
                    "drag: select+copy"
                },
            ),
```

`App::launcher_grid` is `crates/nebula-tui/src/app.rs:4202-4204`:

```rust
    pub fn launcher_grid(&self) -> bool {
        self.launcher_active() && !self.collapsed
    }
```

Since the grid became the only view, `launcher_grid()` is true for any
attached, non-full-screen session — so the label is `"back to the card"` and
the `"sessions"` branch is not reached in these tests. The string
`q: sessions` does not appear anywhere in `crates/nebula-tui/src`.

### What the failing tests actually see

Captured from the harness's own timeout dump, bottom line of the screen:

```
 nebula v0.43.0  ·  open-proj ▸ main ▸ agent-1    ^`: back to the card  drag: select+copy  ⌥c
```

Note the right edge: `⌥click: open link` is clipped to `⌥c` at this terminal
width, so an assertion must not depend on the tail of the footer.

### Repo conventions that apply here

- Test constants carry a doc comment saying what the string *means*, not just
  what it is — see `PROJECT_MENU_ROW` at `e2e_tui.rs:33-34`. Match that shape.
- The harness matches on screen text with `wait_for_text`, a substring search
  over the rendered screen (`e2e_tui.rs:160-190`). A 20-second deadline
  (`WAIT`, `e2e_tui.rs:20`) and a 50 ms poll.
- Comments in this repo explain *why*, often at length, and are kept in sync
  with the code. A comment that no longer describes the code is treated as a
  defect here.

## Commands you will need

| Purpose             | Command                                                        | Expected on success        |
|---------------------|----------------------------------------------------------------|----------------------------|
| Typecheck           | `cargo check --workspace --all-targets`                        | exit 0                     |
| The two tests       | `cargo test -p nebula --test e2e_tui -- nebula_open_from_inside_a_session_raises_the_file_tabs tui_drag_past_the_pane_top_autoscrolls_and_copies_the_run` | `2 passed; 0 failed` |
| All e2e TUI tests   | `cargo test -p nebula --test e2e_tui`                          | `10 passed; 0 failed`      |
| Whole suite         | `make test`                                                    | exit 0, every target `ok`  |
| Format check        | `cargo fmt --all -- --check`                                   | exit 0, no output          |

These are this repo's real commands, taken from the `Makefile` and verified at
`565080d`. The full suite takes about 60 seconds.

## Scope

**In scope** (the only file you should modify):
- `crates/nebula/tests/e2e_tui.rs`

**Out of scope** (do NOT touch, even though they look related):
- `crates/nebula-tui/src/ui.rs` — the footer is correct; the test is wrong.
  Do not change the rendered text to match the test.
- The `"sessions"` / `"normal size"` branches at `ui.rs:4144-4148` — they may
  well be unreachable now, but deciding that is a separate question and
  deleting them here would hide a product change inside a test fix.
- `crates/nebula-tui/src/keymap.rs` — the default chord bindings are fine.
- Any other test in `e2e_tui.rs` — the other eight pass.

## Git workflow

- Branch: `fix-stale-locked-pane-footer-assertion` (this repo uses bare
  descriptive branch names, sometimes prefixed with the issue number —
  e.g. `fix-106-open-pr-list-stale`, `dead-code-sweep-91`).
- One commit. This repo does **not** use conventional commits; messages are
  descriptive sentences, capitalised, no type prefix. For example:
  `The locked pane's e2e assertion reads the label the grid's footer actually draws, so the suite is green again`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Confirm the failure before changing anything

Run the two tests and read the output. You must see both fail with
`timed out waiting for: text "^q: sessions"`.

**Verify**:
`cargo test -p nebula --test e2e_tui -- nebula_open_from_inside_a_session_raises_the_file_tabs tui_drag_past_the_pane_top_autoscrolls_and_copies_the_run`
→ `test result: FAILED. 0 passed; 2 failed`, and the panic message for each
names the text `"^q: sessions"`.

If they pass, STOP (see STOP conditions) — the codebase has moved on.

### Step 2: Point the constant at the label, not the chord

In `crates/nebula/tests/e2e_tui.rs`, replace lines 35-37:

```rust
/// Terminal pane input-locked: keys forward to the PTY. The footer spells
/// chords the compact way `KeyChord::display` does — `^q`, not `Ctrl+q`.
const FOOTER_TERMINAL_LOCKED: &str = "^q: sessions";
```

with:

```rust
/// Terminal pane input-locked: keys forward to the PTY. The label alone,
/// not the chord in front of it — on the grid the footer names whichever
/// key the Hotkeys tab has on `ToggleLauncherPane` (`ui.rs`'s locked-pane
/// arm), so asserting the chord ties this test to a default binding. The
/// tail (`⌥click: open link`) is clipped at the harness's width, so the
/// label is also the widest part that is always drawn.
const FOOTER_TERMINAL_LOCKED: &str = "back to the card";
```

Change nothing else. Both wait sites keep using the constant.

**Verify**: `cargo check --workspace --all-targets` → exit 0.

### Step 3: Confirm the two tests pass

**Verify**:
`cargo test -p nebula --test e2e_tui -- nebula_open_from_inside_a_session_raises_the_file_tabs tui_drag_past_the_pane_top_autoscrolls_and_copies_the_run`
→ `test result: ok. 2 passed; 0 failed`.

Run it a second time. It must pass twice — these tests drive a real PTY, and a
single green run does not prove the fix rather than a lucky timing window.

### Step 4: Confirm nothing else in the file regressed

`start_session` is called by three tests, and the constant is also waited on
directly at the end of `nebula_open_from_inside_a_session_raises_the_file_tabs`.

**Verify**: `cargo test -p nebula --test e2e_tui`
→ `test result: ok. 10 passed; 0 failed`.

### Step 5: Confirm the whole suite is green

**Verify**: `make test` → exit 0. Every target reports `test result: ok`.
Expect roughly: 33 + 308 + 16 + 1266 library tests, then 7 + 5 + 31 + 10 + 6 + 3
integration tests. Total runtime about 60 seconds.

Then `cargo fmt --all -- --check` → exit 0, no output.

## Test plan

No new tests. This plan repairs two existing ones; the product behaviour they
cover (stepping into a session locks the pane, and Ctrl+Q out of the file-tabs
strip returns to it) is unchanged and still asserted.

The regression guard for "a footer string changed and the tests did not" is
plan 002, which puts this suite in CI. Do not add a unit test that pins the
footer text — that would couple a second test to the same wording and double
the maintenance, which is the problem this plan is cleaning up.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo fmt --all -- --check` exits 0
- [ ] `cargo check --workspace --all-targets` exits 0
- [ ] `cargo test -p nebula --test e2e_tui` reports `10 passed; 0 failed`, twice in a row
- [ ] `make test` exits 0
- [ ] `grep -n 'q: sessions' crates/nebula/tests/e2e_tui.rs` returns no matches
- [ ] `git diff --name-only` lists only `crates/nebula/tests/e2e_tui.rs`
- [ ] `plans/README.md` status row for 001 updated

## STOP conditions

Stop and report back (do not improvise) if:

- Step 1's two tests already pass at the commit you are on — somebody fixed
  this, and the rest of the plan would be a no-op change to a working test.
- `crates/nebula-tui/src/ui.rs:4125-4156` does not match the excerpt above —
  the footer has been rewritten again and the right label is now something
  else. Report what it renders instead; do not guess.
- The two tests still fail after step 2, on a *different* assertion. That is a
  second, unrelated failure and it is not in this plan's scope. Paste the new
  panic message, including the `--- screen ---` dump.
- `make test` shows failures in any target other than `e2e_tui`. Those were
  passing at `565080d`; something else is wrong and this plan should not
  absorb it.
- Fixing it appears to require editing `crates/nebula-tui/src/` at all.

## Maintenance notes

- The footer is assembled in one place, `ui.rs:4125-4156`, and this test is the
  only thing outside the TUI crate that reads it. If the locked-pane label
  changes again, this constant is the single site to update.
- A reviewer should check that `ui.rs` is untouched in the diff. A fix that
  changed the rendered footer to match the old test would also be green, and
  would be wrong.
- Deliberately deferred: the `"sessions"` and `"normal size"` branches at
  `ui.rs:4144-4148` look unreachable now that the grid is the only view. That
  wants a product decision and its own plan, not a drive-by deletion here.
- Also deferred: this suite has no coverage of the non-grid footer paths at
  all, which is why the drift went unnoticed. Plan 002 catches the *symptom*
  (a red suite nobody sees) rather than the gap.
