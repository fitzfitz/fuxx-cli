# Milestone 3 — Fire a Notification — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On OSC 9 detection, fire a real native macOS desktop notification (replacing Milestone 2's throwaway `NOTIFICATION DETECTED` stderr line). Completes v1.

**Architecture:** A new decoupled module `src/notifier.rs` with a best-effort `fire(summary, body)` over `notify-rust`. The output pump calls it from the existing detection branch. Detection (`detector`) and dispatch (`notifier`) stay separate; `main` wires them.

**Tech Stack:** Rust (edition 2024), `notify-rust = "4.18.0"`.

## Global Constraints

- Rust **edition 2024**; crate at repo root; **macOS-first**.
- Add exactly one dependency: `notify-rust = "4.18.0"` (verified crates.io 2026-07-17). No others.
- **macOS notify-rust rules:** use `.summary()` + `.body()` + `.show()` only. Do NOT use `.icon()`,
  `.appname()`, or `.hint()` — no-ops/unsupported on macOS. `.show()` is the synchronous call.
- **Best-effort dispatch:** discard the `show()` Result (`let _ = ...`); a failed notification must
  never crash or interrupt the wrapper.
- Keep **detection and dispatch decoupled** (`docs/02-Architecture/26 - Notification Dispatch.md`);
  the Milestone 1 passthrough and the Milestone 2 detector stay unchanged.
- Notification text: summary `"fuxx-cli"`, body `"Your agent sent a notification"`.
- **v1 completes here** — no throttling, no payload extraction, no OSC 99/777, no config, no icons.
- Test output **pristine** (no warnings); `cargo test` must fire **no** banners.
- Explain Rust concepts in commit/PR notes; observe every success check
  (`docs/05-Agentic-Workflow/51 - Delegation Principles.md`).

## Refinement to the approved spec (same intent)

The spec's "temporary startup test notification" is realized as: **Task 1** proves `notify-rust`
builds/links on this machine; **Task 2's manual `printf` → banner check** proves it actually fires
and surfaces any macOS permission prompt. This avoids shipping an every-launch notification and
keeps `cargo test` banner-free. No behavioral difference in the shipped tool.

---

### Task 1: Add `notify-rust` and the `notifier` module

**Files:**
- Modify: `Cargo.toml` (add the dependency)
- Create: `src/notifier.rs`
- Modify: `src/main.rs` (register the module; not yet wired into the pump)

**Interfaces:**
- Produces (consumed by Task 2): `notifier::fire(summary: &str, body: &str)` — best-effort desktop
  notification; returns `()`.

- [ ] **Step 1: Add the dependency**

Run: `cargo add notify-rust@4.18.0`
Expected: `Cargo.toml` `[dependencies]` gains `notify-rust = "4.18.0"`.

- [ ] **Step 2: Create `src/notifier.rs`**

```rust
use notify_rust::Notification;

/// Fire a desktop notification (macOS banner). Best-effort: a failure to display must never
/// crash the wrapper, so the `show()` Result is intentionally discarded. On macOS, only
/// `summary` and `body` are meaningful (`icon`/`appname`/`hint` are no-ops there).
pub fn fire(summary: &str, body: &str) {
    let _ = Notification::new().summary(summary).body(body).show();
}
```

- [ ] **Step 3: Register the module** — add to the top of `src/main.rs`, right after `mod detector;`

```rust
#[allow(dead_code)] // wired into the detection branch in Task 2
mod notifier;
```

- [ ] **Step 4: Verify it builds and nothing regressed**

Run: `cargo build`
Expected: compiles and **links** cleanly — this proves `notify-rust` and its native backend resolve
on this machine.

Run: `cargo test`
Expected: PASS — all existing tests (6 detector unit + 2 cli unit + 6 wrapper integration). No
warnings (the `#[allow(dead_code)]` keeps `fire` from warning while unused). No banners fire (no test
calls `notifier::fire`).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/notifier.rs src/main.rs
git commit -m "feat(m3): add notify-rust and a decoupled notifier module"
```

---

### Task 2: Wire detection → notification; retire the stderr signal

**Files:**
- Modify: `src/main.rs` (call the notifier from the detection branch; remove the `#[allow(dead_code)]`)
- Modify: `tests/wrapper.rs` (remove the two now-obsolete OSC-9 stderr tests)

**Interfaces:**
- Consumes: `notifier::fire` from Task 1 and the existing `detector` in the pump.
- Produces: observable behavior — a macOS banner when the child emits an OSC 9 sequence.

- [ ] **Step 1: Remove the two obsolete Milestone 2 integration tests** from `tests/wrapper.rs`

Delete these two test functions entirely (they asserted the `NOTIFICATION DETECTED` stderr string,
which this task removes):
- `detects_osc9_notification_on_stderr`
- `no_detection_without_a_sequence`

Leave the other wrapper tests (`echo_is_passed_through`, `exit_code_is_propagated`,
`no_args_is_an_error`, `unknown_command_is_an_error`) untouched.

- [ ] **Step 2: Wire the notifier into the pump** — edit `src/main.rs`

Change the module registration (remove the attribute) so it reads:

