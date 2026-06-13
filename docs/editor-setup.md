# Editor Setup

`acp-extension-claude-pty` is a local ACP agent. Configure editors to spawn the binary directly and communicate over stdio.

## Zed

Zed supports external agents through ACP Registry entries, Agent Server extensions, and manual custom-agent settings.

### Manual Custom Agent

Install the adapter first:

```sh
npm install -g acp-extension-claude-pty
acp-extension-claude-pty doctor
```

Then add a custom agent to Zed `settings.json`:

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

If Zed cannot see the same `PATH` as your shell, use an absolute adapter path:

```sh
which acp-extension-claude-pty
```

If Zed cannot find the Claude Code CLI, pass the real `claude` binary path without aliasing over it:

```json
{
  "agent_servers": {
    "acp-extension-claude-pty": {
      "type": "custom",
      "command": "/absolute/path/to/acp-extension-claude-pty",
      "args": [],
      "env": {
        "CLAUDE_CODE_CLI": "/absolute/path/to/claude"
      }
    }
  }
}
```

Manual Zed requirements:

- `acp-extension-claude-pty` exists on the path Zed uses, or `command` is absolute.
- `claude` exists on the path Zed uses, or `CLAUDE_CODE_CLI` points to it.
- Claude Code has been authenticated once in a terminal.
- The editor process can read and write the working directory.

Do not use `npx` in persistent editor config. Use `npx acp-extension-claude-pty doctor` only for smoke checks.

Do not wrap the command in a shell script that prints banners, prompts, or status text to stdout. ACP stdout must contain only protocol JSON-RPC frames.

### Registry Install

After this adapter is published to the ACP Registry, install it from Zed with the `zed::AcpRegistry` command or the Agent Panel configuration view's Add Agent button. Registry-installed agents update automatically and take precedence over an Agent Server extension with the same agent.

Registry publication requires the adapter to return at least one ACP `authMethods` entry during `initialize`. This adapter advertises an existing-Claude-login method and, when the client supports terminal auth, a terminal login method that starts interactive pass-through.

To customize a registry-installed agent, use its registry id with `type: "registry"`:

```json
{
  "agent_servers": {
    "acp-extension-claude-pty": {
      "type": "registry",
      "env": {
        "CLAUDE_CODE_CLI": "/absolute/path/to/claude"
      }
    }
  }
}
```

### Agent Server Extension

Zed Agent Server extensions are still supported, but Zed marks the ACP Registry as the preferred path for external agents. Extension publication needs an `extension.toml` with `[agent_servers.<id>]` targets that download release archives, plus platform `cmd`, optional `args`, optional `sha256`, and optional `env`.

The extension path is useful when Zed-specific packaging is required. Use the ACP Registry for the default public Zed install path.

### Debugging

Open `dev: open acp logs` from the Zed Command Palette to inspect ACP traffic. Do not share logs without redacting prompts, transcript text, tool output, and local paths.

## Other ACP Clients

Use the same command as the agent executable:

```sh
acp-extension-claude-pty
```

Pass project roots as ACP client session data when the client supports it. Paths sent through ACP must be absolute.

## Terminal Shortcuts

For normal terminal use, keep `claude` pointing at the installed Claude Code CLI. If you want a shortcut for this adapter, use a non-conflicting alias:

```sh
alias claude-acp='acp-extension-claude-pty'
```
