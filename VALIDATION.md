# Validation of the initial preview

Local Windows 11 x64 validation, 2026-09-14:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: 3 tests passed (GPU engine identity, pressure weighting, missing data).
- Optimized release build and native `--smoke-test`: passed.
- Native snapshot: 504 processes in 12.97 ms in this run. This measures core collection, not all helper overhead.
- Real 10-second capture completed in 10,034 ms. Manual Stop during provider initialization completed in 5 ms. No events arrived after Stop during the idle assertion. All capture helpers exited.
- GPU, pagefile and network data arrived independently while the local disk performance provider was slow. An earlier combined query confirmed disk counters but took too long; provider isolation was introduced in response. Missing/delayed optional readings remain N/A.
- Native thread snapshot returned thread IDs, CPU splits, base priorities and exit state.
- Release executable: 614,400 bytes.
- Tray idle observation: 20.01 seconds, 0.0 additional process CPU seconds at Windows accounting resolution, 13,357,056-byte working set, 152 handles, zero child collectors. A short observation is not a guarantee of zero overhead on all systems.
- Installer and uninstaller PowerShell scripts parsed without errors. Actual installation/uninstallation, autostart persistence across sign-in, fixes and Undo have not been executed on the user's system as part of validation.
- The native window launched. Visual/interactive QA was interrupted when the user stopped computer use; full UI verification remains outstanding.

No process-name capture files, host diagnostics, credentials or personal environment exports are committed or included in release assets. The GitHub workflow independently repeats formatting, lint, unit tests, build and the native lifecycle smoke test before publishing each build.
