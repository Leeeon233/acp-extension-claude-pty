use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptEventKind {
    UserMessage,
    AssistantMessage,
    AssistantThought,
    System,
    ToolUse,
    ToolResult,
    Diagnostic,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptEvent {
    pub uuid: String,
    pub session_id: Option<String>,
    pub kind: TranscriptEventKind,
    pub text: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(skip)]
    pub raw_input: Option<Value>,
    #[serde(skip)]
    pub raw_output: Option<Value>,
    pub redacted: RedactionSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedactionSummary {
    pub text_redacted: bool,
    pub char_count: usize,
    pub line_count: usize,
    pub value_kind: Option<String>,
}

impl std::fmt::Debug for TranscriptEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TranscriptEvent")
            .field("uuid", &self.uuid)
            .field("session_id", &self.session_id)
            .field("kind", &self.kind)
            .field("text", &self.text.as_ref().map(|_| "<redacted>"))
            .field("id", &self.id)
            .field("name", &self.name)
            .field("model", &self.model)
            .field("is_error", &self.is_error)
            .field("raw_input", &self.raw_input.as_ref().map(|_| "<redacted>"))
            .field(
                "raw_output",
                &self.raw_output.as_ref().map(|_| "<redacted>"),
            )
            .field("redacted", &self.redacted)
            .finish()
    }
}

impl TranscriptEvent {
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn is_local_command_output(&self) -> bool {
        self.kind == TranscriptEventKind::System
            && self.text.as_deref().is_some_and(has_local_command_output)
    }
}

pub fn parse_transcript_line(line: &str) -> anyhow::Result<Vec<TranscriptEvent>> {
    if line.trim().is_empty() {
        return Ok(Vec::new());
    }

    let value: Value = serde_json::from_str(line)?;
    Ok(parse_transcript_record(&value))
}

pub fn parse_transcript_record(value: &Value) -> Vec<TranscriptEvent> {
    let session_id = extract_session_id(value);
    let record_type = string_at(value, &["type"]).unwrap_or("unknown").to_string();

    if record_type == "tool_use" {
        return vec![tool_use_event(value, session_id, 0)];
    }
    if record_type == "tool_result" {
        return vec![tool_result_event(value, session_id, 0)];
    }

    let message = value.get("message").unwrap_or(value);
    let role = string_at(message, &["role"])
        .or_else(|| string_at(value, &["role"]))
        .unwrap_or(record_type.as_str());
    if role == "user" && value.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return Vec::new();
    }
    let model = string_at(message, &["model"]).map(str::to_string);
    let content = message.get("content").or_else(|| value.get("content"));

    match content {
        Some(Value::Array(items)) => {
            let mut events = Vec::new();
            for (index, item) in items.iter().enumerate() {
                events.extend(parse_content_item(
                    item,
                    role,
                    &record_type,
                    session_id.clone(),
                    model.clone(),
                    index,
                ));
            }
            events
        }
        Some(content) => text_events(
            role,
            &record_type,
            session_id,
            model,
            text_from_value(content),
            0,
        ),
        None => vec![diagnostic_event(
            session_id,
            record_type,
            text_from_value(value),
            0,
        )],
    }
}

fn parse_content_item(
    item: &Value,
    role: &str,
    record_type: &str,
    session_id: Option<String>,
    model: Option<String>,
    index: usize,
) -> Vec<TranscriptEvent> {
    match string_at(item, &["type"]) {
        Some("text") => text_events(
            role,
            record_type,
            session_id,
            model,
            text_from_value(item.get("text").unwrap_or(item)),
            index,
        ),
        Some("tool_use") => vec![tool_use_event(item, session_id, index)],
        Some("tool_result") => vec![tool_result_event(item, session_id, index)],
        Some(kind) => vec![diagnostic_event(
            session_id,
            kind.to_string(),
            text_from_value(item),
            index,
        )],
        None => text_events(
            role,
            record_type,
            session_id,
            model,
            text_from_value(item),
            index,
        ),
    }
}

fn text_events(
    role: &str,
    record_type: &str,
    session_id: Option<String>,
    model: Option<String>,
    text: String,
    index: usize,
) -> Vec<TranscriptEvent> {
    let text = if role == "user" {
        strip_local_command_metadata(&text)
    } else {
        text
    };
    if role == "user" && text.trim().is_empty() {
        return Vec::new();
    }
    let kind = match role {
        "user" => TranscriptEventKind::UserMessage,
        "assistant" => TranscriptEventKind::AssistantMessage,
        "system" => TranscriptEventKind::System,
        _ => TranscriptEventKind::Diagnostic,
    };
    let uuid = event_uuid(session_id.as_deref(), record_type, index);
    vec![TranscriptEvent {
        uuid,
        session_id,
        kind,
        redacted: summarize_text(&text),
        text: Some(text),
        id: None,
        name: None,
        model,
        is_error: false,
        raw_input: None,
        raw_output: None,
    }]
}

