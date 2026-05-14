use agent_client_protocol::schema::{Diff, SessionUpdate, ToolCallContent, ToolKind};
use claude_code_cli_acp::{
    acp::updates::TranscriptUpdateMapper, transcript::events::parse_transcript_line,
};
use serde_json::json;

#[test]
fn write_tool_use_emits_creation_diff_from_structured_input() {
    let mut mapper = TranscriptUpdateMapper::new(None, false);
    let line = json!({
        "type": "assistant",
        "sessionId": "session-a",
        "message": {"role": "assistant", "content": [{
            "type": "tool_use",
            "id": "toolu_write",
            "name": "Write",
            "input": {"file_path": "/tmp/project/new.txt", "content": "new body"}
        }]}
    })
    .to_string();
    let updates = mapper.updates_for_event(&parse_transcript_line(&line).expect("parse").remove(0));
    let SessionUpdate::ToolCall(call) = &updates[0] else {
        panic!("expected tool call");
    };
    assert_eq!(call.title, "Write /tmp/project/new.txt");
    assert_eq!(call.kind, ToolKind::Edit);
    assert_diff(&call.content[0], "/tmp/project/new.txt", None, "new body");
}

#[test]
fn edit_tool_use_emits_replacement_diff_from_structured_input() {
    let mut mapper = TranscriptUpdateMapper::new(None, false);
    let line = json!({
        "type": "assistant",
        "sessionId": "session-a",
        "message": {"role": "assistant", "content": [{
            "type": "tool_use",
            "id": "toolu_edit",
            "name": "Edit",
            "input": {
                "file_path": "/tmp/project/file.txt",
                "old_string": "old",
                "new_string": "new"
            }
        }]}
    })
    .to_string();
    let updates = mapper.updates_for_event(&parse_transcript_line(&line).expect("parse").remove(0));
    let SessionUpdate::ToolCall(call) = &updates[0] else {
        panic!("expected tool call");
    };
    assert_eq!(call.title, "Edit /tmp/project/file.txt");
    assert_eq!(call.kind, ToolKind::Edit);
    assert_diff(
        &call.content[0],
        "/tmp/project/file.txt",
        Some("old"),
        "new",
    );
}

#[test]
fn edit_result_without_structured_patch_does_not_fabricate_diff() {
    let mut mapper = TranscriptUpdateMapper::new(None, false);
    let use_line = json!({
        "type": "assistant",
        "sessionId": "session-a",
        "message": {"role": "assistant", "content": [{
            "type": "tool_use",
            "id": "toolu_edit",
            "name": "Edit",
            "input": {"file_path": "/tmp/project/file.txt", "old_string": "old", "new_string": "new"}
        }]}
    })
    .to_string();
    mapper.updates_for_event(&parse_transcript_line(&use_line).expect("parse").remove(0));

    let result_line = json!({
        "type": "user",
        "sessionId": "session-a",
        "message": {"role": "user", "content": [{
            "type": "tool_result",
            "tool_use_id": "toolu_edit",
            "content": "The file was updated.",
            "is_error": false
        }]}
    })
    .to_string();
    let updates = mapper.updates_for_event(
        &parse_transcript_line(&result_line)
            .expect("parse")
            .remove(0),
    );
    let SessionUpdate::ToolCallUpdate(update) = &updates[0] else {
        panic!("expected tool update");
    };
    assert!(update.fields.content.as_ref().is_none_or(Vec::is_empty));
}

fn assert_diff(content: &ToolCallContent, path: &str, old_text: Option<&str>, new_text: &str) {
    let ToolCallContent::Diff(Diff {
        path: actual_path,
        old_text: actual_old_text,
        new_text: actual_new_text,
        ..
    }) = content
    else {
        panic!("expected diff, got {content:?}");
    };
    assert_eq!(actual_path, std::path::Path::new(path));
    assert_eq!(actual_old_text.as_deref(), old_text);
    assert_eq!(actual_new_text, new_text);
}
