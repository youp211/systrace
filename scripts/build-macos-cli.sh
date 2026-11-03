#!/usr/bin/env bash
# Every module, with the CLI only (no TUI or GUI). See build-macos.sh.
exec "$(dirname "$0")/build-macos.sh" cli "$@"
