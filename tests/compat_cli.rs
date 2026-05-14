use std::fs;
use std::path::Path;

use claude_code_cli_acp::compat::claude_probe::ClaudeCli;

#[cfg(unix)]
fn write_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, body).expect("write fake cli");
    let mut permissions = fs::metadata(path).expect("fake cli metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod fake cli");
}

#[tokio::test]
#[cfg(unix)]
async fn claude_probe_uses_env_override_and_reports_required_flags() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = temp.path().join("claude-fake");
    write_executable(
        &fake,
        r#"#!/bin/sh
case "$1" in
  --version) printf '2.1.140 (Claude Code)\n' ;;
  --help) cat <<'EOF'
Usage: claude [options]
  --session-id <uuid>
  --resume
  --continue
  --model <model>
  --permission-mode <mode>
  --settings <path>
  --add-dir <path>
  --debug-file <path>
  --output-format <format>
  --input-format <format>
  --mcp-config <path>
  --strict-mcp-config
  --help
  --version
EOF
    ;;
  *) printf 'unexpected %s\n' "$*" >&2; exit 2 ;;
esac
"#,
    );

    let cli = ClaudeCli::new(fake);
    assert_eq!(cli.version().expect("version"), "2.1.140 (Claude Code)");
    let report = cli.capability_report().await;

    assert!(report.required_flags.iter().all(|flag| flag.present));
    assert_eq!(
        report.version.as_deref().expect("version"),
        "2.1.140 (Claude Code)"
    );
    assert!(
        report
            .required_flags
            .iter()
            .any(|flag| flag.name == "--session-id")
    );
    assert!(
        report
            .removed_flags
            .iter()
            .any(|flag| flag.name == "--enable-auto-mode")
    );
    assert!(!report.launch_uses_print_flag);

    serde_json::to_string_pretty(&report).expect("serializable report");
}

#[test]
fn required_flag_list_matches_current_research_surface() {
    let required = ClaudeCli::required_flags();
    let names = required
        .iter()
        .map(|flag| flag.name.as_str())
        .collect::<Vec<_>>();

    for expected in [
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
    ] {
        assert!(
            names.contains(&expected),
            "missing required flag {expected}"
        );
    }

    assert!(!names.contains(&"--enable-auto-mode"));
}
