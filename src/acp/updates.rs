use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use agent_client_protocol::schema::{
    AvailableCommandsUpdate, ClientCapabilities, ContentBlock, ContentChunk, Diff,
    EmbeddedResourceResource, ImageContent, PermissionOption, PermissionOptionKind, Plan,
    PlanEntry, PlanEntryPriority, PlanEntryStatus, PromptRequest, RequestPermissionOutcome,
    RequestPermissionRequest, SessionId, SessionUpdate, Terminal, TextContent, ToolCall,
    ToolCallContent, ToolCallId, ToolCallLocation, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind,
};
use serde_json::{Value, json};

use crate::{
    config::commands,
    session::manager::PendingPermission,
    terminal::recognizers::{PermissionDecision, PermissionDialog},
    transcript::events::{TranscriptEvent, TranscriptEventKind},
};

pub const ALLOW_ONCE_OPTION_ID: &str = "allow_once";
pub const ALLOW_ALWAYS_OPTION_ID: &str = "allow_always";
pub const REJECT_OPTION_ID: &str = "reject";

pub fn prompt_text(request: &PromptRequest) -> String {
    request
        .prompt
        .iter()
        .filter_map(content_block_text)
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn content_block_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(text) => Some(format_text_prompt(&text.text)),
        ContentBlock::Image(image) => Some(format!(
            "[image attachment: data:{};base64,{}]",
            image.mime_type, image.data
        )),
        ContentBlock::ResourceLink(link) => Some(format_resource_link(&link.name, &link.uri)),
        ContentBlock::Resource(resource) => match &resource.resource {
            EmbeddedResourceResource::TextResourceContents(text) => Some(format!(
                "{}\n\n<context ref=\"{}\">\n{}\n</context>",
                format_resource_link("", &text.uri),
                text.uri,
                text.text
            )),
            EmbeddedResourceResource::BlobResourceContents(blob) => Some(format!(
                "[resource attachment: {};base64,{}]",
                blob.mime_type
                    .as_deref()
                    .unwrap_or("application/octet-stream"),
                blob.blob
            )),
            _ => None,
        },
        ContentBlock::Audio(_) => None,
        _ => None,
    }
}

fn format_resource_link(name: &str, uri: &str) -> String {
    let display_name = if name.is_empty() {
        uri.rsplit('/')
            .next()
            .filter(|part| !part.is_empty())
            .unwrap_or(uri)
    } else {
        name
    };
    if display_name.is_empty() {
        uri.to_string()
    } else {
        format!("[@{display_name}]({uri})")
    }
}

fn format_text_prompt(text: &str) -> String {
    let Some(rest) = text.strip_prefix("/mcp:") else {
        return text.to_string();
    };
    let Some((server, rest)) = rest.split_once(':') else {
        return text.to_string();
    };
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(command, args)| (command, args.trim_start()));
    if server.is_empty() || command.is_empty() {
        return text.to_string();
    }
    if args.is_empty() {
        format!("/{server}:{command} (MCP)")
    } else {
        format!("/{server}:{command} (MCP) {args}")
    }
}

pub fn user_message_chunk(text: impl Into<String>) -> SessionUpdate {
    SessionUpdate::UserMessageChunk(ContentChunk::new(text.into().into()))
}

pub fn agent_message_chunk(text: impl Into<String>) -> SessionUpdate {
    SessionUpdate::AgentMessageChunk(ContentChunk::new(text.into().into()))
}

pub fn agent_thought_chunk(text: impl Into<String>) -> SessionUpdate {
    SessionUpdate::AgentThoughtChunk(ContentChunk::new(text.into().into()))
}

pub fn transcript_event_update(event: &TranscriptEvent) -> Option<SessionUpdate> {
    TranscriptUpdateMapper::new(None, false)
        .updates_for_event(event)
        .into_iter()
        .next()
}

#[derive(Debug, Clone)]
struct ToolUseSnapshot {
    name: String,
}

