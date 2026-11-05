//! M10c: the JavaScript fuxx runs inside a browser page, and the page outline it reads back.
//! No GPUI, no WebKit: plain strings, a parser and a renderer.
//!
//! Every script is one expression, ready for `evaluateJavaScript`, whose value is plain JSON
//! data (the caller turns it into text with `JSON.stringify`). Text from the agent goes in only
//! as JSON literals, as the arguments of a fixed function, never pasted into the code itself.
//! Refs live on the elements as `data-fuxx-ref="eN"`: each snapshot clears and renumbers them,
//! and a new page has none, so a stale ref is "unknown" rather than some other element.

use serde::{Deserialize, Serialize};

/// The outline shows at most this many nodes (the snapshot script stops there too).
const MAX_NODES: usize = 1000;
/// A name, text block or value is cut to this many characters.
const MAX_TEXT: usize = 200;

/// What `snapshot_js` answers with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub url: String,
    pub title: String,
    pub nodes: Vec<Node>,
}

/// One line of the outline: a heading, a landmark, a block of text, an image or a control.
/// Only controls have a ref; only headings have a level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub role: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
}

/// Number the page's visible controls and describe the page; answers a `Snapshot`.
///
/// Walks the DOM in document order, skipping `display:none` and `aria-hidden="true"` subtrees
/// and `<script>`, `<style>`, `<noscript>`, `<template>`, `<svg>`. An element is listed only
/// when it is visible (not `visibility:hidden`, not zero-sized). Controls are links, buttons,
/// fields, `select`s, elements with a control `role`, `[onclick]` and `[contenteditable]`; each
/// gets the next ref. Names: `aria-label`, `aria-labelledby`, a `<label>`, a placeholder, `alt`,
/// the element's text, `title` (80 characters; a heading's 200). Field values come along (a password as `•••`,
/// a checkbox as `checked`/`unchecked`). Text blocks are block-level elements outside controls
/// and headings, with the text of their inline content (200 characters). Same-origin frames
/// and shadow roots are not entered.
pub fn snapshot_js() -> &'static str {
    SNAPSHOT_JS
}

/// Click the element with ref `r` (scrolled into view; pointer and mouse down/up first, so
/// menus that open on press work, then `click()`). Answers `{ok: true}` or
/// `{error: "unknown ref <r>"}`.
pub fn click_js(r: &str) -> String {
    call(CLICK_FN, &[literal(r)])
}

/// Put `text` into the field with ref `r`: a text field's value goes through the native setter
/// (so React and friends notice), then `input` and `change` fire; a `select` picks the option
/// whose value or label is `text`; a contenteditable gets it as its text. Answers `{ok: true}`,
/// `{error: "unknown ref <r>"}` or another `{error}` (not a text field, no such option).
pub fn fill_js(r: &str, text: &str) -> String {
    call(FILL_FN, &[literal(r), literal(text)])
}

/// Press `key` (`Enter`, `Tab`, `Escape`, `Backspace`, `ArrowUp`/`Down`/`Left`/`Right` or one
/// character) in the focused element: keydown, keypress (characters and Enter), keyup. Unless
/// the page prevents it, a character or Backspace edits a text field, Tab moves the focus, and
/// Enter submits the form of a focused `input` (through its submit button when it has one),
/// adds a line to a textarea or clicks a focused link or button. Answers `{ok: true}` or
/// `{error}` for a key it doesn't know.
pub fn press_js(key: &str) -> String {
    call(PRESS_FN, &[literal(key)])
}

/// The page's visible text (`document.body.innerText`), at most 100,000 characters.
pub fn read_js() -> &'static str {
    READ_JS
}

/// `true` once the page shows `text` (in its visible text) and has an element matching
/// `selector`; a missing condition holds. An invalid selector throws, so the caller gets the
/// page's error at once instead of waiting out the time limit.
pub fn wait_check_js(text: Option<&str>, selector: Option<&str>) -> String {
    call(WAIT_FN, &[literal(&text), literal(&selector)])
}

/// The document-start user script that makes `window.open(url)` load `url` in the same window
/// (only `http`/`https`; `window.open()` and `about:blank` stay put and hand back the window).
pub fn window_open_shim() -> &'static str {
    WINDOW_OPEN_SHIM
}

