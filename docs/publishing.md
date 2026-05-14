# Publishing

Before any remote push or publication, stop and request explicit maintainer approval.

## Release Channels

Supported channels:

- GitHub Releases with signed or unsigned platform archives and `SHA256SUMS`.
- npm registry base package plus platform optional-dependency packages.
- crates.io binary crate for `cargo install`.
- Homebrew tap after the first GitHub release exists.
- ACP Registry for Zed and other ACP clients after npm or GitHub release assets are public.
- Zed Agent Server extension only if Zed-specific extension packaging is needed.

Not default channels:

- GitHub Packages for npm. Use only for an internal mirror with scoped package naming and separate docs.
- Linux musl npm packages. Use Cargo install on musl until musl CI and package detection are explicitly added.

## Release Branch Hygiene

Release from a branch that contains only release-ready source, tests, package metadata, CI, README, NOTICE, and user/developer docs. Keep attribution in `README.md` and `NOTICE.md`.

## GitHub Release

All GitHub Actions workflows are manual-only. They run from `workflow_dispatch`, not push, merge, or pull request events.

The manual release workflow builds:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `aarch64-unknown-linux-gnu`
- `x86_64-unknown-linux-gnu`
- `aarch64-pc-windows-msvc`
- `x86_64-pc-windows-msvc`

By default the workflow creates a draft release and does not publish npm/crates packages.

Inputs:

- `tag_name`: optional tag override; defaults to `v<version>` from `Cargo.toml`.
- `draft_release`: default `true`.
- `prerelease`: default `false`.
- `sign_artifacts`: default `false`; requires signing secrets/vars.
- `publish_npm`: default `false`; requires `NPM_TOKEN`.
- `publish_crates`: default `false`; requires `CARGO_REGISTRY_TOKEN`.

Set `draft_release=false` only when the public branch and release notes are ready.

## GitHub npm and crates.io Publishing

The release workflow can publish npm and crates.io from GitHub after the release assets are created. Publication jobs are skipped unless `draft_release=false`.

One-time setup:

```sh
gh secret set NPM_TOKEN --repo moabualruz/claude-code-cli-acp
gh secret set CARGO_REGISTRY_TOKEN --repo moabualruz/claude-code-cli-acp
```

Token requirements:

- `NPM_TOKEN`: npm automation token with publish rights for `claude-code-cli-acp` and all platform packages.
- `CARGO_REGISTRY_TOKEN`: crates.io API token with publish rights for `claude-code-cli-acp`.

The npm job already has `id-token: write` and runs `npm publish --provenance --access public`, so npm provenance is attached when npm accepts the token and GitHub OIDC context.

To publish a future release from GitHub:

```sh
git tag -a v0.1.2 -m "Release v0.1.2"
git push origin main v0.1.2
gh workflow run Release \
  --repo moabualruz/claude-code-cli-acp \
  --ref main \
  -f tag_name=v0.1.2 \
  -f draft_release=false \
  -f prerelease=false \
  -f sign_artifacts=false \
  -f publish_npm=true \
  -f publish_crates=true
```

Use `publish_npm=false` or `publish_crates=false` when only one registry should publish. Keep `draft_release=true` for artifact rehearsal runs; registry publication will remain skipped.

## Signing

macOS signing requires:

- `MACOS_CERTIFICATE`
- `MACOS_CERTIFICATE_PASSWORD`
- `APPLE_NOTARIZATION_KEY`
- `APPLE_NOTARIZATION_KEY_ID`
- `APPLE_NOTARIZATION_ISSUER_ID`
- `MACOS_CODESIGN_IDENTITY`

Windows signing requires:

- `AZURE_SIGNING_TENANT_ID`
- `AZURE_SIGNING_CLIENT_ID`
- `AZURE_SIGNING_CLIENT_SECRET`
- `AZURE_SIGNING_ACCOUNT_NAME`
- `AZURE_SIGNING_CERT_PROFILE_NAME`
- `AZURE_SIGNING_ENDPOINT`

