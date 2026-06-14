# acp-extension-claude-pty

ACP adapter for the real interactive [Claude Code CLI](https://code.claude.com/docs/en/cli-reference).

The adapter runs `claude` through a PTY, reads Claude transcript JSONL for canonical content, and exposes an [Agent Client Protocol](https://agentclientprotocol.com/) server for editors. It also keeps a direct `interactive` pass-through mode and a `print` mode that replaces `claude -p` by using the interactive CLI path.

## Install

Prerequisites:

- Install and authenticate Claude Code: `npm install -g @anthropic-ai/claude-code`, then run `claude`.
- Use Node 18+ for the npm wrapper.
- Use Rust 1.95+ when building from source.

Recommended install after npm publication:

```sh
npm install -g acp-extension-claude-pty
acp-extension-claude-pty doctor
```

Other supported install paths:

```sh
cargo install --git https://github.com/Leeeon233/acp-extension-claude-pty --locked
cargo install --path . --locked
```

Direct GitHub release archives and future Homebrew tap setup are documented in [docs/install.md](docs/install.md).

## ACP Setup

Configure any ACP-compatible client to run:

```sh
acp-extension-claude-pty
```

The ACP server speaks JSON-RPC over stdio. Editor clients must launch it as a local agent process, not through a shell that writes prompts or banners to stdout.

Zed setup details live in [docs/editor-setup.md](docs/editor-setup.md).

Zed supports registry-installed external agents and manual custom agents. This adapter can be used manually today through Zed `agent_servers`, and the ACP Registry publishing checklist is documented in [docs/publishing.md](docs/publishing.md).

## CLI Modes

Interactive pass-through:

```sh
acp-extension-claude-pty interactive -- --model sonnet
```

Default no-handshake invocation behaves like interactive pass-through, but do not alias over `claude`; that name belongs to the installed Claude Code CLI. If you want a shortcut, use a non-conflicting name:

```sh
alias claude-acp='acp-extension-claude-pty'
```

Print mode drives interactive Claude and extracts transcript output. It must not call `claude -p` for core behavior:

```sh
acp-extension-claude-pty print "summarize this repo"
echo "write release notes" | acp-extension-claude-pty print --output-format json
```

Use `--startup-timeout <seconds>` when Claude Code needs longer to paint the
interactive prompt before input can be submitted. The default is 120 seconds:

```sh
acp-extension-claude-pty acp --startup-timeout 180
acp-extension-claude-pty print --startup-timeout 180 "summarize this repo"
```

Doctor and drift checks:

```sh
acp-extension-claude-pty doctor
acp-extension-claude-pty doctor --live-docs
just drift-live
```

`doctor --live-docs` compares local `claude --help`, installed version, npm metadata, and official docs assumptions against the compatibility matrix.

## Verification

Local gate:

```sh
just ci
```

Real e2e is opt-in because it uses the installed Claude CLI and can create transcript records under `~/.claude`:

```sh
just real-e2e
```

See [docs/real-e2e.md](docs/real-e2e.md) for prerequisites and expected coverage.

Developer and package validation docs live in [docs/development.md](docs/development.md). Publishing workflow docs live in [docs/publishing.md](docs/publishing.md).

## Security

Claude transcripts are plaintext under `~/.claude/projects/<project>/<session>.jsonl`. This adapter treats transcript text as sensitive: no body text in logs by default, redaction in diagnostics, and explicit unsafe opt-in required for full transcript debugging.

More detail: [docs/security.md](docs/security.md).

## Compatibility

Claude Code changes quickly. Compatibility rules and drift procedure live in [docs/compatibility.md](docs/compatibility.md).

## Attribution

This project was inspired and guided by Zed `codex-acp`, `agentclientprotocol/claude-agent-acp`, OpenAI Codex, OpenCode, and the ACP/Rust SDK ecosystem. Packaging templates use the public Simple Icons Claude SVG. See [NOTICE.md](NOTICE.md).

## License

Apache-2.0.
