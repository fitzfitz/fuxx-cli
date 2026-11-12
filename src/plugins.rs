//! The installed plugins as the Settings page sees them, and the three things a click can do:
//! install, update and uninstall. Downloads and deletes run off the UI thread; a plugin is
//! only installed after the user has seen what it adds and said yes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use gpui::{AnyWindowHandle, App, Global, PromptLevel, Window};

use crate::appearance::{self, Appearance};
use crate::plugin_fetch::{self, Curl, Staged};
use crate::plugin_manifest::{self, CODE_ALLOWED, Manifest};
use crate::plugin_shell;
use crate::plugin_source::{self, Installed, LINK_HINT, Repo};
use crate::plugin_store::{self, Folders, Record};
use crate::plugin_themes;
use crate::settings;
use crate::shell_integration;

/// How long after launch fuxx first asks GitHub whether installed plugins have a newer version.
pub const FIRST_CHECK: Duration = Duration::from_secs(15);

const NO_HOME: &str = "fuxx can't find your home folder, so it can't install plugins.";

/// What is running right now. Only one thing runs at a time.
#[derive(Clone, Debug, PartialEq)]
pub enum Busy {
    Downloading,
    Checking,
    Removing(String),
    Updating(String),
    #[allow(dead_code)] // the On/Off switch on the page (a later M11z task) is the first caller
    Switching(String),
}

/// The line the page shows under the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

#[derive(Default)]
pub struct Plugins {
    pub records: Vec<Record>,
    /// Plugin id to the newer version waiting on GitHub.
    pub updates: HashMap<String, Installed>,
    pub busy: Option<Busy>,
    /// Launch cleanup is still running; nothing may start until it ends.
    pub cleaning: bool,
    pub message: Option<Message>,
}

impl Global for Plugins {}

/// Whether a new install, update or uninstall may begin.
pub fn can_start(p: &Plugins) -> bool {
    p.busy.is_none() && !p.cleaning
}

/// "Adds 2 themes: Sunset, Sunset Light".
pub fn adds_text(m: &Manifest) -> String {
    let names = m.theme_names();
    let noun = if names.len() == 1 { "theme" } else { "themes" };
    format!("Adds {} {noun}: {}", names.len(), names.join(", "))
}

const CODE_WARNING: &str = "Runs a program on your Mac in every new terminal. fuxx can't limit what it does, so only install plugins from people you trust.";

/// The install sheet's title and detail.
/// A plugin that runs code gets a warning sheet and the warning as its last line.
pub fn install_prompt(m: &Manifest, repo: &Repo) -> (String, String, PromptLevel) {
    let title = format!("Install {} {}?", m.name, m.version);
    let mut lines = vec![format!(
        "From {}",
        repo.page().trim_start_matches("https://")
    )];
    if let Some(d) = &m.description {
        lines.push(d.clone());
    }
    lines.push(adds_text(m));
    let level = if m.runs_code() {
        lines.push(CODE_WARNING.to_string());
        PromptLevel::Warning
    } else {
        PromptLevel::Info
    };
    (title, lines.join("\n"), level)
}

/// What happened, for the line under the list.
#[allow(dead_code)] // the On/Off switch on the page (a later M11z task) is the first caller
#[derive(Clone, Debug, PartialEq)]
pub enum Done {
    Installed,
    /// The label of the version now installed.
    Updated(String),
    Uninstalled,
    On,
    Off,
}

const NEW_TERMINALS: &str =
    "New terminals get the change; terminals already open keep it until you close them.";

/// The success line. A plugin that runs code adds when the change reaches terminals.
pub fn success_text(name: &str, done: &Done, runs_code: bool) -> String {
    let base = match done {
        Done::Installed => format!("Installed {name}. Its themes are in Appearance."),
        Done::Updated(label) => format!("Updated {name} to {label}."),
        Done::Uninstalled => format!("Uninstalled {name}."),
        Done::On => format!("{name} is on."),
        Done::Off => format!("{name} is off."),
    };
    if runs_code {
        format!("{base} {NEW_TERMINALS}")
    } else {
        base
    }
}

/// An update asks again when the new version runs code and the installed one doesn't.
pub fn update_needs_a_prompt(record: &Record, m: &Manifest) -> bool {
    m.runs_code() && !record.runs_code()
}

