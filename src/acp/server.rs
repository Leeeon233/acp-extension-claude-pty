use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use acp::schema::{
    AgentCapabilities, AuthMethod, AuthMethodAgent, AuthMethodTerminal, AuthenticateRequest,
    AuthenticateResponse, CancelNotification, ClientCapabilities, CloseSessionRequest,
    CloseSessionResponse, Implementation, InitializeRequest, InitializeResponse,
    ListSessionsRequest, ListSessionsResponse, LoadSessionRequest, LoadSessionResponse,
    McpCapabilities, NewSessionRequest, NewSessionResponse, PromptCapabilities, PromptRequest,
    PromptResponse, ProtocolVersion, SessionCapabilities, SessionCloseCapabilities, SessionId,
    SessionInfo, SessionListCapabilities, SessionNotification, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse,
    SetSessionModelRequest, SetSessionModelResponse, StopReason,
};
use acp::{Agent, Client, ConnectTo, ConnectionTo, Error};
use agent_client_protocol as acp;
use agent_client_protocol::ByteStreams;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    acp::updates,
    compat::claude_probe::ClaudeCli,
    session::manager::{ManagedSession, SessionManager, TurnOptions},
    transcript::tailer::TranscriptLocator,
};

const CLAUDE_CODE_LOGIN_AUTH_ID: &str = "claude-code-login";
const CLAUDE_CODE_TERMINAL_AUTH_ID: &str = "claude-code-terminal-login";

pub struct AcpServer {
    client_capabilities: Arc<Mutex<ClientCapabilities>>,
    sessions: Arc<Mutex<HashMap<SessionId, Arc<ManagedSession>>>>,
    manager: SessionManager,
    startup_prompt_timeout: Duration,
}

impl AcpServer {
    pub fn new() -> Self {
        Self::with_startup_prompt_timeout(Duration::from_secs(
            crate::session::manager::DEFAULT_STARTUP_PROMPT_TIMEOUT_SECS,
        ))
    }

    pub fn with_startup_prompt_timeout(startup_prompt_timeout: Duration) -> Self {
        Self {
            client_capabilities: Arc::default(),
            sessions: Arc::default(),
            manager: SessionManager::new(),
            startup_prompt_timeout,
        }
    }

    pub async fn serve_stdio(self) -> std::io::Result<()> {
        let stdin = tokio::io::stdin().compat();
        let stdout = tokio::io::stdout().compat_write();
        Arc::new(self)
            .serve(ByteStreams::new(stdout, stdin))
            .await
            .map_err(|e| std::io::Error::other(format!("ACP error: {e}")))
    }

    pub async fn initialize_for_test(
        &self,
        client_capabilities: ClientCapabilities,
    ) -> Result<InitializeResponse, Error> {
        self.initialize(
            InitializeRequest::new(ProtocolVersion::V1)
                .client_capabilities(client_capabilities)
                .client_info(Implementation::new("test-client", "0.0.0")),
        )
        .await
    }

    pub fn create_session_for_test(&self, cwd: PathBuf) -> anyhow::Result<SessionId> {
        let session_id = SessionId::new(Uuid::new_v4().to_string());
        let session = self
            .manager
            .create_session(session_id.clone(), cwd, Vec::new())?;
        self.sessions
            .lock()
            .unwrap()
            .insert(session_id.clone(), session);
        Ok(session_id)
    }