#[derive(Debug, Clone, Default)]
pub struct TranscriptUpdateMapper {
    cwd: Option<PathBuf>,
    supports_terminal_output: bool,
    tool_uses: HashMap<String, ToolUseSnapshot>,
}

impl TranscriptUpdateMapper {
    pub fn new(cwd: Option<PathBuf>, supports_terminal_output: bool) -> Self {
        Self {
            cwd,
            supports_terminal_output,
            tool_uses: HashMap::new(),
        }
    }

    pub fn from_client(cwd: &Path, client_capabilities: &ClientCapabilities) -> Self {
        Self::new(
            Some(cwd.to_path_buf()),
            client_supports_terminal_output(client_capabilities),
        )
    }

    pub fn updates_for_event(&mut self, event: &TranscriptEvent) -> Vec<SessionUpdate> {
        match event.kind {
            TranscriptEventKind::AssistantMessage => event
                .text
                .clone()
                .map(agent_message_chunk)
                .into_iter()
                .collect(),
            TranscriptEventKind::AssistantThought => event
                .text
                .clone()
                .map(agent_thought_chunk)
                .into_iter()
                .collect(),
            TranscriptEventKind::UserMessage => event
                .text
                .clone()
                .map(user_message_chunk)
                .into_iter()
                .collect(),
            TranscriptEventKind::ToolUse => self.tool_use_updates(event),
            TranscriptEventKind::ToolResult => self.tool_result_updates(event),
            TranscriptEventKind::System | TranscriptEventKind::Diagnostic => Vec::new(),
        }
    }

    fn tool_use_updates(&mut self, event: &TranscriptEvent) -> Vec<SessionUpdate> {
        let id = tool_event_id(event);
        let name = event
            .name
            .as_deref()
            .or(event.text.as_deref())
            .unwrap_or("Unknown Tool")
            .to_string();
        let input = event.raw_input.clone();
        self.tool_uses
            .insert(id.clone(), ToolUseSnapshot { name: name.clone() });

        if name == "TodoWrite" {
            return todo_plan_update(input.as_ref()).into_iter().collect();
        }

        let mut info = tool_info(
            &name,
            input.as_ref(),
            self.supports_terminal_output,
            self.cwd.as_deref(),
        );
        let mut meta = claude_tool_meta(&name);
        if name == "Bash" && self.supports_terminal_output {
            meta.insert(
                "terminal_info".to_string(),
                json!({ "terminal_id": id.clone() }),
            );
            info.content = vec![ToolCallContent::Terminal(Terminal::new(id.clone()))];
        }
        let mut tool = ToolCall::new(ToolCallId::new(id.clone()), info.title)
            .kind(info.kind)
            .status(ToolCallStatus::Pending)
            .content(info.content)
            .locations(info.locations)
            .meta(meta);
        if let Some(input) = input {
            tool = tool.raw_input(input);
        }
        vec![SessionUpdate::ToolCall(tool)]
    }

