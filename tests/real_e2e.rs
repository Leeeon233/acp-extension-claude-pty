use std::{
    process::Command as StdCommand,
    sync::{Arc, Mutex},
};

use agent_client_protocol::{
    Channel, Client,
    schema::{
        ClientCapabilities, ContentBlock, Implementation, InitializeRequest, NewSessionRequest,
        PromptRequest, ProtocolVersion, RequestPermissionOutcome, RequestPermissionRequest,
        RequestPermissionResponse, SelectedPermissionOutcome, SessionId, SetSessionModeRequest,
    },
};
use assert_cmd::Command;
use claude_code_cli_acp::{acp::server::AcpServer, transcript::tailer::TranscriptLocator};
use predicates::prelude::*;
use serial_test::serial;

#[test]
#[ignore = "requires installed authenticated Claude Code and may use network/model quota"]
#[serial]
fn real_claude_doctor_interactive_print_and_transcript() {
    if std::env::var("CLAUDE_CODE_ACP_REAL_E2E").ok().as_deref() != Some("1") {
        return;
    }

    let temp = tempfile::tempdir().expect("temp git repo");
    let git_status = StdCommand::new("git")
        .arg("init")
        .current_dir(temp.path())
        .status()
        .expect("run git init for real e2e temp repo");
    assert!(
        git_status.success(),
        "git init failed for real e2e temp repo"
    );

    Command::cargo_bin("claude-code-cli-acp")
        .expect("binary")
        .arg("doctor")
        .assert()
        .success();

    Command::cargo_bin("claude-code-cli-acp")
        .expect("binary")
        .arg("--")
        .arg("--version")
        .assert()
        .success();

    let text_session = deterministic_session_id(temp.path(), "print-text");
    Command::cargo_bin("claude-code-cli-acp")
        .expect("binary")
        .current_dir(temp.path())
        .args([
            "print",
            "--session-id",
            &text_session,
            "--timeout",
            "120",
            "Respond with exactly ACP_REAL_E2E_OK and no other text.",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("ACP_REAL_E2E_OK"));

    let json_session = deterministic_session_id(temp.path(), "print-json");
    Command::cargo_bin("claude-code-cli-acp")
        .expect("binary")
        .current_dir(temp.path())
        .args([
            "print",
            "--session-id",
            &json_session,
            "--timeout",
            "120",
            "--output-format",
            "json",
            "Respond with exactly ACP_REAL_E2E_JSON and no other text.",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"session_id\""))
        .stdout(predicate::str::contains("ACP_REAL_E2E_JSON"));

    let stdin_session = deterministic_session_id(temp.path(), "print-stdin");
    Command::cargo_bin("claude-code-cli-acp")
        .expect("binary")
        .current_dir(temp.path())
        .args(["print", "--session-id", &stdin_session, "--timeout", "120"])
        .write_stdin("Respond with exactly ACP_REAL_E2E_STDIN and no other text.\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("ACP_REAL_E2E_STDIN"));

    let locator = TranscriptLocator::default_home().expect("default Claude transcript home");
    assert!(
        locator
            .find_transcript(&text_session)
            .expect("find text print transcript")
            .is_some(),
        "real text print did not create a Claude transcript for deterministic session id"
    );
    assert!(
        locator
            .find_transcript(&json_session)
            .expect("find json print transcript")
            .is_some(),
        "real json print did not create a Claude transcript for deterministic session id"
    );
    assert!(
        locator
            .find_transcript(&stdin_session)
            .expect("find stdin print transcript")
            .is_some(),
        "real stdin print did not create a Claude transcript for deterministic session id"
    );
}

#[tokio::test]
#[ignore = "requires installed authenticated Claude Code and may use network/model quota"]
#[serial]
async fn real_claude_acp_permission_edit_flow() {
    if std::env::var("CLAUDE_CODE_ACP_REAL_E2E").ok().as_deref() != Some("1") {
        return;
    }

    let temp = tempfile::tempdir().expect("temp git repo");
    let _setting_sources = EnvVarGuard::set("CLAUDE_CODE_ACP_SETTING_SOURCES", "project");
    let git_status = StdCommand::new("git")
        .arg("init")
        .current_dir(temp.path())
        .status()
        .expect("run git init for real e2e temp repo");
    assert!(
        git_status.success(),
        "git init failed for real e2e temp repo"
    );

    let session_id = SessionId::new(deterministic_session_id(temp.path(), "acp-permission"));
    let permission_requests = Arc::new(Mutex::new(Vec::<RequestPermissionRequest>::new()));
    let (client_transport, server_transport) = Channel::duplex();
    let server_task = tokio::spawn(Arc::new(AcpServer::new()).serve(server_transport));
    let permission_requests_for_client = Arc::clone(&permission_requests);
    let cwd = temp.path().to_path_buf();

    Client
        .builder()
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _cx| {
                permission_requests_for_client
                    .lock()
                    .unwrap()
                    .push(request.clone());
                let option_id = request
                    .options
                    .iter()
                    .find(|option| option.option_id.0.as_ref() == "allow_once")
                    .expect("allow once option")
                    .option_id
                    .clone();
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id)),
                ))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(client_transport, {
            let session_id = session_id.clone();
            async move |cx| {
                cx.send_request(
                    InitializeRequest::new(ProtocolVersion::V1)
                        .client_capabilities(ClientCapabilities::default())
                        .client_info(Implementation::new("real-e2e-client", "0.0.0")),
                )
                .block_task()
                .await?;
                let meta = serde_json::json!({
                    "claudeCode": {
                        "sessionId": session_id.0.as_ref()
                    }
                })
                .as_object()
                .expect("object meta")
                .clone();
                let created = cx
                    .send_request(NewSessionRequest::new(cwd.clone()).meta(meta))
                    .block_task()
                    .await?;
                assert_eq!(created.session_id, session_id);
                cx.send_request(SetSessionModeRequest::new(session_id.clone(), "default"))
                    .block_task()
                    .await?;
                cx.send_request(PromptRequest::new(
                    session_id,
                    vec![ContentBlock::from(
                        "Use Bash to run exactly this command: printf ACP_REAL_PERMISSION_OK > acp-real-permission.txt",
                    )],
                ))
                .block_task()
                .await?;
                Ok(())
            }
        })
        .await
        .expect("client connection");

    server_task.abort();

    assert!(
        !permission_requests.lock().unwrap().is_empty(),
        "real ACP e2e did not exercise Claude permission bridge"
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("acp-real-permission.txt"))
            .expect("real permission edit output"),
        "ACP_REAL_PERMISSION_OK"
    );
    assert!(
        TranscriptLocator::default_home()
            .expect("default Claude transcript home")
            .find_transcript(&session_id.0)
            .expect("find ACP transcript")
            .is_some(),
        "real ACP prompt did not create a Claude transcript for deterministic session id"
    );
}

fn deterministic_session_id(cwd: &std::path::Path, label: &str) -> String {
    let mut state = 0xcbf29ce484222325_u64;
    for byte in cwd
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .chain(label.as_bytes())
    {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(0x100000001b3);
    }

    let mut bytes = [0_u8; 16];
    for byte in &mut bytes {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state = state.wrapping_mul(0x2545f4914f6cdd1d);
        *byte = state as u8;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}