```rust
mod notifier;
```

In the output pump's `Ok(n)` branch, replace the Milestone 2 detection block:

```rust
            if detector.feed(&buf[..n]) > 0 {
                eprintln!("NOTIFICATION DETECTED");
            }
```

with:

```rust
            if detector.feed(&buf[..n]) > 0 {
                notifier::fire("fuxx-cli", "Your agent sent a notification");
            }
```

Everything else in `run()` — the passthrough `write_all`/`flush` above it, the detector, the exit
path — stays unchanged.

- [ ] **Step 3: Verify tests (regression, no banners)**

Run: `cargo test`
Expected: PASS — 6 detector unit + 2 cli unit + 4 remaining wrapper tests. No warnings. **No banners
fire**, because no remaining test exercises the detection branch.

- [ ] **Step 4: Manual verification — the milestone's success check (human, real terminal)**

This is the part that cannot be automated (firing a real banner is an OS effect). In a real terminal,
from the repo root:

1. `cargo run -- printf '\033]9;done\007'` → a macOS banner titled **fuxx-cli** with body **Your
   agent sent a notification** appears. If macOS prompts for notification permission for your
   terminal, grant it and retry. (No banner *and* no prompt → check the terminal's notification
   permission in System Settings; the wiring itself is a single line over the already-tested
   detector.)
2. Real end-to-end: `cargo run -- <your agent>` and, when it emits an OSC 9 notification, confirm the
   banner appears.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs tests/wrapper.rs
git commit -m "feat(m3): fire a desktop notification on OSC 9 detection"
```

---

### Task 3: Verify v1 complete and update the vault

**Files:**
- Modify: `docs/03-Tech-Stack/33 - Key Crates.md` (note notify-rust is now in use)
- Modify: `docs/04-Development/43 - Milestone 3 - Fire a Notification.md` (status + checklist)
- Modify: `docs/04-Development/40 - Development Roadmap.md` (Milestone 3 ✅)
- Modify: `docs/06-Progress-Log/60 - Progress Log.md` (new entry + snapshot)

- [ ] **Step 1: Confirm the checks**

Run: `cargo test` (all green, pristine, no banners). Confirm the Task 2 manual banner check was
observed to pass. Only proceed once both hold.

- [ ] **Step 2: Update `docs/03-Tech-Stack/33 - Key Crates.md`**

In the existing `notify-rust` section, add a sentence noting it is now an actual dependency, added in
Milestone 3, used by `src/notifier.rs` to fire the macOS banner on detection.

- [ ] **Step 3: Mark Milestone 3 done** in `docs/04-Development/43 - Milestone 3 - Fire a Notification.md`

Set frontmatter `status: done`, `**Status:: ✅ Done**`, and check all four checklist boxes. Add a
one-line note that the "startup test notification" was realized as a build/link proof (Task 1) plus
the manual detection→banner check (Task 2).

- [ ] **Step 4: Flip the roadmap** in `docs/04-Development/40 - Development Roadmap.md`

Change the Milestone 3 line from `⬜` to `✅`. (The roadmap already marks "v1 is complete here" on
that line — leave that text.)

- [ ] **Step 5: Update the Progress Log** in `docs/06-Progress-Log/60 - Progress Log.md`

Add a new entry at the TOP of the Entries section using the `61 - Log Entry Template`, recording that
Milestone 3 is done and **v1 is complete**. Update the "Current status snapshot": current milestone →
`[[44 - Milestone 4 - Ship via Homebrew]]` (optional/distribution); overall phase = "v1 complete".
Next task = Milestone 4 (optional) or "v1 done — stop adding features."

- [ ] **Step 6: Commit**

```bash
git add "docs/03-Tech-Stack/33 - Key Crates.md" "docs/04-Development/43 - Milestone 3 - Fire a Notification.md" "docs/04-Development/40 - Development Roadmap.md" "docs/06-Progress-Log/60 - Progress Log.md"
git commit -m "docs(m3): mark Milestone 3 done, v1 complete, log progress"
```

---

## Self-Review (author's check against the spec)

**Spec coverage:**
- `notifier` module + `notify-rust` dep (spec §3, §4) → Task 1. Decoupled dispatch (§4) → separate
  module, called from the pump. Wiring replacing the stderr line (§4) → Task 2 Step 2. Prove-the-crate
  intent (§5) → Task 1 build/link + Task 2 manual banner (see Refinement). Remove the two M2 stderr
  tests (§6) → Task 2 Step 1. Manual verification (§6) → Task 2 Step 4. Best-effort/`macOS` rules
  (§3) → Task 1 Step 2 code + Global Constraints. v1-complete docs (§8) → Task 3. All mapped.

**Placeholder scan:** No TBD/TODO. No red-test stub this milestone (the deliverable's success check is
a manual OS banner, explicitly stated); all code steps carry complete code.

**Type consistency:** `notifier::fire(summary: &str, body: &str) -> ()` is identical in Task 1
(definition) and Task 2 (call site, two string literals). `mod notifier;` registration is added with
`#[allow(dead_code)]` in Task 1 and the attribute removed in Task 2 when the call makes it used.