/// The snapshot as text for an agent: a `title:` and a `url:` line, then one line per node.
/// Controls read `[eN] role "name" = "value"`, headings `heading "name" [level=N]`; a heading
/// is indented by its level and the nodes under it one step further. Every line is one line
/// (line breaks in names become spaces) and each text but the URL is cut to 200 characters; past 1,000
/// nodes a last line says how many were left out.
pub fn render_outline(s: &Snapshot) -> String {
    let mut lines = vec![
        format!("title: {}", clipped(&s.title)),
        format!("url: {}", one_line(&s.url)),
    ];
    let mut depth = 0;
    for node in s.nodes.iter().take(MAX_NODES) {
        let role = clipped(&node.role);
        let mut line = String::new();
        let level = (role == "heading").then(|| node.level.unwrap_or(2).clamp(1, 6) as usize);
        if let Some(level) = level {
            depth = level;
        }
        let indent = level.map_or(depth, |l| l - 1);
        line.push_str(&"  ".repeat(indent));
        if let Some(r) = &node.r#ref {
            line.push_str(&format!("[{}] ", clipped(r)));
        }
        line.push_str(&role);
        if !node.name.is_empty() {
            line.push_str(&format!(" {}", quoted(&node.name)));
        }
        if let Some(level) = level {
            line.push_str(&format!(" [level={level}]"));
        }
        if let Some(value) = &node.value {
            line.push_str(&format!(" = {}", quoted(value)));
        }
        lines.push(line);
    }
    if s.nodes.len() > MAX_NODES {
        lines.push(format!("… {} more not shown", s.nodes.len() - MAX_NODES));
    }
    lines.join("\n")
}

/// `text` on one line: runs of whitespace and control characters become one space.
fn one_line(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `text` on one line, cut to `MAX_TEXT` characters (the last one an ellipsis).
fn clipped(text: &str) -> String {
    let joined = one_line(text);
    if joined.chars().count() <= MAX_TEXT {
        return joined;
    }
    let mut cut: String = joined.chars().take(MAX_TEXT - 1).collect();
    cut.push('…');
    cut
}

/// `text` clipped, in double quotes, with `"` and `\` escaped.
fn quoted(text: &str) -> String {
    let escaped = clipped(text).replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// A JSON literal for `value`: the only way agent text gets into a script.
fn literal<T: Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).expect("strings and nulls always serialize")
}

/// `(function)(arg, …)`: a fixed function applied to JSON-literal arguments.
fn call(function: &str, args: &[String]) -> String {
    format!("({function})({})", args.join(", "))
}

