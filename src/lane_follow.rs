//! Following a file as it grows (M11ar: agy writes each subagent's steps to its own transcript):
//! the bytes added since the last read, cut into whole lines. Pure; the agent tab polls.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The most read at once (a big burst is read over several polls).
pub const MAX_READ: usize = 1024 * 1024;

/// Where a followed file was read to, and a line not yet ended.
#[derive(Default)]
pub struct Tail {
    pub offset: u64,
    partial: Vec<u8>,
}

impl Tail {
    /// `bytes` read at `offset`: the whole lines they end (without their newline); a line still
    /// being written waits for the rest.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        self.offset += bytes.len() as u64;
        self.partial.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(at) = self.partial.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=at).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1]);
            let line = line.trim_end_matches('\r');
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }
        lines
    }
}

/// At most `max` bytes of `path` from `from` on. A file not there yet reads nothing; a file
/// shorter than `from` (rewritten) is an error: its tail stops.
pub fn read_new(path: &Path, from: u64, max: usize) -> std::io::Result<Vec<u8>> {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let len = file.metadata()?.len();
    if len < from {
        return Err(std::io::Error::other("the file got shorter"));
    }
    file.seek(SeekFrom::Start(from))?;
    let mut out = Vec::new();
    file.take(max as u64).read_to_end(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct Gone(std::path::PathBuf);
    impl Drop for Gone {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn temp(name: &str) -> Gone {
        Gone(std::env::temp_dir().join(format!("fuxx-lane-follow-{}-{name}", std::process::id())))
    }

    fn append(path: &Path, s: &str) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    fn poll(tail: &mut Tail, path: &Path) -> Vec<String> {
        let bytes = read_new(path, tail.offset, MAX_READ).unwrap();
        tail.feed(&bytes)
    }

    #[test]
    fn new_lines_arrive_in_order() {
        let f = temp("order");
        let mut tail = Tail::default();
        append(&f.0, "one\ntwo\n");
        assert_eq!(poll(&mut tail, &f.0), ["one", "two"]);
        append(&f.0, "three\n");
        assert_eq!(poll(&mut tail, &f.0), ["three"], "only what is new");
        assert!(poll(&mut tail, &f.0).is_empty());
    }

    #[test]
    fn a_half_written_line_waits() {
        let f = temp("half");
        let mut tail = Tail::default();
        append(&f.0, "{\"a\":");
        assert!(poll(&mut tail, &f.0).is_empty(), "no newline yet");
        append(&f.0, "1}\n");
        assert_eq!(poll(&mut tail, &f.0), ["{\"a\":1}"]);
    }

    #[test]
    fn a_missing_file_reads_nothing_yet() {
        let f = temp("missing");
        assert_eq!(read_new(&f.0, 0, MAX_READ).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn a_shrunk_file_stops_the_tail() {
        let f = temp("shrunk");
        append(&f.0, "a long first line\n");
        let mut tail = Tail::default();
        poll(&mut tail, &f.0);
        std::fs::write(&f.0, "x\n").unwrap();
        assert!(read_new(&f.0, tail.offset, MAX_READ).is_err());
    }

    #[test]
    fn reads_are_capped() {
        let f = temp("capped");
        append(&f.0, &"x".repeat(100));
        assert_eq!(read_new(&f.0, 0, 40).unwrap().len(), 40);
        assert_eq!(
            read_new(&f.0, 40, 40).unwrap().len(),
            40,
            "then on from there"
        );
    }
}
