use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionDialog {
    pub title: String,
    pub options: Vec<PermissionDialogOption>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionDialogOption {
    pub ordinal: usize,
    pub accelerator: Option<String>,
    pub label: String,
    pub decision: PermissionDecision,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    AllowOnce,
    AllowAlways,
    Reject,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScreenStatus {
    Idle,
    Thinking,
    WorkspaceTrust,
    Permission,
    Error,
    Exited,
    Unknown,
}

pub fn recognize_screen(text: &str) -> ScreenStatus {
    if recognize_exit(text) {
        ScreenStatus::Exited
    } else if recognize_error(text) {
        ScreenStatus::Error
    } else if recognize_workspace_trust(text) {
        ScreenStatus::WorkspaceTrust
    } else if recognize_permission(text) {
        ScreenStatus::Permission
    } else if recognize_thinking(text) {
        ScreenStatus::Thinking
    } else if recognize_idle(text) {
        ScreenStatus::Idle
    } else {
        ScreenStatus::Unknown
    }
}

pub fn recognize_idle(text: &str) -> bool {
    let normalized = normalize(text);
    normalized.contains("\n>")
        || normalized.contains("│ >")
        || normalized.ends_with("> ")
        || normalized.contains("\n❯")
        || normalized.ends_with("❯ ")
}

pub fn recognize_thinking(text: &str) -> bool {
    let normalized = normalize(text);
    normalized.contains("thinking")
        || normalized.contains("working")
        || normalized.contains("esc to interrupt")
}

pub fn recognize_permission(text: &str) -> bool {
    recognize_permission_dialog(text).is_some()
}

pub fn recognize_permission_dialog(text: &str) -> Option<PermissionDialog> {
    let normalized = normalize(text);
    if !(normalized.contains("permission")
        || normalized.contains("allow this action")
        || normalized.contains("do you want"))
    {
        return None;
    }

    let options = text
        .lines()
        .filter_map(parse_permission_option)
        .collect::<Vec<_>>();
    if options.is_empty() {
        return None;
    }

    Some(PermissionDialog {
        title: permission_title(text),
        options,
    })
}

pub fn recognize_workspace_trust(text: &str) -> bool {
    let normalized = normalize(text);
    normalized.contains("is this a project you created or one you trust")
        && normalized.contains("yes, i trust this folder")
        && normalized.contains("no, exit")
}

pub fn recognize_error(text: &str) -> bool {
    let normalized = normalize(text);
    normalized.contains("error:") || normalized.contains("failed") || normalized.contains("panic")
}

pub fn recognize_exit(text: &str) -> bool {
    let normalized = normalize(text);
    normalized.contains("goodbye")
        || normalized.contains("session ended")
        || normalized.contains("exited")
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
}

fn parse_permission_option(line: &str) -> Option<PermissionDialogOption> {
    let trimmed = line.trim_start_matches([' ', '❯']).trim();
    let (ordinal, rest) = parse_option_prefix(trimmed)?;
    let label = rest.trim().to_string();
    let decision = classify_permission_label(&label)?;
    Some(PermissionDialogOption {
        ordinal,
        accelerator: Some(ordinal.to_string()),
        label,
        decision,
    })
}

fn parse_option_prefix(line: &str) -> Option<(usize, &str)> {
    if let Some(rest) = line.strip_prefix('[') {
        let (number, rest) = rest.split_once(']')?;
        return Some((number.parse().ok()?, rest));
    }

    let mut digits_end = 0;
    for (index, character) in line.char_indices() {
        if character.is_ascii_digit() {
            digits_end = index + character.len_utf8();
        } else {
            break;
        }
    }
    if digits_end == 0 {
        return None;
    }
    let rest = line[digits_end..].trim_start();
    let rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    Some((line[..digits_end].parse().ok()?, rest))
}

fn classify_permission_label(label: &str) -> Option<PermissionDecision> {
    let normalized = normalize(label);
    if normalized.contains("always")
        || normalized.contains("don't ask")
        || normalized.contains("do not ask")
        || normalized.contains("remember")
        || normalized.contains("future")
        || normalized.contains("session")
    {
        Some(PermissionDecision::AllowAlways)
    } else if normalized.contains("deny")
        || normalized.contains("no")
        || normalized.contains("reject")
        || normalized.contains("decline")
        || normalized.contains("abort")
    {
        Some(PermissionDecision::Reject)
    } else if normalized.contains("allow")
        || normalized.contains("yes")
        || normalized.contains("proceed")
    {
        Some(PermissionDecision::AllowOnce)
    } else {
        None
    }
}

fn permission_title(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| {
            let normalized = normalize(line);
            normalized.contains("do you want") || normalized.contains("permission")
        })
        .unwrap_or("Claude permission request")
        .to_string()
}
