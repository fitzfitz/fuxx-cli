//! Where a workspace is (M11b): its first terminal's working folder and the git branch checked
//! out there, for the sidebar row's second line (`branch • ~/folder`).
//!
//! The folder: what the shell last reported (OSC 7, `detector::CwdReports`), else the shell's
//! own working folder (`process_cwd`, from `proc_pidinfo`), else the folder it started in. The
//! branch: `git rev-parse --abbrev-ref HEAD` there (a detached HEAD: its short sha), run off the
//! UI thread with a 2 s cap and cached per folder (`Places`). Both are looked up again at most
//! every 5 s, and only for rows on screen (`due`); outside a repo, or with no git, there is no
//! branch and nothing is said. Since M11i the terminal the status bar follows is looked up too
//! (its folder and branch are on the bar): `place_subjects`.
//!
//! `parse_head`, `short_sha`, `due`, `git_program`, `subjects` and `Places` are pure and tested;
//! `branch_at` runs git and `process_cwd` asks the kernel. The Workspace's `refresh_places` (on
//! each frame, rate-limited by `due`) and `tick_places` (every 5 s while a place can be seen,
//! so nothing wakes a hidden or covered window) do the lookups.

use std::collections::HashMap;
use std::ffi::{OsStr, c_int, c_void};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gpui::{App, Context};

use crate::reduce_motion::WindowVisible;
use crate::sidebar::SidebarRow;
use crate::workspace::Workspace;

/// How often a row's folder and branch are looked up again, at most.
pub const REFRESH: Duration = Duration::from_secs(5);
/// How long git may take before it is killed (and the branch left unknown).
pub const GIT_CAP: Duration = Duration::from_secs(2);

/// How often a frame may check the rows on screen for a lookup that's due (`Places::check_due`).
pub const CHECK: Duration = Duration::from_secs(1);

/// Whether a folder report naming `host` is about this Mac (named `hostname`): no host,
/// `localhost`, or this Mac's name, whatever the case and with or without `.local`.
pub fn is_this_mac(host: &str, hostname: Option<&str>) -> bool {
    let name = |h: &str| {
        let h = h.trim_end_matches('.').to_ascii_lowercase();
        h.strip_suffix(".local").map(str::to_string).unwrap_or(h)
    };
    let host = name(host);
    host.is_empty() || host == "localhost" || hostname.is_some_and(|me| name(me) == host)
}

unsafe extern "C" {
    fn gethostname(name: *mut std::ffi::c_char, len: usize) -> c_int;
}

/// This Mac's name, as the shells see it.
pub fn this_hostname() -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: the buffer is as long as said; the name comes back NUL-terminated (or cut).
    if unsafe { gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let name = String::from_utf8_lossy(&buf[..len]).into_owned();
    (!name.is_empty()).then_some(name)
}

/// What `git rev-parse --abbrev-ref HEAD` printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    /// `HEAD` itself: no branch is checked out.
    Detached,
}

/// `rev-parse --abbrev-ref HEAD`'s output read: a branch, `HEAD` (detached), or none (empty).
pub fn parse_head(stdout: &str) -> Option<Head> {
    match stdout.trim() {
        "" => None,
        "HEAD" => Some(Head::Detached),
        branch => Some(Head::Branch(branch.to_string())),
    }
}

/// `rev-parse --short HEAD`'s output read: the short sha, or none (empty).
pub fn short_sha(stdout: &str) -> Option<String> {
    let sha = stdout.trim();
    (!sha.is_empty()).then(|| sha.to_string())
}

/// Whether something last looked up at `last` (none: never) is looked up again `now`: only for
/// a row on screen (`visible`), and only once `REFRESH` has passed.
pub fn due(last: Option<Instant>, now: Instant, visible: bool) -> bool {
    visible && last.is_none_or(|at| now.saturating_duration_since(at) >= REFRESH)
}

