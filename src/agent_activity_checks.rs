//! Reading a check's outcome from its output (the agent activity chore): cargo, pytest, jest /
//! vitest and go test summaries, since an exit code can lie through a pipe
//! (`cargo test 2>&1 | tail -20` exits as `tail` does). Pure and generic.

use std::borrow::Cow;

use crate::agent_activity::{CheckWhat, Outcome};

/// A check's outcome. A summary the output holds wins (an exit code can lie through a pipe),
/// except a passing one against the check's own non-zero exit, which fails; else the exit code;
/// else the provider's ok / failed. When the exit isn't the check's own (`exit_hidden`: a pipe,
/// `; echo done`, `|| true`) neither exit nor ok counts: no summary is Unknown, never green.
pub fn outcome(
    what: CheckWhat,
    output: &str,
    exit: Option<i32>,
    ok: bool,
    exit_hidden: bool,
) -> Outcome {
    let text = strip_ansi(output);
    let read = cargo(&text)
        .or_else(|| pytest(&text))
        .or_else(|| jest(&text))
        .or_else(|| go_test(&text))
        .or_else(|| {
            (what != CheckWhat::Tests && cargo_finished(&text))
                .then_some(Outcome::Passed { n: None })
        });
    let own_exit = exit.filter(|_| !exit_hidden);
    match read {
        Some(Outcome::Passed { .. }) if own_exit.is_some_and(|c| c != 0) => {
            return Outcome::Failed {
                failed: None,
                of: None,
                first_error: error_line(&text).or_else(|| first_line(&text)),
            };
        }
        Some(found) => return found,
        None => {}
    }
    if exit_hidden {
        return Outcome::Unknown;
    }
    let failed = || Outcome::Failed {
        failed: None,
        of: None,
        first_error: first_line(&text),
    };
    match exit {
        Some(0) => Outcome::Passed { n: None },
        Some(_) => failed(),
        None if ok => Outcome::Passed { n: None },
        None => failed(),
    }
}

/// The first non-blank line, trimmed.
pub fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(String::from)
}

/// cargo's `test result:` lines, summed over every test binary; else a compile error
/// (`error[E…]` / "could not compile") with its first error line.
pub fn cargo(output: &str) -> Option<Outcome> {
    let text = strip_ansi(output);
    let (mut passed, mut failed, mut seen) = (0u32, 0u32, false);
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("test result: ") else {
            continue;
        };
        seen = true;
        passed += count_before(rest, " passed").unwrap_or(0);
        failed += count_before(rest, " failed").unwrap_or(0);
    }
    // A test binary that crashed or quit: cargo says so though every result line was ok.
    let crashed = text.lines().map(str::trim).find(|l| {
        l.starts_with("process didn't exit successfully") || l.starts_with("error: test failed")
    });
    if seen {
        return Some(if failed > 0 {
            Outcome::Failed {
                failed: Some(failed),
                of: Some(passed + failed),
                first_error: panic_line(&text).or_else(|| error_line(&text)),
            }
        } else if let Some(line) = crashed {
            Outcome::Failed {
                failed: None,
                of: None,
                first_error: text
                    .lines()
                    .map(str::trim)
                    .find(|l| l.starts_with("process didn't exit successfully"))
                    .unwrap_or(line)
                    .to_string()
                    .into(),
            }
        } else {
            Outcome::Passed { n: Some(passed) }
        });
    }
    let compile = text.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with("error[E") || l.starts_with("error: could not compile")
    });
    compile.then(|| Outcome::Failed {
        failed: None,
        of: None,
        first_error: error_line(&text),
    })
}

/// cargo's `Finished` line with no error before it: a build or lint that went through.
fn cargo_finished(text: &str) -> bool {
    text.lines()
        .any(|l| l.trim_start().starts_with("Finished `"))
        && !text.lines().any(|l| l.trim_start().starts_with("error"))
}

/// The first line after cargo's first `---- … stdout ----`, without the thread's number
/// (`thread 'x' (97794850) panicked` reads `thread 'x' panicked`). A panic header ending in `:`
/// is joined with its message as one sentence: `x: assertion … failed (src/cli.rs:42)`.
fn panic_line(text: &str) -> Option<String> {
    let mut lines = text.lines().map(str::trim);
    lines.find(|l| l.starts_with("---- ") && l.ends_with(" stdout ----"))?;
    let line = lines.find(|l| !l.is_empty())?;
    if let Some(joined) = panic_sentence(line, lines.next()) {
        return Some(joined);
    }
    let numbered = line
        .split_once("' (")
        .and_then(|(head, rest)| Some((head, rest.split_once(')')?)))
        .filter(|(_, (id, _))| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()));
    Some(match numbered {
        Some((head, (_, tail))) => format!("{head}'{tail}"),
        None => line.to_string(),
    })
}