    fn tool_result_updates(&mut self, event: &TranscriptEvent) -> Vec<SessionUpdate> {
        let id = tool_event_id(event);
        let tool = self.tool_uses.get(&id).cloned();
        if tool.as_ref().is_some_and(|tool| tool.name == "TodoWrite") {
            return Vec::new();
        }
        let name = tool
            .as_ref()
            .map(|tool| tool.name.as_str())
            .unwrap_or("Unknown Tool");
        let raw_output = event.raw_output.clone();
        let status = if event.is_error {
            ToolCallStatus::Failed
        } else {
            ToolCallStatus::Completed
        };

        if event.is_error {
            return vec![SessionUpdate::ToolCallUpdate(tool_result_update(
                ToolResultUpdateParts {
                    id,
                    name,
                    status,
                    raw_output,
                    content: text_content_vec(error_text(
                        event.text.as_deref().unwrap_or_default(),
                    )),
                    locations: None,
                    title: None,
                    extra_meta: None,
                },
            ))];
        }

        if name == "Bash" && self.supports_terminal_output {
            let output = bash_output(raw_output.as_ref(), event.text.as_deref());
            let exit_code = bash_exit_code(raw_output.as_ref(), false);
            let output_update =
                ToolCallUpdate::new(ToolCallId::new(id.clone()), ToolCallUpdateFields::new()).meta(
                    meta_from_pairs([(
                        "terminal_output",
                        json!({ "terminal_id": id.clone(), "data": output }),
                    )]),
                );
            let terminal_content = vec![ToolCallContent::Terminal(Terminal::new(id.clone()))];
            let exit_update = tool_result_update(ToolResultUpdateParts {
                id: id.clone(),
                name,
                status,
                raw_output,
                content: terminal_content,
                locations: None,
                title: None,
                extra_meta: Some(meta_from_pairs([(
                    "terminal_exit",
                    json!({ "terminal_id": id, "exit_code": exit_code, "signal": Value::Null }),
                )])),
            });
            return vec![
                SessionUpdate::ToolCallUpdate(output_update),
                SessionUpdate::ToolCallUpdate(exit_update),
            ];
        }

        let (title, content, locations) = match name {
            "Bash" => {
                let output = bash_output(raw_output.as_ref(), event.text.as_deref());
                if output.trim().is_empty() {
                    (None, Vec::new(), None)
                } else {
                    (
                        None,
                        text_content_vec(format!("```console\n{}\n```", output.trim_end())),
                        None,
                    )
                }
            }
            "Read" => (
                None,
                raw_output
                    .as_ref()
                    .map(|output| acp_content_update(output, false, true))
                    .unwrap_or_default(),
                None,
            ),
            "Edit" | "Write" => (None, Vec::new(), None),
            "ExitPlanMode" => (Some("Exited Plan Mode".to_string()), Vec::new(), None),
            _ => (
                None,
                raw_output
                    .as_ref()
                    .map(|output| acp_content_update(output, false, false))
                    .unwrap_or_else(|| {
                        event.text.clone().map(text_content_vec).unwrap_or_default()
                    }),
                None,
            ),
        };

        vec![SessionUpdate::ToolCallUpdate(tool_result_update(
            ToolResultUpdateParts {
                id,
                name,
                status,
                raw_output,
                content,
                locations,
                title,
                extra_meta: None,
            },
        ))]
    }
}