/// Rewrite `plugins.zsh` from the list as it is on disk, off the UI thread, so terminals
/// opened from now on see it. Quiet when fuxx has nowhere to keep it.
fn regenerate(f: Folders, cx: &mut App) {
    let Some(dir) = shell_integration::integration_dir() else {
        return;
    };
    cx.background_executor()
        .spawn(async move {
            let records = plugin_store::load(&f);
            // A failed write leaves the old script; there's nobody to tell at this point.
            let _ = plugin_shell::write(&dir, &f, &records);
        })
        .detach();
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn folders(cx: &App) -> Option<Folders> {
    cx.global::<Appearance>().plugin_folders.clone()
}

fn set_message(cx: &mut App, text: impl Into<String>, error: bool) {
    cx.global_mut::<Plugins>().message = Some(Message {
        text: text.into(),
        error,
    });
    cx.refresh_windows();
}

/// Reread what is installed and rebuild the theme lists, so the page shows the disk as it is
/// now. Runs after every operation, failed or not: a half-done one may have changed files.
fn refresh(cx: &mut App, id: Option<&str>) {
    let f = folders(cx);
    let records = f.as_ref().map(plugin_store::load);
    if let Some(f) = f {
        regenerate(f, cx);
    }
    let p = cx.global_mut::<Plugins>();
    if let Some(records) = records {
        p.records = records;
    }
    if let Some(id) = id {
        p.updates.remove(id);
    }
    p.busy = None;
    appearance::reload(cx);
    cx.refresh_windows();
}

/// An operation ended with an error (`id` is the plugin it was about, when known).
fn fail(cx: &mut App, id: Option<&str>, text: impl Into<String>) {
    refresh(cx, id);
    set_message(cx, text, true);
}

/// An operation ended with a change.
fn succeed(cx: &mut App, id: &str, text: String) {
    refresh(cx, Some(id));
    set_message(cx, text, false);
}

/// Delete a scratch folder on a background thread: it can be big, and the UI shouldn't wait.
fn discard(work: PathBuf, cx: &mut App) {
    cx.background_executor()
        .spawn(async move { plugin_fetch::remove_work(&work) })
        .detach();
}

/// The page is free again, nothing changed.
fn idle(cx: &mut App) {
    cx.global_mut::<Plugins>().busy = None;
    cx.refresh_windows();
}

/// Start an operation: the page is busy with `busy` and shows no old message. False when
/// another operation is running, or fuxx has no folder to keep plugins in (then it says so).
fn begin(cx: &mut App, busy: Busy) -> Option<Folders> {
    if !can_start(cx.global::<Plugins>()) {
        return None;
    }
    let Some(f) = folders(cx) else {
        set_message(cx, NO_HOME, true);
        return None;
    };
    let p = cx.global_mut::<Plugins>();
    p.busy = Some(busy);
    p.message = None;
    cx.refresh_windows();
    Some(f)
}

/// Set the global, mend what a crash left in the plugins folder, and look for newer versions
/// once `FIRST_CHECK` has passed.
pub fn init(cx: &mut App) {
    let f = folders(cx);
    let records = f.as_ref().map(plugin_store::load).unwrap_or_default();
    cx.set_global(Plugins {
        records,
        cleaning: f.is_some(),
        ..Plugins::default()
    });
    if let Some(f) = f {
        // Mending can delete big folders, so it runs off the UI thread; the list is read again after.
        let dir = shell_integration::integration_dir();
        let mend = cx.background_executor().spawn(async move {
            plugin_store::launch_cleanup(&f);
            let records = plugin_store::load(&f);
            if let Some(dir) = dir {
                let _ = plugin_shell::write(&dir, &f, &records);
            }
            records
        });
        cx.spawn(async move |cx| {
            let records = mend.await;
            cx.update(|cx| {
                let p = cx.global_mut::<Plugins>();
                p.records = records;
                p.cleaning = false;
                appearance::reload(cx);
                cx.refresh_windows();
            });
        })
        .detach();
    }
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        executor.timer(FIRST_CHECK).await;
        cx.update(check_updates);
    })
    .detach();
}

/// Ask GitHub, off the UI thread, whether each installed plugin has a newer version. A plugin
/// that can't be asked about is left as it was: no message, no button.
pub fn check_updates(cx: &mut App) {
    let records = cx.global::<Plugins>().records.clone();
    if records.is_empty() {
        return;
    }
    let work = cx.background_executor().spawn(async move {
        records
            .into_iter()
            .filter_map(|r| {
                let repo = plugin_source::parse_link(&r.repo)?;
                let d = plugin_fetch::latest(&Curl, &repo, plugin_source::chip()).ok()?;
                Some((
                    r.id,
                    plugin_source::is_newer(&r.installed, &d.installed),
                    d.installed,
                    r.installed,
                ))
            })
            .collect::<Vec<_>>()
    });
    cx.spawn(async move |cx| {
        let found = work.await;
        cx.update(|cx| {
            let p = cx.global_mut::<Plugins>();
            for (id, newer, installed, checked) in found {
                // Skip a plugin removed or changed while we were asking.
                if !p
                    .records
                    .iter()
                    .any(|r| r.id == id && r.installed == checked)
                {
                    continue;
                }
                if newer {
                    p.updates.insert(id, installed);
                } else {
                    p.updates.remove(&id);
                }
            }
            cx.refresh_windows();
        });
    })
    .detach();
}

