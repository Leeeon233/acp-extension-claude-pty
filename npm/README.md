# acp-extension-claude-pty

NPM wrapper for `acp-extension-claude-pty`, an ACP adapter for the real Claude Code CLI.

## Install

```sh
npm install -g acp-extension-claude-pty
acp-extension-claude-pty doctor
```

The base package requires Node 18+ and installs one optional platform package for your OS and CPU:

- `acp-extension-claude-pty-darwin-arm64`
- `acp-extension-claude-pty-darwin-x64`
- `acp-extension-claude-pty-linux-arm64`
- `acp-extension-claude-pty-linux-x64`
- `acp-extension-claude-pty-win32-arm64`
- `acp-extension-claude-pty-win32-x64`

## Prerequisites

Install and authenticate Claude Code separately:

```sh
npm install -g @anthropic-ai/claude-code
claude
```

`npx acp-extension-claude-pty doctor` is useful for a quick smoke test, but editors should be configured with a stable installed binary.

## Usage

ACP server:

```sh
acp-extension-claude-pty
```

Zed custom-agent settings can use the installed binary directly:

```json
{
  "agent_servers": {
    "acp-extension-claude-pty": {
      "type": "custom",
      "command": "acp-extension-claude-pty",
      "args": [],
      "env": {}
    }
  }
}
```

Interactive pass-through:

```sh
acp-extension-claude-pty interactive -- --model sonnet
```

Print replacement for `claude -p`:

```sh
acp-extension-claude-pty print "summarize this repository" --output-format text
```

Doctor:

```sh
acp-extension-claude-pty doctor --live-docs
```

## Security

Claude stores transcripts as plaintext under `~/.claude/projects`. Do not publish debug logs or transcript fixtures without sanitizing message and tool-result bodies.