/// `thread 'name' (id) panicked at file:line:col:` and its next line as `name: message
/// (file:line)`; None when `header` isn't that shape or no message follows.
fn panic_sentence(header: &str, message: Option<&str>) -> Option<String> {
    let rest = header.strip_suffix(':')?.strip_prefix("thread '")?;
    let (name, rest) = rest.split_once('\'')?;
    let (_, at) = rest.split_once(" panicked at ")?;
    let message = message.map(str::trim).filter(|m| !m.is_empty())?;
    // `src/cli.rs:42:9` → `src/cli.rs:42`.
    let at = match at.rsplit_once(':') {
        Some((head, col)) if col.chars().all(|c| c.is_ascii_digit()) && head.contains(':') => head,
        _ => at,
    };
    Some(format!("{name}: {message} ({at})"))
}

/// cargo's first `error[E…]:` line, else its first `error:` line other than "test failed".
fn error_line(text: &str) -> Option<String> {
    let lines = || text.lines().map(str::trim);
    lines()
        .find(|l| l.starts_with("error["))
        .or_else(|| {
            lines().find(|l| l.starts_with("error:") && !l.starts_with("error: test failed"))
        })
        .map(String::from)
}

/// The number just before `word` in `text` ("2 passed; 1 failed" → 1 for " failed").
fn count_before(text: &str, word: &str) -> Option<u32> {
    let at = text.find(word)?;
    let digits: String = text[..at]
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().ok()
}

/// pytest's summary line (`==== 1 failed, 5 passed in 0.12s ====`, or `-q`'s bare one).
pub fn pytest(output: &str) -> Option<Outcome> {
    let text = strip_ansi(output);
    let summary = text.lines().rev().find_map(|l| {
        let l = l.trim().trim_matches('=').trim();
        let (counts, time) = l.rsplit_once(" in ")?;
        let timed = time.trim_end_matches('s').trim().parse::<f64>().is_ok();
        let counted = counts.split(", ").all(|part| {
            part.split_once(' ')
                .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()))
        });
        (timed && counted && !counts.is_empty()).then_some(counts.to_string())
    })?;
    let n = |word: &str| -> u32 {
        summary
            .split(", ")
            .filter_map(|p| p.split_once(' '))
            .filter(|(_, w)| w.starts_with(word))
            .filter_map(|(n, _)| n.parse::<u32>().ok())
            .sum()
    };
    let (passed, failed) = (n("passed"), n("failed") + n("error"));
    Some(if failed > 0 {
        Outcome::Failed {
            failed: Some(failed),
            of: Some(passed + failed),
            first_error: text
                .lines()
                .map(str::trim)
                .find(|l| l.starts_with("FAILED ") || l.starts_with("ERROR "))
                .map(String::from),
        }
    } else {
        Outcome::Passed { n: Some(passed) }
    })
}

/// jest's `Tests: 1 failed, 4 passed, 5 total` and vitest's `Tests  1 failed | 4 passed (5)`.
pub fn jest(output: &str) -> Option<Outcome> {
    let text = strip_ansi(output);
    let summary = text.lines().map(str::trim).find_map(|l| {
        let rest = l.strip_prefix("Tests")?;
        (rest.starts_with(':') || rest.starts_with(' ')).then_some(rest.trim_start_matches(':'))
    })?;
    let tally = Tally::of(summary);
    let total = tally.total();
    let (passed, failed) = (tally.count("passed"), tally.count("failed"));
    if passed.is_none() && failed.is_none() {
        return None;
    }
    // A suite that failed to run (an import error, a crash) fails it, though every test passed.
    let suites = suite_failures(&text).filter(|_| failed.is_none_or(|f| f == 0));
    if let Some((f, of)) = suites {
        return Some(Outcome::Failed {
            failed: Some(f),
            of,
            first_error: text
                .lines()
                .map(str::trim)
                .find(|l| l.starts_with("FAIL ") || l.starts_with("● "))
                .map(String::from),
        });
    }
    Some(match failed.filter(|f| *f > 0) {
        Some(f) => Outcome::Failed {
            failed: Some(f),
            of: total.or(Some(f + passed.unwrap_or(0))),
            first_error: {
                let lines = || text.lines().map(str::trim);
                lines()
                    .find(|l| l.starts_with("● "))
                    .or_else(|| lines().find(|l| l.starts_with("FAIL ")))
                    .map(String::from)
            },
        },
        None => Outcome::Passed { n: passed },
    })
}

/// jest's `Test Suites: 1 failed, 2 passed, 3 total` / vitest's `Test Files  1 failed | 2
/// passed (3)`: the failed suites and their total, when any failed.
fn suite_failures(text: &str) -> Option<(u32, Option<u32>)> {
    let line = text.lines().map(str::trim).find_map(|l| {
        l.strip_prefix("Test Suites:")
            .or_else(|| l.strip_prefix("Test Files"))
    })?;
    let tally = Tally::of(line);
    Some((tally.count("failed").filter(|f| *f > 0)?, tally.total()))
}

