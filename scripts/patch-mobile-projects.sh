#!/usr/bin/env sh
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
android_settings="$repo_dir/src-tauri/gen/android/settings.gradle"
if [ -f "$android_settings" ] && ! grep -q "PausIO Wear module" "$android_settings"; then
  printf '\n// PausIO Wear module\ninclude(\":wear\")\nproject(\":wear\").projectDir = file(\"%s/watch/wear-os/app\")\n' "$repo_dir" >> "$android_settings"
fi
android_root_build="$repo_dir/src-tauri/gen/android/build.gradle.kts"
if [ -f "$android_root_build" ] && grep -q 'kotlin-gradle-plugin:1.9.25' "$android_root_build"; then
  # Wear Compose 1.6 is built with Kotlin 2.1. Keep the generated Android host
  # and its embedded wearable module on the same compiler generation.
  perl -0pi -e 's/kotlin-gradle-plugin:1\.9\.25/kotlin-gradle-plugin:2.1.21/g' "$android_root_build"
fi
android_build_task="$repo_dir/src-tauri/gen/android/buildSrc/src/main/java/com/pausio/app/kotlin/BuildTask.kt"
if [ -f "$android_build_task" ] && ! grep -q 'scripts/tauri.sh' "$android_build_task"; then
  # Tauri CLI versions generate either `node tauri android …` or `pnpm android …`.
  # The repository's `tauri` name is a package script, so invoking `pnpm android`
  # from Gradle fails. Use the stable wrapper in both generated variants.
  perl -0pi -e 's!val executable = """(?:node|pnpm)""";!val executable = File(project.rootDir, "../../../scripts/tauri.sh").canonicalPath;!; s!val args = listOf\("tauri", "android", "android-studio-script"\);!val args = listOf("android", "android-studio-script");!' "$android_build_task"
