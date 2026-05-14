use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};

pub const CLAUDE_CODE_CLI_ENV: &str = "CLAUDE_CODE_CLI";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeCli {
    pub executable: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequiredFlag {
    pub name: String,
    pub source: CapabilitySource,
    pub present: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemovedFlag {
    pub name: String,
    pub replacement: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySource {
    LocalHelp,
    OfficialDocs,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeCapabilityReport {
    pub executable: PathBuf,
    pub version: Result<String, String>,
    pub local_help: Result<String, String>,
    pub required_flags: Vec<RequiredFlag>,
    pub removed_flags: Vec<RemovedFlag>,
    pub required_interactive_commands: Vec<String>,
    pub launch_uses_print_flag: bool,
}

impl ClaudeCli {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    pub fn from_env_or_path() -> Self {
        if let Some(executable) = std::env::var_os(CLAUDE_CODE_CLI_ENV) {
            return Self::new(executable);
        }

        match which::which("claude") {
            Ok(path) => Self::new(path),
            Err(_) => Self::new("claude"),
        }
    }

    pub fn from_env() -> Self {
        Self::from_env_or_path()
    }

    pub fn executable(&self) -> &std::path::Path {
        &self.executable
    }

    pub fn version(&self) -> anyhow::Result<String> {
        self.run_single_arg("--version")
    }

    pub fn help(&self) -> anyhow::Result<String> {
        self.run_single_arg("--help")
    }

    pub fn required_flags() -> Vec<RequiredFlag> {
        [
            "--session-id",
            "--resume",
            "--continue",
            "--model",
            "--permission-mode",
            "--settings",
            "--add-dir",
            "--debug-file",
            "--version",
            "--help",
            "--output-format",
            "--input-format",
            "--mcp-config",
            "--strict-mcp-config",
        ]
        .into_iter()
        .map(|name| RequiredFlag {
            name: name.to_string(),
            source: CapabilitySource::OfficialDocs,
            present: false,
        })
        .collect()
    }

    pub fn removed_flags() -> Vec<RemovedFlag> {
        vec![RemovedFlag {
            name: "--enable-auto-mode".to_string(),
            replacement: "--permission-mode auto".to_string(),
        }]
    }

    pub fn required_interactive_commands() -> Vec<String> {
        ["/exit", "/clear", "/resume"]
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    pub async fn capability_report(&self) -> ClaudeCapabilityReport {
        let version = self.version().map_err(|error| error.to_string());
        let help = self.help().map_err(|error| error.to_string());
        let help_text = help.as_deref().unwrap_or_default();
        let required_flags = Self::required_flags()
            .into_iter()
            .map(|mut flag| {
                flag.present = help_contains_flag(help_text, &flag.name);
                flag.source = CapabilitySource::LocalHelp;
                flag
            })
            .collect();

        ClaudeCapabilityReport {
            executable: self.executable.clone(),
            version,
            local_help: help,
            required_flags,
            removed_flags: Self::removed_flags(),
            required_interactive_commands: Self::required_interactive_commands(),
            launch_uses_print_flag: false,
        }
    }

    fn run_single_arg(&self, arg: &str) -> anyhow::Result<String> {
        let output = Command::new(&self.executable)
            .arg(arg)
            .output()
            .with_context(|| format!("failed to run {}", self.executable.display()))?;

        if !output.status.success() {
            return Err(anyhow!(
                "{} {arg} exited with status {}: {}",
                self.executable.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let stdout = String::from_utf8(output.stdout).with_context(|| {
            format!(
                "{} {arg} emitted non-utf8 stdout",
                self.executable.display()
            )
        })?;
        Ok(stdout.trim().to_string())
    }

    pub fn command_argv(&self, arg: impl Into<OsString>) -> Vec<OsString> {
        vec![self.executable.clone().into_os_string(), arg.into()]
    }
}

impl ClaudeCapabilityReport {
    pub fn local_help_contains(&self, flag: &RequiredFlag) -> bool {
        self.local_help
            .as_deref()
            .map(|help| help_contains_flag(help, &flag.name))
            .unwrap_or(false)
    }
}

impl std::fmt::Display for RequiredFlag {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.name)
    }
}

fn help_contains_flag(help: &str, flag: &str) -> bool {
    help.split(|c: char| c.is_whitespace() || c == ',' || c == '[' || c == ']')
        .any(|token| token == flag || token.starts_with(&format!("{flag}=")))
}
