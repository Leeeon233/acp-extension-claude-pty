use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

use acp_extension_claude_pty::acp::server::AcpServer;
use agent_client_protocol::{
    Channel, Client,
    schema::{
        ClientCapabilities, CloseSessionRequest, ContentBlock, Implementation, InitializeRequest,
        NewSessionRequest, PromptRequest, ProtocolVersion, SessionNotification,
        SetSessionConfigOptionRequest, SetSessionModeRequest, StopReason,
    },
};
use serial_test::serial;

#[cfg(unix)]
fn write_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, body).expect("write fake cli");
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod");
}

#[tokio::test]
#[cfg(unix)]
#[serial]
async fn set_session_mode_restarts_pty_before_next_prompt() {
    run_dynamic_mode_case(ModeChange::SetMode("plan"), "plan").await;
}

#[tokio::test]
#[cfg(unix)]
#[serial]
async fn set_session_config_mode_restarts_pty_before_next_prompt() {
    run_dynamic_mode_case(ModeChange::SetConfigOption("acceptEdits"), "acceptEdits").await;
}

#[cfg(unix)]
#[derive(Clone, Copy)]
enum ModeChange {
    SetMode(&'static str),
    SetConfigOption(&'static str),
}

#[cfg(unix)]
async fn run_dynamic_mode_case(mode_change: ModeChange, expected_mode: &'static str) {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake-dynamic-mode");
    write_executable(
        &fake,
        r#"#!/bin/sh
sid=""
sid_arg=""
resume=""
mode=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --session-id) sid_arg="$2"; sid="$2"; shift 2 ;;
    --resume) resume="$2"; shift 2 ;;
    --permission-mode) mode="$2"; shift 2 ;;
    *) shift ;;
  esac
done
if [ -z "$sid" ] && [ -n "$resume" ]; then
  sid="$resume"
fi
printf 'sid_arg=%s effective_sid=%s resume=%s mode=%s\n' "$sid_arg" "$sid" "$resume" "$mode" >> "$HOME/invocations.log"
mkdir -p "$HOME/.claude/projects/fake-project"
transcript="$HOME/.claude/projects/fake-project/$sid.jsonl"
printf 'Claude ready\r\n❯ '
count=0
while IFS= read -r line; do
  count=$((count + 1))
  printf '{"type":"assistant","sessionId":"%s","message":{"role":"assistant","content":[{"type":"text","text":"mode:%s response:%s"}],"model":"fake-model"}}\n' "$sid" "$mode" "$count" >> "$transcript"
  printf '\r\n❯ '
done
"#,
    );

    let _env = EnvGuard::set([("CLAUDE_CODE_CLI", fake.as_path()), ("HOME", temp.path())]);
    let notifications = Arc::new(Mutex::new(Vec::<SessionNotification>::new()));
    let session_id = Arc::new(Mutex::new(None::<String>));
    let (client_transport, server_transport) = Channel::duplex();
    let server = Arc::new(AcpServer::new());
    let server_task = tokio::spawn(server.serve(server_transport));
    let notifications_for_client = Arc::clone(&notifications);
    let session_id_for_client = Arc::clone(&session_id);
    let cwd = temp.path().to_path_buf();

    Client
        .builder()
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                notifications_for_client.lock().unwrap().push(notification);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(client_transport, async move |cx| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::default())
                    .client_info(Implementation::new("dynamic-mode-client", "0.0.0")),
            )
            .block_task()
            .await?;

            let created = cx
                .send_request(NewSessionRequest::new(cwd))
                .block_task()
                .await?;
            *session_id_for_client.lock().unwrap() = Some(created.session_id.0.to_string());

            let first = cx
                .send_request(PromptRequest::new(
                    created.session_id.clone(),
                    vec![ContentBlock::from("first")],
                ))
                .block_task()
                .await?;
            assert_eq!(first.stop_reason, StopReason::EndTurn);

            match mode_change {
                ModeChange::SetMode(mode) => {
                    cx.send_request(SetSessionModeRequest::new(created.session_id.clone(), mode))
                        .block_task()
                        .await?;
                }
                ModeChange::SetConfigOption(mode) => {
                    cx.send_request(SetSessionConfigOptionRequest::new(
                        created.session_id.clone(),
                        "mode",
                        mode,
                    ))
                    .block_task()
                    .await?;
                }
            }

            let second = cx
                .send_request(PromptRequest::new(
                    created.session_id.clone(),
                    vec![ContentBlock::from("second")],
                ))
                .block_task()
                .await?;
            assert_eq!(second.stop_reason, StopReason::EndTurn);

            cx.send_request(CloseSessionRequest::new(created.session_id))
                .block_task()
                .await?;
            Ok(())
        })
        .await
        .expect("client connection");

    server_task.abort();

    let session_id = session_id
        .lock()
        .unwrap()
        .clone()
        .expect("created session id");
    let invocations =
        fs::read_to_string(temp.path().join("invocations.log")).expect("fake cli invocation log");
    let lines = invocations.lines().collect::<Vec<_>>();
    assert_eq!(
        lines,
        vec![
            format!("sid_arg={session_id} effective_sid={session_id} resume= mode="),
            format!("sid_arg= effective_sid={session_id} resume={session_id} mode={expected_mode}"),
        ],
        "{invocations}"
    );
}

struct EnvGuard {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    fn set<const N: usize>(values: [(&'static str, &Path); N]) -> Self {
        let previous = values
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect::<Vec<_>>();
        for (key, value) in values {
            unsafe {
                std::env::set_var(key, value);
            }
        }
        Self { previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..) {
            unsafe {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }
}
