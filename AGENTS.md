# Repository instructions

- Work in this repository on `main`.
- The user explicitly authorizes committing and pushing each completed change to `origin/main`. Do not force-push or publish diagnostic captures.
- Every successful distribution build must have a GitHub release. Pushes to `main` trigger the build/release workflow; wait for it and repair failures. Local compiler and test iterations are validation, not separate distribution releases.
- Keep the application native Rust/windows-rs. Avoid browser runtimes, background polling when idle, telemetry, or continuously active samplers.
- Before pushing: cargo fmt --all -- --check, cargo clippy --locked --all-targets -- -D warnings, cargo test --locked, and the release executable's --smoke-test command.
- Keep performance fixes explicit and reversible. Never quietly turn optional review recommendations into destructive automatic fixes.