    pub async fn serve(
        self: Arc<Self>,
        transport: impl ConnectTo<Agent> + 'static,
    ) -> acp::Result<()> {
        let server = self;
        Agent
            .builder()
            .name("acp-extension-claude-pty")
            .on_receive_request(
                {
                    let server = server.clone();
                    async move |request: InitializeRequest, responder, _cx| {
                        responder.respond_with_result(server.initialize(request).await)
                    }
                },
                acp::on_receive_request!(),
            )
            .on_receive_request(
                {
                    let server = server.clone();
                    async move |request: AuthenticateRequest,
                                responder,
                                cx: ConnectionTo<Client>| {
                        let server = server.clone();
                        cx.spawn(async move {
                            responder.respond_with_result(server.authenticate(request).await)
                        })?;
                        Ok(())
                    }
                },
                acp::on_receive_request!(),
            )
            .on_receive_request(
                {
                    let server = server.clone();
                    async move |request: NewSessionRequest, responder, cx: ConnectionTo<Client>| {
                        let server = server.clone();
                        let session_cx = cx.clone();
                        cx.spawn(async move {
                            responder
                                .respond_with_result(server.new_session(request, session_cx).await)
                        })?;
                        Ok(())
                    }
                },
                acp::on_receive_request!(),
            )
            .on_receive_request(
                {
                    let server = server.clone();
                    async move |request: LoadSessionRequest, responder, cx: ConnectionTo<Client>| {
                        let server = server.clone();
                        let session_cx = cx.clone();
                        cx.spawn(async move {
                            responder
                                .respond_with_result(server.load_session(request, session_cx).await)
                        })?;
                        Ok(())
                    }
                },
                acp::on_receive_request!(),
            )
            .on_receive_request(
                {
                    let server = server.clone();
                    async move |request: ListSessionsRequest,
                                responder,
                                cx: ConnectionTo<Client>| {
                        let server = server.clone();
                        cx.spawn(async move {
                            responder.respond_with_result(server.list_sessions(request).await)
                        })?;
                        Ok(())
                    }
                },
                acp::on_receive_request!(),
            )
            .on_receive_request(
                {
                    let server = server.clone();
                    async move |request: CloseSessionRequest,
                                responder,
                                cx: ConnectionTo<Client>| {
                        let server = server.clone();
                        cx.spawn(async move {
                            responder.respond_with_result(server.close_session(request).await)
                        })?;
                        Ok(())
                    }
                },
                acp::on_receive_request!(),
            )
            .on_receive_request(
                {
                    let server = server.clone();
                    async move |request: PromptRequest, responder, cx: ConnectionTo<Client>| {
                        let server = server.clone();
                        let prompt_cx = cx.clone();
                        cx.spawn(async move {
                            responder.respond_with_result(server.prompt(request, prompt_cx).await)
                        })?;
                        Ok(())
                    }
                },
                acp::on_receive_request!(),
            )
            .on_receive_notification(
                {
                    let server = server.clone();
                    async move |notification: CancelNotification, cx: ConnectionTo<Client>| {
                        let server = server.clone();
                        cx.spawn(async move {
                            if let Err(err) = server.cancel(notification).await {
                                warn!("failed to cancel session: {err:?}");
                            }
                            Ok(())
                        })?;
                        Ok(())
                    }
                },
                acp::on_receive_notification!(),
            )
            .on_receive_request(
                {
                    let server = server.clone();
                    async move |request: SetSessionModeRequest,
                                responder,
                                cx: ConnectionTo<Client>| {
                        let server = server.clone();
                        cx.spawn(async move {
                            responder.respond_with_result(server.set_session_mode(request).await)
                        })?;
                        Ok(())
                    }
                },
                acp::on_receive_request!(),
            )
            .on_receive_request(
                {
                    let server = server.clone();
                    async move |request: SetSessionModelRequest,
                                responder,
                                cx: ConnectionTo<Client>| {
                        let server = server.clone();
                        cx.spawn(async move {
                            responder.respond_with_result(server.set_session_model(request).await)
                        })?;
                        Ok(())
                    }
                },
                acp::on_receive_request!(),
            )
            .on_receive_request(
                {
                    let server = server.clone();
                    async move |request: SetSessionConfigOptionRequest,
                                responder,
                                cx: ConnectionTo<Client>| {
                        let server = server.clone();
                        cx.spawn(async move {
                            responder.respond_with_result(
                                server.set_session_config_option(request).await,
                            )
                        })?;
                        Ok(())
                    }
                },
                acp::on_receive_request!(),
            )
            .connect_to(transport)
            .await
    }

    async fn initialize(&self, request: InitializeRequest) -> Result<InitializeResponse, Error> {
        debug!(
            "initialize requested with protocol version {:?}",
            request.protocol_version
        );
        let client_capabilities = request.client_capabilities;
        *self.client_capabilities.lock().unwrap() = client_capabilities.clone();

        let mut agent_capabilities = AgentCapabilities::new()
            .prompt_capabilities(
                PromptCapabilities::new()
                    .embedded_context(true)
                    .image(false),
            )
            .mcp_capabilities(McpCapabilities::new().http(true))
            .load_session(true);
        agent_capabilities.session_capabilities = SessionCapabilities::new()
            .close(SessionCloseCapabilities::new())
            .list(SessionListCapabilities::new());

        Ok(InitializeResponse::new(ProtocolVersion::V1)
            .agent_capabilities(agent_capabilities)
            .agent_info(
                Implementation::new("acp-extension-claude-pty", env!("CARGO_PKG_VERSION"))
                    .title("ACP Extension Claude PTY"),
            )
            .auth_methods(auth_methods(&client_capabilities)))
    }

