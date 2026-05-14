use agent_client_protocol::schema::SessionConfigKind;
use claude_code_cli_acp::config::{
    session::{SessionConfigState, resolve_permission_mode},
    settings::{ClaudeSettings, PermissionSettings},
};

#[test]
fn resolves_permission_mode_aliases_with_default_fallback() {
    assert_eq!(resolve_permission_mode(None, true), "default");
    assert_eq!(resolve_permission_mode(Some("default"), true), "default");
    assert_eq!(
        resolve_permission_mode(Some("AcceptEdits"), true),
        "acceptEdits"
    );
    assert_eq!(resolve_permission_mode(Some("DONTASK"), true), "dontAsk");
    assert_eq!(resolve_permission_mode(Some("plan"), true), "plan");
    assert_eq!(
        resolve_permission_mode(Some("bypass"), true),
        "bypassPermissions"
    );
    assert_eq!(resolve_permission_mode(Some("bypass"), false), "default");
    assert_eq!(resolve_permission_mode(Some("unknown"), true), "default");
    assert_eq!(resolve_permission_mode(Some("  "), true), "default");
}

#[test]
fn session_config_reflects_settings_models_effort_and_mode() {
    let settings = ClaudeSettings {
        permissions: PermissionSettings {
            default_mode: Some("DontAsk".to_string()),
        },
        model: Some("opus".to_string()),
        effort_level: Some("XHIGH".to_string()),
        available_models: Some(vec!["sonnet".to_string(), "opus[1m]".to_string()]),
        ..ClaudeSettings::default()
    };

    let config = SessionConfigState::from_settings(&settings);

    assert_eq!(config.mode(), "dontAsk");
    assert_eq!(config.model(), "opus[1m]");
    assert_eq!(config.effort(), "xhigh");
    let options = config.config_options();
    assert_eq!(options.len(), 3);
    assert!(options.iter().any(|option| option.id.0.as_ref() == "mode"));
    assert!(options.iter().any(|option| option.id.0.as_ref() == "model"));
    assert!(
        options
            .iter()
            .any(|option| option.id.0.as_ref() == "effort")
    );

    let mode = options
        .iter()
        .find(|option| option.id.0.as_ref() == "mode")
        .expect("mode option");
    let SessionConfigKind::Select(select) = &mode.kind else {
        panic!("mode should be select");
    };
    assert_eq!(select.current_value.0.as_ref(), "dontAsk");
}

#[test]
fn set_config_options_validate_values_and_update_current_state() {
    let mut config = SessionConfigState::from_settings(&ClaudeSettings::default());

    config.set_mode("plan").expect("set plan mode");
    config.set_model("sonnet").expect("set sonnet model");
    config.set_effort("medium").expect("set effort");

    assert_eq!(config.mode(), "plan");
    assert_eq!(config.model(), "sonnet");
    assert_eq!(config.effort(), "medium");
    assert!(config.set_mode("invalid").is_err());
    assert!(config.set_model("missing").is_err());
    assert!(config.set_effort("tiny").is_err());
}
