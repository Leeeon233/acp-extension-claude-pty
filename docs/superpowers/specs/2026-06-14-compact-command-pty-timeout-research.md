# Compact command PTY timeout research

Date: 2026-06-14

## Question

Investigate why starting an ACP prompt turn with `/compact` still times out, and record what the PTY actually renders.

## Current research

### Claude Code slash commands

Live local CLI:

- `claude --version`: `2.1.177 (Claude Code)`
- `/compact [instructions]` is documented as a built-in slash command that frees context by summarizing the conversation so far.
- Claude slash commands are recognized at the start of a message.
- The Claude SDK documentation also shows slash commands can be sent as prompt strings, including `prompt: "/compact"`, which confirms `/compact` is a command surface rather than ordinary model text.
- Claude Code can also load environment variables from `settings.json` under `env`; this works even when the process launcher itself does not have those variables. Passing `--setting-sources project` prevents user settings such as `~/.claude/settings.json` from being read.

Sources:

- https://code.claude.com/docs/en/commands
- https://code.claude.com/docs/en/agent-sdk/slash-commands
- https://code.claude.com/docs/en/env-vars

### Prior art

`agentclientprotocol/claude-agent-acp` uses the Claude Agent SDK instead of a PTY, so SDK slash-command messages can be observed through SDK stream events such as command boundaries.

`zed-industries/codex-acp` is a PTY-free ACP adapter for Codex. Its command surface includes slash-command-like prompts, but it does not need to classify Claude TUI state or tail Claude JSONL transcripts.

The relevant prior-art contrast is unchanged: this repo must bridge Claude Code's interactive TUI, PTY screen state, and JSONL transcript. Local slash commands may complete without any assistant-role transcript event.

Sources:

- https://github.com/agentclientprotocol/claude-agent-acp
- https://github.com/zed-industries/codex-acp
- https://www.npmjs.com/package/@zed-industries/codex-acp

### Dependency research

The repo's current PTY and terminal parsing dependencies remain appropriate:

- `portable-pty = 0.9.0` gives Claude Code a real TTY and transports prompt bytes.
- `vt100 = 0.16.2` renders raw PTY bytes into screen text for recognizers.

The timeout was not caused by these dependencies losing bytes. The raw debug capture contains the `/compact` command, the rendered local-command error, and the resumed prompt/footer. The missing bridge was transcript classification: the completion signal was a `system` local command event, not an `assistant` event.

Sources:

- https://docs.rs/portable-pty
- https://docs.rs/vt100/latest/vt100/struct.Parser.html

## Repository integration map

- `src/session/manager.rs`
  - `prompt_with_permission_handler` submits the ACP prompt to the Claude PTY.
  - Before this fix it only treated non-empty assistant messages and tool results as transcript completion events.
  - A `/compact` local-command result therefore never satisfied the completion condition.
  - The screen fallback must not complete a turn while Claude is still rendering `Thinking...`, even if stale prompt text like `❯/review` remains on screen.
- `src/transcript/events.rs`
  - User-side local-command metadata was already stripped during replay.
  - The same filtering must also strip `<local-command-caveat>` so internal Claude caveat text is not replayed to ACP clients when a transcript file is discovered after the turn starts.
  - Slash-command expansions such as `/review` can appear as `user` records with `isMeta: true`; those are Claude-generated command prompt bodies and should not be replayed as ACP user content.
  - System-side `<local-command-stdout>` / `<local-command-stderr>` output was parsed as a generic `System` event.
- `src/acp/updates.rs`
  - Before this fix it dropped `System` and `Diagnostic` events, so the local-command result was also invisible to ACP clients.
- `src/terminal/recognizers.rs`
  - `recognize_screen` reports `Error` when the screen contains `Error:`.
  - The captured PTY screen can therefore be classified as `Error` even after Claude has returned to an input prompt.

## Reproduction

ACP flow:

```text
initialize -> session/new -> session/prompt("/compact")
```

Debug artifact directory:

```text
target/acp-compact-debug/20260614-165121/
```

