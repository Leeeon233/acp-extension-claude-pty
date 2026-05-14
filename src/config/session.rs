use agent_client_protocol::schema::{
    ConfigOptionUpdate, CurrentModeUpdate, ModelInfo, SessionConfigOption,
    SessionConfigOptionCategory, SessionConfigSelectOption, SessionConfigValueId, SessionMode,
    SessionModeState, SessionModelState, SessionUpdate,
};

use crate::config::settings::ClaudeSettings;

const DEFAULT_MODEL_ID: &str = "default";
const DEFAULT_EFFORT: &str = "high";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionConfigState {
    mode: String,
    model: String,
    effort: String,
    models: Vec<ModelChoice>,
    allow_bypass: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChoice {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

impl SessionConfigState {
    pub fn from_settings(settings: &ClaudeSettings) -> Self {
        let allow_bypass = allow_bypass_permissions();
        let mut models = configured_models(settings.available_models.as_deref());
        let model = resolve_model(
            &models,
            std::env::var("ANTHROPIC_MODEL")
                .ok()
                .as_deref()
                .or(settings.model.as_deref()),
        )
        .unwrap_or_else(|| DEFAULT_MODEL_ID.to_string());
        if !models.iter().any(|choice| choice.id == model) {
            models.push(ModelChoice {
                id: model.clone(),
                name: model.clone(),
                description: None,
            });
        }
        let mode =
            resolve_permission_mode(settings.permissions.default_mode.as_deref(), allow_bypass);
        let effort = normalize_effort(settings.effort_level.as_deref())
            .unwrap_or_else(|| DEFAULT_EFFORT.to_string());

        Self {
            mode,
            model,
            effort,
            models,
            allow_bypass,
        }
    }

    pub fn mode(&self) -> &str {
        &self.mode
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn effort(&self) -> &str {
        &self.effort
    }

    pub fn modes(&self) -> SessionModeState {
        SessionModeState::new(self.mode.clone(), available_modes(self.allow_bypass))
    }

    pub fn models(&self) -> SessionModelState {
        SessionModelState::new(
            self.model.clone(),
            self.models
                .iter()
                .map(|choice| {
                    ModelInfo::new(choice.id.clone(), choice.name.clone())
                        .description(choice.description.clone())
                })
                .collect(),
        )
    }

    pub fn config_options(&self) -> Vec<SessionConfigOption> {
        vec![
            SessionConfigOption::select(
                "mode",
                "Mode",
                self.mode.clone(),
                available_modes(self.allow_bypass)
                    .into_iter()
                    .map(|mode| {
                        SessionConfigSelectOption::new(mode.id.0.to_string(), mode.name)
                            .description(mode.description)
                    })
                    .collect::<Vec<_>>(),
            )
            .description("Session permission mode")
            .category(SessionConfigOptionCategory::Mode),
            SessionConfigOption::select(
                "model",
                "Model",
                self.model.clone(),
                self.models
                    .iter()
                    .map(|choice| {
                        SessionConfigSelectOption::new(choice.id.clone(), choice.name.clone())
                            .description(choice.description.clone())
                    })
                    .collect::<Vec<_>>(),
            )
            .description("Claude model to use")
            .category(SessionConfigOptionCategory::Model),
            SessionConfigOption::select(
                "effort",
                "Effort",
                self.effort.clone(),
                ["low", "medium", "high", "xhigh", "max"]
                    .into_iter()
                    .map(|level| SessionConfigSelectOption::new(level, effort_label(level)))
                    .collect::<Vec<_>>(),
            )
            .description("Claude reasoning effort level")
            .category(SessionConfigOptionCategory::ThoughtLevel),
        ]
    }

    pub fn set_mode(&mut self, mode: &str) -> anyhow::Result<()> {
        let resolved = resolve_permission_mode(Some(mode), self.allow_bypass);
        if resolved == "default" && !mode.trim().eq_ignore_ascii_case("default") {
            anyhow::bail!("invalid permission mode: {mode}");
        }
        self.mode = resolved;
        Ok(())
    }

    pub fn set_model(&mut self, model: &str) -> anyhow::Result<String> {
        let Some(resolved) = resolve_model(&self.models, Some(model)) else {
            anyhow::bail!("invalid model: {model}");
        };
        self.model = resolved.clone();
        Ok(resolved)
    }

    pub fn set_effort(&mut self, effort: &str) -> anyhow::Result<()> {
        let Some(resolved) = normalize_effort(Some(effort)) else {
            anyhow::bail!("invalid effort: {effort}");
        };
        self.effort = resolved;
        Ok(())
    }

    pub fn set_option(
        &mut self,
        config_id: &str,
        value: &SessionConfigValueId,
    ) -> anyhow::Result<Option<SessionUpdate>> {
        match config_id {
            "mode" => {
                self.set_mode(value.0.as_ref())?;
                Ok(Some(SessionUpdate::CurrentModeUpdate(
                    CurrentModeUpdate::new(self.mode.clone()),
                )))
            }
            "model" => {
                self.set_model(value.0.as_ref())?;
                Ok(Some(SessionUpdate::ConfigOptionUpdate(
                    ConfigOptionUpdate::new(self.config_options()),
                )))
            }
            "effort" => {
                self.set_effort(value.0.as_ref())?;
                Ok(Some(SessionUpdate::ConfigOptionUpdate(
                    ConfigOptionUpdate::new(self.config_options()),
                )))
            }
            _ => anyhow::bail!("unknown config option: {config_id}"),
        }
    }
}

pub fn resolve_permission_mode(default_mode: Option<&str>, allow_bypass: bool) -> String {
    let Some(default_mode) = default_mode else {
        return "default".to_string();
    };
    let normalized = default_mode.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "auto" => "auto".to_string(),
        "default" => "default".to_string(),
        "acceptedits" => "acceptEdits".to_string(),
        "dontask" => "dontAsk".to_string(),
        "plan" => "plan".to_string(),
        "bypasspermissions" | "bypass" if allow_bypass => "bypassPermissions".to_string(),
        _ => "default".to_string(),
    }
}

pub fn available_modes(allow_bypass: bool) -> Vec<SessionMode> {
    let mut modes = vec![
        SessionMode::new("auto", "Auto")
            .description("Use Claude's auto mode for permission prompts"),
        SessionMode::new("default", "Default")
            .description("Standard behavior, prompts for dangerous operations"),
        SessionMode::new("acceptEdits", "Accept Edits")
            .description("Auto-accept file edit operations"),
        SessionMode::new("plan", "Plan Mode").description("Planning mode, no tool execution"),
        SessionMode::new("dontAsk", "Don't Ask")
            .description("Deny unapproved permission prompts instead of asking"),
    ];
    if allow_bypass {
        modes.push(
            SessionMode::new("bypassPermissions", "Bypass Permissions")
                .description("Bypass all permission checks"),
        );
    }
    modes
}

pub fn allow_bypass_permissions() -> bool {
    !running_as_root() || std::env::var_os("IS_SANDBOX").is_some()
}

fn configured_models(allowlist: Option<&[String]>) -> Vec<ModelChoice> {
    let mut models = vec![ModelChoice {
        id: DEFAULT_MODEL_ID.to_string(),
        name: "Default".to_string(),
        description: Some("Claude Code default model".to_string()),
    }];

    let configured = allowlist
        .map(|models| models.to_vec())
        .unwrap_or_else(|| vec!["sonnet".to_string(), "opus".to_string()]);
    for model in configured {
        let trimmed = model.trim();
        if trimmed.is_empty() || models.iter().any(|choice| choice.id == trimmed) {
            continue;
        }
        models.push(ModelChoice {
            id: trimmed.to_string(),
            name: model_label(trimmed),
            description: None,
        });
    }
    models
}

fn resolve_model(models: &[ModelChoice], preference: Option<&str>) -> Option<String> {
    let preference = preference?.trim();
    if preference.is_empty() {
        return None;
    }
    let lower = preference.to_ascii_lowercase();
    models
        .iter()
        .find(|model| {
            let id = model.id.to_ascii_lowercase();
            let name = model.name.to_ascii_lowercase();
            id == lower || name == lower || id.contains(&lower) || lower.contains(&id)
        })
        .map(|model| model.id.clone())
}

fn normalize_effort(effort: Option<&str>) -> Option<String> {
    match effort?.trim().to_ascii_lowercase().as_str() {
        "low" => Some("low".to_string()),
        "medium" => Some("medium".to_string()),
        "high" => Some("high".to_string()),
        "xhigh" | "extra-high" | "extra_high" => Some("xhigh".to_string()),
        "max" => Some("max".to_string()),
        _ => None,
    }
}

fn model_label(model: &str) -> String {
    if model == DEFAULT_MODEL_ID {
        return "Default".to_string();
    }
    let mut chars = model.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => model.to_string(),
    }
}

fn effort_label(level: &str) -> String {
    match level {
        "xhigh" => "XHigh".to_string(),
        _ => model_label(level),
    }
}

#[cfg(unix)]
fn running_as_root() -> bool {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() == 0 }
}

#[cfg(not(unix))]
fn running_as_root() -> bool {
    false
}