const SNAPSHOT_JS: &str = r#"(() => {
  const MAX_NODES = 1000;
  const SKIP = new Set(['script', 'style', 'noscript', 'template', 'head', 'svg']);
  const CONTROLS = new Set(['button', 'link', 'textbox', 'searchbox', 'checkbox', 'radio',
    'switch', 'combobox', 'listbox', 'slider', 'spinbutton', 'tab', 'menuitem',
    'menuitemcheckbox', 'menuitemradio', 'option', 'treeitem']);
  const LANDMARKS = new Set(['banner', 'navigation', 'main', 'contentinfo', 'complementary',
    'search', 'form', 'region']);
  const NOT_TYPED = ['button', 'submit', 'reset', 'image', 'file', 'hidden'];
  const styles = new Map();
  const styleOf = (el) => {
    let s = styles.get(el);
    if (!s) { s = getComputedStyle(el); styles.set(el, s); }
    return s;
  };
  // Whitespace collapsed, cut to `max` characters, and never half a surrogate pair (which
  // JSON parsers refuse).
  const clip = (text, max) => {
    let s = String(text == null ? '' : text).replace(/\s+/g, ' ').trim();
    if (s.length > max) s = s.slice(0, max - 1) + '…';
    return typeof s.toWellFormed === 'function' ? s.toWellFormed() : s;
  };
  const gone = (el) => SKIP.has(el.localName) || el.getAttribute('aria-hidden') === 'true' ||
    styleOf(el).display === 'none';
  const shown = (el) => {
    const v = styleOf(el).visibility;
    if (v === 'hidden' || v === 'collapse') return false;
    const box = el.getBoundingClientRect();
    return box.width > 0 && box.height > 0;
  };
  const isBlock = (el) => {
    const d = styleOf(el).display;
    return d !== 'contents' && d !== 'none' && !d.startsWith('inline');
  };
  const isEditableRoot = (el) => el.isContentEditable &&
    !(el.parentElement && el.parentElement.isContentEditable);
  const isField = (el) => ['input', 'select', 'textarea'].includes(el.localName) ||
    isEditableRoot(el);
  // All the text inside `el`, as a name: hidden parts and fields left out, image alts in.
  const textOf = (el) => {
    let out = '';
    for (const n of el.childNodes) {
      if (out.length > 1000) break;
      if (n.nodeType === 3) out += n.data;
      else if (n.nodeType === 1 && !gone(n) && !isField(n)) {
        if (n.localName === 'img') out += ' ' + (n.getAttribute('alt') || '') + ' ';
        else if (n.localName === 'br') out += ' ';
        else out += textOf(n);
      }
    }
    return out;
  };
  // The text of a block's inline content; nested blocks are listed on their own, and labels
  // and controls other than links already name a control.
  const ownText = (el) => {
    let out = '';
    for (const n of el.childNodes) {
      if (out.length > 1000) break;
      if (n.nodeType === 3) out += n.data;
      else if (n.nodeType === 1 && !gone(n) && !isBlock(n) && !isField(n) &&
          n.localName !== 'label' && !(CONTROLS.has(roleOf(n)) && roleOf(n) !== 'link')) {
        if (n.localName === 'img') out += ' ' + (n.getAttribute('alt') || '') + ' ';
        else if (n.localName === 'br') out += ' ';
        else out += ownText(n);
      }
    }
    return out;
  };
  const roleOf = (el) => {
    const explicit = (el.getAttribute('role') || '').trim().split(/\s+/)[0].toLowerCase();
    if (explicit && explicit !== 'none' && explicit !== 'presentation' &&
        explicit !== 'generic') return explicit;
    const tag = el.localName;
    switch (tag) {
      case 'a': case 'area': return el.hasAttribute('href') ? 'link' : '';
      case 'button': case 'summary': return 'button';
      case 'select': return el.multiple || el.size > 1 ? 'listbox' : 'combobox';
      case 'textarea': return 'textbox';
      case 'input': {
        const t = el.type;
        if (t === 'hidden') return '';
        if (NOT_TYPED.includes(t)) return 'button';
        if (t === 'checkbox' || t === 'radio') return t;
        if (t === 'range') return 'slider';
        if (t === 'number') return 'spinbutton';
        if (el.hasAttribute('list')) return 'combobox';
        return t === 'search' ? 'searchbox' : 'textbox';
      }
      case 'h1': case 'h2': case 'h3': case 'h4': case 'h5': case 'h6': return 'heading';
      case 'img': return (el.getAttribute('alt') || '').trim() ? 'img' : '';
      case 'nav': return 'navigation';
      case 'main': return 'main';
      case 'aside': return 'complementary';
      case 'search': return 'search';
      case 'header': case 'footer':
        if (el.closest('article, aside, main, nav, section') !== null) return '';
        return tag === 'header' ? 'banner' : 'contentinfo';
    }
    return '';
  };
  const labelled = (el, max) => {
    let n = clip(el.getAttribute('aria-label'), max);
    if (n) return n;
    const ids = (el.getAttribute('aria-labelledby') || '').trim();
    if (ids) {
      n = clip(ids.split(/\s+/).map((id) => {
        const t = document.getElementById(id);
        return t ? textOf(t) : '';
      }).join(' '), max);
    }
    return n;
  };
  const nameOf = (el, max) => {
    let n = labelled(el, max);
    if (n) return n;
    if (el.labels && el.labels.length) {
      n = clip(Array.from(el.labels, textOf).join(' '), max);
      if (n) return n;
    }
    for (const a of ['placeholder', 'alt']) {
      n = clip(el.getAttribute(a), max);
      if (n) return n;
    }
    if (el.localName === 'input' && NOT_TYPED.includes(el.type)) {
      n = clip(el.type === 'file' ? '' : el.value, max);
      if (n) return n;
      if (el.type === 'submit' || el.type === 'reset') return el.type === 'submit' ? 'Submit' : 'Reset';
    } else if (!isField(el)) {
      n = clip(textOf(el), max);
      if (n) return n;
    }
    return clip(el.getAttribute('title'), max);
  };
  const valueOf = (el, role) => {
    const tag = el.localName;
    if (tag === 'input') {
      if (el.type === 'checkbox' || el.type === 'radio') return el.checked ? 'checked' : 'unchecked';
      if (NOT_TYPED.includes(el.type)) return '';
      if (el.type === 'password') return el.value ? '•••' : '';
      return clip(el.value, 200);
    }
    if (tag === 'textarea') return clip(el.value, 200);
    if (tag === 'select') return clip(Array.from(el.selectedOptions, (o) => o.label).join(', '), 200);
    if (role === 'checkbox' || role === 'radio' || role === 'switch') {
      const c = el.getAttribute('aria-checked');
      return c === 'true' ? 'checked' : c === 'mixed' ? 'mixed' : 'unchecked';
    }
    if (isEditableRoot(el)) return clip(textOf(el), 200);
    return '';
  };

  for (const el of document.querySelectorAll('[data-fuxx-ref]')) el.removeAttribute('data-fuxx-ref');
  const nodes = [];
  let next = 1;
  const add = (node) => { if (nodes.length < MAX_NODES) nodes.push(node); };
  // `quiet`: inside a control or a heading, whose text is already its name.
  const visit = (el, quiet) => {
    if (nodes.length >= MAX_NODES || gone(el)) return;
    let role = roleOf(el);
    const editable = isEditableRoot(el);
    if (CONTROLS.has(role) || editable || el.hasAttribute('onclick')) {
      if (!CONTROLS.has(role)) role = editable ? 'textbox' : 'clickable';
      if (shown(el)) {
        const ref = 'e' + next++;
        el.setAttribute('data-fuxx-ref', ref);
        const node = { role, name: nameOf(el, 80), ref };
        const value = valueOf(el, role);
        if (value) node.value = value;
        add(node);
      }
      quiet = true;
    } else if (role === 'heading') {
      if (shown(el)) {
        const m = /^h([1-6])$/.exec(el.localName);
        const level = m ? Number(m[1]) : parseInt(el.getAttribute('aria-level'), 10) || 2;
        add({ role, name: nameOf(el, 200), level: Math.min(Math.max(level, 1), 6) });
      }
      quiet = true;
    } else if (LANDMARKS.has(role)) {
      if (shown(el)) add({ role, name: labelled(el, 80) });
    } else if (role === 'img') {
      if (!quiet && shown(el)) add({ role, name: nameOf(el, 200) });
      return;
    } else if (!quiet && isBlock(el) && shown(el)) {
      const text = clip(ownText(el), 200);
      if (text) add({ role: 'text', name: text });
    }
    for (const child of el.children) visit(child, quiet);
  };
  if (document.body) visit(document.body, false);
  return { url: location.href, title: clip(document.title, 200), nodes };
})()"#;

