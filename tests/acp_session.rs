use std::{path::Path, sync::Arc};

use acp_extension_claude_pty::acp::server::AcpServer;
use agent_client_protocol::{
    Channel, Client,
    schema::{
        ClientCapabilities, CloseSessionRequest, Implementation, InitializeRequest,
        ListSessionsRequest, LoadSessionRequest, NewSessionRequest, ProtocolVersion,
    },
};

#[tokio::test]
async fn session_list_and_close_track_in_memory_sessions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (client_transport, server_transport) = Channel::duplex();
    let server = Arc::new(AcpServer::new());
    let server_task = tokio::spawn(server.serve(server_transport));
    let cwd = temp.path().to_path_buf();

    Client
        .builder()
        .connect_with(client_transport, async move |cx| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::default())
                    .client_info(Implementation::new("test-client", "0.0.0")),
            )
            .block_task()
            .await?;

            let created = cx
                .send_request(
                    NewSessionRequest::new(cwd.clone()).meta(
                        serde_json::json!({
                            "claudeCode": {
                                "sessionId": "10000000-0000-4000-8000-000000000077"
                            }
                        })
                        .as_object()
                        .expect("object meta")
                        .clone(),
                    ),
                )
                .block_task()
                .await?;
            assert_eq!(
                created.session_id.0.as_ref(),
                "10000000-0000-4000-8000-000000000077"
            );
            let listed = cx
                .send_request(ListSessionsRequest::new().cwd(cwd.clone()))
                .block_task()
                .await?;
            assert_eq!(listed.sessions.len(), 1);
            assert_eq!(listed.sessions[0].session_id, created.session_id);

            cx.send_request(CloseSessionRequest::new(created.session_id.clone()))
                .block_task()
                .await?;
            let listed_after_close = cx
                .send_request(ListSessionsRequest::new().cwd(cwd))
                .block_task()
                .await?;
            assert!(listed_after_close.sessions.is_empty());
            Ok(())
        })
        .await
        .expect("client connection");

    server_task.abort();
}

#[tokio::test]
async fn load_missing_transcript_returns_resource_not_found() {
    let temp = tempfile::tempdir().expect("tempdir");
    let _env = EnvGuard::set([("HOME", temp.path())]);
    let (client_transport, server_transport) = Channel::duplex();
    let server = Arc::new(AcpServer::new());
    let server_task = tokio::spawn(server.serve(server_transport));
    let cwd = temp.path().to_path_buf();

    Client
        .builder()
        .connect_with(client_transport, async move |cx| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::default())
                    .client_info(Implementation::new("test-client", "0.0.0")),
            )
            .block_task()
            .await?;
            let result = cx
                .send_request(LoadSessionRequest::new(
                    "10000000-0000-4000-8000-000000000099",
                    cwd,
                ))
                .block_task()
                .await;
            assert!(result.is_err(), "missing transcript should fail");
            Ok(())
        })
        .await
        .expect("client connection");

    server_task.abort();
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