pub fn client_supports_terminal_output(capabilities: &ClientCapabilities) -> bool {
    capabilities.terminal
        || capabilities
            .meta
            .as_ref()
            .and_then(|meta| meta.get("terminal_output"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

struct ToolInfo {
    title: String,
    kind: ToolKind,
    content: Vec<ToolCallContent>,
    locations: Vec<ToolCallLocation>,
}

fn tool_info(
    name: &str,
    input: Option<&Value>,
    supports_terminal_output: bool,
    cwd: Option<&Path>,
) -> ToolInfo {
    match name {
        "Agent" | "Task" => ToolInfo {
            title: input_string(input, "description").unwrap_or_else(|| "Task".to_string()),
            kind: ToolKind::Think,
            content: input_string(input, "prompt")
                .map(text_content_vec)
                .unwrap_or_default(),
            locations: Vec::new(),
        },
        "Bash" => ToolInfo {
            title: input_string(input, "command").unwrap_or_else(|| "Terminal".to_string()),
            kind: ToolKind::Execute,
            content: if supports_terminal_output {
                Vec::new()
            } else {
                input_string(input, "description")
                    .map(text_content_vec)
                    .unwrap_or_default()
            },
            locations: Vec::new(),
        },
        "Read" => {
            let file_path = input_string(input, "file_path");
            let offset = input_i64(input, "offset").unwrap_or(1);
            let limit = input_i64(input, "limit");
            let suffix = match limit {
                Some(limit) if limit > 0 => {
                    format!(" ({offset} - {})", offset + limit - 1)
                }
                _ if input_i64(input, "offset").is_some() => format!(" (from line {offset})"),
                _ => String::new(),
            };
            let display = file_path
                .as_deref()
                .map(|path| display_path(path, cwd))
                .unwrap_or_else(|| "File".to_string());
            ToolInfo {
                title: format!("Read {display}{suffix}"),
                kind: ToolKind::Read,
                content: Vec::new(),
                locations: file_path
                    .map(|path| vec![ToolCallLocation::new(path).line(offset.max(1) as u32)])
                    .unwrap_or_default(),
            }
        }
        "Write" => {
            let file_path = input_string(input, "file_path");
            let content_text = input_string(input, "content");
            let display = file_path.as_deref().map(|path| display_path(path, cwd));
            let content = match (file_path.as_deref(), content_text.as_deref()) {
                (Some(path), Some(text)) => {
                    vec![ToolCallContent::Diff(Diff::new(path, text.to_string()))]
                }
                (None, Some(text)) => text_content_vec(text.to_string()),
                _ => Vec::new(),
            };
            ToolInfo {
                title: display
                    .map(|path| format!("Write {path}"))
                    .unwrap_or_else(|| "Write".to_string()),
                kind: ToolKind::Edit,
                content,
                locations: file_path
                    .map(|path| vec![ToolCallLocation::new(path)])
                    .unwrap_or_default(),
            }
        }
        "Edit" => {
            let file_path = input_string(input, "file_path");
            let old_text = input_string(input, "old_string");
            let new_text = input_string(input, "new_string");
            let display = file_path.as_deref().map(|path| display_path(path, cwd));
            let content =
                if let (Some(path), Some(new_text)) = (file_path.as_deref(), new_text.as_deref()) {
                    vec![ToolCallContent::Diff(
                        Diff::new(path, new_text.to_string()).old_text(old_text),
                    )]
                } else {
                    Vec::new()
                };
            ToolInfo {
                title: display
                    .map(|path| format!("Edit {path}"))
                    .unwrap_or_else(|| "Edit".to_string()),
                kind: ToolKind::Edit,
                content,
                locations: file_path
                    .map(|path| vec![ToolCallLocation::new(path)])
                    .unwrap_or_default(),
            }
        }
        "Glob" => {
            let mut title = "Find".to_string();
            if let Some(path) = input_string(input, "path") {
                title.push_str(&format!(" `{path}`"));
            }
            if let Some(pattern) = input_string(input, "pattern") {
                title.push_str(&format!(" `{pattern}`"));
            }
            ToolInfo {
                title,
                kind: ToolKind::Search,
                content: Vec::new(),
                locations: input_string(input, "path")
                    .map(|path| vec![ToolCallLocation::new(path)])
                    .unwrap_or_default(),
            }
        }
        "Grep" => ToolInfo {
            title: grep_title(input),
            kind: ToolKind::Search,
            content: Vec::new(),
            locations: Vec::new(),
        },
        "WebFetch" => ToolInfo {
            title: input_string(input, "url")
                .map(|url| format!("Fetch {url}"))
                .unwrap_or_else(|| "Fetch".to_string()),
            kind: ToolKind::Fetch,
            content: input_string(input, "prompt")
                .map(text_content_vec)
                .unwrap_or_default(),
            locations: Vec::new(),
        },
        "WebSearch" => ToolInfo {
            title: web_search_title(input),
            kind: ToolKind::Fetch,
            content: Vec::new(),
            locations: Vec::new(),
        },
        "ExitPlanMode" => ToolInfo {
            title: "Ready to code?".to_string(),
            kind: ToolKind::SwitchMode,
            content: input_string(input, "plan")
                .map(text_content_vec)
                .unwrap_or_default(),
            locations: Vec::new(),
        },
        "Other" => ToolInfo {
            title: name.to_string(),
            kind: ToolKind::Other,
            content: input
                .map(|input| {
                    text_content_vec(format!(
                        "```json\n{}\n```",
                        serde_json::to_string_pretty(input).unwrap_or_else(|_| "{}".to_string())
                    ))
                })
                .unwrap_or_default(),
            locations: Vec::new(),
        },
        _ => ToolInfo {
            title: if name.is_empty() {
                "Unknown Tool".to_string()
            } else {
                name.to_string()
            },
            kind: ToolKind::Other,
            content: Vec::new(),
            locations: Vec::new(),
        },
    }
}

struct ToolResultUpdateParts<'a> {
    id: String,
    name: &'a str,
    status: ToolCallStatus,
    raw_output: Option<Value>,
    content: Vec<ToolCallContent>,
    locations: Option<Vec<ToolCallLocation>>,
    title: Option<String>,
    extra_meta: Option<serde_json::Map<String, Value>>,
}

fn tool_result_update(parts: ToolResultUpdateParts<'_>) -> ToolCallUpdate {
    let ToolResultUpdateParts {
        id,
        name,
        status,
        raw_output,
        content,
        locations,
        title,
        extra_meta,
    } = parts;
    let mut fields = ToolCallUpdateFields::new().status(status);
    if let Some(raw_output) = raw_output {
        fields = fields.raw_output(raw_output);
    }
    if !content.is_empty() {
        fields = fields.content(content);
    }
    if let Some(locations) = locations.filter(|locations| !locations.is_empty()) {
        fields = fields.locations(locations);
    }
    if let Some(title) = title {
        fields = fields.title(title);
    }
    let mut meta = claude_tool_meta(name);
    if let Some(extra_meta) = extra_meta {
        meta.extend(extra_meta);
    }
    ToolCallUpdate::new(ToolCallId::new(id), fields).meta(meta)
}

fn todo_plan_update(input: Option<&Value>) -> Option<SessionUpdate> {
    let todos = input?.get("todos")?.as_array()?;
    let entries = todos
        .iter()
        .map(|todo| {
            let content = todo
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let status = match todo.get("status").and_then(Value::as_str) {
                Some("completed") => PlanEntryStatus::Completed,
                Some("in_progress") => PlanEntryStatus::InProgress,
                _ => PlanEntryStatus::Pending,
            };
            PlanEntry::new(content, PlanEntryPriority::Medium, status)
        })
        .collect();
    Some(SessionUpdate::Plan(Plan::new(entries)))
}

fn acp_content_update(
    value: &Value,
    is_error: bool,
    markdown_escape_text: bool,
) -> Vec<ToolCallContent> {
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| acp_content_block(item, is_error, markdown_escape_text))
            .collect(),
        Value::Object(_) => acp_content_block(value, is_error, markdown_escape_text)
            .into_iter()
            .collect(),
        Value::String(text) if !text.is_empty() => {
            let text = if is_error {
                error_text(text)
            } else if markdown_escape_text {
                markdown_escape(text)
            } else {
                text.clone()
            };
            text_content_vec(text)
        }
        _ => Vec::new(),
    }
}

