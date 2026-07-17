# Milestone 2 — Detect the Signal — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect the OSC 9 notification intro (`ESC ]9;`) in the child's output stream as it passes through the Milestone 1 pump, and log `NOTIFICATION DETECTED` to stderr.

**Architecture:** A new pure module `src/detector.rs` holds a small stateful `OscDetector` (a byte-level state machine that carries partial-match state across `read()` chunks). `src/main.rs` feeds each output chunk to the detector right after the existing (unchanged) passthrough write, and prints the signal on a detection.

**Tech Stack:** Rust (edition 2024), std only. No new dependencies.

## Global Constraints

- Rust **edition 2024**; crate at the **repo root**; macOS-first.
- **No new dependencies** — `OscDetector` is std-only.
- **Byte-level** matching. Target = OSC 9 intro `ESC ] 9 ;` = `[0x1b, 0x5d, 0x39, 0x3b]`.
- The Milestone 1 **passthrough must stay byte-for-byte unchanged**; detection is additive and runs
  *after* the `stdout.write_all(&buf[..n])` in the pump's `Ok(n)` branch.
- The detection signal goes to **stderr** (`eprintln!`), never stdout.
- Match must **exclude OSC 99** (`ESC ] 9 9 ;`) — matching through the `;` handles this.
- Out of scope (do NOT build): payload/terminator parsing, OSC 99/777 variants, ConEmu OSC 9
  disambiguation, the real OS notification (M3), any terminal emulation.
- Test output must be **pristine** (no warnings).
- Explain Rust concepts in commit/PR notes; run and observe every success check
  (`docs/05-Agentic-Workflow/51 - Delegation Principles.md`).

---

### Task 1: `OscDetector` — pure stateful scanner (unit-tested)

**Files:**
- Create: `src/detector.rs`
- Modify: `src/main.rs` (register the module; not yet wired into the pump)

**Interfaces:**
- Produces (consumed by Task 2): `OscDetector::new() -> OscDetector` and
  `OscDetector::feed(&mut self, bytes: &[u8]) -> usize` returning the count of complete OSC 9
  intros newly recognised across this and prior calls' carried state.

- [ ] **Step 1: Write the failing unit tests** in `src/detector.rs`

```rust
/// OSC 9 desktop-notification intro: ESC ] 9 ;
const TARGET: [u8; 4] = [0x1b, 0x5d, 0x39, 0x3b];

pub struct OscDetector {
    /// How many leading bytes of TARGET have matched so far (0..TARGET.len()).
    matched: usize,
}

impl OscDetector {
    pub fn new() -> Self {
        OscDetector { matched: 0 }
    }

    pub fn feed(&mut self, _bytes: &[u8]) -> usize {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_full_intro_in_one_chunk() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;hello\x07"), 1);
    }

    #[test]
    fn detects_intro_split_across_chunks() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"some output\x1b"), 0);
        assert_eq!(d.feed(b"]9;done\x07"), 1);
    }

    #[test]
    fn does_not_detect_osc_99() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]99;progress\x07"), 0);
    }

    #[test]
    fn ignores_plain_text() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"hello world\n"), 0);
    }

    #[test]
    fn counts_two_intros_in_one_chunk() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;a\x07\x1b]9;b\x07"), 2);
    }

    #[test]
    fn restarts_on_repeated_esc() {
        let mut d = OscDetector::new();
        // A stray ESC before the real intro must not swallow the match.
        assert_eq!(d.feed(b"\x1b\x1b]9;x\x07"), 1);
    }
}
```

- [ ] **Step 2: Register the module and run the tests to see them fail**

Add to the top of `src/main.rs`, right after `mod cli;`:

```rust
#[allow(dead_code)] // wired into the output pump in Task 2
mod detector;
```

Run: `cargo test detector`
Expected: compiles, the six `detector::tests` FAIL (panic at `unimplemented!()`). The
`#[allow(dead_code)]` keeps the build warning-free while the module is not yet used by the binary.

- [ ] **Step 3: Implement `feed`** — replace the `unimplemented!()` body in `src/detector.rs`

```rust
    pub fn feed(&mut self, bytes: &[u8]) -> usize {
        let mut detections = 0;
        for &b in bytes {
            if b == TARGET[self.matched] {
                self.matched += 1;
                if self.matched == TARGET.len() {
                    detections += 1;
                    self.matched = 0; // ready for the next sequence
                }
            } else if b == TARGET[0] {
                // The byte that broke the match is itself ESC: begin a fresh match from it,
                // so e.g. "ESC ESC ]9;" is still detected.
                self.matched = 1;
            } else {
                self.matched = 0;
            }
        }
        detections
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test`
Expected: PASS — the six new `detector` unit tests plus all existing Milestone 1 tests
(2 cli + 4 wrapper). No warnings.

- [ ] **Step 5: Commit**

```bash
git add src/detector.rs src/main.rs
git commit -m "feat(m2): add stateful OscDetector for the OSC 9 intro"
```

---

### Task 2: Wire the detector into the pump + integration test

**Files:**
- Modify: `src/main.rs` (feed the detector in the pump; remove the `#[allow(dead_code)]`)
- Modify: `tests/wrapper.rs` (add integration tests)

**Interfaces:**
- Consumes: `OscDetector::new()` / `feed` from Task 1.
- Produces: observable behavior — `NOTIFICATION DETECTED` on stderr when the child emits `ESC ]9;`.

