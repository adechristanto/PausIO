## Summary

<!-- What does this change do? Why? One paragraph. -->

## Type of change

- [ ] Bug fix
- [ ] Feature
- [ ] Refactor (no behavior change)
- [ ] Documentation
- [ ] CI / tooling
- [ ] Chore (deps, config, cleanup)

## Checklist

- [ ] `pnpm check && pnpm test && pnpm build` passes
- [ ] `cargo fmt --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace --all-features` passes
- [ ] Coverage floor maintained: `cargo llvm-cov -p pausio-core --lib --fail-under-lines 90 --summary-only`
- [ ] If touching `crates/pausio-protocol`: `tests/fixtures/watch-settings-v1.json` is updated and all fixture tests pass
- [ ] If touching the break overlay, tray, or command surface: `pnpm test:e2e:desktop` passes
- [ ] No new analytics, telemetry, accounts, network relay, or reading of app names/window titles/screen content/audio/camera/keystrokes

## Notes for reviewer

<!-- Anything a reviewer should know: concurrency considerations, platform-specific behavior, deferred work, etc. -->
