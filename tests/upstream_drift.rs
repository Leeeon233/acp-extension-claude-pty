use acp_extension_claude_pty::compat::docs_probe::{LiveProbe, probe_live};

#[tokio::test]
#[ignore = "live docs/npm drift check"]
async fn live_claude_package_and_docs_match_capability_matrix() {
    let report = probe_live().await.expect("live probe");
    match &report.npm {
        LiveProbe::Available { value } => {
            assert_eq!(value.package, "@anthropic-ai/claude-code");
            assert!(value.latest.is_some(), "missing latest dist-tag");
        }
        LiveProbe::Unavailable { reason } => panic!("npm unavailable: {reason}"),
    }
    match &report.docs {
        LiveProbe::Available { value } => {
            assert!(
                value.missing_required_flags.is_empty(),
                "official Claude docs no longer include required adapter flags {:?}; update compatibility matrix / adapter parser",
                value.missing_required_flags
            );
        }
        LiveProbe::Unavailable { reason } => panic!("docs unavailable: {reason}"),
    }
}
