use std::{ffi::OsString, io::IsTerminal, io::Read, io::Write, time::Duration};

use clap::Parser;

use crate::{acp::server::AcpServer, doctor, interactive, print_mode};

#[derive(Debug, Parser)]
#[command(name = "claude-code-cli-acp")]
#[command(about = "ACP adapter and CLI wrapper for Claude Code")]
struct PrintCommand {
    #[arg()]
    prompt: Vec<String>,
    #[arg(long, default_value = "text")]
    output_format: print_mode::OutputFormat,
    #[arg(long)]
    resume: Option<String>,
    #[arg(long = "continue", default_value_t = false)]
    continue_last: bool,
    #[arg(long)]
    session_id: Option<String>,
    #[arg(long)]
    cwd: Option<std::path::PathBuf>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    permission_mode: Option<String>,
    #[arg(long, default_value_t = 120)]
    timeout: u64,
    #[arg(long, default_value_t = false)]
    attach_on_timeout: bool,
    #[arg(long, default_value_t = false)]
    attach_on_permission: bool,
}

pub async fn run(args: impl IntoIterator<Item = OsString>) -> anyhow::Result<()> {
    let mut args = args.into_iter().collect::<Vec<_>>();
    let _program = args
        .first()
        .cloned()
        .unwrap_or_else(|| "claude-code-cli-acp".into());
    if !args.is_empty() {
        args.remove(0);
    }

    match first_arg(&args).as_deref() {
        None if std::io::stdin().is_terminal() => {
            let status = interactive::run(Vec::new()).await?;
            std::process::exit(status);
        }
        None => AcpServer::new().serve_stdio().await.map_err(Into::into),
        Some("acp") => AcpServer::new().serve_stdio().await.map_err(Into::into),
        Some("interactive") => {
            let forwarded = strip_command_and_separator(args);
            let status = interactive::run(forwarded).await?;
            std::process::exit(status);
        }
        Some("doctor") => {
            let live_docs = args
                .iter()
                .any(|arg| arg == "--live-docs" || arg == "--check-upstream");
            doctor::run(live_docs).await
        }
        Some("--version") | Some("-V") => {
            let mut stdout = std::io::stdout().lock();
            writeln!(
                stdout,
                "{} {}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION")
            )?;
            Ok(())
        }
        Some("print") => {
            let print_args = std::iter::once(OsString::from("print"))
                .chain(args.into_iter().skip(1))
                .collect::<Vec<_>>();
            let command = PrintCommand::parse_from(print_args);
            let prompt = read_print_prompt(command.prompt)?;
            let request = print_mode::PrintRequest {
                prompt,
                output_format: command.output_format,
                resume: command.resume,
                continue_last: command.continue_last,
                session_id: command.session_id,
                cwd: command.cwd,
                model: command.model,
                permission_mode: command.permission_mode,
                timeout: Duration::from_secs(command.timeout),
                attach_on_timeout: command.attach_on_timeout,
                attach_on_permission: command.attach_on_permission,
            };
            print_mode::run(request).await
        }
        Some("--") => {
            let status = interactive::run(args.into_iter().skip(1).collect()).await?;
            std::process::exit(status);
        }
        Some(_) => {
            let status = interactive::run(args).await?;
            std::process::exit(status);
        }
    }
}

fn read_print_prompt(args: Vec<String>) -> anyhow::Result<String> {
    let arg_prompt = args.join(" ");
    if std::io::stdin().is_terminal() {
        return Ok(arg_prompt);
    }

    let mut stdin_prompt = String::new();
    std::io::stdin().read_to_string(&mut stdin_prompt)?;
    let stdin_prompt = stdin_prompt.trim_end_matches(['\r', '\n']);
    match (arg_prompt.is_empty(), stdin_prompt.is_empty()) {
        (true, true) => Ok(String::new()),
        (false, true) => Ok(arg_prompt),
        (true, false) => Ok(stdin_prompt.to_string()),
        (false, false) => Ok(format!("{arg_prompt}\n\n{stdin_prompt}")),
    }
}

fn first_arg(args: &[OsString]) -> Option<String> {
    args.first().map(|arg| arg.to_string_lossy().into_owned())
}

fn strip_command_and_separator(args: Vec<OsString>) -> Vec<OsString> {
    let mut forwarded = args.into_iter().skip(1).collect::<Vec<_>>();
    if forwarded.first().is_some_and(|arg| arg == "--") {
        forwarded.remove(0);
    }
    forwarded
}
