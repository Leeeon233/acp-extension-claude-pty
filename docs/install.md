# Install

`acp-extension-claude-pty` requires a working Claude Code CLI. Install and authenticate Claude first:

```sh
npm install -g @anthropic-ai/claude-code
claude
```

Run `acp-extension-claude-pty doctor` after installing this adapter.

## npm

Recommended for most users after publication:

```sh
npm install -g acp-extension-claude-pty
acp-extension-claude-pty doctor
```

The npm package is a Node 18+ wrapper that installs one optional platform binary package:

- `acp-extension-claude-pty-darwin-arm64`
- `acp-extension-claude-pty-darwin-x64`
- `acp-extension-claude-pty-linux-arm64`
- `acp-extension-claude-pty-linux-x64`
- `acp-extension-claude-pty-win32-arm64`
- `acp-extension-claude-pty-win32-x64`

Use `npx acp-extension-claude-pty doctor` only for quick checks. Configure editors with a stable installed binary, not a one-shot `npx` invocation.

## Source Builds

From the public git repository:

```sh
cargo install --git https://github.com/Leeeon233/acp-extension-claude-pty --locked
```

From a local checkout:

```sh
cargo install --path . --locked
```

Cargo installs source-built binaries into Cargo's install root, usually `~/.cargo/bin`. Ensure that directory is on `PATH`.

## GitHub Release Binary

Download the archive matching your platform from the GitHub release:

- `acp-extension-claude-pty-<version>-aarch64-apple-darwin.tar.gz`
- `acp-extension-claude-pty-<version>-x86_64-apple-darwin.tar.gz`
- `acp-extension-claude-pty-<version>-aarch64-unknown-linux-gnu.tar.gz`
- `acp-extension-claude-pty-<version>-x86_64-unknown-linux-gnu.tar.gz`
- `acp-extension-claude-pty-<version>-aarch64-pc-windows-msvc.zip`
- `acp-extension-claude-pty-<version>-x86_64-pc-windows-msvc.zip`

Verify with the release `SHA256SUMS` file before placing the binary on `PATH`.

macOS/Linux:

```sh
tar xzf acp-extension-claude-pty-<version>-<target>.tar.gz
install -m 0755 acp-extension-claude-pty /usr/local/bin/acp-extension-claude-pty
acp-extension-claude-pty doctor
```

Windows PowerShell:

```powershell
Expand-Archive .\acp-extension-claude-pty-<version>-<target>.zip .
.\acp-extension-claude-pty.exe doctor
```

## Homebrew

Homebrew support is intended through a tap after the first public GitHub release exists:

```sh
brew tap Leeeon233/acp-extension-claude-pty
brew install acp-extension-claude-pty
```

Until the tap formula is published, use npm, Cargo, or direct release assets.

## Editor Setup

Configure ACP clients to spawn:

```sh
acp-extension-claude-pty
```

Do not wrap the command in a script that writes banners or status text to stdout. ACP stdout must contain only JSON-RPC frames.

For Zed, use either the ACP Registry entry after publication or a manual custom `agent_servers` setting:

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

If Zed cannot find `claude`, set `CLAUDE_CODE_CLI` in that `env` map to the absolute path of the real Claude Code CLI binary.

See `docs/editor-setup.md` for full editor-specific notes.
