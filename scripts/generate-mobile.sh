#!/usr/bin/env sh
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
cd "$repo_dir/src-tauri"
node "$repo_dir/frontend/node_modules/@tauri-apps/cli/tauri.js" ios init --ci
node "$repo_dir/frontend/node_modules/@tauri-apps/cli/tauri.js" android init --ci
"$script_dir/patch-mobile-projects.sh"
