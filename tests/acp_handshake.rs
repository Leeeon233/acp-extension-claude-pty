use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

use agent_client_protocol::{
    Channel, Client,
    schema::{
        AuthCapabilities, AuthMethod, CancelNotification, ClientCapabilities, ContentBlock,
        Implementation, InitializeRequest, LoadSessionRequest, NewSessionRequest, PromptRequest,
        ProtocolVersion, RequestPermissionOutcome, RequestPermissionRequest,
        RequestPermissionResponse, SelectedPermissionOutcome, SessionNotification,
        SetSessionModeRequest, StopReason,
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
async fn initialize_advertises_claude_acp_capabilities() {
    let server = AcpServer::new();
    let response = server
        .initialize_for_test(ClientCapabilities::default())
        .await
        .expect("initialize");

    assert_eq!(response.protocol_version, ProtocolVersion::V1);
    assert_eq!(
        response.agent_info.as_ref().expect("agent info").name,
        "claude-code-cli-acp"
    );
    assert!(response.agent_capabilities.load_session);
    assert!(
        response
            .agent_capabilities
            .prompt_capabilities
            .embedded_context
    );
    assert!(!response.agent_capabilities.prompt_capabilities.image);
    assert!(response.auth_methods.iter().any(|method| {
        matches!(method, AuthMethod::Agent(agent) if agent.id.0.as_ref() == "claude-code-login")
    }));
    assert!(!response.auth_methods.iter().any(|method| {
        matches!(
            method,
            AuthMethod::Terminal(terminal)
                if terminal.id.0.as_ref() == "claude-code-terminal-login"
        )
    }));
}

#[tokio::test]
async fn initialize_advertises_terminal_auth_when_client_supports_it() {
    let server = AcpServer::new();
    let response = server
        .initialize_for_test(ClientCapabilities::new().auth(AuthCapabilities::new().terminal(true)))
        .await
        .expect("initialize");

    let terminal_method = response
        .auth_methods
        .iter()
        .find_map(|method| match method {
            AuthMethod::Terminal(terminal)
                if terminal.id.0.as_ref() == "claude-code-terminal-login" =>
            {
                Some(terminal)
            }
            _ => None,
        })
        .expect("terminal auth method");

    assert_eq!(terminal_method.args, ["interactive"]);
}

#[test]
fn server_can_allocate_memory_session_without_spawning_claude() {
    let server = AcpServer::new();
    let temp = tempfile::tempdir().expect("tempdir");
    let session_id = server
        .create_session_for_test(temp.path().to_path_buf())
        .expect("session");

    assert!(!session_id.0.is_empty());
}

#[tokio::test]
#[cfg(unix)]
#[serial]
async fn acp_transport_prompt_permission_load_and_cancel_round_trip() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake-acp");
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
while IFS= read -r line; do
  printf '\r\nDo you want to create acp-permission-probe.txt?\r\n'
  printf ' ❯ 1. Yes\r\n'
  printf '   2. Yes, allow all edits during this session (shift+tab)\r\n'
  printf '   3. No\r\n'
  IFS= read -r choice
  if [ "$choice" = "1" ]; then
    printf '{"type":"assistant","sessionId":"%s","message":{"role":"assistant","content":[{"type":"text","text":"acp permission ok"}],"model":"fake-model"}}\n' "$sid" >> "$transcript"
    printf '\r\n❯ '
  else
    printf '{"type":"assistant","sessionId":"%s","message":{"role":"assistant","content":[{"type":"text","text":"acp permission rejected"}],"model":"fake-model"}}\n' "$sid" >> "$transcript"
    printf '\r\n❯ '
  fi
done
"#,
    );

    let _env = EnvGuard::set([("CLAUDE_CODE_CLI", fake.as_path()), ("HOME", temp.path())]);
    let notifications = Arc::new(Mutex::new(Vec::<SessionNotification>::new()));
    let permission_requests = Arc::new(Mutex::new(Vec::<RequestPermissionRequest>::new()));
    let (client_transport, server_transport) = Channel::duplex();
    let server = Arc::new(AcpServer::new());
    let server_task = tokio::spawn(server.serve(server_transport));

    let notifications_for_client = Arc::clone(&notifications);
    let permission_requests_for_client = Arc::clone(&permission_requests);
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
        .connect_with(client_transport, async move |cx| {
            let initialized = cx
                .send_request(
                    InitializeRequest::new(ProtocolVersion::V1)
                        .client_capabilities(ClientCapabilities::default())
                        .client_info(Implementation::new("fake-client", "0.0.0")),
                )
                .block_task()
                .await?;
            assert_eq!(initialized.protocol_version, ProtocolVersion::V1);

            let created = cx
                .send_request(NewSessionRequest::new(cwd.clone()))
                .block_task()
                .await?;
            cx.send_request(LoadSessionRequest::new(
                created.session_id.clone(),
                cwd.clone(),
            ))
            .block_task()
            .await?;
            cx.send_request(SetSessionModeRequest::new(
                created.session_id.clone(),
                "default",
            ))
            .block_task()
            .await?;
            let prompt_response = cx
                .send_request(PromptRequest::new(
                    created.session_id.clone(),
                    vec![ContentBlock::from("trigger permission")],
                ))
                .block_task()
                .await?;
            assert_eq!(prompt_response.stop_reason, StopReason::EndTurn);
            cx.send_notification(CancelNotification::new(created.session_id))?;
            Ok(())
        })
        .await
        .expect("client connection");

    server_task.abort();

    assert!(permission_requests.lock().unwrap().iter().any(|request| {
        request
            .options
            .iter()
            .any(|option| option.option_id.0.as_ref() == "allow_once")
    }));
    assert!(
        notifications
            .lock()
            .unwrap()
            .iter()
            .any(|notification| format!("{:?}", notification.update).contains("acp permission ok"))
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
