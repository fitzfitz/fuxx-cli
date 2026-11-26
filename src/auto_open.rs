//! Auto-opens (a chore after M11ar): a program in a fuxx terminal or agent tab that opens a
//! browser by itself (`npm run dev` with auto-open, a mockup preview, `open URL`, Python's
//! `webbrowser`) lands where a clicked link would ("Open links in": a split on the right, a tab
//! in this pane, or your default browser), through `fuxx link open URL`.
//!
//! At startup fuxx writes three small POSIX `sh` scripts (the shims) to
//! `~/.local/share/fuxx/shims` (`~/.local/share/fuxx-dev/shims` for a debug build; a path with
//! no space, so tools that split `$BROWSER` can run it), each only when it changed: `open`, `xdg-open` and `fuxx-browser` (for `$BROWSER`). Every fuxx terminal and
//! agent process gets that folder first on `PATH` and `BROWSER=<folder>/fuxx-browser`. A shim
//! takes over only a lone `http://` or `https://` address, or (chore 35) a local page: an
//! existing `.html`/`.htm` file (made absolute; `fuxx link open` makes it a `file://` address)
//! or such a page's `file://` address (`auto_open_target`; also `-u`/`--url URL`, and
//! `-a <our fuxx-browser> URL`, which is how Vite's `open` package uses `$BROWSER`); anything
//! else (PDFs, folders, other files, flags), and any failure (fuxx not reachable, scripting
//! off, no calling tab), runs the real `/usr/bin/open` with the arguments untouched.
//!
//! Limits: macOS login shells run `path_helper` (`/etc/zprofile`), which moves the folder behind
//! `/usr/bin`; fuxx's zsh integration puts it first again at the first prompt (`.zshenv`, from
//! `FUXX_SHIMS`). Without it (bash, fish, integration off) only `$BROWSER` is sure to reach fuxx.
//! SSH sessions' remote shells get no shims. An earlier build's folder under "Application
//! Support" is removed at start when it holds only shims.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The `$BROWSER` shim's file name.
pub const BROWSER_SHIM: &str = "fuxx-browser";
/// Every shim, by file name; each is the same script.
pub const SHIMS: [&str; 3] = ["open", "xdg-open", BROWSER_SHIM];
/// The real `open` a shim runs for everything it doesn't take over (a test points it at a stub).
pub const REAL_OPEN_VAR: &str = "FUXX_REAL_OPEN";
/// The shim folder, for the zsh integration to put first on `PATH` again after `path_helper`.
pub const SHIMS_VAR: &str = "FUXX_SHIMS";
/// The agent tab (`aN`) an agent's process runs in: its `fuxx link open` opens beside it.
pub const AGENT_VAR: &str = "FUXX_AGENT_TAB";

/// Whether `url` is a web address a shim takes over: `http://` or `https://` (any case) and
/// something after it.
pub fn is_web(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    let scheme = if lower.starts_with("http://") {
        7
    } else if lower.starts_with("https://") {
        8
    } else {
        return false;
    };
    url.len() > scheme
}

/// Whether `link open` takes `url` from a shim: a web address (`is_web`), or (chore 35) a local
/// page's (`browser_url::is_local_page`).
pub fn takes(url: &str) -> bool {
    is_web(url) || crate::browser_url::is_local_page(url)
}

/// What a shim given `args`, run in folder `cwd`, hands to fuxx, or none (the real `open` gets
/// them). Exactly one web address (`is_web`) or local page address (`file://…html`); or (chore
/// 35) one page on disk (`.html`/`.htm`, an existing file, not a flag), made absolute against
/// `cwd`; or either after `-u` / `--url`, or after `-a APP` where `APP` is fuxx's own `$BROWSER`
/// shim (`browser`, or its bare name). Any other flag or file, or more arguments: none. The
/// shims' `case` mirrors this (`the_script_mirrors_auto_open_target` runs both on the same
/// cases); only the tests call it, since the decision is the script's.
#[cfg(test)]
pub fn auto_open_target(args: &[String], browser: &str, cwd: &Path) -> Option<String> {
    let url = match args {
        [url] => url,
        [flag, url] if flag == "-u" || flag == "--url" => url,
        [flag, app, url] if flag == "-a" && (app == browser || app == BROWSER_SHIM) => url,
        _ => return None,
    };
    if takes(url) {
        return Some(url.clone());
    }
    let file = cwd.join(url);
    let page = !url.starts_with('-')
        && !url.contains("://")
        && crate::browser_url::is_page_name(url)
        && file.is_file();
    page.then(|| {
        if url.starts_with('/') {
            url.clone()
        } else {
            format!("{}/{url}", cwd.display())
        }
    })
}