/// The git to run: the first `git` on `path` (fuxx's PATH), then Homebrew's. macOS's
/// `/usr/bin/git` is a stub that offers to install the developer tools when they aren't there,
/// so it counts only when `developer_git` (their git) exists. None: no git on this Mac.
pub fn git_program(
    path: Option<&OsStr>,
    developer_git: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    const STUB: &str = "/usr/bin/git";
    let on_path = path.into_iter().flat_map(std::env::split_paths);
    let brew = ["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from);
    on_path
        .chain(brew)
        .map(|dir| dir.join("git"))
        .filter(|git| git != Path::new(STUB) || exists(developer_git))
        .find(|git| exists(git))
}

/// The developer tools' own git, which `/usr/bin/git` runs: the folder `xcode-select` chose,
/// else the Command Line Tools'.
fn developer_git() -> PathBuf {
    let dir = std::fs::read_link("/var/db/xcode_select_link")
        .unwrap_or_else(|_| PathBuf::from("/Library/Developer/CommandLineTools"));
    dir.join("usr/bin/git")
}

/// This Mac's git, or none (see `git_program`).
pub fn find_git() -> Option<PathBuf> {
    let path = std::env::var_os("PATH");
    git_program(path.as_deref(), &developer_git(), |p| p.is_file())
}

/// The branch checked out in `dir`: a detached HEAD gives its short sha; none outside a repo,
/// when git fails, or when it takes longer than `GIT_CAP`.
pub fn branch_at(git: &Path, dir: &Path) -> Option<String> {
    let head = run_git(git, dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    match parse_head(&head)? {
        Head::Branch(branch) => Some(branch),
        Head::Detached => short_sha(&run_git(git, dir, &["rev-parse", "--short", "HEAD"])?),
    }
}

/// `git -C dir args…`'s output, if it succeeded within `GIT_CAP`. git never prompts (no
/// terminal, `GIT_TERMINAL_PROMPT=0`) and never takes the index lock (`GIT_OPTIONAL_LOCKS=0`).
pub(crate) fn run_git(git: &Path, dir: &Path, args: &[&str]) -> Option<String> {
    run_git_outcome(git, dir, args).success()
}

/// `run_git`, telling what git said (its exit code and output) from a failure or the cap.
pub(crate) fn run_git_outcome(git: &Path, dir: &Path, args: &[&str]) -> Ran {
    run_capped_outcome(git_command(git, dir, args), None, GIT_CAP)
}

/// `run_git_outcome`, with what git printed as bytes, for output that may not be UTF-8 (a
/// diff of a Latin-1 file); the caller decodes it. Paths from porcelain/numstat stay on the
/// strict `run_git`.
pub(crate) fn run_git_bytes(git: &Path, dir: &Path, args: &[&str]) -> Ran<Vec<u8>> {
    run_capped_bytes(git_command(git, dir, args), None, GIT_CAP)
}

/// `run_git_outcome` with `input` on git's stdin (`check-ignore --stdin`). Any size: it is
/// written on a thread, within the cap.
pub(crate) fn run_git_stdin_outcome(git: &Path, dir: &Path, args: &[&str], input: &[u8]) -> Ran {
    run_capped_outcome(git_command(git, dir, args), Some(input), GIT_CAP)
}

fn git_command(git: &Path, dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(git);
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    command
}

unsafe extern "C" {
    fn setsid() -> c_int;
    fn kill(pid: c_int, sig: c_int) -> c_int;
}

/// `SIGKILL` from `<sys/signal.h>`.
const SIGKILL: c_int = 9;

/// What a capped command did; what it printed as text (`String`), or as bytes
/// (`run_capped_bytes`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ran<Out = String> {
    /// It ran to its end: its exit code (none: ended by a signal) and what it printed.
    Exited(Option<i32>, Out),
    /// It was still running at the cap, and was killed (with its whole process group).
    TimedOut,
    /// It couldn't be started, or what it printed couldn't be read.
    Failed,
}

impl<Out> Ran<Out> {
    /// What it printed, if it exited successfully (0).
    pub(crate) fn success(self) -> Option<Out> {
        match self {
            Ran::Exited(Some(0), out) => Some(out),
            _ => None,
        }
    }
}

/// Run `command` with no terminal (its own session, nothing on stdin, stderr dropped) and its
/// stdout, if it exits successfully within `cap`; past that it is killed. (The tests' short
/// form of `run_capped_outcome`.)
#[cfg(test)]
pub(crate) fn run_capped(command: Command, cap: Duration) -> Option<String> {
    run_capped_outcome(command, None, cap).success()
}

/// `run_capped`, with `input` (if any) written to the command's stdin (then closed), telling
/// what it said (exit code and stdout) from a failure or the cap. The command leads its own
/// session and process group (`setsid`), so at the cap the whole group is killed: children it
/// started (a hook, an fsmonitor) holding its pipes end too, and so do the pipes' threads.
pub(crate) fn run_capped_outcome(command: Command, input: Option<&[u8]>, cap: Duration) -> Ran {
    match run_capped_bytes(command, input, cap) {
        Ran::Exited(code, out) => {
            String::from_utf8(out).map_or(Ran::Failed, |out| Ran::Exited(code, out))
        }
        Ran::TimedOut => Ran::TimedOut,
        Ran::Failed => Ran::Failed,
    }
}

/// `run_capped_outcome`, with what the command printed as bytes (`ps`, whose output may not be
/// UTF-8 when a process's arguments aren't).
pub(crate) fn run_capped_bytes(
    command: Command,
    input: Option<&[u8]>,
    cap: Duration,
) -> Ran<Vec<u8>> {
    match run_capped_streams(command, input, cap, false) {
        Ran::Exited(code, (out, _)) => Ran::Exited(code, out),
        Ran::TimedOut => Ran::TimedOut,
        Ran::Failed => Ran::Failed,
    }
}

/// `run_capped_bytes`, optionally keeping stderr too (`capture_stderr`; else it is dropped and
/// the second stream is empty). The one kill loop every capped command goes through.
fn run_capped_streams(
    mut command: Command,
    input: Option<&[u8]>,
    cap: Duration,
    capture_stderr: bool,
) -> Ran<(Vec<u8>, Vec<u8>)> {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(if capture_stderr {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    // SAFETY: setsid is async-signal-safe; it only detaches the child from fuxx's session (and
    // so from any controlling terminal fuxx has), making it its own process group's leader.
    unsafe {
        command.pre_exec(|| {
            setsid();
            Ok(())
        });
    }
    let Ok(mut child) = command.spawn() else {
        return Ran::Failed;
    };
    // Both pipes are served by threads so a big input or output never stalls git (a full pipe
    // would hold it until the cap). They end by themselves: the writer when it is done or git's
    // end closes (a broken pipe, as when the cap kills git), the reader at the end of stdout.
    let writer = input.zip(child.stdin.take()).map(|(input, mut stdin)| {
        let input = input.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&input); // dropped after: git sees the end of its input
        })
    });
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Ran::Failed;
    };
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        stdout.read_to_end(&mut out).ok().map(|_| out)
    });
    // Served by its own thread like stdout, so neither pipe can fill and hold git.
    let err_reader = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            stderr.read_to_end(&mut out).ok().map(|_| out)
        })
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < cap => std::thread::sleep(Duration::from_millis(10)),
            waited => {
                // The group (led by the child: `setsid`), so its children go with it. Before
                // `wait`, while the child's pid can't have been reused.
                if let Ok(pid) = c_int::try_from(child.id()) {
                    // SAFETY: a plain signal to our own child's process group.
                    unsafe { kill(-pid, SIGKILL) };
                }
                let _ = child.kill();
                let _ = child.wait();
                return match waited {
                    Ok(None) => Ran::TimedOut,
                    _ => Ran::Failed,
                };
            }
        }
    };
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    let err = match err_reader {
        Some(reader) => reader.join().ok().flatten(),
        None => Some(Vec::new()),
    };
    match (reader.join().ok().flatten(), err) {
        (Some(out), Some(err)) => Ran::Exited(status.code(), (out, err)),
        _ => Ran::Failed,
    }
}

