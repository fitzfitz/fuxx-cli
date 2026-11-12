//! Key sounds (M11s): a sound you set per key plays when you press that key in a terminal.
//!
//! The pure parts (which key a press is, what a link is, where a web link's copy is kept) are
//! tested here; `Sounds` (a GPUI global) loads each link into an `NSSound` once, downloading a web
//! link first off the UI thread, and plays it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gpui::{App, Global};
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_app_kit::NSSound;
use objc2_foundation::NSString;

use crate::appearance::Appearance;

/// A key that can have a sound, in Settings' order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundKey {
    Enter,
    CtrlC,
    /// Not a key: the sound with every banner fuxx posts (M11x).
    Banner,
}

impl SoundKey {
    pub const ALL: [SoundKey; 3] = [SoundKey::Enter, SoundKey::CtrlC, SoundKey::Banner];

    /// Its settings-file key.
    pub fn key(self) -> &'static str {
        match self {
            SoundKey::Enter => "sound-enter",
            SoundKey::CtrlC => "sound-ctrl-c",
            SoundKey::Banner => "sound-banner",
        }
    }

    /// Its On / Off switch's settings-file key (on by default).
    pub fn switch_key(self) -> &'static str {
        match self {
            SoundKey::Enter => "sound-enter-on",
            SoundKey::CtrlC => "sound-ctrl-c-on",
            SoundKey::Banner => "sound-banner-on",
        }
    }

    /// Its name in Settings.
    pub fn label(self) -> &'static str {
        match self {
            SoundKey::Enter => "Enter",
            SoundKey::CtrlC => "Ctrl + C",
            SoundKey::Banner => "Banner sound",
        }
    }
}

/// The key a press is, from the bytes it sends to the terminal: `\r` (either Enter) and `^C`.
pub fn key_for_bytes(bytes: &[u8]) -> Option<SoundKey> {
    match bytes {
        b"\r" => Some(SoundKey::Enter),
        [0x03] => Some(SoundKey::CtrlC),
        _ => None,
    }
}

/// What a sound's link is: a file on this Mac, or a web link to download once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    File(PathBuf),
    Web(String),
}

impl Link {
    /// The link typed in Settings: trimmed; `http(s)://…` is a web link; `~/…` is under `home`;
    /// anything else a path as typed. Blank: none.
    pub fn parse(text: &str, home: Option<&Path>) -> Option<Link> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let lower = text.to_ascii_lowercase();
        if lower.starts_with("https://") || lower.starts_with("http://") {
            return Some(Link::Web(text.to_string()));
        }
        let path = match (text.strip_prefix("~/"), home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ if text == "~" => home.map_or_else(|| PathBuf::from(text), Path::to_path_buf),
            _ => PathBuf::from(text),
        };
        Some(Link::File(path))
    }
}

/// Where a web link's copy is kept under `cache`: named from the link (FNV-1a), keeping its
/// file's extension when it has a short one (NSSound reads the type from it).
pub fn cache_file(cache: &Path, url: &str) -> PathBuf {
    let hash = url.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    let name = url
        .split(['?', '#'])
        .next()
        .and_then(|u| u.rsplit('/').next())
        .unwrap_or("");
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| (1..=4).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric()));
    match ext {
        Some(ext) => cache.join(format!("{hash:016x}.{ext}")),
        None => cache.join(format!("{hash:016x}")),
    }
}

/// The folder web links' copies are kept in.
pub fn cache_dir(home: &Path) -> PathBuf {
    home.join("Library/Caches/fuxx/sounds")
}

/// curl's arguments for fetching `url` into `to`: fail on an HTTP error, follow redirects, 20 s
/// at most.
pub fn download_args(url: &str, to: &Path) -> Vec<OsString> {
    vec![
        "-fsSL".into(),
        "--max-time".into(),
        "20".into(),
        "-o".into(),
        to.into(),
        "--url".into(),
        url.into(),
    ]
}

/// The background service's banner (M11x, after quit): the sound file the settings name for
/// banners, if it's switched on and there is one to play. A file link as written (under `home`
/// for `~/`); a web link only once the app has downloaded its copy (`exists`), else none.
pub fn banner_file(
    entries: &[crate::settings::Entry],
    home: &Path,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let key = SoundKey::Banner;
    let (on, _) = crate::appearance::switch_from_entries(entries, key.switch_key(), true);
    if !on {
        return None;
    }
    let text = entries.iter().rev().find(|e| e.key == key.key())?;
    match Link::parse(&text.value, Some(home))? {
        Link::File(path) => Some(path),
        Link::Web(url) => Some(cache_file(&cache_dir(home), &url)).filter(|f| exists(f)),
    }
}