const CLICK_FN: &str = r#"(r) => {
  let el = null;
  for (const e of document.querySelectorAll('[data-fuxx-ref]')) {
    if (e.getAttribute('data-fuxx-ref') === r) { el = e; break; }
  }
  if (!el) return { error: 'unknown ref ' + r };
  el.scrollIntoView({ block: 'center', inline: 'center' });
  const box = el.getBoundingClientRect();
  const at = { bubbles: true, cancelable: true, composed: true, view: window, button: 0,
    clientX: box.left + box.width / 2, clientY: box.top + box.height / 2 };
  const pointer = { ...at, pointerId: 1, pointerType: 'mouse', isPrimary: true };
  const P = typeof PointerEvent === 'function' ? PointerEvent : MouseEvent;
  el.dispatchEvent(new P('pointerdown', { ...pointer, buttons: 1 }));
  const focus = el.dispatchEvent(new MouseEvent('mousedown', { ...at, buttons: 1, detail: 1 }));
  if (focus && typeof el.focus === 'function') el.focus({ preventScroll: true });
  el.dispatchEvent(new P('pointerup', pointer));
  el.dispatchEvent(new MouseEvent('mouseup', { ...at, detail: 1 }));
  if (typeof el.click === 'function') el.click();
  else el.dispatchEvent(new MouseEvent('click', { ...at, detail: 1 }));
  return { ok: true };
}"#;

