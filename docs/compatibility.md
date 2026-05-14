# Compatibility

`claude-code-cli-acp` tracks live Claude Code CLI behavior. Local assumptions must be checked against `claude --help`, official docs, and npm package metadata.

## Required Claude Surfaces

The adapter depends on:

- Interactive `claude` mode.
- `--session-id <uuid>` for transcript correlation.
- `--resume`, `--continue`, `--model`, `--permission-mode`, `--settings`, `--add-dir`, `--debug-file`, `--mcp-config`, and `--strict-mcp-config` pass-through where supported.
- Transcript JSONL under `~/.claude/projects`.
- Interactive commands used by automation, including `/exit`.

`print` mode intentionally mirrors current `claude -p` user-facing flags, but it must drive interactive Claude through PTY instead of invoking `claude -p`.

## Current Research Snapshot

The 2026-05-14 refresh recorded:

- Local Claude Code: `2.1.141 (Claude Code)` in the final live doctor run.
- npm `@anthropic-ai/claude-code` latest and next: `2.1.141`.
- npm stable: `2.1.128`.
- `--enable-auto-mode` removed; use `--permission-mode auto`.

Update this section whenever `doctor --live-docs` or `just drift-live` detects a changed Claude CLI, npm package, or docs surface.

## Drift Checks

Local doctor:

```sh
claude-code-cli-acp doctor
```

Live docs/npm check:

```sh
claude-code-cli-acp doctor --live-docs
just drift-live
```

Drift checks should distinguish:

- Present in local `--help`.
- Present in official docs.
- Probeable by direct invocation.
- Removed or deprecated upstream.

Do not fail solely because `claude --help` omits a documented flag; official docs state help may be incomplete.

## ACP Surface Scope

Current ACP support is intentionally limited to behavior backed by real Claude CLI/PTTY/transcript evidence:

- Advertised: initialize, auth methods, new/load/list/close session, prompt, cancel, config options, available commands, permission requests, tool updates, TODO plan updates, and terminal-output metadata/fallbacks.
- Not advertised in the current release: fork/resume, SDK raw messages, SDK usage/cost updates, image parity, and separate SDK following streams.

Authentication remains the installed Claude CLI's responsibility. Run `claude` once in a terminal before using this adapter; `doctor` reports missing or incompatible local Claude state. ACP `authMethods` expose the existing Claude Code login path and, for clients with terminal-auth support, an interactive pass-through path where users can complete Claude login.
