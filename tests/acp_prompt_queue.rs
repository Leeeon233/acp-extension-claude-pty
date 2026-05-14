use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

use agent_client_protocol::{
    Channel, Client,
    schema::{
        ClientCapabilities, ContentBlock, Implementation, InitializeRequest, NewSessionRequest,
        PromptRequest, ProtocolVersion, SessionNotification, StopReason,
    },
};
use claude_code_cli_acp::acp::server::AcpServer;
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
async fn concurrent_prompts_on_one_session_are_serialized_through_one_pty() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake-queue");
    write_executable(
        &fake,
        r#"#!/bin/sh
sid=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --session-id) sid="$2"; shift 2 ;;
    *) shift ;;
  esac
done
mkdir -p "$HOME/.claude/projects/fake-project"
transcript="$HOME/.claude/projects/fake-project/$sid.jsonl"
printf 'Claude ready\r\n❯ '
count=0
while IFS= read -r line; do
  count=$((count + 1))
  sleep 0.1
  printf '{"type":"assistant","sessionId":"%s","message":{"role":"assistant","content":[{"type":"text","text":"response-%s"}],"model":"fake-model"}}\n' "$sid" "$count" >> "$transcript"
  printf '\r\n❯ '
done
"#,
    );

    let _env = EnvGuard::set([("CLAUDE_CODE_CLI", fake.as_path()), ("HOME", temp.path())]);
    let notifications = Arc::new(Mutex::new(Vec::<SessionNotification>::new()));
    let (client_transport, server_transport) = Channel::duplex();
    let server = Arc::new(AcpServer::new());
    let server_task = tokio::spawn(server.serve(server_transport));
    let notifications_for_client = Arc::clone(&notifications);
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
                    .client_info(Implementation::new("test-client", "0.0.0")),
            )
            .block_task()
            .await?;
            let created = cx
                .send_request(NewSessionRequest::new(cwd))
                .block_task()
                .await?;

            let first = cx
                .send_request(PromptRequest::new(
                    created.session_id.clone(),
                    vec![ContentBlock::from("first")],
                ))
                .block_task();
            let second = cx
                .send_request(PromptRequest::new(
                    created.session_id,
                    vec![ContentBlock::from("second")],
                ))
                .block_task();
            let (first, second) = tokio::join!(first, second);
            assert_eq!(first?.stop_reason, StopReason::EndTurn);
            assert_eq!(second?.stop_reason, StopReason::EndTurn);
            Ok(())
        })
        .await
        .expect("client connection");

    server_task.abort();
    let rendered = format!("{:?}", notifications.lock().unwrap());
    assert!(rendered.contains("response-1"));
    assert!(rendered.contains("response-2"));
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
