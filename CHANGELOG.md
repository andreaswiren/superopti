# Changelog

Application versions follow Semantic Versioning; preview tags append CI run/attempt. Dates use YYYY-MM-DD.

## [0.2.0] - 2026-09-14

### Added
- Professional native Overview, System checks, Settings and Process details pages, responsive system-DPI layout, labeled fixed-scale graphs and native selectable process table.
- Original app/tray icons, active capture badge, executable resources and manifest.
- Double-click/Enter process drill-down with creation-time identity checks, CPU split, memory, handles, threads and separate read/write I/O totals and rates.
- IPv4/IPv6 TCP endpoints and UDP local sockets; explicit 10-second TCP per-connection incoming/outgoing byte measurement through EStats.
- Fixed pagefile policy: initial and maximum equal max(half installed RAM,16 GiB), rounded up to MiB. Dedicated fix and Fix all, disk-space guard, administrator requirement, backup, Undo and restart notice. Multiple/custom layouts require manual review.
- CRA scope assessment, technical file, requirements mapping, risk register and security policy for individual non-commercial MIT distribution.
- Complete resolved Cargo CycloneDX 1.5 graph, verified archive hashes/license texts, Rust/runtime inventory, schema validation and dependency audit in release packaging.

### Changed
- Installer registration reads the embedded product version and uses the absolute system PowerShell path for uninstall.
- Capture deadline includes helper startup time; Stop terminates all helper threads without blocking on Windows driver I/O cancellation.
- Table selection tracks process instances across refreshes. Detail requests are serialized and identities checked before inspection.
- PowerShell launches from the Windows system directory with concurrently drained, bounded captured output.
- Pinned Rust/release tools; release packages include SBOM and security documentation.

### Known limitations
- TCP polling misses short flows and closing bytes; totals are lower bounds. UDP/QUIC remote peers and per-peer bytes are unavailable.
- Unsigned preview; administrator/reboot and accessibility validation remain incomplete. See VALIDATION.md.

## [0.1.0] - 2026-09-14

### Added
- Initial native Rust/windows-rs tray app, on-demand capture, isolated native/PDH collectors, bounded history and deadline/Stop cleanup.
- CPU, RAM, GPU, disk, commit, pagefile and contributor metrics, thread snapshot and local JSON export.
- Current-user install/uninstall, idle autostart, system checks, animation/Balanced fixes and Undo.
- Main-branch build, tests and automatic preview releases.

[0.2.0]: https://github.com/andreaswiren/superopti/compare/45b4296...main
[0.1.0]: https://github.com/andreaswiren/superopti/releases/tag/v0.1.0-build.1.1