/// `afplay`'s arguments for playing `file` once.
pub fn afplay_args(file: &Path) -> Vec<OsString> {
    vec![file.into()]
}

/// The background service just posted a banner: play the banner's sound with macOS's `afplay`
/// (the service has no app to load an `NSSound` in), waited for so no finished `afplay` is left
/// behind in the long-lived service (call it off the service's own threads: the banner's thread
/// is). Silent when nothing is set, the settings can't be read or the file can't play.
pub fn play_banner_detached() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let path = crate::settings::settings_path(&home, xdg.as_deref());
    let read = |p: &Path| std::fs::read_to_string(p).ok();
    let entries = crate::settings::load(std::slice::from_ref(&path), &read);
    let Some(file) = banner_file(&entries, &home, &|p: &Path| p.exists()) else {
        return;
    };
    let _ = std::process::Command::new("/usr/bin/afplay")
        .args(afplay_args(&file))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// The settings-file lines for a link typed in Settings: none (the line removed) when blank.
pub fn values(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        vec![]
    } else {
        vec![text.to_string()]
    }
}

/// The settings-file lines for a key's switch: none for On (the default), `off` for Off.
pub fn switch_values(on: bool) -> Vec<String> {
    if on { vec![] } else { vec!["off".into()] }
}

/// The problem shown under a field.
pub const NO_FILE: &str = "No such file.";
pub const NOT_A_SOUND: &str = "Not a sound macOS can play.";
pub const DOWNLOAD_FAILED: &str = "Couldn't download it.";
pub const DOWNLOADING: &str = "Downloading\u{2026}";
/// Under an empty field: what goes in it.
pub const HINT: &str = "A file or https:// link";

