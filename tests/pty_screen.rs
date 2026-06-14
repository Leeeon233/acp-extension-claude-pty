use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use acp_extension_claude_pty::pty::input;
use acp_extension_claude_pty::pty::session::{ClaudePtyConfig, ClaudePtySession, mcp_config_json};
use acp_extension_claude_pty::terminal::recognizers::{
    PermissionDecision, ScreenStatus, recognize_permission_dialog, recognize_screen,
};
use acp_extension_claude_pty::terminal::screen::TerminalScreen;
use agent_client_protocol::schema::{
    EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerStdio,
};

#[cfg(unix)]
fn write_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, body).expect("write fake pty cli");
    let mut permissions = fs::metadata(path)
        .expect("fake pty cli metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod fake pty cli");
}

#[test]
fn input_helpers_encode_claude_tui_controls() {
    assert_eq!(input::prompt_submit("hello"), b"hello\r".to_vec());
    assert_eq!(input::slash_command("exit"), b"/exit\r".to_vec());
    assert_eq!(input::slash_command("/clear"), b"/clear\r".to_vec());
    assert_eq!(input::ctrl_c(), vec![0x03]);
    assert_eq!(input::ctrl_d(), vec![0x04]);
    assert_eq!(input::ctrl_j(), vec![0x0a]);
}

#[test]
fn screen_parser_normalizes_ansi_bytes_for_recognizers() {
    let mut screen = TerminalScreen::new(24, 80);
    screen.process(include_bytes!("fixtures/pty/permission.ansi"));
    let text = screen.text();

    assert!(text.contains("Claude needs permission"));
    assert_eq!(recognize_screen(&text), ScreenStatus::Permission);
    let dialog = recognize_permission_dialog(&text).expect("permission dialog");
    assert_eq!(
        dialog
            .options
            .iter()
            .map(|option| option.decision)
            .collect::<Vec<_>>(),
        vec![PermissionDecision::AllowOnce, PermissionDecision::Reject]
    );
}

#[test]
fn recognizers_classify_core_claude_states() {
    assert_eq!(
        recognize_screen("╭─ Claude Code ─╮\n│ >"),
        ScreenStatus::Idle
    );
    assert_eq!(
        recognize_screen(
            "Quick safety check: Is this a project you created or one you trust?\n❯ 1. Yes, I trust this folder\n  2. No, exit"
        ),
        ScreenStatus::WorkspaceTrust
    );
    assert_eq!(recognize_screen("Thinking…"), ScreenStatus::Thinking);
    assert_eq!(
        recognize_screen("Error: network request failed"),
        ScreenStatus::Error
    );
    assert_eq!(recognize_screen("Goodbye!"), ScreenStatus::Exited);
}

#[test]
fn permission_dialog_parses_current_claude_choices() {
    let text = r#"
 Do you want to create acp-permission-probe.txt?
 ❯ 1. Yes
   2. Yes, allow all edits during this session (shift+tab)
   3. No
"#;
    let dialog = recognize_permission_dialog(text).expect("permission dialog");

    assert_eq!(
        dialog.title,
        "Do you want to create acp-permission-probe.txt?"
    );
    assert_eq!(
        dialog
            .options
            .iter()
            .map(|option| (option.accelerator.as_deref(), option.decision))
            .collect::<Vec<_>>(),
        vec![
            (Some("1"), PermissionDecision::AllowOnce),
            (Some("2"), PermissionDecision::AllowAlways),
            (Some("3"), PermissionDecision::Reject),
        ]
    );
    assert_eq!(
        input::permission_choice(&dialog, PermissionDecision::AllowAlways),
        Some(b"2\r".to_vec())
    );
}

#[test]
fn permission_choice_requires_visible_decision() {
    let text = r#"
 Claude needs permission
 [1] Allow
 [2] Deny
"#;
    let dialog = recognize_permission_dialog(text).expect("permission dialog");

    assert_eq!(
        input::permission_choice(&dialog, PermissionDecision::AllowOnce),
        Some(b"1\r".to_vec())
    );
    assert_eq!(
        input::permission_choice(&dialog, PermissionDecision::Reject),
        Some(b"2\r".to_vec())
    );
    assert_eq!(
        input::permission_choice(&dialog, PermissionDecision::AllowAlways),
        None
    );
}