/// Find and unpack the latest version of `repo` off the UI thread, then hand the result to
/// `then` on the main thread.
fn download(
    repo: Repo,
    staging: PathBuf,
    cx: &mut App,
    then: impl FnOnce(Result<Staged, String>, &mut App) + 'static,
) {
    let work = cx.background_executor().spawn(async move {
        let d = plugin_fetch::latest(&Curl, &repo, plugin_source::chip())?;
        plugin_fetch::stage(&Curl, &d, &staging)
    });
    cx.spawn(async move |cx| {
        let staged = work.await;
        cx.update(|cx| then(staged, cx));
    })
    .detach();
}

/// Install the plugin at `link`: download it, show what it adds, and install it on a yes.
pub fn install(link: &str, window: &mut Window, cx: &mut App) {
    if !can_start(cx.global::<Plugins>()) {
        return;
    }
    let Some(repo) = plugin_source::parse_link(link) else {
        set_message(cx, LINK_HINT, true);
        return;
    };
    let Some(f) = begin(cx, Busy::Downloading) else {
        return;
    };
    let handle = window.window_handle();
    download(
        repo.clone(),
        f.staging(),
        cx,
        move |staged, cx| match staged {
            Ok(staged) => confirm_install(handle, f, repo, staged, cx),
            Err(e) => fail(cx, None, e),
        },
    );
}

/// The download is ready: check it, then ask. The window may have closed meanwhile, which
/// counts as a no.
fn confirm_install(handle: AnyWindowHandle, f: Folders, repo: Repo, staged: Staged, cx: &mut App) {
    if let Err(e) = plugin_manifest::accept(&staged.manifest, CODE_ALLOWED) {
        discard(staged.work.clone(), cx);
        fail(cx, None, e.message());
        return;
    }
    cx.global_mut::<Plugins>().busy = Some(Busy::Checking);
    cx.refresh_windows();
    ask_then_commit(handle, f, repo, staged, false, cx);
}

/// Show the install sheet and commit on a yes. `update` is true for a newer version of an
/// installed plugin. The window may have closed meanwhile, which counts as a no.
fn ask_then_commit(
    handle: AnyWindowHandle,
    f: Folders,
    repo: Repo,
    staged: Staged,
    update: bool,
    cx: &mut App,
) {
    let (mut title, detail, level) = install_prompt(&staged.manifest, &repo);
    let yes_label = if update { "Update" } else { "Install" };
    if update {
        title = title.replacen("Install", "Update", 1);
    }
    let work = staged.work.clone();
    let asked = handle.update(cx, move |_, window, cx| {
        let answer = window.prompt(level, &title, Some(&detail), &[yes_label, "Cancel"], cx);
        cx.spawn(async move |cx| {
            let yes = answer.await.ok() == Some(0);
            cx.update(|cx| {
                if yes {
                    let done = if update {
                        Done::Updated(staged.installed.label())
                    } else {
                        Done::Installed
                    };
                    finish_commit(f, repo, staged, done, cx);
                } else {
                    discard(staged.work.clone(), cx);
                    idle(cx);
                }
            });
        })
        .detach();
    });
    if asked.is_err() {
        discard(work, cx);
        idle(cx);
    }
}

/// Move the staged plugin into place and report it as `done`. The move (which deletes an old
/// version) and the scratch folder's removal run off the UI thread.
fn finish_commit(f: Folders, repo: Repo, staged: Staged, done: Done, cx: &mut App) {
    let id = staged.manifest.id.clone();
    let work = cx.background_executor().spawn(async move {
        let done = plugin_store::commit(
            &f,
            &staged.plugin_dir,
            &staged.manifest,
            &repo,
            &staged.installed,
            now_unix(),
        );
        plugin_fetch::remove_work(&staged.work);
        done
    });
    cx.spawn(async move |cx| {
        let committed = work.await;
        cx.update(|cx| match committed {
            Ok(record) => {
                let text = success_text(&record.name, &done, record.runs_code());
                succeed(cx, &record.id, text);
            }
            Err(e) => fail(cx, Some(&id), e),
        });
    })
    .detach();
}