/// A jest / vitest summary line's words: `1 failed, 4 passed, 5 total` or `1 failed | 4 passed
/// (5)`.
struct Tally<'a>(Vec<&'a str>);

impl<'a> Tally<'a> {
    fn of(line: &'a str) -> Self {
        Tally(
            line.split(|c: char| c.is_whitespace() || c == ',' || c == '|')
                .filter(|w| !w.is_empty())
                .collect(),
        )
    }

    /// The number before `word`.
    fn count(&self, word: &str) -> Option<u32> {
        self.0
            .windows(2)
            .find(|w| w[1] == word)
            .and_then(|w| w[0].parse().ok())
    }

    /// `N total`, else vitest's `(N)`.
    fn total(&self) -> Option<u32> {
        self.count("total").or_else(|| {
            self.0
                .iter()
                .find_map(|w| w.strip_prefix('(')?.strip_suffix(')')?.parse().ok())
        })
    }
}

/// go test's `--- FAIL:` / `FAIL\t…` and `ok  \t…` lines.
pub fn go_test(output: &str) -> Option<Outcome> {
    let text = strip_ansi(output);
    let fails: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("--- FAIL:"))
        .collect();
    let package_failed = text.lines().any(|l| l == "FAIL" || l.starts_with("FAIL\t"));
    if !fails.is_empty() || package_failed {
        return Some(Outcome::Failed {
            failed: (!fails.is_empty()).then_some(fails.len() as u32),
            of: None,
            first_error: fails.first().map(|l| l.to_string()),
        });
    }
    let ok = text.lines().any(|l| {
        l.strip_prefix("ok")
            .is_some_and(|rest| rest.starts_with([' ', '\t']) && rest.contains('\t'))
    });
    ok.then_some(Outcome::Passed { n: None })
}

/// `text` without ANSI escape sequences (colours, `ESC [ … letter`).
pub fn strip_ansi(text: &str) -> Cow<'_, str> {
    if !text.contains('\u{1b}') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        } else {
            chars.next();
        }
    }
    Cow::Owned(out)
}

