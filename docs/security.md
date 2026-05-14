# Security

Claude Code transcripts are sensitive plaintext.

## Transcript Storage

Claude stores session JSONL under:

```text
~/.claude/projects/<project>/<session>.jsonl
```

Subagent transcripts and large tool results may also be written below session directories. Treat all transcript, tool-result, and prompt-history files as private.

## Adapter Rules

- Do not log prompt, assistant, tool-call, or tool-result bodies by default.
- Redact or hash transcript fields in diagnostics.
- Require explicit unsafe debug opt-in before writing raw transcript text to logs.
- Do not commit real transcript fixtures.
- Sanitize fixture JSONL before adding tests.

## Incompatible Claude Settings

Transcript extraction needs session persistence. The adapter should report an incompatibility when it detects:

- `CLAUDE_CODE_SKIP_PROMPT_HISTORY`
- `--no-session-persistence` for print flows

## Release Hygiene

Before publishing, run the full verification gate and scan staged release changes for secrets:

```sh
just ci
git diff --cached | gitleaks detect --pipe --redact --no-banner
```
