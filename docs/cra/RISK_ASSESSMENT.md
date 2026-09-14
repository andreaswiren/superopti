# Risk assessment

SuperOpti 0.2.0, 2026-09-14. Qualitative engineering estimates, not certification. Assets: Windows availability/configuration, process privacy, captures and release integrity. Boundaries: OS APIs, child collectors, user-writable install/data, administrator actions, dependencies and GitHub distribution. Windows and the signed-in account are assumed trusted; an already-compromised administrator is outside app protection.

| Risk | Initial severity / likelihood | Controls | Residual / follow-up |
|---|---|---|---|
| Collectors worsen slowness or stall | High / possible | On-demand duration, bounded history, isolated providers, Jobs, Stop/deadline | Medium: benchmark varied hardware/process counts |
| Reused PID misattributes activity | Medium / possible | Creation time, held identity handle, typed reports/table identity | Low/medium: endpoint lifecycle races remain observational limitations |
| Native FFI/buffer defect | High / possible | Typed bindings, bounded aligned tables, count checks, owned handles | Medium: unsafe boundary fuzz/stress testing outstanding |
| TCP counters remain enabled or conflict | Low / possible | Restore only counters enabled by this operation | Low: crash/kill/concurrent third-party toggles can prevent restoration until connection closes |
| Inaccurate network diagnosis | Medium / likely without labels | N/A disabled values, IN/OUT labels, lower-bound and UDP caveats | Medium: complete flow accounting remains unimplemented |
| Fixed pagefile limits commit/dumps | High / possible | Explicit policy, disk reserve, admin check, backup/verify/Undo, restart notice | Medium: disposable-VM mutation/reboot QA needed; fixed sizing is not universally optimal |
| Fix interruption/rollback failure | High / possible | Save before mutation, per-fix results, pagefile restore verification | Medium: timeout/power loss can leave pending changes; backup retained and manual recovery may be needed |
| User-writable files executed elevated | High / possible | asInvoker default, absolute system PowerShell, embedded pagefile script, HKLM backup | Medium/high: installer/executable remain user-writable; installation is not a hardened elevation boundary |
| Diagnostic disclosure | Medium / possible | No upload/payloads/DNS, explicit local exports, captures excluded from releases | Medium: unencrypted local files and accidental sharing; sanitize/delete |
| Compromised dependency/build/release | High / possible | Lock/checksums, pinned tools/actions, audit gate, binary/source evidence | Medium/high: unsigned builds, no independent attestation, runner/toolchain trust |
| Missed vulnerability/maintainer unavailable | High / possible | Private reporting and triage/advisory policy | Medium/high: best-effort individual maintenance without SLA |

Admin pagefile mutation/reboot/Undo and successful privileged TCP accounting require disposable-environment validation; do not represent unperformed tests as passed. Reassess on changes, reports and before any commercial conformity claim. Record incidents privately and publish coordinated sanitized advisories. This document does not approve residual risk for commercial deployment.
