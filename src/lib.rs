//! ACP Extension Claude PTY adapter.
//!
//! The adapter exposes ACP over stdio while driving the installed `claude`
//! executable through a PTY and reading Claude's JSONL transcript as the
//! canonical content stream.
#![deny(clippy::print_stdout, clippy::print_stderr)]

use std::ffi::OsString;

pub mod acp;
pub mod cli;
pub mod compat;
pub mod config;
pub mod doctor;
pub mod error;
pub mod interactive;
pub mod print_mode;
pub mod pty;
pub mod session;
pub mod terminal;
pub mod transcript;

pub async fn run_main(args: impl IntoIterator<Item = OsString>) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    cli::run(args).await
}
