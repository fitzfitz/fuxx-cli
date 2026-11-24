//! Reading a check's outcome from its output (the agent activity chore): cargo, pytest, jest /
//! vitest and go test summaries, since an exit code can lie through a pipe
//! (`cargo test 2>&1 | tail -20` exits as `tail` does). Pure and generic.

use std::borrow::Cow;

use crate::agent_activity::{CheckWhat, Outcome};

/// A check's outcome. A summary the output holds wins (an exit code can lie through a pipe);
/// else the exit code; else, when the check's output went into a pipe, Unknown (never green);
/// else the provider's ok / failed.
pub fn outcome(what: CheckWhat, output: &str, exit: Option<i32>, ok: bool, piped: bool) -> Outcome {
    let text = strip_ansi(output);
    let read = cargo(&text)
        .or_else(|| pytest(&text))
        .or_else(|| jest(&text))
        .or_else(|| go_test(&text))
        .or_else(|| {
            (what != CheckWhat::Tests && cargo_finished(&text))
                .then_some(Outcome::Passed { n: None })
        });
    if let Some(found) = read {
        return found;
    }
    let failed = || Outcome::Failed {
        failed: None,
        of: None,
        first_error: first_line(&text),
    };
    match exit {
        Some(0) => Outcome::Passed { n: None },
        Some(_) => failed(),
        None if piped => Outcome::Unknown,
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
    if seen {
        return Some(if failed > 0 {
            Outcome::Failed {
                failed: Some(failed),
                of: Some(passed + failed),
                first_error: panic_line(&text).or_else(|| error_line(&text)),
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
/// (`thread 'x' (97794850) panicked` reads `thread 'x' panicked`).
fn panic_line(text: &str) -> Option<String> {
    let mut lines = text.lines().map(str::trim);
    lines.find(|l| l.starts_with("---- ") && l.ends_with(" stdout ----"))?;
    let line = lines.find(|l| !l.is_empty())?;
    let numbered = line
        .split_once("' (")
        .and_then(|(head, rest)| Some((head, rest.split_once(')')?)))
        .filter(|(_, (id, _))| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()));
    Some(match numbered {
        Some((head, (_, tail))) => format!("{head}'{tail}"),
        None => line.to_string(),
    })
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
    let words: Vec<&str> = summary
        .split(|c: char| c.is_whitespace() || c == ',' || c == '|')
        .filter(|w| !w.is_empty())
        .collect();
    let count = |word: &str| {
        words
            .windows(2)
            .find(|w| w[1] == word)
            .and_then(|w| w[0].parse::<u32>().ok())
    };
    let total = count("total").or_else(|| {
        words
            .iter()
            .find_map(|w| w.strip_prefix('(')?.strip_suffix(')')?.parse().ok())
    });
    let (passed, failed) = (count("passed"), count("failed"));
    if passed.is_none() && failed.is_none() {
        return None;
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
#[allow(dead_code)] // the activity view (this chore's drawing task) calls it through agent_steps.
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
                first_error: Some("thread 'cli::tests::bad' panicked at src/cli.rs:42:9:".into()),
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