/// The shim folder under `home`: `~/.local/share/fuxx/shims`, `fuxx-dev` for a debug build
/// (`dev`), so its scripts, which name the `fuxx` program that wrote them, don't flip between
/// builds. No space in it (unlike "Application Support"): `$BROWSER` names a file in it, and
/// some tools (gh) split `$BROWSER` on spaces.
pub fn shims_path_for(home: &Path, dev: bool) -> PathBuf {
    let app = if dev { "fuxx-dev" } else { "fuxx" };
    home.join(".local/share").join(app).join("shims")
}

/// Where an earlier build wrote the shims (its path has a space, which broke `$BROWSER` for
/// tools that split it): both builds' folders, removed at start (`remove_old_shims`).
pub fn old_shims_paths_for(home: &Path) -> [PathBuf; 2] {
    let support = home.join("Library/Application Support/fuxx");
    [support.join("shims"), support.join("shims-dev")]
}

/// Remove `dir` if it holds nothing but shims (files named as `SHIMS`); `true` when it went.
/// Anything else in it, or a failure, leaves it.
pub fn remove_old_shims(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    let mut names = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            return false;
        };
        let name = entry.file_name();
        let ours = SHIMS.iter().any(|s| OsStr::new(s) == name);
        if !ours || !entry.file_type().is_ok_and(|t| t.is_file()) {
            return false;
        }
        names.push(name);
    }
    names.iter().all(|n| fs::remove_file(dir.join(n)).is_ok()) && fs::remove_dir(dir).is_ok()
}

/// `s` in single quotes for `sh`, a `'` inside written `'\''`.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The script every shim holds: `fuxx` is the program it calls, `browser` the `$BROWSER` shim's
/// path. POSIX `sh`, every expansion quoted.
pub fn script(fuxx: &Path, browser: &Path) -> String {
    let fuxx = sh_quote(&fuxx.display().to_string());
    let browser = sh_quote(&browser.display().to_string());
    format!(
        r#"#!/bin/sh
# fuxx: a program's own browser opening (open URL, xdg-open URL, $BROWSER URL) goes where a
# clicked link would. A lone http(s) address, a local page's file:// address, or a page on disk
# (an existing .html/.htm file, made absolute) (or any of them after -u/--url, or -a <this
# folder's fuxx-browser>) goes to `fuxx link open`; anything else, or any failure, to the real
# open, untouched. fuxx writes this file at startup; edits are replaced.
fuxx={fuxx}
browser={browser}
real="${{{REAL_OPEN_VAR}:-/usr/bin/open}}"
url=
case $# in
    1) url=$1 ;;
    2) case $1 in -u | --url) url=$2 ;; esac ;;
    3) if [ "$1" = -a ] && {{ [ "$2" = "$browser" ] || [ "$2" = {BROWSER_SHIM} ]; }}; then
        url=$3
    fi ;;
