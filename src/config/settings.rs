use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSettings {
    #[serde(default)]
    pub permissions: PermissionSettings,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub model: Option<String>,
    #[serde(rename = "effortLevel")]
    pub effort_level: Option<String>,
    pub available_models: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSettings {
    pub default_mode: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsPaths {
    pub user: PathBuf,
    pub project: PathBuf,
    pub local: PathBuf,
    pub managed: PathBuf,
}

impl SettingsPaths {
    pub fn for_cwd(cwd: impl AsRef<Path>) -> anyhow::Result<Self> {
        let home = dirs::home_dir().context("home directory unavailable")?;
        Ok(Self::for_cwd_and_home(cwd, home))
    }

    pub fn for_cwd_and_home(cwd: impl AsRef<Path>, home: impl AsRef<Path>) -> Self {
        let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.as_ref().join(".claude"));
        Self {
            user: config_dir.join("settings.json"),
            project: cwd.as_ref().join(".claude/settings.json"),
            local: cwd.as_ref().join(".claude/settings.local.json"),
            managed: managed_settings_path(),
        }
    }

    pub fn with_managed(mut self, managed: impl Into<PathBuf>) -> Self {
        self.managed = managed.into();
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsLoad {
    pub settings: ClaudeSettings,
    pub warnings: Vec<String>,
}

pub fn load_merged_settings(paths: &SettingsPaths) -> SettingsLoad {
    let mut load = SettingsLoad::default();
    for path in [&paths.user, &paths.project, &paths.local, &paths.managed] {
        match load_settings_file(path) {
            Ok(Some(settings)) => merge_settings(&mut load.settings, settings),
            Ok(None) => {}
            Err(err) => load.warnings.push(format!(
                "failed to load settings from {}: {err}",
                path.display()
            )),
        }
    }
    load
}

fn load_settings_file(path: &Path) -> anyhow::Result<Option<ClaudeSettings>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents)
            .with_context(|| format!("parse settings {}", path.display()))
            .map(Some),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("read settings {}", path.display())),
    }
}

fn merge_settings(merged: &mut ClaudeSettings, settings: ClaudeSettings) {
    merged.env.extend(settings.env);
    if let Some(model) = settings.model {
        merged.model = Some(model);
    }
    if let Some(effort_level) = settings.effort_level {
        merged.effort_level = Some(effort_level);
    }
    if let Some(default_mode) = settings.permissions.default_mode {
        merged.permissions.default_mode = Some(default_mode);
    }
    if let Some(models) = settings.available_models {
        let existing = merged.available_models.get_or_insert_with(Vec::new);
        for model in models {
            if !existing.contains(&model) {
                existing.push(model);
            }
        }
    }
}

fn managed_settings_path() -> PathBuf {
    if let Some(path) = std::env::var_os("CLAUDE_CODE_ACP_MANAGED_SETTINGS") {
        return path.into();
    }

    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Library/Application Support/ClaudeCode/managed-settings.json")
    }
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(r"C:\Program Files\ClaudeCode\managed-settings.json")
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        PathBuf::from("/etc/claude-code/managed-settings.json")
    }
}
