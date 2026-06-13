use std::{collections::BTreeMap, fs};

use acp_extension_claude_pty::config::settings::{SettingsPaths, load_merged_settings};

#[test]
fn settings_merge_sources_with_claude_precedence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let cwd = temp.path().join("repo");
    let managed = temp.path().join("managed-settings.json");
    fs::create_dir_all(home.join(".claude")).expect("home claude dir");
    fs::create_dir_all(cwd.join(".claude")).expect("project claude dir");

    fs::write(
        home.join(".claude/settings.json"),
        r#"{
          "env": {"A": "user", "B": "user"},
          "model": "sonnet",
          "effortLevel": "medium",
          "availableModels": ["sonnet", "opus"],
          "permissions": {"defaultMode": "default"}
        }"#,
    )
    .expect("write user settings");
    fs::write(
        cwd.join(".claude/settings.json"),
        r#"{
          "env": {"B": "project", "C": "project"},
          "availableModels": ["opus", "haiku"],
          "permissions": {"defaultMode": "dontAsk"}
        }"#,
    )
    .expect("write project settings");
    fs::write(
        cwd.join(".claude/settings.local.json"),
        r#"{"model": "opus", "effortLevel": "high"}"#,
    )
    .expect("write local settings");
    fs::write(
        &managed,
        r#"{"env": {"C": "managed"}, "availableModels": ["custom"]}"#,
    )
    .expect("write managed settings");

    let load =
        load_merged_settings(&SettingsPaths::for_cwd_and_home(&cwd, &home).with_managed(managed));

    assert!(load.warnings.is_empty());
    assert_eq!(
        load.settings.env,
        BTreeMap::from([
            ("A".to_string(), "user".to_string()),
            ("B".to_string(), "project".to_string()),
            ("C".to_string(), "managed".to_string()),
        ])
    );
    assert_eq!(load.settings.model.as_deref(), Some("opus"));
    assert_eq!(load.settings.effort_level.as_deref(), Some("high"));
    assert_eq!(
        load.settings.available_models,
        Some(vec![
            "sonnet".to_string(),
            "opus".to_string(),
            "haiku".to_string(),
            "custom".to_string()
        ])
    );
    assert_eq!(
        load.settings.permissions.default_mode.as_deref(),
        Some("dontAsk")
    );
}

#[test]
fn settings_ignores_missing_files_and_reports_invalid_json_without_body() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let cwd = temp.path().join("repo");
    fs::create_dir_all(home.join(".claude")).expect("home claude dir");
    fs::write(home.join(".claude/settings.json"), "{not-json").expect("invalid settings");

    let load = load_merged_settings(&SettingsPaths::for_cwd_and_home(&cwd, &home));

    assert_eq!(load.warnings.len(), 1);
    assert!(load.warnings[0].contains("settings.json"));
    assert!(!load.warnings[0].contains("not-json"));
    assert!(load.settings.env.is_empty());
}
