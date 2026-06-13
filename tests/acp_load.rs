use acp_extension_claude_pty::transcript::events::{
    TranscriptEventKind, parse_transcript_line, strip_local_command_metadata,
};
use serde_json::json;

#[test]
fn local_command_metadata_is_stripped_from_loaded_user_messages() {
    let stripped = strip_local_command_metadata(
        "<command-name>/quick-math</command-name>\n<command-args>2+2</command-args>\nvisible",
    );
    assert_eq!(stripped, "visible");
}

#[test]
fn marker_only_local_command_replay_is_not_emitted_as_user_content() {
    let line = json!({
        "type": "user",
        "sessionId": "session-a",
        "message": {"role": "user", "content": [{
            "type": "text",
            "text": "<command-name>/quick-math</command-name><local-command-stdout>4</local-command-stdout>"
        }]}
    })
    .to_string();
    let events = parse_transcript_line(&line).expect("parse");
    assert!(events.is_empty());
}

#[test]
fn normal_user_replay_survives_marker_filter() {
    let line = json!({
        "type": "user",
        "sessionId": "session-a",
        "message": {"role": "user", "content": [{
            "type": "text",
            "text": "real prompt"
        }]}
    })
    .to_string();
    let events = parse_transcript_line(&line).expect("parse");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, TranscriptEventKind::UserMessage);
    assert_eq!(events[0].text.as_deref(), Some("real prompt"));
}
