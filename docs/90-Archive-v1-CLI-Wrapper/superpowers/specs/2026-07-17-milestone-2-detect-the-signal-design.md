# Milestone 2 — Detect the Signal (Design Spec)

**Date:** 2026-07-17
**Milestone:** `docs/04-Development/42 - Milestone 2 - Detect the Signal.md`
**Builds on:** Milestone 1 (the PTY output pump in `src/main.rs`).
**Status:** design approved; awaiting spec review before implementation planning.

## 1. Goal

Spot an **OSC 9 notification sequence** in the child's output stream as it flows through the
Milestone 1 pump, and log a detection. This is the first half of the tool's reason to exist
(Milestone 3 turns a detection into a real desktop notification).

## 2. Scope

**In scope (M2):**
- Detect the **OSC 9 intro** `ESC ] 9 ;` in the byte stream, robust to the sequence being split
  across `read()` chunks.
- On detection, emit `NOTIFICATION DETECTED` to **stderr**.
- Leave the Milestone 1 passthrough behaviour byte-for-byte unchanged.

**Out of scope (deferred):**
- Parsing/extracting the notification payload text or handling the terminator (BEL / ST). M2
  detects the intro only — matches the note's "don't parse the payload yet."
- OSC 99 and OSC 777 variants.
- Disambiguating ConEmu's OSC 9 sub-protocols (see §6, Known limitation).
- Firing an actual OS notification — that is Milestone 3.
- Any terminal emulation/rendering. This is pure output-stream observation; the wrapper/observer
  boundary (`docs/01-Overview/12 - Scope Boundaries.md`) is unchanged.

## 3. The sequence we match (verified)

OSC 9 desktop-notification format (confirmed against terminal docs, 2026-07-17):

```
ESC ] 9 ; <message> BEL
\x1b ] 9 ; ...        \x07
```

M2 matches only the 4-byte **intro**: `ESC ] 9 ;` = `1b 5d 39 3b`.

Matching through the `;` is deliberate — it correctly **excludes OSC 99** (`ESC ] 9 9 ;`): when the
matcher is at "seen `ESC ] 9`, expecting `;`" and instead sees `9`, the match breaks and resets.

## 4. Architecture

One new file, one small edit.

### `src/detector.rs` (new) — the stateful scanner

```rust
pub struct OscDetector { /* how many leading bytes of the target matched so far */ }

impl OscDetector {
    pub fn new() -> Self;
    /// Feed the next chunk of output bytes. Returns the number of complete OSC 9 intros
    /// (`ESC ] 9 ;`) newly recognised in this chunk. Partial-match state carries across calls,
    /// so a sequence split across two feeds is still detected.
    pub fn feed(&mut self, bytes: &[u8]) -> usize;
}
```

Implementation: a tiny state machine over the target `[0x1b, 0x5d, 0x39, 0x3b]`. It holds a
`matched: usize` (0..4). For each incoming byte:
- if it equals `TARGET[matched]`, advance `matched`; if `matched` reaches 4, count one detection
  and reset `matched` to 0;
- otherwise reset `matched` to 0 — but if the breaking byte is itself `ESC` (`0x1b`), set
  `matched = 1` (so `ESC ESC ] 9 ;` and similar restarts are handled, not swallowed).

Pure, no I/O — trivially unit-testable, including the split-across-chunks case.

### `src/main.rs` (edit) — wire into the pump

- Add `mod detector;`.
- Create `let mut detector = OscDetector::new();` before the output loop.
- Inside the existing `Ok(n)` branch, **after** `stdout.write_all(&buf[..n])?` / `flush()?`
  (passthrough stays first), add:
  ```rust
  if detector.feed(&buf[..n]) > 0 {
      eprintln!("NOTIFICATION DETECTED");
  }
  ```
Nothing else in `run()` changes.

## 5. Data flow

```
child output ─▶ reader.read() ─▶ buf[..n]
                                   ├─▶ stdout.write_all(buf[..n])   (passthrough — unchanged)
                                   └─▶ detector.feed(buf[..n]) ──▶ >0 ? ──▶ eprintln!("NOTIFICATION DETECTED")
```

The detector observes a copy of the same bytes; it never alters or delays passthrough.

## 6. Notes & known limitation

- **Signal goes to stderr, not stdout.** stdout is the child's rendered output; writing to it would
  corrupt the display. stderr still reaches the terminal and can disrupt a full-screen TUI mid-draw,
  but M2 only needs to *prove* detection and M3 replaces this line with a real (non-drawing) OS
  notification. Acceptable throwaway.
- **Known limitation — ConEmu OSC 9 overload.** ConEmu uses `ESC ] 9 ; <digit> ; …` for other
  protocols (e.g. `9;4;` progress bars). The `ESC ] 9 ;` intro match will also fire on those. This
  is acceptable for M2's scope; refining it (e.g. rejecting `9 ; <digit> ;`) is deferred. Recorded so
  it isn't a surprise later.

## 7. Testing

**Unit (`src/detector.rs`, pure — the point of the module):**
- Detects a whole `ESC]9;` intro in a single `feed`.
- **Detects a split intro:** `feed(b"...\x1b")` then `feed(b"]9;...")` → one detection total.
- Does **not** detect OSC 99 (`\x1b]99;`).
- Ignores plain text (`b"hello world\n"`) → 0.
- Two intros in one chunk → 2.

**Integration (`tests/`):**
- Feed a real OSC 9 sequence through fuxx and assert stderr contains `NOTIFICATION DETECTED`, e.g.
  running `fuxx` on a child that emits `printf '\e]9;done\a'`.
- A run with no sequence → stderr does **not** contain it.

## 8. What you'll learn (Rust)

Byte-slice pattern matching (`&[u8]`), a hand-written state machine, why escape sequences look the
way they do, and how carrying state across buffer boundaries differs from a naive per-chunk scan.
See `docs/02-Architecture/25 - OSC Sequence Detection.md`.