If `sign_artifacts=false`, the release can still produce unsigned archives. Document unsigned status in release notes.

## npm

npm publication uses:

```sh
npm publish --provenance --access public
```

The workflow publishes platform packages first, then the base wrapper package. This order prevents the base package from pointing users at optional dependencies that are not yet available.

Required secret:

- `NPM_TOKEN`

Local dry run:

```sh
bash npm/testing/test-publish-packages.sh
npm pack --dry-run --json ./npm
```

## crates.io

crates.io publication uses:

```sh
cargo publish --locked
```

Required secret:

- `CARGO_REGISTRY_TOKEN`

Local dry run:

```sh
cargo package --list
cargo publish --dry-run --locked
```

`Cargo.toml` uses a strict `include` list so only the runtime crate files needed by Cargo are packaged.

## Homebrew Tap

After the GitHub release exists:

1. Download or read release `SHA256SUMS`.
2. Copy `packaging/homebrew/claude-code-cli-acp.rb.template` into the tap as `Formula/claude-code-cli-acp.rb`.
3. Point formula URLs at the GitHub release assets.
4. Add SHA256 values from `SHA256SUMS`.
5. Test with:

```sh
brew install --build-from-source ./Formula/claude-code-cli-acp.rb
brew test claude-code-cli-acp
brew audit --strict --online claude-code-cli-acp
```

The formula test should run:

```sh
claude-code-cli-acp --version
```

Do not make Homebrew the first publication gate; it depends on public GitHub release URLs.

## ACP Registry

ACP Registry is the preferred Zed install path for external agents and also reaches other ACP clients. Registry agents update automatically after merge.

Prerequisites:

- Public GitHub repository URL.
- Public npm package or GitHub release assets with versioned URLs.
- Adapter `initialize` returns at least one ACP `authMethods` entry of type `agent` or `terminal`.
- 16x16 monochrome `icon.svg` using `currentColor`; the template uses the public Simple Icons Claude SVG.
- `agent.json` passes the registry schema.

Recommended first registry distribution:

```json
{
  "distribution": {
    "npx": {
      "package": "claude-code-cli-acp@0.1.1",
      "args": []
    }
  }
}
```

Use the template at `packaging/acp-registry/agent.json.template`, then in a fork of `agentclientprotocol/registry`:

```sh
mkdir claude-code-cli-acp
cp packaging/acp-registry/agent.json.template claude-code-cli-acp/agent.json
cp packaging/acp-registry/icon.svg claude-code-cli-acp/icon.svg
```

Before opening the registry PR:

```sh
uv run --with jsonschema .github/workflows/build_registry.py
python3 .github/workflows/verify_agents.py --auth-check --agent claude-code-cli-acp
```

Registry validation checks schema, id uniqueness, icon requirements, distribution availability, version matching, and ACP authentication support.

## Zed Agent Server Extension

Use a Zed Agent Server extension only when Zed-specific install packaging is needed. Zed marks ACP Registry as the preferred external-agent path starting with `v0.221.x`.

Extension requirements:

- Public extension repository with `extension.toml`.
- Accepted extension license.
- `[agent_servers.claude-code-cli-acp]` entry with display name and icon.
- Platform targets with release archive URL, `cmd`, optional `args`, and recommended `sha256`.
- Local dev-extension test in Zed before submitting.

Use `packaging/zed-extension/extension.toml.template` as the starting manifest. Publish by opening a PR to `zed-industries/extensions`, adding the extension as a submodule under `extensions/claude-code-cli-acp`, adding its entry to `extensions.toml`, and running the repository sort command required by Zed docs.

## Final Pre-Publish Checks

Run on the clean public branch:

```sh
just ci
just doctor-live
just real-e2e
git diff --cached | gitleaks detect --pipe --redact --no-banner
```

Then stop and request explicit approval before pushing or publishing.