esac
case $url in
    [Hh][Tt][Tt][Pp]://?* | [Hh][Tt][Tt][Pp][Ss]://?*) ;;
    [Ff][Ii][Ll][Ee]://*.[Hh][Tt][Mm] | [Ff][Ii][Ll][Ee]://*.[Hh][Tt][Mm][Ll]) ;;
    [Ff][Ii][Ll][Ee]://*.[Hh][Tt][Mm][?#]* | [Ff][Ii][Ll][Ee]://*.[Hh][Tt][Mm][Ll][?#]*) ;;
    -* | *://*) exec "$real" "$@" ;;
    *.[Hh][Tt][Mm] | *.[Hh][Tt][Mm][Ll])
        [ -f "$url" ] || exec "$real" "$@"
        case $url in /*) ;; *) url=$PWD/$url ;; esac ;;
    *) exec "$real" "$@" ;;
esac
[ -n "${{FUXX_SOCKET:-}}" ] || exec "$real" "$@"
"$fuxx" link open "$url" >/dev/null 2>&1 || exec "$real" "$@"
"#
    )
}

/// Write every shim into `dir` (`0o755`) for program `fuxx`, each only when missing or
/// different; `Ok(true)` when something was written. Each is written beside its place and
/// renamed over it, so a program running one meanwhile reads the old script or the new one.
pub fn install(dir: &Path, fuxx: &Path) -> io::Result<bool> {
    fs::create_dir_all(dir)?;
    let text = script(fuxx, &dir.join(BROWSER_SHIM));
    let mut wrote = false;
    for name in SHIMS {
        let path = dir.join(name);
        let same = fs::read(&path).is_ok_and(|now| now == text.as_bytes());
        let runnable = fs::metadata(&path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0);
        if same && runnable {
            continue;
        }
        let tmp = dir.join(format!("{name}.{}.tmp", std::process::id()));
        let written = write_script(&tmp, &text).and_then(|()| fs::rename(&tmp, &path));
        if written.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        written?;
        wrote = true;
    }
    Ok(wrote)
}

/// `text` as a new file at `path`, `0o755` whatever the umask.
fn write_script(path: &Path, text: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o755)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.set_permissions(fs::Permissions::from_mode(0o755))
}

/// Set once by `setup`: the shim folder and the `fuxx` program its scripts call.
static INSTALLED: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();

/// At startup, before any terminal: write the shims for this program. On failure, one log line,
/// and terminals start without them (auto-opens go to your default browser, as before).
pub fn setup() {
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty());
    let (Some(home), Ok(exe)) = (home, std::env::current_exe()) else {
        eprintln!("[fuxx] auto-open: $HOME or this program's path is unknown; no shims");
        return;
    };
    // An earlier build's folders (under "Application Support"), if they hold only shims.
    for old in old_shims_paths_for(Path::new(&home)) {
        if remove_old_shims(&old) {
            eprintln!(
                "[fuxx] auto-open: removed the old shims in {}",
                old.display()
            );
        }
    }
    let dir = shims_path_for(Path::new(&home), cfg!(debug_assertions));
    match install(&dir, &exe) {
        Ok(_) => {
            let _ = INSTALLED.set((dir, exe));
        }
        Err(err) => eprintln!(
            "[fuxx] auto-open: could not write {}: {err}; no shims",
            dir.display()
        ),
    }
}

/// The shim folder for a new terminal or agent, checked at each spawn: every shim is there (a
/// stat), or writing them again worked. None before `setup`, or if it failed.
pub fn shims() -> Option<&'static Path> {
    let (dir, exe) = INSTALLED.get()?;
    if SHIMS.iter().all(|name| dir.join(name).is_file()) {
        return Some(dir);
    }
    match install(dir, exe) {
        Ok(_) => Some(dir),
        Err(err) => {
            eprintln!(
                "[fuxx] auto-open: {} is gone and could not be written again: {err}",
                dir.display()
            );
            None
        }
    }
}

/// `path` with `dir` first, and nowhere else (never twice).
pub fn with_shims(path: Option<&OsStr>, dir: &Path) -> String {
    let dir = dir.display().to_string();
    let path = path.map(OsStr::to_string_lossy).unwrap_or_default();
    let rest = path.split(':').filter(|p| !p.is_empty() && *p != dir);
    std::iter::once(dir.as_str())
        .chain(rest)
        .collect::<Vec<_>>()
        .join(":")
}

/// The variables besides `PATH` a terminal or agent with shims in `dir` gets: `BROWSER` (its
/// shim) and `FUXX_SHIMS` (the folder, for the zsh integration).
pub fn shim_vars(dir: &Path) -> Vec<(String, String)> {
    vec![
        (
            "BROWSER".to_string(),
            dir.join(BROWSER_SHIM).display().to_string(),
        ),
        (SHIMS_VAR.to_string(), dir.display().to_string()),
    ]
}

/// The environment fuxx adds to agent tab `aN`'s process while it serves `socket`: the socket,
/// the tab (`FUXX_AGENT_TAB`), an empty `FUXX_SESSION` (one inherited would name a terminal),
/// and with shims in `shims`, `PATH` (`path` with them first) and `shim_vars`. Nothing without
/// a socket: `fuxx` could reach no window.
pub fn agent_env(
    socket: Option<&Path>,
    agent: u64,
    shims: Option<&Path>,
    path: Option<&OsStr>,
) -> Vec<(String, String)> {
    let Some(socket) = socket else {
        return Vec::new();
    };
    let mut env = vec![
        ("FUXX_SOCKET".to_string(), socket.display().to_string()),
        (AGENT_VAR.to_string(), format!("a{agent}")),
        ("FUXX_SESSION".to_string(), String::new()),
    ];
    if let Some(dir) = shims {
        env.push(("PATH".to_string(), with_shims(path, dir)));
        env.extend(shim_vars(dir));
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const URL: &str = "http://localhost:5173";
    const BROWSER: &str = "/x/shims/fuxx-browser";

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn target(a: &[&str]) -> Option<String> {
        auto_open_target(&args(a), BROWSER, Path::new("/nonexistent"))
    }

    #[test]
    fn a_lone_web_address_is_taken_over() {
        assert_eq!(target(&[URL]).as_deref(), Some(URL));
        assert_eq!(
            target(&["HTTPS://example.com/a?b=c"]).as_deref(),
            Some("HTTPS://example.com/a?b=c")
        );
        assert_eq!(target(&["-u", URL]).as_deref(), Some(URL));
        assert_eq!(target(&["--url", URL]).as_deref(), Some(URL));
    }

    #[test]
    fn our_own_browser_shim_as_the_app_is_taken_over() {
        assert_eq!(target(&["-a", BROWSER, URL]).as_deref(), Some(URL));
        assert_eq!(target(&["-a", "fuxx-browser", URL]).as_deref(), Some(URL));
        assert_eq!(target(&["-a", "Safari", URL]), None);
    }

    #[test]
    fn anything_else_goes_to_the_real_open() {
        for a in [
            &[][..],
            &["file:///tmp/code/my-app/report.pdf"],
            &["code/my-app/index.html"],
            &["code/my-app"],
            &["mailto:team@example.com"],
            &["http://"],
            &["https://"],
            &["httpx://example.com"],
            &[URL, URL],
            &["-R", URL],
            &["-a", "Safari", URL],
            &["-u", "file:///etc/hosts"],
            &["--", URL],
            &["-n", "-u", URL],
        ] {
            assert_eq!(target(a), None, "{a:?}");
        }
    }

    /// Chore 35: what `link open` takes from a shim: a web address, or a local page's.
    #[test]
    fn link_open_takes_a_web_address_or_a_local_page() {
        assert!(takes(URL) && takes("file:///x/my-app/index.html"));
        for no in [
            "file:///etc/hosts",
            "/x/my-app/index.html",
            "mailto:team@example.com",
        ] {
            assert!(!takes(no), "{no}");
        }
    }

    #[test]
    fn the_shim_folder_follows_the_dev_split() {
        let home = Path::new("/Users/me");
        assert_eq!(
            shims_path_for(home, false),
            Path::new("/Users/me/.local/share/fuxx/shims")
        );
        assert_eq!(
            shims_path_for(home, true),
            Path::new("/Users/me/.local/share/fuxx-dev/shims")
        );
    }

    /// A tool that splits `$BROWSER` on spaces (gh) must still run it: neither the folder nor
    /// `BROWSER` holds any whitespace, and `BROWSER` is the folder's `fuxx-browser`.
    #[test]
    fn the_shim_folder_and_browser_have_no_whitespace() {
        let home = Path::new("/Users/me");
        for dev in [false, true] {
            let dir = shims_path_for(home, dev);
            let text = dir.display().to_string();
            assert!(!text.contains(char::is_whitespace), "{text}");
            let vars = shim_vars(&dir);
            let browser = vars.iter().find(|(k, _)| k == "BROWSER").map(|(_, v)| v);
            assert_eq!(
                browser.map(String::as_str),
                dir.join("fuxx-browser").to_str()
            );
            assert!(!browser.unwrap().contains(char::is_whitespace));
        }
    }

    /// The folder an earlier build wrote (under "Application Support") goes at start, only when
    /// it holds nothing but shims; anything else of yours keeps it.
    #[test]
    fn the_old_shim_folder_goes_only_when_it_holds_just_shims() {
        let home = Path::new("/Users/me");
        assert_eq!(
            old_shims_paths_for(home),
            [
                Path::new("/Users/me/Library/Application Support/fuxx/shims"),
                Path::new("/Users/me/Library/Application Support/fuxx/shims-dev"),
            ]
        );
        let t = crate::test_temp::dir("auto-open-old");
        let ours = t.join("shims");
        install(&ours, Path::new("/x/fuxx")).unwrap();
        assert!(remove_old_shims(&ours), "just our three files: removed");
        assert!(!ours.exists());
        assert!(!remove_old_shims(&ours), "gone already: nothing to do");
        let mixed = t.join("shims-dev");
        install(&mixed, Path::new("/x/fuxx")).unwrap();
        fs::write(mixed.join("notes.txt"), "mine").unwrap();
        assert!(!remove_old_shims(&mixed), "a file of yours: kept");
        assert!(mixed.join("notes.txt").exists() && mixed.join("open").exists());
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn the_shim_folder_is_first_on_path_once() {
        let dir = Path::new("/a b/shims");
        assert_eq!(
            with_shims(Some(OsStr::new("/usr/bin:/a b/shims:/bin")), dir),
            "/a b/shims:/usr/bin:/bin"
        );
        assert_eq!(
            with_shims(Some(OsStr::new("/a b/shims:/usr/bin")), dir),
            "/a b/shims:/usr/bin"
        );
        assert_eq!(with_shims(None, dir), "/a b/shims");
        assert_eq!(with_shims(Some(OsStr::new("")), dir), "/a b/shims");
    }

    #[test]
    fn an_agent_gets_the_socket_its_tab_and_the_shims() {
        let dir = Path::new("/s");
        let env = agent_env(
            Some(Path::new("/ours.sock")),
            3,
            Some(dir),
            Some(OsStr::new("/usr/bin")),
        );
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("FUXX_SOCKET"), Some("/ours.sock"));
        assert_eq!(get("FUXX_AGENT_TAB"), Some("a3"));
        assert_eq!(get("FUXX_SESSION"), Some(""));
        assert_eq!(get("PATH"), Some("/s:/usr/bin"));
        assert_eq!(get("BROWSER"), Some("/s/fuxx-browser"));
        assert_eq!(get("FUXX_SHIMS"), Some("/s"));
        // No shims: no PATH or BROWSER; no socket: nothing.
        let bare = agent_env(Some(Path::new("/ours.sock")), 3, None, None);
        assert!(bare.iter().all(|(k, _)| k != "PATH" && k != "BROWSER"));
        assert!(agent_env(None, 3, Some(dir), None).is_empty());
    }

    #[test]
    fn install_writes_once_and_again_when_changed() {
        let dir = crate::test_temp::dir("auto-open-install");
        let fuxx = Path::new("/Applications/fuxx's app/fuxx");
        assert!(install(&dir, fuxx).unwrap(), "first write");
        assert!(!install(&dir, fuxx).unwrap(), "unchanged: not written");
        for name in SHIMS {
            let mode = fs::metadata(dir.join(name)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755, "{name}");
        }
        fs::write(dir.join("open"), "edited").unwrap();
        assert!(install(&dir, fuxx).unwrap(), "an edit is replaced");
        assert!(
            install(&dir, Path::new("/other/fuxx")).unwrap(),
            "a new program"
        );
        let text = fs::read_to_string(dir.join("xdg-open")).unwrap();
        assert!(text.contains("fuxx='/other/fuxx'"), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A folder with the shims for a fake `fuxx` that logs its arguments (one per line) to
    /// `fuxx.log` and exits with `$FAKE_FUXX_EXIT`, and a stub real `open` that logs to
    /// `open.log`.
    struct Rig {
        dir: PathBuf,
    }

    impl Rig {
        fn new(name: &str) -> Self {
            let dir = crate::test_temp::dir(name);
            fs::create_dir_all(&dir).unwrap();
            let fake = |file: &str, log: &str, exit: &str| {
                let path = dir.join(file);
                let body = format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nexit {exit}\n",
                    sh_quote(&dir.join(log).display().to_string())
                );
                write_script(&path, &body).unwrap();
                path
            };
            let fuxx = fake("fake-fuxx", "fuxx.log", "\"${FAKE_FUXX_EXIT:-0}\"");
            fake("stub-open", "open.log", "0");
            install(&dir.join("shims"), &fuxx).unwrap();
            // Chore 35's files: two pages, a PDF, a folder named like a page.
            for page in ["page.html", "site/index.HTM", "report.pdf"] {
                let path = dir.join(page);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, "<p>my-app</p>").unwrap();
            }
            fs::create_dir_all(dir.join("folder.html")).unwrap();
            Rig { dir }
        }

        /// The folder the shims run in, as `sh`'s `$PWD` reads it (symbolic links resolved).
        fn here(&self) -> PathBuf {
            fs::canonicalize(&self.dir).unwrap()
        }

        /// Run shim `shim` through `sh` with `args`; (what fuxx got, what the real open got),
        /// each `None` when it wasn't run.
        fn run(&self, shim: &str, args: &[&str], env: &[(&str, &str)]) -> [Option<String>; 2] {
            let logs = ["fuxx.log", "open.log"].map(|l| self.dir.join(l));
            for log in &logs {
                let _ = fs::remove_file(log);
            }
            let status = Command::new("/bin/sh")
                .arg(self.dir.join("shims").join(shim))
                .args(args)
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env(REAL_OPEN_VAR, self.dir.join("stub-open"))
                .envs(env.iter().copied())
                .current_dir(&self.dir)
                .status()
                .expect("sh runs");
            assert!(status.success(), "{shim} {args:?}: {status}");
            logs.map(|l| fs::read_to_string(l).ok())
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    const IN_FUXX: &[(&str, &str)] = &[("FUXX_SOCKET", "/x/fuxx.sock"), ("FUXX_SESSION", "s4")];

    fn lines(a: &[&str]) -> Option<String> {
        Some(a.iter().map(|l| format!("{l}\n")).collect())
    }

    #[test]
    fn the_open_shim_hands_a_web_address_to_fuxx() {
        let rig = Rig::new("auto-open-shim-take");
        let to_fuxx = [lines(&["link", "open", URL]), None];
        assert_eq!(rig.run("open", &[URL], IN_FUXX), to_fuxx);
        assert_eq!(rig.run("open", &["-u", URL], IN_FUXX), to_fuxx);
        assert_eq!(rig.run("open", &["--url", URL], IN_FUXX), to_fuxx);
        let browser = rig.dir.join("shims/fuxx-browser").display().to_string();
        assert_eq!(rig.run("open", &["-a", &browser, URL], IN_FUXX), to_fuxx);
        assert_eq!(rig.run("xdg-open", &[URL], IN_FUXX), to_fuxx);
        assert_eq!(rig.run("fuxx-browser", &[URL], IN_FUXX), to_fuxx);
        // An odd address reaches fuxx as it is, and runs nothing.
        let odd = "https://example.com/a b'c\"$(touch pwned)`touch pwned2`;x=1&y=*";
        assert_eq!(
            rig.run("open", &[odd], IN_FUXX),
            [lines(&["link", "open", odd]), None]
        );
        assert!(!rig.dir.join("pwned").exists() && !rig.dir.join("pwned2").exists());
    }

    /// Every case of the Rust rule, run through the real script: fuxx gets exactly what the
    /// rule picks, and the real open exactly the arguments the rule leaves.
    #[test]
    fn the_script_mirrors_auto_open_target() {
        let rig = Rig::new("auto-open-shim-mirror");
        let browser = rig.dir.join("shims/fuxx-browser").display().to_string();
        let cases: Vec<Vec<&str>> = vec![
            vec![URL],
            vec!["HtTpS://example.com"],
            vec!["-u", URL],
            vec!["--url", URL],
            vec!["-a", &browser, URL],
            vec!["-a", "fuxx-browser", URL],
            vec!["-a", "Safari", URL],
            vec!["-a", &browser, "code/my-app"],
            vec!["http://"],
            vec!["httpx://example.com"],
            vec!["file:///tmp/code/my-app/index.html"],
            vec!["FILE:///x/Page.HTM?a=1#b"],
            vec!["file:///x/report.pdf"],
            vec!["-u", "file:///x/page.html"],
            vec!["page.html"],
            vec!["./site/index.HTM"],
            vec!["missing.html"],
            vec!["report.pdf"],
            vec!["folder.html"],
            vec!["-R", "page.html"],
            vec!["page.html", "site/index.HTM"],
            vec!["code/my-app"],
            vec![URL, URL],
            vec!["-R", URL],
            vec!["--", URL],
            vec!["-n", "-u", URL],
            vec!["-u", "mailto:team@example.com"],
        ];
        for case in cases {
            let owned = args(&case);
            let rule = auto_open_target(&owned, &browser, &rig.here());
            let got = rig.run("open", &case, IN_FUXX);
            let want = match rule {
                Some(url) => [lines(&["link", "open", &url]), None],
                None => [None, lines(&case)],
            };
            assert_eq!(got, want, "{case:?}");
        }
    }

    #[test]
    fn the_open_shim_passes_everything_else_through_untouched() {
        let rig = Rig::new("auto-open-shim-pass");
        for a in [
            &["code/my-app"][..],
            &["file:///tmp/code/my-app/report.pdf"],
            &["-a", "Safari", URL],
            &["-R", "code/my-app"],
            &[URL, "http://localhost:5174"],
            &["mailto:team@example.com"],
            &["-u", "file:///etc/hosts"],
        ] {
            assert_eq!(rig.run("open", a, IN_FUXX), [None, lines(a)], "{a:?}");
        }
        // No arguments (the stub's printf writes one empty line for none).
        assert_eq!(rig.run("open", &[], IN_FUXX), [None, lines(&[""])]);
        // A lone address with the arguments as they were when fuxx can't take it: outside fuxx,
        // or when `fuxx link open` fails (not reachable, scripting off).
        assert_eq!(rig.run("open", &[URL], &[]), [None, lines(&[URL])]);
        let failing = [IN_FUXX, &[("FAKE_FUXX_EXIT", "3")]].concat();
        assert_eq!(
            rig.run("open", &["-u", URL], &failing),
            [lines(&["link", "open", URL]), lines(&["-u", URL])]
        );
    }

    /// Chore 35: a lone page on disk (relative to where the program runs, or absolute) or a
    /// `file://` page address goes to `fuxx link open`, as a web address does.
    #[test]
    fn the_open_shim_hands_a_local_page_to_fuxx() {
        let rig = Rig::new("auto-open-shim-page");
        let here = rig.here().display().to_string();
        let to_fuxx = |url: &str| [lines(&["link", "open", url]), None];
        let page = format!("{here}/page.html");
        assert_eq!(rig.run("open", &["page.html"], IN_FUXX), to_fuxx(&page));
        assert_eq!(
            rig.run("open", &["./site/index.HTM"], IN_FUXX),
            to_fuxx(&format!("{here}/./site/index.HTM"))
        );
        assert_eq!(rig.run("open", &[&page], IN_FUXX), to_fuxx(&page));
        for url in [
            "file:///x/page.html",
            "FILE:///x/Page.HTM",
            "file:///x/page.html?a=1#b",
        ] {
            assert_eq!(rig.run("open", &[url], IN_FUXX), to_fuxx(url), "{url}");
            assert_eq!(rig.run("open", &["-u", url], IN_FUXX), to_fuxx(url));
        }
        assert_eq!(rig.run("xdg-open", &["page.html"], IN_FUXX), to_fuxx(&page));
        assert_eq!(
            rig.run("fuxx-browser", &["page.html"], IN_FUXX),
            to_fuxx(&page)
        );
    }

    /// PDFs, folders, missing files, other files' addresses, flags and more than one file: the
    /// real open, untouched; a page too, outside fuxx or when fuxx can't take it.
    #[test]
    fn the_open_shim_leaves_other_files_to_the_real_open() {
        let rig = Rig::new("auto-open-shim-files");
        for a in [
            &["missing.html"][..],
            &["report.pdf"],
            &["folder.html"],
            &["folder.html/"],
            &["file:///x/report.pdf"],
            &["file:///x/"],
            &["-R", "page.html"],
            &["page.html", "site/index.HTM"],
        ] {
            assert_eq!(rig.run("open", a, IN_FUXX), [None, lines(a)], "{a:?}");
        }
        assert_eq!(
            rig.run("open", &["page.html"], &[]),
            [None, lines(&["page.html"])]
        );
        let failing = [IN_FUXX, &[("FAKE_FUXX_EXIT", "3")]].concat();
        let page = format!("{}/page.html", rig.here().display());
        assert_eq!(
            rig.run("open", &["page.html"], &failing),
            [lines(&["link", "open", &page]), lines(&["page.html"])]
        );
    }
}