    async fn authenticate(
        &self,
        request: AuthenticateRequest,
    ) -> Result<AuthenticateResponse, Error> {
        match request.method_id.0.as_ref() {
            CLAUDE_CODE_LOGIN_AUTH_ID | CLAUDE_CODE_TERMINAL_AUTH_ID => {
                ClaudeCli::from_env_or_path().version().map_err(|err| {
                    internal_error(anyhow::anyhow!(
                        "Claude Code CLI is not available for authentication check: {err}"
                    ))
                })?;
                Ok(AuthenticateResponse::new())
            }
            other => {
                Err(Error::invalid_params()
                    .data(format!("unsupported authentication method: {other}")))
            }
        }
    }

    async fn new_session(
        &self,
        request: NewSessionRequest,
        _cx: ConnectionTo<Client>,
    ) -> Result<NewSessionResponse, Error> {
        let session_id = requested_session_id(request.meta.as_ref())
            .unwrap_or_else(|| SessionId::new(Uuid::new_v4().to_string()));
        let session = self
            .manager
            .create_session(session_id.clone(), request.cwd.clone(), request.mcp_servers)
            .map_err(internal_error)?;
        let response = NewSessionResponse::new(session_id.clone())
            .modes(session.modes())
            .models(session.models())
            .config_options(session.config_options());
        self.sessions
            .lock()
            .unwrap()
            .insert(session_id.clone(), session);
        Ok(response)
    }

    async fn load_session(
        &self,
        request: LoadSessionRequest,
        _cx: ConnectionTo<Client>,
    ) -> Result<LoadSessionResponse, Error> {
        if let Some(session) = self
            .sessions
            .lock()
            .unwrap()
            .get(&request.session_id)
            .cloned()
        {
            return Ok(load_session_response(&session));
        }

        let locator = TranscriptLocator::default_home().map_err(internal_error)?;
        let has_transcript = locator
            .find_transcript(request.session_id.0.as_ref())
            .map_err(internal_error)?
            .is_some();
        if !has_transcript {
            return Err(Error::resource_not_found(None));
        }

        let session = self
            .manager
            .load_session(
                request.session_id.clone(),
                request.cwd.clone(),
                request.mcp_servers,
            )
            .map_err(internal_error)?;
        let response = load_session_response(&session);
        self.sessions
            .lock()
            .unwrap()
            .insert(request.session_id, session);
        Ok(response)
    }

    async fn list_sessions(
        &self,
        request: ListSessionsRequest,
    ) -> Result<ListSessionsResponse, Error> {
        let cwd_filter = request.cwd.as_ref();
        let sessions = self
            .sessions
            .lock()
            .unwrap()
            .values()
            .filter(|session| cwd_filter.is_none_or(|cwd| cwd == session.cwd()))
            .map(|session| {
                SessionInfo::new(session.session_id().clone(), session.cwd().to_path_buf())
                    .title(Some("Claude Code CLI session".to_string()))
            })
            .collect();
        Ok(ListSessionsResponse::new(sessions))
    }

    async fn close_session(
        &self,
        request: CloseSessionRequest,
    ) -> Result<CloseSessionResponse, Error> {
        let session = self.sessions.lock().unwrap().remove(&request.session_id);
        if let Some(session) = session {
            session.shutdown().await.map_err(internal_error)?;
        }
        Ok(CloseSessionResponse::new())
    }