/// How long `switch` may take. It writes the working tree and the index, and a `git switch`
/// killed half way can leave a half-switched tree and a stale `index.lock`: so the cap is only
/// for a git that has truly hung, never for a big checkout on a slow disk (2 minutes).
pub const SWITCH_CAP: Duration = Duration::from_secs(120);
/// How long a `fetch` may take (the network).
pub const FETCH_CAP: Duration = Duration::from_secs(30);

/// `git -C dir args…` for a command that changes things (`switch`, `fetch`): like `run_git`
/// (own session, no terminal, never prompts) but without `GIT_OPTIONAL_LOCKS=0` (it writes),
/// with `env` added, and with git's words kept: `Ok(stdout)` on exit 0, else `Err` of the first
/// non-empty line of its stderr; `Err("git timed out")` at `cap` (git and its children are
/// killed); `Err("git could not run")` if it didn't start or its output couldn't be read.
pub(crate) fn run_git_checked(
    git: &Path,
    dir: &Path,
    args: &[&str],
    cap: Duration,
    env: &[(&str, &str)],
) -> Result<String, String> {
    let mut command = Command::new(git);
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        // English messages: callers match on git's words ("couldn't find remote ref").
        .env("LC_ALL", "C")
        .envs(env.iter().copied());
    match run_capped_streams(command, None, cap, true) {
        Ran::Exited(Some(0), (out, _)) => {
            String::from_utf8(out).map_err(|_| "git could not run".to_string())
        }
        Ran::Exited(_, (_, err)) => {
            let err = String::from_utf8_lossy(&err);
            let line = err.lines().map(str::trim).find(|l| !l.is_empty());
            Err(line.unwrap_or("git failed").to_string())
        }
        Ran::TimedOut => Err("git timed out".to_string()),
        Ran::Failed => Err("git could not run".to_string()),
    }
}

/// `PROC_PIDVNODEPATHINFO` from `<sys/proc_info.h>`: a process's current and root folders.
const PROC_PIDVNODEPATHINFO: c_int = 9;
/// `MAXPATHLEN`.
const MAXPATHLEN: usize = 1024;