const FILL_FN: &str = r#"(r, text) => {
  let el = null;
  for (const e of document.querySelectorAll('[data-fuxx-ref]')) {
    if (e.getAttribute('data-fuxx-ref') === r) { el = e; break; }
  }
  if (!el) return { error: 'unknown ref ' + r };
  el.scrollIntoView({ block: 'center', inline: 'center' });
  if (typeof el.focus === 'function') el.focus({ preventScroll: true });
  const changed = () => {
    el.dispatchEvent(new InputEvent('input', { bubbles: true, composed: true,
      inputType: 'insertText', data: text }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const setValue = (proto, value) =>
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value);
  const tag = el.localName;
  if (tag === 'select') {
    const want = text.trim();
    const option = Array.from(el.options).find((o) =>
      o.value === text || o.label.trim() === want || o.text.trim() === want);
    if (!option) return { error: 'no option ' + JSON.stringify(text) + ' in ' + r };
    setValue(HTMLSelectElement.prototype, option.value);
    changed();
    return { ok: true };
  }
  if (tag === 'textarea' || (tag === 'input' && !['checkbox', 'radio', 'button', 'submit',
      'reset', 'image', 'file', 'hidden', 'range', 'color'].includes(el.type))) {
    setValue(tag === 'input' ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype, text);
    changed();
    return { ok: true };
  }
  if (el.isContentEditable) {
    el.textContent = text;
    el.dispatchEvent(new InputEvent('input', { bubbles: true, composed: true,
      inputType: 'insertText', data: text }));
    return { ok: true };
  }
  return { error: r + ' is not a text field' };
}"#;

const PRESS_FN: &str = r#"(key) => {
  const NAMED = { Enter: 13, Tab: 9, Escape: 27, Backspace: 8, ArrowLeft: 37, ArrowUp: 38,
    ArrowRight: 39, ArrowDown: 40 };
  const isChar = Array.from(key).length === 1;
  if (!(key in NAMED) && !isChar) {
    return { error: 'unknown key ' + JSON.stringify(key) +
      ' (use Enter, Tab, Escape, Backspace, an arrow key or one character)' };
  }
  let code = key;
  let keyCode = NAMED[key];
  if (isChar) {
    const upper = key.toUpperCase();
    keyCode = key.length === 1 ? upper.charCodeAt(0) : 0;
    code = /^[A-Z]$/.test(upper) ? 'Key' + upper : /^[0-9]$/.test(key) ? 'Digit' + key
      : key === ' ' ? 'Space' : '';
  }
  const active = document.activeElement;
  const target = active && active !== document.documentElement ? active : document.body;
  if (!target) return { ok: true };
  const send = (type, legacy) => {
    const ev = new KeyboardEvent(type, { key, code, bubbles: true, cancelable: true,
      composed: true, view: window });
    // Older handlers read keyCode/which/charCode, which the constructor leaves at 0.
    for (const [name, value] of Object.entries(legacy)) {
      Object.defineProperty(ev, name, { get: () => value });
    }
    return target.dispatchEvent(ev);
  };
  const legacy = { keyCode, which: keyCode, charCode: 0 };
  let go = send('keydown', legacy);
  if (go && (isChar || key === 'Enter')) {
    const charCode = isChar ? key.charCodeAt(0) : 13;
    go = send('keypress', { keyCode: charCode, which: charCode, charCode });
  }
  if (go) {
    const tag = target.localName;
    const typed = tag === 'textarea' || (tag === 'input' && !['checkbox', 'radio', 'button',
      'submit', 'reset', 'image', 'file', 'hidden', 'range', 'color'].includes(target.type));
    // Insert `text` at the caret, or (with null) delete the character before it.
    const edit = (text) => {
      if (typed) {
        const proto = tag === 'input' ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
        const v = target.value;
        let start = null;
        let end = null;
        try { start = target.selectionStart; end = target.selectionEnd; } catch (_) {}
        if (start === null || end === null) { start = v.length; end = v.length; }
        if (text === null && start === end && start > 0) start -= 1;
        const insert = text === null ? '' : text;
        Object.getOwnPropertyDescriptor(proto, 'value').set.call(target,
          v.slice(0, start) + insert + v.slice(end));
        try { target.setSelectionRange(start + insert.length, start + insert.length); } catch (_) {}
        target.dispatchEvent(new InputEvent('input', { bubbles: true, composed: true,
          inputType: text === null ? 'deleteContentBackward' : 'insertText', data: text }));
      } else if (target.isContentEditable) {
        if (text === null) document.execCommand('delete', false);
        else document.execCommand('insertText', false, text);
      }
    };
    if (isChar) edit(key);
    else if (key === 'Backspace') edit(null);
    else if (key === 'Tab') {
      const order = (e) => (e.tabIndex > 0 ? e.tabIndex : Infinity);
      const stops = Array.from(document.querySelectorAll('a[href], area[href], button, input, ' +
          'select, textarea, summary, iframe, [tabindex], [contenteditable]'))
        .filter((e) => !e.disabled && e.tabIndex >= 0 && e.type !== 'hidden' &&
          e.getClientRects().length > 0)
        .sort((a, b) => order(a) - order(b));
      if (stops.length) stops[(stops.indexOf(target) + 1) % stops.length].focus();
    } else if (key === 'Enter') {
      if (tag === 'input' && target.form) {
        const form = target.form;
        const submit = Array.from(form.elements).find((e) => e.type === 'submit' ||
          (e.localName === 'input' && e.type === 'image'));
        if (submit) { if (!submit.disabled) submit.click(); }
        else if (typeof form.requestSubmit === 'function') form.requestSubmit();
        else form.submit();
      } else if (tag === 'textarea' || target.isContentEditable) edit('\n');
      else if (tag === 'a' || tag === 'button' || tag === 'summary' ||
        target.getAttribute('role') === 'button' || target.getAttribute('role') === 'link') {
        target.click();
      }
    }
  }
  send('keyup', legacy);
  return { ok: true };
}"#;

