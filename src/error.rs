use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("failed to locate claude executable")]
    ClaudeNotFound,
    #[error("claude command failed: {0}")]
    ClaudeCommand(String),
    #[error("transcript for session {session_id} was not found under {root}")]
    TranscriptNotFound { session_id: String, root: PathBuf },
    #[error("timed out waiting for claude session {session_id}")]
    Timeout { session_id: String },
    #[error("transcript persistence is disabled; transcript extraction cannot work")]
    TranscriptPersistenceDisabled,
    #[error("pty error: {0}")]
    Pty(String),
}

pub type Result<T> = std::result::Result<T, AdapterError>;