fn acp_content_block(
    value: &Value,
    is_error: bool,
    markdown_escape_text: bool,
) -> Option<ToolCallContent> {
    let kind = value.get("type").and_then(Value::as_str)?;
    match kind {
        "text" => {
            let text = value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let text = if is_error {
                error_text(text)
            } else if markdown_escape_text {
                markdown_escape(text)
            } else {
                text.to_string()
            };
            Some(ToolCallContent::Content(agent_text_content(text)))
        }
        "image" => {
            let source = value.get("source")?;
            if source.get("type").and_then(Value::as_str) == Some("base64") {
                Some(ToolCallContent::Content(
                    agent_client_protocol::schema::Content::new(ContentBlock::Image(
                        ImageContent::new(
                            source
                                .get("data")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                            source
                                .get("media_type")
                                .and_then(Value::as_str)
                                .unwrap_or("application/octet-stream"),
                        ),
                    )),
                ))
            } else {
                Some(ToolCallContent::Content(agent_text_content(
                    source
                        .get("url")
                        .and_then(Value::as_str)
                        .map(|url| format!("[image: {url}]"))
                        .unwrap_or_else(|| "[image: file reference]".to_string()),
                )))
            }
        }
        "bash_code_execution_result" => Some(ToolCallContent::Content(agent_text_content(
            format!("Output: {}", bash_output(Some(value), None)),
        ))),
        "web_search_result" => Some(ToolCallContent::Content(agent_text_content(format!(
            "{} ({})",
            value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Result"),
            value.get("url").and_then(Value::as_str).unwrap_or_default()
        )))),
        "web_fetch_result" => Some(ToolCallContent::Content(agent_text_content(format!(
            "Fetched: {}",
            value.get("url").and_then(Value::as_str).unwrap_or_default()
        )))),
        _ => Some(ToolCallContent::Content(agent_text_content(
            value.to_string(),
        ))),
    }
}

