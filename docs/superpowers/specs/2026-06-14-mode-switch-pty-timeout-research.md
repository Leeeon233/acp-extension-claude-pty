# Mode switch PTY timeout research

Date: 2026-06-14

## Question

Investigate why an ACP conversation created in Claude Code plan mode times out when the same ACP session is continued after switching to another mode.

## Current research

### ACP protocol

ACP v1 allows `session/set_mode` at any time during a session, including while the agent is idle or generating. ACP v1 also says session config options are the preferred forward-compatible selector surface; mode-like agents should expose both legacy `modes` and a `configOptions` entry with `category: "mode"`.

Sources:

- https://agentclientprotocol.com/protocol/v1/session-modes
- https://agentclientprotocol.com/protocol/v1/session-config-options

### Claude Code CLI

Live local CLI:

- `claude --version`: `2.1.177 (Claude Code)`
- `claude --help` lists `--session-id`, `--resume`, `--continue`, and `--permission-mode <mode>`.
- `--permission-mode` choices in live help: `acceptEdits`, `auto`, `bypassPermissions`, `default`, `dontAsk`, `plan`.

Official docs still describe `--permission-mode` as the session permission mode flag, `--resume` as resuming a conversation, and settings `permissions.defaultMode` values as `default`, `acceptEdits`, `plan`, `auto`, `dontAsk`, and `bypassPermissions`. Interactive mode can cycle permission modes with Shift+Tab, but that remains a TUI control, not a deterministic adapter API.

Sources:

- https://code.claude.com/docs/en/cli-reference
- https://code.claude.com/docs/en/settings
- https://code.claude.com/docs/en/interactive-mode
- https://github.com/anthropics/claude-code/raw/refs/heads/main/CHANGELOG.md

### Prior art

`agentclientprotocol/claude-agent-acp` uses the Claude Agent SDK rather than a PTY, so it can update SDK permission mode state directly. That path is intentionally outside this repository's product boundary.

`zed-industries/codex-acp` is an ACP adapter for Codex, not Claude Code PTY. Its architecture can apply runtime thread settings to the underlying agent and does not need to rediscover a resumed Claude TUI prompt.

The relevant prior-art contrast is unchanged: PTY-driven Claude Code mode switching must either use documented CLI launch/resume flags or fragile terminal controls. Restarting the PTY with `--resume` remains the better boundary, but the launch argv must track current Claude CLI restrictions.

Sources:

- https://github.com/agentclientprotocol/claude-agent-acp
- https://github.com/zed-industries/codex-acp
- https://zed.dev/docs/ai/external-agents

### Dependency research

The repo uses `portable-pty = 0.9.0` to spawn and communicate with the real interactive Claude CLI and `vt100 = 0.16.2` to parse terminal output into a screen snapshot. These dependencies remain appropriate for the adapter boundary:

- `portable-pty` provides the cross-platform PTY primitive and lets Claude see a TTY.
- `vt100` gives a maintained terminal screen parser so recognizers can operate on rendered screen text rather than raw escape sequences.

Sources:

- https://docs.rs/portable-pty
- https://docs.rs/vt100/latest/vt100/struct.Parser.html

## Repository integration map

- `src/acp/server.rs`
  - `session/set_mode` updates `ManagedSession.permission_mode`.
  - `session/prompt` calls `ManagedSession::prompt_with_permission_handler`.
- `src/session/manager.rs`
  - `ensure_pty` compares desired mode with the live PTY mode.
  - On mismatch it terminates the old PTY and spawns a new one with `resume: Some(session_id)`.
  - `wait_for_idle_prompt` waits for an idle prompt before submitting the next user prompt.
- `src/pty/session.rs`
  - `ClaudePtyConfig::launch_argv` builds the real `claude` argv.
  - Before this fix it always added `--session-id`, then also added `--resume` for restarts.
  - An opt-in `CLAUDE_CODE_ACP_DEBUG_PTY_DIR` capture now records raw PTY bytes for reproductions.
- `script/debug-acp-mode-switch.py`
  - Drives a real stdio ACP flow:
    `initialize -> session/new -> set_mode(plan) -> prompt -> set_mode(acceptEdits) -> prompt -> close`.
  - Writes ACP JSON-RPC logs, stderr, raw PTY `.ansi`, and helper `.txt` views under `target/acp-mode-switch-debug/<timestamp>/`.

## Reproduction

Command:

```sh
cargo +nightly build
script/debug-acp-mode-switch.py --startup-timeout 25 --request-timeout 180
```

Initial failing run before the argv fix:

- Artifact directory: `target/acp-mode-switch-debug/20260614-151441/`
- ACP error: `timed out waiting for Claude interactive prompt`
- Second PTY raw file: `002-e3905c56-8dac-4910-8806-49bba6dd37c9-resume-acceptEdits.ansi`
- Relevant PTY output:

```text
Error: --session-id can only be used with --continue or --resume if --fork-session is also specified.
```

This proves the timeout was not caused by an unrecognized idle prompt after switching modes. The restarted Claude process exited immediately because the adapter passed a Claude 2.1.177-invalid argv combination: `--session-id <id> --resume <id> --permission-mode acceptEdits`.

Post-fix run:

- Artifact directory: `target/acp-mode-switch-debug/20260614-151734/`
- Script status: `ok`
- Second PTY raw file: `002-b275c806-c980-4712-9f25-5d48c7f11044-resume-acceptEdits.ansi`
- Relevant PTY output includes the resumed TUI footer and second prompt:

```text
accept edits on (shift+tab to cycle)
Reply exactly ACP_MODE_SWITCH_SECOND_READY. Do not use tools.
```

The local account/provider state returned `Not logged in · Please run /login`, so this run validates the PTY/session mechanics but not an authenticated model answer.

## Fix

When launching a PTY:

- New conversations continue to pass `--session-id <uuid>`.
- Resumed or continued conversations do not pass `--session-id`.
- Mode restart still passes `--resume <session-id>` and `--permission-mode <desired-mode>`.

This follows the live Claude 2.1.177 CLI behavior and lets `claude --resume <session-id>` preserve the conversation identity without tripping the new argv validation.

## Verification

Focused local verification:

```sh
cargo fmt --check
cargo +nightly test --test dynamic_mode_switch --test pty_screen
cargo +nightly build
script/debug-acp-mode-switch.py --startup-timeout 25 --request-timeout 180
```

Additional local verification also passed:

```sh
cargo +nightly clippy --workspace --all-targets --all-features -- -D warnings
cargo +nightly test --workspace --all-targets --all-features
```

Full workspace verification still needs a Rust 1.95+ stable toolchain. The local `stable` override is currently Rust 1.93.1; `cargo build` without `+nightly` fails because `Cargo.toml` requires Rust 1.95.
