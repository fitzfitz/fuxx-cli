//! One live terminal: a PTY child, an `alacritty_terminal::Term` holding its screen, and a
//! reader thread feeding the child's output into the Term. No GPUI here: the view talks to it
//! through `TerminalCore` (cheap to clone, every field is an `Arc`) and an event channel.

use std::io::{ErrorKind, Read, Write};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Processor, Rgb, StdSyncHandler};
use futures::channel::mpsc::UnboundedSender;
use portable_pty::CommandBuilder;

use crate::colors;
use crate::detector::OscDetector;
use crate::grid::{self, GridSnapshot};
use crate::links::{Link, LinkFinder};
use crate::pty::Pty;

/// Terminal size in cells. Implements alacritty's `Dimensions` so `Term` can use it directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TermDims {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for TermDims {
    fn total_lines(&self) -> usize {
        self.rows // Term::new/resize only read screen_lines + columns
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Everything the UI needs to hear about, in order.
#[derive(Debug)]
pub enum TermEvent {
    /// Something the `Term` raised while parsing (query replies, title, bell, ...).
    Term(Event),
    /// New output was parsed into the screen: redraw. `notifications` = OSC 9 / 99 / 777
    /// notifications in it.
    Output { notifications: usize },
    /// The child closed the PTY (shell exited).
    Exited,
}

/// Receives `Term` events. It runs *inside* `Processor::advance` while the Term lock is held,
/// so it must never lock the Term: it only forwards into the channel.
#[derive(Clone)]
pub struct Listener(UnboundedSender<TermEvent>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.0.unbounded_send(TermEvent::Term(event));
    }
}

/// The parser lives next to the Term under one lock so a stuck synchronized update can be
/// flushed from outside the reader thread (see `flush_stale_sync`).
struct Shared {
    term: Term<Listener>,
    processor: Processor<StdSyncHandler>,
}

#[derive(Clone)]
pub struct TerminalCore {
    shared: Arc<FairMutex<Shared>>,
    pty: Arc<Mutex<Pty>>,
    dims: Arc<Mutex<TermDims>>,
    /// Bytes for the child, written by the writer thread so a child that stops reading can't
    /// block the UI.
    input: Sender<Vec<u8>>,
    /// Built once: compiling the URL regex is not free.
    links: Arc<Mutex<LinkFinder>>,
}

