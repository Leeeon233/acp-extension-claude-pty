# acp-extension-claude-pty agent instructions

This's a fork repo. use Leeeon233/acp-extension-claude-pty as base if you open pull request.

## Required workflow

- Read `AGENTS.md`, `justfile`, `.mise.toml`, `.envrc`, `pyproject.toml`, and `package.json` before suggesting commands. Project config wins.
- Run current research before writing implementation decisions for a feature.
- Persist durable research in public docs when it is user- or maintainer-relevant.
- Every research pass must include competitor/prior-art research, dependency research, and repository-specific integration mapping.
- Do not push a remote branch, publish a package, create a release, or open registry PRs without explicit maintainer approval.

## Product boundaries

- Build a Rust ACP adapter that drives the real interactive `claude` CLI through a PTY.
- Do not implement the ACP path through Claude Agent SDK.
- Do not depend on `claude -p` for core behavior. `print` mode must be this project's replacement for `claude -p`, implemented through the interactive CLI plus transcript extraction.
- Keep `interactive` as a direct pass-through mode, but do not suggest aliasing over the real `claude` binary.
- Track live Claude Code CLI behavior: version, `claude --help` flags, and official docs. Compatibility checks must report when repo assumptions need updating.

## Architecture rules

- Runtime code names must describe responsibility, value, or behavior. Provenance and competitor names belong in docs unless the file is a real third-party integration boundary.
- Keep ACP, PTY, transcript, compatibility, and CLI surfaces separate. Do not put business logic in command handlers.
- Use existing libraries for protocol, PTY, terminal parsing, CLI parsing, tracing, and test harnesses instead of custom implementations.
- Keep Claude-specific behavior in responsibility-named Rust modules and tests.
- Keep attribution in `README.md` and `NOTICE.md`; do not name runtime code by project provenance.
- Treat Claude transcripts as sensitive plaintext. Never log transcript text by default; redact or hash where possible.

## Quality gates

- No fake e2e claim. Unit tests and integration tests can use fixtures, but release acceptance needs a real e2e gate against installed `claude`.
- Default verification once code exists:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - real e2e command documented in the approved plan
- Before commit, run `git status`, `git diff --stat`, and the relevant verification commands.
