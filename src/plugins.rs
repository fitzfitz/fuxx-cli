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
use crate::plugin_source::{self, Installed, LINK_HINT, Repo};
use crate::plugin_store::{self, Folders, Record};
use crate::plugin_themes;
use crate::settings;

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

/// The install sheet's title and detail.
pub fn install_prompt(m: &Manifest, repo: &Repo) -> (String, String) {
    let title = format!("Install {} {}?", m.name, m.version);
    let mut lines = vec![format!(
        "From {}",
        repo.page().trim_start_matches("https://")
    )];
    if let Some(d) = &m.description {
        lines.push(d.clone());
    }
    lines.push(adds_text(m));
    (title, lines.join("\n"))
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
    let records = folders(cx).map(|f| plugin_store::load(&f));
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
        let mend = cx.background_executor().spawn(async move {
            plugin_store::launch_cleanup(&f);
            plugin_store::load(&f)
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
    let (title, detail) = install_prompt(&staged.manifest, &repo);
    let work = staged.work.clone();
    let asked = handle.update(cx, move |_, window, cx| {
        let answer = window.prompt(
            PromptLevel::Info,
            &title,
            Some(&detail),
            &["Install", "Cancel"],
            cx,
        );
        cx.spawn(async move |cx| {
            let yes = answer.await.ok() == Some(0);
            cx.update(|cx| {
                if yes {
                    finish_commit(f, repo, staged, "Installed", cx);
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

/// Move the staged plugin into place and report it. `verb` is "Installed" or "Updated". The
/// move (which deletes an old version) and the scratch folder's removal run off the UI thread.
fn finish_commit(f: Folders, repo: Repo, staged: Staged, verb: &'static str, cx: &mut App) {
    let label = staged.installed.label();
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
        let done = work.await;
        cx.update(|cx| match done {
            Ok(record) => {
                let text = if verb == "Installed" {
                    format!("Installed {}. Its themes are in Appearance.", record.name)
                } else {
                    format!("Updated {} to {label}.", record.name)
                };
                succeed(cx, &record.id, text);
            }
            Err(e) => fail(cx, Some(&id), e),
        });
    })
    .detach();
}

/// Update plugin `id` to its latest version. The click is the consent, so there is no sheet.
pub fn update(id: &str, _window: &mut Window, cx: &mut App) {
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
                } else {
                    finish_commit(f, repo, staged, "Updated", cx);
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
            Ok(()) => succeed(cx, &record.id, format!("Uninstalled {}.", record.name)),
            Err(e) => fail(cx, Some(&record.id), e),
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
        let (title, detail) = install_prompt(
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
}
