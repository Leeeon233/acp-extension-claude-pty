use std::fs;

use acp_extension_claude_pty::config::commands::available_commands;
use agent_client_protocol::schema::AvailableCommandInput;

#[test]
fn discovers_project_commands_and_skills_with_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let commands_dir = temp.path().join(".claude/commands/nested");
    let skills_dir = temp.path().join(".claude/skills/ship");
    fs::create_dir_all(&commands_dir).expect("commands dir");
    fs::create_dir_all(&skills_dir).expect("skills dir");
    fs::write(
        temp.path().join(".claude/commands/quick-math.md"),
        r#"---
description: "10 * 3 = 30 (project)"
argument-hint: "expression"
---
Calculate quickly.
"#,
    )
    .expect("quick math command");
    fs::write(
        commands_dir.join("review.md"),
        r#"# Review Changes

Review the current repository.
"#,
    )
    .expect("nested command");
    fs::write(
        skills_dir.join("SKILL.md"),
        r#"---
description: Ship release checklist
---

Run release checks.
"#,
    )
    .expect("skill");

    let commands = available_commands(temp.path());

    assert!(
        commands
            .iter()
            .any(|command| command.name == "compact" && command.description.contains("Free up"))
    );
    assert!(commands.iter().all(|command| command.name != "logout"));
    let quick_math = commands
        .iter()
        .find(|command| command.name == "quick-math")
        .expect("quick-math command");
    assert_eq!(quick_math.description, "10 * 3 = 30 (project)");
    let Some(AvailableCommandInput::Unstructured(input)) = &quick_math.input else {
        panic!("quick-math should have input hint");
    };
    assert_eq!(input.hint, "expression");

    assert!(commands.iter().any(|command| {
        command.name == "nested:review" && command.description == "Review Changes"
    }));
    assert!(
        commands.iter().any(
            |command| command.name == "ship" && command.description == "Ship release checklist"
        )
    );
}
