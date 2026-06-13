#!/usr/bin/env bash
set -euo pipefail

# This adapter drives the installed Claude Code CLI and does not bundle a Linux
# sandbox helper.
echo "No bundled bwrap install required for acp-extension-claude-pty."
