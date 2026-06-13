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
fn print_mode_drives_interactive_claude_and_reads_transcript() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake");
    write_executable(
        &fake,
        r#"#!/bin/sh
sid=""
prompt=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --session-id) sid="$2"; shift 2 ;;
    *) prompt="$1"; shift ;;
  esac
done
mkdir -p "$HOME/.claude/projects/fake-project"
transcript="$HOME/.claude/projects/fake-project/$sid.jsonl"
printf 'Claude ready\r\n❯ '
if [ -n "$prompt" ]; then
  printf '{"type":"assistant","sessionId":"%s","message":{"role":"assistant","content":[{"type":"text","text":"reply:%s"}],"model":"fake-model"}}\n' "$sid" "$prompt" >> "$transcript"
  printf '> '
fi
while IFS= read -r line; do
  if [ "$line" = "/exit" ]; then
    printf 'Goodbye!\r\n'
    exit 0
  fi
  printf '{"type":"assistant","sessionId":"%s","message":{"role":"assistant","content":[{"type":"text","text":"reply:%s"}],"model":"fake-model"}}\n' "$sid" "$line" >> "$transcript"
  printf '> '
done
"#,
    );

    Command::cargo_bin("acp-extension-claude-pty")
        .expect("binary")
        .env("CLAUDE_CODE_CLI", &fake)
        .env("HOME", temp.path())
        .arg("print")
        .arg("--session-id")
        .arg("11111111-1111-4111-8111-111111111111")
        .arg("--timeout")
        .arg("3")
        .arg("hello")
        .assert()
        .success()
        .stdout(predicate::str::contains("reply:hello"));
}

#[test]
#[cfg(unix)]
fn print_json_includes_session_id_and_model() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake");
    write_executable(
        &fake,
        r#"#!/bin/sh
sid=""
prompt=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --session-id) sid="$2"; shift 2 ;;
    *) prompt="$1"; shift ;;
  esac
done
mkdir -p "$HOME/.claude/projects/fake-project"
transcript="$HOME/.claude/projects/fake-project/$sid.jsonl"
printf 'Claude ready\r\n> '
if [ -n "$prompt" ]; then
  printf '{"type":"assistant","sessionId":"%s","message":{"role":"assistant","content":[{"type":"text","text":"json:%s"}],"model":"fake-model"}}\n' "$sid" "$prompt" >> "$transcript"
  printf '> '
fi
while IFS= read -r line; do
  printf '{"type":"assistant","sessionId":"%s","message":{"role":"assistant","content":[{"type":"text","text":"json:%s"}],"model":"fake-model"}}\n' "$sid" "$line" >> "$transcript"
  printf '> '
done
"#,
    );

    Command::cargo_bin("acp-extension-claude-pty")
        .expect("binary")
        .env("CLAUDE_CODE_CLI", &fake)
        .env("HOME", temp.path())
        .args([
            "print",
            "--session-id",
            "22222222-2222-4222-8222-222222222222",
            "--timeout",
            "3",
            "--output-format",
            "json",
            "hello",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"session_id\""))
        .stdout(predicate::str::contains("fake-model"));
}

#[test]
#[cfg(unix)]
fn print_mode_reads_prompt_from_stdin() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake");
    write_executable(
        &fake,
        r#"#!/bin/sh
sid=""
prompt=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --session-id) sid="$2"; shift 2 ;;
    *) prompt="$1"; shift ;;
  esac
done
mkdir -p "$HOME/.claude/projects/fake-project"
transcript="$HOME/.claude/projects/fake-project/$sid.jsonl"
printf 'Claude ready\r\n> '
while IFS= read -r line; do
  printf '{"type":"assistant","sessionId":"%s","message":{"role":"assistant","content":[{"type":"text","text":"stdin:%s"}],"model":"fake-model"}}\n' "$sid" "$line" >> "$transcript"
  printf '> '
done
"#,
    );

    Command::cargo_bin("acp-extension-claude-pty")
        .expect("binary")
        .env("CLAUDE_CODE_CLI", &fake)
        .env("HOME", temp.path())
        .args([
            "print",
            "--session-id",
            "33333333-3333-4333-8333-333333333333",
            "--timeout",
            "3",
        ])
        .write_stdin("from pipe\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin:from pipe"));
}

#[test]
#[cfg(unix)]
fn print_attach_on_timeout_resumes_session_for_user() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake");
    let marker = temp.path().join("attached-timeout");
    write_executable(
        &fake,
        r#"#!/bin/sh
sid=""
resume=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --session-id) sid="$2"; shift 2 ;;
    --resume) resume="$2"; shift 2 ;;
    *) shift ;;
  esac
done
if [ -n "$resume" ]; then
  printf '%s' "$resume" > "$HOME/attached-timeout"
  exit 0
fi
mkdir -p "$HOME/.claude/projects/fake-project"
printf 'Claude ready\r\n> '
while IFS= read -r line; do
  sleep 5
done
"#,
    );

    Command::cargo_bin("acp-extension-claude-pty")
        .expect("binary")
        .env("CLAUDE_CODE_CLI", &fake)
        .env("HOME", temp.path())
        .args([
            "print",
            "--session-id",
            "44444444-4444-4444-8444-444444444444",
            "--timeout",
            "2",
            "--attach-on-timeout",
            "hang",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "timed out waiting for Claude transcript",
        ));
    assert_eq!(
        std::fs::read_to_string(marker).expect("attach marker"),
        "44444444-4444-4444-8444-444444444444"
    );
}

#[test]
#[cfg(unix)]
fn print_attach_on_permission_resumes_session_for_user() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake");
    let marker = temp.path().join("attached-permission");
    write_executable(
        &fake,
        r#"#!/bin/sh
sid=""
resume=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --session-id) sid="$2"; shift 2 ;;
    --resume) resume="$2"; shift 2 ;;
    *) shift ;;
  esac
done
if [ -n "$resume" ]; then
  printf '%s' "$resume" > "$HOME/attached-permission"
  exit 0
fi
mkdir -p "$HOME/.claude/projects/fake-project"
printf 'Claude ready\r\n❯ '
while IFS= read -r line; do
  printf '\r\nDo you want to create attached.txt?\r\n'
  printf ' ❯ 1. Yes\r\n'
  printf '   2. Yes, allow all edits during this session (shift+tab)\r\n'
  printf '   3. No\r\n'
  sleep 5
done
"#,
    );

    Command::cargo_bin("acp-extension-claude-pty")
        .expect("binary")
        .env("CLAUDE_CODE_CLI", &fake)
        .env("HOME", temp.path())
        .args([
            "print",
            "--session-id",
            "55555555-5555-4555-8555-555555555555",
            "--timeout",
            "3",
            "--attach-on-permission",
            "needs permission",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("attached user to Claude session"));
    assert_eq!(
        std::fs::read_to_string(marker).expect("attach marker"),
        "55555555-5555-4555-8555-555555555555"
    );
}
