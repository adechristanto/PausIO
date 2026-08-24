#!/usr/bin/env sh
# Builds the mixed iPhone/watchOS simulator scheme. Tauri's `ios build` archive
# helper currently chooses one SDK for every target, while this project has a
# watchOS target embedded in its iOS companion.
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
simulator_id=${PAUSIO_IOS_SIMULATOR_ID:?Set PAUSIO_IOS_SIMULATOR_ID to an iOS Simulator UDID}

cd "$repo_dir"
pnpm --dir frontend build

# Keep the generated static library current, including the mobile plugin and
# the freshly built frontend assets. The Xcode phase below then links it once.
PAUSIO_RUSTFLAGS="${RUSTFLAGS:-} --cfg mobile"
RUSTFLAGS="$PAUSIO_RUSTFLAGS" cargo build -p pausio --lib --features custom-protocol --target aarch64-apple-ios-sim
mkdir -p src-tauri/gen/apple/Externals/arm64/debug
cp target/aarch64-apple-ios-sim/debug/libpausio_lib.a src-tauri/gen/apple/Externals/arm64/debug/libapp.a

# Other Tauri CLI invocations can regenerate the Xcode project. Reapply the
# no-second-Rust-build guard immediately before Xcode so CI cannot start the
# development bridge after the static library has already been staged.
./scripts/patch-mobile-projects.sh
# Generated project files from different Tauri CLI releases use either the
# wrapper script or `pnpm tauri` directly. Prefix the concrete build phase as a
# final safeguard; Xcode does not consistently inherit arbitrary shell env vars
# on hosted runners, but explicit build settings are expanded in this script.
# The inserted guard must escape its quotes (\") or the pbxproj plist breaks.
perl -0pi -e 's|(shellScript = ")(?=[^;]*xcode-script)|$1if [ \\\"\$PAUSIO_SKIP_RUST_BUILD\\\" = \\\"1\\\" ]; then exit 0; fi; |g' src-tauri/gen/apple/pausio.xcodeproj/project.pbxproj
PAUSIO_SKIP_RUST_BUILD=1 xcodebuild \
  -project src-tauri/gen/apple/pausio.xcodeproj \
  -scheme pausio_iOS \
  -destination "platform=iOS Simulator,id=$simulator_id" \
  build CODE_SIGNING_ALLOWED=NO PAUSIO_SKIP_RUST_BUILD=1
