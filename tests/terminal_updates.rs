use acp_extension_claude_pty::{
    acp::updates::TranscriptUpdateMapper, transcript::events::parse_transcript_line,
};
use agent_client_protocol::schema::{
    SessionUpdate, Terminal, ToolCallContent, ToolCallStatus, ToolKind,
};
use serde_json::{Value, json};

#[test]
fn bash_terminal_support_uses_terminal_content_and_meta_updates() {
    let mut mapper = TranscriptUpdateMapper::new(None, true);
    let use_line = json!({
        "type": "assistant",
        "sessionId": "session-a",
        "message": {"role": "assistant", "content": [{
            "type": "tool_use",
            "id": "toolu_bash",
            "name": "Bash",
            "input": {"command": "ls -la"}
        }]}
    })
    .to_string();
    let use_updates =
        mapper.updates_for_event(&parse_transcript_line(&use_line).expect("parse").remove(0));
    let SessionUpdate::ToolCall(call) = &use_updates[0] else {
        panic!("expected tool call");
    };
    assert_eq!(call.kind, ToolKind::Execute);
    assert!(matches!(
        call.content.as_slice(),
        [ToolCallContent::Terminal(Terminal { terminal_id, .. })]
            if terminal_id.0.as_ref() == "toolu_bash"
    ));
    assert_eq!(
        meta_value(call.meta.as_ref(), &["terminal_info", "terminal_id"]),
        Some(&json!("toolu_bash"))
    );

    let result_line = json!({
        "type": "user",
        "sessionId": "session-a",
        "message": {"role": "user", "content": [{
            "type": "tool_result",
            "tool_use_id": "toolu_bash",
            "content": {
                "type": "bash_code_execution_result",
                "stdout": "file1.txt\nfile2.txt",
                "stderr": "",
                "return_code": 0
            },
            "is_error": false
        }]}
    })
    .to_string();
    let result_updates = mapper.updates_for_event(
        &parse_transcript_line(&result_line)
            .expect("parse")
            .remove(0),
    );
    assert_eq!(result_updates.len(), 2);

    let SessionUpdate::ToolCallUpdate(output) = &result_updates[0] else {
        panic!("expected terminal output update");
    };
    assert!(output.fields.status.is_none());
    assert_eq!(
        meta_value(output.meta.as_ref(), &["terminal_output", "data"]),
        Some(&json!("file1.txt\nfile2.txt"))
    );

    let SessionUpdate::ToolCallUpdate(exit) = &result_updates[1] else {
        panic!("expected terminal exit update");
    };
    assert_eq!(exit.fields.status, Some(ToolCallStatus::Completed));
    assert_eq!(
        meta_value(exit.meta.as_ref(), &["terminal_exit", "exit_code"]),
        Some(&json!(0))
    );
    assert!(matches!(
        exit.fields.content.as_ref().expect("content").as_slice(),
        [ToolCallContent::Terminal(Terminal { terminal_id, .. })]
            if terminal_id.0.as_ref() == "toolu_bash"
    ));
}

fn meta_value<'a>(
    meta: Option<&'a serde_json::Map<String, Value>>,
    path: &[&str],
) -> Option<&'a Value> {
    let mut value = meta?.get(path[0])?;
    for part in &path[1..] {
        value = value.get(*part)?;
    }
    Some(value)
}
