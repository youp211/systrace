#!/usr/bin/env bash
# Every module, with the CLI and TUI (no GUI). See build-macos.sh.
exec "$(dirname "$0")/build-macos.sh" tui "$@"
