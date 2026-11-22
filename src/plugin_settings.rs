//! Declared plugin settings (M11au, note 94): the `setting.<name> = <type> <default> <label>`
//! lines of a plugin's `fuxx-plugin.conf`, the values the user chose (kept in fuxx's own settings
//! file as `plugin.<id>.<name> = <value>`), and which of them `plugins.zsh` exports.
//!
//! Pure text: nothing here reads or writes a file.

use crate::settings::{self, Entry};

/// What kind of value a setting holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `on` or `off`.
    Bool,
    /// A plain decimal: `3`, `0.5`, `-1`.
    Number,
}

/// One declared setting.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Setting {
    pub name: String,
    pub kind: Kind,
    pub default: String,
    /// What the page shows; the name when the line has none.
    pub label: String,
}

/// The manifest key prefix of a declared setting.
pub const KEY_PREFIX: &str = "setting.";
/// The settings-file key prefix of a stored value.
pub const CONFIG_PREFIX: &str = "plugin.";
/// The environment name prefix of an exported value.
pub const ENV_PREFIX: &str = "FUXX_PLUGIN_SETTING_";

/// A setting's name: `a-z`, `0-9` and `_`, starting with a letter, 1 to 40 characters, so the
/// exported name is always a valid shell name.
pub fn valid_name(name: &str) -> bool {
    (1..=40).contains(&name.len())
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A plain decimal: an optional `-`, digits, and an optional `.` with digits.
pub fn is_number(text: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let t = text.strip_prefix('-').unwrap_or(text);
    match t.split_once('.') {
        Some((whole, frac)) => digits(whole) && digits(frac),
        None => digits(t),
    }
}

/// True when `value` is a value of `kind`.
pub fn fits(kind: Kind, value: &str) -> bool {
    match kind {
        Kind::Bool => value == "on" || value == "off",
        Kind::Number => is_number(value),
    }
}

/// True when `a` and `b` are the same value of `kind` (numbers as numbers: `3.0` is `3`).
pub fn same(kind: Kind, a: &str, b: &str) -> bool {
    match kind {
        Kind::Bool => a == b,
        Kind::Number => match (a.parse::<f64>(), b.parse::<f64>()) {
            (Ok(x), Ok(y)) => x == y,
            _ => a == b,
        },
    }
}

/// One `setting.<name>` line's value (`<type> <default> <label>`), or `None` when it's malformed.
pub fn parse_line(name: &str, value: &str) -> Option<Setting> {
    if !valid_name(name) {
        return None;
    }
    let value = value.trim_start();
    let (kind, rest) = value.split_once(char::is_whitespace).unwrap_or((value, ""));
    let kind = match kind {
        "bool" => Kind::Bool,
        "number" => Kind::Number,
        _ => return None,
    };
    let rest = rest.trim_start();
    let (default, label) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    if !fits(kind, default) {
        return None;
    }
    let label = label.trim();
    Some(Setting {
        name: name.to_string(),
        kind,
        default: default.to_string(),
        label: if label.is_empty() { name } else { label }.to_string(),
    })
}

/// The declared settings in a manifest's entries, in file order. A malformed line is skipped; a
/// name declared twice keeps its first place and its last line.
pub fn parse(entries: &[Entry]) -> Vec<Setting> {
    let mut out: Vec<Setting> = Vec::new();
    for e in entries {
        let Some(name) = e.key.strip_prefix(KEY_PREFIX) else {
            continue;
        };
        let Some(s) = parse_line(name, &e.value) else {
            continue;
        };
        match out.iter_mut().find(|o| o.name == s.name) {
            Some(o) => *o = s,
            None => out.push(s),
        }
    }
    out
}

/// The settings-file key of plugin `id`'s setting `name`.
pub fn config_key(id: &str, name: &str) -> String {
    format!("{CONFIG_PREFIX}{id}.{name}")
}

/// The name a value is exported as: `FUXX_PLUGIN_SETTING_<NAME>`.
pub fn env_name(name: &str) -> String {
    format!("{ENV_PREFIX}{}", name.to_ascii_uppercase())
}

/// The value the user stored for `id`'s `s`, if it is one of its kind (the last line wins).
fn stored<'a>(values: &'a [Entry], id: &str, s: &Setting) -> Option<&'a str> {
    let key = config_key(id, &s.name);
    values
        .iter()
        .rev()
        .find(|e| e.key == key)
        .map(|e| e.value.as_str())
        .filter(|v| fits(s.kind, v))
}

