use std::{ffi::OsString, process::Stdio};

use tokio::process::Command;

use crate::compat::claude_probe::ClaudeCli;

pub async fn run(args: Vec<OsString>) -> anyhow::Result<i32> {
    let cli = ClaudeCli::from_env();
    let mut command = Command::new(cli.executable());
    command.args(args);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    let status = command.status().await?;
    Ok(status.code().unwrap_or(1))
}