/// `struct vinfo_stat` (136 bytes).
#[repr(C)]
struct VinfoStat {
    dev: u32,
    mode: u16,
    nlink: u16,
    ino: u64,
    uid: u32,
    gid: u32,
    times: [i64; 8],
    size: i64,
    blocks: i64,
    blksize: i32,
    flags: u32,
    generation: u32,
    rdev: u32,
    qspare: [i64; 2],
}

/// `struct vnode_info` (152 bytes).
#[repr(C)]
struct VnodeInfo {
    stat: VinfoStat,
    kind: c_int,
    pad: c_int,
    fsid: [i32; 2],
}

/// `struct vnode_info_path` (1176 bytes): the vnode and its path.
#[repr(C)]
struct VnodeInfoPath {
    vnode: VnodeInfo,
    path: [u8; MAXPATHLEN],
}

/// `struct proc_vnodepathinfo` (2352 bytes): the current folder, then the root.
#[repr(C)]
struct ProcVnodePathInfo {
    cdir: VnodeInfoPath,
    rdir: VnodeInfoPath,
}

unsafe extern "C" {
    /// From libproc (in libSystem).
    fn proc_pidinfo(
        pid: c_int,
        flavor: c_int,
        arg: u64,
        buffer: *mut c_void,
        buffersize: c_int,
    ) -> c_int;
}

/// Process `pid`'s working folder, as the kernel knows it; none when it's gone or not ours.
pub fn process_cwd(pid: u32) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let pid = c_int::try_from(pid).ok()?;
    let mut info = std::mem::MaybeUninit::<ProcVnodePathInfo>::zeroed();
    let size = std::mem::size_of::<ProcVnodePathInfo>();
    // SAFETY: the buffer is a zeroed `proc_vnodepathinfo` (layout checked against the SDK in
    // the tests) of the size passed; the kernel writes at most that many bytes.
    let wrote = unsafe {
        proc_pidinfo(
            pid,
            PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr().cast(),
            size as c_int,
        )
    };
    if usize::try_from(wrote).ok()? != size {
        return None;
    }
    // SAFETY: zeroed, then filled in full by the kernel: every field is plain data.
    let info = unsafe { info.assume_init() };
    let path = &info.cdir.path;
    let len = path.iter().position(|&b| b == 0).unwrap_or(path.len());
    (len > 0).then(|| PathBuf::from(OsStr::from_bytes(&path[..len])))
}

/// Where the workspaces are as the sidebar shows them, and the lookups behind it. Kept by the
/// Workspace on the UI thread; git runs elsewhere and reports back (`set_branch`).
#[derive(Default)]
pub struct Places {
    /// Each terminal's working folder (by `sN`; none: not found) as last looked up, and when.
    folders: HashMap<u64, (Option<PathBuf>, Instant)>,
    /// Each folder's branch (none: not in a repo) and when it was last asked for.
    branches: HashMap<PathBuf, (Option<String>, Instant)>,
    /// git, once looked for (inner none: not on this Mac).
    git: Option<Option<PathBuf>>,
    /// Nothing is looked up (the UI screenshots set what they show).
    pub frozen: bool,
    /// Your home folder, which rows show as `~`.
    pub home: Option<PathBuf>,
    /// This Mac's name (`gethostname`): a local terminal's folder report counts only when it's
    /// about this Mac (`is_this_mac`).
    pub hostname: Option<String>,
    /// When the rows are next checked from a frame (`check_due`).
    next_check: Option<Instant>,
}

impl Places {
    /// With `home` shown as `~`.
    pub fn new(home: Option<PathBuf>, hostname: Option<String>) -> Self {
        Self {
            home,
            hostname,
            ..Self::default()
        }
    }

    /// Whether a frame checks the rows `now` (at most once every `CHECK`): each check builds
    /// the rows, so a frame between checks doesn't.
    pub fn check_due(&mut self, now: Instant) -> bool {
        if self.next_check.is_some_and(|next| now < next) {
            return false;
        }
        self.next_check = Some(now + CHECK);
        true
    }

    /// Terminal `id`'s working folder as last found.
    pub fn folder(&self, id: u64) -> Option<&Path> {
        self.folders.get(&id)?.0.as_deref()
    }

    /// The branch checked out in `folder`, as last found.
    pub fn branch(&self, folder: &Path) -> Option<&str> {
        self.branches.get(folder)?.0.as_deref()
    }

    /// Whether terminal `id`'s folder is looked up `now` (its row is on screen).
    pub fn folder_due(&self, id: u64, now: Instant) -> bool {
        !self.frozen && due(self.folders.get(&id).map(|(_, at)| *at), now, true)
    }