/// A compiler error's code: `error[E0425]: …` → `E0425`.
pub fn error_code(first_error: &str) -> Option<&str> {
    let rest = &first_error[first_error.find("error[")? + "error[".len()..];
    let code = &rest[..rest.find(']')?];
    (!code.is_empty()).then_some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARGO_TEST_FAILED: &str = include_str!("fixtures/checks/cargo-test-failed.txt");
    const CARGO_TEST_FAILED_TAIL: &str = include_str!("fixtures/checks/cargo-test-failed-tail.txt");
    const CARGO_TEST_OK: &str = include_str!("fixtures/checks/cargo-test-ok.txt");
    const CARGO_BUILD_E0425: &str = include_str!("fixtures/checks/cargo-build-e0425.txt");
    const PYTEST_FAILED: &str = include_str!("fixtures/checks/pytest-failed.txt");
    const JEST_FAILED: &str = include_str!("fixtures/checks/jest-failed.txt");
    const VITEST_FAILED: &str = include_str!("fixtures/checks/vitest-failed.txt");
    const GO_TEST_FAILED: &str = include_str!("fixtures/checks/go-test-failed.txt");

    #[test]
    fn cargo_test_failures_sum_across_binaries() {
        assert_eq!(
            cargo(CARGO_TEST_FAILED),
            Some(Outcome::Failed {
                failed: Some(1),
                of: Some(3),
                first_error: Some(
                    "cli::tests::bad: assertion `left == right` failed (src/cli.rs:42)".into()
                ),
            })
        );
    }

    #[test]
    fn cargo_test_passes_count_every_result_line() {
        assert_eq!(cargo(CARGO_TEST_OK), Some(Outcome::Passed { n: Some(14) }));
    }

    #[test]
    fn a_compile_error_fails_with_its_first_error_and_code() {
        let first = "error[E0425]: cannot find function `call_within` in this scope";
        assert_eq!(
            cargo(CARGO_BUILD_E0425),
            Some(Outcome::Failed {
                failed: None,
                of: None,
                first_error: Some(first.into())
            })
        );
        assert_eq!(error_code(first), Some("E0425"));
    }

    #[test]
    fn pytest_jest_vitest_and_go_summaries() {
        assert!(
            matches!(pytest(PYTEST_FAILED), Some(Outcome::Failed { failed: Some(1), of: Some(6), first_error: Some(e) }) if e == "FAILED tests/test_api.py::test_get - assert 1 == 2")
        );
        assert_eq!(
            pytest("==== 6 passed in 0.10s ===="),
            Some(Outcome::Passed { n: Some(6) })
        );
        assert!(
            matches!(jest(JEST_FAILED), Some(Outcome::Failed { failed: Some(1), of: Some(5), first_error: Some(e) }) if e == "● api › returns the user")
        );
        assert!(matches!(
            jest(VITEST_FAILED),
            Some(Outcome::Failed {
                failed: Some(1),
                of: Some(5),
                ..
            })
        ));
        assert!(
            matches!(go_test(GO_TEST_FAILED), Some(Outcome::Failed { failed: Some(1), of: None, first_error: Some(e) }) if e == "--- FAIL: TestGet (0.00s)")
        );
        assert_eq!(
            go_test("ok  \tcode/my-app/api\t0.01s\n"),
            Some(Outcome::Passed { n: None })
        );
    }

    #[test]
    fn the_output_wins_over_an_exit_code_a_pipe_hid() {
        assert!(matches!(
            outcome(CheckWhat::Tests, CARGO_TEST_FAILED_TAIL, None, true, true),
            Outcome::Failed {
                failed: Some(1),
                of: Some(3),
                ..
            }
        ));
    }

    #[test]
    fn a_piped_check_with_no_summary_is_unknown_never_passed() {
        assert_eq!(
            outcome(
                CheckWhat::Tests,
                "   Compiling fuxx v0.13.0\n",
                None,
                true,
                true
            ),
            Outcome::Unknown
        );
        assert_eq!(
            outcome(CheckWhat::Lint, "", Some(0), true, false),
            Outcome::Passed { n: None }
        );
        assert_eq!(
            outcome(
                CheckWhat::Lint,
                "Diff in /tmp/fx/code/my-app/src/api.rs at line 3:\n",
                Some(1),
                false,
                false
            ),
            Outcome::Failed {
                failed: None,
                of: None,
                first_error: Some("Diff in /tmp/fx/code/my-app/src/api.rs at line 3:".into())
            }
        );
    }

    #[test]
    fn an_exit_another_command_decides_is_ignored() {
        use crate::agent_activity_command::read_command;
        // `; echo done`: Claude says ok and no exit; no summary → Unknown, never green.
        let hidden = !read_command("cargo fmt --check; echo done").exit_trusted;
        assert_eq!(
            outcome(CheckWhat::Lint, "", None, true, hidden),
            Outcome::Unknown
        );
        // agy always reports an exit: a piped check's 0 is the pipe's.
        assert_eq!(
            outcome(
                CheckWhat::Tests,
                "   Compiling fuxx v0.13.0\n",
                Some(0),
                true,
                true
            ),
            Outcome::Unknown
        );
    }

    #[test]
    fn a_failing_exit_beats_a_passing_summary() {
        // cargo: every result line ok, but a binary crashed.
        let crashed = "Exit code 101\nrunning 12 tests\ntest result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n     Running tests/integration.rs\nerror: test failed, to rerun pass `--test integration`\n\nCaused by:\n  process didn't exit successfully: `target/debug/deps/integration-1a2b` (signal: 11, SIGSEGV: invalid memory reference)\n";
        assert!(matches!(
            cargo(crashed),
            Some(Outcome::Failed { first_error: Some(e), .. }) if e.contains("SIGSEGV")
        ));
        // A trusted non-zero exit turns any passing summary red.
        assert!(matches!(
            outcome(
                CheckWhat::Tests,
                "==== 6 passed in 0.10s ====\nERROR: coverage under 80%\n",
                Some(2),
                false,
                false
            ),
            Outcome::Failed { .. }
        ));
        // Behind a pipe the exit isn't the check's: the summary stands.
        assert_eq!(
            outcome(
                CheckWhat::Tests,
                "==== 6 passed in 0.10s ====",
                Some(1),
                true,
                true
            ),
            Outcome::Passed { n: Some(6) }
        );
    }

    #[test]
    fn a_failed_suite_fails_jest_and_vitest_even_when_every_test_passed() {
        assert!(matches!(
            jest("Test Suites: 1 failed, 2 passed, 3 total\nTests:       5 passed, 5 total\n"),
            Some(Outcome::Failed {
                failed: Some(1),
                of: Some(3),
                ..
            })
        ));
        assert!(matches!(
            jest(" Test Files  1 failed | 2 passed (3)\n      Tests  5 passed (5)\n"),
            Some(Outcome::Failed {
                failed: Some(1),
                of: Some(3),
                ..
            })
        ));
    }

    #[test]
    fn ansi_colours_are_ignored() {
        assert!(matches!(
            pytest("\u{1b}[31m==== 1 failed, 5 passed in 0.12s ====\u{1b}[0m"),
            Some(Outcome::Failed {
                failed: Some(1),
                of: Some(6),
                ..
            })
        ));
    }
}
