# Validation evidence

SuperOpti 0.2.0 preview, Windows 11 x64, 2026-09-14. These observations apply to the local validation build; release CI independently repeats automated gates against the published commit.

## Passed

- Pinned Rust 1.97.1: formatting, clippy all-targets with warnings denied, seven Rust tests (ranking/missing metrics, GPU identity, endpoint byte order/live loopback listener, process identity mismatch rejection, readable focused process counters).
- Pagefile PowerShell parser and seven pure sizing/unknown-memory cases. Read-only live preview detected 64 GiB installed RAM and proposed 32 GiB initial=max. Disk-space guard rejected the proposed change on this machine. No pagefile settings were changed.
- Optimized build with embedded icons, asInvoker/system-DPI manifest and 0.2.0 version resources: 767,488-byte EXE.
- Native snapshot: 471 processes in 11.4122 ms. This measures core collection, not all helper overhead.
- Real capture stopped automatically in 10,002 ms. Immediate manual Stop measured 0 ms at integer-millisecond resolution. No subsequent sample events during the idle assertion. Native thread report returned successfully.
- A shutdown regression initially failed at about 13 seconds: Windows disk-provider I/O cancellation delayed process retirement. The coordinator now terminates all helper threads without synchronously waiting for pending driver I/O; startup time counts toward the deadline. Windows may briefly retain terminating process objects. The test passed after this correction.
- Idle tray observation after startup: 20.23 seconds, 0.0 additional CPU seconds at Windows accounting resolution, 15,032,320-byte working set, 159 handles, zero child processes. This short observation does not guarantee zero overhead on all systems.
- CycloneDX 1.5 schema/reference validation: all 23 Cargo dependencies matched Cargo.lock and cached archive SHA-256 hashes; 20 direct PE imports inventoried. Upstream licenses, Rust library copyright/license texts and sysroot input hashes included.
- cargo-audit 0.22.2: zero reported vulnerabilities, empty warnings, 1,246-advisory RustSec database at revision e2e640471715167f73e22eaf761f2e547adafeec. This is time-bounded known-advisory evidence, not proof of no vulnerabilities.

## Not yet validated

- The native window launched. Desktop automation failed with `window id 136196 was not found`; refreshing target selection returned the same stale binding. Full visual/interactive QA, double-click/Enter behavior, compact layout, 150/200% scaling, Narrator/high contrast and light/dark tray visibility remain manual checklist items in docs/UX_DESIGN.md.
- Administrator pagefile Apply/rollback/reboot/Undo, install/update/uninstall and autostart across sign-in were not exercised on the user's workstation. Validate in disposable Windows VMs before relying on these paths in managed deployments.
- Successful privileged TCP byte measurement against a controlled known-byte transfer, abrupt-connection cleanup and third-party EStats coexistence remain untested. Native endpoint enumeration/decoding is tested. UDP/QUIC remote peer and byte accounting is not implemented.
- No independent penetration test, signed-binary verification, high-core-count hardware matrix or formal performance/accessibility certification.

No captures, host process listings, private endpoints, traces or credentials are committed or packaged. Release CI gates publication on format, lint, tests, build, SBOM validation, advisory audit, pagefile sizing tests and native capture smoke test. Private GitHub vulnerability reporting is enabled.