fi
apple_project="$repo_dir/src-tauri/gen/apple/project.yml"
apple_version=$(sh "$repo_dir/scripts/release-version.sh")
apple_build_number=${PAUSIO_BUILD_NUMBER:-1}
if [ -f "$apple_project" ] && ! grep -q "PausIO Watch target" "$apple_project"; then
  printf '\n  # PausIO Watch target\n  PausIOWatch:\n    type: application\n    platform: watchOS\n    sources:\n      - path: ../../../watch/apple-watch/Sources/PausIOWatch\n    info:\n      path: PausIOWatch-Info.plist\n      properties:\n        CFBundleDisplayName: PausIO Watch\n        CFBundleShortVersionString: "0.1.0"\n        CFBundleVersion: "1"\n        WKApplication: true\n        WKRunsIndependentlyOfCompanionApp: true\n        WKExtendedRuntimeSessionType: Self Care\n        WKCompanionAppBundleIdentifier: com.pausio.app\n    settings:\n      base:\n        PRODUCT_NAME: PausIO Watch\n        PRODUCT_BUNDLE_IDENTIFIER: com.pausio.app.watchapp\n        WATCHOS_DEPLOYMENT_TARGET: 9.0\n' >> "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ] && grep -q 'CFBundleShortVersionString: 0.1.0' "$apple_project"; then
  perl -0pi -e 's/CFBundleShortVersionString: 0\.1\.0/CFBundleShortVersionString: "0.1.0"/g; s/CFBundleVersion: 1$/CFBundleVersion: "1"/mg' "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ]; then
  # Keep phone, watch, and complication marketing/build versions aligned from
  # the single checked-in semantic version source. CI supplies a monotonic
  # PAUSIO_BUILD_NUMBER for distribution archives.
  perl -0pi -e "s/(CFBundleShortVersionString: )\"[^\"]*\"/\${1}\"$apple_version\"/g; s/(CFBundleVersion: )\"[^\"]*\"/\${1}\"$apple_build_number\"/g" "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ] && grep -q 'PausIO Watch target' "$apple_project" && ! grep -q 'WKRunsIndependentlyOfCompanionApp' "$apple_project"; then
  perl -0pi -e 's|(        WKApplication: true\n)|${1}        WKRunsIndependentlyOfCompanionApp: true\n        WKExtendedRuntimeSessionType: Self Care\n|' "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ] && grep -q 'PausIO Watch target' "$apple_project" && ! grep -q 'PausIOWatchComplication:' "$apple_project"; then
  printf '\n  PausIOWatchComplication:\n    type: app-extension\n    platform: watchOS\n    sources:\n      - path: ../../../watch/apple-watch/Sources/PausIOWatchComplication\n    info:\n      path: PausIOWatchComplication-Info.plist\n      properties:\n        CFBundleDisplayName: PausIO\n        CFBundleShortVersionString: "0.1.0"\n        CFBundleVersion: "1"\n        NSExtension:\n          NSExtensionPointIdentifier: com.apple.widgetkit-extension\n    entitlements:\n      path: ../../../watch/apple-watch/Sources/PausIOWatch/PausIOWatch.entitlements\n    settings:\n      base:\n        PRODUCT_NAME: PausIOWatchComplication\n        PRODUCT_BUNDLE_IDENTIFIER: com.pausio.app.watchapp.complication\n        WATCHOS_DEPLOYMENT_TARGET: 9.0\n' >> "$apple_project"
  perl -0pi -e 's|(        WATCHOS_DEPLOYMENT_TARGET: 9\.0\n)(?=\n  PausIOWatchComplication:)|${1}    entitlements:\n      path: ../../../watch/apple-watch/Sources/PausIOWatch/PausIOWatch.entitlements\n    dependencies:\n      - target: PausIOWatchComplication\n        embed: true\n|' "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ] && grep -q "PausIO Watch target" "$apple_project" && ! grep -q "target: PausIOWatch" "$apple_project"; then
  # A WatchConnectivity app must be embedded by its iOS companion; a standalone
  # watchOS target has the right sources but cannot form a paired WCSession.
  perl -0pi -e 's|(    dependencies:\n)(      - framework: libapp\.a)|${1}      - target: PausIOWatch\n        embed: true\n${2}|' "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ] && grep -q "PausIO Watch target" "$apple_project" && ! grep -q "CFBundleDisplayName: PausIO Watch" "$apple_project"; then
  perl -0pi -e 's|(      - path: ../../../watch/apple-watch/Sources/PausIOWatch\n)(    settings:)|$1    info:\n      properties:\n        CFBundleDisplayName: PausIO Watch\n        CFBundleShortVersionString: 0.1.0\n        CFBundleVersion: 1\n$2|' "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ] && grep -q "PausIO Watch target" "$apple_project" && grep -q "WKWatchKitApp: true" "$apple_project"; then
  perl -0pi -e 's|(        CFBundleVersion: 1\n)|$1        WKWatchKitApp: true\n|' "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ] && grep -q "WKWatchKitApp: true" "$apple_project"; then
  perl -0pi -e 's/WKWatchKitApp: true/WKApplication: true/g' "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ] && grep -q "PausIO Watch target" "$apple_project" && ! grep -q "WKCompanionAppBundleIdentifier: com.pausio.app" "$apple_project"; then
  perl -0pi -e 's|(        WKWatchKitApp: true\n)|$1        WKCompanionAppBundleIdentifier: com.pausio.app\n|' "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ] && grep -qE '(node|pnpm) tauri ios xcode-script' "$apple_project"; then
  # Tauri CLI versions generate either `node tauri ios xcode-script` or
  # `pnpm tauri ios xcode-script`. The repository's `tauri` name is a
  # package script, so the generated `pnpm tauri` form only works from the
  # repo root; use the stable POSIX wrapper in both variants.
  perl -0pi -e 's!(node|pnpm) tauri ios xcode-script!\$PROJECT_DIR/../../../scripts/tauri.sh ios xcode-script!g' "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
if [ -f "$apple_project" ] && grep -q 'scripts/tauri.sh ios xcode-script' "$apple_project" && ! grep -q 'PAUSIO_SKIP_RUST_BUILD' "$apple_project"; then
  # Tauri prepares the Rust static library. This opt-in guard lets the mixed
  # iPhone/watch scheme be built directly by Xcode without starting a second
  # Tauri CLI bridge (which only understands one platform destination).
  perl -0pi -e 's|(      - script: )(\$PROJECT_DIR/\.\./\.\./\.\./scripts/tauri\.sh ios xcode-script)|${1}if [ "\$PAUSIO_SKIP_RUST_BUILD" = "1" ]; then exit 0; fi; ${2}|' "$apple_project"
  (
    cd "$repo_dir/src-tauri/gen/apple"
    xcodegen generate
  )
fi
printf '%s\n' "PausIO native sources are canonical under watch/ and plugins/; generated hosts were patched idempotently."