    /// Terminal `id` reported a new folder: look it up at the next refresh, not 5 s on
    /// (2026-11-11). Its branch is asked for then too, once per folder (`ask_branch`).
    pub fn folder_moved(&mut self, id: u64) {
        self.folders.remove(&id);
    }

    pub fn set_folder(&mut self, id: u64, folder: Option<PathBuf>, now: Instant) {
        self.folders.insert(id, (folder, now));
    }

    /// If `folder`'s branch is due `now`, note it as asked (the last answer stays until the new
    /// one comes) and give the git to ask with; none when it isn't due or there is no git.
    pub fn ask_branch(&mut self, folder: &Path, now: Instant) -> Option<PathBuf> {
        let last = self.branches.get(folder).map(|(_, at)| *at);
        if self.frozen || !due(last, now, true) {
            return None;
        }
        let git = self.git()?;
        let branch = self.branches.remove(folder).and_then(|(b, _)| b);
        self.branches.insert(folder.to_path_buf(), (branch, now));
        Some(git)
    }

    /// This Mac's git (looked for once), or none.
    pub fn git(&mut self) -> Option<PathBuf> {
        self.git.get_or_insert_with(find_git).clone()
    }

    /// git's answer for `folder`; true when the branch changed.
    pub fn set_branch(&mut self, folder: &Path, branch: Option<String>) -> bool {
        match self.branches.get_mut(folder) {
            Some((old, _)) if *old != branch => {
                *old = branch;
                true
            }
            Some(_) => false,
            None => {
                self.branches
                    .insert(folder.to_path_buf(), (branch, Instant::now()));
                true
            }
        }
    }

    /// Forget terminals that are gone (by their ids still open), and the branches of folders
    /// no terminal is in any more.
    pub fn retain(&mut self, open: impl Fn(u64) -> bool) {
        self.folders.retain(|id, _| open(*id));
        let held: Vec<&PathBuf> = self
            .folders
            .values()
            .filter_map(|(f, _)| f.as_ref())
            .collect();
        self.branches.retain(|folder, _| held.contains(&folder));
    }
}

