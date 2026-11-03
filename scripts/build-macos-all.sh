#!/usr/bin/env bash
# Every module, with the CLI, TUI and GUI. See build-macos.sh.
exec "$(dirname "$0")/build-macos.sh" all "$@"
