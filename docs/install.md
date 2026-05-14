# Install

`claude-code-cli-acp` requires a working Claude Code CLI. Install and authenticate Claude first:

```sh
npm install -g @anthropic-ai/claude-code
claude
```

Run `claude-code-cli-acp doctor` after installing this adapter.

## npm

Recommended for most users after publication:

```sh
npm install -g claude-code-cli-acp
claude-code-cli-acp doctor
```

The npm package is a Node 18+ wrapper that installs one optional platform binary package:

- `claude-code-cli-acp-darwin-arm64`
- `claude-code-cli-acp-darwin-x64`
- `claude-code-cli-acp-linux-arm64`
- `claude-code-cli-acp-linux-x64`
- `claude-code-cli-acp-win32-arm64`
- `claude-code-cli-acp-win32-x64`

Use `npx claude-code-cli-acp doctor` only for quick checks. Configure editors with a stable installed binary, not a one-shot `npx` invocation.

## Cargo

After crates.io publication:

```sh
cargo install claude-code-cli-acp --locked
```

From the public git repository:

```sh
cargo install --git https://github.com/moabualruz/claude-code-cli-acp --locked
```

From a local checkout:

```sh
cargo install --path . --locked
```

Cargo installs binaries into Cargo's install root, usually `~/.cargo/bin`. Ensure that directory is on `PATH`.

## GitHub Release Binary

Download the archive matching your platform from the GitHub release:

- `claude-code-cli-acp-<version>-aarch64-apple-darwin.tar.gz`
- `claude-code-cli-acp-<version>-x86_64-apple-darwin.tar.gz`
- `claude-code-cli-acp-<version>-aarch64-unknown-linux-gnu.tar.gz`
- `claude-code-cli-acp-<version>-x86_64-unknown-linux-gnu.tar.gz`
- `claude-code-cli-acp-<version>-aarch64-pc-windows-msvc.zip`
- `claude-code-cli-acp-<version>-x86_64-pc-windows-msvc.zip`

Verify with the release `SHA256SUMS` file before placing the binary on `PATH`.

macOS/Linux:

```sh
tar xzf claude-code-cli-acp-<version>-<target>.tar.gz
install -m 0755 claude-code-cli-acp /usr/local/bin/claude-code-cli-acp
claude-code-cli-acp doctor
```

Windows PowerShell:

```powershell
Expand-Archive .\claude-code-cli-acp-<version>-<target>.zip .
.\claude-code-cli-acp.exe doctor
```

## Homebrew

Homebrew support is intended through a tap after the first public GitHub release exists:

```sh
brew tap moabualruz/claude-code-cli-acp
brew install claude-code-cli-acp
```

Until the tap formula is published, use npm, Cargo, or direct release assets.

## Editor Setup

Configure ACP clients to spawn:

```sh
claude-code-cli-acp
```

Do not wrap the command in a script that writes banners or status text to stdout. ACP stdout must contain only JSON-RPC frames.

For Zed, use either the ACP Registry entry after publication or a manual custom `agent_servers` setting:

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

If Zed cannot find `claude`, set `CLAUDE_CODE_CLI` in that `env` map to the absolute path of the real Claude Code CLI binary.

See `docs/editor-setup.md` for full editor-specific notes.