fn tool_use_event(value: &Value, session_id: Option<String>, index: usize) -> TranscriptEvent {
    let id = string_at(value, &["id"]).map(str::to_string);
    TranscriptEvent {
        uuid: id
            .clone()
            .unwrap_or_else(|| event_uuid(session_id.as_deref(), "tool_use", index)),
        session_id,
        kind: TranscriptEventKind::ToolUse,
        text: string_at(value, &["name"]).map(str::to_string),
        id,
        name: string_at(value, &["name"]).map(str::to_string),
        model: None,
        is_error: false,
        raw_input: value.get("input").cloned(),
        raw_output: None,
        redacted: summarize_value(value.get("input").unwrap_or(&Value::Null)),
    }
}

fn tool_result_event(value: &Value, session_id: Option<String>, index: usize) -> TranscriptEvent {
    let raw_output = value.get("content").cloned();
    let text = text_from_value(raw_output.as_ref().unwrap_or(value));
    let id = string_at(value, &["tool_use_id"]).map(str::to_string);
    TranscriptEvent {
        uuid: id
            .clone()
            .unwrap_or_else(|| event_uuid(session_id.as_deref(), "tool_result", index)),
        session_id,
        kind: TranscriptEventKind::ToolResult,
        redacted: summarize_text(&text),
        text: Some(text),
        id,
        name: None,
        model: None,
        is_error: value
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        raw_input: None,
        raw_output,
    }
}

fn diagnostic_event(
    session_id: Option<String>,
    record_type: String,
    text: String,
    index: usize,
) -> TranscriptEvent {
    TranscriptEvent {
        uuid: event_uuid(session_id.as_deref(), &record_type, index),
        session_id,
        kind: TranscriptEventKind::Diagnostic,
        redacted: summarize_text(&text),
        text: Some(text),
        id: None,
        name: Some(record_type),
        model: None,
        is_error: false,
        raw_input: None,
        raw_output: None,
    }
}

fn extract_session_id(value: &Value) -> Option<String> {
    string_at(value, &["sessionId"])
        .or_else(|| string_at(value, &["session_id"]))
        .or_else(|| string_at(value, &["sessionID"]))
        .map(str::to_string)
}

fn string_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for part in path {
        current = current.get(part)?;
    }
    current.as_str()
}

fn text_from_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .or_else(|| item.get("text").and_then(Value::as_str))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => value.to_string(),
    }
}

pub fn strip_local_command_metadata(text: &str) -> String {
    let mut stripped = text.to_string();
    for tag in [
        "command-name",
        "command-message",
        "command-args",
        "local-command-caveat",
        "local-command-stdout",
        "local-command-stderr",
    ] {
        let start_tag = format!("<{tag}>");
        let end_tag = format!("</{tag}>");
        while let Some(start) = stripped.find(&start_tag) {
            let body_start = start + start_tag.len();
            let Some(relative_end) = stripped[body_start..].find(&end_tag) else {
                break;
            };
            let end = body_start + relative_end + end_tag.len();
            stripped.replace_range(start..end, "");
        }
    }
    stripped.trim().to_string()
}

pub fn local_command_output(text: &str) -> Option<String> {
    let output = local_command_segments(text)
        .into_iter()
        .filter_map(|segment| {
            let segment = segment.trim();
            (!segment.is_empty()).then(|| segment.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!output.is_empty()).then_some(output)
}

fn has_local_command_output(text: &str) -> bool {
    !local_command_segments(text).is_empty()
}

fn local_command_segments(text: &str) -> Vec<&str> {
    const TAGS: [(&str, &str); 2] = [
        ("<local-command-stdout>", "</local-command-stdout>"),
        ("<local-command-stderr>", "</local-command-stderr>"),
    ];

    let mut segments = Vec::new();
    let mut rest = text;
    while let Some((start, start_tag, end_tag)) = TAGS
        .iter()
        .filter_map(|(start_tag, end_tag)| {
            rest.find(start_tag)
                .map(|start| (start, *start_tag, *end_tag))
        })
        .min_by_key(|(start, _, _)| *start)
    {
        let body_start = start + start_tag.len();
        let Some(relative_end) = rest[body_start..].find(end_tag) else {
            break;
        };
        let body_end = body_start + relative_end;
        segments.push(&rest[body_start..body_end]);
        rest = &rest[body_end + end_tag.len()..];
    }
    segments
}

fn summarize_text(text: &str) -> RedactionSummary {
    RedactionSummary {
        text_redacted: true,
        char_count: text.chars().count(),
        line_count: text.lines().count().max(usize::from(!text.is_empty())),
        value_kind: None,
    }
}

fn summarize_value(value: &Value) -> RedactionSummary {
    RedactionSummary {
        text_redacted: true,
        char_count: 0,
        line_count: 0,
        value_kind: Some(
            match value {
                Value::Null => "null",
                Value::Bool(_) => "bool",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
            }
            .to_string(),
        ),
    }
}

fn event_uuid(session_id: Option<&str>, record_type: &str, index: usize) -> String {
    format!(
        "{}:{record_type}:{index}",
        session_id.unwrap_or("transcript")
    )
}