fn text_content_vec(text: impl Into<String>) -> Vec<ToolCallContent> {
    vec![ToolCallContent::Content(agent_text_content(text.into()))]
}

fn bash_output(raw_output: Option<&Value>, fallback_text: Option<&str>) -> String {
    match raw_output {
        Some(Value::Object(object))
            if object.get("type").and_then(Value::as_str) == Some("bash_code_execution_result") =>
        {
            [object.get("stdout"), object.get("stderr")]
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .or_else(|| item.get("text").and_then(Value::as_str))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(value) => value.to_string(),
        None => fallback_text.unwrap_or_default().to_string(),
    }
}

fn bash_exit_code(raw_output: Option<&Value>, is_error: bool) -> i64 {
    raw_output
        .and_then(|value| value.get("return_code"))
        .and_then(Value::as_i64)
        .unwrap_or(i64::from(is_error))
}

fn markdown_escape(text: &str) -> String {
    let mut fence = "```".to_string();
    for line in text.lines() {
        let tick_count = line.chars().take_while(|ch| *ch == '`').count();
        while tick_count >= fence.len() {
            fence.push('`');
        }
    }
    format!(
        "{fence}\n{}{}{fence}",
        text,
        if text.ends_with('\n') { "" } else { "\n" }
    )
}

fn error_text(text: &str) -> String {
    format!("```\n{text}\n```")
}

fn grep_title(input: Option<&Value>) -> String {
    let mut label = "grep".to_string();
    if input_bool(input, "-i") {
        label.push_str(" -i");
    }
    if input_bool(input, "-n") {
        label.push_str(" -n");
    }
    for flag in ["-A", "-B", "-C"] {
        if let Some(value) = input_i64(input, flag) {
            label.push_str(&format!(" {flag} {value}"));
        }
    }
    match input_string(input, "output_mode").as_deref() {
        Some("files_with_matches") => label.push_str(" -l"),
        Some("count") => label.push_str(" -c"),
        _ => {}
    }
    if let Some(limit) = input_i64(input, "head_limit") {
        label.push_str(&format!(" | head -{limit}"));
    }
    if let Some(glob) = input_string(input, "glob") {
        label.push_str(&format!(" --include=\"{glob}\""));
    }
    if let Some(file_type) = input_string(input, "type") {
        label.push_str(&format!(" --type={file_type}"));
    }
    if input_bool(input, "multiline") {
        label.push_str(" -P");
    }
    if let Some(pattern) = input_string(input, "pattern") {
        label.push_str(&format!(" \"{pattern}\""));
    }
    if let Some(path) = input_string(input, "path") {
        label.push_str(&format!(" {path}"));
    }
    label
}

fn web_search_title(input: Option<&Value>) -> String {
    let mut label = input_string(input, "query")
        .map(|query| format!("\"{query}\""))
        .unwrap_or_else(|| "Web search".to_string());
    if let Some(domains) = input_string_array(input, "allowed_domains")
        && !domains.is_empty()
    {
        label.push_str(&format!(" (allowed: {})", domains.join(", ")));
    }
    if let Some(domains) = input_string_array(input, "blocked_domains")
        && !domains.is_empty()
    {
        label.push_str(&format!(" (blocked: {})", domains.join(", ")));
    }
    label
}

fn input_string(input: Option<&Value>, key: &str) -> Option<String> {
    input?
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn input_i64(input: Option<&Value>, key: &str) -> Option<i64> {
    input?.get(key).and_then(Value::as_i64)
}

fn input_bool(input: Option<&Value>, key: &str) -> bool {
    input
        .and_then(|input| input.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn input_string_array(input: Option<&Value>, key: &str) -> Option<Vec<String>> {
    Some(
        input?
            .get(key)?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
    )
}

fn display_path(file_path: &str, cwd: Option<&Path>) -> String {
    let Some(cwd) = cwd else {
        return file_path.to_string();
    };
    let path = Path::new(file_path);
    path.strip_prefix(cwd)
        .ok()
        .and_then(|relative| {
            let text = relative.to_string_lossy().to_string();
            (!text.is_empty()).then_some(text)
        })
        .unwrap_or_else(|| file_path.to_string())
}

fn tool_event_id(event: &TranscriptEvent) -> String {
    event.id.clone().unwrap_or_else(|| event.uuid.clone())
}

fn claude_tool_meta(name: &str) -> serde_json::Map<String, Value> {
    meta_from_pairs([("claudeCode", json!({ "toolName": name }))])
}

fn meta_from_pairs<const N: usize>(pairs: [(&str, Value); N]) -> serde_json::Map<String, Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

fn agent_text_content(text: String) -> agent_client_protocol::schema::Content {
    agent_client_protocol::schema::Content::new(ContentBlock::Text(TextContent::new(text)))
}

pub fn pending_permission_update(id: impl Into<String>, title: impl Into<String>) -> SessionUpdate {
    SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        ToolCallId::new(id.into()),
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Pending)
            .title(title.into()),
    ))
}

pub fn permission_request(
    session_id: SessionId,
    permission: &PendingPermission,
) -> RequestPermissionRequest {
    let tool_call = ToolCallUpdate::new(
        permission_tool_call_id(&permission.dialog),
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Pending)
            .title(permission.dialog.title.clone()),
    );
    RequestPermissionRequest::new(
        session_id,
        tool_call,
        permission_options(&permission.dialog),
    )
}

