# Next-turn mode switch design

Date: 2026-06-13

## Goal

Make ACP session mode changes affect the real Claude Code CLI on the next user turn when the adapter is reusing an existing PTY.

Today, `session/set_mode` and `session/set_config_option` update adapter state, but a live PTY keeps running with the permission mode it was launched with. The next prompt can therefore run under the old Claude Code permission mode. This design fixes that without implementing the ACP path through the Claude Agent SDK and without depending on `claude -p`.

## Research summary

### ACP protocol

ACP v1 allows clients to change session modes with `session/set_mode`, and says the current mode can change at any point during a session. Session config options are the preferred forward-compatible path; clients can call `session/set_config_option` with `configId: "mode"` and the agent must return the complete config state.

Relevant sources:

- https://agentclientprotocol.com/protocol/v1/session-modes
- https://agentclientprotocol.com/protocol/v1/session-config-options

### Claude Code CLI

Claude Code exposes permission modes through launch flags, settings, and TUI controls:

- `claude --permission-mode acceptEdits`
- `claude --permission-mode plan`
- `Shift+Tab` cycles modes in the interactive TUI.
- `/plan` prefixes a single prompt into plan behavior.

Claude Code docs list current modes as `default`, `acceptEdits`, `plan`, `auto`, `dontAsk`, and `bypassPermissions`. The docs also state permissions are enforced by Claude Code, not model instructions, so prompt text is not a reliable way to change permission behavior.

Relevant sources:

- https://code.claude.com/docs/en/permission-modes
- https://code.claude.com/docs/en/permissions

### Prior art

`zed-industries/codex-acp` can directly submit runtime thread settings to Codex, so mode changes can mutate the underlying agent state without process restart. It also sends available commands shortly after load.

`agentclientprotocol/claude-agent-acp` uses the Claude Agent SDK, which exposes `query.setPermissionMode`. It keeps ACP modes and config options in sync, emits `current_mode_update`, and updates SDK mode directly.

This repository intentionally drives the real interactive `claude` CLI through a PTY. It does not have a stable direct runtime API equivalent to Codex thread settings or Claude SDK `setPermissionMode`.

## Repository integration map

Current implementation points:

- ACP handlers: `src/acp/server.rs`
  - `set_session_mode` calls `ManagedSession::set_permission_mode`.
  - `set_session_config_option` calls `ManagedSession::set_config_option`.
- Session state and PTY lifecycle: `src/session/manager.rs`
  - `ManagedSession` stores desired model and permission mode separately from the live PTY.
  - `ensure_pty` reuses an existing `ClaudePtySession` before calculating launch arguments.
- PTY launch: `src/pty/session.rs`
  - `ClaudePtyConfig::launch_argv` adds `--permission-mode` only when spawning a new process.
  - `ClaudePtySession` records the launch-time `permission_mode`, but it is not exposed for reconciliation.
- Settings-derived mode: `src/config/session.rs`
  - `SessionConfigState` records the current ACP-visible mode.
  - If the mode came only from settings, `ManagedSession.permission_mode` currently starts as `None`.

## Recommended approach

Use a deterministic PTY restart before the next prompt whenever the desired permission mode differs from the launch mode of the reusable PTY.

Flow:

1. `session/set_mode` and `session/set_config_option(mode)` continue to update desired session mode.
2. If a prompt is already running, do not interrupt it.
3. On the next `session/prompt`, while holding the existing per-session prompt lock:
   - take the stored PTY;
   - compute the desired permission mode from prompt options, explicit session state, or settings-backed config;
   - compare desired mode with the PTY launch mode, treating `None` and `default` as equivalent;
   - if different, terminate the old PTY and spawn a new one using the same session id plus `--resume <session-id>` and `--permission-mode <desired-mode>`;
   - wait for the idle prompt and submit the user's prompt.
4. If no PTY exists, spawn normally with the desired mode.
5. Store the restarted PTY after the turn just like the current path.

The new behavior is intentionally next-turn, not mid-turn. A mode change made while Claude is currently generating affects the following prompt.

## Why not Shift+Tab

Shift+Tab is a TUI cycling control, not a direct "set this mode" command. To use it safely the adapter would need to know the current visible mode, the complete cycle order, and which modes are enabled for the account/model. That is fragile across Claude Code versions and terminal rendering changes. Restarting with `--permission-mode` uses a documented CLI interface and keeps the adapter deterministic.

