use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    acp_extension_claude_pty::run_main(std::env::args_os()).await
}