pub fn permission_decision(outcome: &RequestPermissionOutcome) -> Option<PermissionDecision> {
    match outcome {
        RequestPermissionOutcome::Cancelled => Some(PermissionDecision::Reject),
        RequestPermissionOutcome::Selected(selected) => match selected.option_id.0.as_ref() {
            ALLOW_ONCE_OPTION_ID => Some(PermissionDecision::AllowOnce),
            ALLOW_ALWAYS_OPTION_ID => Some(PermissionDecision::AllowAlways),
            REJECT_OPTION_ID => Some(PermissionDecision::Reject),
            _ => None,
        },
        _ => None,
    }
}

fn permission_options(dialog: &PermissionDialog) -> Vec<PermissionOption> {
    let mut options = Vec::new();
    if dialog
        .options
        .iter()
        .any(|option| option.decision == PermissionDecision::AllowOnce)
    {
        options.push(PermissionOption::new(
            ALLOW_ONCE_OPTION_ID,
            "Allow once",
            PermissionOptionKind::AllowOnce,
        ));
    }
    if dialog
        .options
        .iter()
        .any(|option| option.decision == PermissionDecision::AllowAlways)
    {
        options.push(PermissionOption::new(
            ALLOW_ALWAYS_OPTION_ID,
            "Allow for session",
            PermissionOptionKind::AllowAlways,
        ));
    }
    if dialog
        .options
        .iter()
        .any(|option| option.decision == PermissionDecision::Reject)
    {
        options.push(PermissionOption::new(
            REJECT_OPTION_ID,
            "Reject",
            PermissionOptionKind::RejectOnce,
        ));
    }
    options
}

fn permission_tool_call_id(dialog: &PermissionDialog) -> ToolCallId {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in dialog.title.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    ToolCallId::new(format!("claude-permission-{hash:x}"))
}

pub fn available_commands(cwd: &Path) -> SessionUpdate {
    SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(
        commands::available_commands(cwd),
    ))
}
