# Release pipeline — signing, updater, and distribution channels

**Status:** Unsigned release-candidate automation exists, but production distribution is not implemented. The workflow creates a private draft release only. Publishing still requires owner-controlled accounts, platform certificates, signing/notarization, physical-device verification, and a decision about update hosting.

`bundle.targets` in `src-tauri/tauri.conf.json` is now `"all"` (was `["app", "dmg"]`), so `pnpm tauri build` locally already produces every installer format Tauri supports for the host OS (`.app`/`.dmg` on macOS, `.msi`/`.nsis` on Windows, `.deb`/`.rpm`/`.appimage` on Linux) without needing this checklist — that part is done. Everything below is what turns those into installers a person can actually trust and auto-update.

## 1. macOS: Developer ID signing + notarization

1. Enroll in the [Apple Developer Program](https://developer.apple.com/programs/) (paid, annual).
2. Create a **Developer ID Application** certificate in Xcode or the Apple Developer portal; export it as a `.p12` with a password.
3. Add to `src-tauri/tauri.conf.json` → `bundle.macOS`:
   ```json
   "signingIdentity": "Developer ID Application: Your Name (TEAMID)",
   "hardenedRuntime": true,
   "entitlements": "entitlements.plist"
   ```
4. Create `src-tauri/entitlements.plist` — PausIO needs no special entitlements beyond the hardened-runtime defaults (no camera/mic/network-server access is requested anywhere in the codebase, consistent with its privacy posture).
5. Notarization: Tauri's bundler calls `notarytool` automatically when these environment variables are set at build time: `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password, not the account password), `APPLE_TEAM_ID`. Store these as GitHub Actions secrets, never in the repo.
6. The `.p12` certificate itself must be imported into the CI runner's keychain before building — see [tauri-action](https://github.com/tauri-apps/tauri-action)'s documented `APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD` secrets, which base64-encode the `.p12` into a repo secret and import it at CI time.

## 2. Windows: code signing

Two viable paths — pick one:

- **Traditional EV/OV code-signing certificate** from a CA (DigiCert, Sectigo, etc.). Add to `bundle.windows`: `"certificateThumbprint"`, `"digestAlgorithm": "sha256"`, `"timestampUrl"`. Requires the cert installed on the signing machine/runner (usually a hardware token for EV certs, which rules out most CI runners without a physical dongle).
- **Azure Trusted Signing** (Microsoft's newer cloud-signing service, no hardware token, works from CI) — needs an Azure account + Trusted Signing resource; Tauri supports it via the same `bundle.windows` config pointing at the Azure endpoint. This is the more CI-friendly option and is what's recommended for a project without a physical signing dongle.

## 3. Updater plugin

1. Generate a signing keypair once, locally, and **never commit the private key**:
   ```sh
   pnpm dlx @tauri-apps/cli signer generate -w ~/.tauri/pausio-updater.key
   ```
2. Add the printed public key to `tauri.conf.json`:
   ```json
   "plugins": {
     "updater": {
       "pubkey": "<public key from step 1>",
       "endpoints": ["https://github.com/<org>/<repo>/releases/latest/download/latest.json"]
     }
   },
   "bundle": { "createUpdaterArtifacts": true }
   ```
   (The GitHub-releases endpoint above is the simplest option: point at a `latest.json` asset attached to each release. Any static host works equally well if self-hosting is preferred instead.)
3. Add `tauri-plugin-updater = "2"` to `src-tauri/Cargo.toml` under the existing desktop-only target block, register `tauri_plugin_updater::Builder::new().build()` in `run()`, and add a `check_for_updates` command plus a Settings toggle (default **off**, matching Stretchly's `disableAppUpdateFeatures` precedent for the same reason: update checks are PausIO's only network egress, and that must stay opt-in given the app's local-first/no-telemetry posture). Surface the toggle in Settings with the exact endpoint documented in-app, so nothing calls home invisibly.
4. Store the private key (`TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` if the key has a password) as GitHub Actions secrets — the release workflow needs them to sign each build's artifacts and generate `latest.json`.

## 4. Release GitHub Actions workflow

`.github/workflows/release.yml` verifies a tagged revision, builds unsigned candidates on each platform, generates SHA-256 checksums, and creates a **draft prerelease**. Its safeguards are intentional:

- build jobs receive read-only repository permissions;
- the release-writing permission exists only on the final draft job;
- actions are pinned to full commit SHAs and remain updateable through Dependabot;
- Wear OS never falls back to a debug artifact;
- the workflow cannot make an unsigned release public automatically.

Treat the resulting files as engineering artifacts, not public installers. Before publishing a draft:

1. Replace unsigned desktop/mobile artifacts with the signed outputs described in sections 1 and 2.
2. Notarize macOS artifacts and verify the notarization ticket.
3. Verify Authenticode signatures and timestamps on Windows installers.
4. Install each artifact on a clean supported operating-system version.
5. Run physical-device checks for iOS, Android, Apple Watch, and Wear OS.
6. Confirm the tag version matches every package and application manifest.
7. Regenerate checksums after replacing any artifact.
8. Review the generated notes, mark the release non-prerelease only when appropriate, and publish manually.

When signing secrets exist, prefer the official Tauri release tooling or an equivalently reviewed, commit-pinned workflow. Never place signing material or updater private keys in the repository.

## 5. Distribution channels checklist

Each of these is a separate submission/approval process, not a code change:

- **Homebrew tap**: create a new repo `homebrew-pausio`, add a cask formula pointing at the signed/notarized `.dmg` release asset (mirrors [hovancik/homebrew-stretchly](https://github.com/hovancik/homebrew-stretchly)). Fully self-service, no approval needed.
- **winget**: submit a manifest PR to [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) pointing at the signed `.msi`/`.exe`. Requires the Windows binary to be code-signed first (§2) — winget review rejects unsigned installers.
- **Chocolatey**: package and push to the Chocolatey community repository; requires a Chocolatey account and moderation review.
- **Flathub**: write a Flatpak manifest (`com.pausio.app.yml`), submit to [flathub/flathub](https://github.com/flathub/flathub) for review. Flatpak sandboxing will need explicit portal permissions declared for tray icon (`org.freedesktop.StatusNotifierItem` via `xdg-desktop-portal`) and autostart (`xdg-desktop-portal`'s Background portal) — both are sandboxed differently than the current native GTK path, so this needs its own testing pass once Linux CI exists (see `docs/LINUX_WAYLAND_PLAN.md`).
- **Snapcraft**: write a `snapcraft.yaml`, register the `pausio` name on the Snap Store, push via `snapcraft upload`.
- **AUR**: write a `PKGBUILD`, submit as a new AUR package (self-service, no review, but community trust accrues over time).

Recommended order: Homebrew tap first (fastest, no review gate, matches Stretchly's own primary distribution channel), then winget once Windows signing is in place, then Flathub/Snap/AUR as Linux packaging matures alongside the work in `docs/LINUX_WAYLAND_PLAN.md`.