/// What the page shows for `id`'s `s`: the stored value, else the default.
pub fn current(values: &[Entry], id: &str, s: &Setting) -> String {
    stored(values, id, s).unwrap_or(&s.default).to_string()
}

/// What `plugins.zsh` exports for plugin `id`: `(env name, value)` for each declared setting whose
/// stored value fits its kind and differs from the default, in declaration order.
pub fn changed(values: &[Entry], id: &str, declared: &[Setting]) -> Vec<(String, String)> {
    declared
        .iter()
        .filter_map(|s| {
            let v = stored(values, id, s)?;
            (!same(s.kind, v, &s.default)).then(|| (env_name(&s.name), v.to_string()))
        })
        .collect()
}

/// The lines to write for `s` set to `value`: none (the line goes) for the default.
pub fn values_for(s: &Setting, value: &str) -> Vec<String> {
    if same(s.kind, value, &s.default) {
        Vec::new()
    } else {
        vec![value.to_string()]
    }
}

/// The `plugin.*` entries of a settings file's entries: the stored plugin values.
pub fn plugin_values(entries: &[Entry]) -> Vec<Entry> {
    entries
        .iter()
        .filter(|e| e.key.starts_with(CONFIG_PREFIX))
        .cloned()
        .collect()
}

/// `config` without plugin `id`'s `plugin.<id>.*` lines, or `None` when it has none.
pub fn config_without_plugin(config: &str, id: &str) -> Option<String> {
    let prefix = format!("{CONFIG_PREFIX}{id}.");
    let keys: Vec<String> = settings::parse(config)
        .into_iter()
        .map(|e| e.key)
        .filter(|k| k.starts_with(&prefix))
        .collect();
    if keys.is_empty() {
        return None;
    }
    Some(keys.iter().fold(config.to_string(), |text, k| {
        settings::set_key(&text, k, &[])
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, value: &str) -> Entry {
        Entry {
            key: key.into(),
            value: value.into(),
        }
    }

    fn setting(name: &str, kind: Kind, default: &str) -> Setting {
        Setting {
            name: name.into(),
            kind,
            default: default.into(),
            label: name.into(),
        }
    }

    /// fuxx-completes' four lines, as the spec writes them.
    const EXAMPLE: &str = "setting.list = bool on  Show a suggestion list when you pause\n\
                           setting.list_empty = bool on  Show the list on an empty line too\n\
                           setting.list_delay = number 3 Seconds of pause before the list shows\n\
                           setting.list_size = number 5 Items in the list (1–5)\n";

    #[test]
    fn parses_the_specs_four_lines() {
        let got = parse(&settings::parse(EXAMPLE));
        assert_eq!(
            got,
            [
                Setting {
                    name: "list".into(),
                    kind: Kind::Bool,
                    default: "on".into(),
                    label: "Show a suggestion list when you pause".into(),
                },
                Setting {
                    name: "list_empty".into(),
                    kind: Kind::Bool,
                    default: "on".into(),
                    label: "Show the list on an empty line too".into(),
                },
                Setting {
                    name: "list_delay".into(),
                    kind: Kind::Number,
                    default: "3".into(),
                    label: "Seconds of pause before the list shows".into(),
                },
                Setting {
                    name: "list_size".into(),
                    kind: Kind::Number,
                    default: "5".into(),
                    label: "Items in the list (1–5)".into(),
                },
            ]
        );
    }

    #[test]
    fn malformed_lines_are_skipped() {
        for (name, value) in [
            ("list", "text on Label"),          // unknown type
            ("list", "bool yes Label"),         // a bool is on or off
            ("list", "bool"),                   // no default
            ("delay", "number 3s Label"),       // not a number
            ("delay", "number . Label"),        // not a number
            ("delay", "number 1e3 Label"),      // not a plain decimal
            ("delay", "number -.5 Label"),      // digits before the point
            ("List", "bool on Label"),          // upper case
            ("list-size", "number 5 Label"),    // a dash can't be exported
            ("1list", "bool on Label"),         // starts with a digit
            ("", "bool on Label"),              // no name
            (&"a".repeat(41), "bool on Label"), // too long
            ("list", ""),                       // nothing
        ] {
            assert_eq!(parse_line(name, value), None, "{name} = {value}");
        }
        assert!(parse_line(&"a".repeat(40), "bool on x").is_some());
        assert!(parse_line("delay", "number -1.25 x").is_some());
        assert!(parse_line("delay", "number 0.5 x").is_some());
    }

    #[test]
    fn a_missing_label_is_the_name_and_extra_spaces_are_fine() {
        assert_eq!(
            parse_line("list", "bool off"),
            Some(setting("list", Kind::Bool, "off"))
        );
        let s = parse_line("list", "  bool   on    Show   it  ").unwrap();
        assert_eq!(s.default, "on");
        assert_eq!(s.label, "Show   it");
    }

    #[test]
    fn the_last_line_of_a_name_wins_in_its_first_place() {
        let got = parse(&settings::parse(
            "setting.a = bool on A\nsetting.b = number 1 B\nsetting.a = bool off A2\nsetting.a = bool maybe A3\n",
        ));
        let names: Vec<(&str, &str)> = got
            .iter()
            .map(|s| (s.name.as_str(), s.default.as_str()))
            .collect();
        // The malformed third `a` is skipped, so the second one stands.
        assert_eq!(names, [("a", "off"), ("b", "1")]);
    }

    #[test]
    fn other_keys_are_not_settings() {
        assert!(
            parse(&settings::parse(
                "settings.a = bool on x\nsetting = bool on x\n"
            ))
            .is_empty()
        );
    }

    #[test]
    fn names() {
        assert_eq!(
            config_key("fuxx-completes", "list_delay"),
            "plugin.fuxx-completes.list_delay"
        );
        assert_eq!(env_name("list_delay"), "FUXX_PLUGIN_SETTING_LIST_DELAY");
    }

    #[test]
    fn only_changed_values_that_fit_are_exported() {
        let declared = [
            setting("list", Kind::Bool, "on"),
            setting("list_empty", Kind::Bool, "on"),
            setting("list_delay", Kind::Number, "3"),
            setting("list_size", Kind::Number, "5"),
        ];
        let values = [
            entry("plugin.fc.list", "off"),
            entry("plugin.fc.list_empty", "on"), // the default: not exported
            entry("plugin.fc.list_delay", "3.0"), // the default as a number
            entry("plugin.fc.list_size", "many"), // doesn't fit: ignored
            entry("plugin.fc.gone", "1"),        // not declared any more
            entry("plugin.other.list", "off"),   // another plugin's
        ];
        assert_eq!(
            changed(&values, "fc", &declared),
            [("FUXX_PLUGIN_SETTING_LIST".to_string(), "off".to_string())]
        );
        // The last line wins, and a later junk line doesn't hide an earlier good one.
        let values = [
            entry("plugin.fc.list_delay", "1"),
            entry("plugin.fc.list_delay", "1.5"),
        ];
        assert_eq!(
            changed(&values, "fc", &declared),
            [(
                "FUXX_PLUGIN_SETTING_LIST_DELAY".to_string(),
                "1.5".to_string()
            )]
        );
        assert!(changed(&[], "fc", &declared).is_empty());
    }

    #[test]
    fn the_page_shows_the_stored_value_or_the_default() {
        let s = setting("list_delay", Kind::Number, "3");
        assert_eq!(current(&[], "fc", &s), "3");
        assert_eq!(
            current(&[entry("plugin.fc.list_delay", "2")], "fc", &s),
            "2"
        );
        assert_eq!(
            current(&[entry("plugin.fc.list_delay", "x")], "fc", &s),
            "3"
        );
    }

    #[test]
    fn the_default_removes_the_line() {
        let b = setting("list", Kind::Bool, "on");
        assert!(values_for(&b, "on").is_empty());
        assert_eq!(values_for(&b, "off"), ["off"]);
        let n = setting("list_delay", Kind::Number, "3");
        assert!(values_for(&n, "3.00").is_empty());
        assert_eq!(values_for(&n, "0.5"), ["0.5"]);
    }

    #[test]
    fn plugin_values_are_the_plugin_lines() {
        let entries = settings::parse("theme = x\nplugin.fc.list = off\nfont-size = 14\n");
        assert_eq!(plugin_values(&entries), [entry("plugin.fc.list", "off")]);
    }

    #[test]
    fn uninstall_removes_only_that_plugins_lines() {
        let config = "# mine\ntheme = Sunset\nplugin.fc.list = off\nplugin.fc-two.list = off\n\
                      font-size = 14\nplugin.fc.list_delay = 1\n";
        assert_eq!(
            config_without_plugin(config, "fc").as_deref(),
            Some("# mine\ntheme = Sunset\nplugin.fc-two.list = off\nfont-size = 14\n")
        );
        assert_eq!(config_without_plugin("theme = x\n", "fc"), None);
    }
}