/// Update plugin `id` to its latest version. The click is the consent, so there is no sheet,
/// unless the new version runs code and the installed one didn't: that asks like an install.
pub fn update(id: &str, window: &mut Window, cx: &mut App) {
    if !can_start(cx.global::<Plugins>()) {
        return;
    }
    let Some(record) = cx
        .global::<Plugins>()
        .records
        .iter()
        .find(|r| r.id == id)
        .cloned()
    else {
        return;
    };
    let Some(repo) = plugin_source::parse_link(&record.repo) else {
        set_message(cx, LINK_HINT, true);
        return;
    };
    let Some(f) = begin(cx, Busy::Updating(record.id.clone())) else {
        return;
    };
    let handle = window.window_handle();
    download(
        repo.clone(),
        f.staging(),
        cx,
        move |staged, cx| match staged {
            Err(e) => fail(cx, Some(&record.id), e),
            Ok(staged) => {
                if let Err(e) = plugin_manifest::accept(&staged.manifest, CODE_ALLOWED) {
                    discard(staged.work.clone(), cx);
                    fail(cx, Some(&record.id), e.message());
                } else if staged.manifest.id != record.id {
                    discard(staged.work.clone(), cx);
                    fail(
                        cx,
                        Some(&record.id),
                        format!(
                            "The new version is a different plugin ({}).",
                            staged.manifest.id
                        ),
                    );
                } else if update_needs_a_prompt(&record, &staged.manifest) {
                    ask_then_commit(handle, f, repo, staged, true, cx);
                } else {
                    let done = Done::Updated(staged.installed.label());
                    finish_commit(f, repo, staged, done, cx);
                }
            }
        },
    );
}

/// Uninstall plugin `id` after a yes: its folder and data go, and so does a `theme` line in the
/// config that names one of its themes.
pub fn uninstall(id: &str, window: &mut Window, cx: &mut App) {
    if !can_start(cx.global::<Plugins>()) {
        return;
    }
    let Some(record) = cx
        .global::<Plugins>()
        .records
        .iter()
        .find(|r| r.id == id)
        .cloned()
    else {
        return;
    };
    let answer = window.prompt(
        PromptLevel::Warning,
        &format!("Uninstall {}?", record.name),
        Some("Its files and its data are deleted."),
        &["Uninstall", "Cancel"],
        cx,
    );
    cx.spawn(async move |cx| {
        if answer.await.ok() == Some(0) {
            cx.update(|cx| remove(record, cx));
        }
    })
    .detach();
}

/// The yes: delete off the UI thread. The theme names are taken from the list before the files
/// go, since the list is built from them.
fn remove(record: Record, cx: &mut App) {
    let Some(f) = begin(cx, Busy::Removing(record.id.clone())) else {
        return;
    };
    let appearance = cx.global::<Appearance>();
    let themes: Vec<String> = appearance
        .plugin_themes
        .iter()
        .filter(|t| t.plugin_id == record.id)
        .map(|t| t.name.clone())
        .collect();
    let config = appearance.settings_path.clone();
    let id = record.id.clone();
    let work = cx.background_executor().spawn(async move {
        plugin_store::uninstall(&f, &id)?;
        if let Some(path) = config
            && let Ok(text) = std::fs::read_to_string(&path)
            && let Some(text) = plugin_themes::config_without_theme(&text, &themes)
        {
            settings::write_atomic(&path, &text)
                .map_err(|e| format!("Couldn't update your config: {e}"))?;
        }
        Ok::<(), String>(())
    });
    cx.spawn(async move |cx| {
        let done = work.await;
        cx.update(|cx| match done {
            Ok(()) => {
                let text = success_text(&record.name, &Done::Uninstalled, record.runs_code());
                succeed(cx, &record.id, text);
            }
            Err(e) => fail(cx, Some(&record.id), e),
        });
    })
    .detach();
}

