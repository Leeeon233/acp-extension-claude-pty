# claude-code-cli-acp

NPM wrapper for `claude-code-cli-acp`, an ACP adapter for the real Claude Code CLI.

## Install

```sh
npm install -g claude-code-cli-acp
claude-code-cli-acp doctor
```

The base package requires Node 18+ and installs one optional platform package for your OS and CPU:

- `claude-code-cli-acp-darwin-arm64`
- `claude-code-cli-acp-darwin-x64`
- `claude-code-cli-acp-linux-arm64`
- `claude-code-cli-acp-linux-x64`
- `claude-code-cli-acp-win32-arm64`
- `claude-code-cli-acp-win32-x64`

## Prerequisites

Install and authenticate Claude Code separately:

```sh
npm install -g @anthropic-ai/claude-code
claude
```

`npx claude-code-cli-acp doctor` is useful for a quick smoke test, but editors should be configured with a stable installed binary.

## Usage

ACP server:

```sh
claude-code-cli-acp
```

Zed custom-agent settings can use the installed binary directly:

```json
{
  "agent_servers": {
    "claude-code-cli-acp": {
      "type": "custom",
      "command": "claude-code-cli-acp",
      "args": [],
      "env": {}
    }
  }
}
```

Interactive pass-through:

```sh
claude-code-cli-acp interactive -- --model sonnet
```

Print replacement for `claude -p`:

```sh
claude-code-cli-acp print "summarize this repository" --output-format text
```

Doctor:

```sh
claude-code-cli-acp doctor --live-docs
```

## Security

Claude stores transcripts as plaintext under `~/.claude/projects`. Do not publish debug logs or transcript fixtures without sanitizing message and tool-result bodies.