#[test]
fn pty_config_builds_interactive_launch_args_without_print_mode() {
    let config = ClaudePtyConfig {
        executable: "claude".into(),
        session_id: "11111111-1111-4111-8111-111111111111".into(),
        model: Some("sonnet".into()),
        setting_sources: Some("project".into()),
        ..ClaudePtyConfig::default()
    };

    let argv = config.launch_argv().expect("launch argv");
    let argv = argv
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    assert_eq!(argv[0], "claude");
    assert!(argv.contains(&"--session-id".to_string()));
    assert!(argv.contains(&"11111111-1111-4111-8111-111111111111".to_string()));
    assert!(!argv.contains(&"-p".to_string()));
    assert!(!argv.contains(&"--print".to_string()));

    assert!(argv.contains(&"--model".to_string()));
    assert!(argv.contains(&"sonnet".to_string()));
    assert!(argv.contains(&"--setting-sources".to_string()));
    assert!(argv.contains(&"project".to_string()));
}

#[test]
fn pty_config_omits_session_id_when_resuming() {
    let config = ClaudePtyConfig {
        executable: "claude".into(),
        session_id: "11111111-1111-4111-8111-111111111111".into(),
        resume: Some("11111111-1111-4111-8111-111111111111".into()),
        permission_mode: Some("acceptEdits".into()),
        ..ClaudePtyConfig::default()
    };

    let argv = config.launch_argv().expect("launch argv");
    let argv = argv
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    assert_eq!(argv[0], "claude");
    assert!(!argv.contains(&"--session-id".to_string()));
    assert!(argv.contains(&"--resume".to_string()));
    assert!(argv.contains(&"11111111-1111-4111-8111-111111111111".to_string()));
    assert!(argv.contains(&"--permission-mode".to_string()));
    assert!(argv.contains(&"acceptEdits".to_string()));
}

#[test]
fn mcp_servers_convert_to_claude_mcp_config_json() {
    let config = mcp_config_json(&[
        McpServer::Stdio(
            McpServerStdio::new("local-tools", "/usr/bin/tool")
                .args(vec!["serve".to_string()])
                .env(vec![EnvVariable::new("TOKEN", "redacted")]),
        ),
        McpServer::Http(
            McpServerHttp::new("remote-tools", "https://mcp.example.com")
                .headers(vec![HttpHeader::new("Authorization", "Bearer redacted")]),
        ),
    ]);

    assert_eq!(
        config["mcpServers"]["local-tools"]["command"],
        serde_json::json!("/usr/bin/tool")
    );
    assert_eq!(
        config["mcpServers"]["local-tools"]["args"],
        serde_json::json!(["serve"])
    );
    assert_eq!(
        config["mcpServers"]["local-tools"]["env"]["TOKEN"],
        serde_json::json!("redacted")
    );
    assert_eq!(
        config["mcpServers"]["remote-tools"]["type"],
        serde_json::json!("http")
    );
    assert_eq!(
        config["mcpServers"]["remote-tools"]["headers"]["Authorization"],
        serde_json::json!("Bearer redacted")
    );
}

#[test]
#[cfg(unix)]
fn pty_session_spawns_fake_cli_and_updates_screen_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake-pty");
    write_executable(
        &fake,
        r#"#!/bin/sh
printf 'Claude ready\r\n> '
while IFS= read -r line; do
  printf 'received:%s\r\n> ' "$line"
  if [ "$line" = "/exit" ]; then
    printf 'Goodbye!\r\n'
    exit 0
  fi
done
"#,
    );

    let mut session = ClaudePtySession::spawn(ClaudePtyConfig {
        executable: fake,
        ..ClaudePtyConfig::default()
    })
    .expect("spawn fake pty cli");

    wait_for_screen(&session, "Claude ready");
    session.submit_prompt("ping").expect("submit prompt");
    wait_for_screen(&session, "received:ping");
    session.send_exit().expect("send exit");
    wait_for_screen(&session, "Goodbye!");
    session.terminate().expect("terminate");
}

fn wait_for_screen(session: &ClaudePtySession, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if session
            .screen_snapshot()
            .expect("screen snapshot")
            .contains(needle)
        {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {needle}");
        thread::sleep(Duration::from_millis(25));
    }
}