## Why not `/plan`

`/plan` is useful for one prompt but does not generalize to `acceptEdits`, `auto`, `dontAsk`, or `bypassPermissions`. It also gives plan mode special semantics that differ from the ACP session mode selector. This design keeps all advertised modes on one path.

## Detailed design

### Permission mode normalization

Add a small helper in the session layer:

- canonical desired mode is `options.permission_mode`, else explicit session permission mode, else `config.mode()`;
- launch mode is `None` for `default` and `Some(mode)` for non-default modes, unless explicit `default` is useful for parity;
- comparison treats `None` and `Some("default")` as equal.

This also makes settings-backed non-default modes, such as project `defaultMode: "plan"`, affect initial PTY launch.

### PTY metadata

Expose the launch permission mode from `ClaudePtySession` with a read-only method, for example:

```rust
pub fn permission_mode(&self) -> Option<&str>
```

Do not expose transcript text or screen contents through logs. The only compared data is the mode id.

### PTY restart

When reconciliation detects a mismatch:

- terminate the old PTY without logging transcript content;
- spawn a new `ClaudePtySession` with:
  - same executable;
  - same cwd;
  - same ACP session id;
  - same selected model behavior as the current spawn path;
  - desired permission mode;
  - `resume: Some(session_id)`;
  - existing MCP config and setting-source behavior.

The implementation can use `send_exit` before `terminate` on an idle PTY, but termination must not block the next turn indefinitely. A failed graceful exit should fall back to killing the PTY child.

### Prompt flow

Keep the existing `prompt_lock` as the concurrency boundary. Mode reconciliation happens after the lock is acquired and before writing the next user prompt into the PTY.

This preserves current prompt queue behavior: concurrent prompts serialize through one session, and a mode update made between them applies before the later prompt is submitted.

### ACP response behavior

This feature does not require broad protocol changes:

- `session/set_config_option(mode)` should still return the complete config options response.
- `session/set_mode` can continue returning the default response.

If implementation scope allows, emitting `current_mode_update` and `config_option_update` after successful client-initiated mode changes would improve client synchronization, but it is not required for the next-turn runtime fix.

## Error handling

If the restarted Claude process fails to spawn, return an ACP internal error for that prompt. The session should not silently submit the prompt to the stale-mode PTY.

If Claude rejects an unsupported mode, surface the spawn error. The existing advertised mode list can still be refined later, for example by gating `auto`, but that is outside this feature.

If resume fails because Claude cannot find the session, surface the error rather than falling back to a new unrelated conversation.

## Tests

Add focused tests before implementation is considered complete:

1. Unit test mode normalization:
   - `None` equals `default`;
   - settings-derived `plan` is non-default;
   - explicit prompt option overrides session state.
2. PTY fake integration test:
   - first prompt launches fake Claude without a mode or with initial mode;
   - `set_session_mode("plan")` updates session state;
   - second prompt restarts fake Claude with `--resume <session-id>` and `--permission-mode plan`;
   - prompt output still reaches transcript mapping.
3. Config-option path test:
   - `session/set_config_option(mode=acceptEdits)` follows the same next-turn restart path.
4. Non-regression:
   - no restart when the mode is unchanged;
   - queued prompts remain serialized.

Verification commands after implementation:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```

Real e2e remains opt-in:

```sh
just real-e2e
```

Current local note: this workspace currently reports `rustc 1.93.1`, while `Cargo.toml` requires Rust 1.95. Verification needs a Rust 1.95 or newer toolchain.

## Out of scope

- Do not implement ACP through Claude Agent SDK.
- Do not use `claude -p`.
- Do not alias over the real `claude` binary.
- Do not implement TUI Shift+Tab cycling.
- Do not implement model switching for an already-live PTY, even though the current code has a similar stale-process issue for model changes.
- Do not expand available-command discovery in this feature.

## Open implementation notes

Prefer keeping responsibility boundaries:

- mode normalization and desired-vs-active comparison in the session layer;
- launch argument details in the PTY layer;
- ACP request handlers limited to state mutation and response shaping.

The smallest maintainable implementation is to extract PTY spawn config construction from `ensure_pty`, then let `ensure_pty` decide whether to reuse, restart, or create a PTY.
