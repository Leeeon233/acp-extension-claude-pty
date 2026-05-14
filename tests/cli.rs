use std::{fs, path::Path};

use assert_cmd::Command;
use predicates::prelude::*;

#[cfg(unix)]
fn write_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, body).expect("write fake cli");
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod");
}

#[test]
#[cfg(unix)]
fn pass_through_alias_invokes_interactive_claude() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake");
    let marker = temp.path().join("args.txt");
    write_executable(
        &fake,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\nexit 0\n",
            marker.display()
        ),
    );

    Command::cargo_bin("claude-code-cli-acp")
        .expect("binary")
        .env("CLAUDE_CODE_CLI", &fake)
        .arg("--")
        .arg("--version")
        .assert()
        .success();

    assert_eq!(fs::read_to_string(marker).expect("marker"), "--version\n");
}

#[test]
fn version_reports_adapter_version_without_invoking_claude() {
    Command::cargo_bin("claude-code-cli-acp")
        .expect("binary")
        .env("CLAUDE_CODE_CLI", "/path/that/should/not/run")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")))
        .stdout(predicate::str::contains("claude-code-cli-acp"));
}

#[test]
#[cfg(unix)]
fn doctor_reports_local_probe_without_live_network() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake");
    write_executable(
        &fake,
        r#"#!/bin/sh
case "$1" in
  --version) printf '2.1.140 (Claude Code)\n' ;;
  --help) printf '%s\n' '--session-id --resume --continue --model --permission-mode --settings --add-dir --debug-file --version --help --output-format --input-format --mcp-config --strict-mcp-config' ;;
esac
"#,
    );

    Command::cargo_bin("claude-code-cli-acp")
        .expect("binary")
        .env("CLAUDE_CODE_CLI", &fake)
        .arg("doctor")
        .assert()
        .success()
        .stderr(predicate::str::contains("Claude version: 2.1.140"));
}
