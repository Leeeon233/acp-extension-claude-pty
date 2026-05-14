use agent_client_protocol::schema::{
    ContentBlock, SessionUpdate, TextContent, ToolCall, ToolCallContent, ToolCallStatus,
    ToolCallUpdate, ToolKind,
};
use claude_code_cli_acp::{
    acp::updates::TranscriptUpdateMapper, transcript::events::parse_transcript_line,
};
use serde_json::json;

fn first_tool_call(line: String, cwd: &std::path::Path) -> ToolCall {
    let mut mapper = TranscriptUpdateMapper::new(Some(cwd.to_path_buf()), false);
    let event = parse_transcript_line(&line)
        .expect("parse")
        .into_iter()
        .next()
        .expect("event");
    match mapper
        .updates_for_event(&event)
        .into_iter()
        .next()
        .expect("update")
    {
        SessionUpdate::ToolCall(tool) => tool,
        update => panic!("expected tool call, got {update:?}"),
    }
}

fn tool_use_line(name: &str, input: serde_json::Value) -> String {
    json!({
        "type": "assistant",
        "sessionId": "session-a",
        "message": {
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": format!("toolu_{name}"),
                "name": name,
                "input": input
            }]
        }
    })
    .to_string()
}

#[test]
fn tool_use_titles_kinds_locations_and_raw_input_follow_claude_reference() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cwd = temp.path();
    let source = cwd.join("src/main.rs");

    let read = first_tool_call(
        tool_use_line(
            "Read",
            json!({"file_path": source, "offset": 5, "limit": 3}),
        ),
        cwd,
    );
    assert_eq!(read.title, "Read src/main.rs (5 - 7)");
    assert_eq!(read.kind, ToolKind::Read);
    assert_eq!(read.status, ToolCallStatus::Pending);
    assert_eq!(read.locations[0].line, Some(5));
    assert_eq!(
        read.raw_input
            .as_ref()
            .and_then(|value| value.get("offset"))
            .and_then(serde_json::Value::as_i64),
        Some(5)
    );

    let cases = [
        (
            "Bash",
            json!({"command": "cargo test", "description": "Run tests"}),
            "cargo test",
            ToolKind::Execute,
        ),
        (
            "Task",
            json!({"description": "Inspect repo", "prompt": "Find files"}),
            "Inspect repo",
            ToolKind::Think,
        ),
        (
            "Glob",
            json!({"path": "src", "pattern": "*.rs"}),
            "Find `src` `*.rs`",
            ToolKind::Search,
        ),
        (
            "Grep",
            json!({"pattern": "fn main", "path": "src", "-n": true, "output_mode": "count"}),
            "grep -n -c \"fn main\" src",
            ToolKind::Search,
        ),
        (
            "WebFetch",
            json!({"url": "https://example.com", "prompt": "Summarize"}),
            "Fetch https://example.com",
            ToolKind::Fetch,
        ),
        (
            "WebSearch",
            json!({"query": "agent client protocol", "allowed_domains": ["agentclientprotocol.com"]}),
            "\"agent client protocol\" (allowed: agentclientprotocol.com)",
            ToolKind::Fetch,
        ),
        (
            "ExitPlanMode",
            json!({"plan": "Implementation plan"}),
            "Ready to code?",
            ToolKind::SwitchMode,
        ),
    ];

    for (name, input, title, kind) in cases {
        let call = first_tool_call(tool_use_line(name, input), cwd);
        assert_eq!(call.title, title, "{name}");
        assert_eq!(call.kind, kind, "{name}");
    }
}

#[test]
fn tool_result_updates_status_raw_output_and_formatted_content() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut mapper = TranscriptUpdateMapper::new(Some(temp.path().to_path_buf()), false);

    let use_event =
        parse_transcript_line(&tool_use_line("Bash", json!({"command": "printf hello"})))
            .expect("parse use")
            .remove(0);
    assert!(matches!(
        mapper.updates_for_event(&use_event).as_slice(),
        [SessionUpdate::ToolCall(_)]
    ));

    let result_line = json!({
        "type": "user",
        "sessionId": "session-a",
        "message": {
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_Bash",
                "is_error": false,
                "content": "hello\n"
            }]
        }
    })
    .to_string();
    let result_event = parse_transcript_line(&result_line)
        .expect("parse result")
        .remove(0);
    let updates = mapper.updates_for_event(&result_event);
    assert_eq!(updates.len(), 1);

    let update = match &updates[0] {
        SessionUpdate::ToolCallUpdate(update) => update,
        update => panic!("expected tool update, got {update:?}"),
    };
    assert_eq!(update.fields.status, Some(ToolCallStatus::Completed));
    assert_eq!(update.fields.raw_output.as_ref(), Some(&json!("hello\n")));
    assert_content_text(update, "```console\nhello\n```");
}

#[test]
fn tool_result_errors_are_failed_and_not_terminal_formatted() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut mapper = TranscriptUpdateMapper::new(Some(temp.path().to_path_buf()), false);
    let use_event = parse_transcript_line(&tool_use_line("Bash", json!({"command": "bad"})))
        .expect("parse use")
        .remove(0);
    mapper.updates_for_event(&use_event);

    let result_line = json!({
        "type": "user",
        "sessionId": "session-a",
        "message": {"role": "user", "content": [{
            "type": "tool_result",
            "tool_use_id": "toolu_Bash",
            "is_error": true,
            "content": "command not found"
        }]}
    })
    .to_string();
    let result_event = parse_transcript_line(&result_line)
        .expect("parse result")
        .remove(0);
    let updates = mapper.updates_for_event(&result_event);
    let update = match &updates[0] {
        SessionUpdate::ToolCallUpdate(update) => update,
        update => panic!("expected tool update, got {update:?}"),
    };
    assert_eq!(update.fields.status, Some(ToolCallStatus::Failed));
    assert_content_text(update, "```\ncommand not found\n```");
}

fn assert_content_text(update: &ToolCallUpdate, expected: &str) {
    let content = update
        .fields
        .content
        .as_ref()
        .expect("content")
        .first()
        .expect("first content");
    let ToolCallContent::Content(content) = content else {
        panic!("expected text content, got {content:?}");
    };
    let ContentBlock::Text(TextContent { text, .. }) = &content.content else {
        panic!("expected text block, got {:?}", content.content);
    };
    assert_eq!(text, expected);
}
