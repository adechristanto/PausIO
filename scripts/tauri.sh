#!/usr/bin/env sh
# POSIX entry point for Tauri CLI operations.
#
# This script is NOT a duplicate of scripts/tauri.mjs. It serves two callers
# that require a POSIX-executable path rather than a Node script:
#   - Xcode build phases (scripts/patch-mobile-projects.sh:14)
#   - The Gradle BuildTask.kt patch (scripts/patch-mobile-projects.sh:62)
#
# Both of those callers are outside the pnpm workspace and cannot invoke
# `node scripts/tauri.mjs` directly. Removing this file silently breaks
# iOS and Android builds.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
if [ "${1:-}" = "ios" ] && [ "${2:-}" = "build" ]; then
  generated_build="$repo_dir/src-tauri/gen/apple/build"
  if [ -d "$generated_build" ]; then
    recovery_dir=$(mktemp -d /tmp/pausio-ios-build.XXXXXX)
    mv "$generated_build" "$recovery_dir/build"
  fi
fi
cd "$repo_dir/src-tauri"
exec node "$repo_dir/frontend/node_modules/@tauri-apps/cli/tauri.js" "$@"
