use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    claude_code_cli_acp::run_main(std::env::args_os()).await
}
