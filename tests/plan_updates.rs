use acp_extension_claude_pty::{
    acp::updates::TranscriptUpdateMapper, transcript::events::parse_transcript_line,
};
use agent_client_protocol::schema::{PlanEntryPriority, PlanEntryStatus, SessionUpdate};
use serde_json::json;

#[test]
fn todo_write_tool_use_emits_acp_plan_instead_of_tool_call() {
    let mut mapper = TranscriptUpdateMapper::new(None, false);
    let line = json!({
        "type": "assistant",
        "sessionId": "session-a",
        "message": {"role": "assistant", "content": [{
            "type": "tool_use",
            "id": "toolu_todo",
            "name": "TodoWrite",
            "input": {
                "todos": [
                    {"content": "Research", "status": "completed", "activeForm": "Researching"},
                    {"content": "Implement", "status": "in_progress", "activeForm": "Implementing"},
                    {"content": "Verify", "status": "pending", "activeForm": "Verifying"}
                ]
            }
        }]}
    })
    .to_string();
    let event = parse_transcript_line(&line).expect("parse").remove(0);
    let updates = mapper.updates_for_event(&event);

    assert_eq!(updates.len(), 1);
    let SessionUpdate::Plan(plan) = &updates[0] else {
        panic!("expected plan update, got {:?}", updates[0]);
    };
    assert_eq!(plan.entries.len(), 3);
    assert_eq!(plan.entries[0].content, "Research");
    assert_eq!(plan.entries[0].priority, PlanEntryPriority::Medium);
    assert_eq!(plan.entries[0].status, PlanEntryStatus::Completed);
    assert_eq!(plan.entries[1].status, PlanEntryStatus::InProgress);
    assert_eq!(plan.entries[2].status, PlanEntryStatus::Pending);
}

#[test]
fn todo_write_result_is_not_reported_as_duplicate_tool_update() {
    let mut mapper = TranscriptUpdateMapper::new(None, false);
    let use_line = json!({
        "type": "assistant",
        "sessionId": "session-a",
        "message": {"role": "assistant", "content": [{
            "type": "tool_use",
            "id": "toolu_todo",
            "name": "TodoWrite",
            "input": {"todos": [{"content": "Test task", "status": "pending"}]}
        }]}
    })
    .to_string();
    mapper.updates_for_event(&parse_transcript_line(&use_line).expect("parse").remove(0));

    let result_line = json!({
        "type": "user",
        "sessionId": "session-a",
        "message": {"role": "user", "content": [{
            "type": "tool_result",
            "tool_use_id": "toolu_todo",
            "content": "Todos updated successfully",
            "is_error": false
        }]}
    })
    .to_string();
    let updates = mapper.updates_for_event(
        &parse_transcript_line(&result_line)
            .expect("parse")
            .remove(0),
    );
    assert!(updates.is_empty());
}

#[test]
fn invalid_todo_status_falls_back_to_pending() {
    let mut mapper = TranscriptUpdateMapper::new(None, false);
    let line = json!({
        "type": "assistant",
        "sessionId": "session-a",
        "message": {"role": "assistant", "content": [{
            "type": "tool_use",
            "id": "toolu_todo",
            "name": "TodoWrite",
            "input": {"todos": [{"content": "Unknown", "status": "blocked"}]}
        }]}
    })
    .to_string();
    let updates = mapper.updates_for_event(&parse_transcript_line(&line).expect("parse").remove(0));
    let SessionUpdate::Plan(plan) = &updates[0] else {
        panic!("expected plan update");
    };
    assert_eq!(plan.entries[0].status, PlanEntryStatus::Pending);
}
