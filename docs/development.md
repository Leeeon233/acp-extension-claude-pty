# Development

## Prerequisites

- Rust 1.95+.
- Node 18+ and npm for wrapper/package checks.
- `claude` installed and authenticated for live doctor and real e2e.
- `just` for local recipes.

## Local Build

```sh
cargo build
cargo run -- --help
cargo run -- doctor
```

## Verification

Fast local gates:

```sh
just fmt
just lint
just test
```

Full local gate:

```sh
just ci
```

`just ci` includes Rust fmt/lint/tests, npm package dry-runs, npm wrapper validation, and platform detection tests.

Live checks:

```sh
just doctor-live
just drift-live
just real-e2e
```

`just real-e2e` uses installed real Claude and can write transcript records under `~/.claude/projects`.

## Package Checks

Run package checks without the full test suite:

```sh
just package-dry-run
```

This verifies:

- Generated platform npm package layout
- `npm pack --dry-run --json` for base and platform packages

## Source Boundaries

- Runtime files, tests, and package metadata should be named by responsibility and behavior, not by implementation provenance.
- Keep public attribution in `README.md` and `NOTICE.md`.
- Keep Claude chat/session behavior on the real interactive Claude Code CLI path, not on Claude Agent SDK, Anthropic API calls, or `claude -p`.

## Release Development

Release workflow changes must be validated locally with:

```sh
just ci
git diff --check
```

Before committing release-related changes, run staged secret scanning:

```sh
git diff --cached | gitleaks detect --pipe --redact --no-banner
```
