//! M10a Task 0 spike — throwaway. Does a process that outlives fuxx.app keep access to
//! ~/Documents, ~/Desktop and ~/Downloads (macOS TCC privacy attribution)?
//!
//! `fuxx __detach-probe [--disclaim]` spawns `fuxx __detach-probe-child` in its own session with
//! `posix_spawn`, output appended to ~/Library/Logs/fuxx/probe.log, and exits. The child waits 15 s
//! (time to quit the app), then tries each folder and logs what happened. Neither command is listed
//! in `fuxx help`. Remove at the end of the milestone unless it becomes the daemon's spawn code.

use std::ffi::{CString, c_char, c_int, c_void};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `POSIX_SPAWN_SETSID` (sys/spawn.h in the macOS SDK).
const POSIX_SPAWN_SETSID: i16 = 0x0400;
const O_RDONLY: c_int = 0x0;
const O_WRONLY: c_int = 0x1;
const O_APPEND: c_int = 0x8;
const O_CREAT: c_int = 0x200;
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;
/// Seconds the child waits before probing, so the app can be quit first.
const WAIT_SECS: u64 = 15;

// On macOS both `posix_spawn_file_actions_t` and `posix_spawnattr_t` are opaque pointers.
unsafe extern "C" {
    fn posix_spawn(
        pid: *mut c_int,
        path: *const c_char,
        file_actions: *const *mut c_void,
        attr: *const *mut c_void,
        argv: *const *mut c_char,
        envp: *const *const c_char,
    ) -> c_int;
    fn posix_spawn_file_actions_init(fa: *mut *mut c_void) -> c_int;
    fn posix_spawn_file_actions_destroy(fa: *mut *mut c_void) -> c_int;
    fn posix_spawn_file_actions_addopen(
        fa: *mut *mut c_void,
        fd: c_int,
        path: *const c_char,
        oflag: c_int,
        mode: u16,
    ) -> c_int;
    fn posix_spawn_file_actions_adddup2(fa: *mut *mut c_void, fd: c_int, newfd: c_int) -> c_int;
    fn posix_spawnattr_init(attr: *mut *mut c_void) -> c_int;
    fn posix_spawnattr_destroy(attr: *mut *mut c_void) -> c_int;
    fn posix_spawnattr_setflags(attr: *mut *mut c_void, flags: i16) -> c_int;
    fn _NSGetEnviron() -> *mut *const *const c_char;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn getppid() -> c_int;
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn log_path() -> PathBuf {
    home().join("Library/Logs/fuxx/probe.log")
}

/// Parent side: spawn the detached child, print where to look, return the exit code.
pub fn run_parent(disclaim: bool) -> i32 {
    match spawn_child(disclaim) {
        Ok(pid) => {
            println!(
                "probe started (pid {pid}); quit fuxx now and read ~/Library/Logs/fuxx/probe.log in a minute"
            );
            0
        }
        Err(e) => {
            eprintln!("fuxx __detach-probe: {e}");
            1
        }
    }
}

fn spawn_child(disclaim: bool) -> Result<c_int, String> {
    let log = log_path();
    fs::create_dir_all(log.parent().unwrap()).map_err(|e| format!("create log folder: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let c = |s: &str| CString::new(s).map_err(|e| e.to_string());
    let exe_c = c(exe.to_str().ok_or("exe path is not UTF-8")?)?;
    let log_c = c(log.to_str().ok_or("log path is not UTF-8")?)?;
    let null_c = c("/dev/null")?;
    let child_c = c("__detach-probe-child")?;
    let flag_c = c(if disclaim { "--disclaimed" } else { "--plain" })?;
    let argv = [
        exe_c.as_ptr() as *mut c_char,
        child_c.as_ptr() as *mut c_char,
        flag_c.as_ptr() as *mut c_char,
        std::ptr::null_mut(),
    ];

    let mut note = None;
    let mut pid: c_int = 0;
    // SAFETY: the CStrings and argv outlive the posix_spawn call; argv is null-terminated; the
    // attr and file-actions objects are initialised before use and destroyed exactly once;
    // `environ` comes from _NSGetEnviron and is valid for the process lifetime.
    let rc = unsafe {
        let mut attr: *mut c_void = std::ptr::null_mut();
        let mut fa: *mut c_void = std::ptr::null_mut();
        posix_spawnattr_init(&mut attr);
        posix_spawn_file_actions_init(&mut fa);
        posix_spawnattr_setflags(&mut attr, POSIX_SPAWN_SETSID);
        posix_spawn_file_actions_addopen(&mut fa, 0, null_c.as_ptr(), O_RDONLY, 0);
        posix_spawn_file_actions_addopen(
            &mut fa,
            1,
            log_c.as_ptr(),
            O_WRONLY | O_CREAT | O_APPEND,
            0o644,
        );
        posix_spawn_file_actions_adddup2(&mut fa, 1, 2);
        if disclaim {
            let sym = c("responsibility_spawnattrs_setdisclaim")?;
            let f = dlsym(RTLD_DEFAULT, sym.as_ptr());
            if f.is_null() {
                note = Some("disclaim unavailable");
            } else {
                let f: unsafe extern "C" fn(*mut *mut c_void, c_int) -> c_int =
                    std::mem::transmute(f);
                f(&mut attr, 1);
            }
        }
        let rc = posix_spawn(
            &mut pid,
            exe_c.as_ptr(),
            &fa,
            &attr,
            argv.as_ptr(),
            *_NSGetEnviron(),
        );
        posix_spawn_file_actions_destroy(&mut fa);
        posix_spawnattr_destroy(&mut attr);
        rc
    };
    if let Some(n) = note {
        // Into the same log, so the reader sees it next to the child's lines.
        append(&log, &format!("[parent] {n}"));
        println!("{n}");
    }
    if rc != 0 {
        return Err(format!(
            "posix_spawn failed: {}",
            std::io::Error::from_raw_os_error(rc)
        ));
    }
    Ok(pid)
}

fn append(path: &Path, line: &str) {
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Child side: wait, probe the three folders, log to stdout (which the parent pointed at the log).
pub fn run_child(disclaimed: bool) -> i32 {
    let pid = std::process::id();
    // SAFETY: getppid has no preconditions.
    let ppid = unsafe { getppid() };
    println!(
        "[child] start t={} pid={pid} ppid={ppid} disclaimed={disclaimed}",
        now()
    );
    std::thread::sleep(Duration::from_secs(WAIT_SECS));
    // SAFETY: as above.
    let ppid = unsafe { getppid() };
    println!(
        "[child] probing t={} pid={pid} ppid={ppid} disclaimed={disclaimed}",
        now()
    );
    for name in ["Documents", "Desktop", "Downloads"] {
        let dir = home().join(name);
        match fs::read_dir(&dir) {
            Ok(rd) => println!("[child] {name}: read_dir ok, {} entries", rd.count()),
            Err(e) => println!(
                "[child] {name}: read_dir ERROR {e} (os error {:?})",
                e.raw_os_error()
            ),
        }
        let file = dir.join(format!("fuxx-probe-{pid}.tmp"));
        match fs::write(&file, b"probe").and_then(|()| fs::remove_file(&file)) {
            Ok(()) => println!("[child] {name}: create+remove ok"),
            Err(e) => println!(
                "[child] {name}: create+remove ERROR {e} (os error {:?})",
                e.raw_os_error()
            ),
        }
    }
    println!("[child] done t={}", now());
    0
}
