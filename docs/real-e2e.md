# Real E2E

Real e2e verifies this adapter against an installed, authenticated Claude Code CLI. It is opt-in and serial.

## Prerequisites

- `claude` installed on `PATH`.
- Claude Code authenticated in the same user account.
- Network access available when Claude needs it.
- A disposable temp project can be created.
- Transcript persistence is enabled.

Do not set:

```sh
CLAUDE_CODE_SKIP_PROMPT_HISTORY=1
```

Do not pass `--no-session-persistence` in real e2e print cases.

## Command

```sh
CLAUDE_CODE_ACP_REAL_E2E=1 cargo test --test real_e2e -- --ignored --test-threads=1
```

Equivalent:

```sh
just real-e2e
```

## Current Required Coverage

Real e2e must prove:

- The adapter launches real `claude`, not a fake process.
- Core flow does not call `claude -p`.
- A deterministic `--session-id` is used.
- Transcript JSONL is created under `~/.claude/projects`.
- `print` supports prompt args, stdin, text output, and JSON output.
- `interactive -- --version` passes through to Claude.
- ACP prompt flow can ask the ACP client for Claude permission, send the selected choice into the TUI, and continue the turn.
- ACP permission/edit flow creates the expected file in the isolated temp repo.
- Session exits cleanly through `/exit` after transcript extraction.

## Permission/Edit Gate

ACP permission/edit e2e uses project-only Claude settings for that test so user-level default bypass modes do not mask permission prompts. It then sets ACP session mode to `default`, asks real Claude to run a permission-gated command, approves the visible prompt through `session/request_permission`, and asserts the file was created.

If real e2e cannot run, report it as blocked. Do not replace it with fixture-only tests.