impl TerminalCore {
    pub fn spawn(
        cmd: CommandBuilder,
        dims: TermDims,
        events: UnboundedSender<TermEvent>,
    ) -> Result<Self, String> {
        let (pty, mut reader, mut writer) = Pty::spawn(cmd, dims)?;
        let term = Term::new(Config::default(), &dims, Listener(events.clone()));
        let shared = Arc::new(FairMutex::new(Shared {
            term,
            processor: Processor::new(),
        }));

        let reader_shared = shared.clone();
        std::thread::Builder::new()
            .name("fuxx-pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                // alacritty_terminal ignores notification OSCs, so a byte scanner watches the raw stream.
                let mut detector = OscDetector::new();
                loop {
                    match reader.read(&mut buf) {
                        // EOF. portable-pty already maps macOS's EIO-at-exit to Ok(0).
                        Ok(0) => break,
                        Ok(n) => {
                            let notifications = detector.feed(&buf[..n]);
                            {
                                let mut guard = reader_shared.lock();
                                let Shared { term, processor } = &mut *guard;
                                processor.advance(term, &buf[..n]);
                            } // lock released before waking the UI
                            let _ = events.unbounded_send(TermEvent::Output { notifications });
                        }
                        Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                let _ = events.unbounded_send(TermEvent::Exited);
            })
            .map_err(|e| e.to_string())?;

        // The writer thread owns the PTY writer for the whole session. It ends when every
        // `TerminalCore` clone is dropped (the channel closes) or the child goes away.
        let (input, queued) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new()
            .name("fuxx-pty-writer".into())
            .spawn(move || {
                for bytes in queued {
                    if writer
                        .write_all(&bytes)
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(Self {
            shared,
            pty: Arc::new(Mutex::new(pty)),
            dims: Arc::new(Mutex::new(dims)),
            input,
            links: Arc::new(Mutex::new(LinkFinder::new())),
        })
    }

    /// The allowed-scheme link under `point`, if any.
    pub fn link_at(&self, point: Point) -> Option<Link> {
        let term = &self.shared.lock().term;
        self.links.lock().unwrap().find(term, point)
    }

    /// Queue bytes for the child (keystrokes, query replies). Never blocks.
    pub fn write(&self, bytes: &[u8]) {
        let _ = self.input.send(bytes.to_vec());
    }

    /// User input: snap the view back to the live bottom, drop any selection, then send the
    /// bytes. Query replies use `write` instead so they never move the view.
    pub fn input(&self, bytes: &[u8]) {
        {
            let mut guard = self.shared.lock();
            guard.term.scroll_display(Scroll::Bottom);
            guard.term.selection = None;
        }
        self.write(bytes);
    }

    /// Begin a selection of kind `ty` anchored at `point` (replaces any existing one).
    pub fn start_selection(&self, ty: SelectionType, point: Point, side: Side) {
        self.shared.lock().term.selection = Some(Selection::new(ty, point, side));
    }

    /// Move the selection's moving end; no-op when there is no selection.
    pub fn extend_selection(&self, point: Point, side: Side) {
        if let Some(sel) = self.shared.lock().term.selection.as_mut() {
            sel.update(point, side);
        }
    }

    pub fn clear_selection(&self) {
        self.shared.lock().term.selection = None;
    }

    /// The selected text, or `None` when nothing is selected.
    pub fn selection_text(&self) -> Option<String> {
        self.shared.lock().term.selection_to_string()
    }

    /// Scroll the view through history (positive `Delta` = up).
    pub fn scroll(&self, scroll: Scroll) {
        self.shared.lock().term.scroll_display(scroll);
    }

    /// Lines the view is scrolled up from the live bottom (0 = live).
    pub fn display_offset(&self) -> usize {
        self.shared.lock().term.grid().display_offset()
    }

    /// The last `n` non-blank lines of the live screen, for the status heuristics.
    pub fn tail_text(&self, n: usize) -> String {
        grid::screen_tail_text(&self.shared.lock().term, n)
    }

    /// Resize screen + PTY. No-op when unchanged, so it's cheap to call every frame.
    pub fn resize(&self, dims: TermDims) {
        if dims.cols == 0 || dims.rows == 0 {
            return;
        }
        let mut current = self.dims.lock().unwrap();
        if *current == dims {
            return;
        }
        *current = dims;
        self.shared.lock().term.resize(dims);
        if let Ok(pty) = self.pty.lock() {
            let _ = pty.resize(dims);
        }
    }

    pub fn dims(&self) -> TermDims {
        *self.dims.lock().unwrap()
    }

    pub fn mode(&self) -> TermMode {
        *self.shared.lock().term.mode()
    }

    pub fn snapshot(&self) -> GridSnapshot {
        grid::snapshot(&self.shared.lock().term)
    }

    /// Color for an OSC 4/10/11/12 query reply.
    pub fn color(&self, index: usize) -> Rgb {
        colors::resolve_index(index, self.shared.lock().term.colors())
    }

    /// Synchronized updates (DEC mode 2026, used by Claude Code and most TUIs) buffer output
    /// until an end marker. If the marker never comes, the parser holds the frame until its
    /// deadline (~150 ms) *and* the next byte arrives. Called on a timer to flush it on time.
    /// Returns true if something was flushed (redraw needed).
    pub fn flush_stale_sync(&self) -> bool {
        let mut guard = self.shared.lock();
        let Shared { term, processor } = &mut *guard;
        match processor.sync_timeout().sync_timeout() {
            Some(deadline) if Instant::now() >= deadline => {
                processor.stop_sync(term);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::{Column, Line, Point, Side};
    use alacritty_terminal::selection::SelectionType;
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};
    use std::time::Duration;

    /// Poll the channel until `pred` matches or 5 s pass.
    fn wait_for(
        rx: &mut UnboundedReceiver<TermEvent>,
        mut pred: impl FnMut(&TermEvent) -> bool,
    ) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(ev) if pred(&ev) => return true,
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        false
    }

    fn sh(script: &str) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", script]);
        cmd
    }

    fn screen_text(core: &TerminalCore) -> String {
        core.snapshot()
            .lines
            .iter()
            .map(|l| l.iter().map(|c| c.ch).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn scrolling_moves_the_view_and_input_snaps_back() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("for i in $(seq 1 40); do echo line$i; done; sleep 2"),
            TermDims { cols: 20, rows: 5 },
            tx,
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("line40")));
        core.scroll(Scroll::Delta(5));
        assert_eq!(core.display_offset(), 5);
        core.write(b"x"); // a query reply must not move the view
        assert_eq!(core.display_offset(), 5);
        core.input(b"x"); // typing does
        assert_eq!(core.display_offset(), 0);
    }

    #[test]
    fn child_output_reaches_the_screen_and_exit_is_reported() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf 'hello fuxx'"),
            TermDims { cols: 40, rows: 5 },
            tx,
        )
        .unwrap();
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Exited)));
        assert!(screen_text(&core).contains("hello fuxx"));
    }

    #[test]
    fn cursor_position_query_produces_a_pty_write_reply() {
        let (tx, mut rx) = unbounded();
        let _core = TerminalCore::spawn(
            sh("printf '\\033[6n'; sleep 1"),
            TermDims { cols: 40, rows: 5 },
            tx,
        )
        .unwrap();
        assert!(wait_for(
            &mut rx,
            |e| matches!(e, TermEvent::Term(Event::PtyWrite(s)) if s == "\x1b[1;1R")
        ));
    }

    #[test]
    fn written_bytes_reach_the_child() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("read line; printf \"got:$line\""),
            TermDims { cols: 40, rows: 5 },
            tx,
        )
        .unwrap();
        core.write(b"ping\r");
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Exited)));
        assert!(screen_text(&core).contains("got:ping"));
    }

    #[test]
    fn resize_changes_the_grid_and_ignores_zero() {
        let (tx, _rx) = unbounded();
        let core = TerminalCore::spawn(sh("sleep 1"), TermDims { cols: 40, rows: 5 }, tx).unwrap();
        core.resize(TermDims { cols: 0, rows: 0 });
        assert_eq!(core.dims(), TermDims { cols: 40, rows: 5 });
        core.resize(TermDims { cols: 60, rows: 10 });
        let snap = core.snapshot();
        assert_eq!((snap.cols, snap.rows), (60, 10));
    }

    #[test]
    fn app_cursor_mode_is_visible_to_the_ui() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf '\\033[?1h'; sleep 1"),
            TermDims { cols: 40, rows: 5 },
            tx,
        )
        .unwrap();
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Output { .. })));
        assert!(core.mode().contains(TermMode::APP_CURSOR));
    }

    #[test]
    fn notifications_are_counted() {
        let (tx, mut rx) = unbounded();
        let _core = TerminalCore::spawn(
            sh("printf '\\033]777;notify;fuxx;done\\007'; sleep 1"),
            TermDims { cols: 40, rows: 5 },
            tx,
        )
        .unwrap();
        assert!(wait_for(&mut rx, |e| matches!(
            e,
            TermEvent::Output { notifications: 1 }
        )));
    }

    #[test]
    fn write_never_blocks_when_the_child_stops_reading() {
        let (tx, _rx) = unbounded();
        // Raw mode + not reading stdin: the tty's input queue fills and a blocking write would hang.
        let core = TerminalCore::spawn(sh("stty raw; sleep 5"), TermDims { cols: 40, rows: 5 }, tx)
            .unwrap();
        std::thread::sleep(Duration::from_millis(300)); // let `stty raw` run
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for _ in 0..64 {
                core.write(&[b'x'; 1024]);
            }
            let _ = done_tx.send(());
        });
        assert!(
            done_rx.recv_timeout(Duration::from_secs(2)).is_ok(),
            "write blocked"
        );
    }

    #[test]
    fn selection_text_word_and_input_clears_it() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf 'hello world'; sleep 2"),
            TermDims { cols: 20, rows: 3 },
            tx,
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("world")));
        core.start_selection(
            SelectionType::Simple,
            Point::new(Line(0), Column(0)),
            Side::Left,
        );
        core.extend_selection(Point::new(Line(0), Column(4)), Side::Right);
        assert_eq!(core.selection_text().as_deref(), Some("hello"));
        core.start_selection(
            SelectionType::Semantic,
            Point::new(Line(0), Column(7)),
            Side::Left,
        );
        assert_eq!(core.selection_text().as_deref(), Some("world"));
        core.input(b"x");
        assert_eq!(core.selection_text(), None);
    }
}
