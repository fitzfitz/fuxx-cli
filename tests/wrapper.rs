use std::process::Command;

fn fuxx() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fuxx-cli"))
}

#[test]
fn echo_is_passed_through() {
    let out = fuxx().args(["echo", "hello"]).output().unwrap();
    assert!(out.status.success(), "status: {:?}", out.status);
    let s = String::from_utf8_lossy(&out.stdout);
    // A PTY translates "\n" to "\r\n", so match on the substring, not equality.
    assert!(s.contains("hello"), "stdout was: {s:?}");
}

#[test]
fn exit_code_is_propagated() {
    let status = fuxx().arg("false").status().unwrap();
    assert_eq!(status.code(), Some(1));
}

#[test]
fn no_args_is_an_error() {
    let out = fuxx().output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn unknown_command_is_an_error() {
    let out = fuxx().arg("definitely-not-a-real-command-xyz").output().unwrap();
    assert!(!out.status.success());
}