    async fn prompt(
        &self,
        request: PromptRequest,
        cx: ConnectionTo<Client>,
    ) -> Result<PromptResponse, Error> {
        let session_id = request.session_id.clone();
        let session = self.get_session(&session_id)?;
        let prompt = updates::prompt_text(&request);
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            updates::user_message_chunk(prompt.clone()),
        ))?;
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            updates::available_commands(session.cwd()),
        ))?;

        let permission_cx = cx.clone();
        let permission_session_id = session_id.clone();
        let mut options = TurnOptions::from_prompt_request(&request);
        options.startup_prompt_timeout = self.startup_prompt_timeout;
        let turn = session
            .prompt_with_permission_handler(prompt, options, move |permission| {
                let permission_cx = permission_cx.clone();
                let permission_session_id = permission_session_id.clone();
                async move {
                    let response = permission_cx
                        .send_request(updates::permission_request(
                            permission_session_id,
                            &permission,
                        ))
                        .block_task()
                        .await
                        .map_err(|err| anyhow::anyhow!("permission request failed: {err}"))?;
                    updates::permission_decision(&response.outcome)
                        .ok_or_else(|| anyhow::anyhow!("client returned unknown permission option"))
                }
            })
            .await
            .map_err(internal_error)?;

        let client_capabilities = self.client_capabilities.lock().unwrap().clone();
        let mut update_mapper =
            updates::TranscriptUpdateMapper::from_client(session.cwd(), &client_capabilities);
        for event in &turn.events {
            for update in update_mapper.updates_for_event(event) {
                cx.send_notification(SessionNotification::new(session_id.clone(), update))?;
            }
        }
        if turn.events.is_empty()
            && let Some(screen_text) = turn.screen_text.as_ref().filter(|text| !text.is_empty())
        {
            cx.send_notification(SessionNotification::new(
                session_id,
                updates::agent_message_chunk(screen_text.clone()),
            ))?;
        }

        Ok(PromptResponse::new(StopReason::EndTurn))
    }

    async fn cancel(&self, notification: CancelNotification) -> Result<(), Error> {
        self.get_session(&notification.session_id)?
            .cancel()
            .await
            .map_err(internal_error)
    }

    async fn set_session_mode(
        &self,
        request: SetSessionModeRequest,
    ) -> Result<SetSessionModeResponse, Error> {
        info!(
            "mode change requested for {}: {}",
            request.session_id, request.mode_id
        );
        self.get_session(&request.session_id)?
            .set_permission_mode(Some(request.mode_id.0.to_string()))
            .map_err(internal_error)?;
        Ok(SetSessionModeResponse::default())
    }

    async fn set_session_model(
        &self,
        request: SetSessionModelRequest,
    ) -> Result<SetSessionModelResponse, Error> {
        self.get_session(&request.session_id)?
            .set_model(Some(request.model_id.0.to_string()))
            .map_err(internal_error)?;
        Ok(SetSessionModelResponse::default())
    }

    async fn set_session_config_option(
        &self,
        request: SetSessionConfigOptionRequest,
    ) -> Result<SetSessionConfigOptionResponse, Error> {
        info!(
            "config option requested for {}: {}",
            request.session_id, request.config_id.0
        );
        let Some(value) = request.value.as_value_id() else {
            return Err(internal_error(anyhow::anyhow!(
                "config option requires a value id"
            )));
        };
        let session = self.get_session(&request.session_id)?;
        let _update = session
            .set_config_option(request.config_id.0.as_ref(), value)
            .map_err(internal_error)?;
        Ok(SetSessionConfigOptionResponse::new(
            session.config_options(),
        ))
    }

    fn get_session(&self, session_id: &SessionId) -> Result<Arc<ManagedSession>, Error> {
        self.sessions
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .ok_or_else(|| Error::resource_not_found(None))
    }
}

fn load_session_response(session: &ManagedSession) -> LoadSessionResponse {
    LoadSessionResponse::new()
        .modes(session.modes())
        .models(session.models())
        .config_options(session.config_options())
}

fn auth_methods(client_capabilities: &ClientCapabilities) -> Vec<AuthMethod> {
    let mut methods = vec![AuthMethod::Agent(
        AuthMethodAgent::new(CLAUDE_CODE_LOGIN_AUTH_ID, "Use Claude Code login").description(
            "Uses credentials managed by the installed Claude Code CLI. Run `claude` in a terminal first if authentication is missing.",
        ),
    )];

    if client_capabilities.auth.terminal {
        methods.push(AuthMethod::Terminal(
            AuthMethodTerminal::new(CLAUDE_CODE_TERMINAL_AUTH_ID, "Open Claude Code login")
                .description(
                    "Starts the adapter's interactive pass-through so Claude Code login can be completed in a terminal.",
                )
                .args(vec!["interactive".to_string()]),
        ));
    }

    methods
}

fn requested_session_id(
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<SessionId> {
    let meta = meta?;
    meta.get("claudeCode")
        .and_then(|value| value.get("sessionId"))
        .or_else(|| meta.get("sessionId"))
        .or_else(|| meta.get("session_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|value| SessionId::new(value.to_string()))
}

impl Default for AcpServer {
    fn default() -> Self {
        Self::new()
    }
}

fn internal_error(err: anyhow::Error) -> Error {
    Error::internal_error().data(err.to_string())
}