ACP response before the fix:

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32603,
    "message": "Internal error",
    "data": "timed out waiting for Claude transcript completion for session e2c42835-fc67-4a74-a84a-4d57473be320 (screen status: Error)"
  },
  "id": 3
}
```

Relevant PTY rendering:

```text
/compact
/compact Free up context by summarizing the conversation so far
/security-review Complete a security review of the pending changes on the current branch
/workflows Browse running and completed workflows
❯/compact
⎿  Error: No messages to compact

? for shortcuts · ← for agents
Not logged in · Run /login
Resume this session with:
claude --resume e2c42835-fc67-4a74-a84a-4d57473be320
```

Relevant transcript events:

```json
{"type":"user","message":{"role":"user","content":"<command-name>/compact</command-name>\n            <command-message>compact</command-message>\n            <command-args></command-args>"}}
{"type":"system","subtype":"local_command","content":"<local-command-stderr>Error: No messages to compact</local-command-stderr>","level":"info"}
```

ACP notifications before the fix contained the user prompt and available-command update, but no agent output.

The durable reproducer is:

```sh
script/debug-acp-compact.py --startup-timeout 25 --request-timeout 90
```

It writes JSON-RPC, stderr, raw PTY `.ansi`, and readable PTY `.txt` files under `target/acp-compact-debug/<timestamp>/`.

## Fix

The adapter now treats transcript `System` events containing `<local-command-stdout>` or `<local-command-stderr>` as local-command output:

- The session wait loop can complete the prompt turn when a local-command output event appears and the screen is idle, errored, or unavailable.
- The ACP update mapper emits the stripped local-command output as an `agent_message_chunk`.
- Local-command caveat metadata is stripped from replayed user transcript records.
- `isMeta: true` user transcript records are filtered so slash-command expansion templates are not echoed back to ACP clients.
- Ordinary system and diagnostic events remain suppressed.
- Debug ACP scripts no longer set `CLAUDE_CODE_ACP_SETTING_SOURCES=project` by default, so user settings env such as `ANTHROPIC_AUTH_TOKEN` are not accidentally hidden.

This keeps `/compact` visible and finite without broadening completion detection to all system transcript records.

## Post-fix verification

Command:

```sh
script/debug-acp-compact.py --startup-timeout 25 --request-timeout 90
```

Artifact directory:

```text
target/acp-compact-debug/20260614-170305/
```

ACP updates now include the local command output and complete the turn:

```json
{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"/compact"}}
{"sessionUpdate":"available_commands_update"}
{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Error: No messages to compact"}}
{"stopReason":"end_turn"}
```

## Related `/review` checks

Command:

```sh
script/debug-acp-compact.py --prompt /review --startup-timeout 25 --request-timeout 180
```

Artifact directory:

```text
target/acp-compact-debug/20260614-171423/
```

PTY rendering showed `/review` was recognized as a slash command and expanded:

```text
/review Review a pull request
/code-review Review the current diff for correctness bugs ...
/security-review Complete a security review of the pending changes on the current branch
❯/review
✽ Omosing...
⎿ Not logged in · Please run /login
✻ Worked for 0s
```

The transcript contained:

- a command marker user record for `/review`
- an `isMeta: true` user record with the expanded review prompt body
- an assistant record with `error: "authentication_failed"` and text `Not logged in · Please run /login`

That means the observed early end is not a PTY timeout path. In this local environment it ends because Claude cannot authenticate. The adapter now filters the meta user expansion and forwards the assistant authentication message before returning `end_turn`.

After allowing user settings with:

```sh
script/debug-acp-compact.py --prompt /review --setting-sources user,project --startup-timeout 25 --request-timeout 180
```

Artifact directory:

```text
target/acp-compact-debug/20260614-173213/
```

Claude loaded the user settings authentication env and proceeded into the real review flow. It requested permission for `gh pr list`; the debug client rejected the request, so ACP emitted the Bash tool call, a failed tool result, `[Request interrupted by user for tool use]`, and then `end_turn`.

An intermediate run also exposed a PTY-screen fallback bug: while the TUI showed stale `❯/review` text and `⏺ Thinking for 2s, running 1 shell command...`, the adapter could mistake the screen for an idle assistant response and end the turn early. The settled-screen check now requires an idle prompt with no active thinking, permission, or workspace-trust state before using screen fallback completion.
