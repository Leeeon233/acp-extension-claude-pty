use std::sync::Arc;

use agent_client_protocol::{
    Channel, Client,
    schema::{
        ClientCapabilities, Implementation, InitializeRequest, LoadSessionRequest,
        NewSessionRequest, ProtocolVersion, SessionConfigKind, SessionConfigOption,
        SetSessionConfigOptionRequest, SetSessionModeRequest,
    },
};
use claude_code_cli_acp::acp::server::AcpServer;

#[tokio::test]
async fn new_and_loaded_sessions_advertise_modes_models_and_config_options() {
    let temp = tempfile::tempdir().expect("tempdir");
    let _config_dir = EnvVarGuard::set("CLAUDE_CONFIG_DIR", temp.path().join("home-claude"));
    let (client_transport, server_transport) = Channel::duplex();
    let server_task = tokio::spawn(Arc::new(AcpServer::new()).serve(server_transport));
    let cwd = temp.path().to_path_buf();

    Client
        .builder()
        .connect_with(client_transport, async move |cx| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::default())
                    .client_info(Implementation::new("session-config-client", "0.0.0")),
            )
            .block_task()
            .await?;

            let created = cx
                .send_request(NewSessionRequest::new(cwd.clone()))
                .block_task()
                .await?;
            assert_eq!(
                created
                    .modes
                    .as_ref()
                    .expect("modes")
                    .current_mode_id
                    .0
                    .as_ref(),
                "default"
            );
            assert_eq!(
                config_value(
                    created
                        .config_options
                        .as_ref()
                        .expect("new session config options"),
                    "mode",
                ),
                "default"
            );
            assert_eq!(
                config_value(
                    created
                        .config_options
                        .as_ref()
                        .expect("new session config options"),
                    "model",
                ),
                "default"
            );
            assert!(
                created
                    .models
                    .as_ref()
                    .expect("models")
                    .available_models
                    .len()
                    >= 2
            );

            let response = cx
                .send_request(SetSessionConfigOptionRequest::new(
                    created.session_id.clone(),
                    "mode",
                    "plan",
                ))
                .block_task()
                .await?;
            assert_eq!(config_value(&response.config_options, "mode"), "plan");

            cx.send_request(SetSessionModeRequest::new(
                created.session_id.clone(),
                "default",
            ))
            .block_task()
            .await?;

            let loaded = cx
                .send_request(LoadSessionRequest::new(created.session_id.clone(), cwd))
                .block_task()
                .await?;
            assert!(loaded.config_options.is_some());
            assert!(loaded.modes.is_some());
            assert!(loaded.models.is_some());
            Ok(())
        })
        .await
        .expect("client connection");

    server_task.abort();
}

fn config_value(options: &[SessionConfigOption], id: &str) -> String {
    let option = options
        .iter()
        .find(|option| option.id.0.as_ref() == id)
        .expect("config option");
    let SessionConfigKind::Select(select) = &option.kind else {
        panic!("expected select option");
    };
    select.current_value.0.to_string()
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<std::path::Path>) -> Self {
        let previous = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value.as_ref());
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