/// The terminals (by id) whose facts are on screen, each once, in order: one per row in view
/// (`rows`; none for a row without one), then `followed` (the status bar's).
pub fn subjects(rows: impl IntoIterator<Item = Option<u64>>, followed: Option<u64>) -> Vec<u64> {
    let mut ids: Vec<u64> = Vec::new();
    for id in rows.into_iter().chain([followed]).flatten() {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

impl Workspace {
    /// The sessions (by place in the list) whose places are on screen, each once (`subjects`):
    /// each row in view's first terminal (its second line's place; none with the sidebar
    /// hidden), then the terminal the status bar follows.
    fn place_subjects(&self) -> Vec<usize> {
        let rows = self.rows_in_view(Instant::now());
        let firsts = rows.iter().map(|row| match row {
            SidebarRow::Workspace(w) => self.model.terminals(w.workspace).first().copied(),
            SidebarRow::Header(_) => None,
        });
        subjects(firsts, self.right_sidebar.followed)
            .into_iter()
            .filter_map(|id| self.index_of(id))
            .collect()
    }

    /// Whether rows' places can be looked up: the window can be seen (not hidden, minimised or
    /// covered: the signal the marks' ticker reads) and they aren't frozen.
    fn places_seen(&self, cx: &App) -> bool {
        let seen = cx.try_global::<WindowVisible>().is_none_or(|w| w.visible);
        seen && !self.places.frozen
    }

    /// Whether the places' refresh should run: they can be seen and a place is on screen.
    fn places_wanted(&self, cx: &App) -> bool {
        self.places_seen(cx) && !self.place_subjects().is_empty()
    }

    /// Look up again, where due (`due`), the place of each terminal whose place is on screen
    /// (`place_subjects`): its folder at once (what it reported, else the kernel's
    /// answer for a local one, else the folder it started in; an SSH one's only as reported),
    /// and the branch there from git, off the UI thread. True when a folder changed (git's
    /// answer redraws when it comes).
    pub(crate) fn refresh_places(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.places_seen(cx) {
            return false;
        }
        let now = Instant::now();
        let mut changed = false;
        for i in self.place_subjects() {
            let session = &self.sessions.items()[i];
            let (id, ssh) = (session.id, session.ssh.is_some());
            if self.places.folder_due(id, now) {
                let (reported, pid) = session.view.read(cx).working_folder();
                // A local terminal's report counts only when it's about this Mac (not a shell
                // reached over ssh from it): else the kernel's answer.
                let hostname = self.places.hostname.as_deref();
                let reported = reported
                    .filter(|r| ssh || is_this_mac(&r.host, hostname))
                    .map(|r| r.path);
                let folder = if ssh {
                    reported
                } else {
                    reported
                        .or_else(|| pid.and_then(process_cwd))
                        .or_else(|| session.project.clone())
                };
                changed |= self.places.folder(id) != folder.as_deref();
                self.places.set_folder(id, folder, now);
            }
            // No git over SSH: the folder is on another machine.
            let Some(folder) = self
                .places
                .folder(id)
                .filter(|_| !ssh)
                .map(Path::to_path_buf)
            else {
                continue;
            };
            let Some(git) = self.places.ask_branch(&folder, now) else {
                continue;
            };
            let dir = folder.clone();
            cx.spawn(async move |this, cx| {
                let branch = cx
                    .background_executor()
                    .spawn(async move { branch_at(&git, &dir) })
                    .await;
                this.update(cx, |this, cx| {
                    if this.places.set_branch(&folder, branch) {
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
        changed
    }

    /// Start the places' 5 s refresh unless it's running or nothing is wanted (`places_wanted`);
    /// it stops once nothing is, so a hidden or covered window is never woken. `render` calls
    /// it, so showing the window or the sidebar again starts it.
    pub(crate) fn tick_places(&mut self, cx: &mut Context<Self>) {
        if self.places_ticking || !self.places_wanted(cx) {
            return;
        }
        self.places_ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH).await;
                let wanted = this
                    .update(cx, |this, cx| {
                        this.places_ticking = this.places_wanted(cx);
                        if this.places_ticking && this.refresh_places(cx) {
                            cx.notify();
                        }
                        this.places_ticking
                    })
                    .unwrap_or(false);
                if !wanted {
                    break;
                }
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subjects_are_the_rows_terminals_then_the_followed_one_each_once() {
        // Three rows (the second has no terminal, the third repeats the first's), then the
        // terminal the status bar follows.
        assert_eq!(
            subjects([Some(1), None, Some(2), Some(1)], Some(3)),
            [1, 2, 3]
        );
        // The followed one is a row's: once.
        assert_eq!(subjects([Some(1), Some(2)], Some(2)), [1, 2]);
        // No rows (the sidebar hidden): only the followed one.
        assert_eq!(subjects([], Some(4)), [4]);
        assert!(subjects([None], None).is_empty());
    }

    #[test]
    fn head_reads_as_a_branch_or_detached() {
        assert_eq!(parse_head("main\n"), Some(Head::Branch("main".into())));
        assert_eq!(
            parse_head("feature/rows\n"),
            Some(Head::Branch("feature/rows".into()))
        );
        assert_eq!(parse_head("HEAD\n"), Some(Head::Detached));
        assert_eq!(parse_head(""), None);
        assert_eq!(parse_head("  \n"), None);
        assert_eq!(short_sha("1a2b3c4\n"), Some("1a2b3c4".into()));
        assert_eq!(short_sha("\n"), None);
    }

    #[test]
    fn a_row_is_looked_up_again_only_when_on_screen_and_5_s_on() {
        let now = Instant::now();
        let ago = |ms: u64| Some(now - Duration::from_millis(ms));
        // Never looked up: now, if it shows.
        assert!(due(None, now, true));
        assert!(!due(None, now, false));
        // Not before 5 s.
        assert!(!due(ago(0), now, true));
        assert!(!due(ago(4_999), now, true));
        assert!(due(ago(5_000), now, true));
        assert!(due(ago(60_000), now, true));
        // Off screen (sidebar hidden, window covered, scrolled away): never.
        assert!(!due(ago(60_000), now, false));
    }

    #[test]
    fn git_is_the_first_on_the_path_and_the_stub_only_with_the_tools() {
        let dev = Path::new("/Library/Developer/CommandLineTools/usr/bin/git");
        let has = |files: &'static [&'static str]| {
            move |p: &Path| files.iter().any(|f| p == Path::new(f))
        };
        let path = OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin");
        // The stub alone, no developer tools: no git (it would offer to install them).
        assert_eq!(git_program(Some(path), dev, has(&["/usr/bin/git"])), None);
        // With them, the stub runs their git.
        let with_tools = has(&[
            "/usr/bin/git",
            "/Library/Developer/CommandLineTools/usr/bin/git",
        ]);
        assert_eq!(
            git_program(Some(path), dev, with_tools),
            Some(PathBuf::from("/usr/bin/git"))
        );
        // Homebrew's, even off fuxx's PATH (an app opened from the Finder has a short one).
        let brew = has(&["/usr/bin/git", "/opt/homebrew/bin/git"]);
        assert_eq!(
            git_program(Some(path), dev, brew),
            Some(PathBuf::from("/opt/homebrew/bin/git"))
        );
        // The PATH's own order first.
        let path = OsStr::new("/Users/me/bin:/opt/homebrew/bin:/usr/bin");
        let both = has(&["/Users/me/bin/git", "/opt/homebrew/bin/git"]);
        assert_eq!(
            git_program(Some(path), dev, both),
            Some(PathBuf::from("/Users/me/bin/git"))
        );
        // No PATH at all, nothing installed.
        assert_eq!(git_program(None, dev, has(&[])), None);
    }

    #[test]
    fn the_layouts_match_the_sdk() {
        // Sizes from <sys/proc_info.h>, checked with the SDK's compiler.
        assert_eq!(std::mem::size_of::<VinfoStat>(), 136);
        assert_eq!(std::mem::size_of::<VnodeInfo>(), 152);
        assert_eq!(std::mem::size_of::<VnodeInfoPath>(), 1176);
        assert_eq!(std::mem::offset_of!(VnodeInfoPath, path), 152);
        assert_eq!(std::mem::size_of::<ProcVnodePathInfo>(), 2352);
    }

    #[test]
    fn a_process_working_folder_comes_from_the_kernel() {
        let here = std::env::current_dir().unwrap();
        let found = process_cwd(std::process::id()).expect("our own folder");
        assert_eq!(found.canonicalize().unwrap(), here.canonicalize().unwrap());
        // A process that isn't there.
        assert_eq!(process_cwd(u32::MAX >> 1), None);
    }

    #[test]
    fn a_slow_command_is_killed_at_its_cap() {
        let started = Instant::now();
        let mut sleep = Command::new("/bin/sleep");
        sleep.arg("5");
        assert_eq!(run_capped(sleep, Duration::from_millis(100)), None);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "killed, not waited for"
        );
        let mut echo = Command::new("/bin/echo");
        echo.arg("hi");
        assert_eq!(run_capped(echo, GIT_CAP), Some("hi\n".into()));
        // A failure says nothing.
        assert_eq!(run_capped(Command::new("/usr/bin/false"), GIT_CAP), None);
        assert_eq!(run_capped(Command::new("/no/such/program"), GIT_CAP), None);
    }

    #[test]
    fn the_capped_runner_tells_an_answer_from_a_failure() {
        let mut said_no = Command::new("/bin/sh");
        said_no.args(["-c", "echo out; exit 1"]);
        assert_eq!(
            run_capped_outcome(said_no, None, GIT_CAP),
            Ran::Exited(Some(1), "out\n".into())
        );
        let mut echo = Command::new("/bin/echo");
        echo.arg("hi");
        assert_eq!(
            run_capped_outcome(echo, None, GIT_CAP),
            Ran::Exited(Some(0), "hi\n".into())
        );
        let mut sleep = Command::new("/bin/sleep");
        sleep.arg("5");
        assert_eq!(
            run_capped_outcome(sleep, None, Duration::from_millis(100)),
            Ran::TimedOut
        );
        assert_eq!(
            run_capped_outcome(Command::new("/no/such/program"), None, GIT_CAP),
            Ran::Failed
        );
        assert_eq!(
            Ran::Exited(Some(0), String::from("x")).success(),
            Some("x".into())
        );
        assert_eq!(Ran::Exited(Some(1), String::from("x")).success(), None);
        assert_eq!(Ran::Exited(None, String::from("x")).success(), None);
        assert_eq!(Ran::<String>::TimedOut.success(), None);
    }

    #[test]
    fn the_cap_ends_the_whole_process_group() {
        // A child (as a hook or fsmonitor would be) that keeps stdout open past the cap.
        let pidfile = std::env::temp_dir().join(format!("fuxx-capped-{}", std::process::id()));
        let _ = std::fs::remove_file(&pidfile);
        let mut sh = Command::new("/bin/sh");
        sh.arg("-c")
            .arg("sleep 30 & echo $! > \"$0\"; wait")
            .arg(&pidfile);
        assert_eq!(
            run_capped_outcome(sh, None, Duration::from_millis(300)),
            Ran::TimedOut
        );
        let pid: c_int = std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let _ = std::fs::remove_file(&pidfile);
        let gone = (0..100).any(|_| {
            // SAFETY: signal 0 only asks whether the process is there.
            let alive = unsafe { kill(pid, 0) } == 0;
            if alive {
                std::thread::sleep(Duration::from_millis(20));
            }
            !alive
        });
        assert!(gone, "the grandchild {pid} was left running");
    }

    /// A repo in a fresh folder with one commit, its branch `trunk`; none without git.
    fn repo(name: &str) -> Option<(PathBuf, PathBuf)> {
        let git = find_git()?;
        let dir = std::env::temp_dir().join(format!("fuxx-git-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let ok = Command::new(&git)
                .arg("-C")
                .arg(&dir)
                .args(["-c", "user.name=fuxx", "-c", "user.email=fuxx@example.com"])
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        run(&["init", "-q", "-b", "trunk"]);
        run(&["commit", "-q", "--allow-empty", "-m", "one"]);
        Some((git, dir))
    }

    #[test]
    fn the_branch_is_read_and_a_detached_head_gives_its_short_sha() {
        let Some((git, dir)) = repo("branch") else {
            eprintln!("no git here: skipped");
            return;
        };
        assert_eq!(branch_at(&git, &dir), Some("trunk".into()));
        let sha = run_git(&git, &dir, &["rev-parse", "--short", "HEAD"]).unwrap();
        run_git(&git, &dir, &["checkout", "-q", "--detach"]).unwrap();
        assert_eq!(branch_at(&git, &dir), short_sha(&sha));
        // Not a repo: nothing.
        let outside = std::env::temp_dir().join(format!("fuxx-nogit-{}", std::process::id()));
        std::fs::create_dir_all(&outside).unwrap();
        assert_eq!(branch_at(&git, &outside), None);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn places_ask_git_once_per_folder_every_5_s_and_keep_the_last_answer() {
        let mut p = Places {
            git: Some(Some(PathBuf::from("/usr/bin/git"))),
            ..Places::default()
        };
        let api = Path::new("/w/api");
        let t0 = Instant::now();
        assert!(p.ask_branch(api, t0).is_some());
        // Asked: not again until 5 s on, even before the answer comes.
        assert!(p.ask_branch(api, t0 + Duration::from_secs(1)).is_none());
        assert!(p.set_branch(api, Some("main".into())));
        assert!(!p.set_branch(api, Some("main".into())), "same answer");
        assert_eq!(p.branch(api), Some("main"));
        let later = t0 + REFRESH;
        assert!(p.ask_branch(api, later).is_some());
        // The old answer shows while the new one is on its way.
        assert_eq!(p.branch(api), Some("main"));
        // Another folder is its own.
        assert!(p.ask_branch(Path::new("/w/web"), t0).is_some());
        // No git on this Mac: never asked.
        let mut none = Places {
            git: Some(None),
            ..Places::default()
        };
        assert!(none.ask_branch(api, t0).is_none());
        // Frozen (screenshots): nothing is looked up.
        p.frozen = true;
        assert!(p.ask_branch(api, later + REFRESH).is_none());
        assert!(!p.folder_due(1, later + REFRESH));
    }

    #[test]
    fn a_folder_report_counts_only_from_this_mac() {
        let me = Some("Fitz-MacBook.local");
        // No host, or localhost.
        assert!(is_this_mac("", me));
        assert!(is_this_mac("localhost", me));
        assert!(is_this_mac("LocalHost", None));
        // This Mac's name, whatever the case, with or without `.local` (either side).
        assert!(is_this_mac("Fitz-MacBook.local", me));
        assert!(is_this_mac("fitz-macbook", me));
        assert!(is_this_mac("FITZ-MACBOOK.LOCAL", Some("Fitz-MacBook")));
        assert!(is_this_mac("fitz-macbook.local.", me));
        // Another machine (an ssh from a local terminal), or a name we can't check.
        assert!(!is_this_mac("devbox", me));
        assert!(!is_this_mac("fitz-macbook.example.com", me));
        assert!(!is_this_mac("devbox", None));
        // The real name matches itself.
        let here = this_hostname().expect("this Mac has a name");
        assert!(is_this_mac(&here, Some(&here)));
    }

    #[test]
    fn frames_check_the_rows_at_most_once_a_second() {
        let mut p = Places::default();
        let t0 = Instant::now();
        assert!(p.check_due(t0));
        assert!(!p.check_due(t0));
        assert!(!p.check_due(t0 + CHECK - Duration::from_millis(1)));
        assert!(p.check_due(t0 + CHECK));
    }

    #[test]
    fn branches_of_folders_no_terminal_is_in_are_forgotten() {
        let mut p = Places::default();
        let t0 = Instant::now();
        p.set_folder(1, Some("/w/api".into()), t0);
        p.set_folder(2, Some("/w/web".into()), t0);
        p.set_branch(Path::new("/w/api"), Some("main".into()));
        p.set_branch(Path::new("/w/web"), Some("dev".into()));
        p.retain(|id| id == 1);
        assert_eq!(p.branch(Path::new("/w/api")), Some("main"));
        assert_eq!(p.branch(Path::new("/w/web")), None);
        assert_eq!(p.folder(2), None);
    }
}
