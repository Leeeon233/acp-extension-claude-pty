use std::{path::PathBuf, str::FromStr, time::Duration};

use clap::ValueEnum;
use serde::Serialize;
use uuid::Uuid;

use crate::session::manager::{ManagedSession, SessionManager, TurnOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    StreamJson,
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "stream-json" => Ok(Self::StreamJson),
            other => Err(format!("unsupported output format: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrintRequest {
    pub prompt: String,
    pub output_format: OutputFormat,
    pub resume: Option<String>,
    pub continue_last: bool,
    pub session_id: Option<String>,
    pub cwd: Option<PathBuf>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub timeout: Duration,
    pub attach_on_timeout: bool,
    pub attach_on_permission: bool,
}

#[derive(Debug, Serialize)]
struct JsonOutput {
    session_id: String,
    text: String,
    model: Option<String>,
}

pub async fn run(request: PrintRequest) -> anyhow::Result<()> {
    let cwd = request.cwd.clone().unwrap_or(std::env::current_dir()?);
    let session_id = request
        .session_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let manager = SessionManager::new();
    let session = manager.create_print_session(session_id.clone(), cwd, request.model.clone())?;
    let turn = session
        .prompt(
            request.prompt,
            TurnOptions {
                timeout: request.timeout,
                model: request.model,
                permission_mode: request.permission_mode,
                resume: request.resume,
                continue_last: request.continue_last,
                initial_prompt_argument: false,
                attach_on_timeout: request.attach_on_timeout,
                attach_on_permission: request.attach_on_permission,
            },
        )
        .await?;
    let emit_result = emit_output(
        request.output_format,
        &session,
        &turn.final_text(),
        turn.model(),
    )
    .await;
    let shutdown_result = session.shutdown().await;
    emit_result?;
    shutdown_result
}

async fn emit_output(
    format: OutputFormat,
    session: &ManagedSession,
    text: &str,
    model: Option<String>,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Text => {
            write_stdout(format!("{text}\n"))?;
        }
        OutputFormat::Json => {
            write_stdout(format!(
                "{}\n",
                serde_json::to_string(&JsonOutput {
                    session_id: session.session_id().0.to_string(),
                    text: text.to_string(),
                    model,
                })?
            ))?;
        }
        OutputFormat::StreamJson => {
            write_stdout(format!(
                "{}\n",
                serde_json::to_string(&serde_json::json!({
                    "type": "message",
                    "session_id": session.session_id().0.to_string(),
                    "text": text,
                    "model": model,
                }))?
            ))?;
        }
    }
    Ok(())
}

fn write_stdout(output: String) -> anyhow::Result<()> {
    use std::io::Write;
    std::io::stdout().write_all(output.as_bytes())?;
    Ok(())
}