/// Turn plugin `id` On or Off. Only its record changes, so it's quick, but it still runs off
/// the UI thread like the rest.
#[allow(dead_code)] // the On/Off switch on the page (a later M11z task) is the first caller
pub fn set_enabled(id: &str, on: bool, cx: &mut App) {
    let Some(f) = begin(cx, Busy::Switching(id.to_string())) else {
        return;
    };
    let id = id.to_string();
    let work = cx
        .background_executor()
        .spawn(async move { plugin_store::set_enabled(&f, &id, on) });
    cx.spawn(async move |cx| {
        let done = work.await;
        cx.update(|cx| match done {
            Ok(record) => {
                let done = if on { Done::On } else { Done::Off };
                let text = success_text(&record.name, &done, record.runs_code());
                succeed(cx, &record.id, text);
            }
            Err(e) => fail(cx, None, e),
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sunset_manifest() -> Manifest {
        plugin_manifest::parse(
            "format = 1\nid = sunset-themes\nname = Sunset Themes\nversion = 1.0.0\n\
             description = Warm themes.\ntheme = themes/Sunset\ntheme = themes/Sunset Light\n",
        )
        .unwrap()
    }

    #[test]
    fn the_install_prompt() {
        let (title, detail, level) = install_prompt(
            &sunset_manifest(),
            &Repo {
                owner: "o".into(),
                name: "sunset".into(),
            },
        );
        assert_eq!(title, "Install Sunset Themes 1.0.0?");
        assert_eq!(
            detail,
            "From github.com/o/sunset\nWarm themes.\nAdds 2 themes: Sunset, Sunset Light"
        );
        assert_eq!(level, PromptLevel::Info);
    }

    #[test]
    fn the_prompt_warns_about_code() {
        let m = plugin_manifest::parse(
            "format = 1\nid = helper\nname = Helper\nversion = 1\nzsh = a.zsh\n",
        )
        .unwrap();
        let (_, detail, level) = install_prompt(
            &m,
            &Repo {
                owner: "o".into(),
                name: "helper".into(),
            },
        );
        assert_eq!(level, PromptLevel::Warning);
        assert_eq!(
            detail.lines().last().unwrap(),
            "Runs a program on your Mac in every new terminal. fuxx can't limit what it does, so only install plugins from people you trust."
        );
    }

    #[test]
    fn one_theme_is_singular() {
        let m = plugin_manifest::parse(
            "format = 1\nid = sunset\nname = Sunset\nversion = 1\ntheme = themes/Sunset\n",
        )
        .unwrap();
        assert_eq!(adds_text(&m), "Adds 1 theme: Sunset");
    }

    #[test]
    fn one_operation_at_a_time() {
        let mut p = Plugins::default();
        assert!(can_start(&p));
        p.busy = Some(Busy::Downloading);
        assert!(!can_start(&p));
        p.busy = None;
        p.cleaning = true;
        assert!(!can_start(&p));
        p.cleaning = false;
        assert!(can_start(&p));
    }

    fn record(zsh: &[&str]) -> Record {
        Record {
            id: "helper".into(),
            name: "Helper".into(),
            version: "1".into(),
            description: None,
            repo: "o/helper".into(),
            installed: plugin_source::Installed::Tag("v1".into()),
            themes: Vec::new(),
            installed_at: 0,
            enabled: true,
            bin: None,
            zsh: zsh.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn an_update_asks_when_code_appears() {
        let code = plugin_manifest::parse(
            "format = 1\nid = helper\nname = Helper\nversion = 2\nzsh = a.zsh\n",
        )
        .unwrap();
        assert!(update_needs_a_prompt(&record(&[]), &code));
        assert!(!update_needs_a_prompt(&record(&["a.zsh"]), &code));
        assert!(!update_needs_a_prompt(&record(&[]), &sunset_manifest()));
        assert!(!update_needs_a_prompt(
            &record(&["a.zsh"]),
            &sunset_manifest()
        ));
    }

    #[test]
    fn success_lines() {
        let more =
            " New terminals get the change; terminals already open keep it until you close them.";
        let updated = Done::Updated("v2".into());
        assert_eq!(
            success_text("Sunset", &Done::Installed, false),
            "Installed Sunset. Its themes are in Appearance."
        );
        assert_eq!(
            success_text("Sunset", &updated, false),
            "Updated Sunset to v2."
        );
        assert_eq!(
            success_text("Sunset", &Done::Uninstalled, false),
            "Uninstalled Sunset."
        );
        assert_eq!(
            success_text("Helper", &Done::Installed, true),
            format!("Installed Helper. Its themes are in Appearance.{more}")
        );
        assert_eq!(
            success_text("Helper", &updated, true),
            format!("Updated Helper to v2.{more}")
        );
        assert_eq!(
            success_text("Helper", &Done::Uninstalled, true),
            format!("Uninstalled Helper.{more}")
        );
        assert_eq!(
            success_text("Helper", &Done::On, true),
            format!("Helper is on.{more}")
        );
        assert_eq!(
            success_text("Helper", &Done::Off, true),
            format!("Helper is off.{more}")
        );
    }
}