/// A key's sound as loaded: what was typed, and where it got to.
enum State {
    /// No link.
    None,
    /// A web link being fetched.
    Loading,
    Ready(Retained<NSSound>),
    /// Why it can't play (shown under the field).
    Failed(&'static str),
}

/// Each key's sound, kept loaded (`SoundKey::ALL` order), and a number per key that a change of
/// its link bumps, so a download started for an old link is dropped.
pub struct Sounds {
    slots: [(String, State, u64); SoundKey::ALL.len()],
}

impl Global for Sounds {}

fn slot(key: SoundKey) -> usize {
    key as usize
}

/// Load the links in the settings (at launch and after every settings change): a key whose
/// link changed is loaded again (a file at once; a web link from its copy, or fetched first).
pub fn sync(cx: &mut App) {
    if cx.try_global::<Sounds>().is_none() {
        cx.set_global(Sounds {
            slots: SoundKey::ALL.map(|_| (String::new(), State::None, 0)),
        });
    }
    let links = cx.global::<Appearance>().sound_links.clone();
    for key in SoundKey::ALL {
        let text = links[slot(key)].clone();
        if cx.global::<Sounds>().slots[slot(key)].0 == text {
            continue;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let generation = cx.global::<Sounds>().slots[slot(key)].2 + 1;
        let state = match Link::parse(&text, home.as_deref()) {
            None => State::None,
            Some(Link::File(path)) => load(&path),
            Some(Link::Web(url)) => match home {
                None => State::Failed(DOWNLOAD_FAILED),
                Some(home) => {
                    let file = cache_file(&cache_dir(&home), &url);
                    if file.exists() {
                        load(&file)
                    } else {
                        fetch(key, generation, url, file, cx);
                        State::Loading
                    }
                }
            },
        };
        cx.global_mut::<Sounds>().slots[slot(key)] = (text, state, generation);
    }
}

/// A file's sound, read into memory (`byReference: NO`), or why it can't be.
fn load(path: &Path) -> State {
    if !path.is_file() {
        return State::Failed(NO_FILE);
    }
    let path = NSString::from_str(&path.to_string_lossy());
    match NSSound::initWithContentsOfFile_byReference(NSSound::alloc(), &path, false) {
        Some(sound) => State::Ready(sound),
        None => State::Failed(NOT_A_SOUND),
    }
}

/// Download `url` into `file` off the UI thread (into a `.part` first, so a half-written copy
/// is never played), then load it if the key's link is still the one it was for.
fn fetch(key: SoundKey, generation: u64, url: String, file: PathBuf, cx: &mut App) {
    let work = cx.background_executor().spawn(async move {
        let part = file.with_extension("part");
        let fetched = file
            .parent()
            .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
            && std::process::Command::new("/usr/bin/curl")
                .args(download_args(&url, &part))
                .status()
                .is_ok_and(|s| s.success())
            && std::fs::rename(&part, &file).is_ok();
        if !fetched {
            let _ = std::fs::remove_file(&part);
        }
        fetched.then_some(file)
    });
    cx.spawn(async move |cx| {
        let file = work.await;
        cx.update(|cx| {
            if cx.global::<Sounds>().slots[slot(key)].2 != generation {
                return; // its link changed while it downloaded
            }
            let state = file.map_or(State::Failed(DOWNLOAD_FAILED), |f| load(&f));
            cx.global_mut::<Sounds>().slots[slot(key)].1 = state;
        });
    })
    .detach();
}

/// Play `key`'s sound from its start (a sound still playing starts again). Nothing when it has
/// none or it isn't ready.
pub fn play(key: SoundKey, cx: &App) {
    let Some(sounds) = cx.try_global::<Sounds>() else {
        return;
    };
    if let State::Ready(sound) = &sounds.slots[slot(key)].1 {
        sound.stop();
        sound.play();
    }
}

/// fuxx just posted a banner (M11x): the banner's sound, unless it's switched off.
pub fn banner(cx: &App) {
    if cx.global::<Appearance>().sound_on[SoundKey::Banner as usize] {
        play(SoundKey::Banner, cx);
    }
}

/// A press that sent `bytes` to a terminal: its key's sound, if it has one.
pub fn press(bytes: &[u8], cx: &App) {
    let Some(key) = key_for_bytes(bytes) else {
        return;
    };
    // Switched off: silent (its link is kept; ▶ still plays it).
    if cx.global::<Appearance>().sound_on[key as usize] {
        play(key, cx);
    }
}

/// What to say under `key`'s field: why it can't play, or that it is downloading.
pub fn problem(key: SoundKey, cx: &App) -> Option<&'static str> {
    match &cx.try_global::<Sounds>()?.slots[slot(key)].1 {
        State::Failed(why) => Some(why),
        State::Loading => Some(DOWNLOADING),
        State::None | State::Ready(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_service_s_banner_sound_is_the_file_the_settings_name() {
        use crate::settings::Entry;
        let e = |key: &str, value: &str| Entry {
            key: key.into(),
            value: value.into(),
        };
        let home = Path::new("/Users/me");
        let yes = |_: &Path| true;
        let no = |_: &Path| false;
        // None set, a blank line, switched off: silent.
        assert_eq!(banner_file(&[], home, &yes), None);
        assert_eq!(banner_file(&[e("sound-banner", "  ")], home, &yes), None);
        assert_eq!(
            banner_file(
                &[e("sound-banner", "~/a.wav"), e("sound-banner-on", "off")],
                home,
                &yes
            ),
            None
        );
        // A file: under home for `~/`; the last line wins.
        assert_eq!(
            banner_file(
                &[e("sound-banner", "/b.aiff"), e("sound-banner", "~/a.wav")],
                home,
                &no
            ),
            Some(PathBuf::from("/Users/me/a.wav"))
        );
        // A web link: the app's downloaded copy, if there is one yet.
        let url = "https://x.io/chime.wav";
        let copy = cache_file(&cache_dir(home), url);
        assert_eq!(
            banner_file(&[e("sound-banner", url)], home, &|p: &Path| p == copy),
            Some(copy.clone())
        );
        assert_eq!(banner_file(&[e("sound-banner", url)], home, &no), None);
        assert_eq!(
            afplay_args(Path::new("/x.wav")),
            vec![OsString::from("/x.wav")]
        );
    }

    #[test]
    fn the_banner_s_sound_has_its_own_keys_and_no_key_press() {
        assert_eq!(SoundKey::ALL.len(), 3);
        assert_eq!(SoundKey::Banner.key(), "sound-banner");
        assert_eq!(SoundKey::Banner.switch_key(), "sound-banner-on");
        assert_eq!(SoundKey::Banner.label(), "Banner sound");
        for bytes in [&b"\r"[..], &[0x03], b"b", b"\x07"] {
            assert_ne!(key_for_bytes(bytes), Some(SoundKey::Banner));
        }
    }

    #[test]
    fn enter_and_ctrl_c_are_known_by_the_bytes_they_send() {
        assert_eq!(key_for_bytes(b"\r"), Some(SoundKey::Enter));
        assert_eq!(key_for_bytes(&[0x03]), Some(SoundKey::CtrlC));
        assert_eq!(key_for_bytes(b"a"), None);
        assert_eq!(key_for_bytes(b"\x1b[A"), None);
        assert_eq!(key_for_bytes(b"\r\n"), None);
        assert_eq!(
            SoundKey::ALL.map(SoundKey::key),
            ["sound-enter", "sound-ctrl-c", "sound-banner"]
        );
        assert_eq!(
            SoundKey::ALL.map(SoundKey::label),
            ["Enter", "Ctrl + C", "Banner sound"]
        );
        assert_eq!(
            SoundKey::ALL.map(SoundKey::switch_key),
            ["sound-enter-on", "sound-ctrl-c-on", "sound-banner-on"]
        );
        // On is the default: no line; off writes one.
        assert!(switch_values(true).is_empty());
        assert_eq!(switch_values(false), ["off"]);
    }

    #[test]
    fn a_link_is_a_file_or_a_web_link() {
        let home = Some(Path::new("/Users/me"));
        assert_eq!(Link::parse("  ", home), None);
        assert_eq!(
            Link::parse(" ~/Sounds/click.wav ", home),
            Some(Link::File("/Users/me/Sounds/click.wav".into()))
        );
        assert_eq!(
            Link::parse("/System/Library/Sounds/Tink.aiff", home),
            Some(Link::File("/System/Library/Sounds/Tink.aiff".into()))
        );
        assert_eq!(
            Link::parse("https://x.io/a.mp3", home),
            Some(Link::Web("https://x.io/a.mp3".into()))
        );
        assert_eq!(
            Link::parse("HTTP://x.io/a", home),
            Some(Link::Web("HTTP://x.io/a".into()))
        );
        assert_eq!(
            Link::parse("~/a.wav", None),
            Some(Link::File("~/a.wav".into()))
        );
    }

    #[test]
    fn a_web_link_s_copy_is_named_from_it_and_keeps_its_extension() {
        let cache = Path::new("/c");
        let a = cache_file(cache, "https://x.io/s/click.MP3?v=2");
        assert_eq!(a.extension().and_then(|e| e.to_str()), Some("mp3"));
        assert_eq!(a, cache_file(cache, "https://x.io/s/click.MP3?v=2"));
        assert_ne!(a, cache_file(cache, "https://x.io/s/click.MP3?v=3"));
        assert_eq!(cache_file(cache, "https://x.io/sound").extension(), None);
        assert_eq!(
            cache_file(cache, "https://x.io/a.toolong").extension(),
            None
        );
        assert_eq!(
            cache_dir(Path::new("/Users/me")),
            Path::new("/Users/me/Library/Caches/fuxx/sounds")
        );
        let args = download_args("https://x.io/a.wav", Path::new("/c/f.wav"));
        assert_eq!(
            args,
            [
                "-fsSL",
                "--max-time",
                "20",
                "-o",
                "/c/f.wav",
                "--url",
                "https://x.io/a.wav"
            ]
            .map(OsString::from)
        );
    }

    /// What macOS can play loads; a missing file and a file that isn't a sound say why.
    #[test]
    fn a_file_loads_as_a_sound_or_says_why_not() {
        let tink = Path::new("/System/Library/Sounds/Tink.aiff");
        if tink.exists() {
            assert!(matches!(load(tink), State::Ready(_)), "a macOS sound");
        }
        assert!(matches!(
            load(Path::new("/no/such/sound.wav")),
            State::Failed(NO_FILE)
        ));
        let not_sound = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(matches!(load(&not_sound), State::Failed(NOT_A_SOUND)));
    }

    #[test]
    fn a_blank_link_removes_its_line() {
        assert!(values("  ").is_empty());
        assert_eq!(values(" ~/a.wav "), ["~/a.wav"]);
    }
}
