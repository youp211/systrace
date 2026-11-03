#!/usr/bin/env bash
# Builds a release systrace with this Mac's toolchain, in a target directory of
# its own per variant: building one never replaces another's binary, and a
# configured target-dir can't move it.
#
#   scripts/build-macos.sh all   # every module, with the CLI, TUI and GUI
#   scripts/build-macos.sh tui   # every module, with the CLI and TUI
#   scripts/build-macos.sh cli   # every module, with the CLI only
#
# The binary is target/macos-<variant>/release/systrace.
# build-macos-{all,tui,cli}.sh run the same builds by name.
set -euo pipefail

usage() {
  echo "usage: $0 all|tui|cli" >&2
  exit 2
}

[ $# -eq 1 ] || usage
case $1 in
  all) features=default ;;
  tui) features=modules,tui ;;
  cli) features=modules ;;
  *) usage ;;
esac

if [ "$(uname -s)" != Darwin ]; then
  echo "error: this builds for macOS; run it on a Mac" >&2
  exit 1
fi

# Empty CDPATH, or a relative `cd` can land in a directory it names.
CDPATH='' cd "$(dirname "$0")/.."
dir=target/macos-$1
cargo build --release --locked --no-default-features --features "$features" --target-dir "$dir"

bin=$PWD/$dir/release/systrace
archs=$(lipo -archs "$bin")
echo "built $bin ($archs, $(du -h "$bin" | cut -f1))"