- [ ] **Step 1: Write the failing integration tests** — append to `tests/wrapper.rs`

`/bin/echo` and friends resolved via `CommandBuilder` in Milestone 1, so `printf` resolves the same
way. macOS `printf` does not support `\e`, so ESC is written as the octal escape `\033` and BEL as
`\007`.

```rust
#[test]
fn detects_osc9_notification_on_stderr() {
    let out = fuxx()
        .args(["printf", r"\033]9;task done\007"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("NOTIFICATION DETECTED"),
        "stderr was: {err:?}"
    );
}

#[test]
fn no_detection_without_a_sequence() {
    let out = fuxx().args(["echo", "hello"]).output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("NOTIFICATION DETECTED"), "stderr was: {err:?}");
}
```

- [ ] **Step 2: Run the integration tests to see the first one fail**

Run: `cargo test --test wrapper detects_osc9_notification_on_stderr`
Expected: FAIL — the detector isn't wired into the pump yet, so nothing is written to stderr.
(`no_detection_without_a_sequence` already passes; that's fine.)

- [ ] **Step 3: Wire the detector into the pump** — edit `src/main.rs`

Remove the `#[allow(dead_code)]` line added in Task 1, so the module declaration reads just:

```rust
mod detector;
```

Add to the `use` section near the top:

```rust
use detector::OscDetector;
```

Create the detector just before the output pump loop (right after `let mut buf = [0u8; 8192];`):

```rust
    let mut detector = OscDetector::new();
```

In the pump's `Ok(n)` branch, after the existing `stdout.write_all(&buf[..n])?` and
`stdout.flush()?` (passthrough stays first and unchanged), add:

```rust
            if detector.feed(&buf[..n]) > 0 {
                eprintln!("NOTIFICATION DETECTED");
            }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test`
Expected: PASS — both new integration tests, the six detector unit tests, and all Milestone 1
tests. No warnings (the `#[allow(dead_code)]` is gone because the module is now used).

- [ ] **Step 5: Commit**

```bash
git add src/main.rs tests/wrapper.rs
git commit -m "feat(m2): detect OSC 9 in the output stream and log to stderr"
```

---

### Task 3: Verify the milestone and update the vault

**Files:**
- Modify: `docs/04-Development/42 - Milestone 2 - Detect the Signal.md` (status + checklist)
- Modify: `docs/04-Development/40 - Development Roadmap.md` (Milestone 2 ✅)
- Modify: `docs/06-Progress-Log/60 - Progress Log.md` (new entry + snapshot + freshen repo line)

- [ ] **Step 1: Re-run the success checks**

Run: `cargo test` (all green, pristine). Then observe detection live end-to-end:

```bash
cargo run -- printf '\033]9;hi\007'
```
Expected: `NOTIFICATION DETECTED` appears on stderr. Confirm before proceeding.

- [ ] **Step 2: Mark Milestone 2 done** in `docs/04-Development/42 - Milestone 2 - Detect the Signal.md`

Set frontmatter `status: done`, `**Status:: ✅ Done**`, and check the first three checklist boxes
(byte shape understood; scan for `ESC ]9`; print detection). Leave the "(Later) extract the payload"
box unchecked and add a one-line note that payload extraction, OSC 99/777, and ConEmu
disambiguation are deferred.

- [ ] **Step 3: Flip the roadmap** in `docs/04-Development/40 - Development Roadmap.md`

Change the Milestone 2 line from `⬜` to `✅`.

- [ ] **Step 4: Update the Progress Log** in `docs/06-Progress-Log/60 - Progress Log.md`

Add a new entry at the TOP of the Entries section using the `61 - Log Entry Template`
(Did / Learned / Blocked / Next task), with Next task = `[[43 - Milestone 3 - Fire a Notification]]`.
Update the "Current status snapshot": current milestone → Milestone 3; overall phase notes M2 done.
Also freshen the now-stale **Repo** line — the repo exists and is public and Milestone 1 merged via
PR #1; replace the "was 404 … create it" note with the current state.

- [ ] **Step 5: Commit**

```bash
git add "docs/04-Development/42 - Milestone 2 - Detect the Signal.md" "docs/04-Development/40 - Development Roadmap.md" "docs/06-Progress-Log/60 - Progress Log.md"
git commit -m "docs(m2): mark Milestone 2 done, log progress, refresh repo status"
```

---

## Self-Review (author's check against the spec)

**Spec coverage:**
- Stateful `OscDetector` module (spec §4) → Task 1. Cross-chunk robustness (§1, §4) → Task 1 split
  test. OSC 99 exclusion (§3) → Task 1 `does_not_detect_osc_99`. Wiring after passthrough (§4) →
  Task 2 Step 3. stderr signal (§4, §6) → Task 2. Unit + integration tests (§7) → Tasks 1 & 2.
  ConEmu limitation / deferred items (§2, §6) → left unbuilt by design; noted in Task 3. Every spec
  section maps to a task.

**Placeholder scan:** No TBD/TODO. The only `unimplemented!()` is a deliberate red-test step in
Task 1, replaced in the same task. All code steps carry complete code.

**Type consistency:** `OscDetector::new()` and `feed(&mut self, bytes: &[u8]) -> usize` are identical
in Task 1 (definition), the Task 1 tests, and Task 2's use site. `TARGET` is `[u8; 4]` throughout.
The pump edit references `buf`, `stdout`, `detector`, and `n`, all in scope at the insertion point.
