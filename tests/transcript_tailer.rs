use std::fs::{self, OpenOptions};
use std::io::Write;

use acp_extension_claude_pty::transcript::events::{TranscriptEventKind, parse_transcript_line};
use acp_extension_claude_pty::transcript::tailer::{TranscriptLocator, TranscriptTailer};

#[test]
fn transcript_events_parse_text_arrays_without_plaintext_leaks() {
    let line = r#"{"type":"assistant","sessionId":"session-a","message":{"role":"assistant","content":[{"type":"text","text":"secret assistant body"},{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"cat ~/.ssh/id_rsa"}}]}}"#;
    let events = parse_transcript_line(line).expect("parse line");

    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0],
        event if event.kind == TranscriptEventKind::AssistantMessage
            && event.session_id.as_deref() == Some("session-a")
            && event.redacted.char_count == 21
    ));
    assert!(matches!(
        &events[1],
        event if event.kind == TranscriptEventKind::ToolUse
            && event.id.as_deref() == Some("toolu_1")
            && event.name.as_deref() == Some("Bash")
            && event.redacted.text_redacted
    ));

    let debug = format!("{events:?}");
    assert!(!debug.contains("secret assistant body"));
    assert!(!debug.contains("id_rsa"));
}

#[test]
fn transcript_events_parse_tool_results() {
    let line = r#"{"type":"user","sessionId":"session-a","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":false,"content":"secret output"}]}}"#;
    let events = parse_transcript_line(line).expect("parse line");

    assert!(matches!(
        &events[0],
        event if event.kind == TranscriptEventKind::ToolResult
            && event.session_id.as_deref() == Some("session-a")
            && event.id.as_deref() == Some("toolu_1")
            && event.redacted.char_count == 13
    ));
}

#[test]
fn locator_recursively_finds_encoded_project_transcript_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transcript = temp
        .path()
        .join("projects")
        .join("-Users-mkh-workspace-demo")
        .join("nested")
        .join("session-a.jsonl");
    fs::create_dir_all(transcript.parent().expect("parent")).expect("mkdir");
    fs::write(&transcript, "").expect("write transcript");

    let locator = TranscriptLocator::new(temp.path());
    assert_eq!(
        locator
            .find_transcript("session-a")
            .expect("find transcript"),
        Some(transcript)
    );
}

#[test]
fn tailer_reads_incrementally_and_filters_other_sessions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transcript = temp.path().join("session-a.jsonl");
    fs::write(
        &transcript,
        concat!(
            r#"{"type":"assistant","sessionId":"other","message":{"role":"assistant","content":[{"type":"text","text":"ignore"}]}}"#,
            "\n",
            r#"{"type":"assistant","sessionId":"session-a","message":{"role":"assistant","content":[{"type":"text","text":"first"}]}}"#,
            "\n",
        ),
    )
    .expect("write transcript");

    let mut tailer = TranscriptTailer::from_path("session-a", &transcript);
    let first = tailer.poll().expect("first poll");
    assert_eq!(first.len(), 1);
    assert!(matches!(
        &first[0],
        event if event.kind == TranscriptEventKind::AssistantMessage
            && event.redacted.char_count == 5
    ));

    let mut file = OpenOptions::new()
        .append(true)
        .open(&transcript)
        .expect("open transcript");
    writeln!(
        file,
        r#"{{"type":"user","sessionId":"session-a","message":{{"role":"user","content":"second"}}}}"#
    )
    .expect("append");

    let second = tailer.poll().expect("second poll");
    assert_eq!(second.len(), 1);
    assert!(matches!(
        &second[0],
        event if event.kind == TranscriptEventKind::UserMessage
            && event.redacted.char_count == 6
    ));

    assert!(tailer.poll().expect("third poll").is_empty());
}

#[test]
fn tailer_can_start_at_end_to_ignore_prior_turns() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transcript = temp.path().join("session-a.jsonl");
    fs::write(
        &transcript,
        concat!(
            r#"{"type":"assistant","sessionId":"session-a","message":{"role":"assistant","content":[{"type":"text","text":"old"}]}}"#,
            "\n",
        ),
    )
    .expect("write transcript");

    let mut tailer =
        TranscriptTailer::from_path_at_end("session-a", &transcript).expect("tail from end");
    assert!(tailer.poll().expect("first poll").is_empty());

    let mut file = OpenOptions::new()
        .append(true)
        .open(&transcript)
        .expect("open transcript");
    writeln!(
        file,
        r#"{{"type":"assistant","sessionId":"session-a","message":{{"role":"assistant","content":[{{"type":"text","text":"new"}}]}}}}"#
    )
    .expect("append");

    let events = tailer.poll().expect("second poll");
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        event if event.kind == TranscriptEventKind::AssistantMessage
            && event.redacted.char_count == 3
    ));
}