const READ_JS: &str = r#"(() => {
  let text = document.body ? document.body.innerText : '';
  if (text.length > 100000) text = text.slice(0, 100000);
  return typeof text.toWellFormed === 'function' ? text.toWellFormed() : text;
})()"#;

const WAIT_FN: &str = r#"(text, selector) => {
  if (selector !== null && document.querySelector(selector) === null) return false;
  if (text !== null && !(document.body && document.body.innerText.includes(text))) return false;
  return true;
}"#;

const WINDOW_OPEN_SHIM: &str = r#"(() => {
  window.open = function (url) {
    const text = url === undefined || url === null ? '' : String(url);
    if (text === '' || text === 'about:blank') return window;
    let target;
    try { target = new URL(text, location.href); } catch (_) { return null; }
    if (target.protocol !== 'http:' && target.protocol !== 'https:') return null;
    location.assign(target.href);
    return window;
  };
})()"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// Text that would break out of a script if it were pasted in raw.
    const HOSTILE: &[&str] = &[
        "\"",
        "\\",
        "\n",
        "</script>",
        "'); alert(1); ('",
        "\"); alert(1); (\"",
        "`${alert(1)}`",
        "\u{2028}\u{2029}",
    ];

    /// The script made from a harmless marker splits around the marker's JSON literal; the one
    /// made from `arg` must be exactly the same text with `arg`'s JSON literal in that place.
    fn assert_embedded_as_json(make: impl Fn(&str) -> String, arg: &str) {
        let marker = serde_json::to_string("FUXX_ARG_MARKER").unwrap();
        let benign = make("FUXX_ARG_MARKER");
        assert_eq!(benign.matches(&marker).count(), 1, "{benign}");
        let (before, after) = benign.split_once(&marker).unwrap();
        let hostile_literal = serde_json::to_string(arg).unwrap();
        assert_eq!(
            make(arg),
            format!("{before}{hostile_literal}{after}"),
            "{arg:?}"
        );
        let back: String = serde_json::from_str(&hostile_literal).unwrap();
        assert_eq!(back, arg);
    }

    #[test]
    fn arguments_are_embedded_as_json() {
        for &arg in HOSTILE {
            assert_embedded_as_json(click_js, arg);
            assert_embedded_as_json(|r| fill_js(r, "hello"), arg);
            assert_embedded_as_json(|t| fill_js("e1", t), arg);
            assert_embedded_as_json(press_js, arg);
            assert_embedded_as_json(|t| wait_check_js(Some(t), None), arg);
            assert_embedded_as_json(|s| wait_check_js(None, Some(s)), arg);
        }
        // A missing argument is a JSON `null`.
        assert!(wait_check_js(None, Some("#x")).contains("(null, \"#x\")"));
        assert!(wait_check_js(Some("hi"), None).contains("(\"hi\", null)"));
    }

    #[test]
    fn scripts_are_single_expressions() {
        let scripts = [
            snapshot_js().to_string(),
            click_js("e1"),
            fill_js("e1", "x"),
            press_js("Enter"),
            read_js().to_string(),
            wait_check_js(Some("x"), None),
            window_open_shim().to_string(),
        ];
        for script in scripts {
            assert!(script.starts_with('('), "{script}");
            assert!(script.ends_with(')'), "{script}");
        }
        assert!(snapshot_js().contains("data-fuxx-ref"));
        assert!(click_js("e1").contains("data-fuxx-ref"));
        assert!(fill_js("e1", "x").contains("data-fuxx-ref"));
    }

    /// Recorded from `snapshot_js` in Chromium on a test page (a header, a sign-in form, hidden
    /// buttons that must not show, a list, a clickable div, a contenteditable, an image and a
    /// lone surrogate in the text, which comes back as U+FFFD).
    const SAMPLE: &str = r#"{"url":"http://127.0.0.1:8765/form.html","title":"Sign in","nodes":[
        {"role":"banner","name":""},
        {"role":"link","name":"Home","ref":"e1"},
        {"role":"navigation","name":"Main"},
        {"role":"link","name":"Docs","ref":"e2"},
        {"role":"main","name":""},
        {"role":"heading","name":"Sign in","level":1},
        {"role":"text","name":"Use your work account."},
        {"role":"textbox","name":"Email","ref":"e3","value":"ada@example.com"},
        {"role":"textbox","name":"Password","ref":"e4","value":"•••"},
        {"role":"checkbox","name":"Remember me","ref":"e5","value":"unchecked"},
        {"role":"combobox","name":"Plan","ref":"e6","value":"Basic"},
        {"role":"textbox","name":"Notes","ref":"e7"},
        {"role":"textbox","name":"Search here","ref":"e8"},
        {"role":"button","name":"Sign in","ref":"e9"},
        {"role":"heading","name":"Options","level":2},
        {"role":"text","name":"First item"},
        {"role":"text","name":"Second with link"},
        {"role":"link","name":"with link","ref":"e10"},
        {"role":"clickable","name":"Clickable div","ref":"e11"},
        {"role":"textbox","name":"","ref":"e12","value":"Editable"},
        {"role":"img","name":"Logo"},
        {"role":"heading","name":"Deep link","level":3},
        {"role":"link","name":"Deep link","ref":"e13"},
        {"role":"text","name":"bad � surrogate"},
        {"role":"contentinfo","name":""}
    ]}"#;

    #[test]
    fn snapshot_json_parses() {
        let snapshot: Snapshot = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(snapshot.url, "http://127.0.0.1:8765/form.html");
        assert_eq!(snapshot.title, "Sign in");
        assert_eq!(snapshot.nodes.len(), 25);
        assert_eq!(
            snapshot.nodes[5],
            Node {
                role: "heading".into(),
                name: "Sign in".into(),
                r#ref: None,
                value: None,
                level: Some(1),
            }
        );
        assert_eq!(snapshot.nodes[7].r#ref.as_deref(), Some("e3"));
        assert_eq!(snapshot.nodes[7].value.as_deref(), Some("ada@example.com"));
        assert_eq!(snapshot.nodes[8].value.as_deref(), Some("•••"));
        assert_eq!(snapshot.nodes[23].name, "bad \u{FFFD} surrogate");
        let refs = snapshot.nodes.iter().filter(|n| n.r#ref.is_some()).count();
        assert_eq!(refs, 13);
        // Sent on as JSON (the socket reply), a node keeps only the fields it has.
        let heading = serde_json::to_string(&snapshot.nodes[5]).unwrap();
        assert_eq!(heading, r#"{"role":"heading","name":"Sign in","level":1}"#);
        let field = serde_json::to_string(&snapshot.nodes[7]).unwrap();
        assert_eq!(
            field,
            r#"{"role":"textbox","name":"Email","ref":"e3","value":"ada@example.com"}"#
        );
        // A node with no name still parses.
        let bare: Node = serde_json::from_str(r#"{"role":"main"}"#).unwrap();
        assert_eq!(bare.name, "");
    }

    fn node(role: &str, name: &str) -> Node {
        Node {
            role: role.into(),
            name: name.into(),
            r#ref: None,
            value: None,
            level: None,
        }
    }

    fn control(role: &str, name: &str, r: &str, value: Option<&str>) -> Node {
        Node {
            r#ref: Some(r.into()),
            value: value.map(Into::into),
            ..node(role, name)
        }
    }

    fn heading(name: &str, level: u8) -> Node {
        Node {
            level: Some(level),
            ..node("heading", name)
        }
    }

    #[test]
    fn outline_renders_refs_and_headings() {
        let snapshot = Snapshot {
            url: "http://localhost:8000/form.html".into(),
            title: "Sign in".into(),
            nodes: vec![
                node("navigation", ""),
                control("link", "Home", "e1", None),
                heading("Sign in", 1),
                node("text", "Use your work account."),
                control("textbox", "Email", "e2", Some("ada@example.com")),
                heading("Options", 2),
                control("checkbox", "Remember me", "e3", Some("unchecked")),
                control("textbox", "", "e4", None),
                heading("Help", 1),
                node("img", "A \"quoted\" logo"),
                control("button", "Sign in", "e5", None),
            ],
        };
        let expected = r#"title: Sign in
url: http://localhost:8000/form.html
navigation
[e1] link "Home"
heading "Sign in" [level=1]
  text "Use your work account."
  [e2] textbox "Email" = "ada@example.com"
  heading "Options" [level=2]
    [e3] checkbox "Remember me" = "unchecked"
    [e4] textbox
heading "Help" [level=1]
  img "A \"quoted\" logo"
  [e5] button "Sign in""#;
        assert_eq!(render_outline(&snapshot), expected);
    }

    #[test]
    fn node_and_text_caps() {
        let long = "x".repeat(500);
        let mut nodes: Vec<Node> = (1..=1500)
            .map(|i| control("button", &long, &format!("e{i}"), Some(&long)))
            .collect();
        // A name with line breaks can't start a line of its own (and pose as another control).
        nodes[0].name = "Next\n[e9] link \"Pay\"\r\n\tnow".into();
        // The address is never cut: it is what an agent would open again.
        let url = format!("https://example.com/{}", "a".repeat(300));
        let snapshot = Snapshot {
            url: url.clone(),
            title: "t".into(),
            nodes,
        };
        let outline = render_outline(&snapshot);
        let lines: Vec<&str> = outline.lines().collect();
        assert_eq!(lines[1], format!("url: {url}"));
        // Two header lines, 1,000 nodes, one line saying what was left out.
        assert_eq!(lines.len(), 2 + 1000 + 1);
        assert_eq!(
            lines[2],
            format!(
                r#"[e1] button "Next [e9] link \"Pay\" now" = "{}…""#,
                "x".repeat(199)
            )
        );
        assert_eq!(lines[1001].matches('x').count(), 2 * 200 - 2);
        assert!(lines[1001].starts_with("[e1000] button \""));
        assert!(lines[1001].contains('…'));
        assert_eq!(lines[1002], "… 500 more not shown");
    }
}
