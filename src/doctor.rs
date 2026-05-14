use crate::compat::{claude_probe::ClaudeCli, docs_probe};

pub async fn run(live_docs: bool) -> anyhow::Result<()> {
    let mut output = String::new();
    let cli = ClaudeCli::from_env();
    let report = cli.capability_report().await;
    push_line(
        &mut output,
        format!("Claude executable: {}", cli.executable().display()),
    );
    match &report.version {
        Ok(version) => push_line(&mut output, format!("Claude version: {version}")),
        Err(err) => push_line(&mut output, format!("Claude version: unavailable ({err})")),
    }
    push_line(&mut output, "Required flags:");
    for flag in &report.required_flags {
        push_line(
            &mut output,
            format!(
                "  {}: {}",
                flag.name,
                if flag.present {
                    "local-help"
                } else {
                    "missing-from-local-help"
                }
            ),
        );
    }
    if let Err(err) = &report.local_help {
        push_line(&mut output, format!("Claude help: unavailable ({err})"));
    }
    push_line(
        &mut output,
        "Transcript path: ~/.claude/projects/<project>/<session>.jsonl",
    );
    push_line(&mut output, "Transcript logging: redacted by default");
    push_line(&mut output, "ACP readiness: stdio server available");
    push_line(&mut output, "PTY readiness: portable-pty configured");

    if live_docs {
        match docs_probe::probe_live().await {
            Ok(live) => push_line(&mut output, format!("Upstream: {}", live.summary())),
            Err(err) => push_line(&mut output, format!("Upstream: unavailable ({err})")),
        }
    }

    use std::io::Write;
    std::io::stderr().write_all(output.as_bytes())?;
    Ok(())
}

fn push_line(output: &mut String, line: impl std::fmt::Display) {
    use std::fmt::Write;
    let _ = writeln!(output, "{line}");
}
